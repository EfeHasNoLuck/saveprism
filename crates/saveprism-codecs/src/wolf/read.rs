//! Reads an unencrypted WOLF RPG save into the universal tree.
//!
//! Only the pieces players edit are exposed; every other part is parsed just far enough to
//! advance the cursor to the next one. Lengths are checked against the remaining input before
//! anything is allocated, so a truncated or hostile file fails with `SP-WOLF-001`.

use saveprism_core::{
    ByteSpan, DecodeCtx, IntWidth, Node, NodeFlags, NodeId, ParamValue, Params, Result, SaveDoc,
    TypeHint, Value,
};

use super::{
    HEADER_LEN, HINT_PREFIX, MARKER, MAX_VERSION, P_ORIGINAL, UTF8_MARKER, WOLF_ENCRYPTED,
    parse_error,
};

/// A bank of numbers and where each number lives in the file.
struct Bank {
    values: Vec<Number>,
}

struct Number {
    value: i128,
    width: IntWidth,
    span: ByteSpan,
}

pub(crate) fn read_all(input: &[u8], ctx: &DecodeCtx<'_>) -> Result<(SaveDoc, Params)> {
    if input.len() <= HEADER_LEN {
        return Err(parse_error(
            "a WOLF RPG save is shorter than its 0x14 byte header",
        ));
    }
    if input.get(HEADER_LEN).copied() != Some(MARKER) {
        return Err(saveprism_core::Error::new(
            WOLF_ENCRYPTED,
            "encrypted Wolf RPG save; not supported yet",
        ));
    }

    let is_utf8 = input.get(6).copied() == Some(UTF8_MARKER);
    let mut reader = Reader::new(input, HEADER_LEN + 1);
    let name = reader.mem_string(2, is_utf8)?;
    let version = reader.u16()?;
    if version == 0 || version > MAX_VERSION {
        return Err(parse_error(format!(
            "WOLF RPG file version 0x{version:02X} is not supported (highest is 0x{MAX_VERSION:02X})"
        )));
    }

    let part1 = read_part1(&mut reader, version)?;
    read_part2(&mut reader, version)?;
    read_part3(&mut reader, version)?;
    read_part4(&mut reader, version)?;
    read_part5(&mut reader, version)?;
    let variable_db = read_variable_db(&mut reader)?;
    read_part7(&mut reader)?;

    if reader.pos + 1 != input.len() {
        return Err(parse_error(format!(
            "WOLF RPG save has {} unexpected bytes after its last part",
            input.len().saturating_sub(reader.pos + 1)
        )));
    }
    if reader.u8()? != MARKER {
        return Err(parse_error(
            "WOLF RPG save does not end with the 0x19 marker",
        ));
    }

    let doc = build_doc(ctx, name, version, part1, variable_db)?;
    let mut params = Params::new();
    params.set(P_ORIGINAL, ParamValue::Bytes(input.to_vec()));
    Ok((doc, params))
}

fn hint(text: impl Into<String>) -> TypeHint {
    TypeHint::new(format!("{HINT_PREFIX}{}", text.into()))
}

fn container(doc: &mut SaveDoc, value: Value, hint_text: &str) -> Result<NodeId> {
    let mut node = Node::new(value);
    node.type_hint = Some(hint(hint_text));
    node.flags.insert(NodeFlags::READ_ONLY);
    doc.push(node)
}

