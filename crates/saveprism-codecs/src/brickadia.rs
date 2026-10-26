//! `format.brickadia-brs`: Brickadia save files (`.brs`).
//!
//! # Format
//!
//! A `.brs` file starts with the ASCII magic `BRS` and a little-endian `u16` version (1 to 4).
//! Three zlib-wrapped sections follow: header 1 (map name, author name, description, author UUID,
//! an optional UE4 date-time and the brick count), header 2 (mods, brick assets, colors,
//! materials and brick owners), and the bit-packed brick list. Every section starts with an
//! uncompressed size and a compressed size (`0` means stored).
//!
//! The layout was verified against the MIT-licensed `Brickadia/brs` project
//! (<https://github.com/Brickadia/brs>, `src/read.rs`, `src/write.rs`, `src/save.rs`) and the
//! MIT `Meshiest/brs-js` sources. No code was copied; only the documented field order and the
//! version gates are used.
//!
//! # Tree and editing
//!
//! The root holds the header text fields (`map`, `author`, `description`) as editable strings,
//! the author UUID, the save time and the brick count as read-only scalars, and the header-2
//! table sizes as read-only counts. Brick data is not exposed: it stays in the original bytes.
//!
//! # Exact writing
//!
//! An unchanged header 1 is copied byte for byte. When a text field changes, only header 1 is
//! rebuilt (UTF-8 strings, still zlib-compressed when that is smaller); header 2 and the brick
//! section are copied unchanged.

use std::sync::Arc;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    Codec, Confidence, DecodeCtx, Decoded, Error, ErrorCode, IntWidth, Node, NodeFlags, Params,
    Registry, Result, SaveDoc, SniffCtx, TypeHint, Value,
};

/// The save is damaged or cut short.
pub const BRS_DAMAGED: ErrorCode = ErrorCode::new("SP-BRS-001");
/// The edited header cannot be written back.
pub const BRS_WRITE: ErrorCode = ErrorCode::new("SP-BRS-002");

const MAGIC: &[u8; 3] = b"BRS";
const PREFIX: &str = "BRICKADIA:";

/// Registers the Brickadia save codec.
pub fn register(reg: &mut Registry) {
    reg.add_codec(Arc::new(BrsCodec));
}

/// The `format.brickadia-brs` codec.
#[derive(Clone, Copy, Debug, Default)]
pub struct BrsCodec;

#[derive(Clone, Debug)]
struct Header1 {
    map: String,
    author: String,
    description: String,
    author_id: [u8; 16],
    save_time: Option<[u8; 8]>,
    brick_count: i32,
}

#[derive(Clone, Debug)]
struct Save {
    version: u16,
    header: Header1,
    section2_start: usize,
    header2: Counts,
}

#[derive(Clone, Debug, Default)]
struct Counts {
    mods: usize,
    assets: usize,
    colors: usize,
    materials: usize,
    owners: usize,
}

fn damaged(message: impl Into<String>, offset: usize) -> Error {
    Error::new(BRS_DAMAGED, message).with_offset(len_u64(offset))
}

fn write_error(message: impl Into<String>) -> Error {
    Error::new(BRS_WRITE, message)
}

struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }

    fn take(&mut self, len: usize) -> Option<&'a [u8]> {
        let end = self.at.checked_add(len)?;
        let out = self.bytes.get(self.at..end)?;
        self.at = end;
        Some(out)
    }

    fn i32(&mut self) -> Option<i32> {
        Some(i32::from_le_bytes(<[u8; 4]>::try_from(self.take(4)?).ok()?))
    }

    fn u16(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes(<[u8; 2]>::try_from(self.take(2)?).ok()?))
    }

    fn string(&mut self) -> Option<String> {
        let length = self.i32()?;
        if length >= 0 {
            let raw = self.take(usize::try_from(length).ok()?)?;
            return String::from_utf8(raw.to_vec()).ok();
        }
        let units = usize::try_from(-length).ok()?;
        let raw = self.take(units.checked_mul(2)?)?;
        let mut text = Vec::with_capacity(units);
        for pair in raw.as_chunks::<2>().0 {
            text.push(u16::from_le_bytes(*pair));
        }
        String::from_utf16(&text).ok()
    }
}

