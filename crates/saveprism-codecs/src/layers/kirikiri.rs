//! The KiriKiri / KAG text-stream wrapper (`obfuscation.kirikiri`).
//!
//! Written from the public KiriKiri Z source (`github.com/krkrz/krkrz`, `base/TextStream.cpp`;
//! BSD-style license). Only the byte layout and the two invertible character transforms were read;
//! nothing is executed. The stream starts with two `0xFE` bytes, one mode byte and the text BOM
//! `FF FE`:
//!
//! - mode 0: every UTF-16LE code unit `>= 0x20` is XORed with `((unit & 0xFE) << 8) ^ 1`;
//! - mode 1: every UTF-16LE code unit has its adjacent bits swapped
//!   (`(unit & 0xAAAA) >> 1 | (unit & 0x5555) << 1`);
//! - mode 2: mode byte, BOM, then a little-endian `u64` compressed length, a little-endian `u64`
//!   uncompressed length and a zlib stream of the UTF-16LE text (no BOM).
//!
//! The inner bytes are the BOM followed by the UTF-16LE text, which is what `format.tjs2` reads.
//! An unchanged file is written back with its original bytes (mode 2 compression is recorded as
//! the source); an edited file is scrambled or compressed again with the recorded mode.

use std::io::Write;

use flate2::Compression;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use saveprism_core::limits::len_u64;
use saveprism_core::{Confidence, DecodeCtx, Error, ErrorCode, Layer, Params, Result, SniffCtx};

use super::{keep_source, param_u8, source_if_unchanged};

/// The input is not a KiriKiri text stream.
pub const KIRIKIRI_INVALID: ErrorCode = ErrorCode::new("SP-KIRI-001");
/// The stream cannot be written back (missing BOM, bad mode, compression failed).
pub const KIRIKIRI_WRITE: ErrorCode = ErrorCode::new("SP-KIRI-002");

const SIGNATURE: [u8; 2] = [0xFE, 0xFE];
const BOM: [u8; 2] = [0xFF, 0xFE];
const HEADER_LEN: usize = 5;
const LENGTHS_LEN: usize = 16;

/// `obfuscation.kirikiri`.
///
/// Parameters: `mode` (0, 1 or 2), `level` (0..=9, used when an edited mode-2 stream is written)
/// and the source digest set by `decode`.
#[derive(Clone, Copy, Debug, Default)]
pub struct KirikiriLayer;

fn bad(message: impl Into<String>, offset: usize) -> Error {
    Error::new(KIRIKIRI_INVALID, message).with_offset(len_u64(offset))
}

/// Applies the mode-0 or mode-1 transform to one UTF-16LE code unit (both are involutions).
fn transform(mode: u8, unit: u16) -> u16 {
    match mode {
        0 if unit >= 0x20 => unit ^ ((unit & 0x00FE).wrapping_shl(8) ^ 1),
        1 => (unit & 0xAAAA) >> 1 | (unit & 0x5555) << 1,
        _ => unit,
    }
}

fn read_u64(input: &[u8], at: usize) -> Option<u64> {
    let bytes = input.get(at..at.checked_add(8)?)?;
    let array: [u8; 8] = bytes.try_into().ok()?;
    Some(u64::from_le_bytes(array))
}

fn read_bom(input: &[u8]) -> Option<&[u8]> {
    input.strip_prefix(&BOM)
}

