//! The zlib layer (RFC 1950).

use saveprism_core::limits::len_u64;
use saveprism_core::{Confidence, DecodeCtx, Error, Layer, Params, Result, SniffCtx};

use super::deflate::{deflate, find_level, inflate, level_param};
use super::{LAY_ZLIB_INVALID, param_u8};

/// Largest accepted number of bytes after the Adler-32 checksum.
const MAX_TRAILING: usize = 64 * 1024;
const FDICT: u8 = 0x20;

/// `compression.zlib`.
///
/// Parameters: `cmf`, `flg` (the two header bytes), `level` (0..=9), `exact` (bool, whether
/// `level` reproduces the deflate bytes exactly) and optional `trailing` (bytes after the checksum).
#[derive(Clone, Copy, Debug, Default)]
pub struct ZlibLayer;

fn bad(message: impl Into<String>, offset: usize) -> Error {
    Error::new(LAY_ZLIB_INVALID, message).with_offset(len_u64(offset))
}

/// Adler-32 checksum (RFC 1950).
pub(crate) fn adler32(data: &[u8]) -> u32 {
    const MODULUS: u32 = 65_521;
    // The largest block for which the sums cannot overflow `u32` before the reduction.
    const BLOCK: usize = 5_552;
    let mut a = 1_u32;
    let mut b = 0_u32;
    for block in data.chunks(BLOCK) {
        for &byte in block {
            a += u32::from(byte);
            b += a;
        }
        a %= MODULUS;
        b %= MODULUS;
    }
    (b << 16) | a
}

fn header_is_valid(cmf: u8, flg: u8) -> bool {
    let check = (u32::from(cmf) * 256 + u32::from(flg)) % 31;
    cmf & 0x0F == 8 && cmf >> 4 <= 7 && check == 0
}