fn section(input: &[u8], at: usize, ctx: &DecodeCtx<'_>) -> Result<(Vec<u8>, usize, usize)> {
    let mut cursor = Cursor::new(input);
    if cursor.take(at).is_none() {
        return Err(damaged("the BRS section offset is beyond the file", at));
    }
    let size = cursor
        .i32()
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| damaged("a BRS section has no uncompressed size", at))?;
    let stored = cursor
        .i32()
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| damaged("a BRS section has no compressed size", at + 4))?;
    let data_start = cursor.at;
    let data_end = data_start
        .checked_add(if stored == 0 { size } else { stored })
        .ok_or_else(|| damaged("a BRS section range overflows", data_start))?;
    let data_end = data_end.min(input.len());
    let raw = input
        .get(data_start..data_end)
        .ok_or_else(|| damaged("a BRS section lies beyond the file", data_start))?;
    let inner = if stored == 0 {
        raw.to_vec()
    } else {
        let decoder = flate2::read::ZlibDecoder::new(raw);
        ctx.read_bounded(decoder, len_u64(input.len()), BRS_DAMAGED)?
    };
    if inner.len() != size {
        return Err(damaged(
            format!(
                "a BRS section has {} bytes but its header says {size}",
                inner.len()
            ),
            at,
        ));
    }
    Ok((
        inner,
        data_start,
        data_start + if stored == 0 { size } else { stored },
    ))
}

fn parse_header1(bytes: &[u8], version: u16) -> Option<Header1> {
    let mut cursor = Cursor::new(bytes);
    let map = cursor.string()?;
    let author = cursor.string()?;
    let description = cursor.string()?;
    let author_id: [u8; 16] = cursor.take(16)?.try_into().ok()?;
    let save_time = if version >= 4 {
        Some(cursor.take(8)?.try_into().ok()?)
    } else {
        None
    };
    let brick_count = cursor.i32()?;
    Some(Header1 {
        map,
        author,
        description,
        author_id,
        save_time,
        brick_count,
    })
}

fn array_count(
    cursor: &mut Cursor<'_>,
    mut item: impl FnMut(&mut Cursor<'_>) -> Option<()>,
) -> Option<usize> {
    let count = cursor.i32()?;
    if count < 0 {
        return None;
    }
    let count = usize::try_from(count).ok()?;
    if count > 10_000_000 {
        return None;
    }
    for _ in 0..count {
        item(cursor)?;
    }
    Some(count)
}

fn parse_header2(bytes: &[u8], version: u16) -> Option<Counts> {
    let mut cursor = Cursor::new(bytes);
    let mut string = |cursor: &mut Cursor<'_>| cursor.string().map(|_| ());
    let mods = array_count(&mut cursor, &mut string)?;
    let assets = array_count(&mut cursor, &mut string)?;
    let colors = array_count(&mut cursor, |cursor| cursor.take(4).map(|_| ()))?;
    let materials = if version >= 2 {
        array_count(&mut cursor, &mut string)?
    } else {
        4
    };
    let owners = if version >= 3 {
        array_count(&mut cursor, |cursor| {
            cursor.take(16)?;
            cursor.string().map(|_| ())
        })?
    } else {
        0
    };
    Some(Counts {
        mods,
        assets,
        colors,
        materials,
        owners,
    })
}

fn parse(input: &[u8], ctx: &DecodeCtx<'_>) -> Result<Save> {
    if input.len() < 5 {
        return Err(damaged("the BRS is shorter than its header", 0));
    }
    if input.get(..3) != Some(MAGIC) {
        return Err(damaged("the file has no BRS magic", 0));
    }
    let mut cursor = Cursor::new(input);
    cursor.take(3);
    let version = cursor
        .u16()
        .ok_or_else(|| damaged("the BRS version is missing", 3))?;
    if !(1..=4).contains(&version) {
        return Err(damaged(
            format!("the BRS version {version} is not supported"),
            3,
        ));
    }
    let (header1_bytes, _section1_start, section1_end) = section(input, 5, ctx)?;
    let header = parse_header1(&header1_bytes, version)
        .ok_or_else(|| damaged("the BRS header 1 is malformed", 5))?;
    let (header2_bytes, _section2_start, _section2_end) = section(input, section1_end, ctx)?;
    let header2 = parse_header2(&header2_bytes, version)
        .ok_or_else(|| damaged("the BRS header 2 is malformed", section1_end))?;
    Ok(Save {
        version,
        header,
        section2_start: section1_end,
        header2,
    })
}

fn text_node(doc: &mut SaveDoc, text: &str, read_only: bool) -> Result<saveprism_core::NodeId> {
    let mut node = Node::new(Value::Str(text.to_string()));
    node.type_hint = Some(TypeHint::new(format!("{PREFIX}header:text")));
    if read_only {
        node.flags.insert(NodeFlags::READ_ONLY);
    }
    doc.push(node)
}

fn count_node(doc: &mut SaveDoc, value: i32, hint: &str) -> Result<saveprism_core::NodeId> {
    let mut node = Node::new(Value::Int {
        v: i128::from(value),
        width: IntWidth::I32,
    });
    node.type_hint = Some(TypeHint::new(format!("{PREFIX}{hint}")));
    node.flags.insert(NodeFlags::READ_ONLY);
    doc.push(node)
}

fn doc_text(doc: &SaveDoc, key: &str) -> Option<String> {
    let root = doc.get(doc.root())?;
    let Value::Object(entries) = &root.value else {
        return None;
    };
    let id = entries
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, id)| *id)?;
    match &doc.get(id)?.value {
        Value::Str(text) => Some(text.clone()),
        _ => None,
    }
}

