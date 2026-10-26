//! `format.json`: a format-preserving JSON codec.
//!
//! Own parser and writer (no serde_json), both iterative. Machine-written JSON (JavaScript
//! `JSON.stringify`, Python `json.dumps`, Newtonsoft, Unity) is written back byte for byte:
//!
//! - a **format profile** (`indent`, `newline`, `item_sep`, `key_sep`, `empty`,
//!   `trailing_newline`, `bom`) is inferred from the first whitespace seen and kept in the step
//!   `Params`; `regular = false` is stored when writing with the profile does not reproduce the
//!   source (hand-edited spacing), which the round-trip guard reports as "semantically equal";
//! - a node `lexeme` keeps the source text of numbers and strings that differ from the canonical
//!   JavaScript writing (`1e3`, `-0`, `"é"`, `"\/"`).
//!
//! Numbers without fraction and exponent that fit `i128` are `Int` (`JSON:int`), all others `Float`
//! (`JSON:float`). Unpaired surrogate escapes in a string value read as U+FFFD, keep their lexeme
//! and make the node read-only.
//!
//! Known limitations: an object key written with a non-canonical escape (`"\/"`, `"é"`) is
//! normalised when written (the document is marked irregular); a key with an unpaired surrogate
//! escape is refused (`SP-JSON-005`).
//!
//! Easy Save 3 text may write a dictionary key that is not a string without quotes (`{0 : 159}`).
//! [`decode_es3`] accepts those keys, records them in the object node's lexeme (`es3-bare-keys:…`)
//! and the writer emits them bare again; strict `format.json` never accepts them.

pub(crate) mod parse;
pub(crate) mod profile;
pub(crate) mod write;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use saveprism_core::Params;
use saveprism_core::limits::len_u64;
use saveprism_core::{
    Codec, Confidence, DecodeCtx, Decoded, Error, ErrorCode, OwnedValue, Registry, Result, SaveDoc,
    SniffCtx, TypeHint,
};

use profile::Profile;

/// The text is not valid JSON (the error carries the byte offset).
pub const JSON_SYNTAX: ErrorCode = ErrorCode::new("SP-JSON-001");
/// The input is not valid UTF-8.
pub const JSON_UTF8: ErrorCode = ErrorCode::new("SP-JSON-002");
/// Extra data follows the JSON value.
pub const JSON_TRAILING: ErrorCode = ErrorCode::new("SP-JSON-003");
/// The document holds a value JSON cannot write (bytes, map, reference, NaN, infinity).
pub const JSON_UNWRITABLE: ErrorCode = ErrorCode::new("SP-JSON-004");
/// A string with an unpaired surrogate escape cannot be kept without loss (object keys).
pub const JSON_LONE_SURROGATE: ErrorCode = ErrorCode::new("SP-JSON-005");

/// Parameter: the text encoding of the file (`utf-8`, `shift_jis`, ...).
pub(crate) const P_TEXT_ENCODING: &str = "text_encoding";

/// Prefix of the object-node lexeme that records which entries had a bare (unquoted) key. Only
/// Easy Save 3 text can have such keys; the writer emits those keys bare again.
pub(crate) const BARE_KEY_MARKER: &str = "es3-bare-keys:";

/// Builds the object-node lexeme that records the bare keys, or `None` when there are none.
pub(crate) fn bare_key_lexeme(bare: &[bool]) -> Option<Box<str>> {
    if !bare.iter().any(|flag| *flag) {
        return None;
    }
    let mut text = String::from(BARE_KEY_MARKER);
    let mut first = true;
    for (index, flag) in bare.iter().enumerate() {
        if *flag {
            if !first {
                text.push(',');
            }
            text.push_str(&index.to_string());
            first = false;
        }
    }
    Some(text.into_boxed_str())
}

/// Parses a JSON text into a document (used by codecs whose values hold JSON text, such as
/// Naninovel state slots). The caller charges the nodes of the returned document.
pub(crate) fn parse_text(text: &str, ctx: &DecodeCtx<'_>) -> Result<SaveDoc> {
    Ok(parse::parse(text, 0, ctx, parse::KeyStyle::Strict)?.doc)
}

