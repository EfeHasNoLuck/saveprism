//! `format.reg`: Windows Registry export files (`.reg`).
//!
//! The main use is a Unity `PlayerPrefs` export: Windows games keep their prefs under
//! `HKEY_CURRENT_USER\Software\<Company>\<Product>`, players export that key with `regedit` and edit
//! the `.reg` file. Any other registry export reads the same way.
//!
//! The file is parsed as text only; nothing is imported into the registry and nothing runs.
//! `Windows Registry Editor Version 5.00` files are UTF-16LE with a BOM and CRLF line ends;
//! `REGEDIT4` files are ANSI (Windows-1252). The encoding, the BOM, every line ending and the
//! original wrap points of a long value are kept, so an unchanged file writes back byte for byte.
//! An edit rewrites only that value's line (long hex values are wrapped the `regedit` way again).
//!
//! # Tree
//!
//! The root is an `Object` of keys (the key path as the name), each an `Object` of values.
//! Value names keep the full registry name in the hint. A Unity value name ends with `_h<decimal
//! hash>`; the displayed key drops that suffix and the hint keeps it:
//!
//! | Registry | Node | Hint suffix |
//! |---|---|---|
//! | `REG_DWORD` (`dword:`) | `Int` (signed 32-bit) | `REG:dword:<name>` |
//! | `REG_DWORD` (`hex(4):`, 4 bytes) | `Int` (signed 32-bit) | `REG:hex4-int:<name>` |
//! | `REG_QWORD` (`hex(b):`) | `Float` (64-bit) | `REG:qword:<name>` |
//! | `hex(4):` with 8 bytes (Unity float quirk) | `Float` (64-bit) | `REG:hex4:<name>` |
//! | `REG_BINARY` UTF-8 + trailing NUL (Unity string) | `Str` | `REG:unity-string:<name>` |
//! | `REG_SZ` (`"text"`) | `Str` | `REG:sz:<name>` |
//! | `REG_BINARY` (`hex:`) other | `Bytes` (read-only) | `REG:binary:<name>` |
//! | `REG_EXPAND_SZ` (`hex(2):`) | `Bytes` (read-only) | `REG:expand:<name>` |
//! | `REG_MULTI_SZ` (`hex(7):`) | `Bytes` (read-only) | `REG:multi:<name>` |
//! | other `hex(N):` | `Bytes` (read-only) | `REG:other:<name>` |
//! | deletion (`-`) | `Null` (read-only) | `REG:delete:<name>` |
//!
//! Editable: dword (signed 32-bit range), qword double (float), Unity string binaries (text; the
//! trailing NUL stays) and `REG_SZ` text. Other types and all containers are read-only.
//!
//! # Exact writing
//!
//! The original bytes are kept in the step parameter `original`; the writer parses them again and
//! emits every unchanged chunk verbatim. A changed value is rendered from the tree, so only that
//! value's text changes.

mod read;
mod write;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    Codec, Confidence, DecodeCtx, Decoded, Error, ErrorCode, ParamValue, Params, Registry, Result,
    SniffCtx,
};

/// The file is not a `.reg` export (bad encoding or malformed line).
pub const REG_PARSE: ErrorCode = ErrorCode::new("SP-REG-001");
/// The text is not valid UTF-16LE or cannot be represented in the target ANSI code page.
pub const REG_ENCODING: ErrorCode = ErrorCode::new("SP-REG-002");
/// The document cannot be written as a `.reg` file (unknown value kind, wrong tree shape).
pub const REG_WRITE: ErrorCode = ErrorCode::new("SP-REG-003");

/// Prefix of every hint this codec writes.
pub const HINT_PREFIX: &str = "REG:";

/// Step parameter: the original file bytes.
pub(crate) const ORIGINAL: &str = "original";
/// Step parameter: `"utf16le"` or `"ansi"`.
pub(crate) const ENCODING: &str = "encoding";
/// Step parameter: whether the file started with a UTF-16LE BOM.
pub(crate) const BOM: &str = "bom";

pub(crate) const HEADER_V5: &str = "Windows Registry Editor Version 5.00";
pub(crate) const HEADER_REGEDIT4: &str = "REGEDIT4";

/// Registers the `.reg` codec as `format.reg`.
pub fn register(reg: &mut Registry) {
    reg.add_codec(Arc::new(RegCodec));
}

/// The `format.reg` codec.
#[derive(Clone, Copy, Debug, Default)]
pub struct RegCodec;

/// Text encoding of a `.reg` file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextEncoding {
    /// UTF-16LE with a BOM (the modern regedit export).
    Utf16Le,
    /// ANSI single-byte text (`REGEDIT4`).
    Ansi,
}

