//! Reads an Emuera 1.808+ binary save into a named variable tree.

use saveprism_core::{
    DecodeCtx, IntWidth, Node, NodeFlags, NodeId, Params, Result, SaveDoc, TypeHint, Value,
};

use crate::text_encoding::TextEncoding;

use super::{EMUERA_UNSUPPORTED, HEADER, HINT_PREFIX, MAX_ARRAY_ELEMENTS, VERSION, parse_error};

const TYPE_INT: u8 = 0x00;
const TYPE_INT_ARRAY: u8 = 0x01;
const TYPE_INT_ARRAY_2D: u8 = 0x02;
const TYPE_INT_ARRAY_3D: u8 = 0x03;
const TYPE_STR: u8 = 0x10;
const TYPE_STR_ARRAY: u8 = 0x11;
const TYPE_STR_ARRAY_2D: u8 = 0x12;
const TYPE_STR_ARRAY_3D: u8 = 0x13;
const TYPE_SEPARATOR: u8 = 0xFD;
const TYPE_EOC: u8 = 0xFE;
const TYPE_EOF: u8 = 0xFF;

const MARK_BYTE: u8 = 0xCF;
const MARK_I16: u8 = 0xD0;
const MARK_I32: u8 = 0xD1;
const MARK_I64: u8 = 0xD2;
const MARK_STRING: u8 = 0xD8;
const MARK_EOA1: u8 = 0xE0;
const MARK_EOA2: u8 = 0xE1;
const MARK_ZERO: u8 = 0xF0;
const MARK_ZERO_A1: u8 = 0xF1;
const MARK_ZERO_A2: u8 = 0xF2;

pub(crate) fn read_all(input: &[u8], ctx: &DecodeCtx<'_>) -> Result<(SaveDoc, Params)> {
    if !input.starts_with(&HEADER) {
        return Err(parse_error("not a binary Emuera save"));
    }
    let mut cursor = Cursor::new(input, HEADER.len());
    let version = cursor.u32()?;
    if version != VERSION {
        return Err(saveprism_core::Error::new(
            EMUERA_UNSUPPORTED,
            format!("Emuera binary save version {version} is not supported"),
        ));
    }
    let data_count = cursor.u32()?;
    if data_count != 0 {
        return Err(saveprism_core::Error::new(
            EMUERA_UNSUPPORTED,
            "this Emuera save carries a reserved header data table",
        ));
    }
    let file_type = cursor.u8()?;
    let file_type_name = match file_type {
        0 => "normal",
        1 => "global",
        2 => "variables",
        3 => "character-variables",
        other => {
            return Err(parse_error(format!(
                "unknown Emuera save file type {other}"
            )));
        }
    };
    let game_code = cursor.i64()?;
    let game_version = cursor.i64()?;
    let description = cursor.string()?;

    let mut doc = ctx.new_doc();
    let mut characters_id = None;
    if file_type == 0 || file_type == 3 {
        let char_count = cursor.i64()?;
        if char_count < 0 {
            return Err(parse_error("the Emuera character count is negative"));
        }
        let count = usize::try_from(char_count)
            .map_err(|_| parse_error("the Emuera character count does not fit"))?;
        let mut characters = Vec::with_capacity(count.min(ctx.limits().max_nodes as usize));
        for _ in 0..count {
            characters.push(read_character(&mut doc, &mut cursor, ctx)?);
        }
        let mut node = Node::new(Value::Array(characters));
        node.type_hint = Some(hint("characters"));
        node.flags.insert(NodeFlags::READ_ONLY);
        characters_id = Some(doc.push(node)?);
    }

    let mut variables_id = None;
    if file_type != 3 {
        let entries = read_variables_until_eof(&mut doc, &mut cursor, ctx)?;
        variables_id = Some(push_object(&mut doc, "variables", entries, true)?);
    } else if cursor.u8()? != TYPE_EOF {
        return Err(parse_error(
            "an Emuera character-variable save does not end with 0xFF",
        ));
    }
    if cursor.pos != input.len() {
        return Err(parse_error("the Emuera save has trailing bytes"));
    }

    let header_id = {
        let entries = vec![
            (
                "magic".to_owned(),
                push_read_only_str(&mut doc, "magic", "89 45 52 41 0D 0A 1A 0A")?,
            ),
            (
                "version".to_owned(),
                push_read_only_int(&mut doc, i128::from(version), IntWidth::U32, "version")?,
            ),
            (
                "data_count".to_owned(),
                push_read_only_int(
                    &mut doc,
                    i128::from(data_count),
                    IntWidth::U32,
                    "data-count",
                )?,
            ),
        ];
        push_object(&mut doc, "header", entries, true)?
    };
    let file_type_id = {
        let mut node = Node::new(Value::Str(file_type_name.to_owned()));
        node.type_hint = Some(hint("file-type"));
        node.flags.insert(NodeFlags::READ_ONLY);
        doc.push(node)?
    };
    let game_code_id =
        push_read_only_int(&mut doc, i128::from(game_code), IntWidth::I64, "game-code")?;
    let game_version_id = push_read_only_int(
        &mut doc,
        i128::from(game_version),
        IntWidth::I64,
        "game-version",
    )?;
    let description_id = {
        let mut node = Node::new(Value::Str(description));
        node.type_hint = Some(hint("description"));
        doc.push(node)?
    };
    let mut root_entries = vec![
        ("header".to_owned(), header_id),
        ("file_type".to_owned(), file_type_id),
        ("game_code".to_owned(), game_code_id),
        ("game_version".to_owned(), game_version_id),
        ("description".to_owned(), description_id),
    ];
    if let Some(id) = characters_id {
        root_entries.push(("characters".to_owned(), id));
    }
    if let Some(id) = variables_id {
        root_entries.push(("variables".to_owned(), id));
    }
    let root = push_object(&mut doc, "save", root_entries, true)?;
    doc.set_root(root);
    Ok((doc, Params::new()))
}

