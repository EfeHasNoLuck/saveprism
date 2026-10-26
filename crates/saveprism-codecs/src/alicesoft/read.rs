//! Reads AliceSoft System 3.x / System 4 and Artemis saves into the universal tree.

use flate2::read::ZlibDecoder;
use saveprism_core::limits::len_u64;
use saveprism_core::{
    ByteSpan, DecodeCtx, FloatWidth, IntWidth, Node, NodeFlags, NodeId, Params, Result, SaveDoc,
    TypeHint, Value,
};

use super::{
    ASD_PARSE, HINT_PREFIX, P_ORIGINAL, P_RAW, RSAVE_MAGIC, SYS3_HEADER, SYS3_VERSION,
    SYS4_ENCRYPTED_MARK, bad, header_text, shift_jis_text,
};

const PAGE_COUNT: usize = 256;
const STACK_OFFSET_FIELD: usize = 2144;
const STRING_OFFSET_FIELD: usize = 2148;
const VAR_SYS_FIELD: usize = 2160;

/// Reads a little-endian u32 at `offset` when it is inside `data`.
pub(crate) fn u32_at(data: &[u8], offset: usize) -> Option<u32> {
    let bytes = data.get(offset..offset.checked_add(4)?)?;
    let array = <[u8; 4]>::try_from(bytes).ok()?;
    Some(u32::from_le_bytes(array))
}

fn u16_at(data: &[u8], offset: usize) -> Option<u16> {
    let bytes = data.get(offset..offset.checked_add(2)?)?;
    let array = <[u8; 2]>::try_from(bytes).ok()?;
    Some(u16::from_le_bytes(array))
}

fn i32_at(data: &[u8], offset: usize) -> Option<i32> {
    u32_at(data, offset).map(|value| i32::from_le_bytes(value.to_le_bytes()))
}

fn hint(name: &str) -> TypeHint {
    TypeHint::new(format!("{HINT_PREFIX}{name}"))
}

fn span(start: usize, end: usize) -> ByteSpan {
    ByteSpan {
        start: len_u64(start),
        end: len_u64(end),
    }
}

fn readonly_int(doc: &mut SaveDoc, value: i128, width: IntWidth, name: &str) -> Result<NodeId> {
    let mut node = Node::new(Value::Int { v: value, width });
    node.type_hint = Some(hint(name));
    node.flags.insert(NodeFlags::READ_ONLY);
    doc.push(node)
}

fn readonly_text(doc: &mut SaveDoc, text: String, name: &str) -> Result<NodeId> {
    let mut node = Node::new(Value::Str(text));
    node.type_hint = Some(hint(name));
    node.flags.insert(NodeFlags::READ_ONLY);
    doc.push(node)
}

fn object(
    doc: &mut SaveDoc,
    name: &str,
    entries: Vec<(String, NodeId)>,
    readonly: bool,
) -> Result<NodeId> {
    let mut node = Node::new(Value::Object(entries));
    node.type_hint = Some(hint(name));
    if readonly {
        node.flags.insert(NodeFlags::READ_ONLY);
    }
    doc.push(node)
}

fn array(doc: &mut SaveDoc, name: &str, items: Vec<NodeId>, readonly: bool) -> Result<NodeId> {
    let mut node = Node::new(Value::Array(items));
    node.type_hint = Some(hint(name));
    if readonly {
        node.flags.insert(NodeFlags::READ_ONLY);
    }
    doc.push(node)
}

fn entry(key: &str, id: NodeId) -> (String, NodeId) {
    (key.to_owned(), id)
}

/// Reads a NUL-terminated string from `data` at `start`; returns `(text, content_start, content_end)`.
fn cstring(data: &[u8], start: usize) -> Result<(String, usize, usize)> {
    let rest = data
        .get(start..)
        .ok_or_else(|| bad("a string starts outside the file", start))?;
    let len = rest
        .iter()
        .position(|byte| *byte == 0)
        .ok_or_else(|| bad("a string is not NUL-terminated", start))?;
    let end = start
        .checked_add(len)
        .ok_or_else(|| bad("a string is too long", start))?;
    let bytes = data
        .get(start..end)
        .ok_or_else(|| bad("a string is truncated", start))?;
    let text = shift_jis_text(bytes)
        .or_else(|| std::str::from_utf8(bytes).ok().map(str::to_owned))
        .ok_or_else(|| bad("a string is not valid text", start))?;
    Ok((text, start, end))
}

