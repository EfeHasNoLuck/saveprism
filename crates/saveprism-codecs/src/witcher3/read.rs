//! Decodes the `SNFHFZLC` container, the LZ4 block chunks and the REDengine record stream.

use std::collections::{HashMap, HashSet};

use saveprism_core::limits::len_u64;
use saveprism_core::{
    ByteSpan, DecodeCtx, FloatWidth, IntWidth, Node, NodeFlags, NodeId, ParamValue, Params, Result,
    SaveDoc, TypeHint, Value,
};

use super::{
    FOOTER_MAGIC, HINT_PREFIX, MAGIC, MAX_CHUNKS, MAX_NESTING, P_CHUNK_SIZES, P_HEADER_SIZE,
    P_ORIGINAL, P_STREAM, STREAM_MAGIC, not_save, parse_error,
};

/// Largest number of variable table entries accepted (the game writes far fewer).
const MAX_ENTRIES: usize = 20_000_000;
/// A name longer than this is treated as a malformed string table.
const MAX_NAME: usize = 4096;

struct Container {
    stream: Vec<u8>,
    header_size: usize,
    chunk_sizes: Vec<usize>,
}

fn u16le(data: &[u8], pos: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        data.get(pos..pos.checked_add(2)?)?.try_into().ok()?,
    ))
}

fn u32le(data: &[u8], pos: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        data.get(pos..pos.checked_add(4)?)?.try_into().ok()?,
    ))
}

fn le16(bytes: &[u8]) -> u16 {
    bytes
        .get(..2)
        .and_then(|part| part.try_into().ok())
        .map(u16::from_le_bytes)
        .unwrap_or(0)
}

fn le16s(bytes: &[u8]) -> i16 {
    bytes
        .get(..2)
        .and_then(|part| part.try_into().ok())
        .map(i16::from_le_bytes)
        .unwrap_or(0)
}

fn le32(bytes: &[u8]) -> u32 {
    bytes
        .get(..4)
        .and_then(|part| part.try_into().ok())
        .map(u32::from_le_bytes)
        .unwrap_or(0)
}

fn le32s(bytes: &[u8]) -> i32 {
    bytes
        .get(..4)
        .and_then(|part| part.try_into().ok())
        .map(i32::from_le_bytes)
        .unwrap_or(0)
}

fn le64(bytes: &[u8]) -> u64 {
    bytes
        .get(..8)
        .and_then(|part| part.try_into().ok())
        .map(u64::from_le_bytes)
        .unwrap_or(0)
}

fn le64s(bytes: &[u8]) -> i64 {
    bytes
        .get(..8)
        .and_then(|part| part.try_into().ok())
        .map(i64::from_le_bytes)
        .unwrap_or(0)
}

fn lef32(bytes: &[u8]) -> f32 {
    bytes
        .get(..4)
        .and_then(|part| part.try_into().ok())
        .map(f32::from_le_bytes)
        .unwrap_or(0.0)
}

fn lef64(bytes: &[u8]) -> f64 {
    bytes
        .get(..8)
        .and_then(|part| part.try_into().ok())
        .map(f64::from_le_bytes)
        .unwrap_or(0.0)
}

fn byte_at(bytes: &[u8], index: usize) -> u8 {
    bytes.get(index).copied().unwrap_or(0)
}

fn usize_of(value: u32) -> usize {
    usize::try_from(value).unwrap_or(usize::MAX)
}

fn truncated(at: usize) -> saveprism_core::Error {
    parse_error("the Witcher 3 save ends too early (truncated file)").with_offset(len_u64(at))
}

fn hint(text: &str) -> TypeHint {
    TypeHint::new(format!("{HINT_PREFIX}{text}"))
}

fn hex(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        text.push_str(&format!("{byte:02x}"));
    }
    text
}