fn read_character(
    doc: &mut SaveDoc,
    cursor: &mut Cursor<'_>,
    ctx: &DecodeCtx<'_>,
) -> Result<NodeId> {
    let mut values = Vec::new();
    let mut user_defined = Vec::new();
    let mut in_user = false;
    loop {
        match read_pair(doc, cursor, ctx)? {
            Pair::Eoc => break,
            Pair::Eof => return Err(parse_error("a character record ends without 0xFE")),
            Pair::Separator => in_user = true,
            Pair::Variable(key, value) => {
                if in_user {
                    user_defined.push((key, value));
                } else {
                    values.push((key, value));
                }
            }
        }
    }
    let values_id = push_object(doc, "values", values, true)?;
    let mut entries = vec![("values".to_owned(), values_id)];
    if in_user {
        let user_id = push_object(doc, "user-defined", user_defined, true)?;
        entries.push(("user_defined".to_owned(), user_id));
    }
    push_object(doc, "character", entries, true)
}

fn read_variables_until_eof(
    doc: &mut SaveDoc,
    cursor: &mut Cursor<'_>,
    ctx: &DecodeCtx<'_>,
) -> Result<Vec<(String, NodeId)>> {
    let mut entries = Vec::new();
    loop {
        match read_pair(doc, cursor, ctx)? {
            Pair::Eoc => return Err(parse_error("an unexpected 0xFE ends the variable list")),
            Pair::Eof => return Ok(entries),
            Pair::Separator => {
                return Err(saveprism_core::Error::new(
                    EMUERA_UNSUPPORTED,
                    "a separator outside a character record is not supported",
                ));
            }
            Pair::Variable(key, value) => entries.push((key, value)),
        }
    }
}

enum Pair {
    Variable(String, NodeId),
    Separator,
    Eoc,
    Eof,
}

fn read_pair(doc: &mut SaveDoc, cursor: &mut Cursor<'_>, ctx: &DecodeCtx<'_>) -> Result<Pair> {
    let kind = cursor.u8()?;
    match kind {
        TYPE_SEPARATOR => Ok(Pair::Separator),
        TYPE_EOC => Ok(Pair::Eoc),
        TYPE_EOF => Ok(Pair::Eof),
        TYPE_INT | TYPE_INT_ARRAY | TYPE_INT_ARRAY_2D | TYPE_INT_ARRAY_3D | TYPE_STR
        | TYPE_STR_ARRAY | TYPE_STR_ARRAY_2D | TYPE_STR_ARRAY_3D => {
            let key = cursor.string()?;
            let value = read_value(doc, cursor, kind, ctx)?;
            Ok(Pair::Variable(key, value))
        }
        other => Err(parse_error(format!(
            "unknown Emuera variable type 0x{other:02X}"
        ))),
    }
}

