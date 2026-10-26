//! `format.litedb`: LiteDB embedded document databases (`.db`), datafile versions 7 (LiteDB 4) and
//! 8 (LiteDB 5).
//!
//! LiteDB stores fixed 4096-byte (v4) or 8192-byte (v5) pages: a header page with the collection
//! map, collection pages with index definitions, index pages and data pages whose blocks hold BSON
//! documents. The layout comes from the public MIT-licensed LiteDB source (`Engine/Pages/*.cs`,
//! `Engine/Structures/*.cs`, `Engine/FileReader/FileReaderV7.cs`) and its public documentation;
//! this codec is written from the behavior, not copied.
//!
//! # Tree
//!
//! The root is an `Object` of collections (`LITEDB:collections`), sorted by name. A collection is an
//! `Array` of documents (`LITEDB:collection`); each document is an `Object` (`LITEDB:document`)
//! using the BSON element hints of `format.bson` where the value encoding matches (the `_id` and
//! ordinary fields). Containers stay read-only. Integer (32/64-bit), double and boolean values that
//! fit their original slot are editable; strings, binary values, dates and anything that would need
//! a length change are read-only.
//!
//! LiteDB index pages keep a copy of every indexed key, so `_id` and the top-level field of every
//! collection index (the first path segment of its expression) are read-only in v4 and v5. A
//! collection with an index expression that is not a simple `$.Field` path, or whose collection page
//! cannot be read exactly, is read-only as a whole.
//!
//! # Exact writing
//!
//! LiteDB pages carry no checksum, so a fixed-size value can be replaced in place without moving any
//! page content: block lengths, slot tables, free lists and indexes stay valid. `encode` keeps the
//! original bytes and patches the value spans of fixed-size edits; an unchanged document returns the
//! original file. Every other edit, including an indexed field whose read-only flag a caller
//! cleared, is refused.
//!
//! Errors: `SP-LITEDB-001` malformed or truncated input, `SP-LITEDB-002` a change that cannot be
//! written.

mod read;
mod write;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    Codec, Confidence, DecodeCtx, Decoded, Error, ErrorCode, Params, Registry, Result, SaveDoc,
    SniffCtx,
};

/// The file is malformed, truncated, or not a LiteDB datafile.
pub const LITEDB_PARSE: ErrorCode = ErrorCode::new("SP-LITEDB-001");
/// The document cannot be written back to the datafile.
pub const LITEDB_WRITE: ErrorCode = ErrorCode::new("SP-LITEDB-002");

/// The 27-byte marker stored in the header page.
pub(crate) const HEADER_INFO: &[u8; 27] = b"** This is a LiteDB file **";
/// Key of the original bytes in the step parameters.
pub(crate) const ORIGINAL: &str = "litedb:original";
/// Offset of the marker in a v5 (datafile 8) header page.
const V5_INFO_OFFSET: usize = 32;
/// Offset of the marker in a v4 (datafile 7) header page.
const V4_INFO_OFFSET: usize = 25;

/// Registers `format.litedb`.
pub fn register(reg: &mut Registry) {
    reg.add_codec(Arc::new(LiteDbCodec));
}

/// The `format.litedb` codec.
#[derive(Clone, Copy, Debug, Default)]
pub struct LiteDbCodec;

impl Codec for LiteDbCodec {
    fn id(&self) -> &'static str {
        "format.litedb"
    }

    /// The header marker and datafile version are strong content evidence; no extension hint, so a
    /// `.db` that is SQLite stays SQLite and vice versa.
    fn sniff(&self, input: &[u8], _ctx: &SniffCtx<'_>) -> Confidence {
        if header_version(input).is_some() {
            Confidence::new(0.97)
        } else {
            Confidence::NONE
        }
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<Decoded> {
        ctx.limits().check_input(len_u64(input.len()))?;
        let doc = read::decode(input, ctx.limits())?;
        ctx.charge_nodes(len_u64(doc.len().saturating_sub(1)))?;
        let mut params = Params::new();
        params.set(ORIGINAL, input.to_vec());
        Ok(Decoded { doc, params })
    }

    fn encode(&self, doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
        write::encode(doc, params)
    }
}

/// The datafile version (7 for LiteDB 4, 8 for LiteDB 5) when the header marker matches.
pub(crate) fn header_version(input: &[u8]) -> Option<u8> {
    let v5 = input
        .get(V5_INFO_OFFSET..V5_INFO_OFFSET + HEADER_INFO.len())
        .is_some_and(|bytes| bytes == HEADER_INFO)
        && input.get(V5_INFO_OFFSET + HEADER_INFO.len()).copied() == Some(8)
        && input.len() >= 8192;
    if v5 {
        return Some(8);
    }
    let v4 = input
        .get(V4_INFO_OFFSET..V4_INFO_OFFSET + HEADER_INFO.len())
        .is_some_and(|bytes| bytes == HEADER_INFO)
        && input.get(V4_INFO_OFFSET + HEADER_INFO.len()).copied() == Some(7)
        && input.len() >= 4096;
    if v4 { Some(7) } else { None }
}

pub(crate) fn parse_error(message: impl Into<String>, offset: usize) -> Error {
    Error::new(LITEDB_PARSE, message).with_offset(len_u64(offset))
}

pub(crate) fn write_error(message: impl Into<String>) -> Error {
    Error::new(LITEDB_WRITE, message)
}
