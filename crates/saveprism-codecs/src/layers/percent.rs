//! The percent-encoded text layer (`%XX` escapes).
//!
//! Percent-encoding follows RFC 3986 section 2.1 (`%` and two hex digits).
//!
//! A pasted save code may be URL-encoded. Only printable ASCII with at least one `%XX` escape is
//! accepted, and only when the bytes it spells are recognised by a registered codec or layer, so
//! ordinary text is never claimed. Unlike `encoding.url-escape` (JavaScript `escape()` /
//! `encodeURIComponent` with a JSON hint) this layer works on arbitrary bytes.
//!
//! The original text is kept, so an unchanged code is written back exactly. After an edit, bytes
//! that were literal stay literal and every other byte becomes `%XX` in the recorded hex case.

use saveprism_core::limits::len_u64;
use saveprism_core::{Confidence, DecodeCtx, Error, Layer, Params, Result, SniffCtx};

use super::{LAY_PERCENT_INVALID, inner_is_known, keep_source, source_if_unchanged};

/// Largest input that `sniff` is willing to decode for the recognition check.
const MAX_SNIFF: usize = 8 * 1024 * 1024;

/// `encoding.percent`.
///
/// Parameters: `hex` (`"upper"` or `"lower"`), `raw` (bytes that were literal) and the source
/// digest set by `decode`.
#[derive(Clone, Copy, Debug, Default)]
pub struct PercentLayer;

fn bad(message: impl Into<String>, offset: usize) -> Error {
    Error::new(LAY_PERCENT_INVALID, message).with_offset(len_u64(offset))
}

fn nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Decoded bytes, the literal bytes seen, and whether lower/upper hex letters were used.
struct Decoded {
    bytes: Vec<u8>,
    raw: Vec<u8>,
    lower: bool,
    upper: bool,
}

fn is_plausible(input: &[u8]) -> bool {
    input.iter().all(|&byte| (0x20..0x7F).contains(&byte)) && input.contains(&b'%')
}

fn decode_text(input: &[u8]) -> Option<Decoded> {
    if !is_plausible(input) {
        return None;
    }
    let mut bytes = Vec::with_capacity(input.len());
    let mut raw = Vec::new();
    let mut lower = false;
    let mut upper = false;
    let mut index = 0_usize;
    while let Some(&byte) = input.get(index) {
        if byte != b'%' {
            bytes.push(byte);
            if !raw.contains(&byte) {
                raw.push(byte);
            }
            index = index.saturating_add(1);
            continue;
        }
        let first = *input.get(index.saturating_add(1))?;
        let second = *input.get(index.saturating_add(2))?;
        if first.is_ascii_lowercase() || second.is_ascii_lowercase() {
            lower = true;
        }
        if first.is_ascii_uppercase() || second.is_ascii_uppercase() {
            upper = true;
        }
        bytes.push((nibble(first)? << 4) | nibble(second)?);
        index = index.saturating_add(3);
    }
    raw.sort_unstable();
    raw.dedup();
    (!bytes.is_empty()).then_some(Decoded {
        bytes,
        raw,
        lower,
        upper,
    })
}

fn has_raw(raw: &[u8], byte: u8) -> bool {
    raw.binary_search(&byte).is_ok()
}