fn read_value(
    doc: &mut SaveDoc,
    cursor: &mut Cursor<'_>,
    kind: u8,
    ctx: &DecodeCtx<'_>,
) -> Result<NodeId> {
    match kind {
        TYPE_INT => {
            let value = cursor.compressed_int()?;
            let mut node = Node::new(Value::Int {
                v: i128::from(value),
                width: IntWidth::I64,
            });
            node.type_hint = Some(hint("int"));
            doc.push(node)
        }
        TYPE_STR => {
            let value = cursor.string()?;
            let mut node = Node::new(Value::Str(value));
            node.type_hint = Some(hint("str"));
            doc.push(node)
        }
        TYPE_INT_ARRAY => read_int_array(doc, cursor, ctx),
        TYPE_INT_ARRAY_2D => read_int_array_2d(doc, cursor, ctx),
        TYPE_INT_ARRAY_3D => read_int_array_3d(doc, cursor, ctx),
        TYPE_STR_ARRAY => read_str_array(doc, cursor, ctx),
        TYPE_STR_ARRAY_2D => read_str_array_2d(doc, cursor, ctx),
        TYPE_STR_ARRAY_3D => read_str_array_3d(doc, cursor, ctx),
        _ => Err(parse_error("unknown Emuera value type")),
    }
}

fn read_int_array(
    doc: &mut SaveDoc,
    cursor: &mut Cursor<'_>,
    ctx: &DecodeCtx<'_>,
) -> Result<NodeId> {
    let length = cursor.length()?;
    let mut values = Vec::new();
    loop {
        let mark = cursor.u8()?;
        match mark {
            TYPE_EOF => break,
            MARK_ZERO => {
                let count = cursor.compressed_int()?;
                let count = usize::try_from(count)
                    .map_err(|_| parse_error("an Emuera zero run is negative"))?;
                values.resize(
                    values
                        .len()
                        .checked_add(count)
                        .ok_or_else(|| parse_error("an Emuera zero run is too long"))?,
                    0_i64,
                );
            }
            MARK_EOA1 | MARK_EOA2 | MARK_ZERO_A1 | MARK_ZERO_A2 => {
                return Err(parse_error(
                    "an Emuera one-dimensional array has a row marker",
                ));
            }
            _ => values.push(cursor.int_from_mark(mark)?),
        }
        if values.len() > length {
            return Err(parse_error(
                "an Emuera array has more values than its length",
            ));
        }
    }
    values.resize(length, 0);
    push_int_elements(doc, values, "int-array", ctx)
}

fn read_int_array_2d(
    doc: &mut SaveDoc,
    cursor: &mut Cursor<'_>,
    ctx: &DecodeCtx<'_>,
) -> Result<NodeId> {
    let length0 = cursor.length()?;
    let length1 = cursor.length()?;
    check_product(&[length0, length1])?;
    let mut rows = vec![vec![0_i64; length1]; length0];
    let mut x = 0_usize;
    let mut y = 0_usize;
    loop {
        let mark = cursor.u8()?;
        match mark {
            TYPE_EOF => break,
            MARK_EOA1 => {
                if x >= length0 {
                    return Err(parse_error("an Emuera array has too many rows"));
                }
                x = x.saturating_add(1);
                y = 0;
            }
            MARK_ZERO_A1 => {
                let count = cursor.count()?;
                if x.saturating_add(count) > length0 {
                    return Err(parse_error("an Emuera zero-row run is too long"));
                }
                x = x.saturating_add(count);
                y = 0;
            }
            MARK_ZERO => {
                let count = cursor.count()?;
                if y.saturating_add(count) > length1 {
                    return Err(parse_error("an Emuera zero run is too long"));
                }
                y = y.saturating_add(count);
            }
            MARK_EOA2 | MARK_ZERO_A2 => {
                return Err(parse_error("a two-dimensional array has a plane marker"));
            }
            _ => {
                if x >= length0 || y >= length1 {
                    return Err(parse_error("an Emuera array is longer than its dimensions"));
                }
                let value = cursor.int_from_mark(mark)?;
                if let Some(row) = rows.get_mut(x)
                    && let Some(slot) = row.get_mut(y)
                {
                    *slot = value;
                }
                y = y.saturating_add(1);
            }
        }
    }
    let mut out = Vec::with_capacity(length0);
    for row in rows {
        out.push(push_int_elements(doc, row, "int-row", ctx)?);
    }
    push_array(doc, out, "int-array-2d", ctx)
}