/// A bounds-checked cursor over the decompressed System 4 body.
struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn seek(&mut self, offset: usize) -> Result<()> {
        if offset > self.data.len() {
            return Err(bad("a section offset is past the end of the save", offset));
        }
        self.pos = offset;
        Ok(())
    }

    fn u32(&mut self) -> Result<u32> {
        let value =
            u32_at(self.data, self.pos).ok_or_else(|| bad("a value is cut short", self.pos))?;
        self.pos = self.pos.saturating_add(4);
        Ok(value)
    }

    fn i32(&mut self) -> Result<i32> {
        let value =
            i32_at(self.data, self.pos).ok_or_else(|| bad("a value is cut short", self.pos))?;
        self.pos = self.pos.saturating_add(4);
        Ok(value)
    }

    fn string(&mut self) -> Result<(String, usize, usize)> {
        let found = cstring(self.data, self.pos)?;
        self.pos = found.2.saturating_add(1);
        Ok(found)
    }
}

/// System 3.x `.asd` files.
pub(crate) fn system3(input: &[u8], ctx: &DecodeCtx<'_>) -> Result<(SaveDoc, Params)> {
    if input.len() < SYS3_HEADER {
        return Err(bad("the System 3.x header is cut short", 0));
    }
    let mut doc = ctx.new_doc();

    let signature = input
        .get(..32)
        .ok_or_else(|| bad("the System 3.x signature is cut short", 0))?;
    let magic = readonly_text(
        &mut doc,
        header_text(signature).unwrap_or_default(),
        "magic",
    )?;
    let version = readonly_int(
        &mut doc,
        i128::from(u32_at(input, 32).unwrap_or(SYS3_VERSION)),
        IntWidth::U32,
        "version",
    )?;
    let game_name_bytes = input
        .get(36..64)
        .ok_or_else(|| bad("the game name field is cut short", 36))?;
    let game_name = readonly_text(
        &mut doc,
        header_text(game_name_bytes).unwrap_or_default(),
        "game-name",
    )?;

    let stack_offset = u32_at(input, STACK_OFFSET_FIELD)
        .map(|value| value as usize)
        .ok_or_else(|| bad("the stack offset is cut short", STACK_OFFSET_FIELD))?;
    let string_offset = u32_at(input, STRING_OFFSET_FIELD)
        .map(|value| value as usize)
        .ok_or_else(|| bad("the string offset is cut short", STRING_OFFSET_FIELD))?;

    let stack_size = u32_at(input, stack_offset)
        .ok_or_else(|| bad("the stack block is cut short", stack_offset))?;
    let stack_start = stack_offset.saturating_add(16);
    let stack_end = stack_start.saturating_add(stack_size as usize);
    let stack_bytes = input
        .get(stack_start..stack_end)
        .ok_or_else(|| bad("the stack block is truncated", stack_offset))?;
    let stack_size_id = readonly_int(
        &mut doc,
        i128::from(stack_size),
        IntWidth::U32,
        "stack-size",
    )?;
    let mut stack_node = Node::new(Value::Bytes(stack_bytes.to_vec()));
    stack_node.type_hint = Some(hint("stack"));
    stack_node.flags.insert(NodeFlags::READ_ONLY);
    let stack_id = doc.push(stack_node)?;

    let count = u32_at(input, string_offset.saturating_add(4))
        .ok_or_else(|| bad("the string block is cut short", string_offset))?;
    if count > 1_000_000 {
        return Err(bad("the string count is implausibly large", string_offset));
    }
    let mut strings = Vec::new();
    let mut pos = string_offset.saturating_add(16);
    for _ in 0..count {
        let (text, start, end) = cstring(input, pos)?;
        let mut node = Node::new(Value::Str(text));
        node.type_hint = Some(hint("string"));
        node.span = Some(span(start, end));
        strings.push(doc.push(node)?);
        pos = end.saturating_add(1);
    }
    let strings_id = array(&mut doc, "strings", strings, true)?;

    let mut pages = Vec::new();
    for index in 0..PAGE_COUNT {
        let field = VAR_SYS_FIELD.saturating_add(index.saturating_mul(4));
        let page_offset = u32_at(input, field)
            .map(|value| value as usize)
            .ok_or_else(|| bad("a variable-page offset is cut short", field))?;
        if page_offset == 0 {
            continue;
        }
        let size = u32_at(input, page_offset)
            .ok_or_else(|| bad("a variable page is cut short", page_offset))?;
        if size % 2 != 0 {
            return Err(bad("a variable page has an odd byte size", page_offset));
        }
        let page_number = u32_at(input, page_offset.saturating_add(4))
            .ok_or_else(|| bad("a variable page is cut short", page_offset))?;
        let values_start = page_offset.saturating_add(16);
        let values_end = values_start.saturating_add(size as usize);
        if values_end > input.len() {
            return Err(bad("a variable page is truncated", page_offset));
        }
        let count = size as usize / 2;
        let mut values = Vec::new();
        for value_index in 0..count {
            let value_start = values_start.saturating_add(value_index.saturating_mul(2));
            let value = u16_at(input, value_start)
                .ok_or_else(|| bad("a variable value is cut short", value_start))?;
            let mut node = Node::new(Value::Int {
                v: i128::from(value),
                width: IntWidth::U16,
            });
            node.type_hint = Some(hint("number"));
            node.span = Some(span(value_start, value_start.saturating_add(2)));
            values.push(doc.push(node)?);
        }
        let values_id = array(&mut doc, "values", values, true)?;
        let page_id = readonly_int(
            &mut doc,
            i128::from(page_number),
            IntWidth::U32,
            "page-number",
        )?;
        pages.push(object(
            &mut doc,
            "page",
            vec![entry("page", page_id), entry("values", values_id)],
            true,
        )?);
    }
    let variables_id = array(&mut doc, "variables", pages, true)?;

    let root = object(
        &mut doc,
        "save",
        vec![
            entry("magic", magic),
            entry("version", version),
            entry("game_name", game_name),
            entry("stack_size", stack_size_id),
            entry("stack", stack_id),
            entry("strings", strings_id),
            entry("variables", variables_id),
        ],
        false,
    )?;
    doc.set_root(root);
    let mut params = Params::new();
    params.set(P_ORIGINAL, input.to_vec());
    Ok((doc, params))
}

