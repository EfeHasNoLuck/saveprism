//! The LibLZF layer (`compression.lzf`), raw and framed.
//!
//! Format: the public liblzf `lzf.h` description (BSD, `http://cvs.schmorp.de/liblzf/lzf.h`)
//! and the `lzf` utility "ZV" block stream used by liblzf front ends (documented by the ISC
//! `lzf-rust` project, `https://github.com/xorgy/lzf-rust`). A raw stream is a sequence of
//! tokens:
//!
//! - control byte `0..=31`: a literal run of `control + 1` bytes follows;
//! - control byte `32..=255`: a back reference. `length = control >> 5` (if that is 7, one extra
//!   byte is added), the first byte of the offset is `control & 0x1F`, the next byte is the low
//!   offset byte, the distance is that 13-bit value plus one, and the copied length is
//!   `length + 2` (3 to 264 bytes). Copies may overlap.
//!
//! The framed variant is a sequence of blocks: `Z` `V`, type `0` (uncompressed) or `1`
//! (compressed), a big-endian `u16` uncompressed length and, for type 1, a big-endian `u16`
//! compressed length before the payload. A zero byte ends the stream. A block is never larger
//! than 65535 bytes.
//!
//! The coder here is written from that description. The original bytes are kept so an unchanged
//! file is written back exactly.

use saveprism_core::limits::len_u64;
use saveprism_core::{Confidence, DecodeCtx, Error, Layer, Limits, Params, Result, SniffCtx};

use super::{LAY_LZF_INVALID, inner_is_known, keep_source, source_if_unchanged};

/// Longest back-reference length a token can encode.
const MAX_REF: usize = 264;
/// Largest back-reference distance.
const MAX_OFF: usize = 8192;
/// Largest literal run a token can encode.
const MAX_LITERAL: usize = 32;
/// Largest output `sniff` is willing to produce while proving a raw stream.
const MAX_SNIFF_OUTPUT: u64 = 64 * 1024 * 1024;
/// Hash table size for the compressor.
const HASH_SIZE: usize = 1 << 16;
/// Default block size when a framed file is edited and rewritten.
const DEFAULT_BLOCK: usize = 32 * 1024;

/// `compression.lzf`.
///
/// Parameters: `framed` (bool), `block_size` (1..=65535, used only when framed) and the source
/// digest set by `decode`.
#[derive(Clone, Copy, Debug, Default)]
pub struct LzfLayer;

fn bad(message: impl Into<String>, offset: usize) -> Error {
    Error::new(LAY_LZF_INVALID, message).with_offset(len_u64(offset))
}

fn budget_error(budget: u64) -> Error {
    Error::new(
        saveprism_core::codes::CORE_LIMIT_DECODED,
        format!("the LZF stream produced more than {budget} bytes (possible decompression bomb)"),
    )
}

/// Decompresses a raw LZF stream, refusing more than `budget` output bytes.
fn decode_raw(input: &[u8], budget: u64) -> Result<Vec<u8>> {
    let mut out: Vec<u8> = Vec::new();
    let mut ip = 0_usize;
    while ip < input.len() {
        let Some(&control) = input.get(ip) else {
            return Err(bad("the LZF stream is cut short", ip));
        };
        ip = ip.saturating_add(1);
        if control < 32 {
            let run = usize::from(control).saturating_add(1);
            let end = ip.saturating_add(run);
            let Some(literals) = input.get(ip..end) else {
                return Err(bad("the LZF literal run is cut short", ip));
            };
            let next = len_u64(out.len().saturating_add(literals.len()));
            if next > budget {
                return Err(budget_error(budget));
            }
            out.extend_from_slice(literals);
            ip = end;
            continue;
        }
        let mut length = usize::from(control >> 5);
        let mut distance = usize::from(control & 0x1F) << 8;
        if length == 7 {
            let Some(&extra) = input.get(ip) else {
                return Err(bad("the LZF extra length byte is missing", ip));
            };
            length = length.saturating_add(usize::from(extra));
            ip = ip.saturating_add(1);
        }
        let Some(&low) = input.get(ip) else {
            return Err(bad("the LZF back-reference offset is cut short", ip));
        };
        distance = distance.saturating_add(usize::from(low)).saturating_add(1);
        ip = ip.saturating_add(1);
        length = length.saturating_add(2);
        if distance > out.len() {
            return Err(bad(
                "the LZF back-reference points before the output start",
                ip,
            ));
        }
        let next = len_u64(out.len().saturating_add(length));
        if next > budget {
            return Err(budget_error(budget));
        }
        for _ in 0..length {
            let Some(index) = out.len().checked_sub(distance) else {
                return Err(bad(
                    "the LZF back-reference points before the output start",
                    ip,
                ));
            };
            let Some(&byte) = out.get(index) else {
                return Err(bad("the LZF back-reference is invalid", ip));
            };
            out.push(byte);
        }
    }
    Ok(out)
}

