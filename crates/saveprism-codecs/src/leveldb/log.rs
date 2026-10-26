//! Reads LevelDB write-ahead log records (`doc/log_format.md`) as read-only key/value pairs.

use saveprism_core::{Limits, Node, NodeFlags, NodeId, Params, Result, SaveDoc, TypeHint, Value};

use super::{ORIGINAL, parse_error, read_only_encode};
use crate::kv_common::{Reader, chrome_entry, crc32c, mask_crc, text_or_hex};

const BLOCK_SIZE: usize = 32 * 1024;
const HEADER_SIZE: usize = 7;
const FULL: u8 = 1;
const FIRST: u8 = 2;
const MIDDLE: u8 = 3;
const LAST: u8 = 4;
const HINT_ENTRIES: &str = "LEVELDB:entries";
const HINT_DELETED: &str = "LEVELDB:deleted";
const HINT_CHROME: &str = "LEVELDB:chrome-local-storage";
const HINT_VALUE: &str = "LEVELDB:value";

/// True when the file is a sequence of valid, checksummed log records holding `WriteBatch`es.
pub(super) fn plausible(input: &[u8]) -> bool {
    let Ok(records) = records(input) else {
        return false;
    };
    !records.is_empty() && records.iter().all(|record| batch(record).is_ok())
}

