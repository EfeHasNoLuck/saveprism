//! Writing XML back: only edited text and attribute tokens change.

use saveprism_core::{Error, NodeFlags, Result, SaveDoc, TypeHint, Value};

use super::XML_WRITE;
use crate::text_common::{apply_patches, escape_xml, span_range};

/// Applies the edited leaves to `original`.
pub(crate) fn encode(doc: &SaveDoc, original: &[u8]) -> Result<Vec<u8>> {
    let mut patches = Vec::new();
    let mut stack = vec![doc.root()];
    let mut visited = 0usize;
    while let Some(id) = stack.pop() {
        visited = visited.saturating_add(1);
        if visited > doc.len().saturating_add(1) {
            return Err(Error::new(XML_WRITE, "cyclic XML tree"));
        }
        let Some(node) = doc.get(id) else {
            return Err(Error::new(XML_WRITE, "invalid node in the XML tree"));
        };
        stack.extend(doc.children(id));
        if let Value::Map(entries) = &node.value {
            stack.extend(entries.iter().map(|(key, _)| *key));
        }
        if node.flags.contains(NodeFlags::READ_ONLY) || node.lexeme.is_some() {
            continue;
        }
        let Some(hint) = node.type_hint.as_ref().map(TypeHint::as_str) else {
            continue;
        };
        let Value::Str(text) = &node.value else {
            continue;
        };
        let rendered = match hint {
            "XML:text" | "XML:attribute" => escape_xml(text),
            _ => continue,
        };
        let (start, end) = span_range(node, XML_WRITE)?;
        let old = original
            .get(start..end)
            .ok_or_else(|| Error::new(XML_WRITE, "token span outside the original text"))?;
        if old != rendered.as_bytes() {
            patches.push((start, end, rendered.into_bytes()));
        }
    }
    apply_patches(original, XML_WRITE, &mut patches)
}