fn be_u16(input: &[u8], at: usize) -> Option<usize> {
    let bytes = input.get(at..at.checked_add(2)?)?;
    let pair = <[u8; 2]>::try_from(bytes).ok()?;
    Some(usize::from(u16::from_be_bytes(pair)))
}

/// The decoded bytes and the largest uncompressed block size of a framed stream.
struct Framed {
    data: Vec<u8>,
    block_size: usize,
}

/// Decompresses a `ZV` block stream.
fn decode_framed(input: &[u8], budget: u64) -> Result<Framed> {
    let mut out: Vec<u8> = Vec::new();
    let mut ip = 0_usize;
    let mut block_size = 0_usize;
    while let Some(&first) = input.get(ip) {
        if first == 0 {
            break;
        }
        let Some(magic) = input.get(ip..ip.saturating_add(2)) else {
            return Err(bad("the LZF block header is cut short", ip));
        };
        if magic != b"ZV" {
            return Err(bad("missing the LZF block magic ZV", ip));
        }
        let Some(&kind) = input.get(ip.saturating_add(2)) else {
            return Err(bad("the LZF block type is missing", ip));
        };
        match kind {
            0 => {
                let Some(length) = be_u16(input, ip.saturating_add(3)) else {
                    return Err(bad("the LZF block header is cut short", ip));
                };
                let start = ip.saturating_add(5);
                let end = start.saturating_add(length);
                let Some(block) = input.get(start..end) else {
                    return Err(bad("the LZF uncompressed block is cut short", start));
                };
                let next = len_u64(out.len().saturating_add(block.len()));
                if next > budget {
                    return Err(budget_error(budget));
                }
                out.extend_from_slice(block);
                block_size = block_size.max(length);
                ip = end;
            }
            1 => {
                let Some(compressed_len) = be_u16(input, ip.saturating_add(3)) else {
                    return Err(bad("the LZF block header is cut short", ip));
                };
                let Some(uncompressed_len) = be_u16(input, ip.saturating_add(5)) else {
                    return Err(bad("the LZF block header is cut short", ip));
                };
                let start = ip.saturating_add(7);
                let end = start.saturating_add(compressed_len);
                let Some(block) = input.get(start..end) else {
                    return Err(bad("the LZF compressed block is cut short", start));
                };
                let remaining = budget.saturating_sub(len_u64(out.len()));
                let inner = decode_raw(block, remaining.min(len_u64(uncompressed_len)))?;
                if inner.len() != uncompressed_len {
                    return Err(bad(
                        "the LZF block length does not match the decoded data",
                        start,
                    ));
                }
                out.extend_from_slice(&inner);
                block_size = block_size.max(uncompressed_len);
                ip = end;
            }
            other => {
                return Err(bad(
                    format!("unknown LZF block type {other}"),
                    ip.saturating_add(2),
                ));
            }
        }
    }
    Ok(Framed {
        data: out,
        block_size,
    })
}

fn sniff_limits() -> Limits {
    Limits {
        max_decoded_bytes: MAX_SNIFF_OUTPUT,
        ratio_floor_bytes: MAX_SNIFF_OUTPUT,
        max_ratio: u64::MAX,
        ..Limits::default()
    }
}

/// Emits a literal run of at most 32 bytes, then the next one.
fn emit_literals(out: &mut Vec<u8>, pending: &mut Vec<u8>) {
    while !pending.is_empty() {
        let take = pending.len().min(MAX_LITERAL);
        let control = u8::try_from(take.saturating_sub(1)).unwrap_or(0);
        out.push(control);
        if let Some(part) = pending.get(..take) {
            out.extend_from_slice(part);
        }
        pending.drain(..take);
    }
}

fn tri(data: &[u8], at: usize) -> Option<usize> {
    let a = *data.get(at)?;
    let b = *data.get(at.checked_add(1)?)?;
    let c = *data.get(at.checked_add(2)?)?;
    Some((usize::from(a) << 16) | (usize::from(b) << 8) | usize::from(c))
}

