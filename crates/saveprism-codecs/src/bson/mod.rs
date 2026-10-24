//! `format.bson`: BSON documents (bsonspec.org, specification 1.1).
//!
//! The document is a length-prefixed list of typed, named elements. This codec reads the tree and
//! writes it again with every length recomputed.
//!
//! # Tree
//!
//! The root is an `Object` with the hint `BSON:document`; an array is an `Array` with `BSON:array`.
//! Element names are object keys and stay read-only.
//!
//! | BSON | Node | Hint | Editable |
//! |---|---|---|---|
//! | double | `Float` (`F64`) | `BSON:double` | yes |
//! | string | `Str` | `BSON:string` | yes |
//! | embedded document | `Object` | `BSON:document` | container read-only |
//! | array | `Array` | `BSON:array` | container read-only |
//! | int32 / int64 | `Int` (`I32` / `I64`) | `BSON:int32` / `BSON:int64` | yes |
//! | boolean | `Bool` | `BSON:bool` | yes |
//! | null | `Null` | `BSON:null` | no |
//! | ObjectId | `Bytes` (12) | `BSON:objectid` | no |
//! | UTC datetime | `Int` (`I64`) | `BSON:date` | no |
//! | binary | `Bytes` | `BSON:binary:<subtype>` | no |
//! | regex / DBPointer / code with scope | `Bytes` (raw payload) | `BSON:regex`, ... | no |
//! | JavaScript / symbol | `Str` | `BSON:code` / `BSON:symbol` | no |
//! | timestamp | `Int` (`U64`) | `BSON:timestamp` | no |
//! | decimal128 | `Bytes` (16) | `BSON:decimal128` | no |
//! | undefined / min / max key | `Null` | `BSON:undefined`, ... | no |
//!
//! # Writing
//!
//! The whole document is written again: element order, names and types are kept, nested document
//! and string lengths are recomputed. BSON has one canonical representation for these values, so
//! an unchanged document gives the original bytes.

mod read;
mod write;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    Codec, Confidence, DecodeCtx, Decoded, ErrorCode, Params, Registry, Result, SaveDoc, SniffCtx,
};

/// The document length is wrong, an element is truncated or an unknown type is used.
pub const BSON_PARSE: ErrorCode = ErrorCode::new("SP-BSON-001");
/// A document cannot be written as BSON (a value does not fit or a name contains a NUL).
pub const BSON_WRITE: ErrorCode = ErrorCode::new("SP-BSON-002");

/// Prefix of every hint this codec writes.
pub const HINT_PREFIX: &str = "BSON:";

/// Registers the codec as `format.bson`.
pub fn register(reg: &mut Registry) {
    reg.add_codec(Arc::new(BsonCodec));
}

/// The `format.bson` codec.
#[derive(Clone, Copy, Debug, Default)]
pub struct BsonCodec;

impl Codec for BsonCodec {
    fn id(&self) -> &'static str {
        "format.bson"
    }

    /// The declared length must be the whole input and the document must parse completely.
    fn sniff(&self, input: &[u8], _ctx: &SniffCtx<'_>) -> Confidence {
        if input.len() < 5 || !read::plausible(input, &saveprism_core::Limits::default()) {
            return Confidence::NONE;
        }
        Confidence::new(0.4)
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<Decoded> {
        ctx.limits().check_input(len_u64(input.len()))?;
        let (doc, params) = read::decode(input, ctx.limits())?;
        ctx.charge_nodes(len_u64(doc.len().saturating_sub(1)))?;
        Ok(Decoded { doc, params })
    }

    fn encode(&self, doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
        write::encode(doc, params)
    }
}
