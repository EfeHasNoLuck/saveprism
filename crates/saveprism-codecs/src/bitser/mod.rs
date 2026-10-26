//! `format.bitser`: Bitser, the Lua value serializer used by LÖVE games (`.dat`, `.save`, no
//! fixed extension).
//!
//! Written from the public, ISC-licensed `gvx/bitser` repository (`bitser.lua`, version 1.2,
//! `https://github.com/gvx/bitser`). Only the byte format was read; nothing in the data is
//! executed, no Lua value is constructed and no class is instantiated.
//!
//! # Format
//!
//! | Byte(s) | Meaning |
//! |---|---|
//! | `0..=127` | small integer `byte - 27` (-27..100) |
//! | `128..=191` | reference to seen value `byte - 128` (0-based) |
//! | `192..=223` | short string of `byte - 192` bytes |
//! | `224..=239` | short resource name of `byte - 224` bytes |
//! | `240` | table: length, array items, key count, key/value pairs |
//! | `241` | long resource: a string value, registered in the seen list |
//! | `242` | class instance: class name, then a table body |
//! | `243` | long reference: a number, then the seen index (0-based) |
//! | `244` | long string: a number length, then the bytes |
//! | `245` | 32-bit little-endian signed integer |
//! | `246` | 64-bit little-endian IEEE-754 double |
//! | `247` | nil |
//! | `248` / `249` | false / true |
//! | `250` | 16-bit little-endian signed integer |
//! | `253` | table followed by a metatable |
//! | `254` | extension: an id value and a payload value |
//! | `255` | unknown additional type (refused) |
//!
//! The seen list numbers every non-number, non-boolean, non-nil value in the order Bitser adds
//! it: a table is registered before its contents, a string after its bytes, and a long resource
//! reserves its slot before reading its name.
//!
//! # Tree
//!
//! A table is a read-only `Object` with `array` (`Array`), `map` (`Map`), and `class` or
//! `metatable` when present. Scalars and UTF-8 strings are editable; references, resources,
//! byte strings, class names and metatables are read-only. Containers are read-only, so an edit
//! never changes the shape of a table.
//!
//! # Exact writing
//!
//! The original stream is kept. An unchanged value is left byte for byte; an edited scalar or
//! string replaces only its own span with Bitser's canonical encoding. An unchanged document is
//! written back exactly.

mod read;
mod write;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    Codec, Confidence, DecodeCtx, Decoded, ErrorCode, Params, Registry, Result, SaveDoc, SniffCtx,
};

/// The stream is truncated, uses an unknown type or a reference points at nothing.
pub const BITSER_PARSE: ErrorCode = ErrorCode::new("SP-BITSER-001");
/// A document cannot be written as Bitser (shape changed, number out of range).
pub const BITSER_WRITE: ErrorCode = ErrorCode::new("SP-BITSER-002");

/// Prefix of every hint this codec writes.
pub const HINT_PREFIX: &str = "BITSER:";
/// Parameter holding the original stream.
pub(crate) const ORIGINAL: &str = "original";

/// Registers the codec as `format.bitser`.
pub fn register(reg: &mut Registry) {
    reg.add_codec(Arc::new(BitserCodec));
}

/// The `format.bitser` codec.
#[derive(Clone, Copy, Debug, Default)]
pub struct BitserCodec;

impl Codec for BitserCodec {
    fn id(&self) -> &'static str {
        "format.bitser"
    }

    /// No magic number: a complete parse whose root is a table or instance, as a weak hint below
    /// `Confidence::LOW` so a format with a real signature always wins.
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
