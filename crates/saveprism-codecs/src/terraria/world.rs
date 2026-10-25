//! `format.terraria-world`: a Terraria world `.wld` save.
//!
//! See `mod.rs` for the layout and the public sources. The version 267+ header is decoded; every
//! other part of the file (tiles, chests, signs, NPCs, tile entities, pressure plates, town
//! manager, bestiary, creative powers and the footer) is exposed as read-only `sections` bytes and
//! copied back unchanged. Section pointers are recomputed when an edited header changes length.

use saveprism_core::limits::len_u64;
use saveprism_core::{
    Codec, Confidence, DecodeCtx, Decoded, Error, FloatWidth, IntWidth, Node, NodeFlags, Params,
    Path, Result, SaveDoc, SniffCtx, TypeHint, Value,
};

use super::{FILE_TYPE_WORLD, TERRARIA_PARSE, TERRARIA_UNSUPPORTED, TERRARIA_WRITE};

/// Oldest world version whose header this reader places.
pub(crate) const MIN_VERSION: u32 = 267;
/// Newest world version whose header this reader places (the current release is below this).
pub(crate) const MAX_VERSION: u32 = 600;
/// Sections in a version 220+ world file.
const SECTION_COUNT: usize = 11;
/// Prefix of every hint this codec writes.
pub const HINT_PREFIX: &str = "Terraria:";

/// The `format.terraria-world` codec.
pub(crate) struct TerrariaWorldCodec;

fn parse(message: impl Into<String>) -> Error {
    Error::new(TERRARIA_PARSE, message)
}

fn unsupported(message: impl Into<String>) -> Error {
    Error::new(TERRARIA_UNSUPPORTED, message)
}

fn write_error(message: impl Into<String>) -> Error {
    Error::new(TERRARIA_WRITE, message)
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
            .ok_or_else(|| parse("the world file is too large to read"))?;
        let slice = self
            .bytes
            .get(self.pos..end)
            .ok_or_else(|| parse("the world file ends in the middle of a field"))?;
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

    fn i16(&mut self) -> Result<i16> {
        let bytes = self.take(2)?;
        let array = <[u8; 2]>::try_from(bytes).map_err(|_| parse("two bytes are missing"))?;
        Ok(i16::from_le_bytes(array))
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

    fn f64(&mut self) -> Result<f64> {
        let bytes = self.take(8)?;
        let array = <[u8; 8]>::try_from(bytes).map_err(|_| parse("eight bytes are missing"))?;
        Ok(f64::from_le_bytes(array))
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
                    .map_err(|_| parse("a world string is not valid UTF-8"));
            }
        }
        Err(parse("the string length prefix is longer than five bytes"))
    }
}

/// The fixed part of a world file: version, header, pointers and the frame bit array.
struct SectionHeader {
    version: u32,
    magic: String,
    revision: u32,
    favorite: bool,
    pointers: Vec<i32>,
    frame_bits: Vec<u8>,
    header_start: usize,
    sections_start: usize,
}