/// Compresses `data` as a raw LZF stream with a greedy hash of three-byte sequences.
pub(crate) fn compress_raw(data: &[u8]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    let mut pending: Vec<u8> = Vec::new();
    let mut table = vec![usize::MAX; HASH_SIZE];
    let mut pos = 0_usize;
    while pos < data.len() {
        let mut matched = 0_usize;
        if let Some(key) = tri(data, pos) {
            let slot = key & (HASH_SIZE - 1);
            if let Some(cell) = table.get_mut(slot) {
                let candidate = *cell;
                *cell = pos;
                if candidate != usize::MAX && pos > candidate {
                    let distance = pos.saturating_sub(candidate);
                    if distance <= MAX_OFF
                        && data.get(candidate..candidate.saturating_add(3))
                            == data.get(pos..pos.saturating_add(3))
                    {
                        let max = data.len().saturating_sub(pos).min(MAX_REF);
                        let mut length = 3_usize;
                        while length < max {
                            let left = data.get(candidate.saturating_add(length));
                            let right = data.get(pos.saturating_add(length));
                            if left.is_none() || left != right {
                                break;
                            }
                            length = length.saturating_add(1);
                        }
                        emit_literals(&mut out, &mut pending);
                        emit_back_reference(&mut out, distance, length);
                        matched = length;
                    }
                }
            }
        }
        if matched == 0 {
            if let Some(&byte) = data.get(pos) {
                pending.push(byte);
            }
            if pending.len() >= MAX_LITERAL {
                emit_literals(&mut out, &mut pending);
            }
            pos = pos.saturating_add(1);
        } else {
            pos = pos.saturating_add(matched);
        }
    }
    emit_literals(&mut out, &mut pending);
    out
}

fn emit_back_reference(out: &mut Vec<u8>, distance: usize, length: usize) {
    let stored = length.saturating_sub(2);
    let offset = distance.saturating_sub(1);
    let high = u8::try_from((offset >> 8) & 0x1F).unwrap_or(0);
    if stored < 7 {
        let control = u8::try_from(stored << 5).unwrap_or(0) | high;
        out.push(control);
    } else {
        out.push((7_u8 << 5) | high);
        out.push(u8::try_from(stored.saturating_sub(7)).unwrap_or(0));
    }
    out.push(u8::try_from(offset & 0xFF).unwrap_or(0));
}

/// Wraps `inner` as a `ZV` block stream with blocks of at most `block_size` bytes.
fn encode_framed(inner: &[u8], block_size: usize) -> Vec<u8> {
    let block_size = block_size.clamp(1, usize::from(u16::MAX));
    let mut out: Vec<u8> = Vec::new();
    for chunk in inner.chunks(block_size) {
        let compressed = compress_raw(chunk);
        let use_compressed = !chunk.is_empty()
            && compressed.len() < chunk.len()
            && u16::try_from(compressed.len()).is_ok()
            && u16::try_from(chunk.len()).is_ok();
        if use_compressed {
            out.extend_from_slice(b"ZV\x01");
            out.extend_from_slice(&u16::try_from(compressed.len()).unwrap_or(0).to_be_bytes());
            out.extend_from_slice(&u16::try_from(chunk.len()).unwrap_or(0).to_be_bytes());
            out.extend_from_slice(&compressed);
        } else {
            out.extend_from_slice(b"ZV\x00");
            out.extend_from_slice(&u16::try_from(chunk.len()).unwrap_or(0).to_be_bytes());
            out.extend_from_slice(chunk);
        }
    }
    out
}

impl Layer for LzfLayer {
    fn id(&self) -> &'static str {
        "compression.lzf"
    }

    fn sniff(&self, input: &[u8], _ctx: &SniffCtx<'_>) -> Confidence {
        if input.len() < 2 {
            return Confidence::NONE;
        }
        if len_u64(input.len()) > MAX_SNIFF_OUTPUT {
            return Confidence::NONE;
        }
        let limits = sniff_limits();
        let budget = limits.output_budget(len_u64(input.len()));
        let framed = input.starts_with(b"ZV");
        let decoded = if framed {
            decode_framed(input, budget).map(|framed| framed.data)
        } else {
            decode_raw(input, budget)
        };
        match decoded {
            Ok(inner) if !inner.is_empty() && inner_is_known(&inner) => {
                Confidence::new(if framed { 0.55 } else { 0.42 })
            }
            _ => Confidence::NONE,
        }
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<(Vec<u8>, Params)> {
        let budget = ctx.output_budget(len_u64(input.len()));
        if input.starts_with(b"ZV")
            && let Ok(framed) = decode_framed(input, budget)
        {
            let mut params = Params::new();
            params.set("framed", true);
            params.set(
                "block_size",
                i64::try_from(framed.block_size.clamp(1, usize::from(u16::MAX))).unwrap_or(1),
            );
            keep_source(&mut params, input, &framed.data);
            return Ok((framed.data, params));
        }
        let inner = decode_raw(input, budget)?;
        let mut params = Params::new();
        params.set("framed", false);
        keep_source(&mut params, input, &inner);
        Ok((inner, params))
    }

    fn encode(&self, inner: &[u8], params: &Params) -> Result<Vec<u8>> {
        if let Some(source) = source_if_unchanged(inner, params)? {
            return Ok(source);
        }
        let framed = params.opt_bool("framed")?.unwrap_or(false);
        if !framed {
            return Ok(compress_raw(inner));
        }
        let block_size = match params.opt_int("block_size")? {
            None => DEFAULT_BLOCK,
            Some(value) => usize::try_from(value)
                .ok()
                .filter(|value| (1..=usize::from(u16::MAX)).contains(value))
                .ok_or_else(|| {
                    super::invalid_param(
                        "parameter \"block_size\" must be between 1 and 65535".to_string(),
                    )
                })?,
        };
        Ok(encode_framed(inner, block_size))
    }
}