impl TextEncoding {
    fn name(self) -> &'static str {
        match self {
            TextEncoding::Utf16Le => "utf16le",
            TextEncoding::Ansi => "ansi",
        }
    }

    fn from_name(name: &str) -> Option<Self> {
        match name {
            "utf16le" => Some(TextEncoding::Utf16Le),
            "ansi" => Some(TextEncoding::Ansi),
            _ => None,
        }
    }
}

/// Windows-1252 bytes 0x80..=0x9F. Undefined code points map to the matching C1 control, so the
/// mapping is a bijection for every byte (bytes outside this range are Latin-1).
const CP1252_HIGH: [u16; 32] = [
    0x20AC, 0x0081, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160, 0x2039,
    0x0152, 0x008D, 0x017D, 0x008F, 0x0090, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014,
    0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0x009D, 0x017E, 0x0178,
];

pub(crate) fn ansi_decode(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| {
            let code = u16::from(*byte);
            if (0x80..=0x9F).contains(&code) {
                CP1252_HIGH
                    .get(usize::from(code - 0x80))
                    .copied()
                    .unwrap_or(code)
            } else {
                code
            }
        })
        .map(|code| char::from_u32(u32::from(code)).unwrap_or('\u{FFFD}'))
        .collect()
}

pub(crate) fn ansi_byte(ch: char) -> Option<u8> {
    let code = u32::from(ch);
    let target = u16::try_from(code).ok()?;
    if let Some(index) = CP1252_HIGH.iter().position(|value| *value == target) {
        return u8::try_from(0x80 + index).ok();
    }
    if (0x80..=0x9F).contains(&code) {
        return None;
    }
    u8::try_from(code).ok()
}

pub(crate) fn ansi_encode(text: &str) -> Result<Vec<u8>> {
    text.chars()
        .map(|ch| {
            ansi_byte(ch).ok_or_else(|| {
                Error::new(
                    REG_ENCODING,
                    format!(
                        "character U+{:04X} cannot be written in the ANSI code page",
                        u32::from(ch)
                    ),
                )
            })
        })
        .collect()
}

/// Decodes the input to text, returning the encoding and whether a BOM was present.
pub(crate) fn decode_text(input: &[u8]) -> Result<(String, TextEncoding, bool)> {
    if let Some(body) = input.strip_prefix(&[0xFF, 0xFE]) {
        if !body.len().is_multiple_of(2) {
            return Err(Error::new(
                REG_ENCODING,
                "UTF-16LE data has an odd number of bytes",
            ));
        }
        let units: Vec<u16> = body
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_le_bytes(*pair))
            .collect();
        let text = String::from_utf16(&units)
            .map_err(|_| Error::new(REG_ENCODING, "UTF-16LE data has an unpaired surrogate"))?;
        return Ok((text, TextEncoding::Utf16Le, true));
    }
    if input.starts_with(&[0xFE, 0xFF]) {
        return Err(Error::new(
            REG_ENCODING,
            "UTF-16BE files are not supported (regedit writes UTF-16LE)",
        ));
    }
    Ok((ansi_decode(input), TextEncoding::Ansi, false))
}

/// Encodes text in the given encoding, with a BOM for UTF-16LE when the source had one.
pub(crate) fn encode_text(text: &str, encoding: TextEncoding, bom: bool) -> Result<Vec<u8>> {
    match encoding {
        TextEncoding::Utf16Le => {
            let mut out = Vec::with_capacity(text.len().saturating_mul(2).saturating_add(2));
            if bom {
                out.extend_from_slice(&[0xFF, 0xFE]);
            }
            for unit in text.encode_utf16() {
                out.extend_from_slice(&unit.to_le_bytes());
            }
            Ok(out)
        }
        TextEncoding::Ansi => ansi_encode(text),
    }
}

/// Splits text into physical lines, keeping each line's ending (`\r\n`, `\n` or empty at the end).
pub(crate) fn split_lines(text: &str) -> Vec<(String, String)> {
    let mut lines = Vec::new();
    let mut start = 0_usize;
    let bytes = text.as_bytes();
    for index in 0..bytes.len() {
        if bytes.get(index) != Some(&b'\n') {
            continue;
        }
        let end = if index > start && bytes.get(index - 1) == Some(&b'\r') {
            index - 1
        } else {
            index
        };
        let content = text.get(start..end).unwrap_or("").to_string();
        let eol = text.get(end..=index).unwrap_or("").to_string();
        lines.push((content, eol));
        start = index + 1;
    }
    if start <= text.len() {
        let content = text.get(start..).unwrap_or("").to_string();
        let eol = String::new();
        lines.push((content, eol));
    }
    lines
}