impl Layer for KirikiriLayer {
    fn id(&self) -> &'static str {
        "obfuscation.kirikiri"
    }

    fn sniff(&self, input: &[u8], _ctx: &SniffCtx<'_>) -> Confidence {
        if input.starts_with(&SIGNATURE)
            && input.get(2).is_some_and(|mode| *mode <= 2)
            && input.get(3..HEADER_LEN).is_some_and(|bom| bom == BOM)
        {
            Confidence::new(0.93)
        } else {
            Confidence::NONE
        }
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<(Vec<u8>, Params)> {
        let mode = input
            .get(2)
            .copied()
            .ok_or_else(|| bad("the KiriKiri stream header is cut short", 0))?;
        if mode > 2 {
            return Err(bad(format!("unsupported KiriKiri text mode {mode}"), 2));
        }
        if input.get(0..2) != Some(&SIGNATURE) {
            return Err(bad("missing KiriKiri text signature FE FE", 0));
        }
        if input.get(3..HEADER_LEN) != Some(&BOM) {
            return Err(bad("missing the KiriKiri UTF-16 BOM after the mode", 3));
        }
        let payload = input.get(HEADER_LEN..).unwrap_or_default();
        let body = match mode {
            2 => {
                let compressed = read_u64(input, HEADER_LEN)
                    .ok_or_else(|| bad("the KiriKiri compressed length is missing", 5))?;
                let uncompressed = read_u64(input, HEADER_LEN + 8)
                    .ok_or_else(|| bad("the KiriKiri uncompressed length is missing", 13))?;
                let data_start = HEADER_LEN + LENGTHS_LEN;
                let Ok(compressed) = usize::try_from(compressed) else {
                    return Err(bad("the KiriKiri compressed length is too large", 5));
                };
                let Ok(uncompressed) = usize::try_from(uncompressed) else {
                    return Err(bad("the KiriKiri uncompressed length is too large", 13));
                };
                let data = input
                    .get(data_start..)
                    .ok_or_else(|| bad("the KiriKiri compressed data is missing", data_start))?;
                if data.len() != compressed {
                    return Err(bad(
                        "the KiriKiri compressed length does not match the file",
                        data_start,
                    ));
                }
                let mut decoder = ZlibDecoder::new(data);
                let text =
                    ctx.read_bounded(&mut decoder, len_u64(input.len()), KIRIKIRI_INVALID)?;
                if text.len() != uncompressed {
                    return Err(bad(
                        "the KiriKiri uncompressed length does not match the data",
                        data_start,
                    ));
                }
                text
            }
            _ => {
                if !payload.len().is_multiple_of(2) {
                    return Err(bad(
                        "the KiriKiri UTF-16 text has an odd length",
                        HEADER_LEN,
                    ));
                }
                let mut text = Vec::with_capacity(payload.len());
                for pair in payload.as_chunks::<2>().0 {
                    let unit = u16::from_le_bytes(*pair);
                    text.extend_from_slice(&transform(mode, unit).to_le_bytes());
                }
                ctx.check_output(len_u64(input.len()), len_u64(text.len()))?;
                text
            }
        };
        let mut inner = Vec::with_capacity(body.len().saturating_add(2));
        inner.extend_from_slice(&BOM);
        inner.extend_from_slice(&body);
        let mut params = Params::new();
        params.set("mode", u32::from(mode));
        params.set("level", 6_u32);
        keep_source(&mut params, input, &inner);
        Ok((inner, params))
    }

    fn encode(&self, inner: &[u8], params: &Params) -> Result<Vec<u8>> {
        if let Some(source) = source_if_unchanged(inner, params)? {
            return Ok(source);
        }
        let mode = param_u8(params, "mode", 1)?;
        if mode > 2 {
            return Err(Error::new(
                KIRIKIRI_WRITE,
                format!("unsupported KiriKiri text mode {mode}"),
            ));
        }
        let body = read_bom(inner).ok_or_else(|| {
            Error::new(
                KIRIKIRI_WRITE,
                "the inner KiriKiri text does not start with a UTF-16LE BOM",
            )
        })?;
        let mut out = Vec::with_capacity(inner.len().saturating_add(HEADER_LEN));
        out.extend_from_slice(&SIGNATURE);
        out.push(mode);
        out.extend_from_slice(&BOM);
        match mode {
            2 => {
                let level = param_u8(params, "level", 6)?;
                if level > 9 {
                    return Err(Error::new(
                        KIRIKIRI_WRITE,
                        "the KiriKiri compression level must be 0..=9",
                    ));
                }
                let mut encoder = ZlibEncoder::new(Vec::new(), Compression::new(u32::from(level)));
                encoder.write_all(body).map_err(|source| {
                    Error::new(KIRIKIRI_WRITE, "KiriKiri zlib compression failed")
                        .with_source(source)
                })?;
                let compressed = encoder.finish().map_err(|source| {
                    Error::new(KIRIKIRI_WRITE, "KiriKiri zlib compression failed")
                        .with_source(source)
                })?;
                out.extend_from_slice(&len_u64(compressed.len()).to_le_bytes());
                out.extend_from_slice(&len_u64(body.len()).to_le_bytes());
                out.extend_from_slice(&compressed);
            }
            _ => {
                if !body.len().is_multiple_of(2) {
                    return Err(Error::new(
                        KIRIKIRI_WRITE,
                        "the inner KiriKiri UTF-16 text has an odd length",
                    ));
                }
                for pair in body.as_chunks::<2>().0 {
                    let unit = u16::from_le_bytes(*pair);
                    out.extend_from_slice(&transform(mode, unit).to_le_bytes());
                }
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used, clippy::indexing_slicing)]

    use saveprism_core::{DecodeCtx, Limits};

    use super::*;

    fn text() -> String {
        "(const) %[\n \"gold\" => 120,\n \"name\" => \"Mira\"\n]".to_owned()
    }

    fn inner_text() -> (Vec<u8>, Vec<u8>) {
        let body: Vec<u8> = text().encode_utf16().flat_map(u16::to_le_bytes).collect();
        let mut inner = vec![0xFF, 0xFE];
        inner.extend_from_slice(&body);
        (body, inner)
    }

    fn round_trip(input: &[u8]) -> Vec<u8> {
        let limits = Limits::default();
        let mut ctx = DecodeCtx::new(&limits);
        let (inner, params) = KirikiriLayer.decode(input, &mut ctx).expect("decodes");
        KirikiriLayer.encode(&inner, &params).expect("encodes")
    }

    #[test]
    fn plain_scrambled_and_compressed_modes_round_trip() {
        for mode in [0_u8, 1, 2] {
            let (body, _) = inner_text();
            let mut file = vec![0xFE, 0xFE, mode, 0xFF, 0xFE];
            if mode == 2 {
                let compressed = {
                    use flate2::Compression;
                    use flate2::write::ZlibEncoder;
                    use std::io::Write;
                    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::new(6));
                    encoder.write_all(&body).expect("writes");
                    encoder.finish().expect("finishes")
                };
                file.extend_from_slice(&(compressed.len() as u64).to_le_bytes());
                file.extend_from_slice(&(body.len() as u64).to_le_bytes());
                file.extend_from_slice(&compressed);
            } else {
                for pair in body.as_chunks::<2>().0 {
                    let unit = transform(mode, u16::from_le_bytes(*pair));
                    file.extend_from_slice(&unit.to_le_bytes());
                }
            }
            assert_eq!(round_trip(&file), file, "mode {mode}");
            let limits = Limits::default();
            let mut ctx = DecodeCtx::new(&limits);
            let (inner, _) = KirikiriLayer.decode(&file, &mut ctx).expect("decodes");
            assert_eq!(inner, inner_text().1, "mode {mode} inner text");
        }
    }

    #[test]
    fn edited_text_is_wrapped_again() {
        let (body, _) = inner_text();
        let mut file = vec![0xFE, 0xFE, 1, 0xFF, 0xFE];
        for pair in body.as_chunks::<2>().0 {
            let unit = transform(1, u16::from_le_bytes(*pair));
            file.extend_from_slice(&unit.to_le_bytes());
        }
        let limits = Limits::default();
        let mut ctx = DecodeCtx::new(&limits);
        let (mut inner, params) = KirikiriLayer.decode(&file, &mut ctx).expect("decodes");
        let at = inner.len() - 3;
        if let Some(byte) = inner.get_mut(at) {
            *byte = b'7';
        }
        let wrapped = KirikiriLayer.encode(&inner, &params).expect("encodes");
        let mut ctx = DecodeCtx::new(&limits);
        let (again, _) = KirikiriLayer
            .decode(&wrapped, &mut ctx)
            .expect("decodes again");
        assert_eq!(again, inner);
    }

    #[test]
    fn a_bad_header_is_refused() {
        let limits = Limits::default();
        let mut ctx = DecodeCtx::new(&limits);
        assert!(
            KirikiriLayer
                .decode(b"\xFE\xFE\x09\xFF\xFE", &mut ctx)
                .is_err()
        );
        let mut ctx = DecodeCtx::new(&limits);
        assert!(
            KirikiriLayer
                .decode(b"\xFE\xFE\x01\x00\x00", &mut ctx)
                .is_err()
        );
    }
}
