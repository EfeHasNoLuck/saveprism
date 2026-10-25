//! `format.terraria-player`: the decrypted bytes of a Terraria player `.plr` save.
//!
//! See `mod.rs` for the layout and the public sources. Only the version 135+ Re-Logic layout is
//! decoded; an older or unknown layout is reported as unsupported instead of guessed.
//!
//! The document exposes the header values and the inventory slots. Everything else (appearance,
//! colors, armor, dyes and every field after the inventory) is copied back from the original
//! plaintext, which is kept as the read-only parameter `original`; an unchanged file therefore
//! reproduces its bytes exactly, and an edit patches only the field that changed.

use saveprism_core::limits::len_u64;
use saveprism_core::{
    Codec, Confidence, DecodeCtx, Decoded, Error, IntWidth, Node, NodeFlags, Params, Path, Result,
    SaveDoc, SniffCtx, TypeHint, Value,
};

use super::{
    FILE_TYPE_PLAYER, TERRARIA_PARSE, TERRARIA_UNSUPPORTED, TERRARIA_WRITE, relogic_header,
};

/// Number of main inventory slots written by the current layout.
pub(crate) const INVENTORY_SLOTS: usize = 58;
/// Armor slots in the version 124+ layout.
const ARMOR_SLOTS: usize = 20;
/// Dye slots in the version 124+ layout.
const DYE_SLOTS: usize = 10;
/// Oldest version with the Re-Logic header this reader places.
pub(crate) const MIN_VERSION: i32 = 135;
/// Newest version whose prefix layout this reader places (the current release is below this).
pub(crate) const MAX_VERSION: i32 = 400;
/// Longest character name the game accepts.
const MAX_NAME_CHARS: usize = 20;
/// Highest difficulty byte: 0 Classic, 1 Mediumcore, 2 Hardcore, 3 Journey.
const MAX_DIFFICULTY: u8 = 3;
/// Highest item id the writer accepts: a little above the item count of the current release
/// (Terraria 1.4.4 has about 5450 item ids), so a small game update does not lock out its new
/// items. An id the game cannot load is refused instead of being written.
const MAX_ITEM_ID: i32 = 5600;
/// Lowest item id the writer accepts: the legacy negative "net ids" run from -1 down to -48.
const MIN_ITEM_ID: i32 = -48;
/// Prefix of every hint this codec writes.
pub const HINT_PREFIX: &str = "Terraria:";
/// Parameter holding the original plaintext of the player file.
pub const P_ORIGINAL: &str = "original";

/// The `format.terraria-player` codec.
pub(crate) struct TerrariaPlayerCodec;

fn parse(message: impl Into<String>) -> Error {
    Error::new(TERRARIA_PARSE, message)
}

fn unsupported(message: impl Into<String>) -> Error {
    Error::new(TERRARIA_UNSUPPORTED, message)
}

fn write_error(message: impl Into<String>) -> Error {
    Error::new(TERRARIA_WRITE, message)
}

/// A byte range in the original plaintext.
#[derive(Clone, Copy, Debug)]
struct Span {
    start: usize,
    end: usize,
}

