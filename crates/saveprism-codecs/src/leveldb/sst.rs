//! Reads a LevelDB sorted table into a read-only key/value tree.

use saveprism_core::{Limits, Node, NodeFlags, NodeId, Params, Result, SaveDoc, TypeHint, Value};

use super::{ORIGINAL, TABLE_MAGIC, parse_error, read_only_encode};
use crate::kv_common::{
    Reader, Scheme, chrome_entry, internal_key, parse_block_entries, read_block, text_or_hex,
};

const HINT_ENTRIES: &str = "LEVELDB:entries";
const HINT_DELETED: &str = "LEVELDB:deleted";
const HINT_CHROME: &str = "LEVELDB:chrome-local-storage";
const HINT_VALUE: &str = "LEVELDB:value";

/// True when the last eight bytes are the LevelDB table magic.
pub(super) fn has_magic(input: &[u8]) -> bool {
    input
        .get(input.len().saturating_sub(8)..)
        .and_then(|bytes| <[u8; 8]>::try_from(bytes).ok())
        .is_some_and(|bytes| u64::from_le_bytes(bytes) == TABLE_MAGIC)
}

/// Reads the table bytes into a document and keeps them for the exact round trip.
pub(super) fn decode(input: &[u8], limits: &Limits) -> Result<(SaveDoc, Params)> {
    if input.len() < 48 {
        return Err(parse_error("a LevelDB table is at least 48 bytes", 0));
    }
    if !has_magic(input) {
        return Err(parse_error(
            "the LevelDB table magic number is missing",
            input.len(),
        ));
    }
    let footer_start = input.len() - 48;
    let mut footer = Reader::new(input, footer_start)
        .ok_or_else(|| parse_error("the table footer is cut short", footer_start))?;
    let _metaindex = read_handle(&mut footer)?;
    let (index_offset, index_size) = read_handle(&mut footer)?;
    check_block(index_offset, index_size, footer_start)?;
    let index = read_block(
        input,
        index_offset,
        index_size,
        true,
        Scheme::LevelDb,
        limits.max_decoded_bytes,
    )
    .map_err(|error| parse_error(error.message, index_offset))?;
    let index_entries =
        parse_block_entries(&index).map_err(|error| parse_error(error.message, index_offset))?;
    if index_entries.is_empty() {
        return Err(parse_error("the table index is empty", index_offset));
    }

    let mut doc = SaveDoc::with_node_limit(limits.max_nodes);
    let mut entries: Vec<(String, NodeId)> = Vec::new();
    for (_, value) in &index_entries {
        let mut handle = Reader::new(value, 0)
            .ok_or_else(|| parse_error("the index value is empty", index_offset))?;
        let (offset, size) = read_handle(&mut handle)?;
        if size == 0 {
            continue;
        }
        check_block(offset, size, footer_start)?;
        let block = read_block(
            input,
            offset,
            size,
            true,
            Scheme::LevelDb,
            limits.max_decoded_bytes,
        )
        .map_err(|error| parse_error(error.message, offset))?;
        let pairs = parse_block_entries(&block).map_err(|error| parse_error(error.message, 0))?;
        for (key, value) in pairs {
            let (display, node) = entry_node(&key, value, &mut doc)?;
            entries.push((display, node));
        }
    }

    let mut root = Node::new(Value::Object(entries));
    root.type_hint = Some(TypeHint::new(HINT_ENTRIES));
    root.flags.insert(NodeFlags::READ_ONLY);
    let root = doc.push(root)?;
    doc.set_root(root);
    let mut params = Params::new();
    params.set(ORIGINAL, input.to_vec());
    Ok((doc, params))
}

/// Writes the document back; only the unchanged tree is accepted.
pub(super) fn encode(doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
    read_only_encode(doc, params, decode)
}

fn check_block(offset: u64, size: u64, limit: usize) -> Result<()> {
    let end = offset
        .checked_add(size)
        .and_then(|end| end.checked_add(5))
        .ok_or_else(|| parse_error("the block handle overflows", 0))?;
    if end > u64::try_from(limit).unwrap_or(u64::MAX) {
        return Err(parse_error(
            "a block handle points into the table footer",
            usize::try_from(offset).unwrap_or(limit),
        ));
    }
    Ok(())
}

fn read_handle(reader: &mut Reader<'_>) -> Result<(u64, u64)> {
    let offset = reader
        .varint64()
        .map_err(|error| parse_error(error.message, error.offset))?;
    let size = reader
        .varint64()
        .map_err(|error| parse_error(error.message, error.offset))?;
    Ok((offset, size))
}

fn entry_node(key: &[u8], value: Vec<u8>, doc: &mut SaveDoc) -> Result<(String, NodeId)> {
    let (user_key, kind) = internal_key(key);
    let kind = kind.unwrap_or(1);
    if let Some(chrome) = chrome_entry(user_key, &value) {
        let display = format!("{} / {}", chrome.origin, chrome.key);
        let node = scalar(Value::Str(chrome.value), HINT_CHROME, doc)?;
        return Ok((display, node));
    }
    let display = text_or_hex(user_key);
    let node = match kind {
        0 => scalar(Value::Null, HINT_DELETED, doc)?,
        1 => scalar(Value::Str(text_or_hex(&value)), HINT_VALUE, doc)?,
        other => {
            let mut node = Node::new(Value::Str(text_or_hex(&value)));
            node.type_hint = Some(TypeHint::new(format!("LEVELDB:value/type-{other}")));
            node.flags.insert(NodeFlags::READ_ONLY);
            doc.push(node)?
        }
    };
    Ok((display, node))
}

fn scalar(value: Value, hint: &str, doc: &mut SaveDoc) -> Result<NodeId> {
    let mut node = Node::new(value);
    node.type_hint = Some(TypeHint::new(hint));
    node.flags.insert(NodeFlags::READ_ONLY);
    doc.push(node)
}