fn build_doc(
    ctx: &DecodeCtx<'_>,
    name: MemString,
    version: u16,
    part1: Part1,
    variable_db: Vec<VariableType>,
) -> Result<SaveDoc> {
    let mut doc = ctx.new_doc();

    let name_hint = name.hint();
    let name_id = match name.text {
        Some(text) => {
            let type_hint = name_hint;
            let mut node = Node::new(Value::Str(text));
            node.type_hint = Some(hint(type_hint));
            node.span = Some(name.span);
            doc.push(node)?
        }
        None => {
            let mut node = Node::new(Value::Bytes(name.raw));
            node.type_hint = Some(hint("bytes"));
            node.flags.insert(NodeFlags::READ_ONLY);
            doc.push(node)?
        }
    };

    let version_id = {
        let mut node = Node::new(Value::Int {
            v: i128::from(version),
            width: IntWidth::U16,
        });
        node.type_hint = Some(hint("version"));
        node.flags.insert(NodeFlags::READ_ONLY);
        doc.push(node)?
    };

    let switches_id = build_bank(&mut doc, &part1.switches, "switches", "switch")?;
    let variables_id = build_bank(&mut doc, &part1.variables, "variables", "variable")?;

    let mut type_entries = Vec::with_capacity(variable_db.len());
    for (type_index, ty) in variable_db.into_iter().enumerate() {
        let mut record_entries = Vec::with_capacity(ty.records.len());
        for (record_index, record) in ty.records.into_iter().enumerate() {
            let mut field_entries = Vec::with_capacity(record.fields.len());
            for (field_index, field) in record.fields.into_iter().enumerate() {
                let id = match field {
                    Field::Number { value, span } => {
                        let mut node = Node::new(Value::Int {
                            v: i128::from(value),
                            width: IntWidth::I32,
                        });
                        node.type_hint = Some(hint("field"));
                        node.span = Some(span);
                        doc.push(node)?
                    }
                    Field::Text(text) => {
                        let type_hint = text.hint();
                        let mut node = Node::new(Value::Str(text.text.unwrap_or_default()));
                        node.type_hint = Some(hint(type_hint));
                        node.span = Some(text.span);
                        doc.push(node)?
                    }
                    Field::Opaque(bytes) => {
                        let mut node = Node::new(Value::Bytes(bytes));
                        node.type_hint = Some(hint("bytes"));
                        node.flags.insert(NodeFlags::READ_ONLY);
                        doc.push(node)?
                    }
                };
                field_entries.push((format!("Field {field_index}"), id));
            }
            let id = container(&mut doc, Value::Object(field_entries), "record")?;
            record_entries.push((format!("Record {record_index}"), id));
        }
        let id = container(&mut doc, Value::Object(record_entries), "type")?;
        type_entries.push((format!("Type {type_index}"), id));
    }
    let db_id = container(&mut doc, Value::Object(type_entries), "variable-db")?;

    let mut root_node = Node::new(Value::Object(vec![
        ("Game name".to_string(), name_id),
        ("File version".to_string(), version_id),
        ("Switches".to_string(), switches_id),
        ("Variables".to_string(), variables_id),
        ("Variable DB".to_string(), db_id),
    ]));
    root_node.type_hint = Some(hint("save"));
    root_node.flags.insert(NodeFlags::READ_ONLY);
    let root = doc.push(root_node)?;
    doc.set_root(root);
    Ok(doc)
}

fn build_bank(doc: &mut SaveDoc, bank: &Bank, array_hint: &str, item_hint: &str) -> Result<NodeId> {
    let mut ids = Vec::with_capacity(bank.values.len());
    for number in &bank.values {
        let mut node = Node::new(Value::Int {
            v: number.value,
            width: number.width,
        });
        node.type_hint = Some(hint(item_hint));
        node.span = Some(number.span);
        ids.push(doc.push(node)?);
    }
    container(doc, Value::Array(ids), array_hint)
}

