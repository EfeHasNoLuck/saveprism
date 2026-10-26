use saveprism_core::limits::len_u64;
use saveprism_core::{Error, Params, Result, SaveDoc, Value};

use super::{GFF_WRITE, HINT_PREFIX, P_ORIGINAL};

fn write_error(message: impl Into<String>) -> Error {
    Error::new(GFF_WRITE, message)
}

fn text_bytes(value: &str, latin1: bool) -> Result<Vec<u8>> {
    if latin1 {
        let mut out = Vec::with_capacity(value.len());
        for character in value.chars() {
            let code = u32::from(character);
            let Ok(byte) = u8::try_from(code) else {
                return Err(write_error(
                    "the string contains a character the file's single-byte text cannot hold",
                ));
            };
            out.push(byte);
        }
        Ok(out)
    } else {
        Ok(value.as_bytes().to_vec())
    }
}

fn strip_hint(hint: &str) -> (&str, bool) {
    match hint.strip_prefix(HINT_PREFIX) {
        Some(rest) => match rest.strip_suffix(":latin1") {
            Some(base) => (base, true),
            None => (rest, false),
        },
        None => (hint, false),
    }
}

fn encode_int(value: i128, min: i128, max: i128, bytes: usize) -> Result<Vec<u8>> {
    if !(min..=max).contains(&value) {
        return Err(write_error(format!(
            "the value {value} does not fit this GFF field"
        )));
    }
    let raw = value.to_le_bytes();
    raw.get(..bytes)
        .map(<[u8]>::to_vec)
        .ok_or_else(|| write_error("the field width is invalid"))
}

fn replacement(hint: &str, value: &Value) -> Result<Vec<u8>> {
    let (kind, latin1) = strip_hint(hint);
    match (kind, value) {
        ("byte", Value::Int { v, .. }) => encode_int(*v, 0, i128::from(u8::MAX), 1),
        ("char", Value::Int { v, .. }) => {
            encode_int(*v, i128::from(i8::MIN), i128::from(i8::MAX), 1)
        }
        ("word", Value::Int { v, .. }) => encode_int(*v, 0, i128::from(u16::MAX), 2),
        ("short", Value::Int { v, .. }) => {
            encode_int(*v, i128::from(i16::MIN), i128::from(i16::MAX), 2)
        }
        ("dword", Value::Int { v, .. }) => encode_int(*v, 0, i128::from(u32::MAX), 4),
        ("int", Value::Int { v, .. }) => {
            encode_int(*v, i128::from(i32::MIN), i128::from(i32::MAX), 4)
        }
        ("dword64", Value::Int { v, .. }) => encode_int(*v, 0, i128::from(u64::MAX), 8),
        ("int64", Value::Int { v, .. }) => {
            encode_int(*v, i128::from(i64::MIN), i128::from(i64::MAX), 8)
        }
        ("float", Value::Float { v, .. }) => Ok((*v as f32).to_bits().to_le_bytes().to_vec()),
        ("double", Value::Float { v, .. }) => Ok(v.to_bits().to_le_bytes().to_vec()),
        ("cexostring" | "resref" | "locstring-text", Value::Str(text)) => text_bytes(text, latin1),
        _ => Err(write_error(format!(
            "the node value does not match its GFF type \"{kind}\""
        ))),
    }
}

fn editable(hint: &str) -> bool {
    let (kind, _) = strip_hint(hint);
    matches!(
        kind,
        "byte"
            | "char"
            | "word"
            | "short"
            | "dword"
            | "int"
            | "dword64"
            | "int64"
            | "float"
            | "double"
            | "cexostring"
            | "resref"
            | "locstring-text"
    )
}

pub(super) fn encode(doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
    let original = params
        .opt_bytes(P_ORIGINAL)?
        .ok_or_else(|| write_error("the original GFF bytes are missing"))?;
    let mut out = original.to_vec();
    let mut stack = vec![doc.root()];
    while let Some(id) = stack.pop() {
        let node = doc
            .get(id)
            .ok_or_else(|| write_error("the document contains an invalid node reference"))?;
        match &node.value {
            Value::Array(items) => stack.extend(items.iter().copied()),
            Value::Object(entries) => stack.extend(entries.iter().map(|(_, child)| *child)),
            Value::Map(entries) => {
                stack.extend(entries.iter().flat_map(|(key, value)| [*key, *value]))
            }
            _ => {}
        }
        let Some(hint) = node.type_hint.as_ref().map(|hint| hint.as_str()) else {
            continue;
        };
        if !editable(hint) {
            continue;
        }
        let Some(span) = node.span else {
            continue;
        };
        let start =
            usize::try_from(span.start).map_err(|_| write_error("a GFF field span is invalid"))?;
        let end =
            usize::try_from(span.end).map_err(|_| write_error("a GFF field span is invalid"))?;
        let raw = original
            .get(start..end)
            .ok_or_else(|| write_error("a GFF field span lies beyond the original bytes"))?;
        let new =
            replacement(hint, &node.value).map_err(|error| error.with_offset(len_u64(start)))?;
        if new == raw {
            continue;
        }
        if new.len() != raw.len() {
            return Err(write_error(format!(
                "editing \"{}\" changes its byte length ({} to {}); GFF strings can only keep their length",
                hint,
                raw.len(),
                new.len()
            ))
            .with_offset(len_u64(start)));
        }
        let target = out
            .get_mut(start..end)
            .ok_or_else(|| write_error("a GFF field span lies beyond the original bytes"))?;
        target.copy_from_slice(&new);
    }
    Ok(out)
}
