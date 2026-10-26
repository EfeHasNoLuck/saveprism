//! `format.witcher3`: The Witcher 3: Wild Hunt (PC) saves (`*.sav`) from REDengine 3.
//!
//! The layout was confirmed against public, MIT-licensed projects, read for behaviour only; no
//! code, tables or regexes were copied:
//!
//! - `Atvaark/W3SavegameEditor` (`ChunkedLz4/*.cs`, `Savegame/SavegameFile.cs`,
//!   `VariableParsers/*.cs`, `Values/Gwint/*.cs`): the `SNFHFZLC` chunk table, the `SAV3`
//!   stream header, the footer with the variable table and the `NM`/`RB` section offsets, the
//!   `BS`/`VL`/`OP`/`AVAL`/`PORP`/`BLCK` record tags, and the Gwent record names and fields
//!   (`CR4GwintManager`, `SBCollectionCardSize`, `SBCollectionCard`, `SBLeaderCollectionCardSize`,
//!   `SBLeaderCollectionCard`, `SBSelectedDeckIndex`, `GwintDecks`, `GwintDeck`, `GwintDeckCard`,
//!   `eGwintFaction`).
//! - `dodojesuslol/w3-quest-tracker` (`docs/save-format.md`, `src/w3qt/savefile.py`): the
//!   decompressed stream excludes the container prefix while every stored offset includes it,
//!   the end footer triplet, the REDengine length-prefixed string, and the record tree built
//!   from the variable table.
//! - `solusipse/witcher3-inventory-editor` (`w3_decompress.c`, `w3_parse.c`): the prefix length
//!   and the chunk table order.
//! - `cahian/witcher3-remastered-to-nextgen-save-converter` (`docs/format.md`,
//!   `w3save/container.py`): the last chunk's end offset is 0, a chunk whose compressed size
//!   equals its decompressed size is stored raw, and internal offsets refer to the full
//!   decompressed image.
//!
//! # Tree
//!
//! The root is an `Object` (hint `W3:save`) with the save, game and build version (read-only)
//! and a `Variables` object. `Variables` holds the records from the variable table, nested by
//! their stored offset ranges. A record whose tag is `BS` or `BLCK` becomes a read-only object;
//! a `VL`, `OP`, `AVAL` or `PORP` record with a confirmed scalar type becomes an editable leaf
//! (hint `W3:<type>`); an `SS`, `SXAP` or `ROTS` payload and unknown tags stay read-only `Bytes`.
//!
//! Editable leaves: `Bool`, `Int8`/`Int16`/`Int32`/`Int64`, `Uint8`/`Uint16`/`Uint32`/`Uint64`,
//! `Float`, `Double`, narrow or UTF-16 `String`, `StringAnsi`, and scalar elements of
//! `array:2,0,<scalar>` values (inventory quantities live here or in ordinary `VL` records).
//! Strings may only change to text that takes the same number of bytes; anything else fails with
//! [`W3_WRITE`]. `CName`, enums, `CGUID`, handles and opaque payloads stay read-only: their bytes
//! cannot be rewritten without understanding the game schema. The `MANU` name table accepts both
//! the single-byte and the extended low-6-bit length form, because public parsers disagree on
//! which one older and next-gen saves use.
//!
//! # Gwent
//!
//! The Gwent state lives in the `Variables/CR4GwintManager` record (older saves may spell it
//! `GwintManager`). Its collection blocks (`SBCollectionCardSize`, `SBLeaderCollectionCardSize`)
//! are hinted `W3:gwent-collection`, `GwintDecks` is `W3:gwent-decks`, every `GwintDeck` is
//! `W3:gwent-deck`, and the selected faction (`SBSelectedDeckIndex/deckIndex`) is
//! `W3:gwent-selected-deck`. Each card record (`SBCollectionCard`, `SBLeaderCollectionCard`,
//! `GwintDeckCard`) is hinted `W3:gwent-card:<cardIndex>` so a UI can list owned and missing
//! cards without a name table.
//!
//! Only `numCopies` (the per-card count inside a collection) stays editable: it has the same
//! width, so it can be patched in place. The collection `Size`, `DeckCount`, `CardCount`,
//! `LeaderIndex`, `DeckUnlocked`, every `cardIndex` and the selected faction stay read-only:
//! changing them alone would change a length, rewrite deck membership or leave the structure
//! inconsistent. Card names and factions are not embedded in the save; see the wave report for
//! where the game keeps its card definitions.
//!
//! # Exact writing
//!
//! The decoder keeps the original file, the decompressed stream, the prefix length and the
//! per-chunk decompressed sizes. Every editable leaf carries a span into the decompressed
//! stream. If no leaf changed, `encode` returns the original file bytes unchanged. Otherwise it
//! patches the spans (all same length), compresses each chunk again with the original
//! decompressed sizes (LZ4 block; a chunk that does not shrink is stored raw), rewrites every
//! chunk table entry and keeps the original prefix padding. Changing the stream length is not
//! supported, because that would require rewriting every stored offset, including ones inside
//! payloads this codec keeps opaque.

