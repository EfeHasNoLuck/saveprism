//! Iterative JSON writer (explicit stack, no recursion).

use std::collections::HashSet;

use saveprism_core::{Error, NodeId, Result, SaveDoc, Value, codes};

use super::JSON_UNWRITABLE;
use super::profile::{Indent, Profile};

/// A written document may not grow beyond this (guards shared-child explosions).
const MAX_OUTPUT: usize = 256 * 1024 * 1024;

const HEX: &[u8; 16] = b"0123456789abcdef";

/// Writes the whole document with the given profile.
pub(crate) fn write_document(doc: &SaveDoc, profile: &Profile) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    if profile.bom {
        out.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
    }
    write_value(doc, doc.root(), profile, &mut out)?;
    if profile.trailing_newline {
        newline(profile, &mut out);
    }
    Ok(out)
}

/// Writes the value at `id` (and its children) with `profile`, appending to `out`.
pub(crate) fn write_value(
    doc: &SaveDoc,
    id: NodeId,
    profile: &Profile,
    out: &mut Vec<u8>,
) -> Result<()> {
    write_tree(doc, id, profile, out)
}

fn unwritable(message: impl Into<String>) -> Error {
    Error::new(JSON_UNWRITABLE, message)
}

enum Frame<'a> {
    Array {
        id: NodeId,
        items: &'a [NodeId],
        next: usize,
    },
    Object {
        id: NodeId,
        entries: &'a [(String, NodeId)],
        next: usize,
    },
}

fn write_tree(doc: &SaveDoc, root: NodeId, profile: &Profile, out: &mut Vec<u8>) -> Result<()> {
    let mut stack: Vec<Frame<'_>> = Vec::new();
    // Containers on the current path; a containment cycle cannot be written.
    let mut open: HashSet<NodeId> = HashSet::new();
    let mut pending = Some(root);
    loop {
        if out.len() > MAX_OUTPUT {
            return Err(Error::new(
                codes::CORE_LIMIT_DECODED,
                format!("the written JSON is larger than {MAX_OUTPUT} bytes"),
            ));
        }
        if let Some(id) = pending.take() {
            let node = doc
                .get(id)
                .ok_or_else(|| unwritable(format!("node {} does not exist", id.index())))?;
            match &node.value {
                Value::Array(items) => {
                    if items.is_empty() {
                        write_empty(profile, out, b'[', b']');
                    } else {
                        enter(&mut open, id)?;
                        out.push(b'[');
                        stack.push(Frame::Array {
                            id,
                            items: items.as_slice(),
                            next: 0,
                        });
                    }
                }
                Value::Object(entries) => {
                    if entries.is_empty() {
                        write_empty(profile, out, b'{', b'}');
                    } else {
                        enter(&mut open, id)?;
                        out.push(b'{');
                        stack.push(Frame::Object {
                            id,
                            entries: entries.as_slice(),
                            next: 0,
                        });
                    }
                }
                _ => write_scalar(node, out)?,
            }
        }

        let level = stack.len();
        match stack.last_mut() {
            None => return Ok(()),
            Some(Frame::Array { id, items, next }) => {
                if let Some(child) = items.get(*next) {
                    separator(profile, out, *next == 0, level)?;
                    *next += 1;
                    pending = Some(*child);
                } else {
                    let id = *id;
                    stack.pop();
                    open.remove(&id);
                    close(profile, out, stack.len(), b']')?;
                }
            }
            Some(Frame::Object { id, entries, next }) => {
                if let Some((key, child)) = entries.get(*next) {
                    separator(profile, out, *next == 0, level)?;
                    write_string(out, key);
                    out.push(b':');
                    if profile.colon_space {
                        out.push(b' ');
                    }
                    *next += 1;
                    pending = Some(*child);
                } else {
                    let id = *id;
                    stack.pop();
                    open.remove(&id);
                    close(profile, out, stack.len(), b'}')?;
                }
            }
        }
    }
}

fn enter(open: &mut HashSet<NodeId>, id: NodeId) -> Result<()> {
    if open.insert(id) {
        Ok(())
    } else {
        Err(unwritable(format!(
            "node {} contains itself; JSON cannot hold a cycle",
            id.index()
        )))
    }
}

fn write_empty(profile: &Profile, out: &mut Vec<u8>, open: u8, close: u8) {
    out.push(open);
    if profile.empty_spaced {
        out.push(b' ');
    }
    out.push(close);
}

fn newline(profile: &Profile, out: &mut Vec<u8>) {
    if profile.crlf {
        out.push(b'\r');
    }
    out.push(b'\n');
}

fn write_indent(profile: &Profile, out: &mut Vec<u8>, level: usize) -> Result<()> {
    let (byte, per_level) = match profile.indent {
        Indent::None => return Ok(()),
        Indent::Spaces(count) => (b' ', count),
        Indent::Tab => (b'\t', 1),
    };
    let count = per_level.saturating_mul(level);
    if count > MAX_OUTPUT {
        return Err(Error::new(
            codes::CORE_LIMIT_DECODED,
            "the indentation of the written JSON is too large",
        ));
    }
    out.resize(out.len() + count, byte);
    Ok(())
}

