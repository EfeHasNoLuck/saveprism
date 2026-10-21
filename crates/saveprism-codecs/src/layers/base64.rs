//! The base64 layer. Written by hand (a few dozen lines) so that the exact text layout can be
//! recorded: alphabet, padding, line wrapping, line ending and a final line ending.

use saveprism_core::limits::len_u64;
use saveprism_core::{Confidence, DecodeCtx, Error, Layer, Params, Result, SniffCtx};

use super::{LAY_BASE64_INVALID, invalid_param};

const STANDARD: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const URL_SAFE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// How many leading bytes `sniff` looks at.
const SNIFF_BYTES: usize = 4096;

/// `encoding.base64`.
///
/// Parameters: `alphabet` (`"standard"` or `"url"`), `pad` (bool), `wrap` (line length, 0 = one
/// line), `newline` (`"lf"` or `"crlf"`), `trailing_newline` (bool) and `irregular` (bool, set on
/// decode when the text had an odd layout; `encode` always writes the canonical layout).
#[derive(Clone, Copy, Debug, Default)]
pub struct Base64Layer;

fn bad(message: impl Into<String>, offset: usize) -> Error {
    Error::new(LAY_BASE64_INVALID, message).with_offset(len_u64(offset))
}

fn is_whitespace(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\r' | b'\n')
}

/// The 6-bit value of a character of either alphabet.
fn sextet(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' | b'-' => Some(62),
        b'/' | b'_' => Some(63),
        _ => None,
    }
}

/// How the text was laid out.
struct Layout {
    wrap: usize,
    crlf: bool,
    trailing_newline: bool,
    irregular: bool,
}

fn analyze_layout(input: &[u8]) -> Layout {
    let mut rest = input;
    let mut trailing_newline = false;
    let mut trailing_crlf = false;
    if let Some(stripped) = rest.strip_suffix(b"\r\n") {
        rest = stripped;
        trailing_newline = true;
        trailing_crlf = true;
    } else if let Some(stripped) = rest.strip_suffix(b"\n") {
        rest = stripped;
        trailing_newline = true;
    }

    let mut lines: Vec<&[u8]> = Vec::new();
    let mut crlf_seen = false;
    let mut lf_seen = false;
    let mut start = 0_usize;
    for (index, &byte) in rest.iter().enumerate() {
        if byte != b'\n' {
            continue;
        }
        let mut end = index;
        let has_cr = index > start && rest.get(index - 1) == Some(&b'\r');
        if has_cr {
            end = index - 1;
            crlf_seen = true;
        } else {
            lf_seen = true;
        }
        lines.push(rest.get(start..end).unwrap_or_default());
        start = index + 1;
    }
    lines.push(rest.get(start..).unwrap_or_default());

    let mut irregular = lines
        .iter()
        .any(|line| line.iter().any(|&b| matches!(b, b' ' | b'\t' | b'\r')));
    if crlf_seen && lf_seen {
        irregular = true;
    }
    if trailing_newline && ((crlf_seen && !trailing_crlf) || (lf_seen && trailing_crlf)) {
        irregular = true;
    }
    let crlf = if crlf_seen || lf_seen {
        crlf_seen && !lf_seen
    } else {
        trailing_crlf
    };

    let mut wrap = 0_usize;
    if lines.len() > 1 {
        let first = lines.first().map_or(0, |line| line.len());
        let last = lines.last().map_or(0, |line| line.len());
        let body_ok = lines
            .get(..lines.len() - 1)
            .is_some_and(|body| body.iter().all(|line| line.len() == first));
        if first > 0 && body_ok && (1..=first).contains(&last) {
            wrap = first;
        } else {
            irregular = true;
        }
    }

    Layout {
        wrap,
        crlf,
        trailing_newline,
        irregular,
    }
}

