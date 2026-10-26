//! `format.onscripter`: NScripter / ONScripter saves (`save1.dat` ... and `global.sav`).
//!
//! Written from a behavioral specification read from the public ONScripter sources
//! (<https://github.com/ogapee/onscripter>, GPL-2.0): `ONScripter_file.cpp`,
//! `ONScripter_file2.cpp`, `ScriptParser.cpp` and `ScriptParser.h`. ONScripter is GPL, so only
//! the behavior (field order, sizes and the text encoding) was read; no code, table or constant
//! was copied and this module is an independent implementation.
//!
//! # Layout
//!
//! `saveN.dat` starts with the ASCII marker `ONS`, then the save version as two bytes (major,
//! minor). Version 2.x stores an `int32` marker and thousands of screen-state fields before the
//! variable block. The file ends with the save description as `"text"*`.
//!
//! Variables are written in index order, each as a little-endian `int32` number followed by a
//! NUL-terminated Shift-JIS string. A save slot stores variables `0` up to the game script's
//! `global_variable_border`; the global file (`global.sav`, also spelled `gloval.sav`) stores the
//! global range only and has no header at all.
//!
//! The variable count for a slot comes from the game script, not from the file. Without the
//! script there is no honest way to find where the variable block ends, so slot bodies are kept
//! as read-only bytes and only their version and trailing description are exposed. The global
//! file is self-delimiting: it is a plain sequence of number/string pairs up to EOF, so all of its
//! variables are editable.
//!
//! # Tree
//!
//! A global file is an `Object` with hint `ONS:global` and a read-only `variables` array. Every
//! `ONS:variable` object has a read-only `index` (the NScripter global range starts at 200 by
//! default), an editable `number` (`int32`) and an editable `string` (Shift-JIS). A slot is an
//! `Object` `ONS:save` with `magic`, `version_major`, `version_minor`, an optional editable
//! `description`, and the read-only `body`.
//!
//! # Exact writing
//!
//! The original bytes are kept as the codec parameter `original`; every editable scalar records
//! its byte span. Writing splices only the edited numbers and Shift-JIS strings back, so comments
//! and every byte this codec does not expose stay identical. A character the Shift-JIS encoder
//! cannot represent is refused.

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

/// The file is truncated or a field does not parse.
pub const ONS_PARSE: ErrorCode = ErrorCode::new("SP-ONS-001");
/// A value cannot be written back (a span is outside the file, Shift-JIS cannot represent text).
pub const ONS_WRITE: ErrorCode = ErrorCode::new("SP-ONS-002");

/// Prefix of every hint this codec writes.
pub const HINT_PREFIX: &str = "ONS:";
/// The magic at the start of a save slot.
pub const SAVE_MAGIC: &[u8] = b"ONS";
/// The first variable index of the default NScripter global range.
pub const GLOBAL_BASE: i32 = 200;
/// Parameter that carries the original bytes for the lossless writer.
pub const P_ORIGINAL: &str = "original";

/// Registers the NScripter / ONScripter codec as `format.onscripter`.
pub fn register(reg: &mut Registry) {
    reg.add_codec(Arc::new(OnscripterCodec));
}

/// The `format.onscripter` codec.
#[derive(Clone, Copy, Debug, Default)]
pub struct OnscripterCodec;

fn is_global_name(name: &str) -> bool {
    let base = name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(name)
        .to_ascii_lowercase();
    base == "global.sav" || base == "gloval.sav"
}

fn is_slot_name(name: &str) -> bool {
    let base = name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(name)
        .to_ascii_lowercase();
    let Some(stem) = base.strip_suffix(".dat") else {
        return false;
    };
    let Some(rest) = stem.strip_prefix("save") else {
        return false;
    };
    !rest.is_empty() && rest.bytes().all(|byte| byte.is_ascii_digit())
}

fn looks_like_foreign_magic(input: &[u8]) -> bool {
    input.starts_with(b"QSPSAVEDGAME")
        || input.starts_with(SAVE_MAGIC)
        || input.starts_with(b"GVA")
        || input.starts_with(&[0x89, 0x41, 0x52, 0x45, 0x0A, 0x1A, 0x0A, 0x0D])
}

impl Codec for OnscripterCodec {
    fn id(&self) -> &'static str {
        "format.onscripter"
    }

    fn sniff(&self, input: &[u8], ctx: &SniffCtx<'_>) -> Confidence {
        let named_slot = ctx.file_name.is_some_and(is_slot_name);
        if input.starts_with(SAVE_MAGIC) && input.len() >= 5 {
            return if named_slot {
                Confidence::new(0.98)
            } else {
                Confidence::new(0.88)
            };
        }
        if ctx.file_name.is_some_and(is_global_name)
            && !looks_like_foreign_magic(input)
            && read::global_plausible(input)
        {
            Confidence::new(0.82)
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
    Error::new(ONS_PARSE, message)
}
