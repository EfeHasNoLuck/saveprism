//! Writes a Satisfactory save by patching the decoded image in place and recompressing the body.

use std::io::Write;

use saveprism_core::{
    Error, FloatWidth, IntWidth, Node, NodeFlags, ParamValue, Params, Result, SaveDoc, Value,
};

use super::{
    CHUNK_HEADER_V1_SIZE, CHUNK_HEADER_V2, CHUNK_HEADER_V2_SIZE, HINT_PREFIX, P_CHUNK_SIZES,
    P_CHUNK_VERSION, P_COMPRESSION, P_HEADER_LEN, P_IMAGE, P_MAX_CHUNK, P_ORIGINAL, P_PACKAGE_TAG,
    SAT_WRITE,
};

fn write_error(message: impl Into<String>) -> Error {
    Error::new(SAT_WRITE, message)
}

fn bad_param(key: &str) -> Error {
    write_error(format!(
        "parameter \"{key}\" of format.satisfactory has the wrong shape"
    ))
}

fn int_param(params: &Params, key: &str, max: u64) -> Result<u64> {
    match params.get(key) {
        Some(ParamValue::Int(value)) => {
            let value = u64::try_from(*value)
                .map_err(|_| write_error(format!("parameter \"{key}\" is negative")))?;
            if value > max {
                return Err(write_error(format!("parameter \"{key}\" is out of range")));
            }
            Ok(value)
        }
        _ => Err(bad_param(key)),
    }
}

fn bytes_param<'a>(params: &'a Params, key: &str) -> Result<&'a [u8]> {
    match params.get(key) {
        Some(ParamValue::Bytes(bytes)) => Ok(bytes),
        _ => Err(bad_param(key)),
    }
}