impl Layer for PercentLayer {
    fn id(&self) -> &'static str {
        "encoding.percent"
    }

    fn sniff(&self, input: &[u8], _ctx: &SniffCtx<'_>) -> Confidence {
        if !is_plausible(input) {
            return Confidence::NONE;
        }
        // A JSON save code belongs to `encoding.url-escape`, which records the JavaScript style.
        if input.starts_with(b"%7B")
            || input.starts_with(b"%7b")
            || input.starts_with(b"%5B")
            || input.starts_with(b"%5b")
        {
            return Confidence::NONE;
        }
        if input.len() > MAX_SNIFF {
            return Confidence::LOW;
        }
        match decode_text(input) {
            Some(decoded) if inner_is_known(&decoded.bytes) => Confidence::new(0.55),
            _ => Confidence::NONE,
        }
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<(Vec<u8>, Params)> {
        let decoded = decode_text(input).ok_or_else(|| {
            bad(
                "the text is not printable ASCII with complete %XX escapes",
                0,
            )
        })?;
        if !inner_is_known(&decoded.bytes) {
            return Err(bad(
                "the percent escapes do not spell a format SavePrism knows",
                0,
            ));
        }
        ctx.limits()
            .check_output(len_u64(input.len()), len_u64(decoded.bytes.len()))?;
        let mut params = Params::new();
        params.set(
            "hex",
            if decoded.lower && !decoded.upper {
                "lower"
            } else {
                "upper"
            },
        );
        params.set("raw", decoded.raw.clone());
        keep_source(&mut params, input, &decoded.bytes);
        Ok((decoded.bytes, params))
    }

    fn encode(&self, inner: &[u8], params: &Params) -> Result<Vec<u8>> {
        if let Some(source) = source_if_unchanged(inner, params)? {
            return Ok(source);
        }
        let lower = match params.opt_str("hex")? {
            Some("lower") => true,
            None | Some("upper") => false,
            Some(other) => {
                return Err(super::invalid_param(format!(
                    "parameter \"hex\" must be \"upper\" or \"lower\", not \"{other}\""
                )));
            }
        };
        let mut raw = params.opt_bytes("raw")?.unwrap_or_default().to_vec();
        raw.sort_unstable();
        raw.dedup();
        let table: &[u8; 16] = if lower {
            b"0123456789abcdef"
        } else {
            b"0123456789ABCDEF"
        };
        let mut out = Vec::with_capacity(inner.len().saturating_mul(3));
        for &byte in inner {
            if has_raw(&raw, byte) {
                out.push(byte);
            } else {
                out.push(b'%');
                out.push(table.get(usize::from(byte >> 4)).copied().unwrap_or(b'0'));
                out.push(table.get(usize::from(byte & 0x0F)).copied().unwrap_or(b'0'));
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use saveprism_core::{Limits, codes};

    use super::*;

    fn fixture() -> Vec<u8> {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/synthetic/layers/save.json.pct");
        std::fs::read(path).unwrap()
    }

    fn decode(input: &[u8]) -> Result<(Vec<u8>, Params)> {
        let limits = Limits::default();
        let mut ctx = DecodeCtx::new(&limits);
        PercentLayer.decode(input, &mut ctx)
    }

    #[test]
    fn fixture_round_trips_exactly() {
        let bytes = fixture();
        let (inner, params) = decode(&bytes).unwrap();
        // The fixture is a percent-encoded gzip stream around the JSON save.
        assert_eq!(inner.get(..2), Some(&[0x1F, 0x8B][..]));
        assert_eq!(PercentLayer.encode(&inner, &params).unwrap(), bytes);
    }

    #[test]
    fn literal_bytes_stay_literal_after_an_edit() {
        let (inner, params) = decode(b"%7B%22abc%20def%21%22%3A1%7D").unwrap();
        assert_eq!(inner, b"{\"abc def!\":1}");
        assert_eq!(params.bytes("raw").unwrap(), b"1abcdef");
        assert_eq!(
            PercentLayer.encode(b"{\"abc def? \":1}", &params).unwrap(),
            b"%7B%22abc%20def%3F%20%22%3A1%7D"
        );
    }

    #[test]
    fn lower_case_hex_is_recorded() {
        let (_, params) = decode(b"%7b%22a%22%3a1%7d").unwrap();
        assert_eq!(params.str("hex").unwrap(), "lower");
    }

    #[test]
    fn text_that_spells_an_unknown_format_is_refused() {
        for input in [&b"%41"[..], b"%DE%AD", b"hello%21world"] {
            assert_eq!(
                decode(input).unwrap_err().code(),
                LAY_PERCENT_INVALID,
                "{input:?}"
            );
        }
    }

    #[test]
    fn malformed_text_gives_the_code() {
        for input in [&b"%"[..], b"%7", b"%zz", b"no escapes", b"%00\x01"] {
            assert_eq!(
                decode(input).unwrap_err().code(),
                LAY_PERCENT_INVALID,
                "{input:?}"
            );
        }
    }

    #[test]
    fn sniff_only_accepts_escapes_that_lead_to_a_known_format() {
        let ctx = SniffCtx::default();
        let bytes = fixture();
        assert!(PercentLayer.sniff(&bytes, &ctx) > Confidence::MEDIUM);
        assert_eq!(PercentLayer.sniff(b"%41", &ctx), Confidence::NONE);
        assert_eq!(PercentLayer.sniff(b"plain text", &ctx), Confidence::NONE);
    }

    #[test]
    fn bad_parameters_are_rejected() {
        let mut params = Params::new();
        params.set("hex", "mixed");
        assert_eq!(
            PercentLayer.encode(b"x", &params).unwrap_err().code(),
            codes::CORE_INVALID_PARAM
        );
    }
}
