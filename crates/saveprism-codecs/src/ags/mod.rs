//! `format.ags`: Adventure Game Studio save games (`agssave.000` ... `agssave.999`).
//!
//! Written from the public, Artistic-License-2.0 AGS engine sources
//! (<https://github.com/adventuregamestudio/ags>): `Engine/game/savegame.cpp`,
//! `Engine/game/savegame.h`, `Engine/game/savegame_components.cpp` and
//! `Common/util/string_utils.cpp`. Everything here is an independent implementation; no code or
//! tables were copied. Nothing in a save is executed.
//!
//! # Layout
//!
//! A modern save starts with the ASCII signature `Adventure Game Studio saved game v2`, followed
//! by the save-format version (`int32`). Saves before AGS 3.5 (the legacy signature and version 8)
//! are refused: the engine itself no longer reads them.
//!
//! Version 3.6.3 (3060300) adds a five-field file-format table, all `uint32`:
//! `FileFormatSize`, `Flags`, `EnvInfoOffset`, `UserDescOffset` and `GameDataOffset`. The offset
//! fields may point past padding, so the reader seeks to them instead of assuming a packed header.
//!
//! The environment block (version 13 and later) starts with its own `uint32` size, then five
//! length-prefixed UTF-8 strings (engine name, engine version, game GUID, game title, main data
//! file name). From version 10 a `int32` main-data version follows, then the `int32` colour depth,
//! and from version 13 the `int32` legacy game id. AGS strings are `int32` byte length followed by
//! the bytes (no terminator).
//!
//! The user description is one string, then the optional screenshot: a `uint32` flags word
//! (`1` present, `2` deflate) and, when present, `int32` width, height, colour depth, and either
//! raw pixels or a `uint32` reserved field, a `uint32` compressed size and the deflate stream.
//!
//! Everything from `GameDataOffset` to the end is version-dependent engine state. It is exposed
//! as read-only bytes. The component list at that offset is parsed read-only: each component's
//! name, format version, flags, data offset and sizes. When the `Script Modules` component is
//! present, the global script data is exposed as read-only 32-bit values with their byte offsets
//! (AGS saves do not store variable names).
//!
//! # Tree
//!
//! The root is a read-only `Object` with the hint `AGS:save`:
//!
//! | Key | Node | Editable |
//! |---|---|---|
//! | `signature` | `Str` | no |
//! | `version` | `Int` (I32) | no |
//! | `format` | `Object` `AGS:format` of `U32` fields | no |
//! | `environment` | `Object` `AGS:environment` | strings yes, numbers no |
//! | `description` | `Object` `AGS:description` | `user_text` yes, screenshot no |
//! | `components` | `Array` of `AGS:component` objects | no |
//! | `script_globals` | `Array` of `AGS:script-global` objects | no |
//! | `game_data` | `Bytes` | no |
//!
//! # Exact writing
//!
//! The writer rebuilds the header from the tree, recomputing every size and offset, and appends the
//! untouched game-data bytes. A save written by the AGS engine is packed exactly as this writer
//! packs it, so an unchanged document reproduces the original bytes. A file with non-standard
//! padding in its header fails the round-trip guard and opens read-only, as required.

mod read;
#[cfg(test)]
mod tests;
mod write;

use std::sync::Arc;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    Codec, Confidence, DecodeCtx, Decoded, Error, ErrorCode, Params, Registry, Result, SaveDoc,
    SniffCtx,
};

/// The file is shorter than its header or a field does not parse.
pub const AGS_PARSE: ErrorCode = ErrorCode::new("SP-AGS-001");
/// The save uses the pre-3.5 legacy format or a newer save version than this codec knows.
pub const AGS_UNSUPPORTED: ErrorCode = ErrorCode::new("SP-AGS-002");
/// A value cannot be written back (a string is not valid UTF-8, an offset does not fit, ...).
pub const AGS_WRITE: ErrorCode = ErrorCode::new("SP-AGS-003");

/// Prefix of every hint this codec writes.
pub const HINT_PREFIX: &str = "AGS:";

/// Signature of a modern AGS save.
pub const SIGNATURE: &[u8] = b"Adventure Game Studio saved game v2";
/// Signature of the pre-3.5 format, which the current engine no longer supports.
pub const LEGACY_SIGNATURE: &[u8] = b"Adventure Game Studio saved game";
/// Save-format version of AGS 3.6.3, the newest layout this codec knows.
pub const CURRENT_VERSION: i32 = 3_060_300;
/// First new-style (component) save version.
pub const LOWEST_VERSION: i32 = 9;
/// First version with the environment-block size and legacy id.
pub const VERSION_351: i32 = 13;
/// First version with the file-format offset table.
pub const VERSION_363: i32 = 306_0300;
/// First version with the main-data version field.
pub const VERSION_COMPONENTS_EX: i32 = 10;

/// Registers the Adventure Game Studio codec as `format.ags`.
pub fn register(reg: &mut Registry) {
    reg.add_codec(Arc::new(AgsCodec));
}

/// The `format.ags` codec.
#[derive(Clone, Copy, Debug, Default)]
pub struct AgsCodec;

impl Codec for AgsCodec {
    fn id(&self) -> &'static str {
        "format.ags"
    }

    /// The exact modern signature; the legacy signature also matches so `decode` can explain why
    /// the file is refused.
    fn sniff(&self, input: &[u8], _ctx: &SniffCtx<'_>) -> Confidence {
        if input.starts_with(SIGNATURE) {
            Confidence::new(0.99)
        } else if input.starts_with(LEGACY_SIGNATURE) {
            Confidence::new(0.9)
        } else {
            Confidence::NONE
        }
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<Decoded> {
        ctx.limits().check_input(len_u64(input.len()))?;
        let shared: &DecodeCtx<'_> = ctx;
        let (doc, params) = read::read_all(input, shared)?;
        ctx.charge_nodes(len_u64(doc.len()))?;
        Ok(Decoded { doc, params })
    }

    fn encode(&self, doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
        write::encode(doc, params)
    }
}

pub(crate) fn parse_error(message: impl Into<String>) -> Error {
    Error::new(AGS_PARSE, message)
}
