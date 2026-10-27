//! Decodes the header, the zlib chunk list and the 1.x world body into the document tree.

use std::collections::BTreeMap;
use std::io::Read;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    ByteSpan, DecodeCtx, FloatWidth, IntWidth, Node, NodeFlags, NodeId, ParamValue, Params, Result,
    SaveDoc, TypeHint, Value,
};

use super::{
    CHUNK_HEADER_V1_SIZE, CHUNK_HEADER_V2, CHUNK_HEADER_V2_SIZE, CHUNK_TAG, HINT_PREFIX,
    MAX_CHUNKS, MAX_NESTING, MAX_STRUCT_ELEMENTS, P_CHUNK_SIZES, P_CHUNK_VERSION, P_COMPRESSION,
    P_HEADER_LEN, P_IMAGE, P_MAX_CHUNK, P_ORIGINAL, P_PACKAGE_TAG, not_save, parse_error,
};

/// A string longer than this is treated as a malformed file.
const MAX_STRING: usize = 1 << 24;
/// Largest plausible number of levels or objects in one level.
const MAX_ENTRIES: usize = 5_000_000;

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8], pos: usize) -> Self {
        Self { data, pos }
    }

    fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8]> {
        let end = self
            .pos
            .checked_add(len)
            .ok_or_else(|| parse_error("a length in the Satisfactory save overflows"))?;
        let bytes = self
            .data
            .get(self.pos..end)
            .ok_or_else(|| parse_error("the Satisfactory save ends too early"))?;
        self.pos = end;
        Ok(bytes)
    }

    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?.first().copied().unwrap_or(0))
    }

    fn i8(&mut self) -> Result<i8> {
        Ok(self.u8()? as i8)
    }

    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().map_err(
            |_| parse_error("a u16 in the save is cut short"),
        )?))
    }

    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().map_err(
            |_| parse_error("a u32 in the save is cut short"),
        )?))
    }

    fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into().map_err(
            |_| parse_error("an i32 in the save is cut short"),
        )?))
    }

    fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().map_err(
            |_| parse_error("a u64 in the save is cut short"),
        )?))
    }

    fn i64(&mut self) -> Result<i64> {
        Ok(i64::from_le_bytes(self.take(8)?.try_into().map_err(
            |_| parse_error("an i64 in the save is cut short"),
        )?))
    }

    fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into().map_err(
            |_| parse_error("a float in the save is cut short"),
        )?))
    }

    fn f64(&mut self) -> Result<f64> {
        Ok(f64::from_le_bytes(self.take(8)?.try_into().map_err(
            |_| parse_error("a double in the save is cut short"),
        )?))
    }

    fn guid(&mut self) -> Result<()> {
        self.take(16)?;
        Ok(())
    }

    /// Unreal FString: positive length = UTF-8 with a trailing NUL, negative = UTF-16LE with a
    /// trailing NUL (the length counts characters including the terminator).
    fn string(&mut self) -> Result<String> {
        let length = self.i32()?;
        if length == 0 {
            return Ok(String::new());
        }
        if length > 0 {
            let length = usize::try_from(length).unwrap_or(0);
            if length > MAX_STRING {
                return Err(parse_error("a string in the save is implausibly long"));
            }
            let bytes = self.take(length)?;
            let text = bytes.get(..length.saturating_sub(1)).unwrap_or(&[]);
            Ok(String::from_utf8_lossy(text).into_owned())
        } else {
            let units = length
                .checked_neg()
                .and_then(|value| usize::try_from(value).ok())
                .ok_or_else(|| parse_error("a UTF-16 string length overflows"))?;
            if units > MAX_STRING {
                return Err(parse_error("a string in the save is implausibly long"));
            }
            let bytes = self.take(units.saturating_mul(2))?;
            let text_units = units.saturating_sub(1);
            let mut words = Vec::with_capacity(text_units);
            for index in 0..text_units {
                let start = index.saturating_mul(2);
                if let Some(pair) = bytes.get(start..start.saturating_add(2))
                    && let Ok(word) = <[u8; 2]>::try_from(pair)
                {
                    words.push(u16::from_le_bytes(word));
                }
            }
            Ok(String::from_utf16_lossy(&words))
        }
    }

    fn skip_counted(&mut self, count: usize, what: &str) -> Result<()> {
        if count > MAX_ENTRIES {
            return Err(parse_error(format!(
                "the {what} count {count} is implausible"
            )));
        }
        Ok(())
    }
}

fn hint(text: impl Into<String>) -> TypeHint {
    TypeHint::new(format!("{HINT_PREFIX}{}", text.into()))
}

fn readonly_node(value: Value, type_name: &str) -> Node {
    let mut node = Node::new(value);
    node.type_hint = Some(hint(type_name));
    node.flags.insert(NodeFlags::READ_ONLY);
    node
}

fn scalar_node(value: Value, type_name: &str, span: Option<(usize, usize)>) -> Node {
    let mut node = Node::new(value);
    node.type_hint = Some(hint(type_name));
    if let Some((start, end)) = span {
        node.span = Some(ByteSpan {
            start: len_u64(start),
            end: len_u64(end),
        });
    }
    node
}

fn raw_node(bytes: &[u8], type_name: &str, span: Option<(usize, usize)>) -> Node {
    let mut node = readonly_node(Value::Bytes(bytes.to_vec()), type_name);
    if let Some((start, end)) = span {
        node.span = Some(ByteSpan {
            start: len_u64(start),
            end: len_u64(end),
        });
    }
    node
}

fn int_value(value: i128, width: IntWidth) -> Value {
    Value::Int { v: value, width }
}

fn float_value(value: f64, width: FloatWidth) -> Value {
    Value::Float { v: value, width }
}

fn object_node(entries: Vec<(String, NodeId)>) -> Node {
    readonly_node(Value::Object(entries), "object")
}

fn array_node(items: Vec<NodeId>) -> Node {
    readonly_node(Value::Array(items), "array")
}

