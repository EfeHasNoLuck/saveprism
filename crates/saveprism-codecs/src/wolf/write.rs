//! Writes a WOLF RPG save by splicing edited scalars back into the original bytes.

use saveprism_core::{IntWidth, Node, Params, Result, SaveDoc, Value};

use super::{HEADER_LEN, HINT_PREFIX, MARKER, P_ORIGINAL, WOLF_WRITE};
use saveprism_core::Error;

fn write_error(message: impl Into<String>) -> Error {
    Error::new(WOLF_WRITE, message)
}

pub(crate) fn encode(doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
    let original = match params.get(P_ORIGINAL) {
        Some(saveprism_core::ParamValue::Bytes(bytes)) => bytes,
        _ => {
            return Err(write_error(
                "the recipe has no original WOLF RPG bytes to patch",
            ));
        }
    };
    if original.get(HEADER_LEN).copied() != Some(MARKER) {
        return Err(write_error(
            "the original bytes are not an unencrypted WOLF RPG save",
        ));
    }
    let mut patches: Vec<Patch> = Vec::new();
    let mut stack = vec![doc.root()];
    while let Some(id) = stack.pop() {
        let Some(node) = doc.get(id) else {
            continue;
        };
        if let Some(span) = node.span
            && let Some(patch) = patch_of(node, span, original.len())?
        {
            patches.push(patch);
        }
        stack.extend(doc.children(id));
    }
    patches.sort_by_key(|patch| patch.start);

    let mut out = original.clone();
    let mut changed = false;
    for patch in patches.iter().rev() {
        if patch.start > patch.end || patch.end > out.len() {
            return Err(write_error(
                "a WOLF RPG value span is outside the original bytes",
            ));
        }
        if out.get(patch.start..patch.end) != Some(patch.bytes.as_slice()) {
            changed = true;
        }
        out.splice(patch.start..patch.end, patch.bytes.iter().copied());
    }

    if changed && out.len() > HEADER_LEN {
        let checksum = out
            .get(HEADER_LEN..)
            .unwrap_or_default()
            .iter()
            .fold(0u8, |sum, byte| sum.wrapping_add(*byte));
        if let Some(slot) = out.get_mut(2) {
            *slot = checksum;
        }
    }
    Ok(out)
}

struct Patch {
    start: usize,
    end: usize,
    bytes: Vec<u8>,
}

fn patch_of(
    node: &Node,
    span: saveprism_core::ByteSpan,
    input_len: usize,
) -> Result<Option<Patch>> {
    let start = usize::try_from(span.start)
        .map_err(|_| write_error("a WOLF RPG value span does not fit this platform"))?;
    let end = usize::try_from(span.end)
        .map_err(|_| write_error("a WOLF RPG value span does not fit this platform"))?;
    if end > input_len {
        return Err(write_error(
            "a WOLF RPG value span is outside the original bytes",
        ));
    }
    let bytes = match &node.value {
        Value::Int { v, width } => {
            if !width.contains(*v) {
                return Err(write_error(format!(
                    "value {v} does not fit the {width:?} WOLF RPG field at 0x{start:X}"
                )));
            }
            let size = end.saturating_sub(start);
            match size {
                1 => vec![u8::try_from(*v).map_err(|_| write_error("value does not fit u8"))?],
                2 => u16::try_from(*v)
                    .map_err(|_| write_error("value does not fit u16"))?
                    .to_le_bytes()
                    .to_vec(),
                4 if *width == IntWidth::I32 => i32::try_from(*v)
                    .map_err(|_| write_error("value does not fit i32"))?
                    .to_le_bytes()
                    .to_vec(),
                4 => u32::try_from(*v)
                    .map_err(|_| write_error("value does not fit u32"))?
                    .to_le_bytes()
                    .to_vec(),
                8 => u64::try_from(*v)
                    .map_err(|_| write_error("value does not fit u64"))?
                    .to_le_bytes()
                    .to_vec(),
                _ => {
                    return Err(write_error(format!(
                        "a WOLF RPG integer at 0x{start:X} has an unsupported width of {size} bytes"
                    )));
                }
            }
        }
        Value::Str(text) => {
            let hint = node
                .type_hint
                .as_ref()
                .map(|hint| hint.as_str())
                .and_then(|hint| hint.strip_prefix(HINT_PREFIX))
                .ok_or_else(|| write_error("a WOLF RPG string has no length hint"))?;
            let (length_bytes, nul) = match hint {
                "str:u16" => (2, false),
                "str:u16:nul" => (2, true),
                "str:u32" => (4, false),
                "str:u32:nul" => (4, true),
                other => {
                    return Err(write_error(format!(
                        "a WOLF RPG string has an unknown hint '{other}'"
                    )));
                }
            };
            let mut bytes = Vec::with_capacity(text.len() + 1 + length_bytes);
            let length = text
                .len()
                .checked_add(usize::from(nul))
                .ok_or_else(|| write_error("a WOLF RPG string is too long"))?;
            match length_bytes {
                2 => {
                    let length = u16::try_from(length)
                        .map_err(|_| write_error("a WOLF RPG string is too long for u16"))?;
                    bytes.extend_from_slice(&length.to_le_bytes());
                }
                _ => {
                    let length = u32::try_from(length)
                        .map_err(|_| write_error("a WOLF RPG string is too long for u32"))?;
                    bytes.extend_from_slice(&length.to_le_bytes());
                }
            }
            bytes.extend_from_slice(text.as_bytes());
            if nul {
                bytes.push(0);
            }
            bytes
        }
        _ => return Ok(None),
    };
    Ok(Some(Patch { start, end, bytes }))
}
