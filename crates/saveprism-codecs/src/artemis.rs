//! `format.artemis`: Artemis Engine save envelopes (`save/*.dat`, `saveg.dat`).
//!
//! The only publicly documented part is the outer envelope used by the public save-injection
//! notes and samples (<https://github.com/Gezine/ArtemisLuaLoader>, the archived reader's issue
//! #8): the ASCII magic `BOWS` (slot) or `BOWG` (global), a little-endian version word (1002 in
//! the public samples), the uncompressed size (u32 little-endian) and a zlib stream. The inner
//! body is engine state whose layout has no public description, so it is shown as read-only bytes
//! with that reason. Nothing is executed.
//!
//! # Tree
//!
//! An `Object` `ART:save` with `magic`, `version`, `raw_size`, a read-only `note` and the
//! decompressed body as read-only `ART:body` bytes.
//!
//! # Exact writing
//!
//! The original bytes are kept as the codec parameter `original`; `encode` returns them, so an
//! unchanged file round-trips exactly. There is nothing to edit yet.

use std::sync::Arc;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    Codec, Confidence, DecodeCtx, Decoded, Error, ErrorCode, Params, Registry, Result, SaveDoc,
    SniffCtx,
};

use crate::alicesoft::{is_artemis, read};

/// The envelope is cut short or its zlib body does not decompress.
pub const ART_PARSE: ErrorCode = ErrorCode::new("SP-ART-001");
/// The recipe has no original bytes to return.
pub const ART_WRITE: ErrorCode = ErrorCode::new("SP-ART-002");

/// Registers the Artemis envelope codec.
pub fn register(reg: &mut Registry) {
    reg.add_codec(Arc::new(ArtemisCodec));
}

/// The `format.artemis` codec.
#[derive(Clone, Copy, Debug, Default)]
pub struct ArtemisCodec;

impl Codec for ArtemisCodec {
    fn id(&self) -> &'static str {
        "format.artemis"
    }

    fn sniff(&self, input: &[u8], _ctx: &SniffCtx<'_>) -> Confidence {
        if is_artemis(input) {
            Confidence::new(0.96)
        } else {
            Confidence::NONE
        }
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<Decoded> {
        ctx.limits().check_input(len_u64(input.len()))?;
        let (doc, params) = read::artemis(input, ctx)?;
        Ok(Decoded { doc, params })
    }

    fn encode(&self, _doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
        match params.get(crate::alicesoft::P_ORIGINAL) {
            Some(saveprism_core::ParamValue::Bytes(bytes)) => Ok(bytes.clone()),
            _ => Err(Error::new(
                ART_WRITE,
                "the recipe has no original Artemis bytes to return",
            )),
        }
    }
}
