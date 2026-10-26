//! The RocksDB block-based table reader.

use saveprism_core::limits::len_u64;
use saveprism_core::{
    Limits, Node, NodeFlags, NodeId, ParamValue, Params, Result, SaveDoc, TypeHint, Value,
};

use super::{ORIGINAL, TABLE_MAGIC, parse_error, unsupported, write_error};
use crate::kv_common::{
    Reader, Scheme, internal_key, parse_block_entries, read_block, text_or_hex,
};

/// Smallest possible footer: format version 2 without a properties block.
pub(super) const MIN_FILE: usize = 53;
const HINT_ENTRIES: &str = "ROCKSDB:entries";
const HINT_DELETED: &str = "ROCKSDB:deleted";
const HINT_VALUE: &str = "ROCKSDB:value";
const INDEX_TYPE_PROPERTY: &[u8] = b"rocksdb.block.based.table.index.type";
const INDEX_DELTA_PROPERTY: &[u8] = b"rocksdb.index.value.is.delta.encoded";
const SEPARATE_KV_PROPERTY: &[u8] = b"rocksdb.separate.key.value.in.data.block";

/// True when the last eight bytes are the RocksDB block-based table magic.
pub(super) fn has_magic(input: &[u8]) -> bool {
    input
        .get(input.len().saturating_sub(8)..)
        .and_then(|bytes| <[u8; 8]>::try_from(bytes).ok())
        .is_some_and(|bytes| u64::from_le_bytes(bytes) == TABLE_MAGIC)
}