/// A bounded little-endian reader.
struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8]> {
        let end = self
            .pos
            .checked_add(count)
            .ok_or_else(|| parse("the player file is too large to read"))?;
        let slice = self
            .bytes
            .get(self.pos..end)
            .ok_or_else(|| parse("the player file ends in the middle of a field"))?;
        self.pos = end;
        Ok(slice)
    }

    fn skip(&mut self, count: usize) -> Result<()> {
        self.take(count).map(|_| ())
    }

    fn u8(&mut self) -> Result<u8> {
        let slice = self.take(1)?;
        slice
            .first()
            .copied()
            .ok_or_else(|| parse("a byte is missing"))
    }

    fn bool(&mut self) -> Result<bool> {
        Ok(self.u8()? != 0)
    }

    fn i32(&mut self) -> Result<i32> {
        let bytes = self.take(4)?;
        let array = <[u8; 4]>::try_from(bytes).map_err(|_| parse("four bytes are missing"))?;
        Ok(i32::from_le_bytes(array))
    }

    fn u32(&mut self) -> Result<u32> {
        let bytes = self.take(4)?;
        let array = <[u8; 4]>::try_from(bytes).map_err(|_| parse("four bytes are missing"))?;
        Ok(u32::from_le_bytes(array))
    }

    fn i64(&mut self) -> Result<i64> {
        let bytes = self.take(8)?;
        let array = <[u8; 8]>::try_from(bytes).map_err(|_| parse("eight bytes are missing"))?;
        Ok(i64::from_le_bytes(array))
    }

    fn u64(&mut self) -> Result<u64> {
        let bytes = self.take(8)?;
        let array = <[u8; 8]>::try_from(bytes).map_err(|_| parse("eight bytes are missing"))?;
        Ok(u64::from_le_bytes(array))
    }

    /// A .NET `BinaryReader.ReadString`: 7-bit encoded byte length, then UTF-8.
    fn dotnet_string(&mut self) -> Result<String> {
        let mut length: u32 = 0;
        for shift in [0_u32, 7, 14, 21, 28] {
            let byte = self.u8()?;
            let part = u32::from(byte & 0x7F)
                .checked_shl(shift)
                .ok_or_else(|| parse("the string length is too large"))?;
            length = length
                .checked_add(part)
                .ok_or_else(|| parse("the string length is too large"))?;
            if byte & 0x80 == 0 {
                let length = usize::try_from(length)
                    .map_err(|_| parse("the string length does not fit this machine"))?;
                let bytes = self.take(length)?;
                return String::from_utf8(bytes.to_vec())
                    .map_err(|_| parse("a player name is not valid UTF-8"));
            }
        }
        Err(parse("the string length prefix is longer than five bytes"))
    }
}

/// One decoded item slot and the spans of its three editable values.
struct Item {
    id: i32,
    stack: i32,
    prefix: u8,
    id_span: Span,
    stack_span: Span,
    prefix_span: Span,
}

/// The parsed prefix of a player file, with the spans the patcher needs.
struct Layout {
    version: i32,
    revision: u32,
    favorite: bool,
    flags_span: Span,
    name: String,
    name_span: Span,
    difficulty: u8,
    difficulty_span: Span,
    play_time: i64,
    play_time_span: Option<Span>,
    life: i32,
    life_span: Span,
    max_life: i32,
    max_life_span: Span,
    mana: i32,
    mana_span: Span,
    max_mana: i32,
    max_mana_span: Span,
    items: Vec<Item>,
    tail_start: usize,
}

