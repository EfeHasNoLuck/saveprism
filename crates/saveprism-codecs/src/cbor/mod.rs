//! `format.cbor`: CBOR data (RFC 8949), read as a tree and written back byte for byte.
//!
//! Written from RFC 8949 (STD 94, December 2020). Major types 0 to 7 are read. Tags are kept as a
//! read-only wrapper object with one child, so their content is still editable. Indefinite-length
//! arrays, maps, byte strings and text strings keep their form because untouched bytes are reused.
//!
//! # Tree
//!
//! | CBOR | Node | Hint |
//! |---|---|---|
//! | unsigned / negative integer | `Int` | `CBOR:int` |
//! | byte string | `Bytes` (read-only) | `CBOR:bytes` / `CBOR:bytes:indef` |
//! | text string | `Str` (or `Bytes` read-only when not UTF-8) | `CBOR:text` / `CBOR:text:indef` |
//! | array | `Array` (container read-only) | `CBOR:array` / `CBOR:array:indef` |
//! | map | `Map` (container and keys read-only) | `CBOR:map` / `CBOR:map:indef` |
//! | tag N | `Object` with one `(tag)` child (read-only) | `CBOR:tag:N` |
//! | false / true | `Bool` | `CBOR:bool` |
//! | null | `Null` (read-only) | `CBOR:null` |
//! | undefined | `Null` (read-only) | `CBOR:undefined` |
//! | simple value | `Int` (read-only) | `CBOR:simple` |
//! | float 32 / 64 | `Float` with `F32` / `F64` | `CBOR:float` |
//! | float 16 | `Float` (read-only) | `CBOR:float16` |
//!
//! # Exact writing
//!
//! The original bytes are the parameter `original`. Encoding decodes them again and replaces only
//! the spans of edited leaves, so a non-shortest header, a half float or an indefinite string is
//! never rewritten unless it was edited. An unchanged document gives the original bytes.

mod read;
mod write;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    Codec, Confidence, DecodeCtx, Decoded, ErrorCode, Params, Registry, Result, SaveDoc, SniffCtx,
};

/// The data is not well-formed CBOR, ends early or has bytes after the first item.
pub const CBOR_PARSE: ErrorCode = ErrorCode::new("SP-CBOR-001");
/// A document cannot be written as CBOR (a shape or type changed).
pub const CBOR_WRITE: ErrorCode = ErrorCode::new("SP-CBOR-002");

/// Prefix of every hint this codec writes.
pub const HINT_PREFIX: &str = "CBOR:";
/// Parameter holding the original stream.
pub(crate) const ORIGINAL: &str = "original";

/// Registers the codec as `format.cbor`.
pub fn register(reg: &mut Registry) {
    reg.add_codec(Arc::new(CborCodec));
}

/// The `format.cbor` codec.
#[derive(Clone, Copy, Debug, Default)]
pub struct CborCodec;

impl Codec for CborCodec {
    fn id(&self) -> &'static str {
        "format.cbor"
    }

    /// Only a complete well-formed item whose root is a container; CBOR accepts almost any byte,
    /// so the confidence stays low.
    fn sniff(&self, input: &[u8], _ctx: &SniffCtx<'_>) -> Confidence {
        if input.is_empty() || !read::plausible(input, &saveprism_core::Limits::default()) {
            return Confidence::NONE;
        }
        Confidence::LOW
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