#[cfg(test)]
mod tests {
    use saveprism_core::{Limits, codes};

    use super::*;

    const JSON: &[u8] = b"{\"player\":{\"name\":\"Ada\",\"level\":12,\"gold\":340},\"day\":3}";

    fn decode(input: &[u8]) -> Result<(Vec<u8>, Params)> {
        let limits = Limits::default();
        let mut ctx = DecodeCtx::new(&limits);
        LzfLayer.decode(input, &mut ctx)
    }

    fn raw_fixture() -> Vec<u8> {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/synthetic/lzf/save.json.lzf");
        std::fs::read(path).unwrap()
    }

    fn framed_fixture() -> Vec<u8> {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/synthetic/lzf/save.json.zv");
        std::fs::read(path).unwrap()
    }

    #[test]
    fn hand_made_literal_and_back_reference_decode() {
        // Literal run of one byte 'a', then a back-reference of length 3 at distance 1.
        assert_eq!(
            decode_raw(&[0x00, b'a', 0x20, 0x00], 1024).unwrap(),
            b"aaaa"
        );
        // Literal run of three bytes.
        assert_eq!(decode_raw(&[0x02, b'a', b'b', b'c'], 1024).unwrap(), b"abc");
    }

    #[test]
    fn raw_fixture_round_trips_exactly() {
        let bytes = raw_fixture();
        let (inner, params) = decode(&bytes).unwrap();
        assert_eq!(inner, JSON);
        assert!(!params.bool("framed").unwrap());
        assert_eq!(LzfLayer.encode(&inner, &params).unwrap(), bytes);
    }

    #[test]
    fn framed_fixture_round_trips_exactly() {
        let bytes = framed_fixture();
        let (inner, params) = decode(&bytes).unwrap();
        assert_eq!(inner, JSON);
        assert!(params.bool("framed").unwrap());
        assert_eq!(LzfLayer.encode(&inner, &params).unwrap(), bytes);
    }

    #[test]
    fn edited_content_is_compressed_again() {
        for bytes in [raw_fixture(), framed_fixture()] {
            let (mut inner, params) = decode(&bytes).unwrap();
            inner.push(b'\n');
            let written = LzfLayer.encode(&inner, &params).unwrap();
            let (again, _) = decode(&written).unwrap();
            assert_eq!(again, inner);
        }
    }

    #[test]
    fn bomb_is_stopped_by_the_budget() {
        let bomb = vec![0_u8; 10 * 1024 * 1024];
        let packed = compress_raw(&bomb);
        let limits = Limits {
            max_decoded_bytes: 1024 * 1024,
            ..Limits::default()
        };
        let mut ctx = DecodeCtx::new(&limits);
        assert_eq!(
            LzfLayer.decode(&packed, &mut ctx).unwrap_err().code(),
            codes::CORE_LIMIT_DECODED
        );
    }

    #[test]
    fn damaged_input_gives_the_code() {
        assert_eq!(decode(&[0x05, b'a']).unwrap_err().code(), LAY_LZF_INVALID);
        assert_eq!(decode(&[0x20, 0x00]).unwrap_err().code(), LAY_LZF_INVALID);
        assert_eq!(
            decode(b"ZV\x01\x00\x09\x00\x01x").unwrap_err().code(),
            LAY_LZF_INVALID
        );
    }

    #[test]
    fn sniff_finds_raw_and_framed_by_content() {
        let ctx = SniffCtx::default();
        assert!(LzfLayer.sniff(&raw_fixture(), &ctx) > Confidence::NONE);
        assert!(LzfLayer.sniff(&framed_fixture(), &ctx) > Confidence::MEDIUM);
        assert_eq!(LzfLayer.sniff(b"", &ctx), Confidence::NONE);
    }

    #[test]
    fn compressor_emits_valid_back_references() {
        let data = b"abcabcabcabcabcabcabcabc";
        let packed = compress_raw(data);
        assert!(packed.len() < data.len());
        assert_eq!(decode_raw(&packed, 1024).unwrap(), data);
    }
}
