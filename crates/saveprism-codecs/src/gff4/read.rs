use saveprism_core::limits::len_u64;
use saveprism_core::{
    DecodeCtx, Error, FloatWidth, IntWidth, Node, NodeFlags, NodeId, Params, Result, SaveDoc,
    TypeHint, Value,
};

use super::{
    FIELD_LEN, FLAG_LIST, FLAG_REFERENCE, FLAG_STRUCT, GFF4_PARSE, GFF4_UNSUPPORTED, HEADER_LEN,
    HINT_PREFIX, P_ORIGINAL, STRUCT_LEN,
};

/// Byte order of the platform-dependent part of the file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Endian {
    Little,
    Big,
}

fn read_u16(input: &[u8], offset: usize, endian: Endian) -> Option<u16> {
    let bytes = input.get(offset..offset.checked_add(2)?)?;
    let pair = [*bytes.first()?, *bytes.get(1)?];
    Some(match endian {
        Endian::Little => u16::from_le_bytes(pair),
        Endian::Big => u16::from_be_bytes(pair),
    })
}

pub(crate) fn u32_at(input: &[u8], offset: usize, endian: Endian) -> Option<u32> {
    let bytes = input.get(offset..offset.checked_add(4)?)?;
    let quad = [
        *bytes.first()?,
        *bytes.get(1)?,
        *bytes.get(2)?,
        *bytes.get(3)?,
    ];
    Some(match endian {
        Endian::Little => u32::from_le_bytes(quad),
        Endian::Big => u32::from_be_bytes(quad),
    })
}

fn read_u64(input: &[u8], offset: usize, endian: Endian) -> Option<u64> {
    let bytes = input.get(offset..offset.checked_add(8)?)?;
    let oct = [
        *bytes.first()?,
        *bytes.get(1)?,
        *bytes.get(2)?,
        *bytes.get(3)?,
        *bytes.get(4)?,
        *bytes.get(5)?,
        *bytes.get(6)?,
        *bytes.get(7)?,
    ];
    Some(match endian {
        Endian::Little => u64::from_le_bytes(oct),
        Endian::Big => u64::from_be_bytes(oct),
    })
}

fn read_f32(input: &[u8], offset: usize, endian: Endian) -> Option<f32> {
    u32_at(input, offset, endian).map(f32::from_bits)
}

fn read_f64(input: &[u8], offset: usize, endian: Endian) -> Option<f64> {
    read_u64(input, offset, endian).map(f64::from_bits)
}

/// The platform field decides the byte order of everything after the first five header fields.
pub(crate) fn platform_endian(input: &[u8]) -> Endian {
    match input.get(8..12) {
        Some(b"X360") | Some(b"PS3 ") => Endian::Big,
        _ => Endian::Little,
    }
}

fn parse_error(message: impl Into<String>, offset: usize) -> Error {
    Error::new(GFF4_PARSE, message).with_offset(len_u64(offset))
}

fn unsupported(message: impl Into<String>, offset: usize) -> Error {
    Error::new(GFF4_UNSUPPORTED, message).with_offset(len_u64(offset))
}

#[derive(Clone, Copy, Debug)]
struct StructEntry {
    kind: u32,
    field_count: u32,
    field_offset: u32,
}

struct Reader<'a, 'c> {
    input: &'a [u8],
    endian: Endian,
    struct_count: usize,
    data_offset: usize,
    data_bases: Vec<usize>,
    doc: SaveDoc,
    ctx: &'c DecodeCtx<'a>,
    stack: Vec<u32>,
}