/// Splits the container and decompresses every chunk, checking sizes before allocating.
fn read_container(input: &[u8], ctx: &DecodeCtx<'_>) -> Result<Container> {
    if !input.starts_with(MAGIC) {
        return Err(not_save(
            "the file does not start with the Witcher 3 SNFHFZLC magic",
        ));
    }
    let count = usize_of(u32le(input, 8).ok_or_else(|| truncated(8))?);
    let header_size = usize_of(u32le(input, 12).ok_or_else(|| truncated(12))?);
    if count == 0 || count > MAX_CHUNKS {
        return Err(parse_error(format!(
            "the chunk count {count} is not plausible"
        )));
    }
    let table_end = count
        .checked_mul(12)
        .and_then(|size| size.checked_add(16))
        .ok_or_else(|| parse_error("the chunk table size overflows"))?;
    if header_size < table_end || header_size > input.len() {
        return Err(parse_error(format!(
            "the header size {header_size} does not fit the {} byte file",
            input.len()
        )));
    }

    let mut sizes = Vec::with_capacity(count);
    let mut total: u64 = 0;
    let mut position = header_size;
    for index in 0..count {
        let base = 16 + index * 12;
        let compressed = usize_of(u32le(input, base).ok_or_else(|| truncated(base))?);
        let decompressed = usize_of(u32le(input, base + 4).ok_or_else(|| truncated(base + 4))?);
        let end = usize_of(u32le(input, base + 8).ok_or_else(|| truncated(base + 8))?);
        let after = position
            .checked_add(compressed)
            .ok_or_else(|| parse_error("the chunk offsets overflow"))?;
        if end != 0 && end != after {
            return Err(parse_error(format!(
                "chunk {index} ends at {end}, expected {after}"
            )));
        }
        if after > input.len() {
            return Err(truncated(position));
        }
        total = total
            .checked_add(len_u64(decompressed))
            .ok_or_else(|| parse_error("the decoded size overflows"))?;
        sizes.push((compressed, decompressed));
        position = after;
    }
    ctx.check_output(len_u64(input.len()), total)?;

    let mut stream = Vec::with_capacity(usize::try_from(total).unwrap_or(0));
    let mut position = header_size;
    for (index, (compressed, decompressed)) in sizes.iter().copied().enumerate() {
        let stored = input
            .get(position..position.saturating_add(compressed))
            .ok_or_else(|| truncated(position))?;
        if compressed == decompressed {
            stream.extend_from_slice(stored);
        } else {
            let decoded = lz4_flex::block::decompress(stored, decompressed).map_err(|_| {
                parse_error(format!("chunk {index} is not a valid LZ4 block"))
                    .with_offset(len_u64(position))
            })?;
            if decoded.len() != decompressed {
                return Err(parse_error(format!(
                    "chunk {index} decoded to {} bytes, expected {decompressed}",
                    decoded.len()
                )));
            }
            stream.extend_from_slice(&decoded);
        }
        position = position.saturating_add(compressed);
    }

    Ok(Container {
        stream,
        header_size,
        chunk_sizes: sizes
            .into_iter()
            .map(|(_, decompressed)| decompressed)
            .collect(),
    })
}

/// The names table and the variable table read from the stream.
struct Tables {
    names: Vec<String>,
    entries: Vec<(usize, usize)>,
}

fn read_tables(stream: &[u8], header_size: usize) -> Result<Tables> {
    if stream.len() < 16 + 14 {
        return Err(parse_error("the decoded stream is too small"));
    }
    if !stream.starts_with(STREAM_MAGIC) {
        return Err(not_save(
            "the decoded stream does not start with the SAV3 marker",
        ));
    }
    if !stream.ends_with(FOOTER_MAGIC) {
        return Err(parse_error(
            "the decoded stream does not end with the SE marker",
        ));
    }

    let footer = stream.len() - 6;
    let variables_absolute = u32le(stream, footer).ok_or_else(|| truncated(footer))?;
    let variables_offset = usize_of(variables_absolute)
        .checked_sub(header_size)
        .ok_or_else(|| parse_error("the variable table offset is inside the chunk table"))?;
    if variables_offset + 10 > stream.len() {
        return Err(parse_error(
            "the variable table offset is outside the decoded stream",
        ));
    }

    let names_footer = variables_offset
        .checked_sub(10)
        .ok_or_else(|| parse_error("the variable table offset is too small"))?;
    let names_absolute = u32le(stream, names_footer).ok_or_else(|| truncated(names_footer))?;
    let names_offset = usize_of(names_absolute)
        .checked_sub(header_size)
        .ok_or_else(|| parse_error("the names table offset is inside the chunk table"))?;
    let names_section = names_offset
        .checked_add(2)
        .ok_or_else(|| parse_error("the names table offset overflows"))?;
    if names_section > names_footer {
        return Err(parse_error("the names table offset is after its footer"));
    }
    if stream.get(names_offset..names_section) != Some(b"NM") {
        return Err(parse_error("the names table marker NM is missing"));
    }

    let names = read_names(stream, names_section, names_footer)?;

    let count =
        usize_of(u32le(stream, variables_offset).ok_or_else(|| truncated(variables_offset))?);
    if count > MAX_ENTRIES {
        return Err(parse_error(format!(
            "the variable table has {count} entries (too many)"
        )));
    }
    let table_end = variables_offset
        .checked_add(4)
        .and_then(|start| start.checked_add(count.saturating_mul(8)))
        .ok_or_else(|| parse_error("the variable table size overflows"))?;
    // Real saves count the two words of the 14-byte footer (`string table offset`,
    // `unknown`) as the final variable table entry, so the table can run right up to the
    // last 6 bytes (the variable table offset word and the `SE` marker). The synthetic
    // fixtures end 8 bytes earlier. Both layouts are accepted: the final entry points at
    // the `MANU` table, so record detection drops it.
    if table_end > stream.len() - 6 {
        return Err(parse_error("the variable table runs past the footer"));
    }

    let mut entries = Vec::with_capacity(count.min(1_000_000));
    for index in 0..count {
        let base = variables_offset + 4 + index * 8;
        let offset = u32le(stream, base).ok_or_else(|| truncated(base))?;
        let size = u32le(stream, base + 4).ok_or_else(|| truncated(base + 4))?;
        let offset = usize_of(offset);
        let size = usize_of(size);
        let Some(offset) = offset.checked_sub(header_size) else {
            continue;
        };
        if size == 0 || offset >= stream.len() || size > stream.len() - offset {
            continue;
        }
        entries.push((offset, size));
    }

    Ok(Tables { names, entries })
}

