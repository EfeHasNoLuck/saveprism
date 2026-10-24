//! The JavaScript `escape()` / `encodeURIComponent` layer used by TyranoScript saves.
//!
//! A TyranoScript desktop save is JSON text written through `escape(JSON.stringify(data))` (ASCII
//! punctuation and spaces as `%XX`, Latin-1 characters as `%XX`, characters above U+00FF as
//! `%uXXXX`). Browser storage and some builds use `encodeURIComponent`, whose `%XX` bytes are
//! UTF-8. Both are decoded here to the UTF-8 JSON text that `format.json` reads.
//!
//! The layer records enough to write the file back exactly: the style, the hex case, and the
//! characters whose escaping differs from the style's own rule (`escaped` / `raw`). Text that
//! still does not reproduce the source is marked `irregular`; edited characters not seen in the
//! file are escaped with the file's style. When a file has only ASCII escapes the two styles are
//! told apart by characters they disagree on (for example a raw `!` means `uri`, a raw `/` means
//! `escape`).
//!
//! A `%` that does not start a complete escape is an error (`SP-URL-001`), and bytes that are not
//! UTF-8 are an error (`SP-URL-002`). Decoding never grows the input.

use std::collections::BTreeSet;

use saveprism_core::limits::len_u64;
use saveprism_core::{Confidence, DecodeCtx, Error, Layer, Params, Result, SniffCtx};

use super::{LAY_URL_ESCAPE_INVALID, LAY_URL_ESCAPE_UTF8, invalid_param};

/// How many leading bytes `sniff` looks at.
const SNIFF_BYTES: usize = 3;

/// `encoding.url-escape`.
///
/// Parameters: `style` (`"escape"` or `"uri"`), `hex` (`"upper"` or `"lower"`), `escaped` /
/// `raw` (characters that behave differently from the style, as strings) and `irregular` (bool,
/// set on decode when writing the recorded style does not reproduce the source).
#[derive(Clone, Copy, Debug, Default)]
pub struct UrlEscapeLayer;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Style {
    Escape,
    Uri,
}

impl Style {
    fn name(self) -> &'static str {
        match self {
            Style::Escape => "escape",
            Style::Uri => "uri",
        }
    }

    fn parse(value: &str) -> Result<Self> {
        match value {
            "escape" => Ok(Style::Escape),
            "uri" => Ok(Style::Uri),
            other => Err(invalid_param(format!(
                "parameter \"style\" must be \"escape\" or \"uri\", not \"{other}\""
            ))),
        }
    }
}

/// The decoded text plus which characters were seen escaped and which were seen raw.
struct DecodedText {
    text: String,
    escaped: BTreeSet<char>,
    raw: BTreeSet<char>,
    lower_hex: bool,
    upper_hex: bool,
}

fn bad(message: impl Into<String>, offset: usize) -> Error {
    Error::new(LAY_URL_ESCAPE_INVALID, message).with_offset(len_u64(offset))
}

