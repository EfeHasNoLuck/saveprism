//! Lossless writer for the TJS2 `saveStruct` text: replaces only the tokens an edit changed.

use saveprism_core::{Error, NodeFlags, Result, SaveDoc, Value};

use crate::text_common::{apply_encoded_patches, decode_original, format_float};

use super::TJS_WRITE;

/// Writes the document back over `original` (the exact bytes `decode` received).
pub(crate) fn encode(doc: &SaveDoc, original: &[u8]) -> Result<Vec<u8>> {
    let decoded = decode_original(original, TJS_WRITE)?;
    let mut patches = Vec::new();
    let mut stack = vec![doc.root()];
    let mut visited = 0usize;
    while let Some(id) = stack.pop() {
        visited = visited.saturating_add(1);
        if visited > doc.len().saturating_add(1) {
            return Err(Error::new(TJS_WRITE, "cyclic TJS2 tree"));
        }
        let Some(node) = doc.get(id) else {
            continue;
        };
        stack.extend(doc.children(id));
        if let Value::Map(entries) = &node.value {
            for (key, value) in entries {
                stack.push(*key);
                stack.push(*value);
            }
        }
        if node.flags.contains(NodeFlags::READ_ONLY) || node.lexeme.is_some() {
            continue;
        }
        let Some(hint) = node
            .type_hint
            .as_ref()
            .map(saveprism_core::TypeHint::as_str)
        else {
            continue;
        };
        let Some(span) = node.span else {
            continue;
        };
        let rendered = match (hint, &node.value) {
            ("TJS:int", Value::Int { v, .. }) => Some(v.to_string()),
            ("TJS:real", Value::Float { v, .. }) => Some(format_float(*v)),
            ("TJS:real", Value::Int { v, .. }) => Some(format_float(*v as f64)),
            ("TJS:string", Value::Str(text)) => Some(escape_string(text)),
            _ => None,
        };
        let Some(rendered) = rendered else {
            continue;
        };
        let start = usize::try_from(span.start)
            .map_err(|_| Error::new(TJS_WRITE, "invalid TJS2 token span"))?;
        let end = usize::try_from(span.end)
            .map_err(|_| Error::new(TJS_WRITE, "invalid TJS2 token span"))?;
        let old = decoded
            .text
            .get(start..end)
            .ok_or_else(|| Error::new(TJS_WRITE, "TJS2 token span outside the original text"))?;
        if old != rendered {
            patches.push((start, end, rendered.into_bytes()));
        }
    }
    apply_encoded_patches(original, &decoded, TJS_WRITE, &mut patches)
}

/// Escapes text the way TJS2's `EscapeC` does (C escapes, `\xNN` for control characters).
fn escape_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len().saturating_add(2));
    out.push('"');
    let mut hexflag = false;
    for character in text.chars() {
        match character {
            '\u{07}' => {
                out.push_str("\\a");
                hexflag = false;
            }
            '\u{08}' => {
                out.push_str("\\b");
                hexflag = false;
            }
            '\u{0c}' => {
                out.push_str("\\f");
                hexflag = false;
            }
            '\n' => {
                out.push_str("\\n");
                hexflag = false;
            }
            '\r' => {
                out.push_str("\\r");
                hexflag = false;
            }
            '\t' => {
                out.push_str("\\t");
                hexflag = false;
            }
            '\u{0b}' => {
                out.push_str("\\v");
                hexflag = false;
            }
            '\\' => {
                out.push_str("\\\\");
                hexflag = false;
            }
            '\'' => {
                out.push_str("\\'");
                hexflag = false;
            }
            '"' => {
                out.push_str("\\\"");
                hexflag = false;
            }
            other => {
                if hexflag && other.is_ascii_hexdigit() {
                    out.push_str(&format!("\\x{:02x}", u32::from(other)));
                    continue;
                }
                if (other as u32) < 0x20 {
                    out.push_str(&format!("\\x{:02x}", u32::from(other)));
                    hexflag = true;
                } else {
                    out.push(other);
                    hexflag = false;
                }
            }
        }
    }
    out.push('"');
    out
}
