//! The LZ4 frame layer. A frame starts with the magic number `04 22 4D 18` (little endian
//! `0x184D2204`).
//!
//! Format: the public LZ4 Frame Format Description. The MIT/Apache-2.0 `lz4_flex` crate (already a
//! dependency) does the coding. The original bytes are kept so an unchanged file is written back
//! exactly.

use std::io::Write;

use saveprism_core::limits::len_u64;
use saveprism_core::{Confidence, DecodeCtx, Error, Layer, Params, Result, SniffCtx};

use super::{LAY_ENCODE_FAILED, LAY_LZ4_INVALID, keep_source, source_if_unchanged};

const MAGIC: [u8; 4] = [0x04, 0x22, 0x4D, 0x18];

/// `compression.lz4`.
///
/// Parameters: the source digest set by `decode`.
#[derive(Clone, Copy, Debug, Default)]
pub struct Lz4Layer;

fn bad(message: impl Into<String>, offset: usize) -> Error {
    Error::new(LAY_LZ4_INVALID, message).with_offset(len_u64(offset))
}

impl Layer for Lz4Layer {
    fn id(&self) -> &'static str {
        "compression.lz4"
    }

    fn sniff(&self, input: &[u8], _ctx: &SniffCtx<'_>) -> Confidence {
        if input.starts_with(&MAGIC) {
            Confidence::new(0.96)
        } else {
            Confidence::NONE
        }
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<(Vec<u8>, Params)> {
        if !input.starts_with(&MAGIC) {
            return Err(bad("missing the LZ4 frame magic number 04 22 4D 18", 0));
        }
        if input.len() < 7 {
            return Err(bad("the LZ4 frame descriptor is cut short", 0));
        }
        let decoder = ::lz4_flex::frame::FrameDecoder::new(input);
        let inner = ctx.read_bounded(decoder, len_u64(input.len()), LAY_LZ4_INVALID)?;
        let mut params = Params::new();
        params.set("exact", true);
        keep_source(&mut params, input, &inner);
        Ok((inner, params))
    }

    fn encode(&self, inner: &[u8], params: &Params) -> Result<Vec<u8>> {
        if let Some(source) = source_if_unchanged(inner, params)? {
            return Ok(source);
        }
        let mut encoder = ::lz4_flex::frame::FrameEncoder::new(Vec::new());
        encoder.write_all(inner).map_err(|source| {
            Error::new(LAY_ENCODE_FAILED, "LZ4 compression failed").with_source(source)
        })?;
        encoder.finish().map_err(|source| {
            Error::new(LAY_ENCODE_FAILED, "LZ4 compression failed").with_source(source)
        })
    }
}

#[cfg(test)]
mod tests {
    use saveprism_core::{Limits, codes};

    use super::*;

    fn fixture() -> Vec<u8> {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/synthetic/layers/save.json.lz4");
        std::fs::read(path).unwrap()
    }

    fn decode(input: &[u8]) -> Result<(Vec<u8>, Params)> {
        let limits = Limits::default();
        let mut ctx = DecodeCtx::new(&limits);
        Lz4Layer.decode(input, &mut ctx)
    }

    #[test]
    fn fixture_round_trips_exactly() {
        let bytes = fixture();
        let (inner, params) = decode(&bytes).unwrap();
        assert!(inner.starts_with(b"{"));
        assert_eq!(Lz4Layer.encode(&inner, &params).unwrap(), bytes);
    }

    #[test]
    fn edited_content_is_compressed_again() {
        let bytes = fixture();
        let (mut inner, params) = decode(&bytes).unwrap();
        inner.push(b'\n');
        let written = Lz4Layer.encode(&inner, &params).unwrap();
        let (again, _) = decode(&written).unwrap();
        assert_eq!(again, inner);
    }

    #[test]
    fn bomb_is_stopped_by_the_budget() {
        let mut encoder = ::lz4_flex::frame::FrameEncoder::new(Vec::new());
        encoder.write_all(&vec![0_u8; 10 * 1024 * 1024]).unwrap();
        let packed = encoder.finish().unwrap();
        let limits = Limits {
            max_decoded_bytes: 1024 * 1024,
            ..Limits::default()
        };
        let mut ctx = DecodeCtx::new(&limits);
        assert_eq!(
            Lz4Layer.decode(&packed, &mut ctx).unwrap_err().code(),
            codes::CORE_LIMIT_DECODED
        );
    }

    #[test]
    fn damaged_input_gives_the_code() {
        assert_eq!(decode(b"").unwrap_err().code(), LAY_LZ4_INVALID);
        assert_eq!(decode(&MAGIC).unwrap_err().code(), LAY_LZ4_INVALID);
        assert_eq!(
            decode(b"not lz4 at all").unwrap_err().code(),
            LAY_LZ4_INVALID
        );
    }

    #[test]
    fn sniff_checks_the_magic() {
        let ctx = SniffCtx::default();
        assert!(Lz4Layer.sniff(&MAGIC, &ctx) > Confidence::HIGH);
        assert_eq!(Lz4Layer.sniff(b"\x04\x22\x4D", &ctx), Confidence::NONE);
    }
}