/// The full name of a value without Unity's `_h<decimal hash>` suffix.
pub(crate) fn display_name(full: &str) -> &str {
    if let Some(position) = full.rfind("_h") {
        let digits = full.get(position + 2..).unwrap_or("");
        if !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit()) {
            return full.get(..position).unwrap_or(full);
        }
    }
    full
}

/// Parses a value hint into its tag and full registry name.
pub(crate) fn parse_value_hint(hint: &str) -> Option<(&str, &str)> {
    hint.strip_prefix(HINT_PREFIX)?.split_once(':')
}

impl Codec for RegCodec {
    fn id(&self) -> &'static str {
        "format.reg"
    }

    /// A UTF-16LE BOM followed by the version header, or an ANSI `REGEDIT4` header: high
    /// confidence. The extension alone is a weak hint.
    fn sniff(&self, input: &[u8], ctx: &SniffCtx<'_>) -> Confidence {
        if let Some(body) = input.strip_prefix(&[0xFF, 0xFE]) {
            for header in [HEADER_V5, HEADER_REGEDIT4] {
                let bytes: Vec<u8> = header.encode_utf16().flat_map(u16::to_le_bytes).collect();
                if body.starts_with(&bytes) {
                    return Confidence::HIGH;
                }
            }
            return Confidence::NONE;
        }
        if input.starts_with(HEADER_V5.as_bytes()) || input.starts_with(HEADER_REGEDIT4.as_bytes())
        {
            return Confidence::HIGH;
        }
        let extension = ctx
            .file_name
            .is_some_and(|name| name.to_ascii_lowercase().ends_with(".reg"));
        if extension {
            Confidence::LOW
        } else {
            Confidence::NONE
        }
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<Decoded> {
        ctx.limits().check_input(len_u64(input.len()))?;
        let parsed = read::parse(input)?;
        let depth = 2_usize;
        ctx.check_depth(depth)?;
        let mut doc = ctx.new_doc();
        let mut root_keys = Vec::with_capacity(parsed.sections.len());
        for section in &parsed.sections {
            let mut values = Vec::with_capacity(section.values.len());
            for value in &section.values {
                let mut node = saveprism_core::Node::new(value.value.clone());
                node.type_hint = Some(saveprism_core::TypeHint::new(value.hint.clone()));
                if value.read_only {
                    node.flags.insert(saveprism_core::NodeFlags::READ_ONLY);
                }
                let id = doc.push(node)?;
                values.push((value.display_name.clone(), id));
            }
            let mut key_node = saveprism_core::Node::new(saveprism_core::Value::Object(values));
            key_node.type_hint = Some(saveprism_core::TypeHint::new(format!(
                "{HINT_PREFIX}key:{}",
                section.key
            )));
            key_node.flags.insert(saveprism_core::NodeFlags::READ_ONLY);
            let id = doc.push(key_node)?;
            root_keys.push((section.key.clone(), id));
        }
        let mut root = saveprism_core::Node::new(saveprism_core::Value::Object(root_keys));
        root.type_hint = Some(saveprism_core::TypeHint::new(format!("{HINT_PREFIX}root")));
        root.flags.insert(saveprism_core::NodeFlags::READ_ONLY);
        let root = doc.push(root)?;
        doc.set_root(root);
        ctx.charge_nodes(len_u64(doc.len()))?;

        let mut params = Params::new();
        params.set(ORIGINAL, input.to_vec());
        params.set(ENCODING, parsed.encoding.name());
        params.set(BOM, parsed.bom);
        Ok(Decoded { doc, params })
    }

    fn encode(&self, doc: &saveprism_core::SaveDoc, params: &Params) -> Result<Vec<u8>> {
        let original = match params.get(ORIGINAL) {
            Some(ParamValue::Bytes(bytes)) => bytes.as_slice(),
            None => return write::encode_canonical(doc),
            Some(_) => {
                return Err(Error::new(
                    REG_WRITE,
                    "parameter \"original\" of format.reg must be bytes",
                ));
            }
        };
        let encoding = match params.get(ENCODING) {
            Some(ParamValue::Str(name)) => TextEncoding::from_name(name).ok_or_else(|| {
                Error::new(REG_WRITE, format!("unknown format.reg encoding \"{name}\""))
            })?,
            None => return Err(Error::new(REG_WRITE, "the file encoding is missing")),
            Some(_) => {
                return Err(Error::new(
                    REG_WRITE,
                    "parameter \"encoding\" of format.reg must be text",
                ));
            }
        };
        let bom = matches!(params.get(BOM), Some(ParamValue::Bool(true)));
        write::encode_original(doc, original, encoding, bom)
    }
}