fn read_section_header(input: &[u8]) -> Result<SectionHeader> {
    let mut reader = Reader::new(input);
    let version = reader.u32()?;
    if !(MIN_VERSION..=MAX_VERSION).contains(&version) {
        return Err(unsupported(format!(
            "world version {version} is outside the header layout this reader places ({MIN_VERSION} to {MAX_VERSION})"
        )));
    }
    let magic_bytes = reader.take(7)?;
    let magic = match magic_bytes {
        b"relogic" => "relogic".to_string(),
        b"xindong" => "xindong".to_string(),
        _ => return Err(parse("the world header is not the Re-Logic world header")),
    };
    let file_type = reader.u8()?;
    if file_type != FILE_TYPE_WORLD {
        return Err(parse("the header does not say this is a world file"));
    }
    let revision = reader.u32()?;
    let flags = reader.u64()?;
    let favorite = flags & 1 != 0;

    let count = reader.i16()?;
    let count = usize::try_from(count).map_err(|_| parse("the section count is negative"))?;
    if count != SECTION_COUNT {
        return Err(unsupported(format!(
            "this world has {count} sections; only the current {SECTION_COUNT} section layout is read"
        )));
    }
    let mut pointers = Vec::with_capacity(count);
    for _ in 0..count {
        pointers.push(reader.i32()?);
    }
    let frame_start = reader.pos;
    let bit_count = reader.i16()?;
    let bit_count =
        usize::try_from(bit_count).map_err(|_| parse("the tile frame bit count is negative"))?;
    reader.skip(bit_count.div_ceil(8))?;
    let frame_bits = input
        .get(frame_start..reader.pos)
        .ok_or_else(|| parse("the tile frame bits are missing"))?
        .to_vec();
    let header_start = reader.pos;

    let mut previous = header_start;
    for (index, pointer) in pointers.iter().enumerate() {
        let value =
            usize::try_from(*pointer).map_err(|_| parse("a section pointer is negative"))?;
        if value < previous || value > input.len() {
            return Err(parse(format!(
                "section pointer {index} ({value}) is not inside the file in order"
            )));
        }
        previous = value;
    }
    let first = pointers
        .first()
        .copied()
        .map(|pointer| usize::try_from(pointer).unwrap_or(0))
        .ok_or_else(|| parse("the world has no section pointers"))?;
    if first != header_start {
        return Err(parse("the first section pointer does not end the header"));
    }
    let sections_start = pointers
        .get(1)
        .copied()
        .map(|pointer| usize::try_from(pointer).unwrap_or(0))
        .ok_or_else(|| parse("the world has no header section pointer"))?;
    if sections_start < header_start {
        return Err(parse("the second section pointer is before the header"));
    }
    Ok(SectionHeader {
        version,
        magic,
        revision,
        favorite,
        pointers,
        frame_bits,
        header_start,
        sections_start,
    })
}

fn int(value: i128, width: IntWidth, flags: NodeFlags) -> Node {
    let mut node = Node::new(Value::Int { v: value, width });
    node.flags = flags;
    node
}