/// Walks the header without building a tree, so `sniff` can find the first chunk safely.
pub(crate) fn header_end(input: &[u8], header_version: u32) -> Result<usize> {
    let mut reader = Reader::new(input, 0);
    reader.u32()?;
    reader.u32()?;
    reader.u32()?;
    if header_version >= 14 {
        reader.string()?;
    }
    reader.string()?;
    reader.string()?;
    reader.string()?;
    reader.u32()?;
    reader.u64()?;
    reader.u8()?;
    if header_version >= 7 {
        reader.u32()?;
    }
    if header_version >= 8 {
        reader.string()?;
        reader.u32()?;
    }
    if header_version >= 10 {
        reader.string()?;
    }
    if header_version >= 11 {
        reader.u32()?;
    }
    if header_version >= 12 {
        reader.u32()?;
        reader.guid()?;
    }
    if header_version >= 13 {
        reader.u32()?;
    }
    Ok(reader.pos)
}

struct ChunkInfo {
    sizes: Vec<usize>,
    version: u32,
    max_chunk: u32,
    package_tag: u32,
    compression: u8,
}

fn decompress(input: &[u8], start: usize, ctx: &DecodeCtx<'_>) -> Result<(Vec<u8>, ChunkInfo)> {
    let mut reader = Reader::new(input, start);
    let mut ranges = Vec::new();
    let mut sizes = Vec::new();
    let mut version = 0_u32;
    let mut max_chunk = 0_u32;
    let mut package_tag = 0_u32;
    let mut compression = 0_u8;
    let mut total: u64 = 0;

    while reader.remaining() > 0 {
        if ranges.len() >= MAX_CHUNKS {
            return Err(parse_error("the save has too many compressed chunks"));
        }
        let chunk_start = reader.pos;
        let tag = reader.u32()?;
        if tag != CHUNK_TAG {
            return Err(parse_error("a chunk does not start with the 9E2A83C1 tag"));
        }
        let chunk_version = reader.u32()?;
        let max = reader.u32()?;
        reader.u32()?;
        let (header_size, algorithm, compressed_at, uncompressed_at) = match chunk_version {
            CHUNK_HEADER_V2 => {
                let algorithm = reader.u8()?;
                (CHUNK_HEADER_V2_SIZE, algorithm, 17, 25)
            }
            0 => (CHUNK_HEADER_V1_SIZE, 0, 16, 24),
            other => {
                return Err(parse_error(format!(
                    "unknown Satisfactory chunk header version 0x{other:08X}"
                )));
            }
        };
        if ranges.is_empty() {
            version = chunk_version;
            max_chunk = max;
            package_tag = tag;
            compression = algorithm;
        }
        let compressed = {
            let mut size_reader = Reader::new(input, chunk_start.saturating_add(compressed_at));
            size_reader.u64()?
        };
        let uncompressed = {
            let mut size_reader = Reader::new(input, chunk_start.saturating_add(uncompressed_at));
            size_reader.u64()?
        };
        let compressed = usize::try_from(compressed)
            .map_err(|_| parse_error("a compressed chunk size does not fit this platform"))?;
        let uncompressed = usize::try_from(uncompressed)
            .map_err(|_| parse_error("a chunk size does not fit this platform"))?;
        let body_start = chunk_start
            .checked_add(header_size)
            .ok_or_else(|| parse_error("a chunk offset overflows"))?;
        let body_end = body_start.saturating_add(compressed);
        if body_end > input.len() {
            return Err(parse_error("a compressed chunk is cut short"));
        }
        total = total
            .checked_add(len_u64(uncompressed))
            .ok_or_else(|| parse_error("the decoded body size overflows"))?;
        ranges.push((body_start, compressed, uncompressed));
        sizes.push(uncompressed);
        reader.pos = body_end;
    }

    ctx.check_output(len_u64(input.len()), total)?;

    let mut output = Vec::with_capacity(usize::try_from(total).unwrap_or(0));
    for (body_start, compressed, uncompressed) in ranges {
        let body = input
            .get(body_start..body_start.saturating_add(compressed))
            .ok_or_else(|| parse_error("a compressed chunk is cut short"))?;
        let decoded = inflate_chunk(body, uncompressed)?;
        output.extend_from_slice(&decoded);
    }

    Ok((
        output,
        ChunkInfo {
            sizes,
            version,
            max_chunk,
            package_tag,
            compression,
        },
    ))
}

fn inflate_chunk(body: &[u8], expected: usize) -> Result<Vec<u8>> {
    let mut decoder = flate2::read::ZlibDecoder::new(body);
    let mut output = Vec::with_capacity(expected.min(MAX_STRING));
    let limit = expected
        .checked_add(1)
        .ok_or_else(|| parse_error("a chunk size overflows"))?;
    decoder
        .by_ref()
        .take(u64::try_from(limit).unwrap_or(u64::MAX))
        .read_to_end(&mut output)
        .map_err(|source| parse_error("a zlib chunk cannot be decompressed").with_source(source))?;
    if output.len() != expected {
        return Err(parse_error(format!(
            "a chunk decoded to {} bytes, the header says {expected}",
            output.len()
        )));
    }
    Ok(output)
}

struct Header {
    map_name: String,
    values: Vec<(String, NodeId)>,
}

