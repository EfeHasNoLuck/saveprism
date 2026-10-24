//! The xz layer. A stream starts with the header magic bytes `FD 37 7A 58 5A 00`.
//!
//! Format: the public `.xz` file format specification (`tukaani.org/xz/xz-file-format.txt`). The
//! MIT `lzma-rs` crate does the coding. The index is read first so the declared uncompressed size
//! is checked against the output budget before any dictionary is allocated. The original bytes are
//! kept so an unchanged file is written back exactly.

use std::io::{BufReader, Cursor};

use saveprism_core::limits::len_u64;
use saveprism_core::{Confidence, DecodeCtx, Error, Layer, Params, Result, SniffCtx};

use super::{BoundedSink, LAY_ENCODE_FAILED, LAY_XZ_INVALID, keep_source, source_if_unchanged};

const MAGIC: [u8; 6] = [0xFD, 0x37, 0x7A, 0x58, 0x5A, 0x00];
const FOOTER_MAGIC: [u8; 2] = *b"YZ";
const FOOTER_LEN: usize = 12;

/// `compression.xz`.
///
/// Parameters: the source digest set by `decode`.
#[derive(Clone, Copy, Debug, Default)]
pub struct XzLayer;

fn bad(message: impl Into<String>, offset: usize) -> Error {
    Error::new(LAY_XZ_INVALID, message).with_offset(len_u64(offset))
}

/// Reads a variable-length integer (at most nine bytes) at `at`.
fn varint(input: &[u8], at: usize) -> Option<(u64, usize)> {
    let mut value = 0_u64;
    for index in 0..9 {
        let byte = *input.get(at.checked_add(index)?)?;
        value |= u64::from(byte & 0x7F).checked_shl(u32::try_from(index).ok()?.checked_mul(7)?)?;
        if byte & 0x80 == 0 {
            if index > 0 && byte == 0 {
                return None;
            }
            return Some((value, index + 1));
        }
    }
    None
}

/// The uncompressed size declared by the index of the last stream, or `None` when the footer,
/// header flags and index do not line up.
fn declared_uncompressed(input: &[u8]) -> Option<u64> {
    if input.len() < FOOTER_LEN || input.get(..MAGIC.len())? != MAGIC {
        return None;
    }
    let footer = input.len().checked_sub(FOOTER_LEN)?;
    if input.get(footer + 10..footer + 12)? != FOOTER_MAGIC {
        return None;
    }
    let backward =
        u32::from_le_bytes(<[u8; 4]>::try_from(input.get(footer + 4..footer + 8)?).ok()?);
    let flags = input.get(footer + 8..footer + 10)?;
    if flags != input.get(6..8)? {
        return None;
    }
    let index_size = usize::try_from(backward)
        .ok()?
        .checked_add(1)?
        .checked_mul(4)?;
    let index_start = footer.checked_sub(index_size)?;
    if *input.get(index_start)? != 0x00 {
        return None;
    }
    let (count, used) = varint(input, index_start + 1)?;
    if count > u64::try_from(footer).ok()? {
        return None;
    }
    let mut at = index_start.checked_add(1)?.checked_add(used)?;
    let mut total = 0_u64;
    for _ in 0..count {
        let (_, used) = varint(input, at)?;
        at = at.checked_add(used)?;
        let (uncompressed, used) = varint(input, at)?;
        at = at.checked_add(used)?;
        if at > footer {
            return None;
        }
        total = total.checked_add(uncompressed)?;
    }
    Some(total)
}