/// Reads the `MANU` name table. Both the single-byte length and the extended low-6-bit length
/// form are accepted; the form that consumes the section exactly wins.
fn read_names(stream: &[u8], start: usize, end: usize) -> Result<Vec<String>> {
    if stream.get(start..start + 4) != Some(b"MANU") {
        return Err(parse_error("the MANU name table marker is missing"));
    }
    let count = usize_of(u32le(stream, start + 4).ok_or_else(|| truncated(start + 4))?);
    if count > end.saturating_sub(start) {
        return Err(parse_error("the name table count is not plausible"));
    }
    let body = start + 12;
    if let Some(names) = names_with(stream, body, end, count, false) {
        return Ok(names);
    }
    if let Some(names) = names_with(stream, body, end, count, true) {
        return Ok(names);
    }
    Err(parse_error("the MANU name table is malformed"))
}

fn names_with(
    stream: &[u8],
    body: usize,
    end: usize,
    count: usize,
    extended: bool,
) -> Option<Vec<String>> {
    let mut position = body;
    let mut names = Vec::with_capacity(count.min(4096));
    for _ in 0..count {
        let first = *stream.get(position)?;
        position += 1;
        let mut length = if extended {
            usize::from(first & 0x3f)
        } else {
            usize::from(first)
        };
        if extended && first & 0x40 != 0 {
            let next = *stream.get(position)?;
            position += 1;
            length |= usize::from(next) << 6;
        }
        if length > MAX_NAME {
            return None;
        }
        let bytes = stream.get(position..position.checked_add(length)?)?;
        position += length;
        names.push(bytes.iter().map(|&byte| byte as char).collect());
    }
    let tail = stream.get(position..position.checked_add(8)?)?;
    if tail.get(4..8)? != b"ENOD" {
        return None;
    }
    position += 8;
    (position == end).then_some(names)
}

/// One recognised variable table record.
struct Record {
    offset: usize,
    size: usize,
    tag: [u8; 4],
}

fn tag_of(stream: &[u8], offset: usize, size: usize) -> Option<[u8; 4]> {
    let head = stream.get(offset..offset.checked_add(4)?)?;
    let (tag, header) = match head.get(..2)? {
        b"BS" => (*b"BS\0\0", 4),
        b"VL" => (*b"VL\0\0", 6),
        b"OP" => (*b"OP\0\0", 6),
        b"SS" => (*b"SS\0\0", 6),
        _ => match head {
            b"BLCK" => (*b"BLCK", 10),
            b"AVAL" => (*b"AVAL", 12),
            b"PORP" => (*b"PORP", 12),
            b"SXAP" => (*b"SXAP", 16),
            b"ROTS" => (*b"ROTS", 8),
            _ => return None,
        },
    };
    (size >= header).then_some(tag)
}

struct Tmp {
    offset: usize,
    size: usize,
    tag: [u8; 4],
    name_index: u16,
    children: Vec<usize>,
}

