//! Writes a Witcher 3 save by patching edited scalars into the decoded stream, then recompressing
//! the chunks and rewriting the chunk table.

use saveprism_core::{
    Error, FloatWidth, IntWidth, Node, ParamValue, Params, Result, SaveDoc, Value,
};

use super::{HINT_PREFIX, P_CHUNK_SIZES, P_HEADER_SIZE, P_ORIGINAL, P_STREAM, W3_WRITE};

const RAW_PREFIX: &str = "raw:";

fn write_error(message: impl Into<String>) -> Error {
    Error::new(W3_WRITE, message)
}

fn bad_param(key: &str) -> Error {
    write_error(format!(
        "parameter \"{key}\" of format.witcher3 has the wrong shape"
    ))
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|index| {
            text.get(index..index + 2)
                .and_then(|pair| u8::from_str_radix(pair, 16).ok())
        })
        .collect()
}

fn hint_of(node: &Node) -> Option<&str> {
    node.type_hint
        .as_ref()
        .and_then(|hint| hint.as_str().strip_prefix(HINT_PREFIX))
}

fn encode_int(value: i128, width: IntWidth) -> Result<Vec<u8>> {
    if !width.contains(value) {
        return Err(write_error(format!(
            "value {value} does not fit a {width:?} Witcher 3 field"
        )));
    }
    let bytes = match width {
        IntWidth::I8 => vec![(value as i8) as u8],
        IntWidth::U8 => vec![u8::try_from(value).map_err(|_| write_error("u8"))?],
        IntWidth::I16 => i16::try_from(value)
            .map_err(|_| write_error("i16"))?
            .to_le_bytes()
            .to_vec(),
        IntWidth::U16 => u16::try_from(value)
            .map_err(|_| write_error("u16"))?
            .to_le_bytes()
            .to_vec(),
        IntWidth::I32 => i32::try_from(value)
            .map_err(|_| write_error("i32"))?
            .to_le_bytes()
            .to_vec(),
        IntWidth::U32 => u32::try_from(value)
            .map_err(|_| write_error("u32"))?
            .to_le_bytes()
            .to_vec(),
        IntWidth::I64 => i64::try_from(value)
            .map_err(|_| write_error("i64"))?
            .to_le_bytes()
            .to_vec(),
        IntWidth::U64 => u64::try_from(value)
            .map_err(|_| write_error("u64"))?
            .to_le_bytes()
            .to_vec(),
        IntWidth::Unbounded => {
            return Err(write_error(
                "the Witcher 3 codec has no unbounded integer field",
            ));
        }
    };
    Ok(bytes)
}

fn encode_float(value: f64, width: FloatWidth) -> Vec<u8> {
    match width {
        FloatWidth::F32 => (value as f32).to_le_bytes().to_vec(),
        FloatWidth::F64 => value.to_le_bytes().to_vec(),
    }
}

fn encode_text(text: &str, hint: &str) -> Result<Vec<u8>> {
    match hint {
        "String/utf8" => Ok(text.as_bytes().to_vec()),
        "String/latin1" | "StringAnsi" => text
            .chars()
            .map(|character| {
                u8::try_from(u32::from(character))
                    .map_err(|_| write_error("the text has a character above U+00FF"))
            })
            .collect(),
        "String/utf16" => Ok(text.encode_utf16().flat_map(u16::to_le_bytes).collect()),
        other => Err(write_error(format!(
            "the Witcher 3 string hint \"{other}\" is unknown"
        ))),
    }
}

fn encode_node(node: &Node) -> Result<Vec<u8>> {
    if let Some(raw) = node
        .lexeme
        .as_deref()
        .and_then(|text| text.strip_prefix(RAW_PREFIX))
    {
        return unhex(raw)
            .ok_or_else(|| write_error("a Witcher 3 value has a malformed original-byte lexeme"));
    }
    let hint =
        hint_of(node).ok_or_else(|| write_error("an editable Witcher 3 value has no type hint"))?;
    match (&node.value, hint) {
        (Value::Bool(value), "Bool") => Ok(vec![u8::from(*value)]),
        (Value::Int { v, width }, _) => encode_int(*v, *width),
        (Value::Float { v, width }, _) => Ok(encode_float(*v, *width)),
        (Value::Str(text), hint) => encode_text(text, hint),
        _ => Err(write_error(format!(
            "a Witcher 3 value of type \"{hint}\" cannot be written"
        ))),
    }
}

type Patch = (usize, usize, Vec<u8>);

fn collect_patches(doc: &SaveDoc, stream: &[u8]) -> Result<(Vec<Patch>, bool)> {
    let mut patches = Vec::new();
    let mut changed = false;
    let mut stack = vec![doc.root()];
    let mut visited = vec![false; doc.len()];
    while let Some(id) = stack.pop() {
        if visited.get(id.index()).copied().unwrap_or(true) {
            continue;
        }
        if let Some(slot) = visited.get_mut(id.index()) {
            *slot = true;
        }
        let Some(node) = doc.get(id) else {
            continue;
        };
        if let Some(span) = node.span {
            let start = usize::try_from(span.start)
                .map_err(|_| write_error("a Witcher 3 span does not fit this platform"))?;
            let end = usize::try_from(span.end)
                .map_err(|_| write_error("a Witcher 3 span does not fit this platform"))?;
            if start > end || end > stream.len() {
                return Err(write_error(
                    "a Witcher 3 value span is outside the decoded stream",
                ));
            }
            let bytes = encode_node(node)?;
            if bytes.len() != end - start {
                return Err(write_error(format!(
                    "the value at 0x{start:X} needs {} bytes, the original field has {}",
                    bytes.len(),
                    end - start
                )));
            }
            if stream.get(start..end) != Some(bytes.as_slice()) {
                changed = true;
            }
            patches.push((start, end, bytes));
        }
        stack.extend(doc.children(id));
    }
    patches.sort_by_key(|patch| patch.0);
    Ok((patches, changed))
}