impl<'a> Reader<'a, '_> {
    fn struct_entry(&self, index: u32) -> Result<StructEntry> {
        let index =
            usize::try_from(index).map_err(|_| parse_error("struct index is too large", 0))?;
        if index >= self.struct_count {
            return Err(parse_error("a struct index is out of range", 0));
        }
        let at = HEADER_LEN
            .checked_add(index.saturating_mul(STRUCT_LEN))
            .ok_or_else(|| parse_error("struct offset overflows", 0))?;
        Ok(StructEntry {
            kind: u32_at(self.input, at, self.endian)
                .ok_or_else(|| parse_error("a struct entry is cut short", at))?,
            field_count: u32_at(self.input, at.saturating_add(4), self.endian)
                .ok_or_else(|| parse_error("a struct entry is cut short", at))?,
            field_offset: u32_at(self.input, at.saturating_add(8), self.endian)
                .ok_or_else(|| parse_error("a struct entry is cut short", at))?,
        })
    }

    fn node(&mut self, mut node: Node, hint: &str, offset: usize) -> Result<NodeId> {
        node.type_hint = Some(TypeHint::new(format!("{HINT_PREFIX}{hint}")));
        node.flags.insert(NodeFlags::READ_ONLY);
        self.doc
            .push(node)
            .map_err(|error| error.with_offset(len_u64(offset)))
    }

    fn int(&mut self, value: i128, width: IntWidth, hint: &str, offset: usize) -> Result<NodeId> {
        self.node(Node::new(Value::Int { v: value, width }), hint, offset)
    }

    fn float(
        &mut self,
        value: f64,
        width: FloatWidth,
        hint: &str,
        offset: usize,
    ) -> Result<NodeId> {
        self.node(Node::new(Value::Float { v: value, width }), hint, offset)
    }

    fn floats(&mut self, addr: usize, count: usize, hint: &str, offset: usize) -> Result<NodeId> {
        let mut values = Vec::with_capacity(count);
        for position in 0..count {
            let at = addr
                .checked_add(position.saturating_mul(4))
                .ok_or_else(|| parse_error("a vector offset overflows", addr))?;
            let value = read_f32(self.input, at, self.endian)
                .ok_or_else(|| parse_error("a vector is cut short", at))?;
            values.push(self.float(f64::from(value), FloatWidth::F32, hint, at)?);
        }
        let mut node = Node::new(Value::Array(values));
        node.type_hint = Some(TypeHint::new(format!("{HINT_PREFIX}{hint}")));
        node.flags.insert(NodeFlags::READ_ONLY);
        self.doc
            .push(node)
            .map_err(|error| error.with_offset(len_u64(offset)))
    }

    fn read_struct(&mut self, index: u32, depth: usize) -> Result<NodeId> {
        self.ctx.check_depth(depth)?;
        if self.stack.contains(&index) {
            return Err(parse_error(
                "a struct contains itself through a reference",
                0,
            ));
        }
        let entry = self.struct_entry(index)?;
        let count = usize::try_from(entry.field_count)
            .map_err(|_| parse_error("a struct has too many fields", 0))?;
        let field_start = usize::try_from(entry.field_offset)
            .map_err(|_| parse_error("a field offset is too large", 0))?;
        let field_bytes = count
            .checked_mul(FIELD_LEN)
            .ok_or_else(|| parse_error("a field array overflows", field_start))?;
        let field_end = field_start
            .checked_add(field_bytes)
            .ok_or_else(|| parse_error("a field array overflows", field_start))?;
        if field_end > self.input.len() {
            return Err(parse_error(
                "a struct's field array lies beyond the file",
                field_start,
            ));
        }
        let base = self
            .data_bases
            .get(usize::try_from(index).unwrap_or(usize::MAX))
            .copied()
            .ok_or_else(|| parse_error("a struct has no data block position", 0))?;
        self.stack.push(index);
        let mut entries = Vec::with_capacity(count);
        for position in 0..count {
            let at = field_start
                .checked_add(position.saturating_mul(FIELD_LEN))
                .ok_or_else(|| parse_error("a field offset overflows", field_start))?;
            let label = u32_at(self.input, at, self.endian)
                .ok_or_else(|| parse_error("a field entry is cut short", at))?;
            let kind = u32_at(self.input, at.saturating_add(4), self.endian)
                .ok_or_else(|| parse_error("a field entry is cut short", at))?;
            let value_index = u32_at(self.input, at.saturating_add(8), self.endian)
                .ok_or_else(|| parse_error("a field entry is cut short", at))?;
            let child = self.read_value(base, kind, value_index, depth.saturating_add(1), at)?;
            entries.push((format!("0x{label:08X}"), child));
        }
        self.stack.pop();
        let node = Node::new(Value::Object(entries));
        self.node(node, &format!("struct:{:08X}", entry.kind), field_start)
    }

    fn resolve(&self, addr: usize, flags: u16, offset: usize) -> Result<usize> {
        if flags & FLAG_REFERENCE == 0 {
            return Ok(addr);
        }
        let reference = u32_at(self.input, addr, self.endian)
            .ok_or_else(|| parse_error("a reference is cut short", addr))?;
        let resolved = self
            .data_offset
            .checked_add(
                usize::try_from(reference)
                    .map_err(|_| parse_error("a reference offset is too large", offset))?,
            )
            .ok_or_else(|| parse_error("a reference offset overflows", offset))?;
        if resolved >= self.input.len() {
            return Err(parse_error("a reference lies beyond the file", offset));
        }
        Ok(resolved)
    }

    fn read_value(
        &mut self,
        base: usize,
        raw_kind: u32,
        index: u32,
        depth: usize,
        field_at: usize,
    ) -> Result<NodeId> {
        self.ctx.check_depth(depth)?;
        let type_id = (raw_kind & 0xFFFF) as u16;
        let flags = (raw_kind >> 16) as u16;
        let address = base
            .checked_add(
                usize::try_from(index)
                    .map_err(|_| parse_error("a field index is too large", field_at))?,
            )
            .ok_or_else(|| parse_error("a field index overflows", field_at))?;
        if address >= self.input.len() {
            return Err(parse_error("a field lies beyond the file", field_at));
        }
        let address = self.resolve(address, flags, field_at)?;
        if flags & FLAG_STRUCT != 0 {
            return self.read_struct(u32::from(type_id), depth);
        }
        if flags & FLAG_LIST != 0 {
            return self.read_list(address, type_id, depth, field_at);
        }
        self.read_scalar(address, type_id, field_at)
    }

    fn read_scalar(&mut self, addr: usize, type_id: u16, field_at: usize) -> Result<NodeId> {
        let need = |len: usize| -> Result<()> {
            if addr.saturating_add(len) > self.input.len() {
                Err(parse_error("a value is cut short", addr))
            } else {
                Ok(())
            }
        };
        match type_id {
            0 => {
                need(1)?;
                let value = self.input.get(addr).copied().unwrap_or_default();
                self.int(i128::from(value), IntWidth::U8, "uint8", addr)
            }
            1 => {
                need(1)?;
                let value = self.input.get(addr).copied().unwrap_or_default() as i8;
                self.int(i128::from(value), IntWidth::I8, "int8", addr)
            }
            2 => {
                need(2)?;
                let value = read_u16(self.input, addr, self.endian)
                    .ok_or_else(|| parse_error("a UINT16 is cut short", addr))?;
                self.int(i128::from(value), IntWidth::U16, "uint16", addr)
            }
            3 => {
                need(2)?;
                let value = read_u16(self.input, addr, self.endian)
                    .ok_or_else(|| parse_error("an INT16 is cut short", addr))?
                    as i16;
                self.int(i128::from(value), IntWidth::I16, "int16", addr)
            }
            4 => {
                need(4)?;
                let value = u32_at(self.input, addr, self.endian)
                    .ok_or_else(|| parse_error("a UINT32 is cut short", addr))?;
                self.int(i128::from(value), IntWidth::U32, "uint32", addr)
            }
            5 => {
                need(4)?;
                let value = u32_at(self.input, addr, self.endian)
                    .ok_or_else(|| parse_error("an INT32 is cut short", addr))?
                    as i32;
                self.int(i128::from(value), IntWidth::I32, "int32", addr)
            }
            6 => {
                need(8)?;
                let value = read_u64(self.input, addr, self.endian)
                    .ok_or_else(|| parse_error("a UINT64 is cut short", addr))?;
                self.int(i128::from(value), IntWidth::U64, "uint64", addr)
            }
            7 => {
                need(8)?;
                let value = read_u64(self.input, addr, self.endian)
                    .ok_or_else(|| parse_error("an INT64 is cut short", addr))?
                    as i64;
                self.int(i128::from(value), IntWidth::I64, "int64", addr)
            }
            8 => {
                need(4)?;
                let value = read_f32(self.input, addr, self.endian)
                    .ok_or_else(|| parse_error("a FLOAT32 is cut short", addr))?;
                self.float(f64::from(value), FloatWidth::F32, "float32", addr)
            }
            9 => {
                need(8)?;
                let value = read_f64(self.input, addr, self.endian)
                    .ok_or_else(|| parse_error("a FLOAT64 is cut short", addr))?;
                self.float(value, FloatWidth::F64, "float64", addr)
            }
            10 => {
                need(12)?;
                self.floats(addr, 3, "vector3", addr)
            }
            12 => {
                need(16)?;
                self.floats(addr, 4, "vector4", addr)
            }
            13 => {
                need(16)?;
                self.floats(addr, 4, "quaternion", addr)
            }
            14 => self.read_ecstring(addr),
            15 => {
                need(16)?;
                self.floats(addr, 4, "color4", addr)
            }
            16 => {
                need(64)?;
                self.floats(addr, 16, "matrix4x4", addr)
            }
            17 => {
                need(8)?;
                let string_ref = u32_at(self.input, addr, self.endian)
                    .ok_or_else(|| parse_error("a TlkString is cut short", addr))?;
                let unknown = u32_at(self.input, addr.saturating_add(4), self.endian)
                    .ok_or_else(|| parse_error("a TlkString is cut short", addr))?;
                let reference = self.int(
                    i128::from(string_ref),
                    IntWidth::U32,
                    "tlk-string-ref",
                    addr,
                )?;
                let extra = self.int(
                    i128::from(unknown),
                    IntWidth::U32,
                    "tlk-string-unknown",
                    addr,
                )?;
                let mut node = Node::new(Value::Object(vec![
                    ("string_ref".to_owned(), reference),
                    ("unknown".to_owned(), extra),
                ]));
                node.type_hint = Some(TypeHint::new(format!("{HINT_PREFIX}tlkstring")));
                node.flags.insert(NodeFlags::READ_ONLY);
                self.doc
                    .push(node)
                    .map_err(|error| error.with_offset(len_u64(addr)))
            }
            11 => Err(unsupported("GFF4 field type 11 is not defined", field_at)),
            other => Err(unsupported(
                format!("GFF4 field type {other} is not supported"),
                field_at,
            )),
        }
    }

    fn read_ecstring(&mut self, addr: usize) -> Result<NodeId> {
        let length = u32_at(self.input, addr, self.endian)
            .ok_or_else(|| parse_error("an ECString length is cut short", addr))?;
        let length =
            usize::try_from(length).map_err(|_| parse_error("an ECString is too long", addr))?;
        let bytes = length
            .checked_mul(2)
            .ok_or_else(|| parse_error("an ECString overflows", addr))?;
        let start = addr.saturating_add(4);
        let end = start
            .checked_add(bytes)
            .ok_or_else(|| parse_error("an ECString overflows", addr))?;
        let raw = self
            .input
            .get(start..end)
            .ok_or_else(|| parse_error("an ECString is cut short", start))?;
        let units: Vec<u16> = raw
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| match self.endian {
                Endian::Little => u16::from_le_bytes(*pair),
                Endian::Big => u16::from_be_bytes(*pair),
            })
            .collect();
        let text = String::from_utf16_lossy(&units);
        self.node(Node::new(Value::Str(text)), "ecstring", addr)
    }

    fn read_list(
        &mut self,
        addr: usize,
        type_id: u16,
        depth: usize,
        field_at: usize,
    ) -> Result<NodeId> {
        let count = u32_at(self.input, addr, self.endian)
            .ok_or_else(|| parse_error("a list length is cut short", addr))?;
        let count = usize::try_from(count)
            .map_err(|_| parse_error("a list has too many elements", addr))?;
        let start = addr.saturating_add(4);
        let mut items = Vec::with_capacity(count);
        if type_id == 0xFFFF {
            let bytes = count
                .checked_mul(8)
                .ok_or_else(|| parse_error("a generic list overflows", addr))?;
            let end = start
                .checked_add(bytes)
                .ok_or_else(|| parse_error("a generic list overflows", addr))?;
            if end > self.input.len() {
                return Err(parse_error("a generic list is cut short", start));
            }
            for position in 0..count {
                let at = start
                    .checked_add(position.saturating_mul(8))
                    .ok_or_else(|| parse_error("a generic list overflows", start))?;
                let element_kind = u32_at(self.input, at, self.endian)
                    .ok_or_else(|| parse_error("a generic list element is cut short", at))?;
                let reference = u32_at(self.input, at.saturating_add(4), self.endian)
                    .ok_or_else(|| parse_error("a generic list element is cut short", at))?;
                let resolved =
                    self.data_offset
                        .checked_add(usize::try_from(reference).map_err(|_| {
                            parse_error("a generic list reference is too large", at)
                        })?)
                        .ok_or_else(|| parse_error("a generic list reference overflows", at))?;
                items.push(self.read_value(
                    resolved,
                    element_kind & !u32::from(FLAG_REFERENCE),
                    0,
                    depth,
                    at,
                )?);
            }
        } else {
            let size = fixed_size(type_id).ok_or_else(|| {
                unsupported(
                    format!("a GFF4 list of type {type_id} is not supported"),
                    field_at,
                )
            })?;
            let bytes = count
                .checked_mul(size)
                .ok_or_else(|| parse_error("a list overflows", addr))?;
            let end = start
                .checked_add(bytes)
                .ok_or_else(|| parse_error("a list overflows", addr))?;
            if end > self.input.len() {
                return Err(parse_error("a list is cut short", start));
            }
            for position in 0..count {
                let at = start
                    .checked_add(position.saturating_mul(size))
                    .ok_or_else(|| parse_error("a list overflows", start))?;
                items.push(self.read_scalar(at, type_id, at)?);
            }
        }
        let mut node = Node::new(Value::Array(items));
        node.type_hint = Some(TypeHint::new(format!("{HINT_PREFIX}list")));
        node.flags.insert(NodeFlags::READ_ONLY);
        self.doc
            .push(node)
            .map_err(|error| error.with_offset(len_u64(addr)))
    }
}

