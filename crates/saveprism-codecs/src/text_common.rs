//! Helpers shared by the structured-text codecs (XML, INI, properties, TOML, YAML, CSV, plist).
//!
//! All of these keep the original text as the codec parameter `original` and let every editable
//! scalar record the byte span of its token. Writing applies only the spans whose value changed and
//! copies every other byte, so comments, spacing, order and line endings stay untouched.

use std::collections::HashMap;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    ByteSpan, Error, ErrorCode, Node, NodeFlags, NodeId, Result, SaveDoc, TypeHint, Value,
};

use crate::text_encoding::{DecodedText, TextPatch, apply_text_patches, decode_sniff};

/// Decodes the original file for a writer. The spans recorded by the reader are offsets into the
/// returned UTF-8 text (the BOM is already stripped).
pub(crate) fn decode_original(original: &[u8], code: ErrorCode) -> Result<DecodedText> {
    decode_sniff(original)
        .ok_or_else(|| Error::new(code, "the text encoding of the original file is not known"))
}

/// Applies UTF-8-text patches to the original bytes, translating spans into the file encoding.
pub(crate) fn apply_encoded_patches(
    original: &[u8],
    decoded: &DecodedText,
    code: ErrorCode,
    patches: &mut Vec<TextPatch>,
) -> Result<Vec<u8>> {
    apply_text_patches(original, decoded, code, patches)
}

/// Applies non-overlapping byte patches to `original`.
pub(crate) fn apply_patches(
    original: &[u8],
    code: ErrorCode,
    patches: &mut Vec<(usize, usize, Vec<u8>)>,
) -> Result<Vec<u8>> {
    patches.sort_by_key(|(start, _, _)| *start);
    for pair in patches.windows(2) {
        if let (Some(first), Some(second)) = (pair.first(), pair.get(1))
            && first.1 > second.0
        {
            return Err(Error::new(code, "overlapping token spans"));
        }
    }
    let mut out = original.to_vec();
    for (start, end, replacement) in patches.drain(..).rev() {
        if start > end || end > out.len() {
            return Err(Error::new(code, "token span outside the original text"));
        }
        out.splice(start..end, replacement);
    }
    Ok(out)
}

/// Reads a stored byte span as a `usize` range.
pub(crate) fn span_range(node: &Node, code: ErrorCode) -> Result<(usize, usize)> {
    let span = node
        .span
        .ok_or_else(|| Error::new(code, "editable token has no span"))?;
    let start = usize::try_from(span.start).map_err(|_| Error::new(code, "invalid token span"))?;
    let end = usize::try_from(span.end).map_err(|_| Error::new(code, "invalid token span"))?;
    Ok((start, end))
}

/// A read-only node with a hint.
pub(crate) fn read_only(value: Value, hint: &str) -> Node {
    let mut node = Node::new(value);
    node.type_hint = Some(TypeHint::new(hint.to_owned()));
    node.flags.insert(NodeFlags::READ_ONLY);
    node
}

/// A node with a hint and an optional span; editable nodes get a lexeme marker.
pub(crate) fn node(value: Value, hint: &str, span: Option<(usize, usize)>, editable: bool) -> Node {
    let mut node = Node::new(value);
    node.type_hint = Some(TypeHint::new(hint.to_owned()));
    node.span = span.map(|(start, end)| ByteSpan {
        start: len_u64(start),
        end: len_u64(end),
    });
    if editable {
        node.lexeme = Some("text:original".into());
    } else {
        node.flags.insert(NodeFlags::READ_ONLY);
    }
    node
}