fn build_records(
    stream: &[u8],
    entries: &[(usize, usize)],
    ctx: &DecodeCtx<'_>,
) -> Result<Vec<Tmp>> {
    let mut seen = HashSet::new();
    let mut records: Vec<Record> = Vec::new();
    for &(offset, size) in entries {
        let Some(tag) = tag_of(stream, offset, size) else {
            continue;
        };
        if !seen.insert((offset, size, tag)) {
            continue;
        }
        records.push(Record { offset, size, tag });
    }
    records.sort_by(|left, right| {
        left.offset
            .cmp(&right.offset)
            .then_with(|| right.size.cmp(&left.size))
    });

    let mut tree = vec![Tmp {
        offset: 0,
        size: stream.len(),
        tag: *b"ROOT",
        name_index: 0,
        children: Vec::new(),
    }];
    let mut stack: Vec<(usize, usize)> = vec![(0, stream.len())];
    let mut last_offset = usize::MAX;

    for record in records {
        if record.offset == last_offset {
            continue;
        }
        last_offset = record.offset;
        while stack.len() > 1 && record.offset >= stack.last().map_or(0, |entry| entry.1) {
            stack.pop();
        }
        let Some(&(parent, parent_end)) = stack.last() else {
            continue;
        };
        let Some(end) = record.offset.checked_add(record.size) else {
            continue;
        };
        if record.offset < tree.get(parent).map_or(0, |entry| entry.offset) || end > parent_end {
            continue;
        }
        ctx.check_depth(stack.len())?;
        if stack.len() > MAX_NESTING {
            return Err(parse_error("the record tree is nested too deeply"));
        }
        let name_index = match record.tag {
            [b'B', b'S', 0, 0] | [b'V', b'L', 0, 0] | [b'O', b'P', 0, 0] => {
                u16le(stream, record.offset + 2).unwrap_or(0)
            }
            [b'B', b'L', b'C', b'K'] | [b'A', b'V', b'A', b'L'] | [b'P', b'O', b'R', b'P'] => {
                u16le(stream, record.offset + 4).unwrap_or(0)
            }
            _ => 0,
        };
        let index = tree.len();
        tree.push(Tmp {
            offset: record.offset,
            size: record.size,
            tag: record.tag,
            name_index,
            children: Vec::new(),
        });
        if let Some(entry) = tree.get_mut(parent) {
            entry.children.push(index);
        }
        stack.push((index, end));
    }
    Ok(tree)
}

fn name_at(names: &[String], index: u16) -> String {
    if index == 0 {
        return "record".to_string();
    }
    names
        .get(usize::from(index) - 1)
        .cloned()
        .unwrap_or_else(|| format!("record_{index}"))
}

fn push_bytes(doc: &mut SaveDoc, stream: &[u8], offset: usize, length: usize) -> Result<NodeId> {
    let mut node = Node::new(Value::Bytes(
        stream
            .get(offset..offset.saturating_add(length))
            .unwrap_or(&[])
            .to_vec(),
    ));
    node.type_hint = Some(hint("bytes"));
    node.flags.insert(NodeFlags::READ_ONLY);
    doc.push(node)
}

