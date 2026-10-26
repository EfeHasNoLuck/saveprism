//! Snes9x snapshots (`.000`-`.009`, `.frz`), see `mod.rs`.

use saveprism_core::limits::len_u64;
use saveprism_core::{
    ByteSpan, Codec, Confidence, DecodeCtx, Decoded, Error, ErrorCode, IntWidth, Node, NodeFlags,
    NodeId, Params, Result, SaveDoc, SniffCtx, TypeHint, Value,
};

/// The snapshot text is cut short or a block length is not six decimal digits.
pub const S9X_PARSE: ErrorCode = ErrorCode::new("SP-S9X-001");
/// A snapshot cannot be written (an SRA block changed its length).
pub const S9X_WRITE: ErrorCode = ErrorCode::new("SP-S9X-002");

const SIGNATURE: &str = "#!s9xsnp:";
const ORIGINAL: &str = "original";

/// The `format.snes9x-snapshot` codec.
#[derive(Clone, Copy, Debug, Default)]
pub struct Snes9xSnapshotCodec;

fn parse_error(message: impl Into<String>, at: usize) -> Error {
    Error::new(S9X_PARSE, message).with_offset(len_u64(at))
}

fn hint(name: &str) -> TypeHint {
    TypeHint::new(format!("S9X:{name}"))
}

fn push(
    doc: &mut SaveDoc,
    value: Value,
    kind: &str,
    span: Option<(usize, usize)>,
    read_only: bool,
) -> Result<NodeId> {
    let mut node = Node::new(value);
    node.type_hint = Some(hint(kind));
    if let Some((start, end)) = span {
        node.span = Some(ByteSpan {
            start: len_u64(start),
            end: len_u64(end),
        });
    }
    if read_only {
        node.flags.insert(NodeFlags::READ_ONLY);
    }
    doc.push(node)
}

fn parse(input: &[u8]) -> Result<SaveDoc> {
    if !input.starts_with(SIGNATURE.as_bytes()) {
        return Err(parse_error("missing the #!s9xsnp signature", 0));
    }
    let newline = input
        .iter()
        .position(|&byte| byte == b'\n')
        .ok_or_else(|| parse_error("the snapshot header has no newline", 0))?;
    let header = input.get(..newline).unwrap_or_default();
    let digits = header.get(SIGNATURE.len()..).unwrap_or_default();
    if digits.len() != 4 || !digits.iter().all(u8::is_ascii_digit) {
        return Err(parse_error("the snapshot version is not four digits", 0));
    }
    let version = std::str::from_utf8(digits)
        .ok()
        .and_then(|text| text.parse::<u32>().ok())
        .ok_or_else(|| parse_error("the snapshot version is not a number", 0))?;
    let mut doc = SaveDoc::new();
    let version_node = push(
        &mut doc,
        Value::Int {
            v: i128::from(version),
            width: IntWidth::U32,
        },
        "version",
        None,
        true,
    )?;
    let mut entries = Vec::new();
    let mut pos = newline.saturating_add(1);
    while pos < input.len() {
        let header_end = pos
            .checked_add(11)
            .filter(|end| *end <= input.len())
            .ok_or_else(|| parse_error("the block header is cut short", pos))?;
        let header = input
            .get(pos..header_end)
            .ok_or_else(|| parse_error("the block header is cut short", pos))?;
        let name = std::str::from_utf8(header.get(..3).unwrap_or_default())
            .map_err(|_| parse_error("a block name is not ASCII", pos))?
            .to_owned();
        if header.get(3) != Some(&b':') || header.get(10) != Some(&b':') {
            return Err(parse_error("the block header has the wrong shape", pos));
        }
        let digits = header.get(4..10).unwrap_or_default();
        if !digits.iter().all(u8::is_ascii_digit) {
            return Err(parse_error("the block length is not six digits", pos));
        }
        let length = std::str::from_utf8(digits)
            .ok()
            .and_then(|text| text.parse::<usize>().ok())
            .ok_or_else(|| parse_error("the block length is not a number", pos))?;
        let start = header_end;
        let end = start
            .checked_add(length)
            .filter(|end| *end <= input.len())
            .ok_or_else(|| parse_error("the block data is cut short", start))?;
        let name_node = push(&mut doc, Value::Str(name.clone()), "block-name", None, true)?;
        let size_node = push(
            &mut doc,
            Value::Int {
                v: i128::try_from(length).unwrap_or(i128::MAX),
                width: IntWidth::U32,
            },
            "block-size",
            None,
            true,
        )?;
        let data = input.get(start..end).unwrap_or_default();
        let editable = name == "SRA";
        let data_node = push(
            &mut doc,
            Value::Bytes(data.to_vec()),
            if editable { "sram" } else { "block-data" },
            Some((start, end)),
            !editable,
        )?;
        let mut entry = Node::new(Value::Object(vec![
            ("name".to_owned(), name_node),
            ("size".to_owned(), size_node),
            ("data".to_owned(), data_node),
        ]));
        entry.type_hint = Some(hint("block"));
        entry.flags.insert(NodeFlags::READ_ONLY);
        entries.push(doc.push(entry)?);
        pos = end;
    }
    let block_list = push(&mut doc, Value::Array(entries), "blocks", None, true)?;
    let mut root = Node::new(Value::Object(vec![
        ("version".to_owned(), version_node),
        ("blocks".to_owned(), block_list),
    ]));
    root.type_hint = Some(hint("snapshot"));
    root.flags.insert(NodeFlags::READ_ONLY);
    let root = doc.push(root)?;
    doc.set_root(root);
    Ok(doc)
}

