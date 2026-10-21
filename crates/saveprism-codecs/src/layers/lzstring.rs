//! The LZ-String layer.
//!
//! The decompressor is a bounded Rust port of the public LZ-String algorithm
//! (https://github.com/pieroxy/lz-string, MIT License, Copyright (c) 2013 Pieroxy).
//! Unlike the original it never builds strings: dictionary entries are linked to their parent entry,
//! and the total number of UTF-16 units written is checked against the output budget before every
//! write, because the decompressed size can grow with the square of the input.
//! Compression uses the `lz-str` crate.

use saveprism_core::codes;
use saveprism_core::limits::len_u64;
use saveprism_core::{Confidence, DecodeCtx, Error, Layer, Params, Result, SniffCtx};

use super::{LAY_ENCODE_FAILED, LAY_INVALID_UTF16, LAY_LZSTRING_INVALID, invalid_param};

const BASE64_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
/// The 64 data symbols of the URI-safe alphabet. The reference implementation's
/// key string has a 65th symbol (`$`) that is only the padding slot and is never
/// produced as data, so it is not part of this table (see `in_any_alphabet`).
const URI_ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+-";

/// How many leading bytes `sniff` looks at.
const SNIFF_BYTES: usize = 4096;
/// Dictionary codes wider than this would mean a dictionary of hundreds of millions of entries.
const MAX_CODE_BITS: u32 = 28;
/// Marks a dictionary entry without a parent.
const NO_PARENT: usize = usize::MAX;

/// `compression.lz-string`.
///
/// Parameters: `variant` (`"base64"` or `"uri"`). On decode the variant can be given as a recipe
/// hint; otherwise it is guessed from the characters used.
#[derive(Clone, Copy, Debug, Default)]
pub struct LzStringLayer;

fn bad(message: impl Into<String>) -> Error {
    Error::new(LAY_LZSTRING_INVALID, message)
}

fn truncated() -> Error {
    bad("the compressed data ends too early")
}

/// Reads bits, most significant bit of each 6-bit character first.
struct BitReader<'a> {
    values: &'a [u8],
    index: usize,
    mask: u8,
}

impl<'a> BitReader<'a> {
    fn new(values: &'a [u8]) -> Self {
        Self {
            values,
            index: 0,
            mask: 32,
        }
    }

    fn bit(&mut self) -> Option<u32> {
        let value = *self.values.get(self.index)?;
        let bit = u32::from((value & self.mask) != 0);
        self.mask >>= 1;
        if self.mask == 0 {
            self.mask = 32;
            self.index += 1;
        }
        Some(bit)
    }

    /// Reads `count` bits (count < 32), first bit least significant.
    fn bits(&mut self, count: u32) -> Option<u32> {
        let mut result = 0_u32;
        for shift in 0..count {
            result |= self.bit()? << shift;
        }
        Some(result)
    }
}

/// One dictionary entry: the string `parent + [unit]`.
#[derive(Clone, Copy)]
struct Entry {
    parent: usize,
    unit: u16,
    first: u16,
    len: usize,
}

fn literal(unit: u16) -> Entry {
    Entry {
        parent: NO_PARENT,
        unit,
        first: unit,
        len: 1,
    }
}

fn read_literal(reader: &mut BitReader<'_>, code: u32) -> Result<u16> {
    let width = if code == 0 { 8 } else { 16 };
    let value = reader.bits(width).ok_or_else(truncated)?;
    u16::try_from(value).map_err(|_| bad("a literal does not fit in 16 bits"))
}

/// Appends the string of entry `index` to `out`, refusing to go above `max_units` in total.
fn emit(dict: &[Entry], index: usize, out: &mut Vec<u16>, max_units: u64) -> Result<()> {
    let entry = *dict
        .get(index)
        .ok_or_else(|| bad("the data refers to a missing dictionary entry"))?;
    let total = len_u64(out.len()).saturating_add(len_u64(entry.len));
    if total > max_units {
        return Err(Error::new(
            codes::CORE_LIMIT_DECODED,
            format!(
                "decompressed LZ-String data would be more than {max_units} UTF-16 units (limit; possible decompression bomb)"
            ),
        ));
    }
    let start = out.len();
    out.resize(start + entry.len, 0);
    let mut position = start + entry.len;
    let mut current = entry;
    while position > start {
        position -= 1;
        if let Some(slot) = out.get_mut(position) {
            *slot = current.unit;
        }
        if current.parent == NO_PARENT {
            break;
        }
        current = *dict
            .get(current.parent)
            .ok_or_else(|| bad("the dictionary is inconsistent"))?;
    }
    Ok(())
}