fn read_header(reader: &mut Reader<'_>, doc: &mut SaveDoc) -> Result<Header> {
    let mut values = Vec::new();

    let push_number = |doc: &mut SaveDoc,
                       values: &mut Vec<(String, NodeId)>,
                       name: &str,
                       value: i128,
                       width: IntWidth,
                       span: (usize, usize)|
     -> Result<()> {
        let descriptor = match width {
            IntWidth::U8 => "UInt8",
            IntWidth::U32 => "UInt32",
            IntWidth::U64 => "UInt64",
            _ => "Int",
        };
        let node = scalar_node(int_value(value, width), descriptor, Some(span));
        values.push((name.to_string(), doc.push(node)?));
        Ok(())
    };
    let push_text = |doc: &mut SaveDoc,
                     values: &mut Vec<(String, NodeId)>,
                     name: &str,
                     text: String|
     -> Result<()> {
        values.push((
            name.to_string(),
            doc.push(readonly_node(Value::Str(text), "String"))?,
        ));
        Ok(())
    };

    let start = reader.pos;
    let header_version = reader.u32()?;
    push_number(
        doc,
        &mut values,
        "HeaderVersion",
        i128::from(header_version),
        IntWidth::U32,
        (start, start.saturating_add(4)),
    )?;
    let start = reader.pos;
    let save_version = reader.u32()?;
    push_number(
        doc,
        &mut values,
        "SaveVersion",
        i128::from(save_version),
        IntWidth::U32,
        (start, start.saturating_add(4)),
    )?;
    let start = reader.pos;
    let build = reader.u32()?;
    push_number(
        doc,
        &mut values,
        "BuildVersion",
        i128::from(build),
        IntWidth::U32,
        (start, start.saturating_add(4)),
    )?;

    if header_version >= 14 {
        push_text(doc, &mut values, "SaveName", reader.string()?)?;
    }
    let map_name = reader.string()?;
    push_text(doc, &mut values, "MapName", map_name.clone())?;
    push_text(doc, &mut values, "MapOptions", reader.string()?)?;
    push_text(doc, &mut values, "SessionName", reader.string()?)?;

    let start = reader.pos;
    let played = reader.u32()?;
    push_number(
        doc,
        &mut values,
        "PlayedSeconds",
        i128::from(played),
        IntWidth::U32,
        (start, start.saturating_add(4)),
    )?;
    let start = reader.pos;
    let ticks = reader.u64()?;
    push_number(
        doc,
        &mut values,
        "TimestampTicks",
        i128::from(ticks),
        IntWidth::U64,
        (start, start.saturating_add(8)),
    )?;
    let start = reader.pos;
    let visibility = reader.u8()?;
    push_number(
        doc,
        &mut values,
        "SessionVisibility",
        i128::from(visibility),
        IntWidth::U8,
        (start, start.saturating_add(1)),
    )?;

    if header_version >= 7 {
        let start = reader.pos;
        let editor = reader.u32()?;
        push_number(
            doc,
            &mut values,
            "EditorObjectVersion",
            i128::from(editor),
            IntWidth::U32,
            (start, start.saturating_add(4)),
        )?;
    }
    if header_version >= 8 {
        push_text(doc, &mut values, "ModMetadata", reader.string()?)?;
        let start = reader.pos;
        let flags = reader.u32()?;
        push_number(
            doc,
            &mut values,
            "ModFlags",
            i128::from(flags),
            IntWidth::U32,
            (start, start.saturating_add(4)),
        )?;
    }
    if header_version >= 10 {
        push_text(doc, &mut values, "SaveIdentifier", reader.string()?)?;
    }
    if header_version >= 11 {
        let start = reader.pos;
        let partition = reader.u32()?;
        push_number(
            doc,
            &mut values,
            "IsPartitionedWorld",
            i128::from(partition),
            IntWidth::U32,
            (start, start.saturating_add(4)),
        )?;
    }
    if header_version >= 12 {
        let start = reader.pos;
        let valid = reader.u32()?;
        push_number(
            doc,
            &mut values,
            "ChecksumValid",
            i128::from(valid),
            IntWidth::U32,
            (start, start.saturating_add(4)),
        )?;
        let start = reader.pos;
        let bytes = reader.take(16)?.to_vec();
        let mut node = readonly_node(Value::Bytes(bytes), "MD5");
        node.span = Some(ByteSpan {
            start: len_u64(start),
            end: len_u64(start.saturating_add(16)),
        });
        values.push(("Checksum".to_string(), doc.push(node)?));
    }
    if header_version >= 13 {
        let start = reader.pos;
        let creative = reader.u32()?;
        push_number(
            doc,
            &mut values,
            "CreativeModeEnabled",
            i128::from(creative),
            IntWidth::U32,
            (start, start.saturating_add(4)),
        )?;
    }

    Ok(Header { map_name, values })
}

/// Reads and discards an `FSaveObjectVersionData` block.
fn skip_object_version(reader: &mut Reader<'_>) -> Result<()> {
    reader.u32()?;
    reader.i32()?;
    reader.i32()?;
    reader.i32()?;
    reader.u16()?;
    reader.u16()?;
    reader.u16()?;
    reader.u32()?;
    reader.string()?;
    let count = reader.i32()?;
    if count < 0 {
        return Err(parse_error("a custom version count is negative"));
    }
    let count = usize::try_from(count).unwrap_or(0);
    reader.skip_counted(count, "custom version")?;
    reader.take(count.saturating_mul(20))?;
    Ok(())
}

/// One Unreal tagged property, after its name has been read.
struct Tag {
    name: String,
    ptype: String,
    size: usize,
    payload_start: usize,
    subtype: Option<String>,
    bool_value: Option<(bool, usize)>,
    byte_enum: Option<String>,
}

fn read_tag_node(reader: &mut Reader<'_>, depth: usize) -> Result<(String, Vec<String>)> {
    if depth > MAX_NESTING {
        return Err(parse_error("property type names are nested too deeply"));
    }
    let name = reader.string()?;
    let count = reader.u32()?;
    let count = usize::try_from(count)
        .map_err(|_| parse_error("a property type child count does not fit this platform"))?;
    if count > MAX_ENTRIES {
        return Err(parse_error("a property type has too many children"));
    }
    let mut children = Vec::with_capacity(count.min(16));
    for _ in 0..count {
        let (child, _) = read_tag_node(reader, depth.saturating_add(1))?;
        children.push(child);
    }
    Ok((name, children))
}

fn read_tag(reader: &mut Reader<'_>, complete: bool) -> Result<Option<Tag>> {
    let name = reader.string()?;
    if name == "None" {
        return Ok(None);
    }
    let mut tag = Tag {
        name,
        ptype: String::new(),
        size: 0,
        payload_start: 0,
        subtype: None,
        bool_value: None,
        byte_enum: None,
    };
    if complete {
        let (type_name, child_names) = read_tag_node(reader, 0)?;
        tag.ptype = type_name;
        tag.size = usize::try_from(reader.u32()?)
            .map_err(|_| parse_error("a property size does not fit this platform"))?;
        let flags_at = reader.pos;
        let flags = reader.u8()?;
        if flags & 0x1 != 0 {
            reader.i32()?;
        }
        if flags & 0x2 != 0 {
            reader.guid()?;
        }
        if tag.ptype == "BoolProperty" {
            tag.bool_value = Some((flags & 0x10 != 0, flags_at));
        }
        tag.subtype = child_names.first().cloned();
        tag.payload_start = reader.pos;
        return Ok(Some(tag));
    }

    tag.ptype = reader.string()?;
    let size = reader.i32()?;
    if size < 0 {
        return Err(parse_error("a property payload size is negative"));
    }
    tag.size = usize::try_from(size)
        .map_err(|_| parse_error("a property size does not fit this platform"))?;
    reader.i32()?;
    match tag.ptype.as_str() {
        "ArrayProperty" => tag.subtype = Some(reader.string()?),
        "StructProperty" => {
            tag.subtype = Some(reader.string()?);
            reader.guid()?;
        }
        "SetProperty" => tag.subtype = Some(reader.string()?),
        "BoolProperty" => {
            let start = reader.pos;
            tag.bool_value = Some((reader.i8()? > 0, start));
        }
        "ByteProperty" => tag.byte_enum = Some(reader.string()?),
        "EnumProperty" => {
            reader.string()?;
        }
        "MapProperty" => {
            reader.string()?;
            reader.string()?;
        }
        _ => {}
    }
    if reader.u8()? == 1 {
        reader.guid()?;
    }
    tag.payload_start = reader.pos;
    Ok(Some(tag))
}