/// Decodes JSON text; `style` decides whether bare dictionary keys are accepted. Only Easy Save 3
/// asks for [`KeyStyle::Bare`](parse::KeyStyle::Bare), so a plain `.json` file with bare keys is
/// still rejected.
fn decode_text(input: &[u8], ctx: &mut DecodeCtx<'_>, style: parse::KeyStyle) -> Result<Decoded> {
    let decoded = crate::text_encoding::decode_sniff(input).ok_or_else(|| {
        Error::new(
            JSON_UTF8,
            "the JSON text is not valid in any encoding SavePrism reads (UTF-8, UTF-16, UTF-32, Shift_JIS, GB18030, CP949, Windows-1252)",
        )
    })?;
    let text = decoded.text.as_str();
    let base = if decoded.encoding == crate::text_encoding::TextEncoding::Utf8 {
        len_u64(decoded.bom_len)
    } else {
        0
    };
    let parsed = parse::parse(text, base, ctx, style)?;
    ctx.charge_nodes(len_u64(parsed.doc.len()))?;

    let mut profile = Profile::infer(text, &parsed.observed, decoded.bom_len > 0);
    profile.text_encoding = decoded.encoding;
    let regular = write::write_document(&parsed.doc, &profile).is_ok_and(|written| {
        std::str::from_utf8(&written).is_ok_and(|written| {
            crate::text_encoding::encode_with(written, decoded.encoding, decoded.bom_len > 0)
                .is_ok_and(|bytes| bytes == input)
        })
    });
    Ok(Decoded {
        doc: parsed.doc,
        params: profile.to_params(regular),
    })
}

/// Decodes Easy Save 3 text, where an object key may be a bare number or `true`/`false`.
pub(crate) fn decode_es3(input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<Decoded> {
    decode_text(input, ctx, parse::KeyStyle::Bare)
}

/// Writes the value at `id` as compact JSON (no indentation, no trailing newline).
pub(crate) fn compact_text(doc: &SaveDoc, id: saveprism_core::NodeId) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    write::write_value(doc, id, &profile::Profile::default(), &mut out)?;
    Ok(out)
}

/// Registers the JSON codec as `format.json`.
pub fn register(reg: &mut Registry) {
    reg.add_codec(Arc::new(JsonCodec));
}

/// The hint that a value added by an edit needs so it is written as the same JSON type and reads
/// back as it was: `JSON:int` for whole numbers, `JSON:float` for decimal numbers (so `2.0` stays a
/// decimal number); `None` for other values.
pub fn new_value_hint(value: &OwnedValue) -> Option<TypeHint> {
    match value {
        OwnedValue::Int(_) => Some(TypeHint::new("JSON:int")),
        OwnedValue::Float(_) => Some(TypeHint::new("JSON:float")),
        _ => None,
    }
}

/// The `format.json` codec.
pub(crate) struct JsonCodec;

impl Codec for JsonCodec {
    fn id(&self) -> &'static str {
        "format.json"
    }

    fn sniff(&self, input: &[u8], ctx: &SniffCtx<'_>) -> Confidence {
        let Some(decoded) = crate::text_encoding::decode_sniff(input) else {
            return Confidence::NONE;
        };
        let trimmed = decoded
            .text
            .trim_matches(|ch: char| ch.is_ascii_whitespace());
        let first = trimmed.as_bytes().first().copied();
        let last = trimmed.as_bytes().last().copied();
        let structure = match (first, last) {
            (Some(b'{'), Some(b'}')) | (Some(b'['), Some(b']')) => 0.7,
            (Some(b'{' | b'['), _) => 0.3,
            _ => 0.0,
        };
        let extension = ctx
            .file_name
            .is_some_and(|name| name.to_ascii_lowercase().ends_with(".json"));
        let score = match (structure > 0.0, extension) {
            (true, true) => structure + 0.2,
            (true, false) => structure,
            (false, true) => 0.25,
            (false, false) => 0.0,
        };
        Confidence::new(score)
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<Decoded> {
        decode_text(input, ctx, parse::KeyStyle::Strict)
    }

    fn encode(&self, doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
        let profile = Profile::from_params(params)?;
        let written = write::write_document(doc, &profile)?;
        let text = std::str::from_utf8(&written).map_err(|_| {
            Error::new(
                JSON_UNWRITABLE,
                "internal error: the writer produced invalid UTF-8",
            )
        })?;
        crate::text_encoding::encode_with(text, profile.text_encoding, profile.bom)
    }
}