fn float(value: f64, flags: NodeFlags) -> Node {
    let mut node = Node::new(Value::Float {
        v: value,
        width: FloatWidth::F64,
    });
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

/// One header value, in the order it is written.
enum Field {
    Int(i128, IntWidth, NodeFlags),
    Float(f64, NodeFlags),
    Bool(bool),
    Text(String, NodeFlags),
    Bytes(Vec<u8>, NodeFlags),
}

/// The world header values, built while parsing and written in the same order.
struct Header {
    fields: Vec<(&'static str, Field)>,
    tail: Vec<u8>,
}

fn push_int(fields: &mut Vec<(&'static str, Field)>, key: &'static str, value: i128) {
    fields.push((key, Field::Int(value, IntWidth::I32, NodeFlags::empty())));
}

fn push_read_only_int(fields: &mut Vec<(&'static str, Field)>, key: &'static str, value: i128) {
    fields.push((key, Field::Int(value, IntWidth::I32, NodeFlags::READ_ONLY)));
}

fn push_bool(fields: &mut Vec<(&'static str, Field)>, key: &'static str, value: bool) {
    fields.push((key, Field::Bool(value)));
}

fn read_header(section: &[u8], version: u32) -> Result<Header> {
    let mut reader = Reader::new(section);
    let mut fields: Vec<(&'static str, Field)> = Vec::new();

    let name = reader.dotnet_string()?;
    fields.push(("name", Field::Text(name, NodeFlags::empty())));
    let seed = reader.dotnet_string()?;
    fields.push(("seed", Field::Text(seed, NodeFlags::empty())));
    let world_gen_version = reader.u64()?;
    fields.push((
        "world_gen_version",
        Field::Int(
            i128::from(world_gen_version),
            IntWidth::I64,
            NodeFlags::READ_ONLY,
        ),
    ));
    let guid = reader.take(16)?.to_vec();
    fields.push(("guid", Field::Bytes(guid, NodeFlags::READ_ONLY)));
    push_int(&mut fields, "id", i128::from(reader.i32()?));
    push_int(&mut fields, "left", i128::from(reader.i32()?));
    push_int(&mut fields, "right", i128::from(reader.i32()?));
    push_int(&mut fields, "top", i128::from(reader.i32()?));
    push_int(&mut fields, "bottom", i128::from(reader.i32()?));
    push_int(&mut fields, "height", i128::from(reader.i32()?));
    push_int(&mut fields, "width", i128::from(reader.i32()?));
    push_int(&mut fields, "game_mode", i128::from(reader.i32()?));

    for (key, since) in [
        ("drunk_world", 222_u32),
        ("good_world", 227),
        ("tenth_anniversary_world", 238),
        ("dont_starve_world", 239),
        ("not_the_bees_world", 241),
        ("remix_world", 249),
        ("no_traps_world", 266),
        ("zenith_world", 267),
        ("skyblock_world", 302),
    ] {
        if version >= since {
            push_bool(&mut fields, key, reader.bool()?);
        }
    }

    push_read_only_int(&mut fields, "creation_time", i128::from(reader.i64()?));
    if version >= 284 {
        push_read_only_int(&mut fields, "last_played", i128::from(reader.i64()?));
    }
    push_int(&mut fields, "moon_type", i128::from(reader.u8()?));
    for key in ["tree_x0", "tree_x1", "tree_x2"] {
        push_int(&mut fields, key, i128::from(reader.i32()?));
    }
    for key in ["tree_style0", "tree_style1", "tree_style2", "tree_style3"] {
        push_int(&mut fields, key, i128::from(reader.i32()?));
    }
    for key in ["cave_back_x0", "cave_back_x1", "cave_back_x2"] {
        push_int(&mut fields, key, i128::from(reader.i32()?));
    }
    for key in [
        "cave_back_style0",
        "cave_back_style1",
        "cave_back_style2",
        "cave_back_style3",
        "ice_back_style",
        "jungle_back_style",
        "hell_back_style",
    ] {
        push_int(&mut fields, key, i128::from(reader.i32()?));
    }
    push_int(&mut fields, "spawn_x", i128::from(reader.i32()?));
    push_int(&mut fields, "spawn_y", i128::from(reader.i32()?));
    fields.push((
        "ground_level",
        Field::Float(reader.f64()?, NodeFlags::empty()),
    ));
    fields.push((
        "rock_level",
        Field::Float(reader.f64()?, NodeFlags::empty()),
    ));
    fields.push(("time", Field::Float(reader.f64()?, NodeFlags::empty())));
    push_bool(&mut fields, "day_time", reader.bool()?);
    push_int(&mut fields, "moon_phase", i128::from(reader.i32()?));
    push_bool(&mut fields, "blood_moon", reader.bool()?);
    push_bool(&mut fields, "eclipse", reader.bool()?);
    push_int(&mut fields, "dungeon_x", i128::from(reader.i32()?));
    push_int(&mut fields, "dungeon_y", i128::from(reader.i32()?));
    push_bool(&mut fields, "crimson", reader.bool()?);

    for key in [
        "downed_eye_of_cthulhu",
        "downed_eater_of_worlds",
        "downed_skeletron",
        "downed_queen_bee",
        "downed_destroyer",
        "downed_twins",
        "downed_skeletron_prime",
        "downed_any_mech",
        "downed_plantera",
        "downed_golem",
        "downed_slime_king",
        "saved_goblin",
        "saved_wizard",
        "saved_mech",
        "downed_goblins",
        "downed_clown",
        "downed_frost",
        "downed_pirates",
        "shadow_orb_smashed",
        "spawn_meteor",
    ] {
        push_bool(&mut fields, key, reader.bool()?);
    }
    push_int(&mut fields, "shadow_orb_count", i128::from(reader.u8()?));
    push_int(&mut fields, "altar_count", i128::from(reader.i32()?));
    push_bool(&mut fields, "hard_mode", reader.bool()?);
    if version >= 257 {
        push_bool(&mut fields, "party_of_doom", reader.bool()?);
    }
    push_int(&mut fields, "invasion_delay", i128::from(reader.i32()?));
    push_int(&mut fields, "invasion_size", i128::from(reader.i32()?));
    push_int(&mut fields, "invasion_type", i128::from(reader.i32()?));
    fields.push((
        "invasion_x",
        Field::Float(reader.f64()?, NodeFlags::empty()),
    ));
    fields.push((
        "slime_rain_time",
        Field::Float(reader.f64()?, NodeFlags::empty()),
    ));
    push_int(&mut fields, "sundial_cooldown", i128::from(reader.u8()?));
    push_bool(&mut fields, "is_raining", reader.bool()?);

    let tail = section
        .get(reader.pos..)
        .ok_or_else(|| parse("the world header tail is missing"))?
        .to_vec();
    Ok(Header { fields, tail })
}

fn build_doc(
    input: &[u8],
    meta: &SectionHeader,
    header: &Header,
    ctx: &mut DecodeCtx<'_>,
) -> Result<SaveDoc> {
    let mut doc = ctx.new_doc();
    let read_only = NodeFlags::READ_ONLY;

    let mut roots: Vec<(&str, Node)> = vec![(
        "version",
        int(i128::from(meta.version), IntWidth::I32, read_only),
    )];
    roots.push(("magic", text(&meta.magic, read_only)));
    roots.push((
        "file_type",
        int(i128::from(FILE_TYPE_WORLD), IntWidth::I32, read_only),
    ));
    roots.push((
        "revision",
        int(i128::from(meta.revision), IntWidth::I32, read_only),
    ));
    roots.push(("favorite", boolean(meta.favorite)));
    roots.push((
        "section_count",
        int(
            i128::try_from(meta.pointers.len()).unwrap_or(0),
            IntWidth::I32,
            read_only,
        ),
    ));

    let mut pointer_ids = Vec::with_capacity(meta.pointers.len());
    for pointer in &meta.pointers {
        pointer_ids.push(doc.push(int(i128::from(*pointer), IntWidth::I32, read_only))?);
    }
    let mut pointers = Node::new(Value::Array(pointer_ids));
    pointers.flags = read_only;
    roots.push(("section_pointers", pointers));

    let mut frame = Node::new(Value::Bytes(meta.frame_bits.clone()));
    frame.flags = read_only;
    roots.push(("tile_frame_important", frame));

    let mut header_entries = Vec::with_capacity(header.fields.len());
    for (key, field) in &header.fields {
        let node = match field {
            Field::Int(value, width, flags) => int(*value, *width, *flags),
            Field::Float(value, flags) => float(*value, *flags),
            Field::Bool(value) => boolean(*value),
            Field::Text(value, flags) => text(value, *flags),
            Field::Bytes(value, flags) => {
                let mut node = Node::new(Value::Bytes(value.clone()));
                node.flags = *flags;
                node
            }
        };
        header_entries.push(((*key).to_string(), doc.push(node)?));
    }
    let mut tail = Node::new(Value::Bytes(header.tail.clone()));
    tail.flags = read_only;
    header_entries.push(("tail".to_string(), doc.push(tail)?));
    let mut header_node = Node::new(Value::Object(header_entries));
    header_node.type_hint = Some(hint("world-header"));
    roots.push(("header", header_node));

    let sections = input
        .get(meta.sections_start..)
        .ok_or_else(|| parse("the world sections are missing"))?
        .to_vec();
    let mut sections_node = Node::new(Value::Bytes(sections));
    sections_node.flags = read_only;
    roots.push(("sections", sections_node));

    let mut entries = Vec::with_capacity(roots.len());
    for (key, node) in roots {
        entries.push((key.to_string(), doc.push(node)?));
    }
    let mut root = Node::new(Value::Object(entries));
    root.type_hint = Some(hint("world"));
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

fn u32_at(doc: &SaveDoc, path: &str) -> Result<u32> {
    u32::try_from(int_at(doc, path)?)
        .map_err(|_| write_error(format!("{path} does not fit an unsigned 32-bit integer")))
}

fn f64_at(doc: &SaveDoc, path: &str) -> Result<f64> {
    match &resolve(doc, path)?.value {
        Value::Float { v, .. } => Ok(*v),
        Value::Int { v, .. } => Ok(*v as f64),
        _ => Err(write_error(format!("{path} is not a number"))),
    }
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

fn bytes_at(doc: &SaveDoc, path: &str) -> Result<Vec<u8>> {
    match &resolve(doc, path)?.value {
        Value::Bytes(bytes) => Ok(bytes.clone()),
        _ => Err(write_error(format!("{path} is not bytes"))),
    }
}

fn dotnet_string_bytes(text: &str) -> Result<Vec<u8>> {
    let bytes = text.as_bytes();
    let mut length = u32::try_from(bytes.len())
        .map_err(|_| write_error("the text is too long for the world format"))?;
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

fn write_world(doc: &SaveDoc, _params: &Params) -> Result<Vec<u8>> {
    let version = u32_at(doc, "/version")?;
    if !(MIN_VERSION..=MAX_VERSION).contains(&version) {
        return Err(unsupported(format!(
            "world version {version} cannot be written by this codec"
        )));
    }
    let magic = text_at(doc, "/magic")?;
    if magic != "relogic" && magic != "xindong" {
        return Err(write_error("the world magic must be relogic or xindong"));
    }
    let revision = u32_at(doc, "/revision")?;
    let favorite = bool_at(doc, "/favorite")?;
    let pointer_count = usize::try_from(int_at(doc, "/section_count")?)
        .map_err(|_| write_error("the section count is negative"))?;
    if pointer_count != SECTION_COUNT {
        return Err(write_error(format!(
            "a world must have {SECTION_COUNT} sections"
        )));
    }
    let mut pointers = Vec::with_capacity(pointer_count);
    for index in 0..pointer_count {
        pointers.push(i32_at(doc, &format!("/section_pointers/{index}"))?);
    }
    let frame_bits = bytes_at(doc, "/tile_frame_important")?;
    let sections = bytes_at(doc, "/sections")?;

    let original_header_start = usize::try_from(pointers.first().copied().unwrap_or(-1))
        .map_err(|_| write_error("the first section pointer is not set"))?;
    let original_header_end = usize::try_from(pointers.get(1).copied().unwrap_or(-1))
        .map_err(|_| write_error("the second section pointer is not set"))?;
    if original_header_end < original_header_start {
        return Err(write_error("the section pointers are out of order"));
    }
    let original_header_len = original_header_end.saturating_sub(original_header_start);

    let mut out = Vec::new();
    out.extend_from_slice(&version.to_le_bytes());
    out.extend_from_slice(magic.as_bytes());
    out.push(FILE_TYPE_WORLD);
    out.extend_from_slice(&revision.to_le_bytes());
    out.extend_from_slice(&u64::from(favorite).to_le_bytes());
    let count = i16::try_from(SECTION_COUNT).map_err(|_| write_error("section count"))?;
    out.extend_from_slice(&count.to_le_bytes());
    let pointers_offset = out.len();
    out.resize(
        out.len().saturating_add(
            SECTION_COUNT
                .checked_mul(4)
                .ok_or_else(|| write_error("section pointer size"))?,
        ),
        0,
    );
    out.extend_from_slice(&frame_bits);
    let header_start = out.len();

    let header_prefix = "/header/";
    let name = text_at(doc, &format!("{header_prefix}name"))?;
    out.extend_from_slice(&dotnet_string_bytes(&name)?);
    let seed = text_at(doc, &format!("{header_prefix}seed"))?;
    out.extend_from_slice(&dotnet_string_bytes(&seed)?);
    out.extend_from_slice(
        &u64::try_from(int_at(doc, &format!("{header_prefix}world_gen_version"))?)
            .map_err(|_| write_error("the world generation version is out of range"))?
            .to_le_bytes(),
    );
    let guid = bytes_at(doc, &format!("{header_prefix}guid"))?;
    if guid.len() != 16 {
        return Err(write_error("the world GUID must be 16 bytes"));
    }
    out.extend_from_slice(&guid);

    let int_fields = [
        "id",
        "left",
        "right",
        "top",
        "bottom",
        "height",
        "width",
        "game_mode",
    ];
    for key in int_fields {
        out.extend_from_slice(&i32_at(doc, &format!("{header_prefix}{key}"))?.to_le_bytes());
    }
    let bool_fields = [
        "drunk_world",
        "good_world",
        "tenth_anniversary_world",
        "dont_starve_world",
        "not_the_bees_world",
        "remix_world",
        "no_traps_world",
        "zenith_world",
        "skyblock_world",
    ];
    for key in bool_fields {
        if key == "skyblock_world" && version < 302 {
            continue;
        }
        out.push(u8::from(bool_at(doc, &format!("{header_prefix}{key}"))?));
    }
    out.extend_from_slice(&i64_at(doc, &format!("{header_prefix}creation_time"))?.to_le_bytes());
    if version >= 284 {
        out.extend_from_slice(&i64_at(doc, &format!("{header_prefix}last_played"))?.to_le_bytes());
    }
    out.push(
        u8::try_from(int_at(doc, &format!("{header_prefix}moon_type"))?)
            .map_err(|_| write_error("the moon type must fit a byte"))?,
    );
    for key in ["tree_x0", "tree_x1", "tree_x2"] {
        out.extend_from_slice(&i32_at(doc, &format!("{header_prefix}{key}"))?.to_le_bytes());
    }
    for key in ["tree_style0", "tree_style1", "tree_style2", "tree_style3"] {
        out.extend_from_slice(&i32_at(doc, &format!("{header_prefix}{key}"))?.to_le_bytes());
    }
    for key in ["cave_back_x0", "cave_back_x1", "cave_back_x2"] {
        out.extend_from_slice(&i32_at(doc, &format!("{header_prefix}{key}"))?.to_le_bytes());
    }
    for key in [
        "cave_back_style0",
        "cave_back_style1",
        "cave_back_style2",
        "cave_back_style3",
        "ice_back_style",
        "jungle_back_style",
        "hell_back_style",
    ] {
        out.extend_from_slice(&i32_at(doc, &format!("{header_prefix}{key}"))?.to_le_bytes());
    }
    for key in ["spawn_x", "spawn_y"] {
        out.extend_from_slice(&i32_at(doc, &format!("{header_prefix}{key}"))?.to_le_bytes());
    }
    for key in ["ground_level", "rock_level", "time"] {
        out.extend_from_slice(&f64_at(doc, &format!("{header_prefix}{key}"))?.to_le_bytes());
    }
    out.push(u8::from(bool_at(doc, &format!("{header_prefix}day_time"))?));
    out.extend_from_slice(&i32_at(doc, &format!("{header_prefix}moon_phase"))?.to_le_bytes());
    out.push(u8::from(bool_at(
        doc,
        &format!("{header_prefix}blood_moon"),
    )?));
    out.push(u8::from(bool_at(doc, &format!("{header_prefix}eclipse"))?));
    for key in ["dungeon_x", "dungeon_y"] {
        out.extend_from_slice(&i32_at(doc, &format!("{header_prefix}{key}"))?.to_le_bytes());
    }
    out.push(u8::from(bool_at(doc, &format!("{header_prefix}crimson"))?));
    let flags = [
        "downed_eye_of_cthulhu",
        "downed_eater_of_worlds",
        "downed_skeletron",
        "downed_queen_bee",
        "downed_destroyer",
        "downed_twins",
        "downed_skeletron_prime",
        "downed_any_mech",
        "downed_plantera",
        "downed_golem",
        "downed_slime_king",
        "saved_goblin",
        "saved_wizard",
        "saved_mech",
        "downed_goblins",
        "downed_clown",
        "downed_frost",
        "downed_pirates",
        "shadow_orb_smashed",
        "spawn_meteor",
    ];
    for key in flags {
        out.push(u8::from(bool_at(doc, &format!("{header_prefix}{key}"))?));
    }
    out.push(
        u8::try_from(int_at(doc, &format!("{header_prefix}shadow_orb_count"))?)
            .map_err(|_| write_error("the shadow orb count must fit a byte"))?,
    );
    out.extend_from_slice(&i32_at(doc, &format!("{header_prefix}altar_count"))?.to_le_bytes());
    out.push(u8::from(bool_at(
        doc,
        &format!("{header_prefix}hard_mode"),
    )?));
    if version >= 257 {
        out.push(u8::from(bool_at(
            doc,
            &format!("{header_prefix}party_of_doom"),
        )?));
    }
    for key in ["invasion_delay", "invasion_size", "invasion_type"] {
        out.extend_from_slice(&i32_at(doc, &format!("{header_prefix}{key}"))?.to_le_bytes());
    }
    for key in ["invasion_x", "slime_rain_time"] {
        out.extend_from_slice(&f64_at(doc, &format!("{header_prefix}{key}"))?.to_le_bytes());
    }
    out.push(
        u8::try_from(int_at(doc, &format!("{header_prefix}sundial_cooldown"))?)
            .map_err(|_| write_error("the sundial cooldown must fit a byte"))?,
    );
    out.push(u8::from(bool_at(
        doc,
        &format!("{header_prefix}is_raining"),
    )?));
    out.extend_from_slice(&bytes_at(doc, &format!("{header_prefix}tail"))?);

    let header_end = out.len();
    let header_len = header_end.saturating_sub(header_start);
    let delta = i64::try_from(header_len)
        .unwrap_or(i64::MAX)
        .saturating_sub(i64::try_from(original_header_len).unwrap_or(0));
    let mut new_pointers = Vec::with_capacity(pointers.len());
    for (index, pointer) in pointers.iter().enumerate() {
        let adjusted = if index == 0 {
            i64::from(*pointer)
        } else {
            i64::from(*pointer).saturating_add(delta)
        };
        new_pointers.push(
            i32::try_from(adjusted)
                .map_err(|_| write_error("a section pointer is out of range"))?,
        );
    }
    // The first pointer is the end of section 0, which does not move.
    if let Some(first) = new_pointers.first_mut() {
        *first = i32::try_from(header_start)
            .map_err(|_| write_error("the header is too large for the world format"))?;
    }
    for (index, pointer) in new_pointers.iter().enumerate() {
        let offset = pointers_offset
            .checked_add(index.saturating_mul(4))
            .ok_or_else(|| write_error("section pointer offset"))?;
        let slot = out
            .get_mut(offset..offset.saturating_add(4))
            .ok_or_else(|| write_error("section pointer slot is missing"))?;
        slot.copy_from_slice(&pointer.to_le_bytes());
    }
    out.extend_from_slice(&sections);
    Ok(out)
}

impl Codec for TerrariaWorldCodec {
    fn id(&self) -> &'static str {
        "format.terraria-world"
    }

    fn sniff(&self, input: &[u8], _ctx: &SniffCtx<'_>) -> Confidence {
        let version = match input.get(..4) {
            Some(bytes) => match <[u8; 4]>::try_from(bytes) {
                Ok(array) => u32::from_le_bytes(array),
                Err(_) => return Confidence::NONE,
            },
            None => return Confidence::NONE,
        };
        if !(MIN_VERSION..=MAX_VERSION).contains(&version) {
            return Confidence::NONE;
        }
        if !matches!(input.get(4..11), Some(b"relogic") | Some(b"xindong")) {
            return Confidence::NONE;
        }
        if input.get(11).copied() != Some(FILE_TYPE_WORLD) {
            return Confidence::NONE;
        }
        Confidence::CERTAIN
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<Decoded> {
        ctx.limits().check_input(len_u64(input.len()))?;
        let meta = read_section_header(input)?;
        let section = input
            .get(meta.header_start..meta.sections_start)
            .ok_or_else(|| parse("the world header section is missing"))?;
        let header = read_header(section, meta.version)?;
        let doc = build_doc(input, &meta, &header, ctx)?;
        ctx.charge_nodes(len_u64(doc.len()))?;
        Ok(Decoded {
            doc,
            params: Params::new(),
        })
    }

    fn encode(&self, doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
        write_world(doc, params)
    }
}