fn is_binary_struct(subtype: &str) -> bool {
    matches!(
        subtype,
        "Color"
            | "LinearColor"
            | "Vector"
            | "Vector2D"
            | "Rotator"
            | "Quat"
            | "Vector4"
            | "Vector4D"
            | "Box"
            | "Guid"
            | "DateTime"
            | "IntPoint"
            | "FluidBox"
            | "RailroadTrackPosition"
            | "TimerHandle"
            | "SlateBrush"
            | "InventoryItem"
            | "ClientIdentityInfo"
            | "UniqueNetIdRepl"
            | "PlayerInfoHandle"
    )
}

const SCALAR_TYPES: &[&str] = &[
    "Int8Property",
    "UInt8Property",
    "IntProperty",
    "UInt32Property",
    "Int64Property",
    "UInt64Property",
    "FloatProperty",
    "SingleProperty",
    "DoubleProperty",
];

struct TreeBuilder<'a, 'ctx> {
    doc: &'a mut SaveDoc,
    base: usize,
    level_version: u32,
    ctx: &'ctx DecodeCtx<'ctx>,
}

impl<'a, 'ctx> TreeBuilder<'a, 'ctx> {
    fn span(&self, start: usize, end: usize) -> (usize, usize) {
        (
            self.base.saturating_add(start),
            self.base.saturating_add(end),
        )
    }

    fn scalar(&mut self, tag: &Tag, reader: &mut Reader<'_>) -> Result<Option<NodeId>> {
        if tag.size == 0 && tag.ptype != "BoolProperty" {
            return Ok(None);
        }
        let start = reader.pos;
        let (value, descriptor, size) = match tag.ptype.as_str() {
            "Int8Property" => (
                int_value(i128::from(reader.i8()?), IntWidth::I8),
                "Int8Property",
                1,
            ),
            "UInt8Property" => (
                int_value(i128::from(reader.u8()?), IntWidth::U8),
                "UInt8Property",
                1,
            ),
            "IntProperty" => (
                int_value(i128::from(reader.i32()?), IntWidth::I32),
                "IntProperty",
                4,
            ),
            "UInt32Property" => (
                int_value(i128::from(reader.u32()?), IntWidth::U32),
                "UInt32Property",
                4,
            ),
            "Int64Property" => (
                int_value(i128::from(reader.i64()?), IntWidth::I64),
                "Int64Property",
                8,
            ),
            "UInt64Property" => (
                int_value(i128::from(reader.u64()?), IntWidth::U64),
                "UInt64Property",
                8,
            ),
            "FloatProperty" | "SingleProperty" => (
                float_value(f64::from(reader.f32()?), FloatWidth::F32),
                "FloatProperty",
                4,
            ),
            "DoubleProperty" => (
                float_value(reader.f64()?, FloatWidth::F64),
                "DoubleProperty",
                8,
            ),
            _ => return Ok(None),
        };
        if size != tag.size {
            return Ok(None);
        }
        let span = self.span(start, start.saturating_add(size));
        let node = scalar_node(value, descriptor, Some(span));
        Ok(Some(self.doc.push(node)?))
    }

    fn string_value(&mut self, tag: &Tag, reader: &mut Reader<'_>) -> Result<NodeId> {
        let start = reader.pos;
        let text = match tag.ptype.as_str() {
            "StrProperty" | "NameProperty" | "EnumProperty" => reader.string()?,
            "ByteProperty" => reader.string()?,
            "ObjectProperty" | "InterfaceProperty" => {
                let level = reader.string()?;
                let path = reader.string()?;
                if level.is_empty() {
                    path
                } else {
                    format!("{level}:{path}")
                }
            }
            "SoftObjectProperty" => {
                let path = reader.string()?;
                let name = reader.string()?;
                reader.u32()?;
                if name.is_empty() {
                    path
                } else {
                    format!("{path}:{name}")
                }
            }
            _ => reader.string()?,
        };
        let end = reader.pos;
        let mut node = readonly_node(Value::Str(text), tag.ptype.as_str());
        node.span = Some(ByteSpan {
            start: len_u64(self.span(start, end).0),
            end: len_u64(self.span(start, end).1),
        });
        self.doc.push(node)
    }

    fn property(&mut self, tag: Tag, reader: &mut Reader<'_>, depth: usize) -> Result<NodeId> {
        let payload_start = tag.payload_start;
        let end = payload_start
            .checked_add(tag.size)
            .ok_or_else(|| parse_error("a property payload size overflows"))?;
        self.ctx.check_depth(depth)?;
        if depth > MAX_NESTING {
            return Err(parse_error("properties are nested too deeply"));
        }

        let built = self.build_value(&tag, reader, depth);
        let node = match built {
            Ok(id) if reader.pos == end => id,
            _ => {
                reader.pos = end;
                let bytes = reader.data.get(payload_start..end).unwrap_or(&[]).to_vec();
                let type_name = format!("raw:{}", tag.ptype);
                self.doc.push(raw_node(
                    &bytes,
                    &type_name,
                    Some(self.span(payload_start, end)),
                ))?
            }
        };
        Ok(node)
    }

