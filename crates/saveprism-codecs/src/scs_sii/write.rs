//! Exact SII writing: only the token of an edited value is replaced; comments, spacing, order and
//! every other byte stay as they were (the `godot/config.rs` span-patch pattern).

use saveprism_core::{Error, NodeFlags, Params, Result, SaveDoc, TypeHint, Value};

use super::{ORIGINAL, SCS_SII_WRITE};

fn write_error(message: impl Into<String>) -> Error {
    Error::new(SCS_SII_WRITE, message)
}

fn render(hint: &str, value: &Value) -> Result<Option<String>> {
    Ok(match (hint, value) {
        ("SCS-SII:bool", Value::Bool(v)) => Some(v.to_string()),
        ("SCS-SII:int", Value::Int { v, .. }) => Some(v.to_string()),
        ("SCS-SII:float", Value::Float { v, .. }) => {
            if !v.is_finite() {
                return Err(write_error("an SII float must be finite"));
            }
            let text = format!("{v}");
            if text.contains('.') || text.contains('e') || text.contains('E') {
                Some(text)
            } else {
                Some(format!("{text}.0"))
            }
        }
        ("SCS-SII:string", Value::Str(text)) => Some(quote(text)),
        ("SCS-SII:token", Value::Str(text)) => {
            if is_plain_token(text) {
                Some(text.clone())
            } else {
                Some(quote(text))
            }
        }
        _ => None,
    })
}

fn quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

fn is_plain_token(text: &str) -> bool {
    if text.is_empty()
        || matches!(text, "true" | "false" | "nil" | "null")
        || text.parse::<f64>().is_ok()
        || text.starts_with('"')
        || text.starts_with('(')
        || text.starts_with('&')
    {
        return false;
    }
    text.bytes().all(|byte| {
        byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-' | b'/' | b'+')
    })
}

/// Encodes `doc` by replacing only the spans of changed values in the original text.
pub(super) fn encode(doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
    let original = params.bytes(ORIGINAL)?;
    let mut patches: Vec<(usize, usize, Vec<u8>)> = Vec::new();
    let mut stack = vec![doc.root()];
    let mut visited = 0_usize;
    while let Some(id) = stack.pop() {
        visited = visited.saturating_add(1);
        if visited > doc.len().saturating_add(1) {
            return Err(write_error("cyclic SII tree"));
        }
        let Some(node) = doc.get(id) else {
            continue;
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
        let Some(rendered) = render(hint, &node.value)? else {
            continue;
        };
        let Some(span) = node.span else {
            continue;
        };
        let start = usize::try_from(span.start)
            .map_err(|_| write_error("the SII value span is out of range"))?;
        let end = usize::try_from(span.end)
            .map_err(|_| write_error("the SII value span is out of range"))?;
        let old = original
            .get(start..end)
            .ok_or_else(|| write_error("the SII value span points outside the file"))?;
        if old != rendered.as_bytes() {
            patches.push((start, end, rendered.into_bytes()));
        }
    }
    patches.sort_by_key(|(start, _, _)| *start);
    for pair in patches.windows(2) {
        if let (Some(a), Some(b)) = (pair.first(), pair.get(1))
            && a.1 > b.0
        {
            return Err(write_error("overlapping SII value spans"));
        }
    }
    let mut out = original.to_vec();
    for (start, end, replacement) in patches.into_iter().rev() {
        out.splice(start..end, replacement);
    }
    Ok(out)
}