fn zlib_body(input: &[u8], ctx: &DecodeCtx<'_>, offset: usize) -> Result<Vec<u8>> {
    let body = input
        .get(offset..)
        .ok_or_else(|| bad("the compressed body is missing", offset))?;
    let decoder = ZlibDecoder::new(body);
    ctx.read_bounded(decoder, len_u64(input.len()), ASD_PARSE)
}

/// System 4 `.asd` / `.as2` / `.asdbak` files.
pub(crate) fn system4(input: &[u8], ctx: &DecodeCtx<'_>) -> Result<(SaveDoc, Params)> {
    let raw_size = u32_at(input, 4)
        .map(|value| value as usize)
        .ok_or_else(|| bad("the System 4 size field is cut short", 4))?;
    let mut params = Params::new();
    params.set(P_ORIGINAL, input.to_vec());

    if input.get(8) == Some(&SYS4_ENCRYPTED_MARK) {
        let mut doc = ctx.new_doc();
        let magic = readonly_text(&mut doc, "GD".to_owned(), "magic")?;
        let size = readonly_int(&mut doc, raw_size as i128, IntWidth::U32, "raw-size")?;
        let encrypted = {
            let mut node = Node::new(Value::Bool(true));
            node.flags.insert(NodeFlags::READ_ONLY);
            doc.push(node)?
        };
        let note = readonly_text(
            &mut doc,
            "encrypted System 4 save; the decryption key is not public".to_owned(),
            "note",
        )?;
        let mut body = Node::new(Value::Bytes(input.get(8..).unwrap_or_default().to_vec()));
        body.type_hint = Some(hint("raw"));
        body.flags.insert(NodeFlags::READ_ONLY);
        let body_id = doc.push(body)?;
        let root = object(
            &mut doc,
            "system4",
            vec![
                entry("magic", magic),
                entry("raw_size", size),
                entry("encrypted", encrypted),
                entry("note", note),
                entry("raw", body_id),
            ],
            false,
        )?;
        doc.set_root(root);
        return Ok((doc, params));
    }

    let raw = zlib_body(input, ctx, 8)?;
    if raw.len() != raw_size {
        return Err(bad(
            "the decompressed System 4 body does not match the size field",
            4,
        ));
    }
    params.set(P_RAW, raw.clone());

    if raw.starts_with(RSAVE_MAGIC) {
        return resume(&raw, ctx, params);
    }
    gsave(&raw, ctx, params)
}

