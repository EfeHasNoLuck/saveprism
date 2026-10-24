//! `format.msgpack`: MessagePack data, read as a tree and written back byte for byte.
//!
//! Written from the public MessagePack specification (msgpack.org, `msgpack/msgpack` on GitHub,
//! `spec.md`, last modified 2017-08-09). Nothing is executed: the bytes are parsed into plain
//! data. Extensions are opaque bytes and read-only.
//!
//! # Tree
//!
//! | MessagePack | Node | Hint |
//! |---|---|---|
//! | nil | `Null` (read-only) | `MP:nil` |
//! | false / true | `Bool` | `MP:bool` |
//! | integer | `Int` with the width of its format | `MP:int` |
//! | float 32 / 64 | `Float` with `F32` / `F64` | `MP:float` |
//! | str | `Str` (or `Bytes` read-only when not UTF-8) | `MP:str` / `MP:str-bytes` |
//! | bin | `Bytes` (read-only) | `MP:bin` |
//! | array | `Array` (container read-only) | `MP:array` |
//! | map | `Map` (container and keys read-only) | `MP:map` |
//! | ext | `Bytes` with the payload (read-only) | `MP:ext:<type>` |
//!
//! Containers keep their shape, so an edit never changes the number of elements. Scalars, strings,
//! map values and array elements are editable.
//!
//! # Exact writing
//!
//! The original stream is kept in the parameter `original`. Encoding decodes it again, walks both
//! trees side by side and replaces only the byte span of an edited leaf. Every unchanged format
//! (a wide integer, a non-shortest header, an extension) therefore stays exactly as it was. An
//! unchanged document gives the original bytes.

mod read;
mod write;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    Codec, Confidence, DecodeCtx, Decoded, ErrorCode, Params, Registry, Result, SaveDoc, SniffCtx,
};

/// The data is truncated, has an unknown format byte or does not consume the whole input.
pub const MSGPACK_PARSE: ErrorCode = ErrorCode::new("SP-MSGPACK-001");
/// A document cannot be written as MessagePack (a shape or type changed).
pub const MSGPACK_WRITE: ErrorCode = ErrorCode::new("SP-MSGPACK-002");

/// Prefix of every hint this codec writes.
pub const HINT_PREFIX: &str = "MP:";
/// Parameter holding the original stream.
pub(crate) const ORIGINAL: &str = "original";

/// Registers the codec as `format.msgpack`.
pub fn register(reg: &mut Registry) {
    reg.add_codec(Arc::new(MessagePackCodec));
}

/// The `format.msgpack` codec.
#[derive(Clone, Copy, Debug, Default)]
pub struct MessagePackCodec;

impl Codec for MessagePackCodec {
    fn id(&self) -> &'static str {
        "format.msgpack"
    }

    /// Only a complete parse whose root is a container; MessagePack accepts almost any byte, so
    /// the confidence stays low.
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
