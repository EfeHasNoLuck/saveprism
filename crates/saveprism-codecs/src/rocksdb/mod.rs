//! `format.rocksdb`: RocksDB block-based table (`.sst`) files.
//!
//! RocksDB shares the LevelDB block layout (prefix-compressed entries, restart array, five-byte
//! block trailer) and adds its own footer, compression types and an index handle in the metaindex
//! block from format version 6. The public sources used here are the RocksDB `table/format.cc`
//! footer description (Apache-2.0/BSD), `-include/rocksdb/compression_type.h` and
//! `table/block_based/block_based_table_builder.cc` (all public, permissive). Files are read-only.
//!
//! # Tree
//!
//! The root is an `Object` of key/value pairs (`ROCKSDB:entries`), in table order. Internal keys
//! lose their eight-byte sequence/type trailer; values are shown as text when plain UTF-8 and as
//! hex (`0x...`) otherwise. Every node is read-only.
//!
//! # Supported
//!
//! Block-based tables with format version 2 to 7, a one-level (binary-search) index and no
//! user-defined timestamps. Blocks may use no compression, Snappy, zlib, bzip2 or Zstandard.
//! LZ4/LZ4HC, XPRESS, custom compression, format version 8, two-level indexes and delta-encoded
//! index values are detected and refused with `SP-ROCKSDB-003`.
//!
//! # Why read-only
//!
//! A value of a different length changes the block layout, restart offsets, index handles and
//! checksums, so the original table cannot be reproduced by an in-place patch. `encode` returns the
//! original bytes for an unchanged document and refuses edits.
//!
//! Errors: `SP-ROCKSDB-001` malformed or truncated input, `SP-ROCKSDB-002` a change that cannot be
//! written, `SP-ROCKSDB-003` an unsupported RocksDB feature.

mod read;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    Codec, Confidence, DecodeCtx, Decoded, Error, ErrorCode, Params, Registry, Result, SaveDoc,
    SniffCtx,
};

/// The file is malformed, truncated, or not a RocksDB table.
pub const ROCKSDB_PARSE: ErrorCode = ErrorCode::new("SP-ROCKSDB-001");
/// The document cannot be written back to the file.
pub const ROCKSDB_WRITE: ErrorCode = ErrorCode::new("SP-ROCKSDB-002");
/// The file uses a RocksDB feature this codec does not support.
pub const ROCKSDB_UNSUPPORTED: ErrorCode = ErrorCode::new("SP-ROCKSDB-003");

/// The block-based table magic number (`0x88e241b785f4cff7`).
pub(crate) const TABLE_MAGIC: u64 = 0x88e2_41b7_85f4_cff7;
/// Key of the original bytes in the step parameters.
pub(crate) const ORIGINAL: &str = "rocksdb:original";

/// Registers `format.rocksdb`.
pub fn register(reg: &mut Registry) {
    reg.add_codec(Arc::new(RocksDbCodec));
}

/// The `format.rocksdb` codec.
#[derive(Clone, Copy, Debug, Default)]
pub struct RocksDbCodec;

impl Codec for RocksDbCodec {
    fn id(&self) -> &'static str {
        "format.rocksdb"
    }

    fn sniff(&self, input: &[u8], ctx: &SniffCtx<'_>) -> Confidence {
        if input.len() >= read::MIN_FILE && read::has_magic(input) {
            return Confidence::new(0.98);
        }
        if ctx.file_name.is_some_and(|name| {
            let lower = name.to_ascii_lowercase();
            lower.ends_with(".sst") || lower.ends_with(".sst.tmp")
        }) {
            return Confidence::new(0.3);
        }
        Confidence::NONE
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<Decoded> {
        ctx.limits().check_input(len_u64(input.len()))?;
        let (doc, params) = read::decode(input, ctx.limits())?;
        ctx.charge_nodes(len_u64(doc.len().saturating_sub(1)))?;
        Ok(Decoded { doc, params })
    }

    fn encode(&self, doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
        read::encode(doc, params)
    }
}

pub(crate) fn parse_error(message: impl Into<String>, offset: impl TryInto<u64>) -> Error {
    let offset = offset.try_into().unwrap_or(0);
    Error::new(ROCKSDB_PARSE, message).with_offset(offset)
}

pub(crate) fn unsupported(message: impl Into<String>) -> Error {
    Error::new(ROCKSDB_UNSUPPORTED, message)
}

pub(crate) fn write_error(message: impl Into<String>) -> Error {
    Error::new(ROCKSDB_WRITE, message)
}