fn resume(raw: &[u8], ctx: &DecodeCtx<'_>, params: Params) -> Result<(SaveDoc, Params)> {
    let mut cursor = Cursor::new(raw);
    cursor.seek(4)?;
    let (key, _, _) = cursor.string()?;
    let version = cursor.i32().unwrap_or(0);
    let mut doc = ctx.new_doc();
    let mut comment_ids = Vec::new();
    if version >= 7 {
        let count = cursor.i32().unwrap_or(0);
        if (0..1_000_000).contains(&count) {
            for _ in 0..count {
                if let Ok((text, _, _)) = cursor.string() {
                    comment_ids.push(readonly_text(&mut doc, text, "comment")?);
                }
            }
        }
    }
    let key_id = readonly_text(&mut doc, key, "key")?;
    let version_id = readonly_int(&mut doc, i128::from(version), IntWidth::I32, "version")?;
    let comments_id = array(&mut doc, "comments", comment_ids, true)?;
    let kind = readonly_text(&mut doc, "resume".to_owned(), "kind")?;
    let mut body = Node::new(Value::Bytes(raw.to_vec()));
    body.type_hint = Some(hint("raw"));
    body.flags.insert(NodeFlags::READ_ONLY);
    let body_id = doc.push(body)?;
    let root = object(
        &mut doc,
        "system4",
        vec![
            entry("kind", kind),
            entry("key", key_id),
            entry("version", version_id),
            entry("comments", comments_id),
            entry("raw", body_id),
        ],
        false,
    )?;
    doc.set_root(root);
    Ok((doc, params))
}