/// Decompresses 6-bit character values into UTF-16 units, writing at most `max_units` units.
fn decompress(values: &[u8], max_units: u64) -> Result<Vec<u16>> {
    let mut reader = BitReader::new(values);
    let mut dict: Vec<Entry> = Vec::new();
    for _ in 0..3 {
        dict.push(literal(0));
    }

    let first_code = reader.bits(2).ok_or_else(truncated)?;
    match first_code {
        0 | 1 => {}
        2 => return Ok(Vec::new()),
        _ => return Err(bad("the data starts with an invalid code")),
    }
    dict.push(literal(read_literal(&mut reader, first_code)?));
    let mut out: Vec<u16> = Vec::new();
    let mut previous = 3_usize;
    emit(&dict, previous, &mut out, max_units)?;

    let mut enlarge_in = 4_u64;
    let mut num_bits = 3_u32;
    loop {
        let code = reader.bits(num_bits).ok_or_else(truncated)?;
        let index = match code {
            0 | 1 => {
                let unit = read_literal(&mut reader, code)?;
                dict.push(literal(unit));
                enlarge_in = enlarge_in.saturating_sub(1);
                dict.len() - 1
            }
            2 => return Ok(out),
            other => usize::try_from(other).unwrap_or(usize::MAX),
        };
        if enlarge_in == 0 {
            enlarge_in = 1_u64 << num_bits;
            num_bits += 1;
        }

        let previous_entry = *dict
            .get(previous)
            .ok_or_else(|| bad("the dictionary is inconsistent"))?;
        let size = dict.len();
        let appended = if index < size {
            dict.get(index).map_or(previous_entry.first, |e| e.first)
        } else if index == size {
            previous_entry.first
        } else {
            return Err(bad(format!(
                "code {index} is above the dictionary size {size}"
            )));
        };
        dict.push(Entry {
            parent: previous,
            unit: appended,
            first: previous_entry.first,
            len: previous_entry.len.saturating_add(1),
        });
        emit(&dict, index, &mut out, max_units)?;

        enlarge_in = enlarge_in.saturating_sub(1);
        previous = index;
        if enlarge_in == 0 {
            enlarge_in = 1_u64 << num_bits;
            num_bits += 1;
        }
        if num_bits > MAX_CODE_BITS {
            return Err(bad("the data uses an unreasonably large dictionary"));
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Variant {
    Base64,
    Uri,
}

impl Variant {
    fn name(self) -> &'static str {
        match self {
            Variant::Base64 => "base64",
            Variant::Uri => "uri",
        }
    }

    fn parse(name: &str) -> Option<Self> {
        match name {
            "base64" => Some(Variant::Base64),
            "uri" => Some(Variant::Uri),
            _ => None,
        }
    }

    fn alphabet(self) -> &'static [u8; 64] {
        match self {
            Variant::Base64 => BASE64_ALPHABET,
            Variant::Uri => URI_ALPHABET,
        }
    }

    /// Guesses the variant from the characters (and the missing padding) of `input`.
    /// The flag is true when the text cannot tell the variants apart.
    fn guess(input: &[u8]) -> (Self, bool) {
        if input.iter().any(|&b| b == b'-' || b == b'$') {
            (Variant::Uri, false)
        } else if input.iter().any(|&b| b == b'/' || b == b'=') {
            (Variant::Base64, false)
        } else if input.len() & 3 != 0 {
            (Variant::Uri, false)
        } else {
            (Variant::Base64, true)
        }
    }
}

/// Compresses with the `lz_str` crate, then normalises the padding to what the original
/// JavaScript library emits: the base64 variant is padded with `=` to a multiple of 4 (a
/// remainder of 1 gives `===`), the URI variant is never padded. The crate pads differently.
fn compress(text: &str, variant: Variant) -> Vec<u8> {
    let packed = match variant {
        Variant::Base64 => lz_str::compress_to_base64(text),
        Variant::Uri => lz_str::compress_to_encoded_uri_component(text),
    };
    let mut bytes = packed.into_bytes();
    while bytes.last() == Some(&b'=') {
        bytes.pop();
    }
    if variant == Variant::Base64 {
        let padding: &[u8] = match bytes.len() % 4 {
            1 => b"===",
            2 => b"==",
            3 => b"=",
            _ => b"",
        };
        bytes.extend_from_slice(padding);
    }
    bytes
}

/// Converts UTF-16 units to UTF-8. The UTF-8 length is computed and checked with `check`
/// before any output is allocated.
fn utf16_to_utf8_bounded(units: &[u16], check: impl Fn(u64) -> Result<()>) -> Result<Vec<u8>> {
    let unpaired = || {
        Error::new(
            LAY_INVALID_UTF16,
            "the decompressed data has an unpaired UTF-16 surrogate",
        )
    };
    let mut total = 0_usize;
    for decoded in char::decode_utf16(units.iter().copied()) {
        let ch = decoded.map_err(|_| unpaired())?;
        total = total.saturating_add(ch.len_utf8());
    }
    check(len_u64(total))?;
    let mut bytes = Vec::with_capacity(total);
    let mut buffer = [0_u8; 4];
    for decoded in char::decode_utf16(units.iter().copied()) {
        let ch = decoded.map_err(|_| unpaired())?;
        bytes.extend_from_slice(ch.encode_utf8(&mut buffer).as_bytes());
    }
    Ok(bytes)
}

fn in_any_alphabet(byte: u8) -> bool {
    byte == b'='
        || byte == b'-'
        || byte == b'$'
        || byte == b'/'
        || byte == b'+'
        || byte.is_ascii_alphanumeric()
}

impl Layer for LzStringLayer {
    fn id(&self) -> &'static str {
        "compression.lz-string"
    }

    fn sniff(&self, input: &[u8], _ctx: &SniffCtx<'_>) -> Confidence {
        let head = input
            .get(..SNIFF_BYTES.min(input.len()))
            .unwrap_or_default();
        if !head.is_empty() && head.iter().all(|&b| in_any_alphabet(b)) {
            Confidence::new(0.4)
        } else {
            Confidence::NONE
        }
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<(Vec<u8>, Params)> {
        if input.is_empty() {
            return Err(bad("the input is empty").with_offset(0));
        }
        let (variant, ambiguous) = match ctx.hints().opt_str("variant")? {
            Some(name) => (
                Variant::parse(name).ok_or_else(|| {
                    invalid_param(format!(
                        "hint \"variant\" must be \"base64\" or \"uri\", not \"{name}\""
                    ))
                })?,
                false,
            ),
            None => Variant::guess(input),
        };

        let text = if variant == Variant::Base64 {
            let end = input
                .iter()
                .rposition(|&b| b != b'=')
                .map_or(0, |position| position + 1);
            input.get(..end).unwrap_or_default()
        } else {
            input
        };
        let alphabet = variant.alphabet();
        let mut values = Vec::with_capacity(text.len());
        for (position, &byte) in text.iter().enumerate() {
            let value = alphabet
                .iter()
                .position(|&symbol| symbol == byte)
                .and_then(|index| u8::try_from(index).ok())
                .ok_or_else(|| {
                    bad(format!(
                        "byte 0x{byte:02X} is not in the LZ-String {} alphabet",
                        variant.name()
                    ))
                    .with_offset(len_u64(position))
                })?;
            values.push(value);
        }

        let input_len = len_u64(input.len());
        let budget = ctx.output_budget(input_len);
        let units = decompress(&values, budget / 2)?;
        let bytes = utf16_to_utf8_bounded(&units, |len| ctx.check_output(input_len, len))?;

        // An ambiguous text (only characters shared by both alphabets, length a multiple of 4)
        // is resolved by recompressing with each variant and keeping the one that reproduces it.
        let variant = if ambiguous {
            match std::str::from_utf8(&bytes) {
                Ok(text) if compress(text, Variant::Base64) == input => Variant::Base64,
                Ok(text) if compress(text, Variant::Uri) == input => Variant::Uri,
                _ => Variant::Base64,
            }
        } else {
            variant
        };

        let mut params = Params::new();
        params.set("variant", variant.name());
        Ok((bytes, params))
    }

    fn encode(&self, inner: &[u8], params: &Params) -> Result<Vec<u8>> {
        let name = params.opt_str("variant")?.unwrap_or("base64");
        let variant = Variant::parse(name).ok_or_else(|| {
            invalid_param(format!(
                "parameter \"variant\" must be \"base64\" or \"uri\", not \"{name}\""
            ))
        })?;
        let text = std::str::from_utf8(inner).map_err(|source| {
            Error::new(
                LAY_ENCODE_FAILED,
                "LZ-String can only compress valid UTF-8 text",
            )
            .with_source(source)
        })?;
        Ok(compress(text, variant))
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use saveprism_core::Limits;

    use super::*;

    fn decode_with(input: &[u8], limits: &Limits) -> Result<(Vec<u8>, Params)> {
        let mut ctx = DecodeCtx::new(limits);
        LzStringLayer.decode(input, &mut ctx)
    }

    fn pack(text: &str, variant: Variant) -> String {
        String::from_utf8(compress(text, variant)).unwrap()
    }

    /// Vectors produced by the original JavaScript library (lz-string 1.5.0).
    #[test]
    fn javascript_vectors_match() {
        let twenty_a = "a".repeat(20);
        let vectors: [(&str, &str, &str); 8] = [
            ("a", "IZA=", "IZA"),
            ("ab", "IYIyA===", "IYIyA"),
            ("hello", "BYUwNmD2Q===", "BYUwNmD2Q"),
            ("hello world", "BYUwNmD2AEDukCcwBMg=", "BYUwNmD2AEDukCcwBMg"),
            (
                "Şeyma 🌙",
                "nqAFMJ4LYIYASDwbhMfaA===",
                "nqAFMJ4LYIYASDwbhMfaA",
            ),
            (
                "{\"gold\":1250}",
                "N4Ig5g9gNgJiBcBGATAVgAwF8g==",
                "N4Ig5g9gNgJiBcBGATAVgAwF8g",
            ),
            (twenty_a.as_str(), "IY1/kA==", "IY1-kA"),
            ("SavePrism", "MoQwbgpgCgTglgZwLZA=", "MoQwbgpgCgTglgZwLZA"),
        ];
        for (text, base64, uri) in vectors {
            let mut hints = Params::new();
            for (variant, expected) in [(Variant::Base64, base64), (Variant::Uri, uri)] {
                assert_eq!(pack(text, variant), expected, "{text:?}");
                hints.set("variant", variant.name());
                let limits = Limits::default();
                let mut ctx = DecodeCtx::new(&limits).with_step(0, hints.clone());
                let (inner, params) = LzStringLayer.decode(expected.as_bytes(), &mut ctx).unwrap();
                assert_eq!(inner, text.as_bytes());
                assert_eq!(
                    LzStringLayer.encode(&inner, &params).unwrap(),
                    expected.as_bytes()
                );
            }
        }
    }

    fn check_round_trip(text: &str, packed: &str) {
        let limits = Limits::default();
        let (inner, params) = decode_with(packed.as_bytes(), &limits).unwrap();
        assert_eq!(inner, text.as_bytes());
        let again = LzStringLayer.encode(&inner, &params).unwrap();
        assert_eq!(again, packed.as_bytes());
    }

    proptest! {
        #[test]
        fn base64_round_trip(text in "\\PC{0,200}") {
            let packed = pack(&text, Variant::Base64);
            let limits = Limits::default();
            let (inner, params) = decode_with(packed.as_bytes(), &limits).unwrap();
            prop_assert_eq!(&inner, text.as_bytes());
            let again = LzStringLayer.encode(&inner, &params).unwrap();
            prop_assert_eq!(again, packed.into_bytes());
        }

        #[test]
        fn uri_round_trip(text in "\\PC{0,200}") {
            let packed = pack(&text, Variant::Uri);
            let limits = Limits::default();
            let mut hints = Params::new();
            hints.set("variant", "uri");
            let mut ctx = DecodeCtx::new(&limits).with_step(0, hints);
            let (inner, params) = LzStringLayer.decode(packed.as_bytes(), &mut ctx).unwrap();
            prop_assert_eq!(&inner, text.as_bytes());
            let again = LzStringLayer.encode(&inner, &params).unwrap();
            prop_assert_eq!(again, packed.into_bytes());
        }
    }

    #[test]
    fn known_texts_round_trip() {
        for text in [
            "",
            "a",
            "Hello, World!",
            "{\"gold\":1234,\"name\":\"Ünïcödé 日本語 😀\"}",
            "abababababababababab",
        ] {
            check_round_trip(text, &pack(text, Variant::Base64));
        }
    }

    #[test]
    fn uri_variant_is_guessed_and_kept() {
        let text = "the quick brown fox jumps over the lazy dog, the quick brown fox";
        let packed = pack(text, Variant::Uri);
        check_round_trip(text, &packed);
        let limits = Limits::default();
        let (_, params) = decode_with(packed.as_bytes(), &limits).unwrap();
        assert_eq!(params.str("variant").unwrap(), "uri");
    }

    #[test]
    fn ambiguous_uri_text_comes_back_unchanged() {
        // A URI text without '-' or '$' whose length is a multiple of 4 looks like base64.
        let packed = (0..5000)
            .map(|i| pack(&format!("item {i} item {i}"), Variant::Uri))
            .find(|p| p.len() % 4 == 0 && !p.contains(['-', '$', '/', '=']))
            .unwrap();
        let limits = Limits::default();
        let (inner, params) = decode_with(packed.as_bytes(), &limits).unwrap();
        assert!(matches!(params.str("variant").unwrap(), "base64" | "uri"));
        assert_eq!(
            LzStringLayer.encode(&inner, &params).unwrap(),
            packed.as_bytes()
        );

        // A hint decides without guessing.
        let mut hints = Params::new();
        hints.set("variant", "uri");
        let mut ctx = DecodeCtx::new(&limits).with_step(0, hints);
        let (inner, params) = LzStringLayer.decode(packed.as_bytes(), &mut ctx).unwrap();
        assert_eq!(params.str("variant").unwrap(), "uri");
        assert_eq!(
            LzStringLayer.encode(&inner, &params).unwrap(),
            packed.as_bytes()
        );
    }

    #[test]
    fn utf8_size_is_checked_before_allocation() {
        let text = "日".repeat(100); // 100 UTF-16 units, 300 UTF-8 bytes
        let packed = pack(&text, Variant::Base64);
        let with_budget = |bytes: u64| Limits {
            max_decoded_bytes: bytes,
            ratio_floor_bytes: 1 << 40,
            ..Limits::default()
        };
        let (inner, _) = decode_with(packed.as_bytes(), &with_budget(300)).unwrap();
        assert_eq!(inner, text.as_bytes());
        let error = decode_with(packed.as_bytes(), &with_budget(299)).unwrap_err();
        assert_eq!(error.code(), codes::CORE_LIMIT_DECODED);
    }

    #[test]
    fn quadratic_input_hits_the_decoded_limit() {
        let text = "a".repeat(10_000);
        let packed = pack(&text, Variant::Base64);
        assert!(packed.len() < 1_000);

        // With the default limits it decompresses.
        check_round_trip(&text, &packed);

        // With a small limit it is stopped.
        let limits = Limits {
            max_decoded_bytes: 1_000,
            ratio_floor_bytes: 0,
            ..Limits::default()
        };
        let error = decode_with(packed.as_bytes(), &limits).unwrap_err();
        assert_eq!(error.code(), codes::CORE_LIMIT_DECODED);
    }

    #[test]
    fn invalid_input_gives_the_lz_string_code() {
        let limits = Limits::default();
        for input in [&b""[..], b"!!!!", b"A", b"AAAA"] {
            let error = decode_with(input, &limits).unwrap_err();
            assert_eq!(error.code(), LAY_LZSTRING_INVALID, "{input:?}");
        }
    }

    #[test]
    fn bad_variant_is_a_parameter_error() {
        let mut params = Params::new();
        params.set("variant", "utf16");
        assert_eq!(
            LzStringLayer.encode(b"x", &params).unwrap_err().code(),
            codes::CORE_INVALID_PARAM
        );
    }

    #[test]
    fn non_utf8_inner_bytes_cannot_be_compressed() {
        let error = LzStringLayer
            .encode(&[0xFF, 0xFE], &Params::new())
            .unwrap_err();
        assert_eq!(error.code(), LAY_ENCODE_FAILED);
    }

    #[test]
    fn unpaired_surrogate_is_reported() {
        // First code 1 (16-bit literal) = 0xD800, then the end code. Bits are least significant
        // first inside a value, most significant bit of each 6-bit character first.
        let mut bits: Vec<u8> = vec![1, 0];
        bits.extend((0..16).map(|i| u8::from((0xD800_u32 >> i) & 1 == 1)));
        bits.extend([0, 1, 0]); // code 2 in 3 bits
        while !bits.len().is_multiple_of(6) {
            bits.push(0);
        }
        let values: Vec<u8> = bits
            .chunks(6)
            .map(|chunk| chunk.iter().fold(0_u8, |acc, &bit| (acc << 1) | bit))
            .collect();
        let units = decompress(&values, 100).unwrap();
        assert_eq!(units, [0xD800]);
        let text: Vec<u8> = values
            .iter()
            .filter_map(|&v| BASE64_ALPHABET.get(usize::from(v)).copied())
            .collect();
        let limits = Limits::default();
        let error = decode_with(&text, &limits).unwrap_err();
        assert_eq!(error.code(), LAY_INVALID_UTF16);
    }

    #[test]
    fn sniff_accepts_the_two_alphabets_only() {
        let ctx = SniffCtx::default();
        assert_eq!(LzStringLayer.sniff(b"Q===", &ctx), Confidence::new(0.4));
        assert_eq!(LzStringLayer.sniff(b"abc-$+", &ctx), Confidence::new(0.4));
        assert_eq!(LzStringLayer.sniff(b"{\"a\":1}", &ctx), Confidence::NONE);
        assert_eq!(LzStringLayer.sniff(b"", &ctx), Confidence::NONE);
    }
}
