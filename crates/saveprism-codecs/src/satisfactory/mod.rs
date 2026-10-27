//! `format.satisfactory`: Satisfactory (Coffee Stain Studios) `.sav` saves, header version 13
//! and 14 / save version 45-52.
//!
//! The layout was confirmed against public sources, read for behaviour only; no code, tables or
//! regexes were copied:
//!
//! - The official Satisfactory Wiki page "Save files" (CC BY-SA 4.0, prose specification):
//!   <https://satisfactory.wiki.gg/wiki/Save_files> — the basic types (`Byte`, `Int`, `Uint32`,
//!   `Long`, `Uint64`, `Float`, `Double`, FString with positive/negative length), the
//!   `SaveFileHeader` field order and version gates, the `MD5Hash` block, the zlib chunk framing,
//!   the `SaveFileBody`, the level/TOC/data layout, object headers, Unreal tagged property tags
//!   and the special typed structs (`InventoryItem`, `Vector`, `LinearColor`, ...).
//! - `etothepii4/satisfactory-file-parser` (MIT): <https://github.com/etothepii4/satisfactory-file-parser>
//!   — the exact 1.x body ordering (body size + UE5 padding, `SaveBodyValidation` grids,
//!   level count, TOC/data blobs with their 64-bit length prefix, per-object save custom version
//!   and object version data), the old and "complete type name" property tags, the array of
//!   structs inner tag before save version 53, and the dynamic struct (`InventoryStack`) and
//!   `FInventoryItem`/`FGDynamicStruct` layouts.
//!
//! # Tree
//!
//! The root is an `Object` (hint `SAT:save`) with two children:
//!
//! - `Header`: every header field, in file order. Numeric fields are editable in place; strings,
//!   the checksum and the mod metadata stay read-only because a changed length or stale MD5 could
//!   not be written back safely.
//! - `Body`: `Grids` (level-grouping grids) and `Levels`. Every level is an `Object` with its
//!   name, collectable count and `Classes`; `Classes` groups the level's objects by class so a
//!   world with tens of thousands of objects stays navigable. Each object holds its header
//!   strings and a `Properties` object.
//!
//! Unreal tagged properties keep their name and type in a hint (`SAT:IntProperty`, ...). Scalar
//! payloads (`Int8`, `UInt8`, `Int`, `UInt32`, `Int64`, `UInt64`, `Float`, `Double`, `Bool`,
//! and a `ByteProperty` whose enum type is `None`) are editable leaves with an exact span.
//! `InventoryStack` arrays are opened, so the item count (`NumItems`) of every stack is an
//! editable `IntProperty`. Strings, enums, object references, maps, sets, text, binary structs and
//! arrays of unknown structs stay read-only: changing them would change a length or needs the
//! game schema. Any property whose payload does not parse exactly is kept as read-only bytes by
//! its tag span, so one unusual property never hides the rest of the object.
//!
//! # Exact writing
//!
//! Decode keeps the original file, the decompressed body and the original uncompressed size of
//! every chunk. Editable spans index a combined image (file header followed by the decompressed
//! body). If no leaf changed, `encode` returns the original bytes exactly. Otherwise it patches the
//! image in place and re-compresses the body in chunks of the original uncompressed sizes; an
//! edited file therefore round-trips at the semantic level (zlib output may differ), while an
//! unchanged file is byte-identical.

use std::sync::Arc;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    Codec, Confidence, DecodeCtx, Decoded, Error, ErrorCode, Params, Registry, Result, SniffCtx,
};

mod read;
#[cfg(test)]
mod tests;
mod write;

/// The container, header, body or a chunk is malformed.
pub const SAT_PARSE: ErrorCode = ErrorCode::new("SP-SAT-001");
/// The file is not a Satisfactory save.
pub const SAT_NOT_SAVE: ErrorCode = ErrorCode::new("SP-SAT-002");
/// A value cannot be written back safely.
pub const SAT_WRITE: ErrorCode = ErrorCode::new("SP-SAT-003");

/// Prefix of every hint this codec writes.
pub const HINT_PREFIX: &str = "SAT:";

/// Unreal package tag that starts every compressed chunk.
pub(crate) const CHUNK_TAG: u32 = 0x9E2A_83C1;
/// Second word of a version 2 chunk header, and the header size selector.
pub(crate) const CHUNK_HEADER_V2: u32 = 0x2222_2222;
/// Size of a version 1 / version 2 chunk header.
pub(crate) const CHUNK_HEADER_V1_SIZE: usize = 48;
pub(crate) const CHUNK_HEADER_V2_SIZE: usize = 49;
/// Largest accepted chunk count (a world body has a few hundred).
const MAX_CHUNKS: usize = 65_536;
/// Deepest property nesting accepted (guards the recursive reader before `Limits::max_depth`).
pub(crate) const MAX_NESTING: usize = 64;
/// Largest struct array opened as a tree; bigger arrays stay raw read-only.
pub(crate) const MAX_STRUCT_ELEMENTS: usize = 4096;

/// Parameter names used to reproduce the container.
pub(crate) const P_ORIGINAL: &str = "original";
pub(crate) const P_IMAGE: &str = "image";
pub(crate) const P_HEADER_LEN: &str = "header_len";
pub(crate) const P_CHUNK_SIZES: &str = "chunk_sizes";
pub(crate) const P_CHUNK_VERSION: &str = "chunk_version";
pub(crate) const P_MAX_CHUNK: &str = "max_chunk";
pub(crate) const P_PACKAGE_TAG: &str = "package_tag";
pub(crate) const P_COMPRESSION: &str = "compression";

/// Registers the Satisfactory codec as `format.satisfactory`.
pub fn register(reg: &mut Registry) {
    reg.add_codec(Arc::new(SatisfactoryCodec));
}

/// The `format.satisfactory` codec.
#[derive(Clone, Copy, Debug, Default)]
pub struct SatisfactoryCodec;

fn u32_at(input: &[u8], pos: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        input.get(pos..pos.checked_add(4)?)?.try_into().ok()?,
    ))
}

/// True when the header versions and the first chunk header are plausible.
fn looks_like_save(input: &[u8]) -> bool {
    let (Some(header), Some(save), Some(build)) =
        (u32_at(input, 0), u32_at(input, 4), u32_at(input, 8))
    else {
        return false;
    };
    if !(13..=14).contains(&header) || !(20..=200).contains(&save) {
        return false;
    }
    if build == 0 || build > 100_000_000 {
        return false;
    }
    // Walk the version-gated header strings so the chunk tag is checked at the real offset.
    let Ok(header_end) = read::header_end(input, header) else {
        return false;
    };
    u32_at(input, header_end) == Some(CHUNK_TAG)
}

impl Codec for SatisfactoryCodec {
    fn id(&self) -> &'static str {
        "format.satisfactory"
    }

    /// Version 13/14 plus a plausible save/build version and the first `9E2A83C1` chunk tag is
    /// strong evidence. GVAS (`GVAS`) and every other codec's magic cannot match this.
    fn sniff(&self, input: &[u8], _ctx: &SniffCtx<'_>) -> Confidence {
        if looks_like_save(input) {
            Confidence::HIGH
        } else {
            Confidence::NONE
        }
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
    Error::new(SAT_PARSE, message)
}

pub(crate) fn not_save(message: impl Into<String>) -> Error {
    Error::new(SAT_NOT_SAVE, message)
}
