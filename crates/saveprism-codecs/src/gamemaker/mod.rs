//! `format.gamemaker-ds-map`: the hexadecimal text `ds_map_write` returns in GameMaker Studio 1/2.
//!
//! The layout follows the public reverse-engineered format note
//! (`github.com/kikones34/dsmap-serial-format`, `README.md`; the repository has no license file, so
//! only the byte layout was read and everything here is an independent implementation): the text is
//! two hexadecimal digits per byte; the bytes are a little-endian `u32` magic `402`, a little-endian
//! `u32` entry count, then key/value pairs. Each object is a little-endian `u32` type (`0` real,
//! `1` string); a real is a little-endian IEEE-754 `f64`, a string is a little-endian `u32` byte
//! length followed by the bytes.
//!
//! GameMaker's manual says that `ds_map_read` changed the format after Studio 1.2 and again in 2.3.1
//! and that arrays are also stored; no public description of those variants or of the `ds_list`
//! layout was found, so only magic `402` with real/string objects is opened.
//!
//! # Tree
//!
//! The root is a read-only `Map` (hint `GM:map`) so a numeric key stays a number. Keys are
//! read-only nodes; values are editable.
//!
//! | ds_map | Node | Hint | Editable |
//! |---|---|---|---|
//! | map | `Map` | `GM:map` | no |
//! | key: real | `Float` (F64) | `GM:key:real` | no |
//! | key: string | `Str` | `GM:key:string` | no |
//! | value: real | `Float` (F64) | `GM:real` | yes |
//! | value: string | `Str`, or read-only `Bytes` when not UTF-8 | `GM:string` | text yes |
//!
//! # Exact writing
//!
//! The original text is the codec parameter `original` (with any trailing line ending kept in
//! `trailing`). An unchanged map is written back as the original text, including a lowercase hex
//! dump; an edit writes the canonical uppercase dump. Entry order and types are preserved, so an
//! edit never changes the shape.

mod read;
#[cfg(test)]
mod tests;
mod write;

use std::sync::Arc;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    Codec, Confidence, DecodeCtx, Decoded, Error, ErrorCode, Params, Registry, Result, SniffCtx,
};

/// The text is not a GameMaker `ds_map` dump.
pub const GM_PARSE: ErrorCode = ErrorCode::new("SP-GMD-001");
/// The document cannot be written as a `ds_map` dump.
pub const GM_WRITE: ErrorCode = ErrorCode::new("SP-GMD-002");

/// Prefix of every hint this codec writes.
pub const PREFIX: &str = "GM:";
/// The magic number at the start of the serialized map.
const MAGIC: u32 = 402;

/// Registers `format.gamemaker-ds-map`.
pub fn register(reg: &mut Registry) {
    reg.add_codec(Arc::new(GameMakerDsMapCodec));
}

/// The `format.gamemaker-ds-map` codec.
#[derive(Clone, Copy, Debug, Default)]
pub struct GameMakerDsMapCodec;

/// The text without trailing ASCII whitespace, plus the bytes that were stripped.
fn split_trailing(input: &[u8]) -> (&[u8], &[u8]) {
    let mut end = input.len();
    while end > 0
        && input
            .get(end - 1)
            .is_some_and(|byte| byte.is_ascii_whitespace())
    {
        end -= 1;
    }
    (
        input.get(..end).unwrap_or_default(),
        input.get(end..).unwrap_or_default(),
    )
}

/// True when the whole text is an even run of hexadecimal digits.
fn is_hex_text(text: &[u8]) -> bool {
    !text.is_empty() && text.len().is_multiple_of(2) && text.iter().all(u8::is_ascii_hexdigit)
}

/// Decodes the first eight hex digits as the magic number.
fn starts_with_magic(text: &[u8]) -> bool {
    let Some(header) = text.get(..8) else {
        return false;
    };
    let Ok(high) = std::str::from_utf8(header) else {
        return false;
    };
    let bytes = read::hex_bytes(high).ok();
    bytes.is_some_and(|bytes| {
        bytes
            .get(..4)
            .and_then(|four| <[u8; 4]>::try_from(four).ok())
            .is_some_and(|four| u32::from_le_bytes(four) == MAGIC)
    })
}

impl Codec for GameMakerDsMapCodec {
    fn id(&self) -> &'static str {
        "format.gamemaker-ds-map"
    }

    fn sniff(&self, input: &[u8], _ctx: &SniffCtx<'_>) -> Confidence {
        let (text, _) = split_trailing(input);
        if is_hex_text(text) && starts_with_magic(text) {
            Confidence::new(0.88)
        } else {
            Confidence::NONE
        }
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<Decoded> {
        ctx.limits().check_input(len_u64(input.len()))?;
        let (text, trailing) = split_trailing(input);
        if !is_hex_text(text) {
            return Err(Error::new(
                GM_PARSE,
                "the GameMaker ds_map dump is not an even run of hex digits",
            ));
        }
        let Ok(ascii) = std::str::from_utf8(text) else {
            return Err(Error::new(
                GM_PARSE,
                "the GameMaker ds_map dump is not ASCII",
            ));
        };
        let bytes = read::hex_bytes(ascii)?;
        let mut doc = ctx.new_doc();
        let root = read::parse(&bytes, &mut doc, ctx.limits().max_nodes)?;
        doc.set_root(root);
        ctx.charge_nodes(len_u64(doc.len()))?;
        let mut params = Params::new();
        params.set("original", input.to_vec());
        if !trailing.is_empty() {
            params.set("trailing", trailing.to_vec());
        }
        Ok(Decoded { doc, params })
    }

    fn encode(&self, doc: &saveprism_core::SaveDoc, params: &Params) -> Result<Vec<u8>> {
        let original = params
            .opt_bytes("original")?
            .ok_or_else(|| Error::new(GM_WRITE, "original GameMaker ds_map text is missing"))?;
        let trailing = params.opt_bytes("trailing")?.unwrap_or_default();
        let canonical = write::render(doc)?;
        let (original_text, _) = split_trailing(original);
        if canonical.eq_ignore_ascii_case(original_text) {
            return Ok(original.to_vec());
        }
        let mut out = canonical;
        out.extend_from_slice(trailing);
        Ok(out)
    }
}