fn read_int_array_3d(
    doc: &mut SaveDoc,
    cursor: &mut Cursor<'_>,
    ctx: &DecodeCtx<'_>,
) -> Result<NodeId> {
    let length0 = cursor.length()?;
    let length1 = cursor.length()?;
    let length2 = cursor.length()?;
    check_product(&[length0, length1, length2])?;
    let mut planes = vec![vec![vec![0_i64; length2]; length1]; length0];
    let mut x = 0_usize;
    let mut y = 0_usize;
    let mut z = 0_usize;
    loop {
        let mark = cursor.u8()?;
        match mark {
            TYPE_EOF => break,
            MARK_EOA2 => {
                if x >= length0 {
                    return Err(parse_error("an Emuera array has too many planes"));
                }
                x = x.saturating_add(1);
                y = 0;
                z = 0;
            }
            MARK_ZERO_A2 => {
                let count = cursor.count()?;
                if x.saturating_add(count) > length0 {
                    return Err(parse_error("an Emuera zero-plane run is too long"));
                }
                x = x.saturating_add(count);
                y = 0;
                z = 0;
            }
            MARK_EOA1 => {
                if y >= length1 {
                    return Err(parse_error("an Emuera array has too many rows"));
                }
                y = y.saturating_add(1);
                z = 0;
            }
            MARK_ZERO_A1 => {
                let count = cursor.count()?;
                if y.saturating_add(count) > length1 {
                    return Err(parse_error("an Emuera zero-row run is too long"));
                }
                y = y.saturating_add(count);
                z = 0;
            }
            MARK_ZERO => {
                let count = cursor.count()?;
                if z.saturating_add(count) > length2 {
                    return Err(parse_error("an Emuera zero run is too long"));
                }
                z = z.saturating_add(count);
            }
            _ => {
                if x >= length0 || y >= length1 || z >= length2 {
                    return Err(parse_error("an Emuera array is longer than its dimensions"));
                }
                let value = cursor.int_from_mark(mark)?;
                if let Some(plane) = planes.get_mut(x)
                    && let Some(row) = plane.get_mut(y)
                    && let Some(slot) = row.get_mut(z)
                {
                    *slot = value;
                }
                z = z.saturating_add(1);
            }
        }
    }
    let mut out = Vec::with_capacity(length0);
    for plane in planes {
        let mut rows = Vec::with_capacity(length1);
        for row in plane {
            rows.push(push_int_elements(doc, row, "int-row", ctx)?);
        }
        let mut node = Node::new(Value::Array(rows));
        node.type_hint = Some(hint("int-plane"));
        node.flags.insert(NodeFlags::READ_ONLY);
        out.push(doc.push(node)?);
    }
    push_array(doc, out, "int-array-3d", ctx)
}

fn read_str_array(
    doc: &mut SaveDoc,
    cursor: &mut Cursor<'_>,
    ctx: &DecodeCtx<'_>,
) -> Result<NodeId> {
    let length = cursor.length()?;
    let mut values = Vec::new();
    loop {
        let mark = cursor.u8()?;
        match mark {
            TYPE_EOF => break,
            MARK_ZERO => {
                let count = cursor.count()?;
                values.resize(
                    values
                        .len()
                        .checked_add(count)
                        .ok_or_else(|| parse_error("an Emuera zero run is too long"))?,
                    String::new(),
                );
            }
            MARK_STRING => values.push(cursor.string()?),
            _ => return Err(parse_error("a string array has a non-string token")),
        }
        if values.len() > length {
            return Err(parse_error(
                "an Emuera array has more values than its length",
            ));
        }
    }
    values.resize(length, String::new());
    push_str_elements(doc, values, "str-array", ctx)
}

