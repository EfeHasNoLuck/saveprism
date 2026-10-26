//! `format.adventure-creator`: Adventure Creator (Unity) save files.
//!
//! Adventure Creator (AC) writes one save as a serialized "main data" part and a serialized
//! "scene data" part, joined by the divider `||` (the public `SaveSystem` reference lists
//! `mainDataDivider = "||"` and `mainDataDivider_Replacement = "*DOUBLEPIPE*"`). The JSON format
//! handler joins several per-scene records with `|ROOMDELIMITER|` (the public
//! `FileFormatHandler_Json` reference lists `roomDelimiter = "|ROOMDELIMITER|"`). Three public
//! handlers exist: JSON, XML and Binary; the binary handler serializes .NET `BinaryFormatter`
//! data, which AC stores as Base64 text.
//!
//! This codec splits the file into its exact parts, decodes each part with the existing codec
//! (JSON, XML or Base64 -> [`NrbfCodec`]) and keeps every part as a child tree. Edits go into the
//! part they belong to; an unchanged part is written back with its original bytes.
//!
//! # Tree
//!
//! The root is a read-only `Array` (`AC:parts`). Each element is a read-only `Object`
//! (`AC:part:<json|xml|nrbf>`) with one key, `data`, whose value is the decoded part.
//!
//! # Exact writing
//!
//! The codec parameters keep the original bytes plus, for each part, the child codec parameters
//! and the exact separator before the next part. Writing encodes each part with its own codec and
//! joins them again with those separators, so an unchanged save is byte-identical.
//!
//! Sources (behaviour only, no code copied): the public AC scripting guide pages for
//! [`SaveSystem`](https://adventurecreator.org/scripting-guide/class_a_c_1_1_save_system.html)
//! (dividers, save file contents), [`FileFormatHandler_Json`](https://adventurecreator.org/scripting-guide/class_a_c_1_1_file_format_handler___json.html)
//! (room delimiter), [`FileFormatHandler_Binary`](https://adventurecreator.org/scripting-guide/class_a_c_1_1_file_format_handler___binary.html)
//! (BinaryFormatter data as a string) and the `SaveData` / `SingleLevelData` class references.

mod read;
#[cfg(test)]
mod tests;
mod write;

use std::sync::Arc;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    Codec, Confidence, DecodeCtx, Decoded, Error, ErrorCode, Node, NodeFlags, Params, Registry,
    Result, SaveDoc, SniffCtx, TypeHint, Value,
};

use read::{decode_parts, split_parts};
use write::encode_parts;

/// The save is not an Adventure Creator save (no main divider or no known part format).
pub const AC_PARSE: ErrorCode = ErrorCode::new("SP-AC-001");
/// A part could not be decoded with the codec its content calls for.
pub const AC_PART: ErrorCode = ErrorCode::new("SP-AC-002");
/// The document cannot be written back (missing parameters or a changed part count).
pub const AC_WRITE: ErrorCode = ErrorCode::new("SP-AC-003");

/// The divider between the main data and the scene data (public `SaveSystem.mainDataDivider`).
pub(crate) const MAIN_DIVIDER: &[u8] = b"||";
/// The divider between per-scene JSON records (public `FileFormatHandler_Json.roomDelimiter`).
pub(crate) const ROOM_DIVIDER: &[u8] = b"|ROOMDELIMITER|";

/// Hint of the root and of a part wrapper.
pub(crate) const PARTS_HINT: &str = "AC:parts";
pub(crate) const PART_HINT: &str = "AC:part";

/// Registers the Adventure Creator codec as `format.adventure-creator`.
pub fn register(reg: &mut Registry) {
    reg.add_codec(Arc::new(AdventureCreatorCodec));
}

/// The `format.adventure-creator` codec.
#[derive(Clone, Copy, Debug, Default)]
pub struct AdventureCreatorCodec;

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

fn trim_start_index(bytes: &[u8]) -> usize {
    bytes
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .unwrap_or(bytes.len())
}

