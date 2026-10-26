//! The lzip layer (`compression.lzip`, `.lz`).
//!
//! Written from the public Lzip manual, "File format" and "Format of the LZMA stream in lzip
//! files" (`https://www.nongnu.org/lzip/manual/lzip_manual.html`, version 1.26, 19 July 2026).
//! A member is the 4-byte magic `LZIP`, a version byte (1), a coded dictionary-size byte, a raw
//! LZMA-302eos stream (lc=3, lp=0, pb=2, terminated by the end-of-stream marker), and a 20-byte
//! trailer with the CRC-32, the uncompressed size and the member size (all little endian).
//!
//! The MIT `lzma-rs` crate does the LZMA coding. Its decoder expects a 5-byte properties and
//! dictionary header in front of a raw stream; that header is synthesized from the lzip header
//! and the size is taken from the trailer, so the stream itself is unchanged.
//!
//! The original bytes are kept, so an unchanged file is written back exactly. An edited file is
//! compressed again with the default lzip properties (the encoder uses an 8 MiB dictionary) and
//! gets a fresh header and trailer.
//!
//! Multi-member files and bytes appended after the member are refused (the format allows them,
//! but there is no public sample here to verify an exact round trip). The error names that.

use std::io::{BufReader, Cursor};

use flate2::Crc;
use saveprism_core::limits::len_u64;
use saveprism_core::{Confidence, DecodeCtx, Error, Layer, Params, Result, SniffCtx, codes};

use super::{BoundedSink, LAY_ENCODE_FAILED, LZIP_INVALID, keep_source, source_if_unchanged};

const MAGIC: &[u8; 4] = b"LZIP";
const HEADER_LEN: usize = 6;
const TRAILER_LEN: usize = 20;
/// LZMA properties byte for lc=3, lp=0, pb=2: (pb * 5 + lp) * 9 + lc.
const PROPS: u8 = 0x5D;
/// The encoder's dictionary (8 MiB) as a coded lzip DS byte: log2(8 MiB) = 23, fraction 0.
const ENCODER_DS: u8 = 23;

fn bad(message: impl Into<String>, offset: usize) -> Error {
    Error::new(LZIP_INVALID, message).with_offset(len_u64(offset))
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = Crc::new();
    crc.update(data);
    crc.sum()
}

/// Dictionary size encoded by a lzip `DS` byte.
fn dictionary_size(ds: u8) -> Option<u32> {
    let log = u32::from(ds & 0x1f);
    if !(12..=29).contains(&log) {
        return None;
    }
    let fraction = u32::from(ds >> 5);
    let base = 1_u32 << log;
    Some(base - fraction * (base / 16))
}

/// `compression.lzip`.
///
/// Parameters: `version`, `ds`, `dictionary`, `unpacked` and `crc32` (as read from the member),
/// plus the source digest set by `decode`.
#[derive(Clone, Copy, Debug, Default)]
pub struct LzipLayer;