fn encode_int(value: i128, width: IntWidth) -> Result<Vec<u8>> {
    if !width.contains(value) {
        return Err(write_error(format!(
            "value {value} does not fit a {width:?} Satisfactory field"
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
                "the Satisfactory codec has no unbounded integer field",
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

fn encode_node(node: &Node) -> Result<Vec<u8>> {
    match &node.value {
        Value::Bool(value) => Ok(vec![u8::from(*value)]),
        Value::Int { v, width } => encode_int(*v, *width),
        Value::Float { v, width } => Ok(encode_float(*v, *width)),
        other => {
            let hint = node
                .type_hint
                .as_ref()
                .map(|hint| hint.as_str())
                .unwrap_or("value");
            let descriptor = hint.strip_prefix(HINT_PREFIX).unwrap_or(hint);
            Err(write_error(format!(
                "the Satisfactory value \"{descriptor}\" ({other:?}) cannot be written"
            )))
        }
    }
}

type Patch = (usize, usize, Vec<u8>);

fn collect_patches(doc: &SaveDoc, image: &[u8]) -> Result<(Vec<Patch>, bool)> {
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
        if node.flags.contains(NodeFlags::READ_ONLY) {
            stack.extend(doc.children(id));
            continue;
        }
        if let Some(span) = node.span {
            let start = usize::try_from(span.start)
                .map_err(|_| write_error("a Satisfactory span does not fit this platform"))?;
            let end = usize::try_from(span.end)
                .map_err(|_| write_error("a Satisfactory span does not fit this platform"))?;
            if start > end || end > image.len() {
                return Err(write_error(
                    "a Satisfactory value span is outside the decoded image",
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
            if image.get(start..end) != Some(bytes.as_slice()) {
                changed = true;
            }
            patches.push((start, end, bytes));
        }
        stack.extend(doc.children(id));
    }
    patches.sort_by_key(|patch| patch.0);
    Ok((patches, changed))
}

fn set_u32(buffer: &mut [u8], offset: usize, value: u32) -> Result<()> {
    let slot = buffer
        .get_mut(offset..offset.saturating_add(4))
        .ok_or_else(|| write_error("a chunk header field is outside the header"))?;
    slot.copy_from_slice(&value.to_le_bytes());
    Ok(())
}

fn set_u64(buffer: &mut [u8], offset: usize, value: u64) -> Result<()> {
    let slot = buffer
        .get_mut(offset..offset.saturating_add(8))
        .ok_or_else(|| write_error("a chunk header field is outside the header"))?;
    slot.copy_from_slice(&value.to_le_bytes());
    Ok(())
}

fn chunk_header(
    version: u32,
    package_tag: u32,
    max_chunk: u32,
    compression: u8,
    compressed: usize,
    uncompressed: usize,
) -> Result<Vec<u8>> {
    let (size, header_version) = if version == CHUNK_HEADER_V2 {
        (CHUNK_HEADER_V2_SIZE, CHUNK_HEADER_V2)
    } else {
        (CHUNK_HEADER_V1_SIZE, 0)
    };
    let mut header = vec![0_u8; size];
    set_u32(&mut header, 0, package_tag)?;
    set_u32(&mut header, 4, header_version)?;
    set_u32(&mut header, 8, max_chunk)?;
    let compressed =
        u64::try_from(compressed).map_err(|_| write_error("a compressed chunk is too large"))?;
    let uncompressed = u64::try_from(uncompressed)
        .map_err(|_| write_error("an uncompressed chunk is too large"))?;
    if header_version == CHUNK_HEADER_V2 {
        if let Some(slot) = header.get_mut(16) {
            *slot = compression;
        }
        set_u64(&mut header, 17, compressed)?;
        set_u64(&mut header, 25, uncompressed)?;
        set_u64(&mut header, 33, compressed)?;
        set_u64(&mut header, 41, uncompressed)?;
    } else {
        set_u64(&mut header, 16, compressed)?;
        set_u64(&mut header, 24, uncompressed)?;
        set_u64(&mut header, 32, compressed)?;
        set_u64(&mut header, 40, uncompressed)?;
    }
    Ok(header)
}

fn compress_chunk(raw: &[u8]) -> Result<Vec<u8>> {
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(raw).map_err(|source| {
        write_error("a Satisfactory chunk cannot be compressed").with_source(source)
    })?;
    encoder.finish().map_err(|source| {
        write_error("a Satisfactory chunk cannot be compressed").with_source(source)
    })
}

/// Writes the document: unchanged documents return the original bytes, edited ones get patched
/// values and freshly compressed body chunks.
pub(super) fn encode(doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
    let original = bytes_param(params, P_ORIGINAL)?;
    let image = bytes_param(params, P_IMAGE)?;
    let header_len = usize::try_from(int_param(params, P_HEADER_LEN, u64::MAX)?)
        .map_err(|_| write_error("the header length does not fit this platform"))?;
    if header_len > original.len() || header_len > image.len() {
        return Err(write_error(
            "the Satisfactory header length is outside the file",
        ));
    }
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
        return Err(write_error("the Satisfactory save has no chunks"));
    }
    let version = u32::try_from(int_param(params, P_CHUNK_VERSION, u64::from(u32::MAX))?)
        .map_err(|_| write_error("the chunk version does not fit a u32"))?;
    let max_chunk = u32::try_from(int_param(params, P_MAX_CHUNK, u64::from(u32::MAX))?)
        .map_err(|_| write_error("the maximum chunk size does not fit a u32"))?;
    let package_tag = u32::try_from(int_param(params, P_PACKAGE_TAG, u64::from(u32::MAX))?)
        .map_err(|_| write_error("the package tag does not fit a u32"))?;
    let compression = u8::try_from(int_param(params, P_COMPRESSION, u64::from(u8::MAX))?)
        .map_err(|_| write_error("the compression id does not fit a byte"))?;

    let stream_len: usize = chunk_sizes
        .iter()
        .try_fold(0usize, |sum, size| sum.checked_add(*size))
        .ok_or_else(|| write_error("the chunk sizes overflow"))?;
    let expected_image = header_len
        .checked_add(stream_len)
        .ok_or_else(|| write_error("the decoded image size overflows"))?;
    if expected_image != image.len() {
        return Err(write_error(
            "the chunk sizes do not add up to the decoded body length",
        ));
    }

    let (patches, changed) = collect_patches(doc, image)?;
    if !changed {
        return Ok(original.to_vec());
    }

    let mut out_image = image.to_vec();
    for (start, end, bytes) in &patches {
        out_image
            .get_mut(*start..*end)
            .ok_or_else(|| write_error("a Satisfactory patch is outside the decoded image"))?
            .copy_from_slice(bytes);
    }

    let old_stream = image.get(header_len..).unwrap_or(&[]);
    let new_stream = out_image.get(header_len..).unwrap_or(&[]);

    // A header-only change keeps the original chunk bytes exactly.
    if new_stream == old_stream {
        let mut out = original.to_vec();
        if let Some(prefix) = out.get_mut(..header_len)
            && let Some(new_prefix) = out_image.get(..header_len)
        {
            prefix.copy_from_slice(new_prefix);
        }
        return Ok(out);
    }

    let mut out = Vec::with_capacity(original.len().saturating_add(1024));
    out.extend_from_slice(out_image.get(..header_len).unwrap_or(&[]));
    let mut position = 0usize;
    for size in &chunk_sizes {
        let raw = new_stream
            .get(position..position.saturating_add(*size))
            .ok_or_else(|| write_error("a Satisfactory chunk is outside the decoded body"))?;
        let compressed = compress_chunk(raw)?;
        out.extend_from_slice(&chunk_header(
            version,
            package_tag,
            max_chunk,
            compression,
            compressed.len(),
            *size,
        )?);
        out.extend_from_slice(&compressed);
        position = position.saturating_add(*size);
    }
    Ok(out)
}