impl Layer for XzLayer {
    fn id(&self) -> &'static str {
        "compression.xz"
    }

    fn sniff(&self, input: &[u8], _ctx: &SniffCtx<'_>) -> Confidence {
        if input.starts_with(&MAGIC) {
            Confidence::new(0.98)
        } else {
            Confidence::NONE
        }
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<(Vec<u8>, Params)> {
        if !input.starts_with(&MAGIC) {
            return Err(bad(
                "missing the xz header magic bytes FD 37 7A 58 5A 00",
                0,
            ));
        }
        let declared = declared_uncompressed(input)
            .ok_or_else(|| bad("the xz footer or index is damaged", 0))?;
        let budget = ctx.output_budget(len_u64(input.len()));
        if declared > budget {
            return Err(Error::new(
                saveprism_core::codes::CORE_LIMIT_DECODED,
                format!(
                    "the xz index declares {declared} bytes, the budget is {budget} bytes (possible decompression bomb)"
                ),
            ));
        }
        let mut inner = Vec::new();
        let mut reader = BufReader::new(Cursor::new(input));
        let (result, over) = {
            let mut sink = BoundedSink::new(&mut inner, budget);
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                ::lzma_rs::xz_decompress(&mut reader, &mut sink)
            }));
            (result, sink.over_budget())
        };
        if over {
            return Err(Error::new(
                saveprism_core::codes::CORE_LIMIT_DECODED,
                format!(
                    "the xz stream produced more than {budget} bytes (possible decompression bomb)"
                ),
            ));
        }
        match result {
            Ok(Ok(())) => {}
            Ok(Err(source)) => {
                return Err(
                    bad("the xz stream is damaged or ends too early", 0).with_source(source)
                );
            }
            Err(_) => return Err(bad("the xz stream is malformed", 0)),
        }
        let mut params = Params::new();
        params.set("exact", true);
        keep_source(&mut params, input, &inner);
        Ok((inner, params))
    }

    fn encode(&self, inner: &[u8], params: &Params) -> Result<Vec<u8>> {
        if let Some(source) = source_if_unchanged(inner, params)? {
            return Ok(source);
        }
        let mut out = Vec::new();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            ::lzma_rs::xz_compress(&mut BufReader::new(Cursor::new(inner)), &mut out)
        }));
        match result {
            Ok(Ok(())) => Ok(out),
            Ok(Err(source)) => {
                Err(Error::new(LAY_ENCODE_FAILED, "xz compression failed").with_source(source))
            }
            Err(_) => Err(Error::new(LAY_ENCODE_FAILED, "xz compression failed")),
        }
    }
}

#[cfg(test)]
mod tests {
    use saveprism_core::{Limits, codes};

    use super::*;

    fn fixture() -> Vec<u8> {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/synthetic/layers/save.json.xz");
        std::fs::read(path).unwrap()
    }

    fn decode(input: &[u8]) -> Result<(Vec<u8>, Params)> {
        let limits = Limits::default();
        let mut ctx = DecodeCtx::new(&limits);
        XzLayer.decode(input, &mut ctx)
    }

    #[test]
    fn declared_size_matches_the_index() {
        let bytes = fixture();
        let total = declared_uncompressed(&bytes).unwrap();
        let (inner, _) = decode(&bytes).unwrap();
        assert_eq!(u64::try_from(inner.len()).unwrap(), total);
    }

    #[test]
    fn fixture_round_trips_exactly() {
        let bytes = fixture();
        let (inner, params) = decode(&bytes).unwrap();
        assert!(inner.starts_with(b"{"));
        assert_eq!(XzLayer.encode(&inner, &params).unwrap(), bytes);
    }

    #[test]
    fn edited_content_is_compressed_again() {
        let bytes = fixture();
        let (mut inner, params) = decode(&bytes).unwrap();
        inner.push(b'\n');
        let written = XzLayer.encode(&inner, &params).unwrap();
        let (again, _) = decode(&written).unwrap();
        assert_eq!(again, inner);
    }

    #[test]
    fn a_bomb_is_refused_from_the_index() {
        let mut out = Vec::new();
        ::lzma_rs::xz_compress(
            &mut BufReader::new(Cursor::new(vec![0_u8; 10 * 1024 * 1024])),
            &mut out,
        )
        .unwrap();
        let limits = Limits {
            max_decoded_bytes: 1024 * 1024,
            ..Limits::default()
        };
        let mut ctx = DecodeCtx::new(&limits);
        assert_eq!(
            XzLayer.decode(&out, &mut ctx).unwrap_err().code(),
            codes::CORE_LIMIT_DECODED
        );
    }

    #[test]
    fn damaged_input_gives_the_code() {
        assert_eq!(decode(b"").unwrap_err().code(), LAY_XZ_INVALID);
        assert_eq!(decode(&MAGIC).unwrap_err().code(), LAY_XZ_INVALID);
        let good = fixture();
        let mut cut = good.clone();
        cut.truncate(good.len() - 4);
        assert_eq!(decode(&cut).unwrap_err().code(), LAY_XZ_INVALID);
        let mut footer = good;
        let at = footer.len() - 1;
        *footer.get_mut(at).unwrap() ^= 0xFF;
        assert_eq!(decode(&footer).unwrap_err().code(), LAY_XZ_INVALID);
    }

    #[test]
    fn sniff_checks_the_magic() {
        let ctx = SniffCtx::default();
        assert!(XzLayer.sniff(&MAGIC, &ctx) > Confidence::HIGH);
        assert_eq!(XzLayer.sniff(b"\xFD7zXZ", &ctx), Confidence::NONE);
    }
}