impl Codec for BrsCodec {
    fn id(&self) -> &'static str {
        "format.brickadia-brs"
    }

    fn sniff(&self, input: &[u8], ctx: &SniffCtx<'_>) -> Confidence {
        if input.get(..3) != Some(MAGIC) {
            return Confidence::NONE;
        }
        let limits = saveprism_core::Limits::default();
        let decode_ctx = DecodeCtx::new(&limits);
        if parse(input, &decode_ctx).is_err() {
            return Confidence::NONE;
        }
        let named = ctx
            .file_name
            .is_some_and(|name| name.to_ascii_lowercase().ends_with(".brs"));
        Confidence::new(if named { 1.0 } else { 0.95 })
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<Decoded> {
        ctx.limits().check_input(len_u64(input.len()))?;
        let save = parse(input, ctx)?;
        let mut doc = ctx.new_doc();
        let map = text_node(&mut doc, &save.header.map, false)?;
        let author = text_node(&mut doc, &save.header.author, false)?;
        let description = text_node(&mut doc, &save.header.description, false)?;
        let author_id = text_node(
            &mut doc,
            &save
                .header
                .author_id
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
            true,
        )?;
        let save_time = save.header.save_time.map(i64::from_le_bytes).unwrap_or(0);
        let save_time = count_node(
            &mut doc,
            i32::try_from(save_time).unwrap_or(i32::MAX),
            "header:time",
        )?;
        let brick_count = count_node(&mut doc, save.header.brick_count, "header:brick_count")?;
        let mods = count_node(
            &mut doc,
            i32::try_from(save.header2.mods).unwrap_or(i32::MAX),
            "header2:mods",
        )?;
        let assets = count_node(
            &mut doc,
            i32::try_from(save.header2.assets).unwrap_or(i32::MAX),
            "header2:assets",
        )?;
        let colors = count_node(
            &mut doc,
            i32::try_from(save.header2.colors).unwrap_or(i32::MAX),
            "header2:colors",
        )?;
        let materials = count_node(
            &mut doc,
            i32::try_from(save.header2.materials).unwrap_or(i32::MAX),
            "header2:materials",
        )?;
        let owners = count_node(
            &mut doc,
            i32::try_from(save.header2.owners).unwrap_or(i32::MAX),
            "header2:owners",
        )?;
        let mut root = Node::new(Value::Object(vec![
            ("map".to_string(), map),
            ("author".to_string(), author),
            ("description".to_string(), description),
            ("authorId".to_string(), author_id),
            ("saveTime".to_string(), save_time),
            ("brickCount".to_string(), brick_count),
            ("mods".to_string(), mods),
            ("brickAssets".to_string(), assets),
            ("colors".to_string(), colors),
            ("materials".to_string(), materials),
            ("brickOwners".to_string(), owners),
        ]));
        root.type_hint = Some(TypeHint::new(format!("{PREFIX}save")));
        let root = doc.push(root)?;
        doc.set_root(root);

        let mut params = Params::new();
        params.set("source", input.to_vec());
        params.set("map", save.header.map);
        params.set("author", save.header.author);
        params.set("description", save.header.description);
        params.set("author_id", save.header.author_id.to_vec());
        params.set(
            "save_time",
            save.header
                .save_time
                .map(|raw| raw.to_vec())
                .unwrap_or_default(),
        );
        params.set("brick_count", i64::from(save.header.brick_count));
        params.set("version", i64::from(save.version));
        params.set(
            "section2_start",
            i64::try_from(save.section2_start).unwrap_or(i64::MAX),
        );
        Ok(Decoded { doc, params })
    }

    fn encode(&self, doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
        let source = params.bytes("source")?;
        let map = doc_text(doc, "map").ok_or_else(|| write_error("the map name is missing"))?;
        let author = doc_text(doc, "author").ok_or_else(|| write_error("the author is missing"))?;
        let description = doc_text(doc, "description")
            .ok_or_else(|| write_error("the description is missing"))?;
        let unchanged = map == params.str("map")?
            && author == params.str("author")?
            && description == params.str("description")?;
        if unchanged {
            return Ok(source.to_vec());
        }
        let version = u16::try_from(params.int("version")?)
            .map_err(|_| write_error("the BRS version is invalid"))?;
        let author_id = params.bytes("author_id")?;
        let save_time = params.bytes("save_time")?;
        let brick_count = i32::try_from(params.int("brick_count")?)
            .map_err(|_| write_error("the brick count is invalid"))?;
        let mut header = Vec::new();
        write_string(&mut header, &map)?;
        write_string(&mut header, &author)?;
        write_string(&mut header, &description)?;
        header.extend_from_slice(author_id);
        if version >= 4 {
            header.extend_from_slice(save_time);
        }
        header.extend_from_slice(&brick_count.to_le_bytes());

        let section = encode_section(&header)?;
        let section2_start = usize::try_from(params.int("section2_start")?)
            .map_err(|_| write_error("the header 2 offset is invalid"))?;
        let mut out = source.get(..5).unwrap_or_default().to_vec();
        out.extend_from_slice(&section);
        out.extend_from_slice(source.get(section2_start..).unwrap_or_default());
        Ok(out)
    }
}

