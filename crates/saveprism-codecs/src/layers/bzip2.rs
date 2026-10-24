//! The bzip2 layer. The stream header is `BZh` followed by the block size digit (`1`-`9`).
//!
//! Format: the public bzip2 manual (`sourceware.org/bzip2/manual/manual.html`). The pure-Rust
//! backend of the MIT/Apache-2.0 `bzip2` crate does the coding; the original bytes are kept so an
//! unchanged file is written back exactly.

use std::io::Write;

use saveprism_core::limits::len_u64;
use saveprism_core::{Confidence, DecodeCtx, Error, Layer, Params, Result, SniffCtx};

use super::{
    LAY_BZIP2_INVALID, LAY_ENCODE_FAILED, invalid_param, keep_source, source_if_unchanged,
};

/// `compression.bzip2`.
///
/// Parameters: `level` (1..=9) and the source digest set by `decode`.
#[derive(Clone, Copy, Debug, Default)]
pub struct Bzip2Layer;

fn bad(message: impl Into<String>, offset: usize) -> Error {
    Error::new(LAY_BZIP2_INVALID, message).with_offset(len_u64(offset))
}

fn level_of(input: &[u8]) -> Option<u32> {
    match input.get(3) {
        Some(&digit @ b'1'..=b'9') => Some(u32::from(digit - b'0')),
        _ => None,
    }
}

impl Layer for Bzip2Layer {
    fn id(&self) -> &'static str {
        "compression.bzip2"
    }

    fn sniff(&self, input: &[u8], _ctx: &SniffCtx<'_>) -> Confidence {
        if input.starts_with(b"BZh") && level_of(input).is_some() {
            Confidence::new(0.95)
        } else {
            Confidence::NONE
        }
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<(Vec<u8>, Params)> {
        let level = level_of(input)
            .ok_or_else(|| bad("missing the bzip2 magic number BZh and block size digit", 0))?;
        let decoder = ::bzip2::read::BzDecoder::new(input);
        let inner = ctx.read_bounded(decoder, len_u64(input.len()), LAY_BZIP2_INVALID)?;
        let mut params = Params::new();
        params.set("level", i64::from(level));
        params.set("exact", true);
        keep_source(&mut params, input, &inner);
        Ok((inner, params))
    }

    fn encode(&self, inner: &[u8], params: &Params) -> Result<Vec<u8>> {
        if let Some(source) = source_if_unchanged(inner, params)? {
            return Ok(source);
        }
        let level = match params.opt_int("level")? {
            None => 9,
            Some(value) => u32::try_from(value).map_err(|_| {
                invalid_param("parameter \"level\" must be between 1 and 9".to_string())
            })?,
        };
        let compression = ::bzip2::Compression::try_new(level).ok_or_else(|| {
            invalid_param("parameter \"level\" must be between 1 and 9".to_string())
        })?;
        let mut encoder = ::bzip2::write::BzEncoder::new(Vec::new(), compression);
        encoder.write_all(inner).map_err(|source| {
            Error::new(LAY_ENCODE_FAILED, "bzip2 compression failed").with_source(source)
        })?;
        encoder.finish().map_err(|source| {
            Error::new(LAY_ENCODE_FAILED, "bzip2 compression failed").with_source(source)
        })
    }
}

#[cfg(test)]
mod tests {
    use saveprism_core::{Limits, codes};

    use super::*;

    fn fixture() -> Vec<u8> {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/synthetic/layers/save.json.bz2");
        std::fs::read(path).unwrap()
    }

    fn decode(input: &[u8]) -> Result<(Vec<u8>, Params)> {
        let limits = Limits::default();
        let mut ctx = DecodeCtx::new(&limits);
        Bzip2Layer.decode(input, &mut ctx)
    }

    #[test]
    fn fixture_round_trips_exactly_and_contains_json() {
        let bytes = fixture();
        let (inner, params) = decode(&bytes).unwrap();
        assert!(inner.starts_with(b"{"), "the payload should be JSON");
        assert!(params.bool("exact").unwrap());
        assert_eq!(Bzip2Layer.encode(&inner, &params).unwrap(), bytes);
    }

    #[test]
    fn edited_content_is_compressed_again() {
        let bytes = fixture();
        let (mut inner, params) = decode(&bytes).unwrap();
        inner.push(b'\n');
        let written = Bzip2Layer.encode(&inner, &params).unwrap();
        assert_ne!(written, bytes);
        let (again, _) = decode(&written).unwrap();
        assert_eq!(again, inner);
    }

    #[test]
    fn bomb_is_stopped_by_the_budget() {
        let mut encoder = ::bzip2::write::BzEncoder::new(Vec::new(), ::bzip2::Compression::fast());
        encoder.write_all(&vec![0_u8; 10 * 1024 * 1024]).unwrap();
        let packed = encoder.finish().unwrap();
        let limits = Limits {
            max_decoded_bytes: 1024 * 1024,
            ..Limits::default()
        };
        let mut ctx = DecodeCtx::new(&limits);
        assert_eq!(
            Bzip2Layer.decode(&packed, &mut ctx).unwrap_err().code(),
            codes::CORE_LIMIT_DECODED
        );
    }

    #[test]
    fn damaged_input_gives_the_code() {
        assert_eq!(decode(b"BZh").unwrap_err().code(), LAY_BZIP2_INVALID);
        assert_eq!(
            decode(b"BZh9not bzip2").unwrap_err().code(),
            LAY_BZIP2_INVALID
        );
        let good = fixture();
        assert_eq!(
            decode(good.get(..good.len() / 2).unwrap())
                .unwrap_err()
                .code(),
            LAY_BZIP2_INVALID
        );
    }

    #[test]
    fn sniff_checks_magic_and_level() {
        let ctx = SniffCtx::default();
        assert!(Bzip2Layer.sniff(b"BZh9", &ctx) > Confidence::HIGH);
        assert_eq!(Bzip2Layer.sniff(b"BZh0", &ctx), Confidence::NONE);
        assert_eq!(Bzip2Layer.sniff(b"BZ", &ctx), Confidence::NONE);
    }
}