/// Decodes a record's value. Returns the node and the number of value bytes consumed.
#[allow(clippy::too_many_arguments)]
fn build_value(
    doc: &mut SaveDoc,
    stream: &[u8],
    names: &[String],
    type_name: &str,
    offset: usize,
    length: usize,
    depth: usize,
    ctx: &DecodeCtx<'_>,
) -> Result<(NodeId, usize)> {
    let mut kind = type_name;
    loop {
        if let Some(rest) = kind.strip_prefix("handle:") {
            kind = rest;
        } else if let Some(rest) = kind.strip_prefix("soft:") {
            kind = rest;
        } else {
            break;
        }
    }
    if let Some(element_type) = kind.strip_prefix("array:2,0,") {
        return build_array(doc, stream, names, element_type, offset, length, depth, ctx);
    }

    let bytes = stream
        .get(offset..offset.saturating_add(length))
        .ok_or_else(|| truncated(offset))?;
    let (value, hint_text, span, _raw) = match kind {
        "Bool" if length >= 1 && (byte_at(bytes, 0) == 0 || byte_at(bytes, 0) == 1) => (
            Value::Bool(byte_at(bytes, 0) == 1),
            "Bool",
            Some((offset, offset + 1)),
            bytes,
        ),
        "Uint8" if length >= 1 => (
            Value::Int {
                v: i128::from(byte_at(bytes, 0)),
                width: IntWidth::U8,
            },
            "Uint8",
            Some((offset, offset + 1)),
            bytes,
        ),
        "Int8" if length >= 1 => (
            Value::Int {
                v: i128::from(byte_at(bytes, 0) as i8),
                width: IntWidth::I8,
            },
            "Int8",
            Some((offset, offset + 1)),
            bytes,
        ),
        "Uint16" if length >= 2 => (
            Value::Int {
                v: i128::from(le16(bytes)),
                width: IntWidth::U16,
            },
            "Uint16",
            Some((offset, offset + 2)),
            bytes,
        ),
        "Int16" if length >= 2 => (
            Value::Int {
                v: i128::from(le16s(bytes)),
                width: IntWidth::I16,
            },
            "Int16",
            Some((offset, offset + 2)),
            bytes,
        ),
        "Uint32" if length >= 4 => (
            Value::Int {
                v: i128::from(le32(bytes)),
                width: IntWidth::U32,
            },
            "Uint32",
            Some((offset, offset + 4)),
            bytes,
        ),
        "Int32" if length >= 4 => (
            Value::Int {
                v: i128::from(le32s(bytes)),
                width: IntWidth::I32,
            },
            "Int32",
            Some((offset, offset + 4)),
            bytes,
        ),
        "Uint64" if length >= 8 => (
            Value::Int {
                v: i128::from(le64(bytes)),
                width: IntWidth::U64,
            },
            "Uint64",
            Some((offset, offset + 8)),
            bytes,
        ),
        "Int64" if length >= 8 => (
            Value::Int {
                v: i128::from(le64s(bytes)),
                width: IntWidth::I64,
            },
            "Int64",
            Some((offset, offset + 8)),
            bytes,
        ),
        "Float" if length >= 4 => (
            Value::Float {
                v: f64::from(lef32(bytes)),
                width: FloatWidth::F32,
            },
            "Float",
            Some((offset, offset + 4)),
            bytes,
        ),
        "Double" if length >= 8 => (
            Value::Float {
                v: lef64(bytes),
                width: FloatWidth::F64,
            },
            "Double",
            Some((offset, offset + 8)),
            bytes,
        ),
        "String" => {
            if let Some((text, hint_text, start, end)) = read_string(bytes) {
                (
                    Value::Str(text),
                    hint_text,
                    Some((offset + start, offset + end)),
                    stream.get(offset + start..offset + end).unwrap_or(&[]),
                )
            } else {
                (Value::Bytes(bytes.to_vec()), "bytes", None, &[] as &[u8])
            }
        }
        "StringAnsi" => {
            if let Some((text, start, end)) = read_string_ansi(bytes) {
                (
                    Value::Str(text),
                    "StringAnsi",
                    Some((offset + start, offset + end)),
                    stream.get(offset + start..offset + end).unwrap_or(&[]),
                )
            } else {
                (Value::Bytes(bytes.to_vec()), "bytes", None, &[] as &[u8])
            }
        }
        "CName" if length >= 2 => (
            Value::Str(name_at(names, u16le(bytes, 0).unwrap_or(0))),
            "CName",
            None,
            &[] as &[u8],
        ),
        "CGUID" if length >= 16 => (
            Value::Str(bytes.iter().map(|byte| format!("{byte:02x}")).collect()),
            "CGUID",
            None,
            &[] as &[u8],
        ),
        _ if (kind.starts_with('E') || kind.starts_with('e')) && length >= 2 => (
            Value::Str(name_at(names, u16le(bytes, 0).unwrap_or(0))),
            "CName",
            None,
            &[] as &[u8],
        ),
        _ => (Value::Bytes(bytes.to_vec()), "bytes", None, &[] as &[u8]),
    };

    let mut node = Node::new(value);
    node.type_hint = Some(hint(hint_text));
    if let Some((start, end)) = span {
        node.span = Some(ByteSpan {
            start: len_u64(start),
            end: len_u64(end),
        });
        let raw = stream.get(start..end).unwrap_or(&[]);
        node.lexeme = Some(format!("raw:{}", hex(raw)).into());
    } else {
        node.flags.insert(NodeFlags::READ_ONLY);
    }
    let id = doc.push(node)?;
    let used = if matches!(kind, "String" | "StringAnsi") {
        span.map(|(_, end)| end.saturating_sub(offset))
            .unwrap_or(length)
    } else {
        fixed_width(kind, length)
    };
    Ok((id, used))
}

/// Bytes a fixed-size scalar consumes, or the whole value for variable/unknown kinds.
fn fixed_width(kind: &str, length: usize) -> usize {
    let width = match kind {
        "Bool" | "Uint8" | "Int8" => 1,
        "Uint16" | "Int16" => 2,
        "Uint32" | "Int32" | "Float" | "CName" => 4,
        "Uint64" | "Int64" | "Double" => 8,
        "CGUID" => 16,
        _ if kind.starts_with('E') || kind.starts_with('e') => 2,
        _ => return length,
    };
    if width <= length { width } else { length }
}