    fn build_value(&mut self, tag: &Tag, reader: &mut Reader<'_>, depth: usize) -> Result<NodeId> {
        if SCALAR_TYPES.contains(&tag.ptype.as_str()) {
            if let Some(id) = self.scalar(tag, reader)? {
                return Ok(id);
            }
            return Err(parse_error("a scalar property has the wrong payload size"));
        }
        match tag.ptype.as_str() {
            "BoolProperty" => {
                let (value, offset) = tag
                    .bool_value
                    .ok_or_else(|| parse_error("a bool property has no value byte"))?;
                let span = self.span(offset, offset.saturating_add(1));
                let node = scalar_node(Value::Bool(value), "BoolProperty", Some(span));
                self.doc.push(node)
            }
            "ByteProperty" => {
                if tag.byte_enum.as_deref() == Some("None") {
                    let start = reader.pos;
                    let value = reader.u8()?;
                    if tag.size != 1 {
                        return Err(parse_error("a byte property has the wrong payload size"));
                    }
                    let span = self.span(start, start.saturating_add(1));
                    let node = scalar_node(
                        int_value(i128::from(value), IntWidth::U8),
                        "ByteProperty",
                        Some(span),
                    );
                    self.doc.push(node)
                } else {
                    self.string_value(tag, reader)
                }
            }
            "StrProperty" | "NameProperty" | "EnumProperty" | "ObjectProperty"
            | "InterfaceProperty" | "SoftObjectProperty" => self.string_value(tag, reader),
            "StructProperty" => {
                let subtype = tag.subtype.as_deref().unwrap_or_default();
                if subtype == "InventoryItem" {
                    return self.inventory_item(tag, reader);
                }
                if is_binary_struct(subtype) {
                    let start = reader.pos;
                    reader.pos = start.saturating_add(tag.size);
                    let bytes = reader.data.get(start..reader.pos).unwrap_or(&[]).to_vec();
                    let type_name = format!("struct:{subtype}");
                    return self.doc.push(raw_node(
                        &bytes,
                        &type_name,
                        Some(self.span(start, reader.pos)),
                    ));
                }
                let fields = self.property_list(reader, depth.saturating_add(1))?;
                let mut node = object_node(fields);
                node.type_hint = Some(hint(format!("StructProperty:{subtype}")));
                self.doc.push(node)
            }
            "ArrayProperty" => self.array(tag, reader, depth),
            "MapProperty" | "SetProperty" | "TextProperty" => {
                let start = reader.pos;
                reader.pos = start.saturating_add(tag.size);
                let bytes = reader.data.get(start..reader.pos).unwrap_or(&[]).to_vec();
                let type_name = format!("raw:{}", tag.ptype);
                self.doc.push(raw_node(
                    &bytes,
                    &type_name,
                    Some(self.span(start, reader.pos)),
                ))
            }
            _ => {
                let start = reader.pos;
                reader.pos = start.saturating_add(tag.size);
                let bytes = reader.data.get(start..reader.pos).unwrap_or(&[]).to_vec();
                let type_name = format!("raw:{}", tag.ptype);
                self.doc.push(raw_node(
                    &bytes,
                    &type_name,
                    Some(self.span(start, reader.pos)),
                ))
            }
        }
    }

    fn array(&mut self, tag: &Tag, reader: &mut Reader<'_>, depth: usize) -> Result<NodeId> {
        let payload_start = tag.payload_start;
        let end = payload_start
            .checked_add(tag.size)
            .ok_or_else(|| parse_error("an array payload size overflows"))?;
        let count = usize::try_from(reader.u32()?)
            .map_err(|_| parse_error("an array count does not fit this platform"))?;
        let subtype = tag.subtype.as_deref().unwrap_or_default();

        if subtype == "StructProperty" {
            let (element_type, _element_size) = if self.level_version < 53 {
                let inner = read_tag(reader, false)?
                    .ok_or_else(|| parse_error("a struct array has no inner tag"))?;
                (inner.subtype.unwrap_or_default(), inner.size)
            } else {
                (subtype.to_string(), tag.size.saturating_sub(4))
            };
            if is_binary_struct(&element_type) || count > MAX_STRUCT_ELEMENTS {
                reader.pos = end;
                let bytes = reader.data.get(payload_start..end).unwrap_or(&[]).to_vec();
                return self.doc.push(raw_node(
                    &bytes,
                    "raw:ArrayProperty",
                    Some(self.span(payload_start, end)),
                ));
            }
            let mut items = Vec::with_capacity(count.min(1024));
            for _ in 0..count {
                let fields = self.property_list(reader, depth.saturating_add(1))?;
                let mut node = object_node(fields);
                node.type_hint = Some(hint(format!("StructProperty:{element_type}")));
                items.push(self.doc.push(node)?);
            }
            return self.doc.push(array_node(items));
        }

        if SCALAR_TYPES.contains(&subtype) || matches!(subtype, "BoolProperty" | "ByteProperty") {
            let mut items = Vec::with_capacity(count.min(4096));
            for _ in 0..count {
                let start = reader.pos;
                let (value, descriptor, size) = match subtype {
                    "BoolProperty" => (Value::Bool(reader.u8()? != 0), "BoolProperty", 1),
                    "ByteProperty" => (
                        int_value(i128::from(reader.u8()?), IntWidth::U8),
                        "ByteProperty",
                        1,
                    ),
                    "Int8Property" => (
                        int_value(i128::from(reader.i8()?), IntWidth::I8),
                        "Int8Property",
                        1,
                    ),
                    "UInt8Property" => (
                        int_value(i128::from(reader.u8()?), IntWidth::U8),
                        "UInt8Property",
                        1,
                    ),
                    "IntProperty" => (
                        int_value(i128::from(reader.i32()?), IntWidth::I32),
                        "IntProperty",
                        4,
                    ),
                    "UInt32Property" => (
                        int_value(i128::from(reader.u32()?), IntWidth::U32),
                        "UInt32Property",
                        4,
                    ),
                    "Int64Property" => (
                        int_value(i128::from(reader.i64()?), IntWidth::I64),
                        "Int64Property",
                        8,
                    ),
                    "UInt64Property" => (
                        int_value(i128::from(reader.u64()?), IntWidth::U64),
                        "UInt64Property",
                        8,
                    ),
                    "FloatProperty" | "SingleProperty" => (
                        float_value(f64::from(reader.f32()?), FloatWidth::F32),
                        "FloatProperty",
                        4,
                    ),
                    "DoubleProperty" => (
                        float_value(reader.f64()?, FloatWidth::F64),
                        "DoubleProperty",
                        8,
                    ),
                    _ => return Err(parse_error("unknown array element type")),
                };
                let span = self.span(start, start.saturating_add(size));
                items.push(self.doc.push(scalar_node(value, descriptor, Some(span)))?);
            }
            return self.doc.push(array_node(items));
        }

        reader.pos = end;
        let bytes = reader.data.get(payload_start..end).unwrap_or(&[]).to_vec();
        self.doc.push(raw_node(
            &bytes,
            "raw:ArrayProperty",
            Some(self.span(payload_start, end)),
        ))
    }