fn read_layout(input: &[u8]) -> Result<Layout> {
    let mut reader = Reader::new(input);
    let version = reader.i32()?;
    if !(MIN_VERSION..=MAX_VERSION).contains(&version) {
        return Err(unsupported(format!(
            "player version {version} is outside the layout this reader places ({MIN_VERSION} to {MAX_VERSION}); open the save in the game once to upgrade it"
        )));
    }
    let header = reader.u64()?;
    if !relogic_header(header, FILE_TYPE_PLAYER) {
        return Err(parse("the player header is not the Re-Logic player header"));
    }
    let revision = reader.u32()?;
    let flags_start = reader.pos;
    let flags = reader.u64()?;
    let flags_end = reader.pos;
    let favorite = flags & 1 != 0;

    let name_start = reader.pos;
    let name = reader.dotnet_string()?;
    let name_end = reader.pos;

    let difficulty_start = reader.pos;
    let difficulty = reader.u8()?;
    let difficulty_end = reader.pos;

    let play_time_span = if version >= 138 {
        let start = reader.pos;
        let value = reader.i64()?;
        Some((
            Span {
                start,
                end: reader.pos,
            },
            value,
        ))
    } else {
        None
    };
    let play_time = play_time_span.map_or(0, |(_, value)| value);
    let play_time_span = play_time_span.map(|(span, _)| span);

    let _hair = reader.i32()?;
    if version >= 82 {
        let _hair_dye = reader.u8()?;
    }
    if version >= 283 {
        let _team = reader.u8()?;
    }
    if version >= 124 {
        reader.skip(2)?;
    } else if version >= 83 {
        reader.skip(1)?;
    }
    if version >= 119 {
        reader.skip(1)?;
    }
    // Skin variant (a boolean before version 107, still one byte).
    reader.skip(1)?;

    let life_start = reader.pos;
    let life = reader.i32()?;
    let life_end = reader.pos;
    let max_life_start = reader.pos;
    let max_life = reader.i32()?;
    let max_life_end = reader.pos;
    let mana_start = reader.pos;
    let mana = reader.i32()?;
    let mana_end = reader.pos;
    let max_mana_start = reader.pos;
    let max_mana = reader.i32()?;
    let max_mana_end = reader.pos;

    if version >= 125 {
        reader.skip(1)?;
    }
    if version >= 229 {
        reader.skip(2)?;
        if version >= 256 {
            reader.skip(1)?;
        }
        if version >= 260 {
            reader.skip(6)?;
        }
    }
    if version >= 182 {
        reader.skip(1)?;
    }
    if version >= 128 {
        reader.skip(4)?;
    }
    if version >= 254 {
        reader.skip(8)?;
    }
    // Seven colors, RGB.
    reader.skip(21)?;
    // Armor (id + prefix) and dyes.
    reader.skip(ARMOR_SLOTS.checked_mul(5).unwrap_or(0))?;
    reader.skip(DYE_SLOTS.checked_mul(5).unwrap_or(0))?;

    let mut items = Vec::with_capacity(INVENTORY_SLOTS);
    for _ in 0..INVENTORY_SLOTS {
        let id_start = reader.pos;
        let id = reader.i32()?;
        let id_end = reader.pos;
        let stack_start = reader.pos;
        let stack = reader.i32()?;
        let stack_end = reader.pos;
        let prefix_start = reader.pos;
        let prefix = reader.u8()?;
        let prefix_end = reader.pos;
        if version >= 114 {
            let _favorite = reader.bool()?;
        }
        items.push(Item {
            id,
            stack,
            prefix,
            id_span: Span {
                start: id_start,
                end: id_end,
            },
            stack_span: Span {
                start: stack_start,
                end: stack_end,
            },
            prefix_span: Span {
                start: prefix_start,
                end: prefix_end,
            },
        });
    }

    if reader.pos > input.len() {
        return Err(parse("the player file ends inside the inventory"));
    }
    Ok(Layout {
        version,
        revision,
        favorite,
        flags_span: Span {
            start: flags_start,
            end: flags_end,
        },
        name,
        name_span: Span {
            start: name_start,
            end: name_end,
        },
        difficulty,
        difficulty_span: Span {
            start: difficulty_start,
            end: difficulty_end,
        },
        play_time,
        play_time_span,
        life,
        life_span: Span {
            start: life_start,
            end: life_end,
        },
        max_life,
        max_life_span: Span {
            start: max_life_start,
            end: max_life_end,
        },
        mana,
        mana_span: Span {
            start: mana_start,
            end: mana_end,
        },
        max_mana,
        max_mana_span: Span {
            start: max_mana_start,
            end: max_mana_end,
        },
        items,
        tail_start: reader.pos,
    })
}

/// Offset of the difficulty byte, for tests that patch a fixture.
#[cfg(test)]
pub(crate) fn difficulty_offset(input: &[u8]) -> Option<usize> {
    read_layout(input)
        .ok()
        .map(|layout| layout.difficulty_span.start)
}

fn int(value: i128, width: IntWidth, flags: NodeFlags) -> Node {
    let mut node = Node::new(Value::Int { v: value, width });
    node.flags = flags;
    node
}

fn text(value: &str, flags: NodeFlags) -> Node {
    let mut node = Node::new(Value::Str(value.to_string()));
    node.flags = flags;
    node
}

fn boolean(value: bool) -> Node {
    Node::new(Value::Bool(value))
}

fn hint(name: &str) -> TypeHint {
    TypeHint::new(format!("{HINT_PREFIX}{name}"))
}

