//! The `encoding.js-binary-string` layer.
//!
//! RPG Maker MZ saves `pako.deflate(json, {to: "string"})`: a JavaScript string in which every
//! character is one byte (0..=255). NW.js writes that string to disk as UTF-8, so a byte above
//! 0x7F takes two bytes in the file. Decoding reads the UTF-8 text and turns each character back
//! into one byte; encoding does the reverse. The mapping is one to one, so it needs no parameters.

use saveprism_core::limits::len_u64;
use saveprism_core::{Confidence, DecodeCtx, Error, Layer, Params, Result, SniffCtx};

use super::{RPG_BINSTR_RANGE, RPG_BINSTR_UTF8};

/// `encoding.js-binary-string`.
#[derive(Clone, Copy, Debug, Default)]
pub struct BinaryStringLayer;

/// True when the two bytes are a valid zlib header (deflate, window <= 32 KiB, check value).
fn zlib_header(cmf: u8, flg: u8) -> bool {
    cmf & 0x0F == 8 && cmf >> 4 <= 7 && (u32::from(cmf) * 256 + u32::from(flg)) % 31 == 0
}

impl Layer for BinaryStringLayer {
    fn id(&self) -> &'static str {
        "encoding.js-binary-string"
    }

    fn sniff(&self, input: &[u8], _ctx: &SniffCtx<'_>) -> Confidence {
        let Ok(text) = std::str::from_utf8(input) else {
            return Confidence::NONE;
        };
        let mut chars = text.chars();
        let first = chars.next().and_then(|c| u8::try_from(u32::from(c)).ok());
        let second = chars.next().and_then(|c| u8::try_from(u32::from(c)).ok());
        let header_ok = matches!((first, second), (Some(cmf), Some(flg)) if cmf == 0x78 && zlib_header(cmf, flg));
        if header_ok && text.chars().all(|c| u32::from(c) <= 0xFF) {
            Confidence::HIGH
        } else {
            Confidence::NONE
        }
    }

    fn decode(&self, input: &[u8], _ctx: &mut DecodeCtx<'_>) -> Result<(Vec<u8>, Params)> {
        let text = std::str::from_utf8(input).map_err(|source| {
            Error::new(
                RPG_BINSTR_UTF8,
                "the file is not valid UTF-8, so it is not a binary string; \
                 it may not be an RPG Maker MZ save",
            )
            .with_offset(len_u64(source.valid_up_to()))
            .with_source(source)
        })?;
        // The output is never longer than the input (a character takes at least one byte).
        let mut out = Vec::with_capacity(input.len());
        for (offset, ch) in text.char_indices() {
            let byte = u8::try_from(u32::from(ch)).map_err(|_| {
                Error::new(
                    RPG_BINSTR_RANGE,
                    format!(
                        "character U+{:04X} is above U+00FF, so the text is not a binary string; \
                         it may not be an RPG Maker MZ save",
                        u32::from(ch)
                    ),
                )
                .with_offset(len_u64(offset))
            })?;
            out.push(byte);
        }
        Ok((out, Params::new()))
    }

    fn encode(&self, inner: &[u8], _params: &Params) -> Result<Vec<u8>> {
        let mut out = Vec::with_capacity(inner.len().saturating_mul(2));
        let mut buffer = [0_u8; 4];
        for &byte in inner {
            out.extend_from_slice(char::from(byte).encode_utf8(&mut buffer).as_bytes());
        }
        Ok(out)
    }
}
