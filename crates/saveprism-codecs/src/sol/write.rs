use std::collections::HashSet;

use saveprism_core::{Error, FloatWidth, ParamValue, Params, SaveDoc, Value};

use super::{ORIGINAL, PREFIX, SOL_WRITE, STRING_NO_EMPTY};

pub(super) fn encode(doc: &SaveDoc, params: &Params) -> saveprism_core::Result<Vec<u8>> {
    let original = match params.get(ORIGINAL) {
        Some(ParamValue::Bytes(bytes)) => bytes,
        _ => return Err(Error::new(SOL_WRITE, "original LSO bytes are missing")),
    };
    if original.len() < 6 {
        return Err(Error::new(SOL_WRITE, "original LSO header is truncated"));
    }
    let mut patches = Vec::new();
    let no_empty: HashSet<usize> = match params.get(STRING_NO_EMPTY) {
        Some(ParamValue::List(items)) => items
            .iter()
            .filter_map(|item| match item {
                ParamValue::Int(index) => usize::try_from(*index).ok(),
                _ => None,
            })
            .collect(),
        _ => HashSet::new(),
    };
    let mut stack = vec![doc.root()];
    while let Some(id) = stack.pop() {
        let node = doc
            .get(id)
            .ok_or_else(|| Error::new(SOL_WRITE, "document contains an invalid node reference"))?;
        match &node.value {
            Value::Array(items) => stack.extend(items.iter().copied()),
            Value::Object(entries) => stack.extend(entries.iter().map(|(_, child)| *child)),
            Value::Map(entries) => {
                stack.extend(entries.iter().flat_map(|(key, value)| [*key, *value]))
            }
            _ => {}
        }
        let Some(hint) = node.type_hint.as_ref().map(|h| h.as_str()) else {
            continue;
        };
        if !hint.starts_with(PREFIX) {
            continue;
        }
        let Some(span) = node.span else { continue };
        let start = usize::try_from(span.start)
            .map_err(|_| Error::new(SOL_WRITE, "scalar span is invalid"))?;
        let end = usize::try_from(span.end)
            .map_err(|_| Error::new(SOL_WRITE, "scalar span is invalid"))?;
        let raw = original
            .get(start..end)
            .ok_or_else(|| Error::new(SOL_WRITE, "scalar span exceeds original bytes"))?;
        let replacement = match (hint, &node.value) {
            (
                "SOL:amf0-number",
                Value::Float {
                    v,
                    width: FloatWidth::F64,
                },
            ) => {
                if raw.len() != 9 {
                    return Err(Error::new(SOL_WRITE, "invalid AMF0 number span"));
                }
                let old = u64::from_be_bytes(
                    raw.get(1..9)
                        .ok_or_else(|| Error::new(SOL_WRITE, "invalid AMF0 number span"))?
                        .try_into()
                        .map_err(|_| Error::new(SOL_WRITE, "invalid AMF0 number span"))?,
                );
                if v.to_bits() == old {
                    None
                } else {
                    Some([vec![0], v.to_bits().to_be_bytes().to_vec()].concat())
                }
            }
            (
                "SOL:amf3-double",
                Value::Float {
                    v,
                    width: FloatWidth::F64,
                },
            ) => {
                if raw.len() != 9 {
                    return Err(Error::new(SOL_WRITE, "invalid AMF3 double span"));
                }
                let old = u64::from_be_bytes(
                    raw.get(1..9)
                        .ok_or_else(|| Error::new(SOL_WRITE, "invalid AMF3 double span"))?
                        .try_into()
                        .map_err(|_| Error::new(SOL_WRITE, "invalid AMF3 double span"))?,
                );
                if v.to_bits() == old {
                    None
                } else {
                    Some([vec![5], v.to_bits().to_be_bytes().to_vec()].concat())
                }
            }
            ("SOL:amf3-vector-i32", Value::Int { v, .. }) => {
                let value = i32::try_from(*v)
                    .map_err(|_| Error::new(SOL_WRITE, "signed vector value is out of range"))?;
                if raw.len() != 4 {
                    return Err(Error::new(SOL_WRITE, "invalid AMF3 vector value span"));
                }
                let old = i32::from_be_bytes(
                    raw.try_into()
                        .map_err(|_| Error::new(SOL_WRITE, "invalid AMF3 vector value span"))?,
                );
                if old == value {
                    None
                } else {
                    Some(value.to_be_bytes().to_vec())
                }
            }
            ("SOL:amf3-vector-u32", Value::Int { v, .. }) => {
                let value = u32::try_from(*v)
                    .map_err(|_| Error::new(SOL_WRITE, "unsigned vector value is out of range"))?;
                if raw.len() != 4 {
                    return Err(Error::new(SOL_WRITE, "invalid AMF3 vector value span"));
                }
                let old = u32::from_be_bytes(
                    raw.try_into()
                        .map_err(|_| Error::new(SOL_WRITE, "invalid AMF3 vector value span"))?,
                );
                if old == value {
                    None
                } else {
                    Some(value.to_be_bytes().to_vec())
                }
            }
            (
                "SOL:amf3-vector-double",
                Value::Float {
                    v,
                    width: FloatWidth::F64,
                },
            ) => {
                if raw.len() != 8 {
                    return Err(Error::new(SOL_WRITE, "invalid AMF3 vector value span"));
                }
                let old = u64::from_be_bytes(
                    raw.try_into()
                        .map_err(|_| Error::new(SOL_WRITE, "invalid AMF3 vector value span"))?,
                );
                if v.to_bits() == old {
                    None
                } else {
                    Some(v.to_bits().to_be_bytes().to_vec())
                }
            }
            ("SOL:amf0-bool", Value::Bool(value)) => {
                if raw.len() != 2 {
                    return Err(Error::new(SOL_WRITE, "invalid AMF0 boolean span"));
                }
                if raw.get(1).copied() == Some(u8::from(*value)) {
                    None
                } else {
                    Some(vec![1, u8::from(*value)])
                }
            }
            ("SOL:amf3-bool", Value::Bool(value)) => {
                if raw.len() != 1 {
                    return Err(Error::new(SOL_WRITE, "invalid AMF3 boolean span"));
                }
                let want = if *value { 3 } else { 2 };
                if raw.first().copied() == Some(want) {
                    None
                } else {
                    Some(vec![want])
                }
            }
            ("SOL:amf3-int", Value::Int { v, .. }) => {
                if !(-(1 << 28)..=(1 << 28) - 1).contains(v) {
                    return Err(Error::new(
                        SOL_WRITE,
                        "AMF3 integer edit is outside the signed 29-bit range",
                    ));
                }
                let old = raw
                    .get(1..)
                    .ok_or_else(|| Error::new(SOL_WRITE, "invalid AMF3 integer span"))?;
                let old_value = ((decode_u29(old)? << 3) as i32 >> 3) as i128;
                if old_value == *v {
                    None
                } else {
                    Some([vec![4], encode_u29((*v as i32 as u32) & 0x1fff_ffff)].concat())
                }
            }
            ("SOL:amf0-string", Value::Str(text)) => patch_amf0_string(raw, text, false)?,
            ("SOL:amf0-long-string", Value::Str(text)) => patch_amf0_string(raw, text, true)?,
            ("SOL:amf3-string", Value::Str(text)) => {
                if no_empty.contains(&id.index()) && text.is_empty() {
                    return Err(Error::new(
                        SOL_WRITE,
                        "this AMF3 string cannot be changed to empty because later string references depend on its table slot",
                    ));
                }
                patch_amf3_string(raw, text)?
            }
            ("SOL:amf0-number" | "SOL:amf3-double" | "SOL:amf3-vector-double", _)
            | ("SOL:amf0-bool" | "SOL:amf3-bool", _)
            | ("SOL:amf3-int" | "SOL:amf3-vector-i32" | "SOL:amf3-vector-u32", _)
            | ("SOL:amf0-string" | "SOL:amf0-long-string" | "SOL:amf3-string", _) => {
                return Err(Error::new(
                    SOL_WRITE,
                    format!("value for {hint} has an incompatible type"),
                ));
            }
            _ => None,
        };
        if let Some(bytes) = replacement {
            patches.push((start, end, bytes));
        }
    }
    patches.sort_by_key(|patch| patch.0);
    let mut out = Vec::with_capacity(original.len());
    let mut at = 0;
    for (start, end, bytes) in patches {
        if start < at || end < start {
            return Err(Error::new(SOL_WRITE, "overlapping scalar spans"));
        }
        out.extend_from_slice(
            original
                .get(at..start)
                .ok_or_else(|| Error::new(SOL_WRITE, "invalid scalar span"))?,
        );
        out.extend_from_slice(&bytes);
        at = end;
    }
    out.extend_from_slice(
        original
            .get(at..)
            .ok_or_else(|| Error::new(SOL_WRITE, "invalid scalar span"))?,
    );
    let tail_len = u32::try_from(out.len().saturating_sub(6))
        .map_err(|_| Error::new(SOL_WRITE, "LSO file is too large"))?;
    out.get_mut(2..6)
        .ok_or_else(|| Error::new(SOL_WRITE, "LSO header is truncated"))?
        .copy_from_slice(&tail_len.to_be_bytes());
    Ok(out)
}

