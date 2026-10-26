//! `format.emuera`: Emuera / eraMaker binary saves (`save00.sav`, `global.sav`).
//!
//! Written from a behavioral specification read from the public sources of the zlib-licensed
//! Emuera fork (<https://github.com/0x00000FF/Emuera>): `Sub/EraBinaryDataReader.cs`,
//! `Sub/EraBinaryDataWriter.cs`, `GameData/Variable/VariableData.cs`,
//! `GameData/Variable/CharacterData.cs` and `GameData/Variable/VariableEvaluator.cs`. The Emuera
//! sources are used for behavior only; no code or tables were copied. Nothing in a save is
//! executed.
//!
//! # Layout
//!
//! A binary save starts with an eight-byte header (`89 45 52 41 0D 0A 1A 0A`), the `uint32`
//! format version `1808`, and a `uint32` count of reserved `uint32` values (always zero in
//! current Emuera). A `byte` file type follows: `0` normal slot, `1` global, `2` saved variables,
//! `3` saved character variables.
//!
//! Then come two `int64` values (the game's unique code and script version) and a .NET string
//! (7-bit encoded byte length, then UTF-16LE bytes). A normal or character-variable file stores
//! the character count as `int64`, then that many character records; normal, global and saved
//! variable files end with global variables, and every file ends with the `0xFF` marker.
//!
//! A variable record is a type byte, a .NET string key, and a value:
//!
//! | Type | Value |
//! |---|---|
//! | `0x00` | compressed integer (`0..207` as one byte, then `0xD0`+i16, `0xD1`+i32, `0xD2`+i64) |
//! | `0x01` `0x02` `0x03` | one-, two-, three-dimensional integer array |
//! | `0x10` | .NET UTF-16 string |
//! | `0x11` `0x12` `0x13` | one-, two-, three-dimensional string array |
//! | `0xFD` | separator (start of user-defined character variables) |
//! | `0xFE` | end of a character record |
//! | `0xFF` | end of the variable list / file |
//!
//! Integer arrays start with an `int32` length (two or three lengths for higher dimensions) and
//! use `0xF0` + count for zero runs, `0xE0`/`0xE1` for row/plane ends, `0xF1`/`0xF2` for empty
//! rows/planes, and `0xFF` as the final marker. String arrays use the same zero-run compression
//! and prefix each value with `0xD8`.
//!
//! # Tree
//!
//! The root is a read-only `Object` `EMUERA:save` with `header`, `file_type`, `game_code`,
//! `game_version`, an editable `description`, optional `characters` and an optional `variables`
//! object. Variable keys are the names Emuera stores (`FLAG`, `TFLAG`, `BASE`, `ABL`, `TALENT`,
//! `EXP`, `MARK`, `CFLAG`, `NAME`, `CSTR`, ...). Integers are `Int` (I64), strings are `Str`,
//! arrays are read-only `Array` containers whose elements are editable. A character record has a
//! read-only `values` object and, when present, a read-only `user_defined` object after the
//! separator. Containers are read-only: no insert, remove or rename.
//!
//! # Exact writing
//!
//! The writer rebuilds the whole file in the same order and with Emuera's own integer and
//! zero-run encoding, so an unchanged file written by Emuera reproduces its bytes. A file that
//! uses a non-canonical encoding fails the round-trip guard and opens read-only.
//!
//! The older text save format is not claimed by this codec: its fixed positional part stores no
//! variable names or lengths and can only be split with the game's own variable tables. Emuera
//! writes the binary form when its `SystemSaveInBinary` option is on (the default is the text
//! format), and the header makes the two forms easy to tell apart.

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

/// The file is not a binary Emuera save or a field does not parse.
pub const EMUERA_PARSE: ErrorCode = ErrorCode::new("SP-EMUERA-001");
/// The save uses a format version this codec does not know, or the text save format.
pub const EMUERA_UNSUPPORTED: ErrorCode = ErrorCode::new("SP-EMUERA-002");
/// A value cannot be written back (wrong type, array shape mismatch, string too long).
pub const EMUERA_WRITE: ErrorCode = ErrorCode::new("SP-EMUERA-003");

/// Prefix of every hint this codec writes.
pub const HINT_PREFIX: &str = "EMUERA:";
/// Binary header of an Emuera 1.808+ save.
pub const HEADER: [u8; 8] = [0x89, 0x45, 0x52, 0x41, 0x0D, 0x0A, 0x1A, 0x0A];
/// The only binary save version this codec knows.
pub const VERSION: u32 = 1808;
/// Largest number of elements this codec allocates for one array.
pub const MAX_ARRAY_ELEMENTS: usize = 10_000_000;

/// Registers the Emuera codec as `format.emuera`.
pub fn register(reg: &mut Registry) {
    reg.add_codec(Arc::new(EmueraCodec));
}

/// The `format.emuera` codec.
#[derive(Clone, Copy, Debug, Default)]
pub struct EmueraCodec;

impl Codec for EmueraCodec {
    fn id(&self) -> &'static str {
        "format.emuera"
    }

    fn sniff(&self, input: &[u8], _ctx: &SniffCtx<'_>) -> Confidence {
        if input.starts_with(&HEADER) {
            Confidence::new(0.99)
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
    Error::new(EMUERA_PARSE, message)
}