fn build_doc(input: &[u8], layout: &Layout, ctx: &mut DecodeCtx<'_>) -> Result<SaveDoc> {
    let mut doc = ctx.new_doc();
    let read_only = NodeFlags::READ_ONLY;

    let mut roots: Vec<(&str, Node)> = Vec::new();
    roots.push((
        "version",
        int(i128::from(layout.version), IntWidth::I32, read_only),
    ));
    roots.push(("magic", text("relogic", read_only)));
    roots.push((
        "file_type",
        int(i128::from(FILE_TYPE_PLAYER), IntWidth::I32, read_only),
    ));
    roots.push((
        "revision",
        int(i128::from(layout.revision), IntWidth::I32, read_only),
    ));
    roots.push(("favorite", boolean(layout.favorite)));
    roots.push(("name", text(&layout.name, NodeFlags::empty())));
    roots.push((
        "difficulty",
        int(
            i128::from(layout.difficulty),
            IntWidth::I32,
            NodeFlags::empty(),
        ),
    ));
    roots.push((
        "play_time_ticks",
        int(
            i128::from(layout.play_time),
            IntWidth::I64,
            if layout.play_time_span.is_some() {
                NodeFlags::empty()
            } else {
                read_only
            },
        ),
    ));
    roots.push((
        "life",
        int(i128::from(layout.life), IntWidth::I32, NodeFlags::empty()),
    ));
    roots.push((
        "max_life",
        int(
            i128::from(layout.max_life),
            IntWidth::I32,
            NodeFlags::empty(),
        ),
    ));
    roots.push((
        "mana",
        int(i128::from(layout.mana), IntWidth::I32, NodeFlags::empty()),
    ));
    roots.push((
        "max_mana",
        int(
            i128::from(layout.max_mana),
            IntWidth::I32,
            NodeFlags::empty(),
        ),
    ));

    let mut slots = Vec::with_capacity(layout.items.len());
    for item in &layout.items {
        let id = doc.push(int(i128::from(item.id), IntWidth::I32, NodeFlags::empty()))?;
        let stack = doc.push(int(
            i128::from(item.stack),
            IntWidth::I32,
            NodeFlags::empty(),
        ))?;
        let prefix = doc.push(int(
            i128::from(item.prefix),
            IntWidth::I32,
            NodeFlags::empty(),
        ))?;
        let mut node = Node::new(Value::Object(vec![
            ("id".to_string(), id),
            ("stack".to_string(), stack),
            ("prefix".to_string(), prefix),
        ]));
        node.type_hint = Some(hint("item"));
        slots.push(doc.push(node)?);
    }
    let mut inventory = Node::new(Value::Array(slots));
    inventory.flags = read_only;
    roots.push(("inventory", inventory));

    let tail = input
        .get(layout.tail_start..)
        .ok_or_else(|| parse("the player tail is missing"))?
        .to_vec();
    let mut tail_node = Node::new(Value::Bytes(tail));
    tail_node.flags = read_only;
    roots.push(("tail", tail_node));

    let mut entries = Vec::with_capacity(roots.len());
    for (key, node) in roots {
        entries.push((key.to_string(), doc.push(node)?));
    }
    let mut root = Node::new(Value::Object(entries));
    root.type_hint = Some(hint("player"));
    let root_id = doc.push(root)?;
    doc.set_root(root_id);
    Ok(doc)
}

fn resolve<'a>(doc: &'a SaveDoc, path: &str) -> Result<&'a saveprism_core::Node> {
    let parsed = Path::parse(path).map_err(|_| write_error("bad internal path"))?;
    let id = doc
        .resolve(&parsed)
        .ok_or_else(|| write_error(format!("the document has no {path}")))?;
    doc.get(id)
        .ok_or_else(|| write_error(format!("the document has no {path}")))
}

fn int_at(doc: &SaveDoc, path: &str) -> Result<i128> {
    match &resolve(doc, path)?.value {
        Value::Int { v, .. } => Ok(*v),
        _ => Err(write_error(format!("{path} is not an integer"))),
    }
}

fn i32_at(doc: &SaveDoc, path: &str) -> Result<i32> {
    i32::try_from(int_at(doc, path)?)
        .map_err(|_| write_error(format!("{path} does not fit a 32-bit integer")))
}

fn i64_at(doc: &SaveDoc, path: &str) -> Result<i64> {
    i64::try_from(int_at(doc, path)?)
        .map_err(|_| write_error(format!("{path} does not fit a 64-bit integer")))
}