fn read_string(bytes: &[u8]) -> Option<(String, &'static str, usize, usize)> {
    let first = *bytes.first()?;
    let narrow = first & 0x80 != 0;
    let mut length = usize::from(first & 0x3f);
    let mut position = 1;
    if first & 0x40 != 0 {
        let next = *bytes.get(position)?;
        position += 1;
        length |= usize::from(next) << 6;
    }
    let byte_length = if narrow {
        length
    } else {
        length.checked_mul(2)?
    };
    if !narrow && !byte_length.is_multiple_of(2) {
        return None;
    }
    let text = bytes.get(position..position.checked_add(byte_length)?)?;
    let (value, hint_text) = if narrow {
        match std::str::from_utf8(text) {
            Ok(value) => (value.to_string(), "String/utf8"),
            Err(_) => (
                text.iter().map(|&byte| byte as char).collect::<String>(),
                "String/latin1",
            ),
        }
    } else {
        let units: Vec<u16> = text
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_le_bytes(*pair))
            .collect();
        (String::from_utf16(&units).ok()?, "String/utf16")
    };
    Some((value, hint_text, position, position + byte_length))
}

fn read_string_ansi(bytes: &[u8]) -> Option<(String, usize, usize)> {
    let length = usize::from(*bytes.first()?);
    let data = bytes.get(1..1 + length)?;
    let text = if data.last() == Some(&0) && !data.is_empty() {
        data.get(..data.len().saturating_sub(1)).unwrap_or(data)
    } else {
        data
    };
    Some((
        text.iter().map(|&byte| byte as char).collect(),
        1,
        1 + text.len(),
    ))
}

#[allow(clippy::too_many_arguments)]
fn build_array(
    doc: &mut SaveDoc,
    stream: &[u8],
    names: &[String],
    element_type: &str,
    offset: usize,
    length: usize,
    depth: usize,
    ctx: &DecodeCtx<'_>,
) -> Result<(NodeId, usize)> {
    if depth > MAX_NESTING {
        return Err(parse_error("the record tree is nested too deeply"));
    }
    ctx.check_depth(depth + 1)?;
    let Some(count) = u32le(stream, offset).map(usize_of) else {
        return Ok((push_bytes(doc, stream, offset, length)?, length));
    };
    if count > length.saturating_sub(4) {
        return Ok((push_bytes(doc, stream, offset, length)?, length));
    }
    let end = offset.saturating_add(length);
    let mut position = offset + 4;
    let mut children = Vec::with_capacity(count.min(4096));
    for _ in 0..count {
        let remaining = end.saturating_sub(position);
        match build_value(
            doc,
            stream,
            names,
            element_type,
            position,
            remaining,
            depth + 1,
            ctx,
        ) {
            Ok((child, used)) if used > 0 && position.saturating_add(used) <= end => {
                children.push(child);
                position += used;
            }
            _ => return Ok((push_bytes(doc, stream, offset, length)?, length)),
        }
    }
    if position != end {
        return Ok((push_bytes(doc, stream, offset, length)?, length));
    }
    let mut node = Node::new(Value::Array(children));
    node.type_hint = Some(hint("array"));
    node.flags.insert(NodeFlags::READ_ONLY);
    Ok((doc.push(node)?, length))
}

/// Builds a record and returns its display name and node.
fn build_record(
    doc: &mut SaveDoc,
    stream: &[u8],
    names: &[String],
    record: &Tmp,
    children: Vec<(String, NodeId)>,
    ctx: &DecodeCtx<'_>,
) -> Result<(String, NodeId)> {
    let offset = record.offset;
    let size = record.size;
    let name = name_at(names, record.name_index);

    match record.tag {
        [b'B', b'S', 0, 0] | [b'B', b'L', b'C', b'K'] => {
            let mut node = Node::new(Value::Object(children));
            node.type_hint = Some(hint("block"));
            node.flags.insert(NodeFlags::READ_ONLY);
            Ok((name, doc.push(node)?))
        }
        [b'V', b'L', 0, 0] | [b'O', b'P', 0, 0] => {
            let type_index = u16le(stream, offset + 4).unwrap_or(0);
            let type_name = names
                .get(usize::from(type_index).saturating_sub(1))
                .map(String::as_str)
                .unwrap_or("");
            let value_len = size - 6;
            let (id, used) =
                build_value(doc, stream, names, type_name, offset + 6, value_len, 0, ctx)?;
            if used != value_len {
                return Ok((name, push_bytes(doc, stream, offset, size)?));
            }
            Ok((name, id))
        }
        [b'A', b'V', b'A', b'L'] | [b'P', b'O', b'R', b'P'] => {
            let type_index = u16le(stream, offset + 6).unwrap_or(0);
            let type_name = names
                .get(usize::from(type_index).saturating_sub(1))
                .map(String::as_str)
                .unwrap_or("");
            let value_size = u32le(stream, offset + 8).map(usize_of).unwrap_or(0);
            if value_size > size.saturating_sub(12) {
                return Ok((name, push_bytes(doc, stream, offset, size)?));
            }
            let (id, used) = build_value(
                doc,
                stream,
                names,
                type_name,
                offset + 12,
                value_size,
                0,
                ctx,
            )?;
            if used != value_size {
                return Ok((name, push_bytes(doc, stream, offset, size)?));
            }
            Ok((name, id))
        }
        _ => Ok((name, push_bytes(doc, stream, offset, size)?)),
    }
}

