use saveprism_core::limits::len_u64;
use saveprism_core::{
    ByteSpan, DecodeCtx, Error, IntWidth, Node, NodeFlags, NodeId, Params, Result, SaveDoc,
    TypeHint, Value,
};

use super::{
    FIELD_LEN, GFF_PARSE, GFF_UNSUPPORTED, HEADER_LEN, HINT_PREFIX, LABEL_LEN, P_ORIGINAL,
    STRUCT_LEN, T_BYTE, T_CEXOLOCSTRING, T_CEXOSTRING, T_CHAR, T_DOUBLE, T_DWORD, T_DWORD64,
    T_FLOAT, T_INT, T_INT64, T_LIST, T_RESREF, T_SHORT, T_STRUCT, T_VOID, T_WORD,
};

pub(crate) fn u8_at(input: &[u8], offset: usize) -> Option<u8> {
    input.get(offset).copied()
}

pub(crate) fn u16_at(input: &[u8], offset: usize) -> Option<u16> {
    let bytes = input.get(offset..offset.checked_add(2)?)?;
    Some(u16::from_le_bytes([*bytes.first()?, *bytes.get(1)?]))
}

pub(crate) fn u32_at(input: &[u8], offset: usize) -> Option<u32> {
    let bytes = input.get(offset..offset.checked_add(4)?)?;
    Some(u32::from_le_bytes([
        *bytes.first()?,
        *bytes.get(1)?,
        *bytes.get(2)?,
        *bytes.get(3)?,
    ]))
}

pub(crate) fn u64_at(input: &[u8], offset: usize) -> Option<u64> {
    let bytes = input.get(offset..offset.checked_add(8)?)?;
    Some(u64::from_le_bytes([
        *bytes.first()?,
        *bytes.get(1)?,
        *bytes.get(2)?,
        *bytes.get(3)?,
        *bytes.get(4)?,
        *bytes.get(5)?,
        *bytes.get(6)?,
        *bytes.get(7)?,
    ]))
}

fn parse_error(message: impl Into<String>, offset: usize) -> Error {
    Error::new(GFF_PARSE, message).with_offset(len_u64(offset))
}

fn unsupported(message: impl Into<String>, offset: usize) -> Error {
    Error::new(GFF_UNSUPPORTED, message).with_offset(len_u64(offset))
}

/// A decoded text string and how it was stored, so the writer can use the same encoding.
fn text(bytes: &[u8]) -> (String, bool) {
    match std::str::from_utf8(bytes) {
        Ok(value) => (value.to_owned(), false),
        Err(_) => (bytes.iter().map(|&byte| char::from(byte)).collect(), true),
    }
}

fn label_text(bytes: &[u8]) -> String {
    let end = bytes
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(bytes.len());
    bytes
        .get(..end)
        .unwrap_or_default()
        .iter()
        .map(|&byte| char::from(byte))
        .collect()
}

#[derive(Clone, Copy, Debug)]
struct Header {
    struct_offset: usize,
    struct_count: usize,
    field_offset: usize,
    field_count: usize,
    label_offset: usize,
    label_count: usize,
    data_offset: usize,
    data_count: usize,
    field_indices_offset: usize,
    field_indices_count: usize,
    list_indices_offset: usize,
    list_indices_count: usize,
}

#[derive(Clone, Copy, Debug)]
struct StructEntry {
    kind: u32,
    data_or_offset: u32,
    field_count: u32,
}

#[derive(Clone, Copy, Debug)]
struct FieldEntry {
    kind: u32,
    label: u32,
    data_or_offset: u32,
}

fn section(
    input: &[u8],
    offset: u32,
    count: u32,
    item: usize,
    name: &str,
) -> Result<(usize, usize)> {
    let offset =
        usize::try_from(offset).map_err(|_| parse_error("section offset is too large", 0))?;
    let count = usize::try_from(count).map_err(|_| parse_error("section count is too large", 0))?;
    let bytes = count
        .checked_mul(item)
        .ok_or_else(|| parse_error("section size overflows", offset))?;
    let end = offset
        .checked_add(bytes)
        .ok_or_else(|| parse_error("section end overflows", offset))?;
    if end > input.len() {
        return Err(parse_error(
            format!("the {name} section lies beyond the end of the file"),
            offset,
        ));
    }
    Ok((offset, end))
}