/// Reads the log into a document and keeps the bytes for the exact round trip.
pub(super) fn decode(input: &[u8], limits: &Limits) -> Result<(SaveDoc, Params)> {
    if input.is_empty() {
        return Err(parse_error("the LevelDB log is empty", 0));
    }
    let records = records(input).map_err(|error| parse_error(error.message, error.offset))?;
    if records.is_empty() {
        return Err(parse_error("the LevelDB log has no complete record", 0));
    }
    let mut doc = SaveDoc::with_node_limit(limits.max_nodes);
    let mut entries: Vec<(String, NodeId)> = Vec::new();
    for record in &records {
        for entry in batch(record).map_err(|error| parse_error(error.message, error.offset))? {
            let (display, node) = match entry.value {
                Some(value) => {
                    if let Some(chrome) = chrome_entry(&entry.key, &value) {
                        let display = format!("{} / {}", chrome.origin, chrome.key);
                        let node = scalar(Value::Str(chrome.value), HINT_CHROME, &mut doc)?;
                        (display, node)
                    } else {
                        let display = text_or_hex(&entry.key);
                        let node = scalar(Value::Str(text_or_hex(&value)), HINT_VALUE, &mut doc)?;
                        (display, node)
                    }
                }
                None => (
                    text_or_hex(&entry.key),
                    scalar(Value::Null, HINT_DELETED, &mut doc)?,
                ),
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
    read_only_encode(doc, params, decode)
}

/// One `WriteBatch` entry.
struct BatchEntry {
    key: Vec<u8>,
    value: Option<Vec<u8>>,
}

fn records(input: &[u8]) -> std::result::Result<Vec<Vec<u8>>, crate::kv_common::ParseError> {
    let mut result = Vec::new();
    let mut assembled = Vec::new();
    let mut open = false;
    let mut pos = 0_usize;
    while pos < input.len() {
        let block_start = pos - pos % BLOCK_SIZE;
        let block_end = (block_start + BLOCK_SIZE).min(input.len());
        let left = block_end - pos;
        if left < HEADER_SIZE {
            if input
                .get(pos..block_end)
                .is_some_and(|bytes| bytes.iter().any(|byte| *byte != 0))
            {
                return Err(crate::kv_common::ParseError::new(
                    "the log ends inside a record",
                    pos,
                ));
            }
            pos = block_end;
            continue;
        }
        let header = input.get(pos..pos + HEADER_SIZE).ok_or_else(|| {
            crate::kv_common::ParseError::new("the log record header is cut short", pos)
        })?;
        let crc = u32::from_le_bytes(
            header
                .get(..4)
                .and_then(|bytes| <[u8; 4]>::try_from(bytes).ok())
                .unwrap_or_default(),
        );
        let length = u16::from_le_bytes(
            header
                .get(4..6)
                .and_then(|bytes| <[u8; 2]>::try_from(bytes).ok())
                .unwrap_or_default(),
        );
        let kind = header.get(6).copied().unwrap_or(0);
        if crc == 0 && length == 0 && kind == 0 {
            if input
                .get(pos..block_end)
                .is_some_and(|bytes| bytes.iter().any(|byte| *byte != 0))
            {
                return Err(crate::kv_common::ParseError::new(
                    "the log block padding is not zeroed",
                    pos,
                ));
            }
            pos = block_end;
            continue;
        }
        if !(FULL..=LAST).contains(&kind) {
            return Err(crate::kv_common::ParseError::new(
                format!("the log record type {kind} is not defined"),
                pos + 6,
            ));
        }
        let length_usize = usize::from(length);
        if HEADER_SIZE + length_usize > left {
            return Err(crate::kv_common::ParseError::new(
                "the log record crosses its block boundary",
                pos,
            ));
        }
        let payload_start = pos + HEADER_SIZE;
        let payload = input
            .get(payload_start..payload_start + length_usize)
            .ok_or_else(|| {
                crate::kv_common::ParseError::new("the log record payload is cut short", pos)
            })?;
        let mut checked = Vec::with_capacity(1 + payload.len());
        checked.push(kind);
        checked.extend_from_slice(payload);
        if mask_crc(crc32c(&checked)) != crc {
            return Err(crate::kv_common::ParseError::new(
                "the log record checksum does not match",
                pos,
            ));
        }
        match kind {
            FULL => {
                if open {
                    return Err(crate::kv_common::ParseError::new(
                        "a full record interrupts a fragmented record",
                        pos,
                    ));
                }
                result.push(payload.to_vec());
            }
            FIRST => {
                if open {
                    return Err(crate::kv_common::ParseError::new(
                        "a first fragment interrupts a fragmented record",
                        pos,
                    ));
                }
                assembled.clear();
                assembled.extend_from_slice(payload);
                open = true;
            }
            MIDDLE => {
                if !open {
                    return Err(crate::kv_common::ParseError::new(
                        "a middle fragment has no first fragment",
                        pos,
                    ));
                }
                assembled.extend_from_slice(payload);
            }
            LAST => {
                if !open {
                    return Err(crate::kv_common::ParseError::new(
                        "a last fragment has no first fragment",
                        pos,
                    ));
                }
                assembled.extend_from_slice(payload);
                result.push(std::mem::take(&mut assembled));
                open = false;
            }
            _ => {}
        }
        pos = payload_start + length_usize;
    }
    Ok(result)
}

fn batch(record: &[u8]) -> std::result::Result<Vec<BatchEntry>, crate::kv_common::ParseError> {
    let mut reader = Reader::new(record, 0)
        .ok_or_else(|| crate::kv_common::ParseError::new("the write batch is empty", 0))?;
    reader
        .u64_le()
        .map_err(|error| crate::kv_common::ParseError::new(error.message, error.offset))?;
    let count = reader
        .u32_le()
        .map_err(|error| crate::kv_common::ParseError::new(error.message, error.offset))?;
    let count = usize::try_from(count).unwrap_or(usize::MAX);
    if count > record.len() {
        return Err(crate::kv_common::ParseError::new(
            "the write batch entry count is too large",
            8,
        ));
    }
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        let start = reader.pos();
        let tag = reader
            .byte()
            .map_err(|error| crate::kv_common::ParseError::new(error.message, error.offset))?;
        let key_len = usize::try_from(reader.varint32()?).map_err(|_| {
            crate::kv_common::ParseError::new("the write batch key is too long", start)
        })?;
        let key = reader
            .take(key_len)
            .map_err(|error| crate::kv_common::ParseError::new(error.message, error.offset))?
            .to_vec();
        let value = match tag {
            0 => None,
            1 => {
                let value_len = usize::try_from(reader.varint32()?).map_err(|_| {
                    crate::kv_common::ParseError::new("the write batch value is too long", start)
                })?;
                Some(
                    reader
                        .take(value_len)
                        .map_err(|error| {
                            crate::kv_common::ParseError::new(error.message, error.offset)
                        })?
                        .to_vec(),
                )
            }
            other => {
                return Err(crate::kv_common::ParseError::new(
                    format!("the write batch tag {other} is not defined"),
                    start,
                ));
            }
        };
        entries.push(BatchEntry { key, value });
    }
    if reader.pos() != record.len() {
        return Err(crate::kv_common::ParseError::new(
            "the write batch has trailing bytes",
            reader.pos(),
        ));
    }
    Ok(entries)
}

fn scalar(value: Value, hint: &str, doc: &mut SaveDoc) -> Result<NodeId> {
    let mut node = Node::new(value);
    node.type_hint = Some(TypeHint::new(hint));
    node.flags.insert(NodeFlags::READ_ONLY);
    doc.push(node)
}
