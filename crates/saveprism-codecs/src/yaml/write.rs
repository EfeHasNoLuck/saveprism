//! Rendering edited YAML scalars in the style they had.

use saveprism_core::{Error, NodeFlags, Result, SaveDoc, TypeHint, Value};

use super::YAML_WRITE;
use crate::text_common::{apply_patches, format_float, span_range};

/// Applies the edited leaves to `original`.
pub(crate) fn encode(doc: &SaveDoc, original: &[u8]) -> Result<Vec<u8>> {
    let mut patches = Vec::new();
    let mut stack = vec![doc.root()];
    let mut visited = 0usize;
    while let Some(id) = stack.pop() {
        visited = visited.saturating_add(1);
        if visited > doc.len().saturating_add(1) {
            return Err(Error::new(YAML_WRITE, "cyclic YAML tree"));
        }
        let Some(node) = doc.get(id) else {
            return Err(Error::new(YAML_WRITE, "invalid node in the YAML tree"));
        };
        stack.extend(doc.children(id));
        if node.flags.contains(NodeFlags::READ_ONLY) || node.lexeme.is_some() {
            continue;
        }
        let Some(hint) = node.type_hint.as_ref().map(TypeHint::as_str) else {
            continue;
        };
        let Some(rest) = hint.strip_prefix("YAML:") else {
            continue;
        };
        let Some((style, kind)) = rest.split_once(':') else {
            continue;
        };
        let rendered = match (kind, &node.value) {
            ("string", Value::Str(text)) if style == "literal" => text.clone(),
            ("string", Value::Str(text)) => render_string(text, style),
            ("int", Value::Int { v, .. }) => v.to_string(),
            ("float", Value::Float { v, .. }) => render_float(*v),
            ("bool", Value::Bool(value)) => value.to_string(),
            _ => continue,
        };
        let (start, end) = span_range(node, YAML_WRITE)?;
        let old = original
            .get(start..end)
            .ok_or_else(|| Error::new(YAML_WRITE, "token span outside the original text"))?;
        let replacement = if style == "literal" {
            render_literal(&rendered, original, start, old.last() == Some(&b'\n'))
        } else {
            rendered
        };
        if old != replacement.as_bytes() {
            patches.push((start, end, replacement.into_bytes()));
        }
    }
    apply_patches(original, YAML_WRITE, &mut patches)
}

fn render_string(text: &str, style: &str) -> String {
    match style {
        "single" if !text.chars().any(|c| c.is_control()) => {
            format!("'{}'", text.replace('\'', "''"))
        }
        "plain" if is_safe_plain(text) => text.to_owned(),
        _ => double_quoted(text),
    }
}

fn is_safe_plain(text: &str) -> bool {
    if text.is_empty()
        || text.trim() != text
        || text.contains('\n')
        || text.contains('\r')
        || text.contains('\t')
        || text.contains(": ")
        || text.contains(" #")
    {
        return false;
    }
    if text.starts_with([
        '-', '?', ':', ',', '[', ']', '{', '}', '#', '&', '*', '!', '|', '>', '\'', '"', '%', '@',
        '`',
    ]) {
        return false;
    }
    !matches!(
        text,
        "true" | "True" | "TRUE" | "false" | "False" | "FALSE" | "null" | "Null" | "NULL" | "~"
    ) && parse_as_number(text).is_none()
}

fn parse_as_number(text: &str) -> Option<()> {
    if text.bytes().any(|byte| byte.is_ascii_digit()) {
        let clean: String = text.chars().filter(|c| *c != '_').collect();
        if clean.parse::<i128>().is_ok() || clean.parse::<f64>().is_ok() {
            return Some(());
        }
        if clean
            .strip_prefix("0x")
            .is_some_and(|rest| i128::from_str_radix(rest, 16).is_ok())
        {
            return Some(());
        }
    }
    None
}

fn render_float(value: f64) -> String {
    if value.is_nan() {
        ".nan".to_owned()
    } else if value == f64::INFINITY {
        ".inf".to_owned()
    } else if value == f64::NEG_INFINITY {
        "-.inf".to_owned()
    } else {
        format_float(value)
    }
}

fn double_quoted(text: &str) -> String {
    let mut out = String::with_capacity(text.len().saturating_add(2));
    out.push('"');
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\0' => out.push_str("\\0"),
            '\u{07}' => out.push_str("\\a"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0b}' => out.push_str("\\v"),
            '\u{0c}' => out.push_str("\\f"),
            character if character < ' ' => {
                out.push_str(&format!("\\u{:04x}", u32::from(character)));
            }
            character => out.push(character),
        }
    }
    out.push('"');
    out
}

/// Renders a literal block body. The span starts at the first content character, so the indent of
/// the following lines is taken from the original line prefix.
fn render_literal(value: &str, original: &[u8], start: usize, had_newline: bool) -> String {
    let indent = line_indent(original, start);
    let mut parts: Vec<&str> = value.split('\n').collect();
    let trailing_newline = value.ends_with('\n') || had_newline;
    if value.ends_with('\n') {
        parts.pop();
    }
    let mut out = String::with_capacity(value.len().saturating_add(indent.len()));
    for (index, part) in parts.iter().enumerate() {
        if index > 0 {
            out.push('\n');
            out.push_str(&indent);
        }
        out.push_str(part);
    }
    if trailing_newline {
        out.push('\n');
    }
    out
}

fn line_indent(original: &[u8], start: usize) -> String {
    let mut line_start = start;
    while line_start > 0 && original.get(line_start - 1) != Some(&b'\n') {
        line_start -= 1;
    }
    let prefix = original.get(line_start..start).unwrap_or_default();
    if prefix.iter().all(u8::is_ascii_whitespace) {
        String::from_utf8_lossy(prefix).into_owned()
    } else {
        String::new()
    }
}
