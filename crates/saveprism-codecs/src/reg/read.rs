//! Parses a `.reg` file into sections of values and a chunk list that keeps every original byte.

use saveprism_core::{Error, Result, Value};

use super::{
    HEADER_REGEDIT4, HEADER_V5, REG_PARSE, TextEncoding, decode_text, display_name, split_lines,
};

/// One original chunk of the file: raw text, or a value whose rendering may change.
pub(crate) enum Chunk {
    /// Text emitted verbatim (header, key lines, blank lines, comments).
    Raw(String),
    /// A value: the section and the index inside that section.
    Value { section: usize, index: usize },
}

/// A parsed value.
pub(crate) struct ValueData {
    /// Display key (Unity `_h<number>` suffix removed).
    pub display_name: String,
    /// Full registry name.
    pub full_name: String,
    /// Parsed value.
    pub value: Value,
    /// `REG:<tag>:<full name>`.
    pub hint: String,
    /// True for values the editor must not change.
    pub read_only: bool,
    /// Original physical lines, endings included.
    pub raw: String,
    /// Line ending of the first physical line.
    pub eol: String,
}

/// A key and its values.
pub(crate) struct SectionData {
    /// Full key path.
    pub key: String,
    /// Values in file order.
    pub values: Vec<ValueData>,
}

/// Everything the writer needs to reproduce the file.
pub(crate) struct ParsedFile {
    /// Text encoding.
    pub encoding: TextEncoding,
    /// Whether the file started with a UTF-16LE BOM.
    pub bom: bool,
    /// Sections in file order.
    pub sections: Vec<SectionData>,
    /// Chunks in file order.
    pub chunks: Vec<Chunk>,
}

fn parse_error(message: impl Into<String>) -> Error {
    Error::new(REG_PARSE, message)
}

/// Parses `input`; every line is checked, nothing is executed.
pub(crate) fn parse(input: &[u8]) -> Result<ParsedFile> {
    if input.is_empty() {
        return Err(parse_error("the file is empty"));
    }
    let (text, encoding, bom) = decode_text(input)?;
    let lines = split_lines(&text);
    let Some((first, first_eol)) = lines.first() else {
        return Err(parse_error("the file is empty"));
    };
    if first != HEADER_V5 && first != HEADER_REGEDIT4 {
        return Err(parse_error(format!(
            "the first line is not a registry editor header (found {:?})",
            first
        )));
    }

    let mut sections: Vec<SectionData> = Vec::new();
    let mut chunks: Vec<Chunk> = Vec::new();
    let mut current: Option<usize> = None;
    let mut raw = String::new();
    raw.push_str(first);
    raw.push_str(first_eol);

    let mut index = 1_usize;
    while let Some((line, eol)) = lines.get(index) {
        index += 1;
        let trimmed = line.trim_start();
        if line.trim().is_empty() || trimmed.starts_with(';') {
            raw.push_str(line);
            raw.push_str(eol);
            continue;
        }
        if trimmed.starts_with('[') {
            let key = parse_key(line).ok_or_else(|| {
                parse_error(format!("line {index}: malformed key line {:?}", line))
            })?;
            if !raw.is_empty() {
                chunks.push(Chunk::Raw(std::mem::take(&mut raw)));
            }
            sections.push(SectionData {
                key,
                values: Vec::new(),
            });
            current = Some(sections.len().saturating_sub(1));
            raw.push_str(line);
            raw.push_str(eol);
            continue;
        }

        // A value line, possibly continued on following lines with a trailing backslash.
        let mut block = String::new();
        block.push_str(line);
        block.push_str(eol);
        let mut logical = line.clone();
        while logical.ends_with('\\') {
            logical.pop();
            let Some((next, next_eol)) = lines.get(index) else {
                return Err(parse_error(format!(
                    "line {index}: a continued value ends at the end of the file"
                )));
            };
            index += 1;
            block.push_str(next);
            block.push_str(next_eol);
            logical.push_str(next);
        }

        let section = current.ok_or_else(|| {
            parse_error(format!(
                "line {index}: a value appears before the first [key] line"
            ))
        })?;
        let (full_name, spec) = split_name(&logical)
            .ok_or_else(|| parse_error(format!("line {index}: malformed value line {:?}", line)))?;
        let (value, tag, read_only) = parse_value(&full_name, spec)
            .map_err(|error| error.context(format!("line {index}")))?;
        let data = ValueData {
            display_name: display_name(&full_name).to_string(),
            hint: format!(
                "{}{}:{}",
                super::HINT_PREFIX,
                tag,
                if full_name == "@" { "@" } else { &full_name }
            ),
            full_name,
            value,
            read_only,
            raw: block,
            eol: eol.clone(),
        };
        if !raw.is_empty() {
            chunks.push(Chunk::Raw(std::mem::take(&mut raw)));
        }
        let Some(section_data) = sections.get_mut(section) else {
            return Err(parse_error("internal section index is invalid"));
        };
        let value_index = section_data.values.len();
        section_data.values.push(data);
        chunks.push(Chunk::Value {
            section,
            index: value_index,
        });
    }
    if !raw.is_empty() {
        chunks.push(Chunk::Raw(raw));
    }

    Ok(ParsedFile {
        encoding,
        bom,
        sections,
        chunks,
    })
}