/// Writes the document: unchanged documents return the original bytes, edited ones get a rebuilt
/// chunk table and freshly compressed LZ4 blocks.
pub(super) fn encode(doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
    let original = match params.get(P_ORIGINAL) {
        Some(ParamValue::Bytes(bytes)) => bytes,
        _ => {
            return Err(write_error(
                "the recipe has no original Witcher 3 bytes to patch",
            ));
        }
    };
    let stream = match params.get(P_STREAM) {
        Some(ParamValue::Bytes(bytes)) => bytes,
        _ => return Err(write_error("the recipe has no decoded Witcher 3 stream")),
    };
    let header_size = match params.get(P_HEADER_SIZE) {
        Some(ParamValue::Int(value)) => usize::try_from(*value)
            .map_err(|_| write_error("the header size parameter is negative"))?,
        _ => return Err(bad_param(P_HEADER_SIZE)),
    };
    let chunk_sizes = match params.get(P_CHUNK_SIZES) {
        Some(ParamValue::List(items)) => items
            .iter()
            .map(|item| match item {
                ParamValue::Int(value) => usize::try_from(*value)
                    .map_err(|_| write_error("a chunk size parameter is negative")),
                _ => Err(bad_param(P_CHUNK_SIZES)),
            })
            .collect::<Result<Vec<_>>>()?,
        _ => return Err(bad_param(P_CHUNK_SIZES)),
    };
    if chunk_sizes.is_empty() {
        return Err(write_error("the Witcher 3 save has no chunks"));
    }
    let table_end = chunk_sizes
        .len()
        .checked_mul(12)
        .and_then(|size| size.checked_add(16))
        .ok_or_else(|| write_error("the chunk table size overflows"))?;
    if header_size < table_end || header_size > original.len() {
        return Err(write_error(
            "the header size does not fit the original Witcher 3 file",
        ));
    }
    if chunk_sizes
        .iter()
        .try_fold(0usize, |sum, size| sum.checked_add(*size))
        != Some(stream.len())
    {
        return Err(write_error(
            "the chunk sizes do not add up to the decoded stream length",
        ));
    }

    let (patches, changed) = collect_patches(doc, stream)?;
    if !changed {
        return Ok(original.clone());
    }

    let mut out_stream = stream.clone();
    for (start, end, bytes) in &patches {
        out_stream
            .get_mut(*start..*end)
            .ok_or_else(|| write_error("a Witcher 3 patch is outside the decoded stream"))?
            .copy_from_slice(bytes);
    }

    let mut stored_chunks: Vec<Vec<u8>> = Vec::with_capacity(chunk_sizes.len());
    let mut position = 0usize;
    for size in &chunk_sizes {
        let raw = out_stream
            .get(position..position.saturating_add(*size))
            .ok_or_else(|| write_error("a Witcher 3 chunk is outside the decoded stream"))?;
        let compressed = lz4_flex::block::compress(raw);
        if compressed.len() < raw.len() {
            stored_chunks.push(compressed);
        } else {
            stored_chunks.push(raw.to_vec());
        }
        position = position.saturating_add(*size);
    }

    let mut out = original.get(..header_size).unwrap_or(original).to_vec();
    let mut chunk_position = header_size;
    for (index, (stored, size)) in stored_chunks.iter().zip(&chunk_sizes).enumerate() {
        let entry = 16 + index * 12;
        let end = chunk_position
            .checked_add(stored.len())
            .ok_or_else(|| write_error("the chunk offsets overflow"))?;
        let end_u32 =
            u32::try_from(end).map_err(|_| write_error("the written save is larger than 4 GiB"))?;
        let compressed_u32 = u32::try_from(stored.len())
            .map_err(|_| write_error("a compressed chunk is larger than 4 GiB"))?;
        let decompressed_u32 = u32::try_from(*size)
            .map_err(|_| write_error("a decoded chunk is larger than 4 GiB"))?;
        if let Some(slot) = out.get_mut(entry..entry + 4) {
            slot.copy_from_slice(&compressed_u32.to_le_bytes());
        }
        if let Some(slot) = out.get_mut(entry + 4..entry + 8) {
            slot.copy_from_slice(&decompressed_u32.to_le_bytes());
        }
        if let Some(slot) = out.get_mut(entry + 8..entry + 12) {
            slot.copy_from_slice(&end_u32.to_le_bytes());
        }
        chunk_position = end;
    }
    for stored in stored_chunks {
        out.extend_from_slice(&stored);
    }
    Ok(out)
}
