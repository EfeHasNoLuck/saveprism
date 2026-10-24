//! The Zstandard frame layer. A frame starts with the magic number `28 B5 2F FD` (little endian
//! `0xFD2FB528`).
//!
//! Format: RFC 8878. The BSD-3-Clause `zstd` crate does the coding. The original bytes are kept so
//! an unchanged file is written back exactly.

use saveprism_core::limits::len_u64;
use saveprism_core::{Confidence, DecodeCtx, Error, Layer, Params, Result, SniffCtx};

use super::{LAY_ENCODE_FAILED, LAY_ZSTD_INVALID, invalid_param, keep_source, source_if_unchanged};

const MAGIC: [u8; 4] = [0x28, 0xB5, 0x2F, 0xFD];

/// `compression.zstd`.
///
/// Parameters: `level` (1..=22) and the source digest set by `decode`.
#[derive(Clone, Copy, Debug, Default)]
pub struct ZstdLayer;

fn bad(message: impl Into<String>, offset: usize) -> Error {
    Error::new(LAY_ZSTD_INVALID, message).with_offset(len_u64(offset))
}

impl Layer for ZstdLayer {
    fn id(&self) -> &'static str {
        "compression.zstd"
    }

    fn sniff(&self, input: &[u8], _ctx: &SniffCtx<'_>) -> Confidence {
        if input.starts_with(&MAGIC) {
            Confidence::new(0.97)
        } else {
            Confidence::NONE
        }
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<(Vec<u8>, Params)> {
        if !input.starts_with(&MAGIC) {
            return Err(bad(
                "missing the Zstandard frame magic number 28 B5 2F FD",
                0,
            ));
        }
        let decoder = ::zstd::stream::read::Decoder::new(input).map_err(|source| {
            bad("the Zstandard frame header is damaged", 0).with_source(source)
        })?;
        let inner = ctx.read_bounded(decoder, len_u64(input.len()), LAY_ZSTD_INVALID)?;
        let mut params = Params::new();
        params.set("level", 3_i64);
        params.set("exact", true);
        keep_source(&mut params, input, &inner);
        Ok((inner, params))
    }

    fn encode(&self, inner: &[u8], params: &Params) -> Result<Vec<u8>> {
        if let Some(source) = source_if_unchanged(inner, params)? {
            return Ok(source);
        }
        let level = match params.opt_int("level")? {
            None => 3,
            Some(value) => i32::try_from(value).map_err(|_| {
                invalid_param("parameter \"level\" must be between 1 and 22".to_string())
            })?,
        };
        if !(1..=22).contains(&level) {
            return Err(invalid_param(
                "parameter \"level\" must be between 1 and 22".to_string(),
            ));
        }
        ::zstd::stream::encode_all(std::io::Cursor::new(inner), level).map_err(|source| {
            Error::new(LAY_ENCODE_FAILED, "Zstandard compression failed").with_source(source)
        })
    }
}

#[cfg(test)]
mod tests {
    use saveprism_core::{Limits, codes};

    use super::*;

    fn fixture() -> Vec<u8> {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/synthetic/layers/save.json.zst");
        std::fs::read(path).unwrap()
    }

    fn decode(input: &[u8]) -> Result<(Vec<u8>, Params)> {
        let limits = Limits::default();
        let mut ctx = DecodeCtx::new(&limits);
        ZstdLayer.decode(input, &mut ctx)
    }

    #[test]
    fn fixture_round_trips_exactly() {
        let bytes = fixture();
        let (inner, params) = decode(&bytes).unwrap();
        assert!(inner.starts_with(b"{"));
        assert_eq!(ZstdLayer.encode(&inner, &params).unwrap(), bytes);
    }

    #[test]
    fn edited_content_is_compressed_again() {
        let bytes = fixture();
        let (mut inner, params) = decode(&bytes).unwrap();
        inner.push(b'\n');
        let written = ZstdLayer.encode(&inner, &params).unwrap();
        let (again, _) = decode(&written).unwrap();
        assert_eq!(again, inner);
    }

    #[test]
    fn bomb_is_stopped_by_the_budget() {
        let packed =
            ::zstd::stream::encode_all(std::io::Cursor::new(vec![0_u8; 10 * 1024 * 1024]), 1)
                .unwrap();
        let limits = Limits {
            max_decoded_bytes: 1024 * 1024,
            ..Limits::default()
        };
        let mut ctx = DecodeCtx::new(&limits);
        assert_eq!(
            ZstdLayer.decode(&packed, &mut ctx).unwrap_err().code(),
            codes::CORE_LIMIT_DECODED
        );
    }

    #[test]
    fn damaged_input_gives_the_code() {
        assert_eq!(decode(b"").unwrap_err().code(), LAY_ZSTD_INVALID);
        assert_eq!(decode(&MAGIC).unwrap_err().code(), LAY_ZSTD_INVALID);
        assert_eq!(
            decode(b"not a zstandard frame").unwrap_err().code(),
            LAY_ZSTD_INVALID
        );
    }

    #[test]
    fn sniff_checks_the_magic() {
        let ctx = SniffCtx::default();
        assert!(ZstdLayer.sniff(&MAGIC, &ctx) > Confidence::HIGH);
        assert_eq!(ZstdLayer.sniff(b"\x28\xB5\x2F", &ctx), Confidence::NONE);
    }
}