fn write_string(out: &mut Vec<u8>, text: &str) -> Result<()> {
    let length =
        i32::try_from(text.len()).map_err(|_| write_error("a header string is too long"))?;
    out.extend_from_slice(&length.to_le_bytes());
    out.extend_from_slice(text.as_bytes());
    Ok(())
}

fn encode_section(inner: &[u8]) -> Result<Vec<u8>> {
    use std::io::Write as _;
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
    encoder
        .write_all(inner)
        .map_err(|source| write_error("zlib compression failed").with_source(source))?;
    let compressed = encoder
        .finish()
        .map_err(|source| write_error("zlib compression failed").with_source(source))?;
    let mut out = Vec::new();
    let size = i32::try_from(inner.len()).map_err(|_| write_error("the header is too large"))?;
    out.extend_from_slice(&size.to_le_bytes());
    if compressed.len() < inner.len() {
        let stored = i32::try_from(compressed.len())
            .map_err(|_| write_error("the compressed header is too large"))?;
        out.extend_from_slice(&stored.to_le_bytes());
        out.extend_from_slice(&compressed);
    } else {
        out.extend_from_slice(&0_i32.to_le_bytes());
        out.extend_from_slice(inner);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use saveprism_core::detect::{DetectOptions, detect};
    use saveprism_core::guard::{Verdict, check_round_trip};
    use saveprism_core::{Limits, pipeline};

    use super::*;
    use crate::register_all;

    fn fixture() -> Vec<u8> {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/synthetic/archives/brs/save.brs");
        std::fs::read(path).unwrap()
    }

    fn decode(input: &[u8]) -> Decoded {
        let limits = Limits::default();
        let mut ctx = DecodeCtx::new(&limits);
        BrsCodec.decode(input, &mut ctx).unwrap()
    }

    #[test]
    fn fixture_round_trips_exactly() {
        let bytes = fixture();
        let decoded = decode(&bytes);
        assert_eq!(doc_text(&decoded.doc, "map").unwrap(), "Synthetic Park");
        assert_eq!(
            BrsCodec.encode(&decoded.doc, &decoded.params).unwrap(),
            bytes
        );
    }

    #[test]
    fn editing_a_header_text_rebuilds_only_header1() {
        let bytes = fixture();
        let mut decoded = decode(&bytes);
        let root = decoded.doc.root();
        let map_id = decoded
            .doc
            .children(root)
            .first()
            .copied()
            .expect("map node");
        decoded
            .doc
            .set_value(map_id, Value::Str("Another Park".to_string()))
            .unwrap();
        let written = BrsCodec.encode(&decoded.doc, &decoded.params).unwrap();
        assert_ne!(written, bytes);
        let again = decode(&written);
        assert_eq!(doc_text(&again.doc, "map").unwrap(), "Another Park");
        assert_eq!(doc_text(&again.doc, "author").unwrap(), "Test Builder");
    }

    #[test]
    fn detection_and_round_trip_agree() {
        let bytes = fixture();
        let mut reg = Registry::default();
        register_all(&mut reg);
        let detection =
            detect(&bytes, &reg, &Limits::default(), &DetectOptions::default()).unwrap();
        assert_eq!(detection.best.recipe.kinds(), ["format.brickadia-brs"]);
        let report =
            check_round_trip(&bytes, &detection.best.recipe, &reg, &Limits::default()).unwrap();
        assert_eq!(report.overall, Verdict::Exact);
        let decoded =
            pipeline::decode(&bytes, &detection.best.recipe, &reg, &Limits::default()).unwrap();
        let encoded = pipeline::encode(&decoded.doc, &decoded.recipe, &reg).unwrap();
        assert_eq!(encoded, bytes);
    }

    #[test]
    fn random_bytes_are_not_a_save() {
        assert_eq!(
            BrsCodec.sniff(b"BRS\x04\x00", &SniffCtx::default()),
            Confidence::NONE
        );
    }
}