/// Object entries with each repeated name numbered in order (`SBCollectionCard`,
/// `SBCollectionCard#2`, ...), so every record has a path of its own. The next number is kept per
/// name, so many repeats of one name stay linear.
pub(super) fn numbered(entries: impl Iterator<Item = (String, NodeId)>) -> Vec<(String, NodeId)> {
    let mut used: HashSet<String> = HashSet::new();
    let mut next: HashMap<String, usize> = HashMap::new();
    let mut out = Vec::new();
    for (name, id) in entries {
        let mut key = name.clone();
        if used.contains(&key) {
            let number = next.entry(name.clone()).or_insert(2);
            loop {
                key = format!("{name}#{number}");
                *number = number.saturating_add(1);
                if !used.contains(&key) {
                    break;
                }
            }
        }
        used.insert(key.clone());
        out.push((key, id));
    }
    out
}

/// A record name without the `#n` that [`numbered`] adds to a repeated one.
fn base_name(key: &str) -> &str {
    match key.rsplit_once('#') {
        Some((base, number))
            if !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit()) =>
        {
            base
        }
        _ => key,
    }
}

/// Cloned `Object` entries of a node (empty for any other value).
fn object_entries(doc: &SaveDoc, id: NodeId) -> Vec<(String, NodeId)> {
    match doc.get(id).map(|node| &node.value) {
        Some(Value::Object(entries)) => entries.clone(),
        _ => Vec::new(),
    }
}

fn int_value(doc: &SaveDoc, id: NodeId) -> Option<i128> {
    match doc.get(id)?.value {
        Value::Int { v, .. } => Some(v),
        _ => None,
    }
}

fn set_hint(doc: &mut SaveDoc, id: NodeId, text: &str) -> Result<()> {
    doc.set_type_hint(id, Some(hint(text)))
}

fn mark_read_only(doc: &mut SaveDoc, id: NodeId) -> Result<()> {
    let Some(node) = doc.get(id) else {
        return Ok(());
    };
    let mut flags = node.flags;
    flags.insert(NodeFlags::READ_ONLY);
    doc.set_flags(id, flags)
}

/// Notes one Gwent card record: its type hint carries the card index, and its identity stays
/// read-only so a UI cannot rewrite deck membership in place.
fn annotate_gwent_card(doc: &mut SaveDoc, id: NodeId) -> Result<()> {
    let index = object_entries(doc, id)
        .into_iter()
        .find(|(name, _)| base_name(name) == "cardIndex")
        .and_then(|(_, child)| int_value(doc, child));
    let text = match index {
        Some(index) => format!("gwent-card:{index}"),
        None => "gwent-card".to_string(),
    };
    set_hint(doc, id, &text)?;
    mark_read_only(doc, id)?;
    for (name, child) in object_entries(doc, id) {
        if base_name(&name) == "cardIndex" {
            mark_read_only(doc, child)?;
        }
    }
    Ok(())
}

/// Notes a collection block (`SBCollectionCardSize` or `SBLeaderCollectionCardSize`). The
/// per-card copy counts stay editable; the size word stays read-only.
fn annotate_gwent_collection(doc: &mut SaveDoc, id: NodeId, card_name: &str) -> Result<()> {
    set_hint(doc, id, "gwent-collection")?;
    for (name, child) in object_entries(doc, id) {
        if base_name(&name) == "Size" {
            mark_read_only(doc, child)?;
        } else if base_name(&name) == card_name {
            annotate_gwent_card(doc, child)?;
        }
    }
    Ok(())
}

/// Notes one deck: its leader, unlocked flag, card count and card list are read-only.
fn annotate_gwent_deck(doc: &mut SaveDoc, id: NodeId) -> Result<()> {
    set_hint(doc, id, "gwent-deck")?;
    mark_read_only(doc, id)?;
    for (name, child) in object_entries(doc, id) {
        match base_name(&name) {
            "DeckUnlocked" | "LeaderIndex" | "CardCount" => mark_read_only(doc, child)?,
            "GwintDeckCard" => annotate_gwent_card(doc, child)?,
            _ => {}
        }
    }
    Ok(())
}