impl Layer for LzipLayer {
    fn id(&self) -> &'static str {
        "compression.lzip"
    }

    fn sniff(&self, input: &[u8], _ctx: &SniffCtx<'_>) -> Confidence {
        if input.starts_with(MAGIC) {
            Confidence::new(0.95)
        } else {
            Confidence::NONE
        }
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<(Vec<u8>, Params)> {
        if input.len() < HEADER_LEN + TRAILER_LEN {
            return Err(bad("the lzip member is cut short", 0));
        }
        let version = *input
            .get(4)
            .ok_or_else(|| bad("the lzip header is cut short", 4))?;
        if version != 1 {
            return Err(bad(format!("unsupported lzip format version {version}"), 4));
        }
        let ds = *input
            .get(5)
            .ok_or_else(|| bad("the lzip header is cut short", 5))?;
        let dictionary = dictionary_size(ds)
            .ok_or_else(|| bad("the lzip dictionary-size byte is not valid", 5))?;
        let trailer_start = input
            .len()
            .checked_sub(TRAILER_LEN)
            .filter(|start| *start >= HEADER_LEN)
            .ok_or_else(|| bad("the lzip member is cut short", 0))?;
        let trailer = input
            .get(trailer_start..)
            .ok_or_else(|| bad("the lzip trailer is cut short", trailer_start))?;
        let crc = u32::from_le_bytes(
            trailer
                .get(0..4)
                .ok_or_else(|| bad("the lzip trailer is cut short", trailer_start))?
                .try_into()
                .map_err(|_| bad("the lzip trailer is cut short", trailer_start))?,
        );
        let unpacked = u64::from_le_bytes(
            trailer
                .get(4..12)
                .ok_or_else(|| bad("the lzip trailer is cut short", trailer_start))?
                .try_into()
                .map_err(|_| bad("the lzip trailer is cut short", trailer_start))?,
        );
        let member_size = u64::from_le_bytes(
            trailer
                .get(12..20)
                .ok_or_else(|| bad("the lzip trailer is cut short", trailer_start))?
                .try_into()
                .map_err(|_| bad("the lzip trailer is cut short", trailer_start))?,
        );
        if member_size != len_u64(input.len()) {
            return Err(bad(
                "trailing data or a multi-member lzip file is not supported here",
                trailer_start,
            ));
        }
        let budget = ctx.output_budget(len_u64(input.len()));
        if unpacked > budget {
            return Err(Error::new(
                codes::CORE_LIMIT_DECODED,
                format!(
                    "the lzip trailer declares {unpacked} bytes, the budget is {budget} bytes (possible decompression bomb)"
                ),
            ));
        }
        let stream = input
            .get(HEADER_LEN..trailer_start)
            .ok_or_else(|| bad("the lzip member is cut short", HEADER_LEN))?;
        let mut header = Vec::with_capacity(5);
        header.push(PROPS);
        header.extend_from_slice(&dictionary.to_le_bytes());
        let mut packed = header;
        packed.extend_from_slice(stream);

        let options = ::lzma_rs::decompress::Options {
            unpacked_size: ::lzma_rs::decompress::UnpackedSize::UseProvided(Some(unpacked)),
            memlimit: usize::try_from(budget).ok(),
            allow_incomplete: false,
        };
        let mut reader = BufReader::new(Cursor::new(packed.as_slice()));
        let mut inner = Vec::new();
        let (result, over) = {
            let mut sink = BoundedSink::new(&mut inner, budget);
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                ::lzma_rs::lzma_decompress_with_options(&mut reader, &mut sink, &options)
            }));
            (result, sink.over_budget())
        };
        if over {
            return Err(Error::new(
                codes::CORE_LIMIT_DECODED,
                format!(
                    "the lzip stream produced more than {budget} bytes (possible decompression bomb)"
                ),
            ));
        }
        match result {
            Ok(Ok(())) => {}
            Ok(Err(source)) => {
                return Err(
                    bad("the LZMA stream inside the lzip member is damaged", 0).with_source(source)
                );
            }
            Err(_) => {
                return Err(bad(
                    "the LZMA stream inside the lzip member is malformed",
                    0,
                ));
            }
        }
        if len_u64(inner.len()) != unpacked {
            return Err(bad(
                "the lzip trailer size does not match the decoded data",
                trailer_start + 4,
            ));
        }
        if crc32(&inner) != crc {
            return Err(bad(
                "the lzip CRC-32 does not match the data",
                trailer_start,
            ));
        }

        let mut params = Params::new();
        params.set("version", i64::from(version));
        params.set("ds", i64::from(ds));
        params.set("dictionary", i64::from(dictionary));
        params.set("unpacked", i64::try_from(unpacked).unwrap_or(i64::MAX));
        params.set("crc32", i64::from(crc));
        keep_source(&mut params, input, &inner);
        Ok((inner, params))
    }

    fn encode(&self, inner: &[u8], params: &Params) -> Result<Vec<u8>> {
        if let Some(source) = source_if_unchanged(inner, params)? {
            return Ok(source);
        }
        let options = ::lzma_rs::compress::Options {
            unpacked_size: ::lzma_rs::compress::UnpackedSize::WriteToHeader(None),
        };
        let mut encoded = Vec::new();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            ::lzma_rs::lzma_compress_with_options(
                &mut BufReader::new(Cursor::new(inner)),
                &mut encoded,
                &options,
            )
        }));
        match result {
            Ok(Ok(())) => {}
            Ok(Err(source)) => {
                return Err(
                    Error::new(LAY_ENCODE_FAILED, "LZMA compression failed").with_source(source)
                );
            }
            Err(_) => return Err(Error::new(LAY_ENCODE_FAILED, "LZMA compression failed")),
        }
        let stream = encoded.get(13..).ok_or_else(|| {
            Error::new(LAY_ENCODE_FAILED, "the LZMA encoder wrote a short header")
        })?;
        let member_size = len_u64(HEADER_LEN)
            .saturating_add(len_u64(stream.len()))
            .saturating_add(len_u64(TRAILER_LEN));
        let mut out = Vec::with_capacity(usize::try_from(member_size).unwrap_or(0));
        out.extend_from_slice(MAGIC);
        out.push(1);
        out.push(ENCODER_DS);
        out.extend_from_slice(stream);
        out.extend_from_slice(&crc32(inner).to_le_bytes());
        out.extend_from_slice(&len_u64(inner.len()).to_le_bytes());
        out.extend_from_slice(&member_size.to_le_bytes());
        Ok(out)
    }
}

