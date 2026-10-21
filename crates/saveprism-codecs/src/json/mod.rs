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

mod parse;
mod profile;
mod write;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use saveprism_core::Params;
use saveprism_core::limits::len_u64;
use saveprism_core::{
    Codec, Confidence, DecodeCtx, Decoded, Error, ErrorCode, Registry, Result, SaveDoc, SniffCtx,
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

const BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];

/// Registers the JSON codec as `format.json`.
pub fn register(reg: &mut Registry) {
    reg.add_codec(Arc::new(JsonCodec));
}

/// The `format.json` codec.
pub(crate) struct JsonCodec;

fn is_ws(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r')
}

impl Codec for JsonCodec {
    fn id(&self) -> &'static str {
        "format.json"
    }

    fn sniff(&self, input: &[u8], ctx: &SniffCtx<'_>) -> Confidence {
        let body = input.strip_prefix(&BOM[..]).unwrap_or(input);
        let first = body
            .iter()
            .position(|byte| !is_ws(*byte))
            .and_then(|index| body.get(index))
            .copied();
        let last = body
            .iter()
            .rposition(|byte| !is_ws(*byte))
            .and_then(|index| body.get(index))
            .copied();
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
        let (bom, body) = match input.strip_prefix(&BOM[..]) {
            Some(rest) => (true, rest),
            None => (false, input),
        };
        let base = if bom { len_u64(BOM.len()) } else { 0 };
        let text = std::str::from_utf8(body).map_err(|source| {
            Error::new(
                JSON_UTF8,
                format!(
                    "the input is not valid UTF-8 (invalid byte at offset {})",
                    base + len_u64(source.valid_up_to())
                ),
            )
            .with_offset(base + len_u64(source.valid_up_to()))
            .with_source(source)
        })?;

        let parsed = parse::parse(text, base, ctx)?;
        ctx.charge_nodes(len_u64(parsed.doc.len()))?;

        let profile = Profile::infer(text, &parsed.observed, bom);
        let regular =
            write::write_document(&parsed.doc, &profile).is_ok_and(|written| written == input);
        Ok(Decoded {
            doc: parsed.doc,
            params: profile.to_params(regular),
        })
    }

    fn encode(&self, doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
        let profile = Profile::from_params(params)?;
        write::write_document(doc, &profile)
    }
}