    /// The `FInventoryItem` inside an `InventoryStack`: the item class reference is named and the
    /// item state (`FGDynamicStruct`) stays raw. A struct whose reference does not fit its tag is
    /// left to the caller's raw fallback.
    fn inventory_item(&mut self, tag: &Tag, reader: &mut Reader<'_>) -> Result<NodeId> {
        let start = reader.pos;
        let end = start
            .checked_add(tag.size)
            .ok_or_else(|| parse_error("an InventoryItem size overflows"))?;
        let level = reader.string()?;
        let path = reader.string()?;
        let reference_end = reader.pos;
        if reference_end > end {
            return Err(parse_error(
                "an InventoryItem reference does not fit its tag",
            ));
        }
        let text = if level.is_empty() {
            path
        } else {
            format!("{level}:{path}")
        };
        let mut item_class = readonly_node(Value::Str(text), "String");
        let (span_start, span_end) = self.span(start, reference_end);
        item_class.span = Some(ByteSpan {
            start: len_u64(span_start),
            end: len_u64(span_end),
        });
        let class_id = self.doc.push(item_class)?;
        let state_bytes = reader.data.get(reference_end..end).unwrap_or(&[]).to_vec();
        let state_id = self.doc.push(raw_node(
            &state_bytes,
            "raw:InventoryItemState",
            Some(self.span(reference_end, end)),
        ))?;
        reader.pos = end;
        let mut node = object_node(vec![
            ("ItemClass".to_string(), class_id),
            ("ItemState".to_string(), state_id),
        ]);
        node.type_hint = Some(hint("StructProperty:InventoryItem"));
        self.doc.push(node)
    }

    fn property_list(
        &mut self,
        reader: &mut Reader<'_>,
        depth: usize,
    ) -> Result<Vec<(String, NodeId)>> {
        self.ctx.check_depth(depth)?;
        if depth > MAX_NESTING {
            return Err(parse_error("properties are nested too deeply"));
        }
        let complete = self.level_version >= 53;
        let mut fields: Vec<(String, NodeId)> = Vec::new();
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        loop {
            let Some(tag) = read_tag(reader, complete)? else {
                break;
            };
            let name = tag.name.clone();
            let id = self.property(tag, reader, depth)?;
            let seen = counts.entry(name.clone()).or_insert(0);
            *seen = seen.saturating_add(1);
            let key = if *seen == 1 {
                name
            } else {
                format!("{name}#{seen}")
            };
            fields.push((key, id));
        }
        Ok(fields)
    }
}

pub(crate) fn read_all(input: &[u8], ctx: &DecodeCtx<'_>) -> Result<(SaveDoc, Params)> {
    let mut reader = Reader::new(input, 0);
    let mut doc = ctx.new_doc();

    // Parse the header into a temporary document first because `read_header` pushes nodes.
    let header = read_header(&mut reader, &mut doc)?;
    let header_end = reader.pos;

    let (stream, chunks) = decompress(input, header_end, ctx)?;
    let save_version = u32_from(input, 4)?;

    let mut combined = Vec::with_capacity(header_end.saturating_add(stream.len()));
    combined.extend_from_slice(input.get(..header_end).unwrap_or(&[]));
    combined.extend_from_slice(&stream);
    let base = header_end;

    let body = read_body(&stream, &header.map_name, save_version, ctx, &mut doc, base)?;

    let header_id = doc.push(object_node(header.values))?;
    let body_id = doc.push(object_node(body))?;
    let mut root_node = object_node(vec![
        ("Header".to_string(), header_id),
        ("Body".to_string(), body_id),
    ]);
    root_node.type_hint = Some(hint("save"));
    let root = doc.push(root_node)?;
    doc.set_root(root);

    let mut params = Params::new();
    params.set(P_ORIGINAL, input.to_vec());
    params.set(P_IMAGE, combined);
    params.set(
        P_HEADER_LEN,
        i64::try_from(header_end).map_err(|_| parse_error("the header is too large"))?,
    );
    let sizes = chunks
        .sizes
        .iter()
        .map(|size| i64::try_from(*size).unwrap_or(i64::MAX))
        .collect::<Vec<_>>();
    params.set(
        P_CHUNK_SIZES,
        ParamValue::List(sizes.into_iter().map(ParamValue::Int).collect()),
    );
    params.set(P_CHUNK_VERSION, i64::from(chunks.version));
    params.set(P_MAX_CHUNK, i64::from(chunks.max_chunk));
    params.set(P_PACKAGE_TAG, i64::from(chunks.package_tag));
    params.set(P_COMPRESSION, i64::from(chunks.compression));
    Ok((doc, params))
}

fn u32_from(input: &[u8], pos: usize) -> Result<u32> {
    let bytes = input
        .get(pos..pos.saturating_add(4))
        .ok_or_else(|| parse_error("the Satisfactory header is cut short"))?;
    Ok(u32::from_le_bytes(bytes.try_into().map_err(|_| {
        parse_error("the Satisfactory header is cut short")
    })?))
}

