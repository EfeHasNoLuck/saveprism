//! `format.wolf-rpg`: unencrypted WOLF RPG Editor saves (`SaveData/SaveDataNN.sav`).
//!
//! The save layout was read from the public, MIT-licensed `Sinflower/WolfSave` project
//! (<https://github.com/Sinflower/WolfSave>, files `WolfSaveParser.h`, `SavePart1.h` ...
//! `SavePart7.h`, `MemData.h`, `VariableDatabase.h`). Its little-endian, length-prefixed block
//! conventions and version gating were cross-checked against the `SakiMika/wolfrpg-dsi` engine
//! port (<https://github.com/SakiMika/wolfrpg-dsi>; the repository has no license file, so it
//! was read only for behaviour). That port implements the engine's own native save format, not
//! this container, so the save part order itself rests on WolfSave. Everything here is an
//! independent implementation; no code, tables or regexes were copied.
//!
//! # Tree
//!
//! The file is a 0x14 byte header, the start byte `0x19`, a length-prefixed game name
//! (`u16` length), a `u16` file version, SavePart1 through SavePart5, the variable database,
//! SavePart7 and a final `0x19`. The root is an `Object` (hint `WOLF:save`):
//!
//! | WOLF data | Node | Hint | Editable |
//! |---|---|---|---|
//! | Game name (length-prefixed string) | `Str` (or read-only `Bytes` when not UTF-8) | `WOLF:name:u16[:nul]` | yes |
//! | File version | `Int` (U16) | `WOLF:version` | no |
//! | Normal switches (`SavePart1`) | `Array` of `Int` (U8 before 0x64, U32 after) | `WOLF:switches` | values yes |
//! | Normal variables (`SavePart1`) | `Array` of `Int` (I32) | `WOLF:variables` | values yes |
//! | Variable DB (`VariableDatabase`) | `Object` of `Type N` `Object`s of `Record N` `Object`s of `Field N` values | `WOLF:variable-db`, `WOLF:type`, `WOLF:record`, `WOLF:field` | scalar fields yes |
//!
//! Names are not stored in a WOLF save, so values are shown as `Switch N`, `Variable N`,
//! `Type N` / `Record N` / `Field N`. Containers are read-only: no insert, remove or rename.
//! A variable-database field is a number (`Int`, I32) or a length-prefixed string (`Str`).
//!
//! All other bytes (map, party, event and system state) are kept verbatim and are not exposed:
//! writing splices the edited scalars back into the original bytes, so every unexposed byte is
//! reproduced exactly.
//!
//! # Exact writing
//!
//! The decoder records the original bytes and a `ByteSpan` on every exposed scalar; the writer
//! copies the original and replaces only the spans whose node value changed. String lengths are
//! recomputed, and the header checksum byte at offset 2 (sum of every byte from 0x14 to the end)
//! is recomputed when anything changed. An unchanged document therefore encodes to the original
//! bytes, including parts this codec does not understand.
//!
//! # Encryption
//!
//! Only saves whose byte at 0x14 is `0x19` are read. A save that does not have it (for example
//! a Pro-protected or otherwise encrypted one, or one with a non-zero encryption header) fails
//! with [`WOLF_ENCRYPTED`] and is never decrypted or rewritten.

mod read;
#[cfg(test)]
mod tests;
mod write;

use std::sync::Arc;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    Codec, Confidence, DecodeCtx, Decoded, Error, ErrorCode, Params, Registry, Result, SniffCtx,
};

/// The file is truncated or does not match the WOLF save layout.
pub const WOLF_PARSE: ErrorCode = ErrorCode::new("SP-WOLF-001");
/// The save is encrypted (or Pro-protected); encrypted saves are not supported.
pub const WOLF_ENCRYPTED: ErrorCode = ErrorCode::new("SP-WOLF-002");
/// A value cannot be written back in place (wrong type, bad range, span out of bounds).
pub const WOLF_WRITE: ErrorCode = ErrorCode::new("SP-WOLF-003");

/// Prefix of every hint this codec writes.
pub const HINT_PREFIX: &str = "WOLF:";

/// Offset of the header length and of the start byte.
pub(crate) const HEADER_LEN: usize = 0x14;
/// Byte that starts and ends a save body.
pub(crate) const MARKER: u8 = 0x19;
/// Highest file version whose part layout this module knows (3.279).
pub(crate) const MAX_VERSION: u16 = 0x8E;
/// Byte at offset 6 that marks UTF-8 text (otherwise the text is Shift-JIS).
pub(crate) const UTF8_MARKER: u8 = 0x55;

/// Parameter that carries the original bytes for the lossless writer.
pub(crate) const P_ORIGINAL: &str = "original";

/// Registers the WOLF RPG Editor codec as `format.wolf-rpg`.
pub fn register(reg: &mut Registry) {
    reg.add_codec(Arc::new(WolfRpgCodec));
}

/// The `format.wolf-rpg` codec.
#[derive(Clone, Copy, Debug, Default)]
pub struct WolfRpgCodec;

/// True when `name` looks like a WOLF save slot (`SaveData01.sav`, `SaveData.sav`, ...).
fn is_wolf_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let Some(stem) = lower.strip_suffix(".sav") else {
        return false;
    };
    let base = stem.rsplit(['/', '\\']).next().unwrap_or(stem);
    base == "savedata"
        || (base.starts_with("savedata") && base[8..].bytes().all(|b| b.is_ascii_digit()))
}

/// True when the bytes after 0x14 look like the start of a plain WOLF save body.
fn body_magic(input: &[u8]) -> bool {
    input.get(HEADER_LEN).copied() == Some(MARKER)
}

/// A weak header check used by `sniff`: name, version and end byte are plausible.
fn header_plausible(input: &[u8]) -> bool {
    if !body_magic(input) || input.last().copied() != Some(MARKER) {
        return false;
    }
    let mut pos = HEADER_LEN + 1;
    let Some(length) = read_u16_at(input, pos) else {
        return false;
    };
    pos += 2;
    if length as usize > 1024 || pos + length as usize + 2 > input.len() {
        return false;
    }
    pos += length as usize;
    let Some(version) = read_u16_at(input, pos) else {
        return false;
    };
    version != 0 && version <= MAX_VERSION
}

fn read_u16_at(input: &[u8], pos: usize) -> Option<u16> {
    let bytes = input.get(pos..pos.checked_add(2)?)?;
    Some(u16::from_le_bytes([*bytes.first()?, *bytes.get(1)?]))
}

impl Codec for WolfRpgCodec {
    fn id(&self) -> &'static str {
        "format.wolf-rpg"
    }

    /// `0x19` at offset 0x14, a plausible length-prefixed name/version and a final `0x19`.
    /// A `SaveData*.sav` name raises the score; a `.sav` with a Pro `P` header gets a weak
    /// score so `decode` can report the clear encryption error.
    fn sniff(&self, input: &[u8], ctx: &SniffCtx<'_>) -> Confidence {
        let named = ctx.file_name.is_some_and(is_wolf_name);
        if header_plausible(input) {
            return if named {
                Confidence::new(0.97)
            } else {
                Confidence::HIGH
            };
        }
        let pro = named && input.get(1).copied() == Some(b'P') && input.len() > HEADER_LEN;
        if pro {
            Confidence::LOW
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

    fn encode(&self, doc: &saveprism_core::SaveDoc, params: &Params) -> Result<Vec<u8>> {
        write::encode(doc, params)
    }
}

pub(crate) fn parse_error(message: impl Into<String>) -> Error {
    Error::new(WOLF_PARSE, message)
}