mod read;
#[cfg(test)]
mod tests;
mod write;

use std::sync::Arc;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    Codec, Confidence, DecodeCtx, Decoded, Error, ErrorCode, Params, Registry, Result, SniffCtx,
};

/// The container or stream is truncated, malformed or fails LZ4 decoding.
pub const W3_PARSE: ErrorCode = ErrorCode::new("SP-W3-001");
/// The file is not a Witcher 3 save (magic or footer markers are missing).
pub const W3_NOT_SAVE: ErrorCode = ErrorCode::new("SP-W3-002");
/// A value cannot be written back safely (wrong type, span out of bounds, changed string size).
pub const W3_WRITE: ErrorCode = ErrorCode::new("SP-W3-003");

/// Prefix of every hint this codec writes.
pub const HINT_PREFIX: &str = "W3:";

/// Outer container magic.
pub(crate) const MAGIC: &[u8; 8] = b"SNFHFZLC";
/// Start of the decompressed game stream.
pub(crate) const STREAM_MAGIC: &[u8; 4] = b"SAV3";
/// End marker of the decompressed game stream.
pub(crate) const FOOTER_MAGIC: &[u8; 2] = b"SE";

/// Largest chunk count this codec accepts (the game writes far fewer).
const MAX_CHUNKS: usize = 65_536;
/// Deepest record nesting (guards the recursive tree builder before `Limits::max_depth`).
pub(crate) const MAX_NESTING: usize = 1_024;

/// Parameter names used to reproduce the container.
pub(crate) const P_ORIGINAL: &str = "original";
pub(crate) const P_STREAM: &str = "stream";
pub(crate) const P_HEADER_SIZE: &str = "header_size";
pub(crate) const P_CHUNK_SIZES: &str = "chunk_sizes";

/// Registers the Witcher 3 codec as `format.witcher3`.
pub fn register(reg: &mut Registry) {
    reg.add_codec(Arc::new(Witcher3Codec));
}

/// The `format.witcher3` codec.
#[derive(Clone, Copy, Debug, Default)]
pub struct Witcher3Codec;

/// The chunk table looks structurally plausible.
fn looks_like_container(input: &[u8]) -> bool {
    let Some(magic) = input.get(..MAGIC.len()) else {
        return false;
    };
    if magic != MAGIC {
        return false;
    }
    let Some(count) = read_u32(input, 8) else {
        return false;
    };
    let Some(header_size) = read_u32(input, 12) else {
        return false;
    };
    let count = count as usize;
    if count == 0 || count > MAX_CHUNKS {
        return false;
    }
    let Some(table_end) = count.checked_mul(12).and_then(|size| size.checked_add(16)) else {
        return false;
    };
    let header_size = header_size as usize;
    header_size >= table_end && header_size <= input.len()
}

fn read_u32(input: &[u8], pos: usize) -> Option<u32> {
    let bytes = input.get(pos..pos.checked_add(4)?)?;
    Some(u32::from_le_bytes([
        *bytes.first()?,
        *bytes.get(1)?,
        *bytes.get(2)?,
        *bytes.get(3)?,
    ]))
}

impl Codec for Witcher3Codec {
    fn id(&self) -> &'static str {
        "format.witcher3"
    }

    /// The `SNFHFZLC` magic plus a plausible chunk table is strong evidence; the bare magic is a
    /// weak hint so a truncated file still reaches `decode` and reports a clear error.
    fn sniff(&self, input: &[u8], _ctx: &SniffCtx<'_>) -> Confidence {
        if looks_like_container(input) {
            return Confidence::HIGH;
        }
        if input.starts_with(MAGIC) {
            return Confidence::MEDIUM;
        }
        Confidence::NONE
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<Decoded> {
        ctx.limits().check_input(len_u64(input.len()))?;
        let (doc, params) = read::read_all(input, ctx)?;
        ctx.charge_nodes(len_u64(doc.len()))?;
        Ok(Decoded { doc, params })
    }

    fn encode(&self, doc: &saveprism_core::SaveDoc, params: &Params) -> Result<Vec<u8>> {
        write::encode(doc, params)
    }
}

pub(crate) fn parse_error(message: impl Into<String>) -> Error {
    Error::new(W3_PARSE, message)
}

pub(crate) fn not_save(message: impl Into<String>) -> Error {
    Error::new(W3_NOT_SAVE, message)
}