// ---------------------------------------------------------------------------------------------
// Cursor

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

    fn need(&self, count: usize) -> Result<()> {
        if count > self.remaining() {
            return Err(parse_error(format!(
                "WOLF RPG save is truncated: needed {count} bytes at offset 0x{:X}, {} left",
                self.pos,
                self.remaining()
            )));
        }
        Ok(())
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8]> {
        self.need(count)?;
        let start = self.pos;
        self.pos += count;
        self.data
            .get(start..self.pos)
            .ok_or_else(|| parse_error("internal: WOLF cursor left the input"))
    }

    fn u8(&mut self) -> Result<u8> {
        self.take(1)?
            .first()
            .copied()
            .ok_or_else(|| parse_error("internal: WOLF cursor read past the input"))
    }

    fn u16(&mut self) -> Result<u16> {
        let bytes: [u8; 2] = self
            .take(2)?
            .try_into()
            .map_err(|_| parse_error("internal: bad WOLF u16 read"))?;
        Ok(u16::from_le_bytes(bytes))
    }

    fn u32(&mut self) -> Result<u32> {
        let bytes: [u8; 4] = self
            .take(4)?
            .try_into()
            .map_err(|_| parse_error("internal: bad WOLF u32 read"))?;
        Ok(u32::from_le_bytes(bytes))
    }

    fn i32(&mut self) -> Result<i32> {
        Ok(self.u32()? as i32)
    }

    fn skip(&mut self, count: usize) -> Result<()> {
        self.take(count).map(|_| ())
    }

    fn skip_u32s(&mut self, count: u32) -> Result<()> {
        let bytes = usize::try_from(count)
            .ok()
            .and_then(|n| n.checked_mul(4))
            .ok_or_else(|| parse_error("a WOLF RPG array length is too large"))?;
        self.skip(bytes)
    }

    /// `MemData<T>`: a T-sized length followed by that many bytes.
    fn mem(&mut self, length_bytes: usize) -> Result<Mem<'_>> {
        let start = self.pos;
        let length = match length_bytes {
            1 => u32::from(self.u8()?),
            2 => u32::from(self.u16()?),
            4 => self.u32()?,
            _ => return Err(parse_error("internal: bad WOLF string length width")),
        };
        let length = usize::try_from(length)
            .map_err(|_| parse_error("a WOLF RPG string is too long for this platform"))?;
        self.need(length)?;
        let data = self.take(length)?;
        Ok(Mem {
            start,
            end: self.pos,
            data,
        })
    }

    fn mem_string(&mut self, length_bytes: usize, is_utf8: bool) -> Result<MemString> {
        let mem = self.mem(length_bytes)?;
        Ok(MemString::new(mem, length_bytes, is_utf8))
    }
}

struct Mem<'a> {
    start: usize,
    end: usize,
    data: &'a [u8],
}

struct MemString {
    raw: Vec<u8>,
    text: Option<String>,
    span: ByteSpan,
    length_bytes: usize,
    nul: bool,
}

impl MemString {
    fn new(mem: Mem<'_>, length_bytes: usize, is_utf8: bool) -> Self {
        let raw = mem.data.to_vec();
        let (content, nul) = match raw.last() {
            Some(0) => (
                raw.get(..raw.len().saturating_sub(1)).unwrap_or_default(),
                true,
            ),
            _ => (raw.as_slice(), false),
        };
        let text = if is_utf8 {
            std::str::from_utf8(content).ok().map(str::to_string)
        } else {
            None
        };
        Self {
            raw,
            text,
            span: ByteSpan {
                start: u64::try_from(mem.start).unwrap_or(u64::MAX),
                end: u64::try_from(mem.end).unwrap_or(u64::MAX),
            },
            length_bytes,
            nul,
        }
    }