fn utf8_error(offset: usize, source: std::str::Utf8Error) -> Error {
    Error::new(
        LAY_URL_ESCAPE_UTF8,
        format!("the escaped bytes are not valid UTF-8 (invalid byte at offset {offset})"),
    )
    .with_offset(len_u64(offset))
    .with_source(source)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Reads `count` hex digits at `start`. Returns the value and whether a lower/upper letter was seen.
fn read_hex(input: &[u8], start: usize, count: usize) -> Option<(u32, bool, bool)> {
    let mut value = 0_u32;
    let mut lower = false;
    let mut upper = false;
    for offset in 0..count {
        let byte = input.get(start.saturating_add(offset))?;
        if byte.is_ascii_lowercase() {
            lower = true;
        } else if byte.is_ascii_uppercase() {
            upper = true;
        }
        value = value
            .checked_mul(16)?
            .checked_add(u32::from(hex_value(*byte)?))?;
    }
    Some((value, lower, upper))
}

/// True when a `%uXXXX` escape occurs anywhere, or a `%XX` escape with a byte above 0x7F.
fn scan_escapes(input: &[u8]) -> (bool, bool) {
    let mut has_u = false;
    let mut has_high = false;
    for (index, &byte) in input.iter().enumerate() {
        if byte != b'%' {
            continue;
        }
        match input.get(index.saturating_add(1)) {
            Some(&b'u' | &b'U') => has_u = true,
            Some(_)
                if read_hex(input, index.saturating_add(1), 2)
                    .is_some_and(|(value, _, _)| value >= 0x80) =>
            {
                has_high = true;
            }
            _ => {}
        }
    }
    (has_u, has_high)
}

/// Decodes `escape()` output: `%XX` is a code point U+00XX and `%uXXXX` a UTF-16 code unit.
fn decode_escape(input: &[u8]) -> Result<DecodedText> {
    let mut text = String::with_capacity(input.len());
    let mut escaped = BTreeSet::new();
    let mut raw = BTreeSet::new();
    let mut lower_hex = false;
    let mut upper_hex = false;
    let mut index = 0_usize;

    while let Some(&byte) = input.get(index) {
        if byte != b'%' {
            let start = index;
            let mut end = index;
            while input.get(end).is_some_and(|&b| b != b'%') {
                end = end.saturating_add(1);
            }
            let run = input.get(start..end).unwrap_or_default();
            let chunk = std::str::from_utf8(run)
                .map_err(|source| utf8_error(start.saturating_add(source.valid_up_to()), source))?;
            for character in chunk.chars() {
                text.push(character);
                raw.insert(character);
            }
            index = end;
            continue;
        }

        if matches!(input.get(index.saturating_add(1)), Some(&b'u' | &b'U')) {
            let (value, lower, upper) = read_hex(input, index.saturating_add(2), 4)
                .ok_or_else(|| bad("a \"%u\" escape needs four hex digits", index))?;
            lower_hex |= lower;
            upper_hex |= upper;
            let mut consumed = 6_usize;
            let code = if (0xD800..=0xDBFF).contains(&value) {
                let next = index.saturating_add(6);
                if !matches!(input.get(next), Some(&b'%'))
                    || !matches!(input.get(next.saturating_add(1)), Some(&b'u' | &b'U'))
                {
                    return Err(bad(
                        "a high surrogate is not followed by a low surrogate escape",
                        index,
                    ));
                }
                let (low, low_lower, low_upper) = read_hex(input, next.saturating_add(2), 4)
                    .ok_or_else(|| bad("a \"%u\" escape needs four hex digits", next))?;
                lower_hex |= low_lower;
                upper_hex |= low_upper;
                if !(0xDC00..=0xDFFF).contains(&low) {
                    return Err(bad("the second surrogate is not a low surrogate", next));
                }
                consumed = 12;
                0x1_0000 + ((value - 0xD800) << 10) + (low - 0xDC00)
            } else if (0xDC00..=0xDFFF).contains(&value) {
                return Err(bad("a low surrogate without a high surrogate", index));
            } else {
                value
            };
            let character = char::from_u32(code)
                .ok_or_else(|| bad("the escape is not a Unicode scalar value", index))?;
            text.push(character);
            escaped.insert(character);
            index = index.saturating_add(consumed);
        } else {
            let (value, lower, upper) = read_hex(input, index.saturating_add(1), 2)
                .ok_or_else(|| bad("a \"%\" escape needs two hex digits", index))?;
            lower_hex |= lower;
            upper_hex |= upper;
            let character = char::from_u32(value)
                .ok_or_else(|| bad("the escape is not a Unicode scalar value", index))?;
            text.push(character);
            escaped.insert(character);
            index = index.saturating_add(3);
        }
    }

    Ok(DecodedText {
        text,
        escaped,
        raw,
        lower_hex,
        upper_hex,
    })
}

/// Decodes `encodeURIComponent` output: consecutive `%XX` are UTF-8 bytes.
fn decode_uri(input: &[u8]) -> Result<DecodedText> {
    let mut text = String::with_capacity(input.len());
    let mut escaped = BTreeSet::new();
    let mut raw = BTreeSet::new();
    let mut lower_hex = false;
    let mut upper_hex = false;
    let mut index = 0_usize;

    while let Some(&byte) = input.get(index) {
        if byte != b'%' {
            let start = index;
            let mut end = index;
            while input.get(end).is_some_and(|&b| b != b'%') {
                end = end.saturating_add(1);
            }
            let run = input.get(start..end).unwrap_or_default();
            let chunk = std::str::from_utf8(run)
                .map_err(|source| utf8_error(start.saturating_add(source.valid_up_to()), source))?;
            for character in chunk.chars() {
                text.push(character);
                raw.insert(character);
            }
            index = end;
            continue;
        }

        let start = index;
        let mut bytes = Vec::new();
        while input.get(index) == Some(&b'%') {
            let (value, lower, upper) = read_hex(input, index.saturating_add(1), 2)
                .ok_or_else(|| bad("a \"%\" escape needs two hex digits", index))?;
            lower_hex |= lower;
            upper_hex |= upper;
            bytes.push(value as u8);
            index = index.saturating_add(3);
        }
        let chunk = std::str::from_utf8(&bytes)
            .map_err(|source| utf8_error(start.saturating_add(source.valid_up_to()), source))?;
        for character in chunk.chars() {
            text.push(character);
            escaped.insert(character);
        }
    }

    Ok(DecodedText {
        text,
        escaped,
        raw,
        lower_hex,
        upper_hex,
    })
}

fn safe_ascii(byte: u8, style: Style) -> bool {
    if byte.is_ascii_alphanumeric() {
        return true;
    }
    match style {
        Style::Escape => matches!(byte, b'@' | b'*' | b'_' | b'+' | b'-' | b'.' | b'/'),
        Style::Uri => matches!(
            byte,
            b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')'
        ),
    }
}

/// Whether the style escapes `character` when it is not overridden by `escaped` / `raw`.
fn default_escaped(character: char, style: Style) -> bool {
    let code = character as u32;
    if !character.is_ascii() {
        return true;
    }
    if code < 0x20 || code == 0x7F {
        return true;
    }
    let byte = u8::try_from(code).unwrap_or(b'?');
    !safe_ascii(byte, style)
}

/// Guesses the style of an ASCII-only file from characters the two styles disagree on.
///
/// A character escaped although `escape` would leave it raw (or raw although `escape` would
/// escape it) is evidence for `uri`, and the other way round for `escape`. The larger side wins;
/// a tie (no evidence) keeps the default.
fn style_evidence(decoded: &DecodedText) -> Option<Style> {
    let mut score = 0_i32;
    for &character in &decoded.escaped {
        match (
            default_escaped(character, Style::Escape),
            default_escaped(character, Style::Uri),
        ) {
            (true, false) => score += 1,
            (false, true) => score -= 1,
            _ => {}
        }
    }
    for &character in &decoded.raw {
        match (
            default_escaped(character, Style::Escape),
            default_escaped(character, Style::Uri),
        ) {
            (true, false) => score -= 1,
            (false, true) => score += 1,
            _ => {}
        }
    }
    if score > 0 {
        Some(Style::Escape)
    } else if score < 0 {
        Some(Style::Uri)
    } else {
        None
    }
}

fn push_hex(out: &mut String, value: u32, digits: u32, lower: bool) {
    let table: &[u8; 16] = if lower {
        b"0123456789abcdef"
    } else {
        b"0123456789ABCDEF"
    };
    for shift in (0..digits).rev() {
        let nibble = (value >> (shift.saturating_mul(4))) & 0xF;
        let digit = usize::try_from(nibble)
            .ok()
            .and_then(|index| table.get(index))
            .copied()
            .unwrap_or(b'0');
        out.push(char::from(digit));
    }
}

fn push_escaped(out: &mut String, character: char, style: Style, lower: bool) {
    match style {
        Style::Escape => {
            let code = character as u32;
            if code <= 0xFF {
                out.push('%');
                push_hex(out, code, 2, lower);
            } else if code <= 0xFFFF {
                out.push_str("%u");
                push_hex(out, code, 4, lower);
            } else {
                let value = code - 0x1_0000;
                let high = 0xD800 + (value >> 10);
                let low = 0xDC00 + (value & 0x3FF);
                out.push_str("%u");
                push_hex(out, high, 4, lower);
                out.push_str("%u");
                push_hex(out, low, 4, lower);
            }
        }
        Style::Uri => {
            let mut buffer = [0_u8; 4];
            for &byte in character.encode_utf8(&mut buffer).as_bytes() {
                out.push('%');
                push_hex(out, u32::from(byte), 2, lower);
            }
        }
    }
}

fn encode_text(
    text: &str,
    style: Style,
    lower: bool,
    escaped: &BTreeSet<char>,
    raw: &BTreeSet<char>,
) -> String {
    let mut out = String::with_capacity(text.len().saturating_mul(2));
    for character in text.chars() {
        let escape = if escaped.contains(&character) {
            true
        } else if raw.contains(&character) {
            false
        } else {
            default_escaped(character, style)
        };
        if escape {
            push_escaped(&mut out, character, style, lower);
        } else {
            out.push(character);
        }
    }
    out
}

/// Characters whose escaping differs from the style's default.
fn overrides(decoded: &DecodedText, style: Style) -> (BTreeSet<char>, BTreeSet<char>) {
    let mut escaped = BTreeSet::new();
    let mut raw = BTreeSet::new();
    for &character in &decoded.escaped {
        if !default_escaped(character, style) {
            escaped.insert(character);
        }
    }
    for &character in &decoded.raw {
        if default_escaped(character, style) {
            raw.insert(character);
        }
    }
    (escaped, raw)
}

fn set_text(chars: &BTreeSet<char>) -> String {
    chars.iter().copied().collect()
}

fn parse_set(value: &str) -> BTreeSet<char> {
    value.chars().collect()
}

impl Layer for UrlEscapeLayer {
    fn id(&self) -> &'static str {
        "encoding.url-escape"
    }

    fn sniff(&self, input: &[u8], _ctx: &SniffCtx<'_>) -> Confidence {
        let head = input
            .get(..SNIFF_BYTES.min(input.len()))
            .unwrap_or_default();
        let is_json_start = matches!(head, [b'%', b'7' | b'5', b'B' | b'b']);
        if is_json_start {
            Confidence::MEDIUM
        } else {
            Confidence::NONE
        }
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<(Vec<u8>, Params)> {
        let (has_u, has_high) = scan_escapes(input);
        let (style, decoded) = if let Some(hint) = ctx.hints().opt_str("style")? {
            let style = Style::parse(hint)?;
            let decoded = match style {
                Style::Escape => decode_escape(input)?,
                Style::Uri => decode_uri(input)?,
            };
            (style, decoded)
        } else if has_u {
            (Style::Escape, decode_escape(input)?)
        } else if has_high {
            // `%u` is absent; high `%XX` bytes are UTF-8 in `uri` but Latin-1 in `escape`.
            match decode_uri(input) {
                Ok(decoded) => (Style::Uri, decoded),
                Err(_) => (Style::Escape, decode_escape(input)?),
            }
        } else {
            // Both styles decode ASCII escapes the same way; the raw/escaped characters decide.
            let decoded = decode_escape(input)?;
            let style = style_evidence(&decoded).unwrap_or(Style::Escape);
            (style, decoded)
        };
        if decoded.text.len() > input.len() {
            return Err(Error::new(
                LAY_URL_ESCAPE_INVALID,
                "the decoded text is larger than the input",
            ));
        }

        let (escaped, raw) = overrides(&decoded, style);
        let lower = decoded.lower_hex && !decoded.upper_hex;
        let written = encode_text(&decoded.text, style, lower, &escaped, &raw);
        let irregular = written.as_bytes() != input;

        let mut params = Params::new();
        params.set("style", style.name());
        params.set("hex", if lower { "lower" } else { "upper" });
        if !escaped.is_empty() {
            params.set("escaped", set_text(&escaped));
        }
        if !raw.is_empty() {
            params.set("raw", set_text(&raw));
        }
        params.set("irregular", irregular);
        Ok((decoded.text.into_bytes(), params))
    }

    fn encode(&self, inner: &[u8], params: &Params) -> Result<Vec<u8>> {
        let style = match params.opt_str("style")? {
            None => Style::Escape,
            Some(value) => Style::parse(value)?,
        };
        let lower = match params.opt_str("hex")? {
            None | Some("upper") => false,
            Some("lower") => true,
            Some(other) => {
                return Err(invalid_param(format!(
                    "parameter \"hex\" must be \"upper\" or \"lower\", not \"{other}\""
                )));
            }
        };
        let escaped = parse_set(params.opt_str("escaped")?.unwrap_or_default());
        let raw = parse_set(params.opt_str("raw")?.unwrap_or_default());
        let text = std::str::from_utf8(inner)
            .map_err(|source| utf8_error(source.valid_up_to(), source))?;
        Ok(encode_text(text, style, lower, &escaped, &raw).into_bytes())
    }
}

#[cfg(test)]
mod tests {
    use saveprism_core::{Limits, codes};

    use super::*;

    fn decode(input: &[u8]) -> Result<(Vec<u8>, Params)> {
        let limits = Limits::default();
        let mut ctx = DecodeCtx::new(&limits);
        UrlEscapeLayer.decode(input, &mut ctx)
    }

    fn decode_with_style(input: &[u8], style: &str) -> Result<(Vec<u8>, Params)> {
        let limits = Limits::default();
        let mut hints = Params::new();
        hints.set("style", style);
        let mut ctx = DecodeCtx::new(&limits).with_step(0, hints);
        UrlEscapeLayer.decode(input, &mut ctx)
    }

    fn round_trip(input: &[u8]) -> (Vec<u8>, Params) {
        let (inner, params) = decode(input).unwrap();
        let again = UrlEscapeLayer.encode(&inner, &params).unwrap();
        assert_eq!(again, input, "round trip changed the text");
        (inner, params)
    }

    #[test]
    fn escape_style_with_latin1_and_japanese() {
        let input = b"%7B%22name%22%3A%22caf%E9%20%u65E5%u672C%22%7D";
        let (inner, params) = round_trip(input);
        assert_eq!(
            std::str::from_utf8(&inner).unwrap(),
            "{\"name\":\"caf\u{e9} \u{65e5}\u{672c}\"}"
        );
        assert_eq!(params.str("style").unwrap(), "escape");
        assert_eq!(params.str("hex").unwrap(), "upper");
        assert!(!params.bool("irregular").unwrap());
    }

    #[test]
    fn uri_style_uses_utf8_bytes() {
        let input = b"%7B%22name%22%3A%22%E6%97%A5%E6%9C%AC%22%7D";
        let (inner, params) = round_trip(input);
        assert_eq!(
            std::str::from_utf8(&inner).unwrap(),
            "{\"name\":\"\u{65e5}\u{672c}\"}"
        );
        assert_eq!(params.str("style").unwrap(), "uri");
    }

    #[test]
    fn lower_hex_is_recorded() {
        let input = b"%7b%22name%22%3a%22caf%e9%22%7d";
        let (inner, params) = round_trip(input);
        assert_eq!(
            std::str::from_utf8(&inner).unwrap(),
            "{\"name\":\"caf\u{e9}\"}"
        );
        assert_eq!(params.str("hex").unwrap(), "lower");
    }

    #[test]
    fn astral_characters_use_surrogate_pairs() {
        let input = b"%uD83D%uDE00";
        let (inner, params) = round_trip(input);
        assert_eq!(std::str::from_utf8(&inner).unwrap(), "\u{1F600}");
        assert_eq!(params.str("style").unwrap(), "escape");
        let mut edited = Params::new();
        edited.set("style", "escape");
        assert_eq!(
            UrlEscapeLayer
                .encode("\u{1F600}".as_bytes(), &edited)
                .unwrap(),
            input
        );
    }

    #[test]
    fn escaped_safe_characters_are_recorded_and_survive_an_edit() {
        // `escape` would leave `@` raw; escaping it is an override that must be kept.
        let input = b"%7B%22a%22%3A%22x%40y%22%7D";
        let (inner, params) = decode_with_style(input, "escape").unwrap();
        let again = UrlEscapeLayer.encode(&inner, &params).unwrap();
        assert_eq!(again, input);
        assert_eq!(params.str("escaped").unwrap(), "@");
        let mut text = inner.clone();
        text.extend_from_slice(b"!");
        let edited = UrlEscapeLayer.encode(&text, &params).unwrap();
        assert_eq!(
            std::str::from_utf8(&edited).unwrap(),
            "%7B%22a%22%3A%22x%40y%22%7D%21"
        );
    }

    #[test]
    fn escaped_spaces_are_the_style_default_and_are_reproduced() {
        let input = b"%7B%22a%22%3A%22x%20y%22%7D";
        let (inner, params) = round_trip(input);
        // The space was escaped, which is the style default, so nothing is overridden.
        assert!(!params.contains("raw"));
        let mut text = inner.clone();
        text.push(b' ');
        let edited = UrlEscapeLayer.encode(&text, &params).unwrap();
        assert_eq!(
            std::str::from_utf8(&edited).unwrap(),
            "%7B%22a%22%3A%22x%20y%22%7D%20"
        );
    }

    #[test]
    fn ascii_only_uri_style_is_told_apart_by_the_characters_it_leaves_raw() {
        let input = b"%7B%22a%22%3A%22yes!%2Fno%22%7D";
        let (inner, params) = round_trip(input);
        assert_eq!(std::str::from_utf8(&inner).unwrap(), "{\"a\":\"yes!/no\"}");
        assert_eq!(params.str("style").unwrap(), "uri");
        // A new `!` stays raw and a new `/` is escaped, as `encodeURIComponent` does.
        let mut text = inner.clone();
        text.extend_from_slice(b"!/");
        let edited = UrlEscapeLayer.encode(&text, &params).unwrap();
        assert_eq!(
            std::str::from_utf8(&edited).unwrap(),
            "%7B%22a%22%3A%22yes!%2Fno%22%7D!%2F"
        );
    }

    #[test]
    fn malformed_escapes_are_errors() {
        for input in [
            &b"%"[..],
            b"%7",
            b"%zz",
            b"%u12",
            b"%uD800",
            b"%uDC00",
            b"%uD800%u0041",
            b"%7B%",
        ] {
            let error = decode(input).unwrap_err();
            assert_eq!(error.code(), LAY_URL_ESCAPE_INVALID, "{input:?}");
        }
        assert!(decode(b"%zz").unwrap_err().offset().is_some());
    }

    #[test]
    fn invalid_utf8_in_uri_style_is_an_error() {
        let error = decode_with_style(b"%7B%22a%22%3A%22%FF%22%7D", "uri").unwrap_err();
        assert_eq!(error.code(), LAY_URL_ESCAPE_UTF8);
    }

    #[test]
    fn sniff_only_accepts_a_json_start() {
        let ctx = SniffCtx::default();
        assert_eq!(
            UrlEscapeLayer.sniff(b"%7B%22a%22%3A1%7D", &ctx),
            Confidence::MEDIUM
        );
        assert_eq!(
            UrlEscapeLayer.sniff(b"%5B1%2C2%5D", &ctx),
            Confidence::MEDIUM
        );
        assert_eq!(UrlEscapeLayer.sniff(b"%7b", &ctx), Confidence::MEDIUM);
        assert_eq!(UrlEscapeLayer.sniff(b"{\"a\":1}", &ctx), Confidence::NONE);
        assert_eq!(UrlEscapeLayer.sniff(b"%22a%22", &ctx), Confidence::NONE);
        assert_eq!(UrlEscapeLayer.sniff(b"", &ctx), Confidence::NONE);
    }

    #[test]
    fn bad_parameters_are_rejected() {
        for (key, value) in [("style", "rot13"), ("hex", "mixed")] {
            let mut params = Params::new();
            params.set(key, value);
            assert_eq!(
                UrlEscapeLayer.encode(b"{}", &params).unwrap_err().code(),
                codes::CORE_INVALID_PARAM
            );
        }
    }

    #[test]
    fn decoding_never_grows_the_input() {
        let (inner, _) = decode(b"%u0041").unwrap();
        assert_eq!(inner, b"A");
    }

    use proptest::prelude::*;

    proptest! {
        #[test]
        fn arbitrary_bytes_never_panic(data in prop::collection::vec(any::<u8>(), 0..256)) {
            let limits = Limits::default();
            let mut ctx = DecodeCtx::new(&limits);
            if let Ok((inner, params)) = UrlEscapeLayer.decode(&data, &mut ctx) {
                let _ = UrlEscapeLayer.encode(&inner, &params);
            }
        }

        #[test]
        fn any_text_round_trips_in_both_styles(text in "\\PC*") {
            for style in ["escape", "uri"] {
                let mut params = Params::new();
                params.set("style", style);
                let encoded = UrlEscapeLayer.encode(text.as_bytes(), &params).unwrap();
                let (inner, _) = decode(&encoded).unwrap();
                prop_assert_eq!(inner, text.as_bytes());
            }
        }
    }
}