/// Separator before a child: nothing/comma in compact mode, newline + indent in pretty mode.
fn separator(profile: &Profile, out: &mut Vec<u8>, first: bool, level: usize) -> Result<()> {
    if profile.indent == Indent::None {
        if !first {
            out.push(b',');
            if profile.comma_space {
                out.push(b' ');
            }
        }
        return Ok(());
    }
    if !first {
        out.push(b',');
    }
    newline(profile, out);
    write_indent(profile, out, level)
}

fn close(profile: &Profile, out: &mut Vec<u8>, level: usize, bracket: u8) -> Result<()> {
    if profile.indent != Indent::None {
        newline(profile, out);
        write_indent(profile, out, level)?;
    }
    out.push(bracket);
    Ok(())
}

fn write_scalar(node: &saveprism_core::Node, out: &mut Vec<u8>) -> Result<()> {
    if let Some(lexeme) = &node.lexeme {
        out.extend_from_slice(lexeme.as_bytes());
        return Ok(());
    }
    match &node.value {
        Value::Null => out.extend_from_slice(b"null"),
        Value::Bool(true) => out.extend_from_slice(b"true"),
        Value::Bool(false) => out.extend_from_slice(b"false"),
        Value::Int { v, .. } => out.extend_from_slice(v.to_string().as_bytes()),
        Value::Float { v, .. } => {
            let hinted = node
                .type_hint
                .as_ref()
                .is_some_and(|hint| hint.as_str() == "JSON:float");
            match float_text(*v, hinted) {
                Some(text) => out.extend_from_slice(text.as_bytes()),
                None => {
                    return Err(unwritable(format!(
                        "the number {v} has no JSON form (NaN and infinity are not allowed)"
                    )));
                }
            }
        }
        Value::Str(text) => write_string(out, text),
        Value::Bytes(_) => return Err(unwritable("JSON cannot hold raw bytes")),
        Value::Map(_) => return Err(unwritable("JSON cannot hold a map with non-text keys")),
        Value::Ref(_) => return Err(unwritable("JSON cannot hold a reference")),
        Value::Array(_) | Value::Object(_) => {
            return Err(unwritable("internal error: container written as scalar"));
        }
    }
    Ok(())
}

/// Canonical (JavaScript `JSON.stringify`) text of a float, or `None` for NaN and infinity.
/// With `hinted` (`JSON:float`) a whole number gets a `.0` so it reads back as a float.
pub(crate) fn float_text(v: f64, hinted: bool) -> Option<String> {
    if !v.is_finite() {
        return None;
    }
    let abs = v.abs();
    if abs == 0.0 {
        let mut text = String::from(if v.is_sign_negative() { "-0" } else { "0" });
        if hinted {
            text.push_str(".0");
        }
        return Some(text);
    }
    if (1e-6..1e21).contains(&abs) {
        let mut text = format!("{v}");
        if hinted && !text.contains('.') {
            text.push_str(".0");
        }
        return Some(text);
    }
    let text = format!("{v:e}");
    Some(match text.split_once('e') {
        Some((mantissa, exponent)) if !exponent.starts_with('-') => {
            format!("{mantissa}e+{exponent}")
        }
        _ => text,
    })
}

/// Writes `text` as a JSON string with JavaScript escaping: `"`, `\`, control characters
/// (`\b \f \n \r \t`, others `\u00xx` lower case); `/` and non-ASCII stay raw.
pub(crate) fn write_string(out: &mut Vec<u8>, text: &str) {
    out.push(b'"');
    let bytes = text.as_bytes();
    let mut run = 0;
    for (index, &byte) in bytes.iter().enumerate() {
        let escape: &[u8] = match byte {
            b'"' => b"\\\"",
            b'\\' => b"\\\\",
            0x08 => b"\\b",
            0x0C => b"\\f",
            b'\n' => b"\\n",
            b'\r' => b"\\r",
            b'\t' => b"\\t",
            0x00..=0x1F => {
                out.extend_from_slice(bytes.get(run..index).unwrap_or_default());
                out.extend_from_slice(b"\\u00");
                out.push(HEX.get(usize::from(byte >> 4)).copied().unwrap_or(b'0'));
                out.push(HEX.get(usize::from(byte & 0xF)).copied().unwrap_or(b'0'));
                run = index + 1;
                continue;
            }
            _ => continue,
        };
        out.extend_from_slice(bytes.get(run..index).unwrap_or_default());
        out.extend_from_slice(escape);
        run = index + 1;
    }
    out.extend_from_slice(bytes.get(run..).unwrap_or_default());
    out.push(b'"');
}