fn u8_at(doc: &SaveDoc, path: &str) -> Result<u8> {
    u8::try_from(int_at(doc, path)?)
        .map_err(|_| write_error(format!("{path} must be between 0 and 255")))
}

fn bool_at(doc: &SaveDoc, path: &str) -> Result<bool> {
    match &resolve(doc, path)?.value {
        Value::Bool(value) => Ok(*value),
        _ => Err(write_error(format!("{path} is not a boolean"))),
    }
}

fn text_at(doc: &SaveDoc, path: &str) -> Result<String> {
    match &resolve(doc, path)?.value {
        Value::Str(text) => Ok(text.clone()),
        _ => Err(write_error(format!("{path} is not text"))),
    }
}

/// Canonical .NET `BinaryWriter.Write(string)` bytes.
fn dotnet_string_bytes(text: &str) -> Result<Vec<u8>> {
    let bytes = text.as_bytes();
    let mut length = u32::try_from(bytes.len())
        .map_err(|_| write_error("the text is too long for the player format"))?;
    let mut out = Vec::with_capacity(bytes.len().saturating_add(5));
    loop {
        let mut byte =
            u8::try_from(length & 0x7F).map_err(|_| write_error("the text length does not fit"))?;
        length >>= 7;
        if length != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if length == 0 {
            break;
        }
    }
    out.extend_from_slice(bytes);
    Ok(out)
}

/// A replacement of `original[start..end]` with `bytes`.
struct Patch {
    start: usize,
    end: usize,
    bytes: Vec<u8>,
}

fn push_patch(patches: &mut Vec<Patch>, original: &[u8], span: Span, bytes: Vec<u8>) -> Result<()> {
    let current = original
        .get(span.start..span.end)
        .ok_or_else(|| write_error("a field span is outside the original file"))?;
    if current != bytes {
        patches.push(Patch {
            start: span.start,
            end: span.end,
            bytes,
        });
    }
    Ok(())
}