impl Codec for Snes9xSnapshotCodec {
    fn id(&self) -> &'static str {
        "format.snes9x-snapshot"
    }

    fn sniff(&self, input: &[u8], _ctx: &SniffCtx<'_>) -> Confidence {
        if input.starts_with(SIGNATURE.as_bytes()) {
            Confidence::new(0.95)
        } else {
            Confidence::NONE
        }
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<Decoded> {
        ctx.limits().check_input(len_u64(input.len()))?;
        let doc = parse(input)?;
        ctx.charge_nodes(len_u64(doc.len().saturating_sub(1)))?;
        let mut params = Params::new();
        params.set(ORIGINAL, input.to_vec());
        Ok(Decoded { doc, params })
    }

    fn encode(&self, doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
        let original = params.bytes(ORIGINAL)?;
        let fresh = parse(original)?;
        let current_blocks = blocks_root(doc)?;
        let fresh_blocks = blocks_root(&fresh)?;
        if current_blocks.len() != fresh_blocks.len() {
            return Err(Error::new(
                S9X_WRITE,
                "the number of snapshot blocks changed",
            ));
        }
        let mut patches = Vec::new();
        for (current, original_entry) in current_blocks.iter().zip(fresh_blocks.iter()) {
            let current_name = entry_text(doc, *current, "name")?;
            let fresh_name = entry_text(&fresh, *original_entry, "name")?;
            if current_name != fresh_name || current_name != "SRA" {
                continue;
            }
            let current_data = entry_bytes(doc, *current, "data")?;
            let fresh_data = entry_bytes(&fresh, *original_entry, "data")?;
            if current_data == fresh_data {
                continue;
            }
            if current_data.len() != fresh_data.len() {
                return Err(Error::new(
                    S9X_WRITE,
                    "an SRA block changed its length (it must stay the same size)",
                ));
            }
            let id = entry_node(&fresh, *original_entry, "data")?;
            let span = fresh
                .get(id)
                .and_then(|node| node.span)
                .ok_or_else(|| Error::new(S9X_WRITE, "the SRA block has no byte span"))?;
            patches.push((
                usize::try_from(span.start).unwrap_or(0),
                usize::try_from(span.end).unwrap_or(0),
                current_data.to_vec(),
            ));
        }
        patches.sort_by_key(|(start, _, _)| *start);
        let mut out = original.to_vec();
        for (start, end, bytes) in patches.into_iter().rev() {
            if start > end || end > out.len() {
                return Err(Error::new(S9X_WRITE, "an SRA block span is out of range"));
            }
            out.splice(start..end, bytes);
        }
        Ok(out)
    }
}

fn blocks_root(doc: &SaveDoc) -> Result<Vec<NodeId>> {
    let root = doc
        .get(doc.root())
        .ok_or_else(|| Error::new(S9X_WRITE, "the snapshot root is missing"))?;
    let entries = match &root.value {
        Value::Object(entries) => entries,
        _ => return Err(Error::new(S9X_WRITE, "the snapshot root is not an object")),
    };
    let blocks = entries
        .iter()
        .find(|(key, _)| key == "blocks")
        .map(|(_, id)| *id)
        .ok_or_else(|| Error::new(S9X_WRITE, "the blocks list is missing"))?;
    match doc.get(blocks).map(|node| &node.value) {
        Some(Value::Array(items)) => Ok(items.clone()),
        _ => Err(Error::new(S9X_WRITE, "the blocks list is not a list")),
    }
}

fn entry_node(doc: &SaveDoc, id: NodeId, key: &str) -> Result<NodeId> {
    let node = doc
        .get(id)
        .ok_or_else(|| Error::new(S9X_WRITE, "a block entry is missing"))?;
    match &node.value {
        Value::Object(entries) => entries
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, id)| *id)
            .ok_or_else(|| Error::new(S9X_WRITE, "a block entry field is missing")),
        _ => Err(Error::new(S9X_WRITE, "a block entry is not an object")),
    }
}

fn entry_text(doc: &SaveDoc, id: NodeId, key: &str) -> Result<String> {
    let node = doc
        .get(entry_node(doc, id, key)?)
        .ok_or_else(|| Error::new(S9X_WRITE, "a block entry field is missing"))?;
    match &node.value {
        Value::Str(text) => Ok(text.clone()),
        _ => Err(Error::new(S9X_WRITE, "a block entry text is missing")),
    }
}

fn entry_bytes<'a>(doc: &'a SaveDoc, id: NodeId, key: &str) -> Result<&'a [u8]> {
    let node = doc
        .get(entry_node(doc, id, key)?)
        .ok_or_else(|| Error::new(S9X_WRITE, "a block entry field is missing"))?;
    match &node.value {
        Value::Bytes(bytes) => Ok(bytes),
        _ => Err(Error::new(S9X_WRITE, "a block entry data is missing")),
    }
}
