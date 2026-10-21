//! The gzip layer (RFC 1952). The header is parsed by hand so every field can be written back.

use flate2::Crc;
use saveprism_core::limits::len_u64;
use saveprism_core::{Confidence, DecodeCtx, Error, Layer, Params, Result, SniffCtx};

use super::deflate::{deflate, find_level, inflate, level_param};
use super::{LAY_ENCODE_FAILED, LAY_GZIP_INVALID, invalid_param, param_u8, param_u32};

/// Largest accepted number of bytes after the gzip trailer.
const MAX_TRAILING: usize = 64 * 1024;

const FTEXT: u8 = 0x01;
const FHCRC: u8 = 0x02;
const FEXTRA: u8 = 0x04;
const FNAME: u8 = 0x08;
const FCOMMENT: u8 = 0x10;
const RESERVED_FLAGS: u8 = 0xE0;

/// `compression.gzip`.
///
/// Parameters: `mtime`, `xfl`, `os` (integers), `ftext`, `fhcrc` (bool), optional `fname`,
/// `fcomment`, `fextra` (bytes), `level` (0..=9), `exact` (bool, whether `level` reproduces the
/// deflate bytes exactly) and optional `trailing` (bytes after the trailer).
#[derive(Clone, Copy, Debug, Default)]
pub struct GzipLayer;

fn bad(message: impl Into<String>, offset: usize) -> Error {
    Error::new(LAY_GZIP_INVALID, message).with_offset(len_u64(offset))
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = Crc::new();
    crc.update(data);
    crc.sum()
}

/// Forward-only reader over the input.
struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, count: usize) -> Option<&'a [u8]> {
        let end = self.pos.checked_add(count)?;
        let slice = self.data.get(self.pos..end)?;
        self.pos = end;
        Some(slice)
    }

    /// Reads up to (not including) a zero byte and skips the zero.
    fn take_cstr(&mut self) -> Option<&'a [u8]> {
        let rest = self.data.get(self.pos..)?;
        let length = rest.iter().position(|&byte| byte == 0)?;
        let text = rest.get(..length)?;
        self.pos = self.pos.checked_add(length)?.checked_add(1)?;
        Some(text)
    }
}