fn read_str_array_2d(
    doc: &mut SaveDoc,
    cursor: &mut Cursor<'_>,
    ctx: &DecodeCtx<'_>,
) -> Result<NodeId> {
    let length0 = cursor.length()?;
    let length1 = cursor.length()?;
    check_product(&[length0, length1])?;
    let mut rows = vec![vec![String::new(); length1]; length0];
    let mut x = 0_usize;
    let mut y = 0_usize;
    loop {
        let mark = cursor.u8()?;
        match mark {
            TYPE_EOF => break,
            MARK_EOA1 => {
                if x >= length0 {
                    return Err(parse_error("an Emuera array has too many rows"));
                }
                x = x.saturating_add(1);
                y = 0;
            }
            MARK_ZERO_A1 => {
                let count = cursor.count()?;
                if x.saturating_add(count) > length0 {
                    return Err(parse_error("an Emuera zero-row run is too long"));
                }
                x = x.saturating_add(count);
                y = 0;
            }
            MARK_ZERO => {
                let count = cursor.count()?;
                if y.saturating_add(count) > length1 {
                    return Err(parse_error("an Emuera zero run is too long"));
                }
                y = y.saturating_add(count);
            }
            MARK_STRING => {
                if x >= length0 || y >= length1 {
                    return Err(parse_error("an Emuera array is longer than its dimensions"));
                }
                let value = cursor.string()?;
                if let Some(row) = rows.get_mut(x)
                    && let Some(slot) = row.get_mut(y)
                {
                    *slot = value;
                }
                y = y.saturating_add(1);
            }
            _ => return Err(parse_error("a string array has a non-string token")),
        }
    }
    let mut out = Vec::with_capacity(length0);
    for row in rows {
        out.push(push_str_elements(doc, row, "str-row", ctx)?);
    }
    push_array(doc, out, "str-array-2d", ctx)
}

fn read_str_array_3d(
    doc: &mut SaveDoc,
    cursor: &mut Cursor<'_>,
    ctx: &DecodeCtx<'_>,
) -> Result<NodeId> {
    let length0 = cursor.length()?;
    let length1 = cursor.length()?;
    let length2 = cursor.length()?;
    check_product(&[length0, length1, length2])?;
    let mut planes = vec![vec![vec![String::new(); length2]; length1]; length0];
    let mut x = 0_usize;
    let mut y = 0_usize;
    let mut z = 0_usize;
    loop {
        let mark = cursor.u8()?;
        match mark {
            TYPE_EOF => break,
            MARK_EOA2 => {
                if x >= length0 {
                    return Err(parse_error("an Emuera array has too many planes"));
                }
                x = x.saturating_add(1);
                y = 0;
                z = 0;
            }
            MARK_ZERO_A2 => {
                let count = cursor.count()?;
                if x.saturating_add(count) > length0 {
                    return Err(parse_error("an Emuera zero-plane run is too long"));
                }
                x = x.saturating_add(count);
                y = 0;
                z = 0;
            }
            MARK_EOA1 => {
                if y >= length1 {
                    return Err(parse_error("an Emuera array has too many rows"));
                }
                y = y.saturating_add(1);
                z = 0;
            }
            MARK_ZERO_A1 => {
                let count = cursor.count()?;
                if y.saturating_add(count) > length1 {
                    return Err(parse_error("an Emuera zero-row run is too long"));
                }
                y = y.saturating_add(count);
                z = 0;
            }
            MARK_ZERO => {
                let count = cursor.count()?;
                if z.saturating_add(count) > length2 {
                    return Err(parse_error("an Emuera zero run is too long"));
                }
                z = z.saturating_add(count);
            }
            MARK_STRING => {
                if x >= length0 || y >= length1 || z >= length2 {
                    return Err(parse_error("an Emuera array is longer than its dimensions"));
                }
                let value = cursor.string()?;
                if let Some(plane) = planes.get_mut(x)
                    && let Some(row) = plane.get_mut(y)
                    && let Some(slot) = row.get_mut(z)
                {
                    *slot = value;
                }
                z = z.saturating_add(1);
            }
            _ => return Err(parse_error("a string array has a non-string token")),
        }
    }
    let mut out = Vec::with_capacity(length0);
    for plane in planes {
        let mut rows = Vec::with_capacity(length1);
        for row in plane {
            rows.push(push_str_elements(doc, row, "str-row", ctx)?);
        }
        let mut node = Node::new(Value::Array(rows));
        node.type_hint = Some(hint("str-plane"));
        node.flags.insert(NodeFlags::READ_ONLY);
        out.push(doc.push(node)?);
    }
    push_array(doc, out, "str-array-3d", ctx)
}