#[allow(clippy::vec_init_then_push)]
fn read_body(
    stream: &[u8],
    map_name: &str,
    save_version: u32,
    ctx: &DecodeCtx<'_>,
    doc: &mut SaveDoc,
    base: usize,
) -> Result<Vec<(String, NodeId)>> {
    let mut reader = Reader::new(stream, 0);
    let total_rest = reader.i32()?;
    let extra = if save_version >= 37 { 8 } else { 4 };
    let expected = i64::from(total_rest).saturating_add(extra);
    if expected != i64::try_from(stream.len()).unwrap_or(i64::MAX) {
        return Err(not_save(format!(
            "the save body says it is {expected} bytes, the decompressed data is {}",
            stream.len()
        )));
    }
    if save_version >= 37 {
        reader.u32()?;
    }
    if save_version >= 53 {
        skip_object_version(&mut reader)?;
    }

    let mut body = Vec::new();

    // Level-grouping grids.
    let grid_count = reader.i32()?;
    if grid_count < 0 {
        return Err(parse_error("a grid count is negative"));
    }
    reader.skip_counted(usize::try_from(grid_count).unwrap_or(0), "grid")?;
    let mut grids = Vec::new();
    for _ in 0..grid_count {
        let name = reader.string()?;
        let cell_start = reader.pos;
        let cell = reader.i32()?;
        let hash_start = reader.pos;
        let hash = reader.u32()?;
        let child_count = reader.u32()?;
        let child_count = usize::try_from(child_count)
            .map_err(|_| parse_error("a grid child count does not fit this platform"))?;
        reader.skip_counted(child_count, "grid child")?;
        for _ in 0..child_count {
            reader.string()?;
            reader.u32()?;
        }
        let mut fields = Vec::new();
        fields.push((
            "CellSize".to_string(),
            doc.push(scalar_node(
                int_value(i128::from(cell), IntWidth::I32),
                "Int",
                Some((
                    base.saturating_add(cell_start),
                    base.saturating_add(cell_start).saturating_add(4),
                )),
            ))?,
        ));
        fields.push((
            "GridHash".to_string(),
            doc.push(scalar_node(
                int_value(i128::from(hash), IntWidth::U32),
                "UInt32",
                Some((
                    base.saturating_add(hash_start),
                    base.saturating_add(hash_start).saturating_add(4),
                )),
            ))?,
        ));
        fields.push((
            "ChildCount".to_string(),
            doc.push(readonly_node(
                int_value(i128::from(child_count as u64), IntWidth::U32),
                "UInt32",
            ))?,
        ));
        grids.push((name, doc.push(object_node(fields))?));
    }
    body.push(("Grids".to_string(), doc.push(object_node(grids))?));

    // Levels: sublevels first, the persistent level (named by the header's map name) last.
    let level_count = reader.i32()?;
    if level_count < 0 {
        return Err(parse_error("a level count is negative"));
    }
    let level_count = usize::try_from(level_count).unwrap_or(0);
    reader.skip_counted(level_count, "level")?;
    let mut levels = Vec::with_capacity(level_count.saturating_add(1));
    let mut total_objects = 0_usize;
    for index in 0..=level_count {
        let persistent = index == level_count;
        let name = if persistent {
            map_name.to_string()
        } else {
            reader.string()?
        };
        let level = read_level(
            stream,
            &mut reader,
            &name,
            persistent,
            save_version,
            ctx,
            doc,
            base,
        )?;
        total_objects = total_objects.saturating_add(level.1);
        levels.push((name, level.0));
    }
    body.push((
        "LevelCount".to_string(),
        doc.push(readonly_node(
            int_value(i128::from(level_count as u64), IntWidth::U32),
            "UInt32",
        ))?,
    ));
    body.push((
        "ObjectCount".to_string(),
        doc.push(readonly_node(
            int_value(i128::from(total_objects as u64), IntWidth::U32),
            "UInt32",
        ))?,
    ));
    body.push((
        "Levels".to_string(),
        doc.push(array_node(levels.into_iter().map(|(_, id)| id).collect()))?,
    ));

    // Unresolved world data after the levels (usually an empty count).
    if reader.pos < stream.len() {
        let count = reader.i32()?;
        if count > 0 {
            let count = usize::try_from(count).unwrap_or(0);
            reader.skip_counted(count, "unresolved reference")?;
            for _ in 0..count {
                reader.string()?;
                reader.string()?;
            }
        }
    }

    Ok(body)
}

struct LevelHeader {
    kind: u32,
    type_path: String,
    root: String,
    instance: String,
    flags: u32,
    parent: String,
}

#[allow(clippy::too_many_arguments)]
fn read_level(
    stream: &[u8],
    reader: &mut Reader<'_>,
    name: &str,
    persistent: bool,
    save_version: u32,
    ctx: &DecodeCtx<'_>,
    doc: &mut SaveDoc,
    base: usize,
) -> Result<(NodeId, usize)> {
    let toc_len = reader.i32()?;
    if toc_len < 0 {
        return Err(parse_error("a TOC blob length is negative"));
    }
    let toc_len = usize::try_from(toc_len).unwrap_or(0);
    if save_version >= 37 {
        reader.u32()?;
    }
    let toc_start = reader.pos;
    reader.pos = toc_start.saturating_add(toc_len);
    if reader.pos > stream.len() {
        return Err(parse_error("a TOC blob is cut short"));
    }

    let data_len = reader.i32()?;
    if data_len < 0 {
        return Err(parse_error("a data blob length is negative"));
    }
    let data_len = usize::try_from(data_len).unwrap_or(0);
    if save_version >= 37 {
        reader.u32()?;
    }
    let data_start = reader.pos;
    let data_end = data_start.saturating_add(data_len);
    if data_end > stream.len() {
        return Err(parse_error("a data blob is cut short"));
    }

    // The second collectable/destroyed list (and a streamable level's own save version) comes
    // after the two blobs. Read it now so the TOC and object headers know their version.
    let mut after = Reader::new(stream, data_end);
    let level_version = if persistent {
        save_version
    } else {
        after.i32()?.max(0) as u32
    };
    let trailing_collectables = if persistent {
        read_destroyed_map(&mut after)?
    } else {
        read_reference_list(&mut after)?
    };
    if !persistent && level_version >= 53 {
        let has_version = after.i32()?;
        if has_version > 0 {
            skip_object_version(&mut after)?;
        }
    }
    let level_end = after.pos;

    // TOC: object headers, then the first copy of the collectable/destroyed list.
    let mut toc = Reader::new(stream, toc_start);
    let header_count = toc.i32()?;
    if header_count < 0 {
        return Err(parse_error("an object header count is negative"));
    }
    let header_count = usize::try_from(header_count).unwrap_or(0);
    toc.skip_counted(header_count, "object header")?;
    let mut headers = Vec::with_capacity(header_count.min(4096));
    for _ in 0..header_count {
        let kind = toc.u32()?;
        let type_path = toc.string()?;
        let root = toc.string()?;
        let instance = toc.string()?;
        let flags = if level_version >= 49 { toc.u32()? } else { 0 };
        let parent = if kind == 1 {
            toc.u32()?;
            toc.take(40)?;
            toc.u32()?;
            String::new()
        } else {
            toc.string()?
        };
        headers.push(LevelHeader {
            kind,
            type_path,
            root,
            instance,
            flags,
            parent,
        });
    }
    let first_collectables = if toc.pos < toc_start.saturating_add(toc_len) {
        if persistent {
            read_destroyed_map(&mut toc)?
        } else {
            read_reference_list(&mut toc)?
        }
    } else {
        0
    };
    let _ = first_collectables;

    // Data blob: one payload per object header, grouped by class for a navigable tree.
    let mut data = Reader::new(stream, data_start);
    let object_count = data.i32()?;
    if object_count < 0 || usize::try_from(object_count).unwrap_or(usize::MAX) != headers.len() {
        return Err(parse_error(
            "the object data count does not match the object header count",
        ));
    }
    let mut builder = TreeBuilder {
        doc,
        base,
        level_version,
        ctx,
    };
    let mut classes: BTreeMap<String, Vec<NodeId>> = BTreeMap::new();
    for header in &headers {
        let object_version = if save_version >= 38 {
            data.i32()?.max(0) as u32
        } else {
            level_version
        };
        let _ = object_version;
        if save_version >= 38 {
            data.u32()?;
        }
        let size = data.u32()?;
        let size = usize::try_from(size)
            .map_err(|_| parse_error("an object size does not fit this platform"))?;
        let payload_start = data.pos;
        let payload_end = payload_start
            .checked_add(size)
            .ok_or_else(|| parse_error("an object payload size overflows"))?;
        if payload_end > stream.len() {
            return Err(parse_error("an object payload is cut short"));
        }
        let object_id = build_object(&mut builder, &mut data, header, payload_start, payload_end)?;
        data.pos = payload_end;
        if level_version >= 53 {
            let has_version = data.i32()?;
            if has_version > 0 {
                skip_object_version(&mut data)?;
            }
        }
        classes
            .entry(class_of(&header.type_path))
            .or_default()
            .push(object_id);
    }
    reader.pos = level_end;

    let mut fields = Vec::new();
    fields.push((
        "Name".to_string(),
        doc.push(readonly_node(Value::Str(name.to_string()), "String"))?,
    ));
    fields.push((
        "CollectablesCount".to_string(),
        doc.push(readonly_node(
            int_value(i128::from(trailing_collectables as u64), IntWidth::U32),
            "UInt32",
        ))?,
    ));
    let class_entries = classes
        .into_iter()
        .map(|(class_name, objects)| Ok((class_name, doc.push(array_node(objects))?)))
        .collect::<Result<Vec<_>>>()?;
    fields.push(("Classes".to_string(), doc.push(object_node(class_entries))?));
    let level_id = doc.push(object_node(fields))?;
    Ok((level_id, headers.len()))
}