/// True when the text looks like standard Base64 (padding allowed, whitespace ignored).
fn looks_like_base64(text: &[u8]) -> bool {
    let mut symbols = 0usize;
    for &byte in text {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'+' | b'/' => {
                symbols = symbols.saturating_add(1)
            }
            b'=' => {}
            b' ' | b'\t' | b'\r' | b'\n' => {}
            _ => return false,
        }
    }
    symbols >= 16
}

impl Codec for AdventureCreatorCodec {
    fn id(&self) -> &'static str {
        "format.adventure-creator"
    }

    fn sniff(&self, input: &[u8], ctx: &SniffCtx<'_>) -> Confidence {
        let Ok(text) = std::str::from_utf8(input) else {
            return Confidence::NONE;
        };
        let bytes = text.as_bytes();
        let Some(divider) = text.find("||") else {
            return Confidence::NONE;
        };
        if divider == 0 {
            return Confidence::NONE;
        }
        let main = bytes.get(..divider).unwrap_or_default();
        let rest = bytes
            .get(divider.saturating_add(MAIN_DIVIDER.len())..)
            .unwrap_or_default();
        let start = trim_start_index(main);
        let main_body = main.get(start..).unwrap_or_default();
        if main_body.is_empty() {
            return Confidence::NONE;
        }
        let rest_start = trim_start_index(rest);
        let rest_body = rest.get(rest_start..).unwrap_or_default();
        let rest_ok = rest_body.is_empty()
            || rest_body.first().is_some_and(|byte| {
                matches!(byte, b'{' | b'[' | b'<') || looks_like_base64(rest_body)
            });
        let named = ctx.file_name.is_some_and(|name| {
            let lower = name.to_ascii_lowercase();
            [".save", ".sav", ".dat", ".txt"]
                .iter()
                .any(|extension| lower.ends_with(extension))
        });
        let json_like = main_body.first() == Some(&b'{') && contains(main_body, b"\"mainData\"");
        let xml_like = main_body.first() == Some(&b'<')
            && contains(main_body, b"<SaveData")
            && contains(main_body, b"mainData");
        let binary_like = looks_like_base64(main_body) && read::looks_like_nrbf_base64(main_body);
        let score = if rest_ok && (json_like || xml_like) {
            0.96
        } else if rest_ok && binary_like {
            0.94
        } else {
            return Confidence::NONE;
        };
        Confidence::new(if named { score + 0.02 } else { score })
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<Decoded> {
        ctx.limits().check_input(len_u64(input.len()))?;
        let text = std::str::from_utf8(input).map_err(|_| {
            Error::new(
                AC_PARSE,
                "an Adventure Creator save is text (JSON, XML or Base64)",
            )
        })?;
        let (parts, separators, kind) = split_parts(text.as_bytes())?;
        if parts.is_empty() {
            return Err(Error::new(AC_PARSE, "the save has no data parts"));
        }
        let decoded = decode_parts(&parts, kind, ctx)?;
        let mut doc = ctx.new_doc();
        let mut wrappers = Vec::with_capacity(decoded.len());
        for part in &decoded {
            let child =
                crate::text_common::copy_graph(&part.doc, &mut doc, part.doc.root(), AC_PART)?;
            let entries = vec![("data".to_owned(), child)];
            let mut wrapper = Node::new(Value::Object(entries));
            wrapper.type_hint = Some(TypeHint::new(format!("{PART_HINT}:{}", kind.name())));
            wrapper.lexeme = Some(part.text.clone().into_boxed_str());
            wrapper.flags.insert(NodeFlags::READ_ONLY);
            wrappers.push(doc.push(wrapper)?);
        }
        let mut root = Node::new(Value::Array(wrappers));
        root.type_hint = Some(TypeHint::new(PARTS_HINT));
        root.flags.insert(NodeFlags::READ_ONLY);
        let root = doc.push(root)?;
        doc.set_root(root);

        let mut params = Params::new();
        params.set("original", input.to_vec());
        params.set("parts", read::parts_param(&decoded));
        params.set("separators", read::separators_param(&separators));
        Ok(Decoded { doc, params })
    }

    fn encode(&self, doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
        encode_parts(doc, params)
    }
}
