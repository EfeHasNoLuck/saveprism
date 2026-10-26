//! Writes AliceSoft saves back, splicing changed scalars into the original bytes.

use std::collections::BTreeMap;
use std::io::Write;

use flate2::Compression;
use flate2::write::ZlibEncoder;
use saveprism_core::{ByteSpan, Error, ParamValue, Params, Result, SaveDoc, Value};

use super::{ASD_WRITE, P_ORIGINAL, P_RAW, SYS4_MAGIC, shift_jis_bytes};

struct Patch {
    span: ByteSpan,
    replacement: Vec<u8>,
}

fn write_error(message: impl Into<String>) -> Error {
    Error::new(ASD_WRITE, message)
}

fn i32_bytes(value: i128) -> Result<Vec<u8>> {
    let value = i32::try_from(value)
        .map_err(|_| write_error("an AliceSoft System 4 number does not fit int32"))?;
    Ok(value.to_le_bytes().to_vec())
}

fn u16_bytes(value: i128) -> Result<Vec<u8>> {
    let value = u16::try_from(value)
        .map_err(|_| write_error("an AliceSoft System 3.x number does not fit uint16"))?;
    Ok(value.to_le_bytes().to_vec())
}

fn float_bytes(value: f64) -> Vec<u8> {
    (value as f32).to_le_bytes().to_vec()
}

/// Encodes text and refuses a change that would move later absolute offsets.
fn fixed_text(span: ByteSpan, text: &str) -> Result<Vec<u8>> {
    let bytes = shift_jis_bytes(text)?;
    if bytes.contains(&0) {
        return Err(write_error("an AliceSoft string cannot contain a NUL byte"));
    }
    let length = span.end.saturating_sub(span.start);
    if u64::try_from(bytes.len()).ok() != Some(length) {
        return Err(write_error(format!(
            "an AliceSoft string must keep its byte length ({length} bytes); the new text is {} bytes",
            bytes.len()
        )));
    }
    Ok(bytes)
}

fn collect_system3(doc: &SaveDoc, patches: &mut Vec<Patch>) -> Result<()> {
    let mut stack = vec![doc.root()];
    while let Some(id) = stack.pop() {
        let Some(node) = doc.get(id) else {
            continue;
        };
        let hint = node.type_hint.as_ref().map(|hint| hint.as_str());
        if let (Some(span), Some(hint)) = (node.span, hint) {
            let replacement = match (hint, &node.value) {
                ("ASD:number", Value::Int { v, .. }) => Some(u16_bytes(*v)?),
                ("ASD:string", Value::Str(text)) => Some(fixed_text(span, text)?),
                _ => None,
            };
            if let Some(replacement) = replacement {
                patches.push(Patch { span, replacement });
            }
        }
        stack.extend(doc.children(id));
    }
    Ok(())
}

fn collect_system4(doc: &SaveDoc, patches: &mut Vec<Patch>) -> Result<()> {
    let mut stack = vec![doc.root()];
    while let Some(id) = stack.pop() {
        let Some(node) = doc.get(id) else {
            continue;
        };
        let hint = node.type_hint.as_ref().map(|hint| hint.as_str());
        if let (Some(span), Some(hint)) = (node.span, hint) {
            let replacement = match (hint, &node.value) {
                ("ASD:value-int", Value::Int { v, .. }) => Some(i32_bytes(*v)?),
                ("ASD:value-bool", Value::Bool(value)) => {
                    Some(i32::from(*value).to_le_bytes().to_vec())
                }
                ("ASD:value-bool", Value::Int { v, .. }) => {
                    let value: i32 = if *v == 0 { 0 } else { 1 };
                    Some(value.to_le_bytes().to_vec())
                }
                ("ASD:value-float", Value::Float { v, .. }) => Some(float_bytes(*v)),
                ("ASD:string", Value::Str(text)) => Some(fixed_text(span, text)?),
                _ => None,
            };
            if let Some(replacement) = replacement {
                patches.push(Patch { span, replacement });
            }
        }
        stack.extend(doc.children(id));
    }
    Ok(())
}

fn apply(original: &[u8], mut patches: Vec<Patch>) -> Result<Vec<u8>> {
    patches.sort_by_key(|patch| patch.span.start);
    let mut unique: BTreeMap<u64, (u64, Vec<u8>)> = BTreeMap::new();
    for patch in patches {
        match unique.get(&patch.span.start) {
            Some((end, bytes)) => {
                if *end != patch.span.end || *bytes != patch.replacement {
                    return Err(write_error(
                        "two AliceSoft values claim the same byte range with different text",
                    ));
                }
            }
            None => {
                unique.insert(patch.span.start, (patch.span.end, patch.replacement));
            }
        }
    }
    let mut out = original.to_vec();
    let mut previous_end = u64::try_from(original.len()).unwrap_or(u64::MAX);
    for (start, (end, bytes)) in unique.iter().rev() {
        if *start > *end || *end > previous_end {
            return Err(write_error(
                "an AliceSoft value range is outside the original bytes or overlaps another",
            ));
        }
        previous_end = *start;
        let start = usize::try_from(*start)
            .map_err(|_| write_error("an AliceSoft span does not fit this platform"))?;
        let end = usize::try_from(*end)
            .map_err(|_| write_error("an AliceSoft span does not fit this platform"))?;
        if start > end || end > out.len() {
            return Err(write_error(
                "an AliceSoft value span is outside the original bytes",
            ));
        }
        out.splice(start..end, bytes.iter().copied());
    }
    Ok(out)
}

fn original_bytes(params: &Params) -> Result<Vec<u8>> {
    match params.get(P_ORIGINAL) {
        Some(ParamValue::Bytes(bytes)) => Ok(bytes.to_vec()),
        _ => Err(write_error(
            "the recipe has no original AliceSoft bytes to patch",
        )),
    }
}

pub(crate) fn encode(doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
    let original = original_bytes(params)?;
    if let Some(ParamValue::Bytes(raw)) = params.get(P_RAW) {
        let mut patches = Vec::new();
        collect_system4(doc, &mut patches)?;
        let patched = apply(raw, patches)?;
        if patched == *raw {
            return Ok(original);
        }
        return compress_system4(&original, &patched);
    }
    let mut patches = Vec::new();
    collect_system3(doc, &mut patches)?;
    apply(&original, patches)
}

fn compress_system4(original: &[u8], raw: &[u8]) -> Result<Vec<u8>> {
    if !original.starts_with(SYS4_MAGIC) {
        return Err(write_error(
            "the original System 4 file has no GD envelope to rebuild",
        ));
    }
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::new(6));
    encoder
        .write_all(raw)
        .map_err(|error| write_error(format!("compressing the System 4 body failed: {error}")))?;
    let compressed = encoder
        .finish()
        .map_err(|error| write_error(format!("compressing the System 4 body failed: {error}")))?;
    let size = u32::try_from(raw.len())
        .map_err(|_| write_error("the System 4 body is larger than 4 GiB"))?;
    let mut out = Vec::with_capacity(8 + compressed.len());
    out.extend_from_slice(original.get(..4).unwrap_or(SYS4_MAGIC));
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&compressed);
    Ok(out)
}
