//! The LZMA-alone layer (`.lzma`): no magic number, only a 13-byte header.
//!
//! Header (public LZMA SDK `DOC/lzma.txt`): one properties byte (`lc`, `lp`, `pb`), a
//! little-endian 32-bit dictionary size and a little-endian 64-bit uncompressed size
//! (`0xFFFF_FFFF_FFFF_FFFF` means unknown). Because there is no magic, `sniff` only reports a weak
//! hint when those fields are sane; `decode` then has to decompress the stream.
//!
//! The MIT `lzma-rs` crate does the coding. The original bytes are kept so an unchanged file is
//! written back exactly.

use std::io::{BufReader, Cursor};

use saveprism_core::limits::len_u64;
use saveprism_core::{Confidence, DecodeCtx, Error, Layer, Limits, Params, Result, SniffCtx};

use super::{
    BoundedSink, LAY_ENCODE_FAILED, LAY_LZMA_INVALID, inner_is_known, keep_source,
    source_if_unchanged,
};

const HEADER_LEN: usize = 13;
/// Largest output `sniff` accepts while proving that the stream decodes.
const MAX_SNIFF_OUTPUT: u64 = 64 * 1024 * 1024;
const UNKNOWN_SIZE: u64 = 0xFFFF_FFFF_FFFF_FFFF;
/// Largest dictionary size accepted before decoding (4 GiB is the format maximum).
const MAX_DICT: u32 = 0x4000_0000;

/// `compression.lzma`.
///
/// Parameters: `props`, `dict_size`, `unpacked` (as read from the header) and the source digest.
#[derive(Clone, Copy, Debug, Default)]
pub struct LzmaLayer;

fn bad(message: impl Into<String>, offset: usize) -> Error {
    Error::new(LAY_LZMA_INVALID, message).with_offset(len_u64(offset))
}

/// `props`, `dict_size` and the declared size (or `None` for the unknown marker) when the header is
/// plausible.
fn header(input: &[u8]) -> Option<(u8, u32, Option<u64>)> {
    let head = input.get(..HEADER_LEN)?;
    let props = *head.first()?;
    if props >= 225 {
        return None;
    }
    let dict_size = u32::from_le_bytes(<[u8; 4]>::try_from(head.get(1..5)?).ok()?);
    if !(0x1000..=MAX_DICT).contains(&dict_size) {
        return None;
    }
    let stored = u64::from_le_bytes(<[u8; 8]>::try_from(head.get(5..13)?).ok()?);
    let unpacked = if stored == UNKNOWN_SIZE {
        None
    } else {
        Some(stored)
    };
    Some((props, dict_size, unpacked))
}

impl Layer for LzmaLayer {
    fn id(&self) -> &'static str {
        "compression.lzma"
    }

    /// No magic: only a weak hint, and only when the header fields are sane and the stream really
    /// decodes (a bounded trial here, because many binary files have a plausible first byte).
    fn sniff(&self, input: &[u8], _ctx: &SniffCtx<'_>) -> Confidence {
        if header(input).is_none() || input.len() <= HEADER_LEN {
            return Confidence::NONE;
        }
        if len_u64(input.len()) > MAX_SNIFF_OUTPUT {
            return Confidence::new(0.42);
        }
        let limits = Limits {
            max_decoded_bytes: MAX_SNIFF_OUTPUT,
            max_ratio: u64::MAX,
            ..Limits::default()
        };
        let mut ctx = DecodeCtx::new(&limits);
        match self.decode(input, &mut ctx) {
            Ok((inner, _)) if inner_is_known(&inner) => Confidence::new(0.42),
            _ => Confidence::NONE,
        }
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<(Vec<u8>, Params)> {
        let (props, dict_size, unpacked) =
            header(input).ok_or_else(|| bad("the LZMA-alone header is not valid", 0))?;
        let budget = ctx.output_budget(len_u64(input.len()));
        if let Some(size) = unpacked
            && size > budget
        {
            return Err(Error::new(
                saveprism_core::codes::CORE_LIMIT_DECODED,
                format!(
                    "the LZMA header declares {size} bytes, the budget is {budget} bytes (possible decompression bomb)"
                ),
            ));
        }
        let memlimit = usize::try_from(budget).ok();
        let options = ::lzma_rs::decompress::Options {
            unpacked_size: ::lzma_rs::decompress::UnpackedSize::ReadFromHeader,
            memlimit,
            allow_incomplete: false,
        };
        let mut reader = BufReader::new(Cursor::new(input));
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
                saveprism_core::codes::CORE_LIMIT_DECODED,
                format!(
                    "the LZMA stream produced more than {budget} bytes (possible decompression bomb)"
                ),
            ));
        }
        match result {
            Ok(Ok(())) => {}
            Ok(Err(source)) => {
                return Err(
                    bad("the LZMA stream is damaged or ends too early", 0).with_source(source)
                );
            }
            Err(_) => return Err(bad("the LZMA stream is malformed", 0)),
        }
        if let Some(size) = unpacked
            && size != len_u64(inner.len())
        {
            return Err(bad(
                "the LZMA header size does not match the decoded data",
                HEADER_LEN,
            ));
        }
        let mut params = Params::new();
        params.set("props", i64::from(props));
        params.set("dict_size", i64::from(dict_size));
        params.set(
            "unpacked",
            match unpacked {
                Some(size) => i64::try_from(size).unwrap_or(i64::MAX),
                None => -1,
            },
        );
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
            ::lzma_rs::lzma_compress(&mut BufReader::new(Cursor::new(inner)), &mut out)
        }));
        match result {
            Ok(Ok(())) => Ok(out),
            Ok(Err(source)) => {
                Err(Error::new(LAY_ENCODE_FAILED, "LZMA compression failed").with_source(source))
            }
            Err(_) => Err(Error::new(LAY_ENCODE_FAILED, "LZMA compression failed")),
        }
    }
}