/// `[key path]` → key path. Trailing spaces inside the brackets are kept.
fn parse_key(line: &str) -> Option<String> {
    let start = line.find('[')?;
    let end = line.rfind(']')?;
    if end <= start {
        return None;
    }
    let key = line.get(start + 1..end)?;
    if key.is_empty() {
        return None;
    }
    Some(key.to_string())
}

/// Splits a value line into the full value name (`@` for the default value) and the data text.
fn split_name(line: &str) -> Option<(String, &str)> {
    let trimmed = line.trim_start();
    if let Some(rest) = trimmed.strip_prefix('@') {
        let rest = rest.trim_start();
        let spec = rest.strip_prefix('=')?.trim();
        return Some(("@".to_string(), spec));
    }
    if !trimmed.starts_with('"') {
        return None;
    }
    let chars: Vec<(usize, char)> = trimmed.char_indices().collect();
    let mut escaped = false;
    let mut close = None;
    for (offset, ch) in chars.iter().skip(1) {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' => escaped = true,
            '"' => {
                close = Some(*offset);
                break;
            }
            _ => {}
        }
    }
    let close = close?;
    let name_text = trimmed.get(..=close)?;
    let name = unescape(name_text)?
        .strip_prefix('"')?
        .strip_suffix('"')?
        .to_string();
    let rest = trimmed.get(close + 1..)?.trim_start();
    let spec = rest.strip_prefix('=')?.trim();
    Some((name, spec))
}