fn patch_amf0_string(
    raw: &[u8],
    text: &str,
    long: bool,
) -> saveprism_core::Result<Option<Vec<u8>>> {
    let len_width = if long { 4 } else { 2 };
    if raw.len() < 1 + len_width || raw.first().copied() != Some(if long { 12 } else { 2 }) {
        return Err(Error::new(SOL_WRITE, "invalid AMF0 string span"));
    }
    let old_len = if long {
        u32::from_be_bytes(
            raw.get(1..5)
                .ok_or_else(|| Error::new(SOL_WRITE, "invalid AMF0 string span"))?
                .try_into()
                .map_err(|_| Error::new(SOL_WRITE, "invalid AMF0 string span"))?,
        ) as usize
    } else {
        u16::from_be_bytes(
            raw.get(1..3)
                .ok_or_else(|| Error::new(SOL_WRITE, "invalid AMF0 string span"))?
                .try_into()
                .map_err(|_| Error::new(SOL_WRITE, "invalid AMF0 string span"))?,
        ) as usize
    };
    if raw
        .get(1 + len_width..)
        .is_some_and(|bytes| bytes == text.as_bytes())
        && old_len == text.len()
    {
        return Ok(None);
    }
    let len = if long {
        u32::try_from(text.len())
            .map_err(|_| Error::new(SOL_WRITE, "AMF0 long string is too long"))?
    } else {
        u32::from(
            u16::try_from(text.len())
                .map_err(|_| Error::new(SOL_WRITE, "AMF0 short string is too long"))?,
        )
    };
    let mut out = vec![if long { 12 } else { 2 }];
    let len_bytes = if long {
        len.to_be_bytes().to_vec()
    } else {
        u16::try_from(len)
            .map_err(|_| Error::new(SOL_WRITE, "AMF0 short string is too long"))?
            .to_be_bytes()
            .to_vec()
    };
    out.extend_from_slice(&len_bytes);
    out.extend_from_slice(text.as_bytes());
    Ok(Some(out))
}

