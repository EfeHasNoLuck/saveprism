//! `format.protobuf`: schema-less Protocol Buffers (`.pb`, `.pbf`, `.protobuf`, `.protobin`,
//! `.bytes`, and protocol-buffer payloads inside other saves).
//!
//! Written from the public Protocol Buffers encoding guide
//! (`https://protobuf.dev/programming-guides/encoding/`, checked 2026-10-26) and the "Condensed
//! reference card" at its end. Nothing is executed: the wire format is parsed into plain data.
//!
//! # Wire format
//!
//! A message is a sequence of `(tag, value)` records. The tag is a varint
//! `(field_number << 3) | wire_type`; the six wire types are VARINT (0), I64 (1), LEN (2),
//! SGROUP (3), EGROUP (4) and I32 (5). Field numbers must be 1..2^29-1 and may not use the
//! reserved 19000..19999 range. Varints are little-endian base-128 with a continuation bit; I64
//! and I32 payloads are little-endian fixed-width values; LEN payloads are a length varint
//! followed by that many bytes.
//!
//! # Tree
//!
//! | Wire data | Node | Hint | Editable |
//! |---|---|---|---|
//! | message | `Array` of field objects | `PB:message` | no (shape is fixed) |
//! | field | `Object` with `number`, `wire`, `value` | `PB:field` | the `value` only |
//! | VARINT | `Int` (`U64`) | `PB:varint` | yes |
//! | I64 | `Int` (`U64`) | `PB:fixed64` | yes |
//! | I32 | `Int` (`U32`) | `PB:fixed32` | yes |
//! | LEN, nested message | `Array` | `PB:message` | nested values only |
//! | LEN, UTF-8 | `Str` | `PB:string` | yes |
//! | LEN, other bytes | `Bytes` | `PB:bytes` | no |
//! | group | `Bytes` (tag to end tag) | `PB:group` | no |
//!
//! A length-delimited field is a nested message only when its payload parses completely under the
//! same rules and its canonical re-encoding is exactly those bytes. Otherwise it is a string when
//! the bytes are valid UTF-8, else bytes. Groups are deprecated and kept as opaque read-only
//! bytes. Because protobuf has no magic number, `sniff` never reports more than a weak hint and
//! never outranks a codec with a real signature.
//!
//! # Exact writing
//!
//! The original bytes are kept as the parameter `original`. An unchanged field is copied byte for
//! byte (including a non-minimal tag or varint); an edited scalar or length-delimited field is
//! re-encoded canonically, and the length prefix of every enclosing message is recomputed from
//! the new payload. A file with no edits therefore reproduces its input exactly.

mod read;
mod write;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    Codec, Confidence, DecodeCtx, Decoded, ErrorCode, Params, Registry, Result, SaveDoc, SniffCtx,
};

/// The bytes are not a complete message (bad tag, unknown wire type, a field past its message).
pub const PROTO_PARSE: ErrorCode = ErrorCode::new("SP-PROTO-001");
/// A document cannot be written as a protobuf message (shape changed, value out of range).
pub const PROTO_WRITE: ErrorCode = ErrorCode::new("SP-PROTO-002");

/// Prefix of every hint this codec writes.
pub const HINT_PREFIX: &str = "PB:";
/// Parameter holding the original stream.
pub(crate) const ORIGINAL: &str = "original";

/// Registers the codec as `format.protobuf`.
pub fn register(reg: &mut Registry) {
    reg.add_codec(Arc::new(ProtobufCodec));
}

/// The `format.protobuf` codec.
#[derive(Clone, Copy, Debug, Default)]
pub struct ProtobufCodec;

impl Codec for ProtobufCodec {
    fn id(&self) -> &'static str {
        "format.protobuf"
    }

    /// Protobuf has no magic, so this is deliberately a weak hint: a full parse with at least one
    /// field, below `Confidence::LOW` so any format with a real signature wins.
    fn sniff(&self, input: &[u8], _ctx: &SniffCtx<'_>) -> Confidence {
        if read::plausible(input, &saveprism_core::Limits::default()) {
            Confidence::new(0.2)
        } else {
            Confidence::NONE
        }
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
