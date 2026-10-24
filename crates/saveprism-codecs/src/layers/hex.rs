//! The hex text layer: a pasted save code written as hexadecimal digits.
//!
//! Only text made of hex digits and whitespace is accepted, and only when the bytes it spells are
//! recognised by a registered codec or layer, so ordinary text is never claimed. The original text
//! is kept, so an unchanged code is written back exactly; after an edit the canonical contiguous
//! hex form is written (upper or lower case as in the file).

use saveprism_core::limits::len_u64;
use saveprism_core::{Confidence, DecodeCtx, Error, Layer, Params, Result, SniffCtx};

use super::{LAY_HEX_INVALID, inner_is_known, keep_source, source_if_unchanged};

/// Largest input that `sniff` is willing to decode for the recognition check.
const MAX_SNIFF: usize = 8 * 1024 * 1024;

/// `encoding.hex`.
///
/// Parameters: `hex` (`"upper"` or `"lower"`) and the source digest set by `decode`.
#[derive(Clone, Copy, Debug, Default)]
pub struct HexLayer;

fn bad(message: impl Into<String>, offset: usize) -> Error {
    Error::new(LAY_HEX_INVALID, message).with_offset(len_u64(offset))
}

fn nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn is_text_whitespace(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\r' | b'\n')
}

/// Decodes the digits; `None` when there is a non-hex byte or an odd number of digits.
fn decode_text(input: &[u8]) -> Option<(Vec<u8>, bool, bool)> {
    let mut out = Vec::with_capacity(input.len() / 2);
    let mut high: Option<u8> = None;
    let mut lower = false;
    let mut upper = false;
    for &byte in input {
        if is_text_whitespace(byte) {
            continue;
        }
        if byte.is_ascii_lowercase() {
            lower = true;
        } else if byte.is_ascii_uppercase() {
            upper = true;
        }
        let value = nibble(byte)?;
        match high {
            None => high = Some(value),
            Some(first) => {
                out.push((first << 4) | value);
                high = None;
            }
        }
    }
    if high.is_some() || out.is_empty() {
        None
    } else {
        Some((out, lower, upper))
    }
}

impl Layer for HexLayer {
    fn id(&self) -> &'static str {
        "encoding.hex"
    }

    fn sniff(&self, input: &[u8], _ctx: &SniffCtx<'_>) -> Confidence {
        if input.is_empty() {
            return Confidence::NONE;
        }
        if input.len() > MAX_SNIFF {
            // Too large to prove here; `decode` still checks the inner format.
            let plausible = input
                .iter()
                .all(|&byte| is_text_whitespace(byte) || nibble(byte).is_some());
            return if plausible {
                Confidence::LOW
            } else {
                Confidence::NONE
            };
        }
        match decode_text(input) {
            Some((inner, _, _)) if inner_is_known(&inner) => Confidence::new(0.62),
            _ => Confidence::NONE,
        }
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<(Vec<u8>, Params)> {
        let (inner, lower, upper) = decode_text(input)
            .ok_or_else(|| bad("the text is not a whole number of hex digits", 0))?;
        if !inner_is_known(&inner) {
            return Err(bad(
                "the hex text does not spell a format SavePrism knows",
                0,
            ));
        }
        ctx.limits()
            .check_output(len_u64(input.len()), len_u64(inner.len()))?;
        let mut params = Params::new();
        params.set("hex", if lower && !upper { "lower" } else { "upper" });
        keep_source(&mut params, input, &inner);
        Ok((inner, params))
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
        let table: &[u8; 16] = if lower {
            b"0123456789abcdef"
        } else {
            b"0123456789ABCDEF"
        };
        let mut out = Vec::with_capacity(inner.len().saturating_mul(2));
        for &byte in inner {
            out.push(table.get(usize::from(byte >> 4)).copied().unwrap_or(b'0'));
            out.push(table.get(usize::from(byte & 0x0F)).copied().unwrap_or(b'0'));
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
            .join("../../fixtures/synthetic/layers/save.json.hex");
        std::fs::read(path).unwrap()
    }

    fn decode(input: &[u8]) -> Result<(Vec<u8>, Params)> {
        let limits = Limits::default();
        let mut ctx = DecodeCtx::new(&limits);
        HexLayer.decode(input, &mut ctx)
    }

    #[test]
    fn fixture_round_trips_exactly() {
        let bytes = fixture();
        let (inner, params) = decode(&bytes).unwrap();
        assert!(inner.starts_with(b"{"));
        assert_eq!(HexLayer.encode(&inner, &params).unwrap(), bytes);
    }

    #[test]
    fn lower_case_is_recorded_and_used_after_an_edit() {
        let mut text = b"7b2261223a317d".to_vec();
        let (_, params) = decode(&text).unwrap();
        assert_eq!(params.str("hex").unwrap(), "lower");
        text.push(b'0');
        let edited = HexLayer.encode(b"{\"a\":1}\0", &params).unwrap();
        assert_eq!(edited, b"7b2261223a317d00");
    }

    #[test]
    fn whitespace_is_accepted_but_not_needed() {
        let (inner, params) = decode(b"7B 22 61 22 3A 31 7D\n").unwrap();
        assert_eq!(inner, b"{\"a\":1}");
        assert_eq!(params.str("hex").unwrap(), "upper");
    }

    #[test]
    fn text_that_spells_an_unknown_format_is_refused() {
        for input in [&b"deadbeef"[..], b"41", b"7B22"] {
            assert_eq!(
                decode(input).unwrap_err().code(),
                LAY_HEX_INVALID,
                "{input:?}"
            );
        }
    }

    #[test]
    fn malformed_text_gives_the_code() {
        for input in [&b"7G"[..], b"7", b"", b"\xFF\xFE"] {
            assert_eq!(
                decode(input).unwrap_err().code(),
                LAY_HEX_INVALID,
                "{input:?}"
            );
        }
    }

    #[test]
    fn sniff_only_accepts_hex_that_leads_to_a_known_format() {
        let ctx = SniffCtx::default();
        let bytes = fixture();
        assert!(HexLayer.sniff(&bytes, &ctx) > Confidence::MEDIUM);
        assert_eq!(HexLayer.sniff(b"deadbeef", &ctx), Confidence::NONE);
        assert_eq!(HexLayer.sniff(b"hello", &ctx), Confidence::NONE);
    }

    #[test]
    fn bad_parameters_are_rejected() {
        let mut params = Params::new();
        params.set("hex", "mixed");
        assert_eq!(
            HexLayer.encode(b"x", &params).unwrap_err().code(),
            codes::CORE_INVALID_PARAM
        );
    }
}