fn patch_amf3_string(raw: &[u8], text: &str) -> saveprism_core::Result<Option<Vec<u8>>> {
    if raw.first().copied() != Some(6) {
        return Err(Error::new(SOL_WRITE, "invalid AMF3 string span"));
    }
    let (old_len, used) = decode_u29_with_len(
        raw.get(1..)
            .ok_or_else(|| Error::new(SOL_WRITE, "invalid AMF3 string span"))?,
    )?;
    let old_len = (old_len >> 1) as usize;
    if raw
        .get(1 + used..)
        .is_some_and(|bytes| bytes == text.as_bytes())
        && old_len == text.len()
    {
        return Ok(None);
    }
    if text.len() > 0x0fff_ffff {
        return Err(Error::new(SOL_WRITE, "AMF3 string is too long"));
    }
    let mut out = vec![6];
    out.extend_from_slice(&encode_u29(((text.len() as u32) << 1) | 1));
    out.extend_from_slice(text.as_bytes());
    Ok(Some(out))
}

fn decode_u29(bytes: &[u8]) -> saveprism_core::Result<u32> {
    decode_u29_with_len(bytes).map(|(v, _)| v)
}
fn decode_u29_with_len(bytes: &[u8]) -> saveprism_core::Result<(u32, usize)> {
    let mut value = 0u32;
    for (i, byte) in bytes.iter().copied().take(4).enumerate() {
        if i < 3 {
            value = (value << 7) | u32::from(byte & 0x7f);
            if byte & 0x80 == 0 {
                return Ok((value, i + 1));
            }
        } else {
            value = (value << 8) | u32::from(byte);
            return Ok((value, 4));
        }
    }
    Err(Error::new(SOL_WRITE, "truncated AMF3 U29"))
}
fn encode_u29(value: u32) -> Vec<u8> {
    if value < 0x80 {
        vec![value as u8]
    } else if value < 0x4000 {
        vec![((value >> 7) as u8) | 0x80, (value & 0x7f) as u8]
    } else if value < 0x20_0000 {
        vec![
            ((value >> 14) as u8) | 0x80,
            (((value >> 7) as u8) & 0x7f) | 0x80,
            (value & 0x7f) as u8,
        ]
    } else {
        vec![
            ((value >> 22) as u8) | 0x80,
            (((value >> 15) as u8) & 0x7f) | 0x80,
            (((value >> 8) as u8) & 0x7f) | 0x80,
            value as u8,
        ]
    }
}
