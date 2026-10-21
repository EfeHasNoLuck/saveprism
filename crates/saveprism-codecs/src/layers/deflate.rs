//! Shared raw-deflate helpers for the gzip and zlib layers: bounded inflate and compression level search.

use std::io::Write;

use flate2::Compression;
use flate2::read::DeflateDecoder;
use flate2::write::DeflateEncoder;
use saveprism_core::limits::len_u64;
use saveprism_core::{DecodeCtx, Error, ErrorCode, Result};

use super::LAY_ENCODE_FAILED;

/// Largest inflated size for which the compression level search is attempted.
pub(crate) const MAX_LEVEL_SEARCH_BYTES: usize = 64 * 1024 * 1024;

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