fn read_reference_list(reader: &mut Reader<'_>) -> Result<usize> {
    let count = reader.i32()?;
    if count < 0 {
        return Err(parse_error("a reference count is negative"));
    }
    let count = usize::try_from(count).unwrap_or(0);
    reader.skip_counted(count, "reference")?;
    for _ in 0..count {
        reader.string()?;
        reader.string()?;
    }
    Ok(count)
}

fn read_destroyed_map(reader: &mut Reader<'_>) -> Result<usize> {
    let count = reader.i32()?;
    if count < 0 {
        return Err(parse_error("a destroyed actor map count is negative"));
    }
    let count = usize::try_from(count).unwrap_or(0);
    reader.skip_counted(count, "destroyed actor level")?;
    let mut total = 0_usize;
    for _ in 0..count {
        reader.string()?;
        total = total.saturating_add(read_reference_list(reader)?);
    }
    Ok(total)
}

fn class_of(type_path: &str) -> String {
    type_path
        .rsplit_once('.')
        .map(|(_, value)| value)
        .or_else(|| type_path.rsplit_once('/').map(|(_, value)| value))
        .unwrap_or(type_path)
        .to_string()
}

#[allow(clippy::vec_init_then_push)]
fn build_object(
    builder: &mut TreeBuilder<'_, '_>,
    reader: &mut Reader<'_>,
    header: &LevelHeader,
    payload_start: usize,
    payload_end: usize,
) -> Result<NodeId> {
    let mut parent = header.parent.clone();
    let mut component_count = 0_u32;
    let properties = if header.kind == 1 {
        let level = reader.string()?;
        let path = reader.string()?;
        parent = if level.is_empty() {
            path
        } else {
            format!("{level}:{path}")
        };
        component_count = reader.u32()?;
        let count = usize::try_from(component_count)
            .map_err(|_| parse_error("a component count does not fit this platform"))?;
        reader.skip_counted(count, "component")?;
        for _ in 0..count {
            reader.string()?;
            reader.string()?;
        }
        builder.property_list(reader, 1)
    } else {
        builder.property_list(reader, 1)
    };

    let properties = match properties {
        Ok(fields) => builder.doc.push(object_node(fields))?,
        Err(_) => {
            reader.pos = payload_end;
            let bytes = reader
                .data
                .get(payload_start..payload_end)
                .unwrap_or(&[])
                .to_vec();
            builder.doc.push(raw_node(
                &bytes,
                "raw:object",
                Some(builder.span(payload_start, payload_end)),
            ))?
        }
    };

    let trailing = if reader.pos < payload_end {
        let start = reader.pos;
        let bytes = reader.data.get(start..payload_end).unwrap_or(&[]).to_vec();
        Some((start, bytes))
    } else {
        None
    };

    let mut fields = Vec::new();
    fields.push((
        "TypePath".to_string(),
        builder.doc.push(readonly_node(
            Value::Str(header.type_path.clone()),
            "String",
        ))?,
    ));
    fields.push((
        "InstanceName".to_string(),
        builder
            .doc
            .push(readonly_node(Value::Str(header.instance.clone()), "String"))?,
    ));
    fields.push((
        "RootObject".to_string(),
        builder
            .doc
            .push(readonly_node(Value::Str(header.root.clone()), "String"))?,
    ));
    fields.push((
        "Flags".to_string(),
        builder.doc.push(readonly_node(
            int_value(i128::from(header.flags), IntWidth::U32),
            "UInt32",
        ))?,
    ));
    if header.kind == 1 {
        fields.push((
            "Parent".to_string(),
            builder
                .doc
                .push(readonly_node(Value::Str(parent), "String"))?,
        ));
        fields.push((
            "ComponentCount".to_string(),
            builder.doc.push(readonly_node(
                int_value(i128::from(component_count), IntWidth::U32),
                "UInt32",
            ))?,
        ));
    }
    fields.push(("Properties".to_string(), properties));
    if let Some((start, bytes)) = trailing {
        fields.push((
            "TrailingData".to_string(),
            builder.doc.push(raw_node(
                &bytes,
                "raw:object-trailing",
                Some(builder.span(start, payload_end)),
            ))?,
        ));
    }
    builder.doc.push(object_node(fields))
}
