//! `format.leveldb-sst` and `format.leveldb-log`: LevelDB table and write-ahead log files.
//!
//! LevelDB keeps a database in a directory: `*.ldb`/`*.sst` sorted tables, `*.log` write-ahead logs,
//! `MANIFEST-*` and `CURRENT`. A save is therefore a folder; this codec opens one file at a time,
//! read-only. The byte layout comes from the public LevelDB `doc/table_format.md` and
//! `doc/log_format.md` (BSD-3-Clause), and from `table/block_builder.cc`, `table/format.cc` and
//! `db/log_writer.cc` in the same public tree.
//!
//! # Tree
//!
//! The root is an `Object` of key/value pairs (`LEVELDB:entries`). Keys and values are shown as text
//! when they are plain UTF-8, as Chromium Local Storage records when they use Chrome's
//! `_<origin>\0<typed key>` framing (`LEVELDB:chrome-local-storage`), and otherwise as hex
//! (`0x...`). Deleted entries are `Null` with `LEVELDB:deleted`. Every node is read-only.
//!
//! A table key is a LevelDB internal key; its eight-byte sequence/type trailer is removed before
//! display. A log record is a `WriteBatch` (sequence, count, put/delete entries), so repeated keys
//! stay in file order.
//!
//! # Why read-only
//!
//! Editing a LevelDB file in place is not exact: a value with a different length changes block
//! sizes, restart offsets, index entries, checksums and possibly the compaction level, and a log
//! edit changes the WriteBatch length and checksum. The original bytes are kept and `encode`
//! returns them unchanged; any change is refused.
//!
//! Errors: `SP-LEVELDB-001` malformed or truncated input, `SP-LEVELDB-002` a change that cannot be
//! written (the file is read-only), `SP-LEVELDB-003` a feature this codec does not support.

mod log;
mod sst;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    Codec, Confidence, DecodeCtx, Decoded, Error, ErrorCode, Params, Registry, Result, SaveDoc,
    SniffCtx,
};

/// The file is malformed, truncated, or not a LevelDB file.
pub const LEVELDB_PARSE: ErrorCode = ErrorCode::new("SP-LEVELDB-001");
/// The document cannot be written back to the file.
pub const LEVELDB_WRITE: ErrorCode = ErrorCode::new("SP-LEVELDB-002");
/// The file uses a feature this codec does not support.
pub const LEVELDB_UNSUPPORTED: ErrorCode = ErrorCode::new("SP-LEVELDB-003");

/// The footer magic of a LevelDB sorted table (`0xdb4775248b80fb57`).
pub(crate) const TABLE_MAGIC: u64 = 0xdb47_7524_8b80_fb57;
/// Key of the original bytes in the step parameters.
pub(crate) const ORIGINAL: &str = "leveldb:original";
/// Most bytes `sniff` is willing to parse for a log file.
const SNIFF_LIMIT: usize = 8 * 1024 * 1024;

/// Registers `format.leveldb-sst` and `format.leveldb-log`.
pub fn register(reg: &mut Registry) {
    reg.add_codec(Arc::new(LevelDbSstCodec));
    reg.add_codec(Arc::new(LevelDbLogCodec));
}

/// The `format.leveldb-sst` codec.
#[derive(Clone, Copy, Debug, Default)]
pub struct LevelDbSstCodec;

impl Codec for LevelDbSstCodec {
    fn id(&self) -> &'static str {
        "format.leveldb-sst"
    }

    fn sniff(&self, input: &[u8], ctx: &SniffCtx<'_>) -> Confidence {
        if input.len() >= 48 && sst::has_magic(input) {
            return Confidence::new(0.98);
        }
        if ctx.file_name.is_some_and(|name| {
            let lower = name.to_ascii_lowercase();
            lower.ends_with(".ldb") || lower.ends_with(".sst")
        }) {
            return Confidence::new(0.3);
        }
        Confidence::NONE
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<Decoded> {
        ctx.limits().check_input(len_u64(input.len()))?;
        let (doc, params) = sst::decode(input, ctx.limits())?;
        ctx.charge_nodes(len_u64(doc.len().saturating_sub(1)))?;
        Ok(Decoded { doc, params })
    }

    fn encode(&self, doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
        sst::encode(doc, params)
    }
}

/// The `format.leveldb-log` codec.
#[derive(Clone, Copy, Debug, Default)]
pub struct LevelDbLogCodec;

impl Codec for LevelDbLogCodec {
    fn id(&self) -> &'static str {
        "format.leveldb-log"
    }

    fn sniff(&self, input: &[u8], ctx: &SniffCtx<'_>) -> Confidence {
        let named = ctx
            .file_name
            .is_some_and(|name| name.to_ascii_lowercase().ends_with(".log"));
        if input.is_empty() {
            return Confidence::NONE;
        }
        if input.len() > SNIFF_LIMIT {
            return if named {
                Confidence::LOW
            } else {
                Confidence::NONE
            };
        }
        if log::plausible(input) {
            if named {
                Confidence::new(0.65)
            } else {
                Confidence::new(0.3)
            }
        } else {
            Confidence::NONE
        }
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<Decoded> {
        ctx.limits().check_input(len_u64(input.len()))?;
        let (doc, params) = log::decode(input, ctx.limits())?;
        ctx.charge_nodes(len_u64(doc.len().saturating_sub(1)))?;
        Ok(Decoded { doc, params })
    }

    fn encode(&self, doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
        log::encode(doc, params)
    }
}

pub(crate) fn parse_error(message: impl Into<String>, offset: impl TryInto<u64>) -> Error {
    let offset = offset.try_into().unwrap_or(0);
    Error::new(LEVELDB_PARSE, message).with_offset(offset)
}

pub(crate) fn write_error(message: impl Into<String>) -> Error {
    Error::new(LEVELDB_WRITE, message)
}

type DecodeFn = fn(&[u8], &saveprism_core::Limits) -> Result<(SaveDoc, Params)>;

/// The read-only `encode` both file kinds share: unchanged documents return the original bytes.
pub(crate) fn read_only_encode(
    doc: &SaveDoc,
    params: &Params,
    decode: DecodeFn,
) -> Result<Vec<u8>> {
    let original = match params.get(ORIGINAL) {
        Some(saveprism_core::ParamValue::Bytes(bytes)) => bytes.as_slice(),
        _ => return Err(write_error("original bytes are missing")),
    };
    let limits = saveprism_core::Limits::default();
    limits.check_input(len_u64(original.len()))?;
    let (baseline, _) = decode(original, &limits)?;
    if doc.tree_eq(&baseline) {
        Ok(original.to_vec())
    } else {
        Err(write_error(
            "a LevelDB file cannot be edited in place exactly, so it is read-only",
        ))
    }
}