fn encode_group(data: &[u8], table: &[u8; 64], pad: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len().div_ceil(3).saturating_mul(4));
    for chunk in data.chunks(3) {
        let b0 = chunk.first().copied().unwrap_or(0);
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);
        let n = (u32::from(b0) << 16) | (u32::from(b1) << 8) | u32::from(b2);
        let symbol = |shift: u32| -> u8 {
            let index = usize::try_from((n >> shift) & 63).unwrap_or(0);
            table.get(index).copied().unwrap_or(b'A')
        };
        out.push(symbol(18));
        out.push(symbol(12));
        if chunk.len() > 1 {
            out.push(symbol(6));
        } else if pad {
            out.push(b'=');
        }
        if chunk.len() > 2 {
            out.push(symbol(0));
        } else if pad {
            out.push(b'=');
        }
    }
    out
}

impl Layer for Base64Layer {
    fn id(&self) -> &'static str {
        "encoding.base64"
    }

    fn sniff(&self, input: &[u8], _ctx: &SniffCtx<'_>) -> Confidence {
        let head = input
            .get(..SNIFF_BYTES.min(input.len()))
            .unwrap_or_default();
        let plausible = head
            .iter()
            .all(|&b| is_whitespace(b) || b == b'=' || sextet(b).is_some());
        if !plausible {
            return Confidence::NONE;
        }
        let significant = input
            .iter()
            .filter(|&&b| !is_whitespace(b) && b != b'=')
            .count();
        if significant == 0 || significant & 3 == 1 {
            return Confidence::NONE;
        }
        Confidence::MEDIUM
    }

    fn decode(&self, input: &[u8], _ctx: &mut DecodeCtx<'_>) -> Result<(Vec<u8>, Params)> {
        let layout = analyze_layout(input);
        let mut irregular = layout.irregular;

        let mut values: Vec<u8> = Vec::with_capacity(input.len());
        let mut padding = 0_usize;
        let mut uses_standard = false;
        let mut uses_url = false;
        for (position, &byte) in input.iter().enumerate() {
            if is_whitespace(byte) {
                continue;
            }
            if byte == b'=' {
                padding += 1;
                continue;
            }
            if padding > 0 {
                return Err(bad("data follows the padding", position));
            }
            match byte {
                b'+' | b'/' => uses_standard = true,
                b'-' | b'_' => uses_url = true,
                _ => {}
            }
            let value = sextet(byte).ok_or_else(|| {
                bad(
                    format!("byte 0x{byte:02X} is not a base64 character"),
                    position,
                )
            })?;
            values.push(value);
        }
        if uses_standard && uses_url {
            return Err(bad(
                "the text mixes the standard and the URL-safe alphabet",
                0,
            ));
        }
        if padding > 2 {
            return Err(bad("more than two padding characters", 0));
        }
        let count = values.len();
        if count % 4 == 1 {
            return Err(bad(
                format!("{count} base64 characters cannot be a whole number of bytes"),
                input.len(),
            ));
        }
        if padding > 0 && (count + padding) & 3 != 0 {
            return Err(bad(
                "the padding does not complete the last group of four",
                input.len(),
            ));
        }

        let mut out = Vec::with_capacity(count / 4 * 3 + 3);
        for group in values.chunks(4) {
            let mut n = 0_u32;
            for index in 0..4 {
                n = (n << 6) | u32::from(group.get(index).copied().unwrap_or(0));
            }
            out.push(((n >> 16) & 0xFF) as u8);
            if group.len() > 2 {
                out.push(((n >> 8) & 0xFF) as u8);
            }
            if group.len() > 3 {
                out.push((n & 0xFF) as u8);
            }
            // Unused low bits must be zero, otherwise writing the text back changes it.
            let leftover = match group.len() {
                2 => n & 0xFFFF,
                3 => n & 0xFF,
                _ => 0,
            };
            if leftover != 0 {
                irregular = true;
            }
        }

        let mut params = Params::new();
        params.set("alphabet", if uses_url { "url" } else { "standard" });
        params.set("pad", padding > 0);
        params.set("wrap", i64::try_from(layout.wrap).unwrap_or(0));
        params.set("newline", if layout.crlf { "crlf" } else { "lf" });
        params.set("trailing_newline", layout.trailing_newline);
        params.set("irregular", irregular);
        Ok((out, params))
    }

    fn encode(&self, inner: &[u8], params: &Params) -> Result<Vec<u8>> {
        let table = match params.opt_str("alphabet")?.unwrap_or("standard") {
            "standard" => STANDARD,
            "url" => URL_SAFE,
            other => {
                return Err(invalid_param(format!(
                    "parameter \"alphabet\" must be \"standard\" or \"url\", not \"{other}\""
                )));
            }
        };
        let pad = params.opt_bool("pad")?.unwrap_or(true);
        let wrap = match params.opt_int("wrap")? {
            None => 0,
            Some(value) => usize::try_from(value).map_err(|_| {
                invalid_param("parameter \"wrap\" must not be negative".to_string())
            })?,
        };
        let newline: &[u8] = match params.opt_str("newline")?.unwrap_or("lf") {
            "lf" => b"\n",
            "crlf" => b"\r\n",
            other => {
                return Err(invalid_param(format!(
                    "parameter \"newline\" must be \"lf\" or \"crlf\", not \"{other}\""
                )));
            }
        };
        let trailing_newline = params.opt_bool("trailing_newline")?.unwrap_or(false);

        let text = encode_group(inner, table, pad);
        let mut out = Vec::with_capacity(text.len() + text.len() / 32 + 4);
        if wrap == 0 {
            out.extend_from_slice(&text);
        } else {
            for (index, line) in text.chunks(wrap).enumerate() {
                if index > 0 {
                    out.extend_from_slice(newline);
                }
                out.extend_from_slice(line);
            }
        }
        if trailing_newline {
            out.extend_from_slice(newline);
        }
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
        Base64Layer.decode(input, &mut ctx)
    }

    fn round_trip(input: &[u8]) -> (Vec<u8>, Params) {
        let (inner, params) = decode(input).unwrap();
        let again = Base64Layer.encode(&inner, &params).unwrap();
        assert_eq!(again, input, "round trip changed the text");
        (inner, params)
    }

    #[test]
    fn standard_padded() {
        let (inner, params) = round_trip(b"SGVsbG8sIFdvcmxkIQ==");
        assert_eq!(inner, b"Hello, World!");
        assert_eq!(params.str("alphabet").unwrap(), "standard");
        assert!(params.bool("pad").unwrap());
        assert_eq!(params.int("wrap").unwrap(), 0);
        assert!(!params.bool("irregular").unwrap());
    }

    #[test]
    fn all_lengths_round_trip() {
        for length in 0..40_u8 {
            let data: Vec<u8> = (0..length).map(|i| i.wrapping_mul(37)).collect();
            for pad in [true, false] {
                let mut params = Params::new();
                params.set("pad", pad);
                let text = Base64Layer.encode(&data, &params).unwrap();
                let (inner, decoded) = round_trip(&text);
                assert_eq!(inner, data);
                if length % 3 > 0 {
                    assert_eq!(decoded.bool("pad").unwrap(), pad);
                }
            }
        }
    }

    #[test]
    fn url_alphabet() {
        let (inner, params) = round_trip(b"-__-");
        assert_eq!(inner, [0xFB, 0xFF, 0xFE]);
        assert_eq!(params.str("alphabet").unwrap(), "url");
        let (inner, params) = round_trip(b"+//+");
        assert_eq!(inner, [0xFB, 0xFF, 0xFE]);
        assert_eq!(params.str("alphabet").unwrap(), "standard");
    }

    #[test]
    fn unpadded() {
        let (inner, params) = round_trip(b"SGVsbG8");
        assert_eq!(inner, b"Hello");
        assert!(!params.bool("pad").unwrap());
    }

    #[test]
    fn wrapped_at_76_with_crlf_and_trailing_newline() {
        let data: Vec<u8> = (0..200_u8).collect();
        let plain = Base64Layer.encode(&data, &Params::new()).unwrap();
        let mut text = Vec::new();
        for (index, line) in plain.chunks(76).enumerate() {
            if index > 0 {
                text.extend_from_slice(b"\r\n");
            }
            text.extend_from_slice(line);
        }
        text.extend_from_slice(b"\r\n");

        let (inner, params) = round_trip(&text);
        assert_eq!(inner, data);
        assert_eq!(params.int("wrap").unwrap(), 76);
        assert_eq!(params.str("newline").unwrap(), "crlf");
        assert!(params.bool("trailing_newline").unwrap());
        assert!(!params.bool("irregular").unwrap());
    }

    #[test]
    fn wrapped_with_lf_and_no_trailing_newline() {
        let text = b"QUJDREVG\nR0hJSktM\nTU4=";
        let (inner, params) = round_trip(text);
        assert_eq!(inner, b"ABCDEFGHIJKLMN");
        assert_eq!(params.int("wrap").unwrap(), 8);
        assert_eq!(params.str("newline").unwrap(), "lf");
        assert!(!params.bool("trailing_newline").unwrap());
    }

    #[test]
    fn trailing_newline_only() {
        let (inner, params) = round_trip(b"QUJD\n");
        assert_eq!(inner, b"ABC");
        assert_eq!(params.int("wrap").unwrap(), 0);
        assert!(params.bool("trailing_newline").unwrap());
    }

    #[test]
    fn irregular_whitespace_decodes_but_is_flagged() {
        let (inner, params) = decode(b"QUJD REVG").unwrap();
        assert_eq!(inner, b"ABCDEF");
        assert!(params.bool("irregular").unwrap());
        // The canonical text is written, not the original.
        assert_eq!(Base64Layer.encode(&inner, &params).unwrap(), b"QUJDREVG");

        // Ragged line lengths.
        let (_, params) = decode(b"QUJDRE\nVG\nR0hJKLTU").unwrap();
        assert!(params.bool("irregular").unwrap());
    }

    #[test]
    fn non_canonical_trailing_bits_are_flagged() {
        let (inner, params) = decode(b"QR==").unwrap();
        assert_eq!(inner, b"A");
        assert!(params.bool("irregular").unwrap());
    }

    #[test]
    fn invalid_input_gives_the_base64_code() {
        for input in [
            &b"QUJD*"[..],
            b"QUJDR",
            b"QU=JD",
            b"QUJD===",
            b"QU=",
            b"+-+-",
            b"\xFF\xFE",
        ] {
            let error = decode(input).unwrap_err();
            assert_eq!(error.code(), LAY_BASE64_INVALID, "{input:?}");
        }
        assert!(decode(b"QUJD*").unwrap_err().offset().is_some());
    }

    #[test]
    fn empty_input_is_empty_data() {
        let (inner, _) = round_trip(b"");
        assert!(inner.is_empty());
    }

    #[test]
    fn bad_parameters_are_rejected() {
        for (key, value) in [("alphabet", "rot13"), ("newline", "cr")] {
            let mut params = Params::new();
            params.set(key, value);
            assert_eq!(
                Base64Layer.encode(b"x", &params).unwrap_err().code(),
                codes::CORE_INVALID_PARAM
            );
        }
        let mut params = Params::new();
        params.set("wrap", -1_i64);
        assert_eq!(
            Base64Layer.encode(b"x", &params).unwrap_err().code(),
            codes::CORE_INVALID_PARAM
        );
    }

    #[test]
    fn sniff_accepts_base64_text_only() {
        let ctx = SniffCtx::default();
        assert_eq!(
            Base64Layer.sniff(b"SGVsbG8sIFdvcmxkIQ==", &ctx),
            Confidence::MEDIUM
        );
        assert_eq!(Base64Layer.sniff(b"{\"a\": 1}", &ctx), Confidence::NONE);
        assert_eq!(Base64Layer.sniff(b"QUJDR", &ctx), Confidence::NONE);
        assert_eq!(Base64Layer.sniff(b"", &ctx), Confidence::NONE);
        assert_eq!(
            Base64Layer.sniff(&[0x1F, 0x8B, 0x08], &ctx),
            Confidence::NONE
        );
    }
}