fn header(input: &[u8]) -> Result<Header> {
    if input.len() < HEADER_LEN {
        return Err(parse_error("the file is shorter than a GFF header", 0));
    }
    if input.get(4..8) != Some(super::VERSION) {
        return Err(parse_error("the GFF version is not V3.2", 4));
    }
    let get = |offset: usize| {
        u32_at(input, offset).ok_or_else(|| parse_error("the GFF header is cut short", offset))
    };
    let (struct_offset, struct_end) = section(input, get(8)?, get(12)?, STRUCT_LEN, "struct")?;
    let (field_offset, field_end) = section(input, get(16)?, get(20)?, FIELD_LEN, "field")?;
    let (label_offset, label_end) = section(input, get(24)?, get(28)?, LABEL_LEN, "label")?;
    let (data_offset, data_end) = section(input, get(32)?, get(36)?, 1, "field data")?;
    let (field_indices_offset, field_indices_end) =
        section(input, get(40)?, get(44)?, 1, "field indices")?;
    let (list_indices_offset, list_indices_end) =
        section(input, get(48)?, get(52)?, 1, "list indices")?;
    for (start, end, name) in [
        (struct_offset, struct_end, "struct"),
        (field_offset, field_end, "field"),
        (label_offset, label_end, "label"),
        (data_offset, data_end, "field data"),
        (field_indices_offset, field_indices_end, "field indices"),
        (list_indices_offset, list_indices_end, "list indices"),
    ] {
        if start > end {
            return Err(parse_error(
                format!("the {name} section is inverted"),
                start,
            ));
        }
    }
    Ok(Header {
        struct_offset,
        struct_count: get(12)? as usize,
        field_offset,
        field_count: get(20)? as usize,
        label_offset,
        label_count: get(28)? as usize,
        data_offset,
        data_count: get(36)? as usize,
        field_indices_offset,
        field_indices_count: get(44)? as usize,
        list_indices_offset,
        list_indices_count: get(52)? as usize,
    })
}

struct Reader<'a, 'c> {
    input: &'a [u8],
    header: Header,
    doc: SaveDoc,
    ctx: &'c DecodeCtx<'a>,
    stack: Vec<u32>,
}