fn check_product(lengths: &[usize]) -> Result<()> {
    let mut product = 1_usize;
    for length in lengths {
        product = product
            .checked_mul(*length)
            .ok_or_else(|| parse_error("an Emuera array is too large"))?;
    }
    if product > MAX_ARRAY_ELEMENTS {
        return Err(parse_error(format!(
            "an Emuera array has more than {MAX_ARRAY_ELEMENTS} elements (dimensions {lengths:?})"
        )));
    }
    Ok(())
}

fn push_int_elements(
    doc: &mut SaveDoc,
    values: Vec<i64>,
    hint_name: &str,
    _ctx: &DecodeCtx<'_>,
) -> Result<NodeId> {
    let mut items = Vec::with_capacity(values.len());
    for value in values {
        let mut node = Node::new(Value::Int {
            v: i128::from(value),
            width: IntWidth::I64,
        });
        node.type_hint = Some(hint("int"));
        items.push(doc.push(node)?);
    }
    let mut node = Node::new(Value::Array(items));
    node.type_hint = Some(hint(hint_name));
    node.flags.insert(NodeFlags::READ_ONLY);
    doc.push(node)
}

fn push_str_elements(
    doc: &mut SaveDoc,
    values: Vec<String>,
    hint_name: &str,
    _ctx: &DecodeCtx<'_>,
) -> Result<NodeId> {
    let mut items = Vec::with_capacity(values.len());
    for value in values {
        let mut node = Node::new(Value::Str(value));
        node.type_hint = Some(hint("str"));
        items.push(doc.push(node)?);
    }
    let mut node = Node::new(Value::Array(items));
    node.type_hint = Some(hint(hint_name));
    node.flags.insert(NodeFlags::READ_ONLY);
    doc.push(node)
}

fn push_array(
    doc: &mut SaveDoc,
    items: Vec<NodeId>,
    name: &str,
    _ctx: &DecodeCtx<'_>,
) -> Result<NodeId> {
    let mut node = Node::new(Value::Array(items));
    node.type_hint = Some(hint(name));
    node.flags.insert(NodeFlags::READ_ONLY);
    doc.push(node)
}

fn hint(name: &str) -> TypeHint {
    TypeHint::new(format!("{HINT_PREFIX}{name}"))
}

fn push_read_only_str(doc: &mut SaveDoc, name: &str, text: &str) -> Result<NodeId> {
    let mut node = Node::new(Value::Str(text.to_owned()));
    node.type_hint = Some(hint(name));
    node.flags.insert(NodeFlags::READ_ONLY);
    doc.push(node)
}

fn push_read_only_int(
    doc: &mut SaveDoc,
    value: i128,
    width: IntWidth,
    name: &str,
) -> Result<NodeId> {
    let mut node = Node::new(Value::Int { v: value, width });
    node.type_hint = Some(hint(name));
    node.flags.insert(NodeFlags::READ_ONLY);
    doc.push(node)
}

fn push_object(
    doc: &mut SaveDoc,
    name: &str,
    entries: Vec<(String, NodeId)>,
    read_only: bool,
) -> Result<NodeId> {
    let mut node = Node::new(Value::Object(entries));
    node.type_hint = Some(hint(name));
    if read_only {
        node.flags.insert(NodeFlags::READ_ONLY);
    }
    doc.push(node)
}

struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(data: &'a [u8], pos: usize) -> Self {
        Self { data, pos }
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8]> {
        let end = self
            .pos
            .checked_add(count)
            .filter(|end| *end <= self.data.len())
            .ok_or_else(|| parse_error("the Emuera save is truncated"))?;
        let slice = self
            .data
            .get(self.pos..end)
            .ok_or_else(|| parse_error("the Emuera save is truncated"))?;
        self.pos = end;
        Ok(slice)
    }

    fn u8(&mut self) -> Result<u8> {
        let byte = self
            .data
            .get(self.pos)
            .copied()
            .ok_or_else(|| parse_error("the Emuera save is truncated"))?;
        self.pos = self.pos.saturating_add(1);
        Ok(byte)
    }

    fn u32(&mut self) -> Result<u32> {
        let bytes = self.take(4)?;
        let array =
            <[u8; 4]>::try_from(bytes).map_err(|_| parse_error("the Emuera save is truncated"))?;
        Ok(u32::from_le_bytes(array))
    }

    fn i16(&mut self) -> Result<i16> {
        let bytes = self.take(2)?;
        let array =
            <[u8; 2]>::try_from(bytes).map_err(|_| parse_error("the Emuera save is truncated"))?;
        Ok(i16::from_le_bytes(array))
    }

    fn i32(&mut self) -> Result<i32> {
        let bytes = self.take(4)?;
        let array =
            <[u8; 4]>::try_from(bytes).map_err(|_| parse_error("the Emuera save is truncated"))?;
        Ok(i32::from_le_bytes(array))
    }

    fn i64(&mut self) -> Result<i64> {
        let bytes = self.take(8)?;
        let array =
            <[u8; 8]>::try_from(bytes).map_err(|_| parse_error("the Emuera save is truncated"))?;
        Ok(i64::from_le_bytes(array))
    }

    fn length(&mut self) -> Result<usize> {
        let value = self.i32()?;
        if value < 0 {
            return Err(parse_error("an Emuera array length is negative"));
        }
        let length = usize::try_from(value)
            .map_err(|_| parse_error("an Emuera array length does not fit"))?;
        if length > MAX_ARRAY_ELEMENTS {
            return Err(parse_error(format!(
                "an Emuera array has more than {MAX_ARRAY_ELEMENTS} elements (length {length}, at {})",
                self.pos
            )));
        }
        Ok(length)
    }

    fn compressed_int(&mut self) -> Result<i64> {
        let mark = self.u8()?;
        self.int_from_mark(mark)
    }

    /// A compressed count (zero runs and row/plane runs use the integer encoding, not int32).
    fn count(&mut self) -> Result<usize> {
        let value = self.compressed_int()?;
        if value < 0 {
            return Err(parse_error("an Emuera run count is negative"));
        }
        let count = usize::try_from(value)
            .map_err(|_| parse_error("an Emuera run count does not fit this platform"))?;
        if count > MAX_ARRAY_ELEMENTS {
            return Err(parse_error(format!(
                "an Emuera run is longer than {MAX_ARRAY_ELEMENTS} elements"
            )));
        }
        Ok(count)
    }

    fn int_from_mark(&mut self, mark: u8) -> Result<i64> {
        if mark <= MARK_BYTE {
            Ok(i64::from(mark))
        } else if mark == MARK_I16 {
            Ok(i64::from(self.i16()?))
        } else if mark == MARK_I32 {
            Ok(i64::from(self.i32()?))
        } else if mark == MARK_I64 {
            self.i64()
        } else {
            Err(parse_error(format!(
                "invalid Emuera integer marker 0x{mark:02X}"
            )))
        }
    }

    fn string(&mut self) -> Result<String> {
        let mut length = 0_usize;
        let mut shift = 0_u32;
        let mut bytes_read = 0_usize;
        loop {
            if bytes_read >= 5 {
                return Err(parse_error("an Emuera string length is too long"));
            }
            let byte = self.u8()?;
            bytes_read = bytes_read.saturating_add(1);
            let chunk = u32::from(byte & 0x7F)
                .checked_shl(shift)
                .ok_or_else(|| parse_error("an Emuera string length is too long"))?;
            length = length
                .checked_add(usize::try_from(chunk).map_err(|_| {
                    parse_error("an Emuera string length does not fit this platform")
                })?)
                .ok_or_else(|| parse_error("an Emuera string length is too long"))?;
            if byte & 0x80 == 0 {
                break;
            }
            shift = shift.saturating_add(7);
        }
        let bytes = self.take(length)?;
        TextEncoding::Utf16Le
            .decode(bytes)
            .ok_or_else(|| parse_error("an Emuera string is not valid UTF-16LE"))
    }
}