fn gsave(raw: &[u8], ctx: &DecodeCtx<'_>, params: Params) -> Result<(SaveDoc, Params)> {
    let mut cursor = Cursor::new(raw);
    let (key, _, _) = cursor
        .string()
        .map_err(|_| bad("the save key is cut short", 0))?;
    let uk1 = cursor.i32()?;
    let version = cursor.i32()?;
    let _uk2 = cursor.i32()?;
    let nr_ain_globals = cursor.i32()?;
    if !matches!(version, 4 | 5 | 7) {
        return unknown_body(raw, ctx, params, version);
    }
    let records_offset = cursor.u32()? as usize;
    let nr_records = cursor.i32()?;
    let globals_offset = cursor.u32()? as usize;
    let nr_globals = cursor.i32()?;
    let strings_offset = cursor.u32()? as usize;
    let nr_strings = cursor.i32()?;
    let arrays_offset = cursor.u32()? as usize;
    let nr_arrays = cursor.i32()?;
    let keyvals_offset = cursor.u32()? as usize;
    let nr_keyvals = cursor.i32()?;
    let group = if version >= 5 {
        cursor.string().map(|(text, _, _)| text).unwrap_or_default()
    } else {
        String::new()
    };
    if nr_globals < 0 || nr_strings < 0 {
        return Err(bad("a System 4 section count is negative", 0));
    }
    if globals_offset > raw.len() || strings_offset > raw.len() {
        return Err(bad("a System 4 section offset is past the end", 0));
    }

    let mut doc = ctx.new_doc();
    let key_id = readonly_text(&mut doc, key, "key")?;
    let version_id = readonly_int(&mut doc, i128::from(version), IntWidth::I32, "version")?;
    let group_id = readonly_text(&mut doc, group, "group")?;
    let uk1_id = readonly_int(&mut doc, i128::from(uk1), IntWidth::I32, "reserved")?;
    let nr_ain_id = readonly_int(
        &mut doc,
        i128::from(nr_ain_globals),
        IntWidth::I32,
        "ain-globals",
    )?;
    let records_count = readonly_int(&mut doc, i128::from(nr_records), IntWidth::I32, "count")?;
    let globals_count = readonly_int(&mut doc, i128::from(nr_globals), IntWidth::I32, "count")?;
    let strings_count = readonly_int(&mut doc, i128::from(nr_strings), IntWidth::I32, "count")?;
    let arrays_count = readonly_int(&mut doc, i128::from(nr_arrays), IntWidth::I32, "count")?;
    let keyvals_count = readonly_int(&mut doc, i128::from(nr_keyvals), IntWidth::I32, "count")?;
    let arrays_offset_id = readonly_int(&mut doc, arrays_offset as i128, IntWidth::U32, "offset")?;
    let keyvals_offset_id =
        readonly_int(&mut doc, keyvals_offset as i128, IntWidth::U32, "offset")?;
    let records_offset_id =
        readonly_int(&mut doc, records_offset as i128, IntWidth::U32, "offset")?;
    let counts_id = object(
        &mut doc,
        "counts",
        vec![
            entry("records", records_count),
            entry("globals", globals_count),
            entry("strings", strings_count),
            entry("arrays", arrays_count),
            entry("keyvals", keyvals_count),
            entry("arrays_offset", arrays_offset_id),
            entry("keyvals_offset", keyvals_offset_id),
            entry("records_offset", records_offset_id),
        ],
        true,
    )?;

    let mut table = Vec::new();
    let mut string_spans: Vec<(String, usize, usize)> = Vec::new();
    cursor.seek(strings_offset)?;
    for _ in 0..nr_strings {
        let (text, start, end) = cursor.string()?;
        string_spans.push((text.clone(), start, end));
        let mut node = Node::new(Value::Str(text));
        node.type_hint = Some(hint("string-table"));
        node.flags.insert(NodeFlags::READ_ONLY);
        table.push(doc.push(node)?);
    }
    let strings_id = array(&mut doc, "strings", table, true)?;

    let mut globals = Vec::new();
    cursor.seek(globals_offset)?;
    for _ in 0..nr_globals {
        let data_type = cursor.i32()?;
        let value_offset = cursor.pos;
        let value = cursor.i32()?;
        let (name, _, _) = cursor.string()?;
        if version <= 5 {
            let _unknown = cursor.i32()?;
        }
        let value_id = global_value(&mut doc, data_type, value, value_offset, &string_spans)?;
        let type_id = readonly_int(&mut doc, i128::from(data_type), IntWidth::I32, "type")?;
        let mut name_node = Node::new(Value::Str(name));
        name_node.type_hint = Some(hint("name"));
        name_node.flags.insert(NodeFlags::READ_ONLY);
        let name_id = doc.push(name_node)?;
        globals.push(object(
            &mut doc,
            "global",
            vec![
                entry("name", name_id),
                entry("type", type_id),
                entry("value", value_id),
            ],
            true,
        )?);
    }
    let globals_id = array(&mut doc, "globals", globals, true)?;
    let kind = readonly_text(&mut doc, "global".to_owned(), "kind")?;
    let root = object(
        &mut doc,
        "system4",
        vec![
            entry("kind", kind),
            entry("key", key_id),
            entry("version", version_id),
            entry("group", group_id),
            entry("reserved", uk1_id),
            entry("ain_globals", nr_ain_id),
            entry("counts", counts_id),
            entry("globals", globals_id),
            entry("strings", strings_id),
        ],
        false,
    )?;
    doc.set_root(root);
    Ok((doc, params))
}