impl<'a> Reader<'a, '_> {
    fn struct_entry(&self, index: u32) -> Result<StructEntry> {
        let index =
            usize::try_from(index).map_err(|_| parse_error("struct index is too large", 0))?;
        if index >= self.header.struct_count {
            return Err(parse_error("a struct index is out of range", index));
        }
        let at = self
            .header
            .struct_offset
            .checked_add(index.saturating_mul(STRUCT_LEN))
            .ok_or_else(|| parse_error("struct offset overflows", index))?;
        Ok(StructEntry {
            kind: u32_at(self.input, at)
                .ok_or_else(|| parse_error("a struct entry is cut short", at))?,
            data_or_offset: u32_at(self.input, at.saturating_add(4))
                .ok_or_else(|| parse_error("a struct entry is cut short", at))?,
            field_count: u32_at(self.input, at.saturating_add(8))
                .ok_or_else(|| parse_error("a struct entry is cut short", at))?,
        })
    }

    fn field_entry(&self, index: u32) -> Result<(FieldEntry, usize)> {
        let index =
            usize::try_from(index).map_err(|_| parse_error("field index is too large", 0))?;
        if index >= self.header.field_count {
            return Err(parse_error("a field index is out of range", index));
        }
        let at = self
            .header
            .field_offset
            .checked_add(index.saturating_mul(FIELD_LEN))
            .ok_or_else(|| parse_error("field offset overflows", index))?;
        Ok((
            FieldEntry {
                kind: u32_at(self.input, at)
                    .ok_or_else(|| parse_error("a field entry is cut short", at))?,
                label: u32_at(self.input, at.saturating_add(4))
                    .ok_or_else(|| parse_error("a field entry is cut short", at))?,
                data_or_offset: u32_at(self.input, at.saturating_add(8))
                    .ok_or_else(|| parse_error("a field entry is cut short", at))?,
            },
            at,
        ))
    }

    fn label(&self, index: u32) -> Result<String> {
        let index =
            usize::try_from(index).map_err(|_| parse_error("label index is too large", 0))?;
        if index >= self.header.label_count {
            return Err(parse_error("a label index is out of range", index));
        }
        let at = self
            .header
            .label_offset
            .checked_add(index.saturating_mul(LABEL_LEN))
            .ok_or_else(|| parse_error("label offset overflows", index))?;
        let bytes = self
            .input
            .get(at..at.saturating_add(LABEL_LEN))
            .ok_or_else(|| parse_error("a label is cut short", at))?;
        Ok(label_text(bytes))
    }

    fn data(&self, offset: u32, len: usize) -> Result<usize> {
        let offset = usize::try_from(offset)
            .map_err(|_| parse_error("a field data offset is too large", 0))?;
        let end = offset
            .checked_add(len)
            .ok_or_else(|| parse_error("a field data range overflows", offset))?;
        if end > self.header.data_count {
            return Err(parse_error(
                "a field data range lies beyond the field data block",
                self.header.data_offset.saturating_add(offset),
            ));
        }
        self.header
            .data_offset
            .checked_add(offset)
            .ok_or_else(|| parse_error("a field data offset overflows", offset))
    }

    fn span(&self, start: usize, end: usize) -> ByteSpan {
        ByteSpan {
            start: len_u64(start),
            end: len_u64(end),
        }
    }

    fn scalar(&mut self, value: Value, hint: &str, start: usize, end: usize) -> Result<NodeId> {
        let mut node = Node::new(value);
        node.type_hint = Some(TypeHint::new(format!("{HINT_PREFIX}{hint}")));
        node.span = Some(self.span(start, end));
        self.doc
            .push(node)
            .map_err(|error| error.with_offset(len_u64(start)))
    }

    fn string_node(
        &mut self,
        bytes: &[u8],
        hint: &str,
        start: usize,
        end: usize,
    ) -> Result<NodeId> {
        let (value, latin1) = text(bytes);
        let hint = if latin1 {
            format!("{hint}:latin1")
        } else {
            hint.to_owned()
        };
        let mut node = Node::new(Value::Str(value));
        node.type_hint = Some(TypeHint::new(format!("{HINT_PREFIX}{hint}")));
        node.span = Some(self.span(start, end));
        self.doc
            .push(node)
            .map_err(|error| error.with_offset(len_u64(start)))
    }

    fn read_struct(&mut self, index: u32, depth: usize) -> Result<NodeId> {
        self.ctx.check_depth(depth)?;
        if self.stack.contains(&index) {
            return Err(parse_error(
                "a struct contains itself through a list",
                self.header.struct_offset,
            ));
        }
        let entry = self.struct_entry(index)?;
        let count = usize::try_from(entry.field_count)
            .map_err(|_| parse_error("a struct has too many fields", 0))?;
        let indices = if count == 0 {
            Vec::new()
        } else if count == 1 {
            vec![entry.data_or_offset]
        } else {
            let start = usize::try_from(entry.data_or_offset).map_err(|_| {
                parse_error(
                    "a field index offset is too large",
                    self.header.field_indices_offset,
                )
            })?;
            let bytes = count
                .checked_mul(4)
                .ok_or_else(|| parse_error("a field index array overflows", start))?;
            let end = start
                .checked_add(bytes)
                .ok_or_else(|| parse_error("a field index array overflows", start))?;
            if end > self.header.field_indices_count {
                return Err(parse_error(
                    "a field index array lies beyond the field indices section",
                    self.header.field_indices_offset.saturating_add(start),
                ));
            }
            let base = self
                .header
                .field_indices_offset
                .checked_add(start)
                .ok_or_else(|| parse_error("a field index offset overflows", start))?;
            let mut indices = Vec::with_capacity(count);
            for position in 0..count {
                let at = base
                    .checked_add(position.saturating_mul(4))
                    .ok_or_else(|| parse_error("a field index offset overflows", base))?;
                indices.push(
                    u32_at(self.input, at)
                        .ok_or_else(|| parse_error("a field index is cut short", at))?,
                );
            }
            indices
        };
        let struct_kind = entry.kind;
        self.stack.push(index);
        let mut entries = Vec::with_capacity(indices.len());
        for field_index in indices {
            let (field, field_at) = self.field_entry(field_index)?;
            let label = self.label(field.label)?;
            let child = self.read_field(field, field_at, depth.saturating_add(1))?;
            entries.push((label, child));
        }
        self.stack.pop();
        let mut node = Node::new(Value::Object(entries));
        node.type_hint = Some(TypeHint::new(format!(
            "{HINT_PREFIX}struct:{struct_kind:08X}"
        )));
        node.flags.insert(NodeFlags::READ_ONLY);
        self.doc.push(node)
    }

    fn read_field(&mut self, field: FieldEntry, field_at: usize, depth: usize) -> Result<NodeId> {
        self.ctx.check_depth(depth)?;
        let value_at = field_at.saturating_add(8);
        match field.kind {
            T_BYTE => {
                let value = u8_at(self.input, value_at)
                    .ok_or_else(|| parse_error("a BYTE field is cut short", value_at))?;
                self.scalar(
                    Value::Int {
                        v: i128::from(value),
                        width: IntWidth::U8,
                    },
                    "byte",
                    value_at,
                    value_at.saturating_add(1),
                )
            }
            T_CHAR => {
                let value = u8_at(self.input, value_at)
                    .ok_or_else(|| parse_error("a CHAR field is cut short", value_at))?
                    as i8;
                self.scalar(
                    Value::Int {
                        v: i128::from(value),
                        width: IntWidth::I8,
                    },
                    "char",
                    value_at,
                    value_at.saturating_add(1),
                )
            }
            T_WORD => {
                let value = u16_at(self.input, value_at)
                    .ok_or_else(|| parse_error("a WORD field is cut short", value_at))?;
                self.scalar(
                    Value::Int {
                        v: i128::from(value),
                        width: IntWidth::U16,
                    },
                    "word",
                    value_at,
                    value_at.saturating_add(2),
                )
            }
            T_SHORT => {
                let value = u16_at(self.input, value_at)
                    .ok_or_else(|| parse_error("a SHORT field is cut short", value_at))?
                    as i16;
                self.scalar(
                    Value::Int {
                        v: i128::from(value),
                        width: IntWidth::I16,
                    },
                    "short",
                    value_at,
                    value_at.saturating_add(2),
                )
            }
            T_DWORD => {
                let value = u32_at(self.input, value_at)
                    .ok_or_else(|| parse_error("a DWORD field is cut short", value_at))?;
                self.scalar(
                    Value::Int {
                        v: i128::from(value),
                        width: IntWidth::U32,
                    },
                    "dword",
                    value_at,
                    value_at.saturating_add(4),
                )
            }
            T_INT => {
                let value = u32_at(self.input, value_at)
                    .ok_or_else(|| parse_error("an INT field is cut short", value_at))?
                    as i32;
                self.scalar(
                    Value::Int {
                        v: i128::from(value),
                        width: IntWidth::I32,
                    },
                    "int",
                    value_at,
                    value_at.saturating_add(4),
                )
            }
            T_FLOAT => {
                let bits = u32_at(self.input, value_at)
                    .ok_or_else(|| parse_error("a FLOAT field is cut short", value_at))?;
                self.scalar(
                    Value::Float {
                        v: f32::from_bits(bits) as f64,
                        width: saveprism_core::FloatWidth::F32,
                    },
                    "float",
                    value_at,
                    value_at.saturating_add(4),
                )
            }
            T_DWORD64 => {
                let start = self.data(field.data_or_offset, 8)?;
                let value = u64_at(self.input, start)
                    .ok_or_else(|| parse_error("a DWORD64 field is cut short", start))?;
                self.scalar(
                    Value::Int {
                        v: i128::from(value),
                        width: IntWidth::U64,
                    },
                    "dword64",
                    start,
                    start.saturating_add(8),
                )
            }
            T_INT64 => {
                let start = self.data(field.data_or_offset, 8)?;
                let value = u64_at(self.input, start)
                    .ok_or_else(|| parse_error("an INT64 field is cut short", start))?
                    as i64;
                self.scalar(
                    Value::Int {
                        v: i128::from(value),
                        width: IntWidth::I64,
                    },
                    "int64",
                    start,
                    start.saturating_add(8),
                )
            }
            T_DOUBLE => {
                let start = self.data(field.data_or_offset, 8)?;
                let bits = u64_at(self.input, start)
                    .ok_or_else(|| parse_error("a DOUBLE field is cut short", start))?;
                self.scalar(
                    Value::Float {
                        v: f64::from_bits(bits),
                        width: saveprism_core::FloatWidth::F64,
                    },
                    "double",
                    start,
                    start.saturating_add(8),
                )
            }
            T_CEXOSTRING => {
                let start = self.data(field.data_or_offset, 4)?;
                let length = u32_at(self.input, start)
                    .ok_or_else(|| parse_error("a CExoString length is cut short", start))?;
                let length = usize::try_from(length)
                    .map_err(|_| parse_error("a CExoString is too long", start))?;
                let bytes_at = start.saturating_add(4);
                let end = bytes_at
                    .checked_add(length)
                    .ok_or_else(|| parse_error("a CExoString overflows", start))?;
                if end
                    > self
                        .header
                        .data_offset
                        .saturating_add(self.header.data_count)
                {
                    return Err(parse_error(
                        "a CExoString lies beyond the field data block",
                        bytes_at,
                    ));
                }
                let bytes = self
                    .input
                    .get(bytes_at..end)
                    .ok_or_else(|| parse_error("a CExoString is cut short", bytes_at))?;
                self.string_node(bytes, "cexostring", bytes_at, end)
            }
            T_RESREF => {
                let start = self.data(field.data_or_offset, 1)?;
                let length = u8_at(self.input, start)
                    .ok_or_else(|| parse_error("a ResRef length is cut short", start))?;
                let length = usize::from(length);
                if length > 16 {
                    return Err(parse_error("a ResRef is longer than 16 bytes", start));
                }
                let bytes_at = start.saturating_add(1);
                let end = bytes_at
                    .checked_add(length)
                    .ok_or_else(|| parse_error("a ResRef overflows", start))?;
                if end
                    > self
                        .header
                        .data_offset
                        .saturating_add(self.header.data_count)
                {
                    return Err(parse_error(
                        "a ResRef lies beyond the field data block",
                        bytes_at,
                    ));
                }
                let bytes = self
                    .input
                    .get(bytes_at..end)
                    .ok_or_else(|| parse_error("a ResRef is cut short", bytes_at))?;
                self.string_node(bytes, "resref", bytes_at, end)
            }
            T_CEXOLOCSTRING => self.read_locstring(field.data_or_offset),
            T_VOID => {
                let start = self.data(field.data_or_offset, 4)?;
                let length = u32_at(self.input, start)
                    .ok_or_else(|| parse_error("a VOID length is cut short", start))?;
                let length = usize::try_from(length)
                    .map_err(|_| parse_error("a VOID field is too large", start))?;
                let bytes_at = start.saturating_add(4);
                let end = bytes_at
                    .checked_add(length)
                    .ok_or_else(|| parse_error("a VOID field overflows", start))?;
                if end
                    > self
                        .header
                        .data_offset
                        .saturating_add(self.header.data_count)
                {
                    return Err(parse_error(
                        "a VOID field lies beyond the field data block",
                        bytes_at,
                    ));
                }
                let bytes = self
                    .input
                    .get(bytes_at..end)
                    .ok_or_else(|| parse_error("a VOID field is cut short", bytes_at))?;
                let mut node = Node::new(Value::Bytes(bytes.to_vec()));
                node.type_hint = Some(TypeHint::new(format!("{HINT_PREFIX}void")));
                node.flags.insert(NodeFlags::READ_ONLY);
                self.doc
                    .push(node)
                    .map_err(|error| error.with_offset(len_u64(bytes_at)))
            }
            T_STRUCT => self.read_struct(field.data_or_offset, depth),
            T_LIST => self.read_list(field.data_or_offset, depth),
            other => Err(unsupported(
                format!("GFF field type {other} is not supported"),
                field_at,
            )),
        }
    }

    fn read_locstring(&mut self, offset: u32) -> Result<NodeId> {
        let start = self.data(offset, 4)?;
        let total = u32_at(self.input, start)
            .ok_or_else(|| parse_error("a CExoLocString size is cut short", start))?;
        let total = usize::try_from(total)
            .map_err(|_| parse_error("a CExoLocString is too large", start))?;
        let after_size = start.saturating_add(4);
        let end = after_size
            .checked_add(total)
            .ok_or_else(|| parse_error("a CExoLocString overflows", start))?;
        if end > self.input.len()
            || end
                > self
                    .header
                    .data_offset
                    .saturating_add(self.header.data_count)
        {
            return Err(parse_error(
                "a CExoLocString lies beyond the field data block",
                after_size,
            ));
        }
        let strref = u32_at(self.input, after_size)
            .ok_or_else(|| parse_error("a CExoLocString StringRef is cut short", after_size))?;
        let count_at = after_size.saturating_add(4);
        let count = u32_at(self.input, count_at)
            .ok_or_else(|| parse_error("a CExoLocString count is cut short", count_at))?;
        let count = usize::try_from(count)
            .map_err(|_| parse_error("a CExoLocString has too many strings", count_at))?;
        let mut position = count_at.saturating_add(4);
        let mut strings = Vec::with_capacity(count);
        for _ in 0..count {
            let id = u32_at(self.input, position)
                .ok_or_else(|| parse_error("a CExoLocString SubString is cut short", position))?;
            let length_at = position.saturating_add(4);
            let length = u32_at(self.input, length_at)
                .ok_or_else(|| parse_error("a CExoLocString SubString is cut short", length_at))?;
            let length = usize::try_from(length)
                .map_err(|_| parse_error("a CExoLocString SubString is too long", length_at))?;
            let bytes_at = length_at.saturating_add(4);
            let bytes_end = bytes_at
                .checked_add(length)
                .ok_or_else(|| parse_error("a CExoLocString SubString overflows", bytes_at))?;
            if bytes_end > end {
                return Err(parse_error(
                    "a CExoLocString SubString lies beyond the string",
                    bytes_at,
                ));
            }
            let bytes = self
                .input
                .get(bytes_at..bytes_end)
                .ok_or_else(|| parse_error("a CExoLocString SubString is cut short", bytes_at))?;
            let text = self.string_node(bytes, "locstring-text", bytes_at, bytes_end)?;
            let mut id_node = Node::new(Value::Int {
                v: i128::from(id),
                width: IntWidth::U32,
            });
            id_node.type_hint = Some(TypeHint::new(format!("{HINT_PREFIX}lang")));
            id_node.span = Some(self.span(position, position.saturating_add(4)));
            id_node.flags.insert(NodeFlags::READ_ONLY);
            let id_node = self
                .doc
                .push(id_node)
                .map_err(|error| error.with_offset(len_u64(position)))?;
            let mut entry = Node::new(Value::Object(vec![
                ("id".to_owned(), id_node),
                ("text".to_owned(), text),
            ]));
            entry.type_hint = Some(TypeHint::new(format!("{HINT_PREFIX}locstring-entry")));
            entry.flags.insert(NodeFlags::READ_ONLY);
            strings.push(
                self.doc
                    .push(entry)
                    .map_err(|error| error.with_offset(len_u64(position)))?,
            );
            position = bytes_end;
        }
        if position != end {
            return Err(parse_error(
                "a CExoLocString size does not match its contents",
                start,
            ));
        }
        let mut strref_node = Node::new(Value::Int {
            v: i128::from(strref),
            width: IntWidth::U32,
        });
        strref_node.type_hint = Some(TypeHint::new(format!("{HINT_PREFIX}strref")));
        strref_node.span = Some(self.span(after_size, after_size.saturating_add(4)));
        strref_node.flags.insert(NodeFlags::READ_ONLY);
        let strref_node = self
            .doc
            .push(strref_node)
            .map_err(|error| error.with_offset(len_u64(after_size)))?;
        let mut list = Node::new(Value::Array(strings));
        list.type_hint = Some(TypeHint::new(format!("{HINT_PREFIX}locstring-strings")));
        list.flags.insert(NodeFlags::READ_ONLY);
        let list = self
            .doc
            .push(list)
            .map_err(|error| error.with_offset(len_u64(start)))?;
        let mut node = Node::new(Value::Object(vec![
            ("strref".to_owned(), strref_node),
            ("strings".to_owned(), list),
        ]));
        node.type_hint = Some(TypeHint::new(format!("{HINT_PREFIX}cexolocstring")));
        node.flags.insert(NodeFlags::READ_ONLY);
        self.doc
            .push(node)
            .map_err(|error| error.with_offset(len_u64(start)))
    }

    fn read_list(&mut self, offset: u32, depth: usize) -> Result<NodeId> {
        let start =
            usize::try_from(offset).map_err(|_| parse_error("a list offset is too large", 0))?;
        let count_at = start
            .checked_add(self.header.list_indices_offset)
            .ok_or_else(|| parse_error("a list offset overflows", start))?;
        if start.saturating_add(4) > self.header.list_indices_count {
            return Err(parse_error(
                "a list lies beyond the list indices section",
                count_at,
            ));
        }
        let count = u32_at(self.input, count_at)
            .ok_or_else(|| parse_error("a list length is cut short", count_at))?;
        let count = usize::try_from(count)
            .map_err(|_| parse_error("a list has too many elements", count_at))?;
        let bytes = count
            .checked_mul(4)
            .ok_or_else(|| parse_error("a list overflows", count_at))?;
        let end = start
            .checked_add(4)
            .and_then(|position| position.checked_add(bytes))
            .ok_or_else(|| parse_error("a list overflows", count_at))?;
        if end > self.header.list_indices_count {
            return Err(parse_error(
                "a list lies beyond the list indices section",
                count_at,
            ));
        }
        let mut items = Vec::with_capacity(count);
        for position in 0..count {
            let at = count_at
                .checked_add(4)
                .and_then(|base| base.checked_add(position.saturating_mul(4)))
                .ok_or_else(|| parse_error("a list offset overflows", count_at))?;
            let index = u32_at(self.input, at)
                .ok_or_else(|| parse_error("a list element is cut short", at))?;
            items.push(self.read_struct(index, depth)?);
        }
        let mut node = Node::new(Value::Array(items));
        node.type_hint = Some(TypeHint::new(format!("{HINT_PREFIX}list")));
        node.flags.insert(NodeFlags::READ_ONLY);
        self.doc
            .push(node)
            .map_err(|error| error.with_offset(len_u64(count_at)))
    }
}

pub(super) fn read_all(input: &[u8], ctx: &DecodeCtx<'_>) -> Result<(SaveDoc, Params)> {
    let header = header(input)?;
    if header.struct_count == 0 {
        return Err(parse_error("the GFF file has no top-level struct", 8));
    }
    let file_type = input.get(..4).unwrap_or_default();
    let tag: String = file_type
        .iter()
        .map(|&byte| char::from(byte))
        .collect::<String>()
        .trim_end()
        .to_owned();
    let mut reader = Reader {
        input,
        header,
        doc: ctx.new_doc(),
        ctx,
        stack: Vec::new(),
    };
    let root = reader.read_struct(0, 0)?;
    reader
        .doc
        .set_type_hint(
            root,
            Some(TypeHint::new(format!("{HINT_PREFIX}root:{tag}"))),
        )
        .map_err(|error| error.with_offset(0))?;
    reader.doc.set_root(root);
    let mut params = Params::new();
    params.set(P_ORIGINAL, input.to_vec());
    params.set("gff:file_type", file_type.to_vec());
    params.set("gff:version", super::VERSION.to_vec());
    Ok((reader.doc, params))
}