/// Reads the table into a read-only document.
pub(super) fn decode(input: &[u8], limits: &Limits) -> Result<(SaveDoc, Params)> {
    if input.len() < MIN_FILE {
        return Err(parse_error("a RocksDB table is at least 53 bytes", 0));
    }
    if !has_magic(input) {
        return Err(parse_error(
            "the RocksDB table magic number is missing",
            input.len(),
        ));
    }
    let footer_start = input.len() - 53;
    let version = fixed_u32(input, input.len() - 12)
        .ok_or_else(|| parse_error("the footer format version is cut short", footer_start))?;
    if !(2..=7).contains(&version) {
        return Err(unsupported(format!(
            "RocksDB format version {version} is not supported (only 2 to 7)"
        )));
    }
    let metaindex = parse_footer(input, footer_start, version)?;
    let (meta_offset, meta_size) = metaindex;
    check_block(meta_offset, meta_size, footer_start)?;
    let meta_block = read_block(
        input,
        meta_offset,
        meta_size,
        false,
        Scheme::RocksDb,
        limits.max_decoded_bytes,
    )
    .map_err(|error| parse_error(error.message, meta_offset))?;
    let meta_entries = parse_block_entries(&meta_block)
        .map_err(|error| parse_error(error.message, meta_offset))?;

    let mut index = None;
    let mut properties = None;
    for (key, value) in &meta_entries {
        if key.as_slice() == b"index" {
            index = Some(handle(value)?);
        } else if key.as_slice() == b"rocksdb.properties" {
            properties = Some(handle(value)?);
        }
    }
    let (index_offset, index_size) = if version >= 6 {
        index.ok_or_else(|| parse_error("the metaindex has no index block", meta_offset))?
    } else {
        let mut footer = Reader::new(input, footer_start + 1)
            .ok_or_else(|| parse_error("the footer is cut short", footer_start))?;
        footer
            .varint64()
            .map_err(|error| parse_error(error.message, error.offset))?;
        footer
            .varint64()
            .map_err(|error| parse_error(error.message, error.offset))?;
        let offset = footer
            .varint64()
            .map_err(|error| parse_error(error.message, error.offset))?;
        let size = footer
            .varint64()
            .map_err(|error| parse_error(error.message, error.offset))?;
        (offset, size)
    };
    check_block(index_offset, index_size, footer_start)?;

    if let Some((offset, size)) = properties {
        check_block(offset, size, footer_start)?;
        let block = read_block(
            input,
            offset,
            size,
            false,
            Scheme::RocksDb,
            limits.max_decoded_bytes,
        )
        .map_err(|error| parse_error(error.message, offset))?;
        let entries =
            parse_block_entries(&block).map_err(|error| parse_error(error.message, offset))?;
        for (key, value) in &entries {
            if key.as_slice() == INDEX_TYPE_PROPERTY {
                let kind = fixed_u32(value, 0).unwrap_or(0);
                if kind == 2 {
                    return Err(unsupported(
                        "RocksDB two-level (partitioned) indexes are not supported",
                    ));
                }
            } else if key.as_slice() == INDEX_DELTA_PROPERTY && property_is_set(value) {
                return Err(unsupported(
                    "RocksDB delta-encoded index values are not supported",
                ));
            } else if key.as_slice() == SEPARATE_KV_PROPERTY && property_is_set(value) {
                return Err(unsupported(
                    "RocksDB data blocks with separated keys and values are not supported",
                ));
            }
        }
    }

    let index_block = read_block(
        input,
        index_offset,
        index_size,
        false,
        Scheme::RocksDb,
        limits.max_decoded_bytes,
    )
    .map_err(|error| parse_error(error.message, index_offset))?;
    let index_entries = parse_block_entries(&index_block)
        .map_err(|error| parse_error(error.message, index_offset))?;
    if index_entries.is_empty() {
        return Err(parse_error("the table index is empty", index_offset));
    }

    let mut doc = SaveDoc::with_node_limit(limits.max_nodes);
    let mut entries: Vec<(String, NodeId)> = Vec::new();
    for (_, value) in &index_entries {
        let (offset, size) = handle(value)?;
        if size == 0 {
            continue;
        }
        check_block(offset, size, footer_start)?;
        let block = read_block(
            input,
            offset,
            size,
            false,
            Scheme::RocksDb,
            limits.max_decoded_bytes,
        )
        .map_err(|error| parse_error(error.message, offset))?;
        let pairs = parse_block_entries(&block).map_err(|error| parse_error(error.message, 0))?;
        for (key, value) in pairs {
            let (user_key, kind) = internal_key(&key);
            let kind = kind.unwrap_or(1);
            let display = text_or_hex(user_key);
            let node = match kind {
                0 => scalar(Value::Null, HINT_DELETED, &mut doc)?,
                1 => scalar(Value::Str(text_or_hex(&value)), HINT_VALUE, &mut doc)?,
                other => {
                    let mut node = Node::new(Value::Str(text_or_hex(&value)));
                    node.type_hint = Some(TypeHint::new(format!("ROCKSDB:value/type-{other}")));
                    node.flags.insert(NodeFlags::READ_ONLY);
                    doc.push(node)?
                }
            };
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
    let original = match params.get(ORIGINAL) {
        Some(ParamValue::Bytes(bytes)) => bytes.as_slice(),
        _ => return Err(write_error("original RocksDB bytes are missing")),
    };
    let limits = Limits::default();
    limits.check_input(len_u64(original.len()))?;
    let (baseline, _) = decode(original, &limits)?;
    if doc.tree_eq(&baseline) {
        Ok(original.to_vec())
    } else {
        Err(write_error(
            "a RocksDB table cannot be edited in place exactly, so it is read-only",
        ))
    }
}

fn parse_footer(input: &[u8], footer_start: usize, version: u32) -> Result<(u64, u64)> {
    let _checksum = input
        .get(footer_start)
        .copied()
        .ok_or_else(|| parse_error("the footer checksum type is cut short", footer_start))?;
    if version >= 6 {
        let extended = input.get(footer_start + 1..footer_start + 5);
        if extended != Some(&[0x3e, 0x00, 0x7a, 0x00][..]) {
            return Err(parse_error(
                "the footer extended magic number is wrong",
                footer_start + 1,
            ));
        }
        let field = fixed_u32(input, footer_start + 13)
            .ok_or_else(|| parse_error("the metaindex size is cut short", footer_start + 13))?;
        let size = u64::from(field);
        let end = u64::try_from(footer_start)
            .unwrap_or(u64::MAX)
            .saturating_sub(5);
        let offset = end.saturating_sub(size);
        Ok((offset, size))
    } else {
        let mut reader = Reader::new(input, footer_start + 1)
            .ok_or_else(|| parse_error("the footer is cut short", footer_start))?;
        let offset = reader
            .varint64()
            .map_err(|error| parse_error(error.message, error.offset))?;
        let size = reader
            .varint64()
            .map_err(|error| parse_error(error.message, error.offset))?;
        Ok((offset, size))
    }
}

fn handle(value: &[u8]) -> Result<(u64, u64)> {
    let mut reader =
        Reader::new(value, 0).ok_or_else(|| parse_error("the block handle is empty", 0))?;
    let offset = reader
        .varint64()
        .map_err(|error| parse_error(error.message, error.offset))?;
    let size = reader
        .varint64()
        .map_err(|error| parse_error(error.message, error.offset))?;
    Ok((offset, size))
}

fn fixed_u32(input: &[u8], at: usize) -> Option<u32> {
    let bytes: [u8; 4] = input.get(at..at.checked_add(4)?)?.try_into().ok()?;
    Some(u32::from_le_bytes(bytes))
}

fn property_is_set(value: &[u8]) -> bool {
    if value.len() == 8 {
        return value
            .try_into()
            .ok()
            .is_some_and(|bytes| u64::from_le_bytes(bytes) != 0);
    }
    !matches!(value, b"0" | b"false" | b"" | b" ")
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

fn scalar(value: Value, hint: &str, doc: &mut SaveDoc) -> Result<NodeId> {
    let mut node = Node::new(value);
    node.type_hint = Some(TypeHint::new(hint));
    node.flags.insert(NodeFlags::READ_ONLY);
    doc.push(node)
}
