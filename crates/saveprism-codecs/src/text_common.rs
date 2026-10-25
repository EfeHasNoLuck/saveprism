//! Helpers shared by the structured-text codecs (XML, INI, properties, TOML, YAML, CSV, plist).
//!
//! All of these keep the original text as the codec parameter `original` and let every editable
//! scalar record the byte span of its token. Writing applies only the spans whose value changed and
//! copies every other byte, so comments, spacing, order and line endings stay untouched.

use saveprism_core::limits::len_u64;
use saveprism_core::{
    ByteSpan, Error, ErrorCode, Node, NodeFlags, NodeId, Result, SaveDoc, TypeHint, Value,
};

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