impl Layer for GzipLayer {
    fn id(&self) -> &'static str {
        "compression.gzip"
    }

    fn sniff(&self, input: &[u8], _ctx: &SniffCtx<'_>) -> Confidence {
        if input.starts_with(&[0x1F, 0x8B, 0x08]) {
            Confidence::new(0.95)
        } else {
            Confidence::NONE
        }
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<(Vec<u8>, Params)> {
        let mut cursor = Cursor {
            data: input,
            pos: 0,
        };
        let head = cursor
            .take(10)
            .ok_or_else(|| bad("the gzip header is cut short", 0))?;
        let Ok([id1, id2, method, flags, m0, m1, m2, m3, xfl, os]) = <[u8; 10]>::try_from(head)
        else {
            return Err(bad("the gzip header is cut short", 0));
        };
        if id1 != 0x1F || id2 != 0x8B {
            return Err(bad("missing gzip magic number 1F 8B", 0));
        }
        if method != 8 {
            return Err(bad(
                format!("unsupported gzip compression method {method}"),
                2,
            ));
        }
        if flags & RESERVED_FLAGS != 0 {
            return Err(bad("reserved gzip flag bits are set", 3));
        }
        let mtime = u32::from_le_bytes([m0, m1, m2, m3]);

        let mut fextra: Option<Vec<u8>> = None;
        if flags & FEXTRA != 0 {
            let at = cursor.pos;
            let length = cursor
                .take(2)
                .ok_or_else(|| bad("the gzip extra field is cut short", at))?;
            let length = match <[u8; 2]>::try_from(length) {
                Ok(bytes) => usize::from(u16::from_le_bytes(bytes)),
                Err(_) => return Err(bad("the gzip extra field is cut short", at)),
            };
            let extra = cursor
                .take(length)
                .ok_or_else(|| bad("the gzip extra field is cut short", at))?;
            fextra = Some(extra.to_vec());
        }
        let mut fname: Option<Vec<u8>> = None;
        if flags & FNAME != 0 {
            let at = cursor.pos;
            let text = cursor
                .take_cstr()
                .ok_or_else(|| bad("the gzip file name is not terminated", at))?;
            fname = Some(text.to_vec());
        }
        let mut fcomment: Option<Vec<u8>> = None;
        if flags & FCOMMENT != 0 {
            let at = cursor.pos;
            let text = cursor
                .take_cstr()
                .ok_or_else(|| bad("the gzip comment is not terminated", at))?;
            fcomment = Some(text.to_vec());
        }
        let header_end = cursor.pos;
        if flags & FHCRC != 0 {
            let at = cursor.pos;
            let stored = cursor
                .take(2)
                .ok_or_else(|| bad("the gzip header checksum is cut short", at))?;
            let stored = match <[u8; 2]>::try_from(stored) {
                Ok(bytes) => u16::from_le_bytes(bytes),
                Err(_) => return Err(bad("the gzip header checksum is cut short", at)),
            };
            let header = input.get(..header_end).unwrap_or_default();
            if u32::from(stored) != crc32(header) & 0xFFFF {
                return Err(bad("the gzip header checksum does not match", at));
            }
        }

        let body_start = cursor.pos;
        let body = input.get(body_start..).unwrap_or_default();
        let inflated = inflate(
            body,
            len_u64(input.len()),
            ctx,
            LAY_GZIP_INVALID,
            body_start,
        )?;
        let body_end = body_start
            .checked_add(inflated.consumed)
            .ok_or_else(|| bad("the gzip stream is too large", body_start))?;
        cursor.pos = body_end;
        let trailer = cursor
            .take(8)
            .ok_or_else(|| bad("the gzip trailer is cut short", body_end))?;
        let Ok([c0, c1, c2, c3, s0, s1, s2, s3]) = <[u8; 8]>::try_from(trailer) else {
            return Err(bad("the gzip trailer is cut short", body_end));
        };
        let stored_crc = u32::from_le_bytes([c0, c1, c2, c3]);
        let stored_size = u32::from_le_bytes([s0, s1, s2, s3]);
        if stored_crc != crc32(&inflated.data) {
            return Err(bad("the gzip CRC-32 does not match the data", body_end));
        }
        let actual_size = (len_u64(inflated.data.len()) & 0xFFFF_FFFF) as u32;
        if stored_size != actual_size {
            return Err(bad(
                "the gzip size field does not match the data",
                body_end + 4,
            ));
        }
        let trailing = input.get(cursor.pos..).unwrap_or_default();
        if trailing.starts_with(&[0x1F, 0x8B]) {
            return Err(bad("multi-member gzip is not supported yet", cursor.pos));
        }
        if trailing.len() > MAX_TRAILING {
            return Err(bad(
                format!("more than {MAX_TRAILING} bytes follow the gzip trailer"),
                cursor.pos,
            ));
        }

        let original = input.get(body_start..body_end).unwrap_or_default();
        let (level, exact) = match find_level(&inflated.data, original) {
            Some(level) => (level, true),
            None => {
                let level = match xfl {
                    2 => 9,
                    4 => 1,
                    _ => 6,
                };
                (level, false)
            }
        };

        let mut params = Params::new();
        params.set("mtime", mtime);
        params.set("xfl", u32::from(xfl));
        params.set("os", u32::from(os));
        params.set("ftext", flags & FTEXT != 0);
        params.set("fhcrc", flags & FHCRC != 0);
        if let Some(bytes) = fname {
            params.set("fname", bytes);
        }
        if let Some(bytes) = fcomment {
            params.set("fcomment", bytes);
        }
        if let Some(bytes) = fextra {
            params.set("fextra", bytes);
        }
        params.set("level", level);
        params.set("exact", exact);
        if !trailing.is_empty() {
            params.set("trailing", trailing.to_vec());
        }
        Ok((inflated.data, params))
    }

    fn encode(&self, inner: &[u8], params: &Params) -> Result<Vec<u8>> {
        let mtime = param_u32(params, "mtime", 0)?;
        let xfl = param_u8(params, "xfl", 0)?;
        let os = param_u8(params, "os", 255)?;
        let ftext = params.opt_bool("ftext")?.unwrap_or(false);
        let fhcrc = params.opt_bool("fhcrc")?.unwrap_or(false);
        let fname = params.opt_bytes("fname")?;
        let fcomment = params.opt_bytes("fcomment")?;
        let fextra = params.opt_bytes("fextra")?;
        let trailing = params.opt_bytes("trailing")?.unwrap_or_default();
        let level = level_param(params)?;

        let mut flags = 0_u8;
        if ftext {
            flags |= FTEXT;
        }
        if fhcrc {
            flags |= FHCRC;
        }
        if fextra.is_some() {
            flags |= FEXTRA;
        }
        if fname.is_some() {
            flags |= FNAME;
        }
        if fcomment.is_some() {
            flags |= FCOMMENT;
        }

        let mut out = vec![0x1F, 0x8B, 8, flags];
        out.extend_from_slice(&mtime.to_le_bytes());
        out.push(xfl);
        out.push(os);
        if let Some(extra) = fextra {
            let length = u16::try_from(extra.len()).map_err(|_| {
                invalid_param("parameter \"fextra\" is longer than 65535 bytes".into())
            })?;
            out.extend_from_slice(&length.to_le_bytes());
            out.extend_from_slice(extra);
        }
        for (key, text) in [("fname", fname), ("fcomment", fcomment)] {
            if let Some(text) = text {
                if text.contains(&0) {
                    return Err(Error::new(
                        LAY_ENCODE_FAILED,
                        format!("parameter \"{key}\" contains a zero byte"),
                    ));
                }
                out.extend_from_slice(text);
                out.push(0);
            }
        }
        if fhcrc {
            let check = u16::try_from(crc32(&out) & 0xFFFF).unwrap_or(0);
            out.extend_from_slice(&check.to_le_bytes());
        }

        out.extend_from_slice(&deflate(inner, level)?);
        out.extend_from_slice(&crc32(inner).to_le_bytes());
        let size = (len_u64(inner.len()) & 0xFFFF_FFFF) as u32;
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(trailing);
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use flate2::Compression;
    use flate2::write::GzEncoder;
    use saveprism_core::{Limits, codes};

    use super::*;

    /// Pseudo-random words so that different compression levels give different output.
    fn sample_text() -> Vec<u8> {
        let mut state = 12345_u32;
        let mut out = Vec::new();
        for _ in 0..20_000 {
            state = state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            let word = (state >> 16) % 500;
            out.extend_from_slice(format!("w{word} ").as_bytes());
        }
        out
    }

    fn gzip_with_level(data: &[u8], level: u32) -> Vec<u8> {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::new(level));
        encoder.write_all(data).unwrap();
        encoder.finish().unwrap()
    }

    fn decode(input: &[u8]) -> Result<(Vec<u8>, Params)> {
        let limits = Limits::default();
        let mut ctx = DecodeCtx::new(&limits);
        GzipLayer.decode(input, &mut ctx)
    }

    /// A gzip file around a hand-made deflate body.
    fn wrap_gzip(body: &[u8], data: &[u8]) -> Vec<u8> {
        let mut out = vec![0x1F, 0x8B, 8, 0, 0, 0, 0, 0, 0, 3];
        out.extend_from_slice(body);
        out.extend_from_slice(&crc32(data).to_le_bytes());
        out.extend_from_slice(&u32::try_from(data.len()).unwrap().to_le_bytes());
        out
    }

    #[test]
    fn levels_are_found_and_round_trip() {
        let data = sample_text();
        for level in [1_u32, 6, 9] {
            let packed = gzip_with_level(&data, level);
            let (inner, params) = decode(&packed).unwrap();
            assert_eq!(inner, data);
            assert!(params.bool("exact").unwrap(), "level {level} not exact");
            // Several levels may produce identical bytes, so the found level can differ
            // from `level`; byte-exact re-encoding is what matters.
            assert_eq!(GzipLayer.encode(&inner, &params).unwrap(), packed);
        }
    }

    #[test]
    fn header_fields_are_preserved() {
        let data = b"hello hello hello hello".to_vec();
        let mut params = Params::new();
        params.set("mtime", 1_700_000_000_u32);
        params.set("xfl", 2_u32);
        params.set("os", 3_u32);
        params.set("fname", b"save.json".to_vec());
        params.set("fcomment", b"made by test".to_vec());
        params.set("fextra", vec![1_u8, 2, 3]);
        params.set("fhcrc", true);
        params.set("ftext", true);
        params.set("level", 9_u32);
        params.set("trailing", vec![7_u8, 8, 9]);
        let packed = GzipLayer.encode(&data, &params).unwrap();

        let (inner, decoded) = decode(&packed).unwrap();
        assert_eq!(inner, data);
        assert_eq!(decoded.int("mtime").unwrap(), 1_700_000_000);
        assert_eq!(decoded.int("os").unwrap(), 3);
        assert_eq!(decoded.bytes("fname").unwrap(), b"save.json");
        assert_eq!(decoded.bytes("fcomment").unwrap(), b"made by test");
        assert_eq!(decoded.bytes("fextra").unwrap(), [1, 2, 3]);
        assert!(decoded.bool("fhcrc").unwrap());
        assert_eq!(decoded.bytes("trailing").unwrap(), [7, 8, 9]);
        assert_eq!(GzipLayer.encode(&inner, &decoded).unwrap(), packed);
    }

    #[test]
    fn unknown_deflate_layout_opens_but_is_not_exact() {
        // Two stored blocks: no compression level produces this.
        let mut body = vec![0x00, 3, 0, 0xFC, 0xFF];
        body.extend_from_slice(b"abc");
        body.extend_from_slice(&[0x01, 3, 0, 0xFC, 0xFF]);
        body.extend_from_slice(b"def");
        let packed = wrap_gzip(&body, b"abcdef");
        let (inner, params) = decode(&packed).unwrap();
        assert_eq!(inner, b"abcdef");
        assert!(!params.bool("exact").unwrap());
    }

    #[test]
    fn bomb_is_stopped_by_the_budget() {
        let packed = gzip_with_level(&vec![0_u8; 10 * 1024 * 1024], 6);
        let limits = Limits {
            max_decoded_bytes: 1024 * 1024,
            ..Limits::default()
        };
        let mut ctx = DecodeCtx::new(&limits);
        let error = GzipLayer.decode(&packed, &mut ctx).unwrap_err();
        assert_eq!(error.code(), codes::CORE_LIMIT_DECODED);
    }

    #[test]
    fn damaged_input_gives_the_gzip_code() {
        let good = gzip_with_level(b"some data some data", 6);

        let short = good.get(..6).unwrap();
        assert_eq!(decode(short).unwrap_err().code(), LAY_GZIP_INVALID);

        let mut bad_crc = good.clone();
        let at = bad_crc.len() - 8;
        *bad_crc.get_mut(at).unwrap() ^= 0xFF;
        assert_eq!(decode(&bad_crc).unwrap_err().code(), LAY_GZIP_INVALID);

        let mut bad_size = good.clone();
        let at = bad_size.len() - 1;
        *bad_size.get_mut(at).unwrap() ^= 0xFF;
        assert_eq!(decode(&bad_size).unwrap_err().code(), LAY_GZIP_INVALID);

        let cut = good.get(..good.len() - 5).unwrap();
        assert_eq!(decode(cut).unwrap_err().code(), LAY_GZIP_INVALID);

        assert_eq!(
            decode(b"not gzip at all").unwrap_err().code(),
            LAY_GZIP_INVALID
        );

        let mut too_much = good.clone();
        too_much.extend(vec![0_u8; MAX_TRAILING + 1]);
        assert_eq!(decode(&too_much).unwrap_err().code(), LAY_GZIP_INVALID);
    }

    #[test]
    fn second_member_is_rejected_but_padding_is_kept() {
        let good = gzip_with_level(b"some data some data", 6);
        let mut two = good.clone();
        two.extend_from_slice(&good);
        let error = decode(&two).unwrap_err();
        assert_eq!(error.code(), LAY_GZIP_INVALID);

        let mut padded = good.clone();
        padded.extend_from_slice(&[0, 0, 0, 0]);
        let (_, params) = decode(&padded).unwrap();
        assert_eq!(params.bytes("trailing").unwrap(), [0, 0, 0, 0]);
    }

    #[test]
    fn sniff_checks_the_magic_number() {
        let ctx = SniffCtx::default();
        assert!(GzipLayer.sniff(&[0x1F, 0x8B, 0x08, 0], &ctx) > Confidence::HIGH);
        assert_eq!(GzipLayer.sniff(&[0x1F, 0x8B], &ctx), Confidence::NONE);
        assert_eq!(GzipLayer.sniff(b"", &ctx), Confidence::NONE);
    }

    #[test]
    fn bad_parameters_are_rejected() {
        let mut params = Params::new();
        params.set("level", 10_u32);
        assert_eq!(
            GzipLayer.encode(b"x", &params).unwrap_err().code(),
            codes::CORE_INVALID_PARAM
        );
        let mut params = Params::new();
        params.set("fname", b"a\0b".to_vec());
        assert_eq!(
            GzipLayer.encode(b"x", &params).unwrap_err().code(),
            LAY_ENCODE_FAILED
        );
    }
}