fn annotate_gwent_decks(doc: &mut SaveDoc, id: NodeId) -> Result<()> {
    set_hint(doc, id, "gwent-decks")?;
    for (name, child) in object_entries(doc, id) {
        if base_name(&name) == "DeckCount" {
            mark_read_only(doc, child)?;
        } else if base_name(&name) == "GwintDeck" {
            annotate_gwent_deck(doc, child)?;
        }
    }
    Ok(())
}

fn annotate_gwint_manager(doc: &mut SaveDoc, id: NodeId) -> Result<()> {
    set_hint(doc, id, "gwent-manager")?;
    for (name, child) in object_entries(doc, id) {
        match base_name(&name) {
            "SBCollectionCardSize" => annotate_gwent_collection(doc, child, "SBCollectionCard")?,
            "SBLeaderCollectionCardSize" => {
                annotate_gwent_collection(doc, child, "SBLeaderCollectionCard")?;
            }
            "GwintDecks" => annotate_gwent_decks(doc, child)?,
            "SBSelectedDeckIndex" => {
                set_hint(doc, child, "gwent-selected-deck")?;
                for (field, value) in object_entries(doc, child) {
                    if base_name(&field) == "deckIndex" {
                        mark_read_only(doc, value)?;
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// Finds the `CR4GwintManager` record (the game also writes it without the `CR4` prefix in
/// older saves) and marks its Gwent collection, decks and cards.
fn annotate_gwent(doc: &mut SaveDoc, variables: NodeId) -> Result<()> {
    for (name, id) in object_entries(doc, variables) {
        if name.to_ascii_lowercase().ends_with("gwintmanager") {
            annotate_gwint_manager(doc, id)?;
        }
    }
    Ok(())
}

/// Reads the whole save into a document and the parameters needed to write it back.
pub(super) fn read_all(input: &[u8], ctx: &DecodeCtx<'_>) -> Result<(SaveDoc, Params)> {
    let container = read_container(input, ctx)?;
    let stream = &container.stream;
    let tables = read_tables(stream, container.header_size)?;

    let mut doc = ctx.new_doc();
    let tree = build_records(stream, &tables.entries, ctx)?;

    let mut node_ids: Vec<Option<(String, NodeId)>> = vec![None; tree.len()];
    for index in (1..tree.len()).rev() {
        let Some(record) = tree.get(index) else {
            continue;
        };
        let children = numbered(
            record
                .children
                .iter()
                .filter_map(|child| node_ids.get(*child).and_then(|entry| entry.clone())),
        );
        let built = build_record(&mut doc, stream, &tables.names, record, children, ctx)?;
        if let Some(slot) = node_ids.get_mut(index) {
            *slot = Some(built);
        }
    }

    let variables = numbered(
        tree.first()
            .into_iter()
            .flat_map(|root| root.children.iter())
            .filter_map(|child| node_ids.get(*child).and_then(|entry| entry.clone())),
    );
    let variables_id = doc.push({
        let mut node = Node::new(Value::Object(variables));
        node.type_hint = Some(hint("variables"));
        node.flags.insert(NodeFlags::READ_ONLY);
        node
    })?;
    annotate_gwent(&mut doc, variables_id)?;

    let version = |doc: &mut SaveDoc, position: usize, label: &str| -> Result<NodeId> {
        let value = u32le(stream, position).map(i128::from).unwrap_or(0);
        let mut node = Node::new(Value::Int {
            v: value,
            width: IntWidth::U32,
        });
        node.type_hint = Some(hint(label));
        node.flags.insert(NodeFlags::READ_ONLY);
        doc.push(node)
    };
    let save_version = version(&mut doc, 4, "save_version")?;
    let game_version = version(&mut doc, 8, "game_version")?;
    let unknown = version(&mut doc, 12, "unknown")?;

    let root = doc.push({
        let mut node = Node::new(Value::Object(vec![
            ("save_version".to_string(), save_version),
            ("game_version".to_string(), game_version),
            ("unknown".to_string(), unknown),
            ("Variables".to_string(), variables_id),
        ]));
        node.type_hint = Some(hint("save"));
        node.flags.insert(NodeFlags::READ_ONLY);
        node
    })?;
    doc.set_root(root);

    let mut params = Params::new();
    params.set(P_ORIGINAL, ParamValue::Bytes(input.to_vec()));
    params.set(P_STREAM, ParamValue::Bytes(container.stream));
    params.set(
        P_HEADER_SIZE,
        ParamValue::Int(i64::try_from(container.header_size).unwrap_or(i64::MAX)),
    );
    params.set(
        P_CHUNK_SIZES,
        ParamValue::List(
            container
                .chunk_sizes
                .into_iter()
                .map(|size| ParamValue::Int(i64::try_from(size).unwrap_or(i64::MAX)))
                .collect(),
        ),
    );
    Ok((doc, params))
}
