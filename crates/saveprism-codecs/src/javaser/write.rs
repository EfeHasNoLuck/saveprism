use super::{JAVA_WRITE, ORIGINAL, PREFIX};
use saveprism_core::{Error, FloatWidth, IntWidth, NodeFlags, ParamValue, Params, SaveDoc, Value};

pub(super) fn encode(doc: &SaveDoc, params: &Params) -> saveprism_core::Result<Vec<u8>> {
    let original = match params.get(ORIGINAL) {
        Some(ParamValue::Bytes(bytes)) => bytes,
        _ => return Err(Error::new(JAVA_WRITE, "original stream bytes are missing")),
    };
    let mut patches: Vec<(usize, usize, Vec<u8>)> = Vec::new();
    let mut stack = vec![doc.root()];
    while let Some(id) = stack.pop() {
        let node = doc
            .get(id)
            .ok_or_else(|| Error::new(JAVA_WRITE, "invalid node reference"))?;
        match &node.value {
            Value::Array(a) => stack.extend(a.iter().copied()),
            Value::Object(o) => stack.extend(o.iter().map(|(_, n)| *n)),
            Value::Map(m) => stack.extend(m.iter().flat_map(|(k, v)| [*k, *v])),
            _ => {}
        }
        if !node
            .type_hint
            .as_ref()
            .is_some_and(|h| h.as_str().starts_with(PREFIX))
        {
            continue;
        }
        let Some(span) = node.span else { continue };
        let a = usize::try_from(span.start).map_err(|_| Error::new(JAVA_WRITE, "invalid span"))?;
        let b = usize::try_from(span.end).map_err(|_| Error::new(JAVA_WRITE, "invalid span"))?;
        let raw = original
            .get(a..b)
            .ok_or_else(|| Error::new(JAVA_WRITE, "span exceeds original stream"))?;
        let replacement = match (
            &node.value,
            node.type_hint.as_ref().map(|h| h.as_str()).unwrap_or(""),
        ) {
            (Value::Int { v, width }, hint) => Some(integer_patch(*v, *width, hint, raw)?),
            (Value::Bool(v), "JAVASER:boolean") => {
                if raw.len() != 1 {
                    return Err(Error::new(JAVA_WRITE, "invalid boolean span"));
                }
                Some(vec![u8::from(*v)])
            }
            (
                Value::Float {
                    v,
                    width: FloatWidth::F32,
                },
                "JAVASER:float",
            ) => {
                if raw.len() != 4 {
                    return Err(Error::new(JAVA_WRITE, "invalid float span"));
                }
                Some((*v as f32).to_bits().to_be_bytes().to_vec())
            }
            (
                Value::Float {
                    v,
                    width: FloatWidth::F64,
                },
                "JAVASER:double",
            ) => {
                if raw.len() != 8 {
                    return Err(Error::new(JAVA_WRITE, "invalid double span"));
                }
                Some(v.to_bits().to_be_bytes().to_vec())
            }
            (Value::Str(s), "JAVASER:string") if node.lexeme.as_deref() != Some(s.as_str()) => {
                Some(string_patch(s, raw)?)
            }
            _ => None,
        };
        if let Some(bytes) = replacement
            && bytes.as_slice() != raw
        {
            patches.push((a, b, bytes));
        }
        if node.flags.contains(NodeFlags::READ_ONLY) && patches.last().is_some_and(|p| p.0 == a) {
            return Err(Error::new(JAVA_WRITE, "attempted edit of a read-only node"));
        }
    }
    patches.sort_by_key(|p| std::cmp::Reverse(p.0));
    let mut out = original.clone();
    let mut last = usize::MAX;
    for (a, b, repl) in patches {
        if b > last {
            return Err(Error::new(JAVA_WRITE, "overlapping Java value spans"));
        }
        out.splice(a..b, repl);
        last = a;
    }
    Ok(out)
}
fn integer_patch(
    v: i128,
    width: IntWidth,
    hint: &str,
    raw: &[u8],
) -> saveprism_core::Result<Vec<u8>> {
    let out = match (hint, width) {
        ("JAVASER:byte", IntWidth::I8) => i8::try_from(v).ok().map(|n| n.to_be_bytes().to_vec()),
        ("JAVASER:short", IntWidth::I16) => i16::try_from(v).ok().map(|n| n.to_be_bytes().to_vec()),
        ("JAVASER:char", IntWidth::U16) => u16::try_from(v).ok().map(|n| n.to_be_bytes().to_vec()),
        ("JAVASER:int", IntWidth::I32) => i32::try_from(v).ok().map(|n| n.to_be_bytes().to_vec()),
        ("JAVASER:long", IntWidth::I64) => i64::try_from(v).ok().map(|n| n.to_be_bytes().to_vec()),
        _ => None,
    }
    .ok_or_else(|| {
        Error::new(
            JAVA_WRITE,
            "integer edit is out of range or has the wrong width",
        )
    })?;
    if out.len() != raw.len() {
        return Err(Error::new(JAVA_WRITE, "integer span has the wrong width"));
    }
    Ok(out)
}
fn string_patch(text: &str, raw: &[u8]) -> saveprism_core::Result<Vec<u8>> {
    if raw.len() < 3 {
        return Err(Error::new(JAVA_WRITE, "string span is truncated"));
    }
    let encoded = encode_mutf8(text);
    let token = *raw
        .first()
        .ok_or_else(|| Error::new(JAVA_WRITE, "string span is empty"))?;
    let mut out = Vec::new();
    if encoded.len() <= u16::MAX as usize {
        out.push(0x74);
        out.extend_from_slice(&(encoded.len() as u16).to_be_bytes());
    } else {
        out.push(0x7c);
        out.extend_from_slice(&(encoded.len() as u64).to_be_bytes());
    }
    out.extend(encoded);
    if !matches!(token, 0x74 | 0x7c) {
        return Err(Error::new(JAVA_WRITE, "string has an invalid token"));
    }
    Ok(out)
}
fn encode_mutf8(text: &str) -> Vec<u8> {
    let mut out = Vec::new();
    for unit in text.encode_utf16() {
        match unit {
            0 => out.extend_from_slice(&[0xc0, 0x80]),
            1..=0x7f => out.push(unit as u8),
            0x80..=0x7ff => {
                out.push(0xc0 | ((unit >> 6) as u8));
                out.push(0x80 | (unit as u8 & 0x3f));
            }
            _ => {
                out.push(0xe0 | ((unit >> 12) as u8));
                out.push(0x80 | ((unit >> 6) as u8 & 0x3f));
                out.push(0x80 | (unit as u8 & 0x3f));
            }
        }
    }
    out
}