impl Layer for ZlibLayer {
    fn id(&self) -> &'static str {
        "compression.zlib"
    }

    fn sniff(&self, input: &[u8], _ctx: &SniffCtx<'_>) -> Confidence {
        match (input.first(), input.get(1)) {
            (Some(&cmf), Some(&flg)) if header_is_valid(cmf, flg) => {
                if cmf == 0x78 {
                    Confidence::new(0.92)
                } else {
                    Confidence::HIGH
                }
            }
            _ => Confidence::NONE,
        }
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<(Vec<u8>, Params)> {
        let (Some(&cmf), Some(&flg)) = (input.first(), input.get(1)) else {
            return Err(bad("the zlib header is cut short", 0));
        };
        if !header_is_valid(cmf, flg) {
            return Err(bad("the zlib header is not valid", 0));
        }
        if flg & FDICT != 0 {
            return Err(bad("preset dictionary not supported", 1));
        }
        let body = input.get(2..).unwrap_or_default();
        let inflated = inflate(body, len_u64(input.len()), ctx, LAY_ZLIB_INVALID, 2)?;
        let body_end = 2_usize
            .checked_add(inflated.consumed)
            .ok_or_else(|| bad("the zlib stream is too large", 2))?;
        let checksum_end = body_end
            .checked_add(4)
            .ok_or_else(|| bad("the zlib stream is too large", 2))?;
        let stored = input
            .get(body_end..checksum_end)
            .ok_or_else(|| bad("the zlib checksum is cut short", body_end))?;
        let Ok(stored) = <[u8; 4]>::try_from(stored) else {
            return Err(bad("the zlib checksum is cut short", body_end));
        };
        if u32::from_be_bytes(stored) != adler32(&inflated.data) {
            return Err(bad("the zlib Adler-32 does not match the data", body_end));
        }
        let trailing = input.get(checksum_end..).unwrap_or_default();
        if trailing.len() > MAX_TRAILING {
            return Err(bad(
                format!("more than {MAX_TRAILING} bytes follow the zlib checksum"),
                checksum_end,
            ));
        }

        let original = input.get(2..body_end).unwrap_or_default();
        let (level, exact) = match find_level(&inflated.data, original) {
            Some(level) => (level, true),
            None => {
                let level = match flg >> 6 {
                    0 => 1,
                    1 => 5,
                    3 => 9,
                    _ => 6,
                };
                (level, false)
            }
        };

        let mut params = Params::new();
        params.set("cmf", u32::from(cmf));
        params.set("flg", u32::from(flg));
        params.set("level", level);
        params.set("exact", exact);
        if !trailing.is_empty() {
            params.set("trailing", trailing.to_vec());
        }
        Ok((inflated.data, params))
    }

    fn encode(&self, inner: &[u8], params: &Params) -> Result<Vec<u8>> {
        let cmf = param_u8(params, "cmf", 0x78)?;
        let flg = param_u8(params, "flg", 0x9C)?;
        let level = level_param(params)?;
        let trailing = params.opt_bytes("trailing")?.unwrap_or_default();
        if !header_is_valid(cmf, flg) || flg & FDICT != 0 {
            return Err(super::invalid_param(
                "parameters \"cmf\" and \"flg\" are not a valid zlib header without dictionary"
                    .to_string(),
            ));
        }

        let mut out = vec![cmf, flg];
        out.extend_from_slice(&deflate(inner, level)?);
        out.extend_from_slice(&adler32(inner).to_be_bytes());
        out.extend_from_slice(trailing);
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use saveprism_core::{Limits, codes};

    use super::*;

    fn decode(input: &[u8]) -> Result<(Vec<u8>, Params)> {
        let limits = Limits::default();
        let mut ctx = DecodeCtx::new(&limits);
        ZlibLayer.decode(input, &mut ctx)
    }

    fn sample_text() -> Vec<u8> {
        let mut state = 99_u32;
        let mut out = Vec::new();
        for _ in 0..10_000 {
            state = state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            out.extend_from_slice(format!("k{} ", (state >> 16) % 300).as_bytes());
        }
        out
    }

    fn make_zlib(data: &[u8], cmf: u8, flg: u8, level: u32) -> Vec<u8> {
        let mut out = vec![cmf, flg];
        out.extend_from_slice(&deflate(data, level).unwrap());
        out.extend_from_slice(&adler32(data).to_be_bytes());
        out
    }

    #[test]
    fn adler32_known_value() {
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
        assert_eq!(adler32(b""), 1);
    }

    #[test]
    fn headers_78_01_9c_da_round_trip() {
        let data = sample_text();
        for (flg, level) in [(0x01_u8, 1_u32), (0x9C, 6), (0xDA, 9)] {
            let packed = make_zlib(&data, 0x78, flg, level);
            let (inner, params) = decode(&packed).unwrap();
            assert_eq!(inner, data);
            assert_eq!(params.int("cmf").unwrap(), 0x78);
            assert_eq!(params.int("flg").unwrap(), i64::from(flg));
            assert!(params.bool("exact").unwrap());
            // Several levels may produce identical bytes, so the found level can differ
            // from `level`; byte-exact re-encoding is what matters.
            assert_eq!(ZlibLayer.encode(&inner, &params).unwrap(), packed);
        }
    }

    #[test]
    fn trailing_bytes_are_kept() {
        let mut packed = make_zlib(b"abcabcabc", 0x78, 0x9C, 6);
        packed.extend_from_slice(&[1, 2, 3]);
        let (inner, params) = decode(&packed).unwrap();
        assert_eq!(params.bytes("trailing").unwrap(), [1, 2, 3]);
        assert_eq!(ZlibLayer.encode(&inner, &params).unwrap(), packed);
    }

    #[test]
    fn unknown_deflate_layout_opens_but_is_not_exact() {
        let mut body = vec![0x00, 3, 0, 0xFC, 0xFF];
        body.extend_from_slice(b"abc");
        body.extend_from_slice(&[0x01, 3, 0, 0xFC, 0xFF]);
        body.extend_from_slice(b"def");
        let mut packed = vec![0x78, 0x9C];
        packed.extend_from_slice(&body);
        packed.extend_from_slice(&adler32(b"abcdef").to_be_bytes());
        let (inner, params) = decode(&packed).unwrap();
        assert_eq!(inner, b"abcdef");
        assert!(!params.bool("exact").unwrap());
    }

    #[test]
    fn bomb_is_stopped_by_the_budget() {
        let packed = make_zlib(&vec![0_u8; 10 * 1024 * 1024], 0x78, 0x9C, 6);
        let limits = Limits {
            max_decoded_bytes: 1024 * 1024,
            ..Limits::default()
        };
        let mut ctx = DecodeCtx::new(&limits);
        let error = ZlibLayer.decode(&packed, &mut ctx).unwrap_err();
        assert_eq!(error.code(), codes::CORE_LIMIT_DECODED);
    }

    #[test]
    fn damaged_input_gives_the_zlib_code() {
        let good = make_zlib(b"some data some data", 0x78, 0x9C, 6);

        assert_eq!(decode(b"").unwrap_err().code(), LAY_ZLIB_INVALID);
        assert_eq!(decode(&[0x78]).unwrap_err().code(), LAY_ZLIB_INVALID);
        assert_eq!(
            decode(&[0x78, 0x00, 1, 2]).unwrap_err().code(),
            LAY_ZLIB_INVALID
        );

        let mut bad_sum = good.clone();
        let at = bad_sum.len() - 1;
        *bad_sum.get_mut(at).unwrap() ^= 0xFF;
        assert_eq!(decode(&bad_sum).unwrap_err().code(), LAY_ZLIB_INVALID);

        let cut = good.get(..good.len() - 6).unwrap();
        assert_eq!(decode(cut).unwrap_err().code(), LAY_ZLIB_INVALID);
    }

    #[test]
    fn preset_dictionary_is_refused() {
        // 0x78 0xBB: FDICT set and a valid check value.
        let error = decode(&[0x78, 0xBB, 0, 0, 0, 0, 3, 0]).unwrap_err();
        assert_eq!(error.code(), LAY_ZLIB_INVALID);
        assert!(error.message().contains("preset dictionary not supported"));
    }

    #[test]
    fn sniff_checks_the_header() {
        let ctx = SniffCtx::default();
        assert!(ZlibLayer.sniff(&[0x78, 0x9C], &ctx) > Confidence::HIGH);
        assert_eq!(ZlibLayer.sniff(&[0x78, 0x9D], &ctx), Confidence::NONE);
        assert_eq!(ZlibLayer.sniff(&[0x78], &ctx), Confidence::NONE);
        assert_eq!(ZlibLayer.sniff(b"", &ctx), Confidence::NONE);
    }
}