fn fixed_size(type_id: u16) -> Option<usize> {
    match type_id {
        0 | 1 => Some(1),
        2 | 3 => Some(2),
        4 | 5 | 8 => Some(4),
        6 | 7 | 9 | 17 => Some(8),
        10 => Some(12),
        12 | 13 | 15 => Some(16),
        16 => Some(64),
        _ => None,
    }
}

pub(super) fn read_all(input: &[u8], ctx: &DecodeCtx<'_>) -> Result<(SaveDoc, Params)> {
    if input.len() < HEADER_LEN || input.get(..4) != Some(b"GFF ") {
        return Err(parse_error("not a GFF4 file", 0));
    }
    let Some(version) = super::version(input) else {
        return Err(parse_error("the GFF4 version is not V4.0 or V4.1", 4));
    };
    let endian = platform_endian(input);
    let struct_count =
        u32_at(input, 20, endian).ok_or_else(|| parse_error("the GFF4 header is cut short", 20))?;
    let struct_count = usize::try_from(struct_count)
        .map_err(|_| parse_error("the struct count is too large", 20))?;
    if struct_count == 0 {
        return Err(parse_error("the GFF4 file has no top-level struct", 20));
    }
    let data_offset =
        u32_at(input, 24, endian).ok_or_else(|| parse_error("the GFF4 header is cut short", 24))?;
    let data_offset = usize::try_from(data_offset)
        .map_err(|_| parse_error("the data offset is too large", 24))?;
    let struct_bytes = struct_count
        .checked_mul(STRUCT_LEN)
        .ok_or_else(|| parse_error("the struct array overflows", HEADER_LEN))?;
    let struct_end = HEADER_LEN
        .checked_add(struct_bytes)
        .ok_or_else(|| parse_error("the struct array overflows", HEADER_LEN))?;
    if struct_end > input.len() {
        return Err(parse_error("the struct array is cut short", HEADER_LEN));
    }
    if data_offset < struct_end || data_offset > input.len() {
        return Err(parse_error("the data block offset is outside the file", 24));
    }
    let mut data_bases = Vec::with_capacity(struct_count);
    let mut position = data_offset;
    for index in 0..struct_count {
        let at = HEADER_LEN
            .checked_add(index.saturating_mul(STRUCT_LEN))
            .ok_or_else(|| parse_error("the struct array overflows", HEADER_LEN))?;
        let size = u32_at(input, at.saturating_add(12), endian)
            .ok_or_else(|| parse_error("a struct entry is cut short", at))?;
        let size =
            usize::try_from(size).map_err(|_| parse_error("a struct size is too large", at))?;
        data_bases.push(position);
        position = position
            .checked_add(size)
            .ok_or_else(|| parse_error("the data block overflows", at))?;
    }
    if position > input.len() {
        return Err(parse_error(
            "the struct data lies beyond the end of the file",
            data_offset,
        ));
    }
    let file_type = input.get(12..16).unwrap_or_default();
    let tag: String = file_type
        .iter()
        .map(|&byte| char::from(byte))
        .collect::<String>()
        .trim_end()
        .to_owned();
    let mut reader = Reader {
        input,
        endian,
        struct_count,
        data_offset,
        data_bases,
        doc: ctx.new_doc(),
        ctx,
        stack: Vec::new(),
    };
    let root = reader.read_struct(0, 0)?;
    reader
        .doc
        .set_type_hint(
            root,
            Some(TypeHint::new(format!("{HINT_PREFIX}root:{tag}:{version}"))),
        )
        .map_err(|error| error.with_offset(0))?;
    reader.doc.set_root(root);
    let mut params = Params::new();
    params.set(P_ORIGINAL, input.to_vec());
    params.set("gff4:version", version);
    Ok((reader.doc, params))
}
