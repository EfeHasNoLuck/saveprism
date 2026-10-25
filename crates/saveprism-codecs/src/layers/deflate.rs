//! Shared raw-deflate helpers for the gzip and zlib layers: bounded inflate and compression level search.

use std::io::Write;

use flate2::Compression;
use flate2::read::DeflateDecoder;
use flate2::write::DeflateEncoder;
use saveprism_core::limits::len_u64;
use saveprism_core::{
    Confidence, DecodeCtx, Error, ErrorCode, Layer, Limits, Params, Result, SniffCtx,
};

use super::{LAY_ENCODE_FAILED, inner_is_known};

/// Largest inflated size for which the compression level search is attempted.
pub(crate) const MAX_LEVEL_SEARCH_BYTES: usize = 64 * 1024 * 1024;
/// Largest output `sniff` accepts while proving a raw stream without an extension hint.
const MAX_SNIFF_OUTPUT: u64 = 8 * 1024 * 1024;

/// Order in which levels are tried: the common ones first.
const LEVEL_ORDER: [u32; 10] = [6, 9, 1, 2, 3, 4, 5, 7, 8, 0];

/// Result of [`inflate`].
pub(crate) struct Inflated {
    /// The inflated bytes.
    pub data: Vec<u8>,
    /// How many bytes of the deflate body the decoder consumed.
    pub consumed: usize,
}

/// Inflates the raw deflate stream at the start of `body` under the output budget of the context.
/// `input_len` is the size of the whole layer input (the budget follows it); `base_offset` is where
/// `body` starts in that input (for error offsets).
pub(crate) fn inflate(
    body: &[u8],
    input_len: u64,
    ctx: &DecodeCtx<'_>,
    code: ErrorCode,
    base_offset: usize,
) -> Result<Inflated> {
    let mut decoder = DeflateDecoder::new(body);
    let data = ctx
        .read_bounded(&mut decoder, input_len, code)
        .map_err(|error| {
            if error.code() == code {
                error
                    .with_offset(len_u64(base_offset))
                    .context("the deflate data is damaged or ends too early")
            } else {
                error
            }
        })?;
    let consumed = usize::try_from(decoder.total_in())
        .map_err(|_| Error::new(code, "the deflate stream is too large"))?;
    Ok(Inflated { data, consumed })
}

/// Compresses `data` as raw deflate at `level` (0..=9).
pub(crate) fn deflate(data: &[u8], level: u32) -> Result<Vec<u8>> {
    let mut encoder = DeflateEncoder::new(Vec::new(), Compression::new(level));
    encoder.write_all(data).map_err(|source| {
        Error::new(LAY_ENCODE_FAILED, "deflate compression failed").with_source(source)
    })?;
    encoder.finish().map_err(|source| {
        Error::new(LAY_ENCODE_FAILED, "deflate compression failed").with_source(source)
    })
}

/// Finds the level whose output equals `original` byte for byte; `None` when no level does or
/// when `data` is too large to search.
pub(crate) fn find_level(data: &[u8], original: &[u8]) -> Option<u32> {
    if data.len() > MAX_LEVEL_SEARCH_BYTES {
        return None;
    }
    LEVEL_ORDER
        .into_iter()
        .find(|&level| deflate(data, level).is_ok_and(|candidate| candidate == original))
}

/// Reads the optional `level` parameter (default 6) and checks it is 0..=9.
pub(crate) fn level_param(params: &saveprism_core::Params) -> Result<u32> {
    let level = super::param_u32(params, "level", 6)?;
    if level > 9 {
        return Err(super::invalid_param(
            "parameter \"level\" must be between 0 and 9".to_string(),
        ));
    }
    Ok(level)
}

/// `compression.deflate`: a raw deflate stream (RFC 1951) without a header, as .NET's
/// `DeflateStream` writes it. Naninovel `.nson` slots use this by default.
///
/// Parameters: `level` (0..=9) and `exact` (whether it reproduces the original bytes).
#[derive(Clone, Copy, Debug, Default)]
pub struct DeflateLayer;

impl Layer for DeflateLayer {
    fn id(&self) -> &'static str {
        "compression.deflate"
    }

    fn sniff(&self, input: &[u8], ctx: &SniffCtx<'_>) -> Confidence {
        let nson = ctx
            .file_name
            .is_some_and(|name| name.to_ascii_lowercase().ends_with(".nson"));
        if nson {
            return Confidence::new(0.55);
        }
        // Raw deflate has no header: prove the stream only for inputs small enough to decode
        // here, and only when the result is a format SavePrism recognises.
        if input.len() < 4 || len_u64(input.len()) > MAX_SNIFF_OUTPUT {
            return Confidence::NONE;
        }
        let limits = Limits {
            max_decoded_bytes: MAX_SNIFF_OUTPUT,
            ratio_floor_bytes: MAX_SNIFF_OUTPUT,
            max_ratio: u64::MAX,
            ..Limits::default()
        };
        let mut trial = DecodeCtx::new(&limits);
        match self.decode(input, &mut trial) {
            Ok((inner, _)) if !inner.is_empty() && inner_is_known(&inner) => Confidence::new(0.42),
            _ => Confidence::NONE,
        }
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<(Vec<u8>, Params)> {
        let inflated = inflate(
            input,
            len_u64(input.len()),
            ctx,
            super::LAY_DEFLATE_INVALID,
            0,
        )?;
        if inflated.consumed != input.len() {
            return Err(Error::new(
                super::LAY_DEFLATE_INVALID,
                "the raw deflate stream is followed by unexpected bytes",
            )
            .with_offset(len_u64(inflated.consumed)));
        }
        let exact = find_level(&inflated.data, input);
        let mut params = Params::new();
        params.set("level", i64::from(exact.unwrap_or(6)));
        params.set("exact", exact.is_some());
        Ok((inflated.data, params))
    }

    fn encode(&self, inner: &[u8], params: &Params) -> Result<Vec<u8>> {
        deflate(inner, level_param(params)?)
    }
}