fn global_value(
    doc: &mut SaveDoc,
    data_type: i32,
    value: i32,
    offset: usize,
    strings: &[(String, usize, usize)],
) -> Result<NodeId> {
    match data_type {
        10 | 55 => {
            let mut node = Node::new(Value::Int {
                v: i128::from(value),
                width: IntWidth::I32,
            });
            node.type_hint = Some(hint("value-int"));
            node.span = Some(span(offset, offset.saturating_add(4)));
            doc.push(node)
        }
        47 => {
            let mut node = Node::new(Value::Bool(value != 0));
            node.type_hint = Some(hint("value-bool"));
            node.span = Some(span(offset, offset.saturating_add(4)));
            doc.push(node)
        }
        11 => {
            let number = f32::from_bits(value as u32);
            let mut node = Node::new(Value::Float {
                v: f64::from(number),
                width: FloatWidth::F32,
            });
            node.type_hint = Some(hint("value-float"));
            node.span = Some(span(offset, offset.saturating_add(4)));
            doc.push(node)
        }
        12 => {
            if value == 0x7fff_ffff || value < 0 {
                let mut node = Node::new(Value::Str(String::new()));
                node.type_hint = Some(hint("value-string"));
                node.flags.insert(NodeFlags::READ_ONLY);
                return doc.push(node);
            }
            let found = strings.get(value as usize);
            let (text, start, end) = found
                .map(|(text, start, end)| (text.clone(), *start, *end))
                .unwrap_or_default();
            let mut node = Node::new(Value::Str(text));
            node.type_hint = Some(hint("string"));
            if found.is_some() {
                node.span = Some(span(start, end));
            } else {
                node.flags.insert(NodeFlags::READ_ONLY);
            }
            doc.push(node)
        }
        _ => {
            let mut node = Node::new(Value::Int {
                v: i128::from(value),
                width: IntWidth::I32,
            });
            node.type_hint = Some(hint("value-ref"));
            node.flags.insert(NodeFlags::READ_ONLY);
            node.span = Some(span(offset, offset.saturating_add(4)));
            doc.push(node)
        }
    }
}

fn unknown_body(
    raw: &[u8],
    ctx: &DecodeCtx<'_>,
    params: Params,
    version: i32,
) -> Result<(SaveDoc, Params)> {
    let mut doc = ctx.new_doc();
    let kind = readonly_text(&mut doc, "unknown".to_owned(), "kind")?;
    let version_id = readonly_int(&mut doc, i128::from(version), IntWidth::I32, "version")?;
    let mut body = Node::new(Value::Bytes(raw.to_vec()));
    body.type_hint = Some(hint("raw"));
    body.flags.insert(NodeFlags::READ_ONLY);
    let body_id = doc.push(body)?;
    let root = object(
        &mut doc,
        "system4",
        vec![
            entry("kind", kind),
            entry("version", version_id),
            entry("raw", body_id),
        ],
        false,
    )?;
    doc.set_root(root);
    Ok((doc, params))
}

/// Artemis `BOWS` / `BOWG` envelopes: a small header and a zlib stream.
pub(crate) fn artemis(input: &[u8], ctx: &DecodeCtx<'_>) -> Result<(SaveDoc, Params)> {
    let magic_bytes = input
        .get(..4)
        .ok_or_else(|| bad("the Artemis magic is cut short", 0))?;
    let magic = std::str::from_utf8(magic_bytes)
        .map_err(|_| bad("the Artemis magic is not text", 0))?
        .to_owned();
    let version = u32_at(input, 4).ok_or_else(|| bad("the Artemis version is cut short", 4))?;
    let raw_size = u32_at(input, 8).ok_or_else(|| bad("the Artemis size field is cut short", 8))?;
    let body = zlib_body(input, ctx, 12)?;
    if body.len() != raw_size as usize {
        return Err(bad("the Artemis body does not match the size field", 8));
    }
    let mut doc = ctx.new_doc();
    let magic_id = readonly_text(&mut doc, magic, "magic")?;
    let version_id = readonly_int(&mut doc, i128::from(version), IntWidth::U32, "version")?;
    let size_id = readonly_int(&mut doc, i128::from(raw_size), IntWidth::U32, "raw-size")?;
    let note = readonly_text(
        &mut doc,
        "Artemis engine save body; the inner layout is not publicly documented, so it stays read-only"
            .to_owned(),
        "note",
    )?;
    let mut body_node = Node::new(Value::Bytes(body));
    body_node.type_hint = Some(hint("raw"));
    body_node.flags.insert(NodeFlags::READ_ONLY);
    let body_id = doc.push(body_node)?;
    let root = object(
        &mut doc,
        "artemis",
        vec![
            entry("magic", magic_id),
            entry("version", version_id),
            entry("raw_size", size_id),
            entry("note", note),
            entry("body", body_id),
        ],
        false,
    )?;
    doc.set_root(root);
    let mut params = Params::new();
    params.set(P_ORIGINAL, input.to_vec());
    Ok((doc, params))
}