fn write_player(doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
    let original = params.bytes(P_ORIGINAL)?;
    let layout = read_layout(original)?;

    // Only a changed value is checked, so a file that already holds an odd value still saves.
    let name = text_at(doc, "/name")?;
    if name != layout.name && name.chars().count() > MAX_NAME_CHARS {
        return Err(write_error(format!(
            "a character name can have at most {MAX_NAME_CHARS} characters"
        )));
    }
    let difficulty = u8_at(doc, "/difficulty")?;
    if difficulty != layout.difficulty && difficulty > MAX_DIFFICULTY {
        return Err(write_error(format!(
            "the difficulty must be 0 (Classic), 1 (Mediumcore), 2 (Hardcore) or {MAX_DIFFICULTY} (Journey), not {difficulty}"
        )));
    }
    let play_time = i64_at(doc, "/play_time_ticks")?;
    let life = i32_at(doc, "/life")?;
    let max_life = i32_at(doc, "/max_life")?;
    let mana = i32_at(doc, "/mana")?;
    let max_mana = i32_at(doc, "/max_mana")?;
    let favorite = bool_at(doc, "/favorite")?;

    let mut patches = Vec::new();
    if favorite != layout.favorite {
        let flags = match original.get(layout.flags_span.start..layout.flags_span.end) {
            Some(bytes) => match <[u8; 8]>::try_from(bytes) {
                Ok(array) => u64::from_le_bytes(array),
                Err(_) => return Err(write_error("the favorite flags are missing")),
            },
            None => return Err(write_error("the favorite flags are missing")),
        };
        let flags = if favorite { flags | 1 } else { flags & !1 };
        push_patch(
            &mut patches,
            original,
            layout.flags_span,
            flags.to_le_bytes().to_vec(),
        )?;
    }
    push_patch(
        &mut patches,
        original,
        layout.name_span,
        dotnet_string_bytes(&name)?,
    )?;
    push_patch(
        &mut patches,
        original,
        layout.difficulty_span,
        vec![difficulty],
    )?;
    if let Some(span) = layout.play_time_span {
        push_patch(
            &mut patches,
            original,
            span,
            play_time.to_le_bytes().to_vec(),
        )?;
    }
    push_patch(
        &mut patches,
        original,
        layout.life_span,
        life.to_le_bytes().to_vec(),
    )?;
    push_patch(
        &mut patches,
        original,
        layout.max_life_span,
        max_life.to_le_bytes().to_vec(),
    )?;
    push_patch(
        &mut patches,
        original,
        layout.mana_span,
        mana.to_le_bytes().to_vec(),
    )?;
    push_patch(
        &mut patches,
        original,
        layout.max_mana_span,
        max_mana.to_le_bytes().to_vec(),
    )?;
    for (index, item) in layout.items.iter().enumerate() {
        let id = i32_at(doc, &format!("/inventory/{index}/id"))?;
        if id != item.id && !(MIN_ITEM_ID..=MAX_ITEM_ID).contains(&id) {
            return Err(write_error(format!(
                "inventory slot {index} cannot hold item id {id}: the game loads ids from {MIN_ITEM_ID} to {MAX_ITEM_ID} (0 is an empty slot)"
            )));
        }
        let stack = i32_at(doc, &format!("/inventory/{index}/stack"))?;
        let prefix = u8_at(doc, &format!("/inventory/{index}/prefix"))?;
        push_patch(
            &mut patches,
            original,
            item.id_span,
            id.to_le_bytes().to_vec(),
        )?;
        push_patch(
            &mut patches,
            original,
            item.stack_span,
            stack.to_le_bytes().to_vec(),
        )?;
        push_patch(&mut patches, original, item.prefix_span, vec![prefix])?;
    }

    patches.sort_by_key(|patch| patch.start);
    let mut out = Vec::with_capacity(original.len());
    let mut cursor = 0;
    for patch in &patches {
        if patch.start < cursor {
            return Err(write_error("two edits overlap in the player file"));
        }
        let between = original
            .get(cursor..patch.start)
            .ok_or_else(|| write_error("a patch is outside the original file"))?;
        out.extend_from_slice(between);
        out.extend_from_slice(&patch.bytes);
        cursor = patch.end;
    }
    let rest = original
        .get(cursor..)
        .ok_or_else(|| write_error("the tail is outside the original file"))?;
    out.extend_from_slice(rest);
    // The AES layer needs whole blocks, so an edit that changed the encoded length is padded with
    // zeros (the game's reader stops after the player data and ignores what follows). A plaintext
    // that was not a whole number of blocks to begin with is not encrypted and stays unpadded, so
    // an unedited file always reproduces its bytes.
    if original.len().is_multiple_of(16) {
        let remainder = out.len() % 16;
        if remainder != 0 {
            out.resize(out.len().saturating_add(16 - remainder), 0);
        }
    }
    Ok(out)
}

impl Codec for TerrariaPlayerCodec {
    fn id(&self) -> &'static str {
        "format.terraria-player"
    }

    fn sniff(&self, input: &[u8], _ctx: &SniffCtx<'_>) -> Confidence {
        let version = match input.get(..4) {
            Some(bytes) => match <[u8; 4]>::try_from(bytes) {
                Ok(array) => i32::from_le_bytes(array),
                Err(_) => return Confidence::NONE,
            },
            None => return Confidence::NONE,
        };
        let header = match input.get(4..12) {
            Some(bytes) => match <[u8; 8]>::try_from(bytes) {
                Ok(array) => u64::from_le_bytes(array),
                Err(_) => return Confidence::NONE,
            },
            None => return Confidence::NONE,
        };
        if (MIN_VERSION..=MAX_VERSION).contains(&version)
            && relogic_header(header, FILE_TYPE_PLAYER)
        {
            Confidence::CERTAIN
        } else {
            Confidence::NONE
        }
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<Decoded> {
        ctx.limits().check_input(len_u64(input.len()))?;
        let layout = read_layout(input)?;
        let doc = build_doc(input, &layout, ctx)?;
        ctx.charge_nodes(len_u64(doc.len()))?;
        let mut params = Params::new();
        params.set(P_ORIGINAL, input.to_vec());
        Ok(Decoded { doc, params })
    }

    fn encode(&self, doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
        write_player(doc, params)
    }
}