/// Reverses `\\` and `\"` in a quoted name or `REG_SZ` text.
pub(crate) fn unescape(text: &str) -> Option<String> {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('\\') => out.push('\\'),
            Some('"') => out.push('"'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    Some(out)
}

/// Parses the data after `=` into a value, a hint tag and whether it is read-only.
fn parse_value(full_name: &str, spec: &str) -> Result<(Value, &'static str, bool)> {
    if spec == "-" {
        return Ok((Value::Null, "delete", true));
    }
    if spec.starts_with('"') {
        let text = unescape(spec)
            .and_then(|text| text.strip_prefix('"').map(str::to_string))
            .and_then(|text| text.strip_suffix('"').map(str::to_string))
            .ok_or_else(|| parse_error("a REG_SZ value is not quoted correctly"))?;
        return Ok((Value::Str(text), "sz", false));
    }
    if let Some(rest) = spec.strip_prefix("dword:") {
        let value = parse_dword(rest)?;
        return Ok((
            Value::Int {
                v: i128::from(value),
                width: saveprism_core::IntWidth::I32,
            },
            "dword",
            false,
        ));
    }
    if let Some((kind, bytes)) = parse_hex(spec)? {
        return Ok(classify(full_name, kind, bytes));
    }
    Err(parse_error(format!(
        "the value data {:?} has an unknown form",
        spec
    )))
}

fn parse_dword(text: &str) -> Result<i32> {
    if text.len() != 8 {
        return Err(parse_error("a dword needs exactly 8 hex digits"));
    }
    let value = u32::from_str_radix(text, 16)
        .map_err(|_| parse_error(format!("{:?} is not a dword", text)))?;
    Ok(value as i32)
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn parse_hex_byte(text: &str) -> Option<u8> {
    let bytes = text.as_bytes();
    if bytes.len() != 2 {
        return None;
    }
    let high = hex_nibble(*bytes.first()?)?;
    let low = hex_nibble(*bytes.get(1)?)?;
    Some((high << 4) | low)
}

/// `hex:`, `hex(b):`, `hex(2):`, ... → the type (when given) and the bytes.
fn parse_hex(spec: &str) -> Result<Option<(Option<u32>, Vec<u8>)>> {
    let Some(rest) = spec.strip_prefix("hex") else {
        return Ok(None);
    };
    let (kind, list) = if let Some(after) = rest.strip_prefix('(') {
        let Some(end) = after.find(')') else {
            return Err(parse_error("a hex type has no closing parenthesis"));
        };
        let token = after.get(..end).unwrap_or("");
        if token.is_empty() {
            return Err(parse_error("a hex type is empty"));
        }
        let kind = u32::from_str_radix(token, 16)
            .map_err(|_| parse_error(format!("{:?} is not a hex type", token)))?;
        let Some(list) = after.get(end + 1..).and_then(|text| text.strip_prefix(':')) else {
            return Err(parse_error("a hex type is not followed by ':'"));
        };
        (Some(kind), list)
    } else {
        let Some(list) = rest.strip_prefix(':') else {
            return Ok(None);
        };
        (None, list)
    };
    let mut bytes = Vec::new();
    if !list.trim().is_empty() {
        for item in list.split(',') {
            let item = item.trim();
            if item.is_empty() {
                return Err(parse_error("a hex list has an empty byte"));
            }
            let byte = parse_hex_byte(item)
                .ok_or_else(|| parse_error(format!("{:?} is not a hex byte", item)))?;
            bytes.push(byte);
        }
    }
    Ok(Some((kind, bytes)))
}

/// True for a Unity value name with the `_h<decimal hash>` suffix.
fn is_unity_name(name: &str) -> bool {
    display_name(name) != name
}

/// The trailing-NUL UTF-8 Unity string in `bytes`, if there is one.
fn unity_string(bytes: &[u8]) -> Option<String> {
    let body = bytes.strip_suffix(&[0x00])?;
    let text = std::str::from_utf8(body).ok()?;
    Some(text.to_string())
}

fn classify(full_name: &str, kind: Option<u32>, bytes: Vec<u8>) -> (Value, &'static str, bool) {
    match (kind, bytes.len()) {
        (Some(4), 4) => {
            let value = u32::from_le_bytes([
                bytes.first().copied().unwrap_or(0),
                bytes.get(1).copied().unwrap_or(0),
                bytes.get(2).copied().unwrap_or(0),
                bytes.get(3).copied().unwrap_or(0),
            ]);
            (
                Value::Int {
                    v: i128::from(value as i32),
                    width: saveprism_core::IntWidth::I32,
                },
                "hex4-int",
                false,
            )
        }
        (Some(0x0B), 8) | (Some(4), 8) => (
            float_value(&bytes),
            if kind == Some(0x0B) { "qword" } else { "hex4" },
            false,
        ),
        (None, _) => {
            if is_unity_name(full_name)
                && let Some(text) = unity_string(&bytes)
            {
                return (Value::Str(text), "unity-string", false);
            }
            (Value::Bytes(bytes), "binary", true)
        }
        (Some(2), _) => (Value::Bytes(bytes), "expand", true),
        (Some(7), _) => (Value::Bytes(bytes), "multi", true),
        _ => (Value::Bytes(bytes), "other", true),
    }
}

fn float_value(bytes: &[u8]) -> Value {
    let mut raw = [0_u8; 8];
    for (slot, byte) in raw.iter_mut().zip(bytes.iter().copied()) {
        *slot = byte;
    }
    Value::Float {
        v: f64::from_bits(u64::from_le_bytes(raw)),
        width: saveprism_core::FloatWidth::F64,
    }
}