    fn hint(&self) -> String {
        let width = if self.length_bytes == 2 { "u16" } else { "u32" };
        if self.nul {
            format!("str:{width}:nul")
        } else {
            format!("str:{width}")
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Parts

struct Part1 {
    switches: Bank,
    variables: Bank,
}

fn read_part1(reader: &mut Reader<'_>, version: u16) -> Result<Part1> {
    reader.skip_u32s(3)?;
    if version >= 0x69 {
        let rows = reader.u32()?;
        let cols = reader.u32()?;
        let check = reader.u32()?;
        if check != u32::MAX {
            reader.pos -= 4;
            let count = u64::from(rows)
                .checked_mul(u64::from(cols))
                .and_then(|n| n.checked_mul(3))
                .ok_or_else(|| parse_error("a WOLF RPG grid is too large"))?;
            let count =
                u32::try_from(count).map_err(|_| parse_error("a WOLF RPG grid is too large"))?;
            reader.skip_u32s(count)?;
        }
    }
    reader.mem(4)?;
    let strings = if version >= 0x8A {
        31
    } else if version >= 0x73 {
        15
    } else {
        7
    };
    for _ in 0..strings {
        reader.mem(4)?;
    }

    let switch_count = reader.u32()?;
    let mut switches = Vec::new();
    if switch_count > 0 {
        let (width, bytes) = if version >= 0x64 {
            (IntWidth::U32, 4)
        } else {
            (IntWidth::U8, 1)
        };
        switches.reserve(usize::try_from(switch_count).unwrap_or(0).min(4096));
        for _ in 0..switch_count {
            let start = reader.pos;
            let value = if width == IntWidth::U32 {
                i128::from(reader.u32()?)
            } else {
                i128::from(reader.u8()?)
            };
            switches.push(Number {
                value,
                width,
                span: ByteSpan {
                    start: u64::try_from(start).unwrap_or(u64::MAX),
                    end: u64::try_from(start + bytes).unwrap_or(u64::MAX),
                },
            });
        }
    }

    let map_count = reader.u32()?;
    for _ in 0..map_count {
        read_part1_1(reader, version)?;
    }

    if version >= 0x72 {
        reader.skip_u32s(11)?;
    }

    let variable_count = reader.u32()?;
    let variable_bytes = usize::try_from(variable_count)
        .ok()
        .and_then(|n| n.checked_mul(4))
        .ok_or_else(|| parse_error("a WOLF RPG variable bank is too large"))?;
    reader.need(variable_bytes)?;
    let mut variables = Vec::with_capacity(usize::try_from(variable_count).unwrap_or(0).min(4096));
    for _ in 0..variable_count {
        let start = reader.pos;
        let value = i128::from(reader.i32()?);
        variables.push(Number {
            value,
            width: IntWidth::I32,
            span: ByteSpan {
                start: u64::try_from(start).unwrap_or(u64::MAX),
                end: u64::try_from(start + 4).unwrap_or(u64::MAX),
            },
        });
    }

    Ok(Part1 {
        switches: Bank { values: switches },
        variables: Bank { values: variables },
    })
}

fn read_part1_1(reader: &mut Reader<'_>, version: u16) -> Result<()> {
    reader.skip_u32s(4)?;
    reader.u8()?;
    reader.u32()?;
    reader.mem(4)?;
    reader.skip(4 * 2 + 1 + 1)?;
    read_part1_1_1(reader)?;
    read_part1_1_1(reader)?;
    reader.skip(15)?;
    let count = reader.u32()?;
    reader.skip_u32s(count)?;
    if version >= 0x70 {
        reader.skip(2)?;
    }
    if version >= 0x73 {
        reader.skip_u32s(4)?;
    }
    if version >= 0x78 {
        reader.u32()?;
    }
    if version >= 0x85 {
        reader.skip_u32s(3)?;
    }
    if version >= 0x8A {
        let count = reader.u16()?;
        for _ in 0..count {
            reader.mem(2)?;
        }
        reader.u32()?;
        reader.u8()?;
        reader.skip_u32s(2)?;
    }
    if version >= 0x8B {
        reader.u32()?;
    }
    if version >= 0x8C {
        reader.u32()?;
    }
    Ok(())
}

fn read_part1_1_1(reader: &mut Reader<'_>) -> Result<()> {
    reader.skip(6)?;
    let count = reader.u32()?;
    if count > 0x10000 {
        return Err(parse_error("a WOLF RPG map record has too many entries"));
    }
    for _ in 0..count {
        reader.u8()?;
        let length = reader.u8()?;
        reader.skip_u32s(u32::from(length))?;
        let flags = reader.u8()?;
        reader.skip(usize::from(flags))?;
    }
    Ok(())
}

fn read_part2(reader: &mut Reader<'_>, version: u16) -> Result<()> {
    reader.skip(2)?;
    reader.skip_u32s(19)?;
    reader.skip(3 * 2)?;
    reader.u32()?;
    reader.skip(3 * 2)?;
    reader.skip_u32s(9)?;
    reader.u8()?;
    reader.u16()?;
    reader.skip_u32s(2)?;
    reader.skip(4 * 2)?;
    reader.skip_u32s(3)?;
    reader.u8()?;
    if version <= 96 {
        reader.u32()?;
    }
    reader.u32()?;
    if version >= 98 {
        reader.skip_u32s(2)?;
    }
    if version >= 100 {
        reader.skip_u32s(2)?;
        reader.u8()?;
        let count = reader.u32()?;
        for _ in 0..count {
            reader.mem(4)?;
        }
        let count = reader.u32()?;
        reader.skip_u32s(count)?;
        reader.skip_u32s(5)?;
    }
    if version >= 101 {
        reader.skip_u32s(6)?;
    }
    if version >= 102 {
        reader.u16()?;
    }
    if version >= 103 {
        reader.mem(2)?;
        reader.mem(2)?;
    }
    if version >= 104 {
        reader.u32()?;
    }
    if version >= 106 {
        reader.skip_u32s(3)?;
    }
    if version >= 108 {
        reader.mem(2)?;
        reader.skip_u32s(3)?;
        reader.mem(2)?;
        reader.skip_u32s(3)?;
    }
    if version >= 109 {
        reader.mem(2)?;
        reader.mem(2)?;
        reader.u8()?;
    }
    if version >= 110 {
        reader.u8()?;
    }
    if version >= 119 {
        reader.mem(2)?;
        reader.mem(2)?;
        reader.mem(2)?;
    }
    if version >= 121 {
        reader.u32()?;
    }
    if version >= 122 {
        reader.u32()?;
    }
    if version >= 124 {
        reader.u32()?;
    }
    if version >= 126 {
        reader.u32()?;
    }
    if version >= 128 {
        reader.u32()?;
    }
    if version >= 129 {
        reader.skip_u32s(2)?;
    }
    if version >= 130 {
        reader.u32()?;
    }
    if version >= 131 {
        reader.u32()?;
    }
    if version >= 132 {
        reader.skip_u32s(4)?;
    }
    if version >= 134 {
        reader.u8()?;
    }
    if version >= 136 {
        reader.u8()?;
    }
    if version >= 137 {
        reader.skip_u32s(4)?;
        reader.skip_u32s(24)?;
    }
    if version >= 0x8A {
        reader.skip_u32s(4)?;
        reader.mem(4)?;
        let count = reader.u32()?;
        for _ in 0..count {
            reader.mem(4)?;
        }
    }
    if version >= 0x8D {
        reader.skip(14)?;
        reader.skip(0x100)?;
        reader.skip(2)?;
    }
    if version >= 0x8E {
        reader.u32()?;
    }
    Ok(())
}

fn read_part3(reader: &mut Reader<'_>, version: u16) -> Result<()> {
    reader.u32()?;
    let var2 = reader.u32()?;
    if (var2 as i32) >= 0 {
        for _ in 0..var2 {
            let count = reader.u32()?;
            if (count as i32) < 0 {
                return Err(parse_error("a WOLF RPG map list has a negative length"));
            }
            reader.skip_u32s(count)?;
            for _ in 0..count {
                let flags = reader.u8()?;
                reader.skip_u32s(u32::from(flags))?;
            }
        }
        let var3 = reader.u32()?;
        if var3 <= 0x270F {
            if (var3 as i32) > 0 {
                for _ in 0..var3 {
                    let count = reader.u32()?;
                    if (count as i32) < 0 {
                        return Err(parse_error("a WOLF RPG map list has a negative length"));
                    }
                    reader.skip_u32s(count)?;
                }
            }
            let var4 = reader.u32()?;
            if (var4 as i32) >= 0 {
                for _ in 0..var4 {
                    if version < 0x6F {
                        reader.mem(2)?;
                    } else {
                        reader.mem(4)?;
                    }
                }
                let var5 = reader.u32()?;
                if (var5 as i32) < 0 || var5 > 10000 {
                    return Err(parse_error("a WOLF RPG event list is too long"));
                }
                for _ in 0..var5 {
                    let flags = reader.u8()?;
                    reader.skip_u32s(u32::from(flags))?;
                }
                let var6 = reader.u32()?;
                if var6 > 10000 {
                    return Err(parse_error("a WOLF RPG event list is too long"));
                }
                if (var6 as i32) <= 0 {
                    return Err(parse_error("a WOLF RPG event list is empty"));
                }
                for _ in 0..var6 {
                    let flags = reader.u8()?;
                    for _ in 0..flags {
                        if version < 0x6F {
                            reader.mem(2)?;
                        } else {
                            reader.mem(4)?;
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

fn read_part4(reader: &mut Reader<'_>, version: u16) -> Result<()> {
    read_part1_1(reader, version)?;
    let count = reader.u32()?;
    for _ in 0..count {
        read_part1_1(reader, version)?;
    }
    reader.skip(2)?;
    let count = reader.u32()?;
    reader.skip_u32s(count)?;
    if version >= 0x8A {
        let count = reader.u32()?;
        let bytes = usize::try_from(count)
            .ok()
            .and_then(|n| n.checked_mul(8))
            .ok_or_else(|| parse_error("a WOLF RPG array is too large"))?;
        reader.skip(bytes)?;
    }
    Ok(())
}

fn read_part5(reader: &mut Reader<'_>, version: u16) -> Result<()> {
    let count = reader.u16()?;
    if (count & 0x8000) != 0 {
        return Ok(());
    }
    for _ in 0..count {
        read_part5_1(reader, version)?;
    }
    Ok(())
}

fn read_part5_1(reader: &mut Reader<'_>, version: u16) -> Result<()> {
    reader.u32()?;
    reader.skip(2)?;
    reader.u16()?;
    reader.skip(2)?;
    reader.mem(2)?;
    reader.skip_u32s(2)?;
    reader.skip_u32s(8)?;
    reader.skip_u32s(6)?;
    if version >= 0x69 {
        reader.skip_u32s(18)?;
    }
    if version >= 0x6B {
        reader.u8()?;
    }
    if version >= 0x72 {
        reader.skip(2)?;
        reader.skip_u32s(8)?;
    }
    if version >= 0x73 {
        reader.skip_u32s(4)?;
        if version >= 0x74 {
            reader.skip_u32s(5)?;
        }
        if version >= 0x75 {
            reader.skip_u32s(6)?;
        }
        reader.skip_u32s(6)?;
    }
    if version >= 0x76 {
        reader.skip_u32s(3)?;
    }
    if version >= 0x81 {
        reader.skip_u32s(21)?;
        if version >= 0x87 {
            reader.skip_u32s(5)?;
        }
        if version >= 0x89 {
            let groups = reader.u32()?;
            for _ in 0..groups {
                let count = reader.u32()?;
                reader.skip_u32s(count)?;
            }
            reader.skip_u32s(7)?;
        }
    }
    Ok(())
}

struct VariableType {
    records: Vec<VariableRecord>,
}

struct VariableRecord {
    fields: Vec<Field>,
}

enum Field {
    Number { value: i32, span: ByteSpan },
    Text(MemString),
    Opaque(Vec<u8>),
}

fn read_variable_db(reader: &mut Reader<'_>) -> Result<Vec<VariableType>> {
    reader.u8()?;
    let type_count = reader.u32()?;
    let mut types = Vec::with_capacity(usize::try_from(type_count).unwrap_or(0).min(256));
    for _ in 0..type_count {
        let flag = reader.i32()?;
        let mut config = Vec::new();
        if flag <= -1 {
            if flag <= -2 {
                reader.i32()?;
            }
            let field_count = reader.u32()?;
            config.reserve(usize::try_from(field_count).unwrap_or(0).min(256));
            for _ in 0..field_count {
                config.push(reader.u32()?);
            }
        }
        let data_count = reader.u32()?;
        let mut records = Vec::with_capacity(usize::try_from(data_count).unwrap_or(0).min(256));
        for _ in 0..data_count {
            let mut numbers = Vec::with_capacity(config.len());
            for &kind in &config {
                if kind >= 2000 {
                    continue;
                }
                let start = reader.pos;
                let value = reader.i32()?;
                numbers.push((
                    value,
                    ByteSpan {
                        start: u64::try_from(start).unwrap_or(u64::MAX),
                        end: u64::try_from(start + 4).unwrap_or(u64::MAX),
                    },
                ));
            }
            let mut fields = Vec::with_capacity(config.len());
            let mut number_index = 0usize;
            for &kind in &config {
                if kind < 2000 {
                    if let Some((value, span)) = numbers.get(number_index).copied() {
                        fields.push(Field::Number { value, span });
                    }
                    number_index += 1;
                } else {
                    let mem = reader.mem(4)?;
                    let mem = MemString::new(mem, 4, true);
                    if mem.text.is_some() {
                        fields.push(Field::Text(mem));
                    } else {
                        fields.push(Field::Opaque(mem.raw));
                    }
                }
            }
            records.push(VariableRecord { fields });
        }
        types.push(VariableType { records });
    }
    Ok(types)
}

fn read_part7(reader: &mut Reader<'_>) -> Result<()> {
    let flag = reader.u8()?;
    if flag != 1 {
        return Ok(());
    }
    let count = reader.u32()?;
    for _ in 0..count {
        let kind = reader.u8()?;
        if kind < 0xFA {
            reader.u8()?;
        } else {
            reader.u32()?;
        }
    }
    Ok(())
}