/// True when the header and trailer are structurally plausible (used only by tests).
#[cfg(test)]
fn plausible(input: &[u8]) -> bool {
    if input.len() < HEADER_LEN + TRAILER_LEN || !input.starts_with(MAGIC) {
        return false;
    }
    input.get(4).copied() == Some(1) && input.get(5).copied().and_then(dictionary_size).is_some()
}

#[cfg(test)]
mod tests {
    use saveprism_core::{Limits, codes};

    use super::*;

    fn fixture() -> Vec<u8> {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/synthetic/layers/save.json.lz");
        std::fs::read(path).unwrap()
    }

    fn decode(input: &[u8]) -> Result<(Vec<u8>, Params)> {
        let limits = Limits::default();
        let mut ctx = DecodeCtx::new(&limits);
        LzipLayer.decode(input, &mut ctx)
    }

    #[test]
    fn fixture_round_trips_exactly() {
        let bytes = fixture();
        assert!(plausible(&bytes));
        let (inner, params) = decode(&bytes).unwrap();
        assert!(inner.starts_with(b"{"));
        assert_eq!(LzipLayer.encode(&inner, &params).unwrap(), bytes);
    }

    #[test]
    fn edited_content_is_compressed_again_and_reads_back() {
        let bytes = fixture();
        let (mut inner, params) = decode(&bytes).unwrap();
        inner.extend_from_slice(b"\n");
        let written = LzipLayer.encode(&inner, &params).unwrap();
        let (again, _) = decode(&written).unwrap();
        assert_eq!(again, inner);
        assert!(written.starts_with(MAGIC));
        assert_eq!(
            written.get(5).copied().and_then(dictionary_size),
            Some(8 * 1024 * 1024)
        );
    }

    #[test]
    fn a_declared_bomb_is_refused_before_decoding() {
        let mut packed = fixture();
        let size = 10_u64 * 1024 * 1024;
        let at = packed.len() - 16;
        packed
            .get_mut(at..at + 8)
            .unwrap()
            .copy_from_slice(&size.to_le_bytes());
        let limits = Limits {
            max_decoded_bytes: 1024 * 1024,
            ..Limits::default()
        };
        let mut ctx = DecodeCtx::new(&limits);
        assert_eq!(
            LzipLayer.decode(&packed, &mut ctx).unwrap_err().code(),
            codes::CORE_LIMIT_DECODED
        );
    }

    #[test]
    fn damaged_input_gives_the_lzip_code() {
        assert_eq!(decode(b"").unwrap_err().code(), LZIP_INVALID);
        assert_eq!(decode(b"LZIP\x01\x17").unwrap_err().code(), LZIP_INVALID);
        let good = fixture();
        let mut bad_crc = good.clone();
        let at = bad_crc.len() - 20;
        *bad_crc.get_mut(at).unwrap() ^= 0xFF;
        assert_eq!(decode(&bad_crc).unwrap_err().code(), LZIP_INVALID);
        let cut = good.get(..good.len() - 3).unwrap();
        assert_eq!(decode(cut).unwrap_err().code(), LZIP_INVALID);
        assert_eq!(
            decode(b"not lzip at all, just text").unwrap_err().code(),
            LZIP_INVALID
        );
    }

    #[test]
    fn trailing_or_multimember_data_is_named() {
        let good = fixture();
        let mut trailing = good.clone();
        trailing.extend_from_slice(b"tail");
        let error = decode(&trailing).unwrap_err();
        assert_eq!(error.code(), LZIP_INVALID);
        assert!(error.message().contains("trailing"));
        let mut two = good.clone();
        two.extend_from_slice(&good);
        assert_eq!(decode(&two).unwrap_err().code(), LZIP_INVALID);
    }

    #[test]
    fn sniff_checks_the_magic() {
        let ctx = SniffCtx::default();
        assert!(LzipLayer.sniff(&fixture(), &ctx) > Confidence::HIGH);
        assert!(LzipLayer.sniff(b"LZIP", &ctx) > Confidence::HIGH);
        assert_eq!(LzipLayer.sniff(b"", &ctx), Confidence::NONE);
    }

    #[test]
    fn dictionary_codes_are_decoded() {
        assert_eq!(dictionary_size(0x17), Some(8 * 1024 * 1024));
        assert_eq!(dictionary_size(0xD3), Some(320 * 1024));
        assert_eq!(dictionary_size(0x00), None);
        assert_eq!(dictionary_size(0xE0), None);
    }
}