/// Adds a child under `key`; repeated keys become `key#2`, `key#3`, and so on, in order.
pub(crate) fn append_numbered(
    doc: &mut SaveDoc,
    entries: &mut Vec<(String, NodeId)>,
    _group_hint: &str,
    key: String,
    id: NodeId,
) -> Result<()> {
    let _ = doc;
    if !entries.iter().any(|(name, _)| name == &key) {
        entries.push((key, id));
        return Ok(());
    }
    let mut number = 2usize;
    loop {
        let name = format!("{key}#{number}");
        if !entries.iter().any(|(existing, _)| existing == &name) {
            entries.push((name, id));
            return Ok(());
        }
        number = number.saturating_add(1);
        if number == usize::MAX {
            return Err(Error::new(
                ErrorCode::new("SP-TEXT-001"),
                "too many repeated child keys",
            ));
        }
    }
}

/// True when `text` contains only ASCII whitespace.
pub(crate) fn is_blank(text: &str) -> bool {
    text.bytes().all(|byte| byte.is_ascii_whitespace())
}

/// Escapes text for XML character data or an attribute value (quotes are escaped in both).
pub(crate) fn escape_xml(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            character => out.push(character),
        }
    }
    out
}

/// Formats an `f64` with a decimal point or exponent so it reads back as a float.
pub(crate) fn format_float(value: f64) -> String {
    if value.is_nan() {
        return "nan".to_owned();
    }
    if value == f64::INFINITY {
        return "inf".to_owned();
    }
    if value == f64::NEG_INFINITY {
        return "-inf".to_owned();
    }
    let text = value.to_string();
    if text.bytes().any(|byte| matches!(byte, b'.' | b'e' | b'E')) {
        text
    } else {
        format!("{text}.0")
    }
}

/// True for extensions that must keep going to the JSON/text codecs, whatever their content.
pub(crate) fn is_generic_text_name(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    [".txt", ".json", ".jsonl", ".log", ".md"]
        .iter()
        .any(|extension| name.ends_with(extension))
}

/// Copies the graph reachable from `root` into `target`, preserving references and cycles.
/// `code` is the error used when a node is missing or a reference leaves the copied graph.
pub(crate) fn copy_graph(
    source: &SaveDoc,
    target: &mut SaveDoc,
    root: NodeId,
    code: ErrorCode,
) -> Result<NodeId> {
    let mut map: HashMap<NodeId, NodeId> = HashMap::new();
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        if map.contains_key(&id) {
            continue;
        }
        let Some(node) = source.get(id) else {
            return Err(Error::new(
                code,
                "the decoded data refers to a missing node",
            ));
        };
        let new_id = target.reserve()?;
        map.insert(id, new_id);
        match &node.value {
            Value::Array(items) => stack.extend(items.iter().copied()),
            Value::Object(entries) => stack.extend(entries.iter().map(|(_, child)| *child)),
            Value::Map(entries) => {
                for (key, value) in entries {
                    stack.push(*key);
                    stack.push(*value);
                }
            }
            Value::Ref(child) => stack.push(*child),
            _ => {}
        }
    }
    let mut pending: Vec<(NodeId, NodeId)> = map.iter().map(|(old, new)| (*old, *new)).collect();
    pending.sort_by_key(|(old, _)| old.index());
    for (old, new) in pending {
        let Some(node) = source.get(old) else {
            continue;
        };
        let mut copy = node.clone();
        match &mut copy.value {
            Value::Array(items) => {
                for item in items.iter_mut() {
                    *item = remap(*item, &map, code)?;
                }
            }
            Value::Object(entries) => {
                for (_, child) in entries.iter_mut() {
                    *child = remap(*child, &map, code)?;
                }
            }
            Value::Map(entries) => {
                for (key, value) in entries.iter_mut() {
                    *key = remap(*key, &map, code)?;
                    *value = remap(*value, &map, code)?;
                }
            }
            Value::Ref(child) => *child = remap(*child, &map, code)?,
            _ => {}
        }
        target.finish(new, copy)?;
    }
    map.get(&root)
        .copied()
        .ok_or_else(|| Error::new(code, "the decoded data has no root"))
}

fn remap(id: NodeId, map: &HashMap<NodeId, NodeId>, code: ErrorCode) -> Result<NodeId> {
    map.get(&id)
        .copied()
        .ok_or_else(|| Error::new(code, "a reference leaves the copied data"))
}