#[cfg(test)]
mod tests {
    use saveprism_core::{Limits, codes};

    use super::*;

    fn fixture() -> Vec<u8> {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/synthetic/layers/save.json.lzma");
        std::fs::read(path).unwrap()
    }

    fn decode(input: &[u8]) -> Result<(Vec<u8>, Params)> {
        let limits = Limits::default();
        let mut ctx = DecodeCtx::new(&limits);
        LzmaLayer.decode(input, &mut ctx)
    }

    #[test]
    fn fixture_round_trips_exactly() {
        let bytes = fixture();
        let (inner, params) = decode(&bytes).unwrap();
        assert!(inner.starts_with(b"{"));
        assert_eq!(LzmaLayer.encode(&inner, &params).unwrap(), bytes);
    }

    #[test]
    fn header_fields_are_meanwhile_not_mistaken_for_another_format() {
        let bytes = fixture();
        let (props, dict_size, unpacked) = header(&bytes).unwrap();
        assert!(props < 225);
        assert!((0x1000..=MAX_DICT).contains(&dict_size));
        assert!(unpacked.is_none(), "our encoder writes the unknown size");
    }

    #[test]
    fn edited_content_is_compressed_again() {
        let bytes = fixture();
        let (mut inner, params) = decode(&bytes).unwrap();
        inner.push(b'\n');
        let written = LzmaLayer.encode(&inner, &params).unwrap();
        let (again, _) = decode(&written).unwrap();
        assert_eq!(again, inner);
    }

    #[test]
    fn a_declared_bomb_is_refused_before_decoding() {
        let mut packed = fixture();
        let size = 10_u64 * 1024 * 1024;
        packed
            .get_mut(5..13)
            .unwrap()
            .copy_from_slice(&size.to_le_bytes());
        let limits = Limits {
            max_decoded_bytes: 1024 * 1024,
            ..Limits::default()
        };
        let mut ctx = DecodeCtx::new(&limits);
        assert_eq!(
            LzmaLayer.decode(&packed, &mut ctx).unwrap_err().code(),
            codes::CORE_LIMIT_DECODED
        );
    }

    #[test]
    fn damaged_input_gives_the_code() {
        assert_eq!(decode(b"").unwrap_err().code(), LAY_LZMA_INVALID);
        assert_eq!(decode(&[0_u8; 13]).unwrap_err().code(), LAY_LZMA_INVALID);
        let good = fixture();
        assert_eq!(
            decode(good.get(..good.len() - 5).unwrap())
                .unwrap_err()
                .code(),
            LAY_LZMA_INVALID
        );
    }

    #[test]
    fn sniff_is_only_a_weak_hint() {
        let ctx = SniffCtx::default();
        let bytes = fixture();
        let confidence = LzmaLayer.sniff(&bytes, &ctx);
        assert!(confidence > Confidence::NONE);
        assert!(confidence < Confidence::MEDIUM);
        assert_eq!(LzmaLayer.sniff(b"short", &ctx), Confidence::NONE);
    }
}
