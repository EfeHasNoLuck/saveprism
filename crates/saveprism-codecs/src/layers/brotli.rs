//! The Brotli layer. Brotli has no magic number, so it is only claimed for a `.br` file name and
//! when the stream decompresses cleanly (the trial decode of the detector).
//!
//! Format: RFC 7932. The BSD-3-Clause/MIT `brotli` crate does the coding. The original bytes are
//! kept so an unchanged file is written back exactly.

use std::io::Cursor;

use saveprism_core::limits::len_u64;
use saveprism_core::{Confidence, DecodeCtx, Error, Layer, Params, Result, SniffCtx};

use super::{
    LAY_BROTLI_INVALID, LAY_ENCODE_FAILED, invalid_param, keep_source, source_if_unchanged,
};

/// `compression.brotli`.
///
/// Parameters: `quality` (0..=11), `lgwin` (10..=24) and the source digest set by `decode`.
#[derive(Clone, Copy, Debug, Default)]
pub struct BrotliLayer;

impl Layer for BrotliLayer {
    fn id(&self) -> &'static str {
        "compression.brotli"
    }

    fn sniff(&self, _input: &[u8], ctx: &SniffCtx<'_>) -> Confidence {
        let named = ctx
            .file_name
            .is_some_and(|name| name.to_ascii_lowercase().ends_with(".br"));
        if named {
            Confidence::new(0.55)
        } else {
            Confidence::NONE
        }
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<(Vec<u8>, Params)> {
        let decoder = ::brotli::Decompressor::new(input, 4096);
        let inner = ctx.read_bounded(decoder, len_u64(input.len()), LAY_BROTLI_INVALID)?;
        let mut params = Params::new();
        params.set("quality", 6_i64);
        params.set("lgwin", 22_i64);
        params.set("exact", true);
        keep_source(&mut params, input, &inner);
        Ok((inner, params))
    }

    fn encode(&self, inner: &[u8], params: &Params) -> Result<Vec<u8>> {
        if let Some(source) = source_if_unchanged(inner, params)? {
            return Ok(source);
        }
        let quality = match params.opt_int("quality")? {
            None => 6,
            Some(value) => i32::try_from(value).map_err(|_| {
                invalid_param("parameter \"quality\" must be between 0 and 11".to_string())
            })?,
        };
        if !(0..=11).contains(&quality) {
            return Err(invalid_param(
                "parameter \"quality\" must be between 0 and 11".to_string(),
            ));
        }
        let lgwin = match params.opt_int("lgwin")? {
            None => 22,
            Some(value) => i32::try_from(value).map_err(|_| {
                invalid_param("parameter \"lgwin\" must be between 10 and 24".to_string())
            })?,
        };
        if !(10..=24).contains(&lgwin) {
            return Err(invalid_param(
                "parameter \"lgwin\" must be between 10 and 24".to_string(),
            ));
        }
        let encoder = ::brotli::enc::BrotliEncoderParams {
            quality,
            lgwin,
            size_hint: inner.len(),
            ..Default::default()
        };
        let mut out = Vec::new();
        ::brotli::BrotliCompress(&mut Cursor::new(inner), &mut out, &encoder).map_err(
            |source| Error::new(LAY_ENCODE_FAILED, "Brotli compression failed").with_source(source),
        )?;
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use saveprism_core::{Limits, codes};

    use super::*;

    fn fixture() -> Vec<u8> {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/synthetic/layers/save.json.br");
        std::fs::read(path).unwrap()
    }

    fn decode(input: &[u8]) -> Result<(Vec<u8>, Params)> {
        let limits = Limits::default();
        let mut ctx = DecodeCtx::new(&limits);
        BrotliLayer.decode(input, &mut ctx)
    }

    #[test]
    fn fixture_round_trips_exactly() {
        let bytes = fixture();
        let (inner, params) = decode(&bytes).unwrap();
        assert!(inner.starts_with(b"{"));
        assert_eq!(BrotliLayer.encode(&inner, &params).unwrap(), bytes);
    }

    #[test]
    fn edited_content_is_compressed_again() {
        let bytes = fixture();
        let (mut inner, params) = decode(&bytes).unwrap();
        inner.push(b'\n');
        let written = BrotliLayer.encode(&inner, &params).unwrap();
        let (again, _) = decode(&written).unwrap();
        assert_eq!(again, inner);
    }

    #[test]
    fn bomb_is_stopped_by_the_budget() {
        let mut out = Vec::new();
        let encoder = ::brotli::enc::BrotliEncoderParams::default();
        ::brotli::BrotliCompress(
            &mut Cursor::new(vec![0_u8; 10 * 1024 * 1024]),
            &mut out,
            &encoder,
        )
        .unwrap();
        let limits = Limits {
            max_decoded_bytes: 1024 * 1024,
            ..Limits::default()
        };
        let mut ctx = DecodeCtx::new(&limits);
        assert_eq!(
            BrotliLayer.decode(&out, &mut ctx).unwrap_err().code(),
            codes::CORE_LIMIT_DECODED
        );
    }

    #[test]
    fn damaged_input_gives_the_code() {
        assert_eq!(decode(b"").unwrap_err().code(), LAY_BROTLI_INVALID);
        assert_eq!(
            decode(b"this is not brotli").unwrap_err().code(),
            LAY_BROTLI_INVALID
        );
    }

    #[test]
    fn sniff_needs_the_extension() {
        let named = SniffCtx {
            file_name: Some("save.json.br"),
            depth: 0,
        };
        let unnamed = SniffCtx::default();
        assert!(BrotliLayer.sniff(b"x", &named) > Confidence::MEDIUM);
        assert_eq!(BrotliLayer.sniff(b"x", &unnamed), Confidence::NONE);
    }
}
