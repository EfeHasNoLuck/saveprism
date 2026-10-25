//! `format.tjs2`: KiriKiri / KAG TJS2 structured text saves (`.ksd` save data, `.kdt` system data).
//!
//! Written from the public KiriKiri Z source (`github.com/krkrz/krkrz`, `tjsDictionary.cpp`,
//! `tjsArray.cpp`, `tjsUtils.cpp`, `tjsString.cpp`, `tjsVariant.cpp`, `base/TextStream.cpp`; the
//! project is BSD-style licensed). Only behaviour was read: the text is parsed into a data tree,
//! nothing in it is evaluated or executed.
//!
//! `Dictionary.saveStruct` and `Array.saveStruct` write a TJS2 expression. A dictionary is
//! `(const) %[ "key" => value, ... ]`, an array `(const) [ value, ... ]`; nested containers repeat
//! the `(const)` prefix. Scalar forms are an integer (decimal), a real (a hexadecimal float with a
//! decimal comment, `0x1.8p+1 /* 3 */`), a double-quoted string with C escapes, `<% AA BB %>` for
//! octets, and `void` / `null`.
//!
//! The same data is often written through a KiriKiri text stream (`base/TextStream.cpp`): plain
//! UTF-16LE with a BOM, a simple per-code-unit scramble (`FE FE 01` mode) or a zlib-compressed
//! stream (`FE FE 02` mode). Those wrappers are the `obfuscation.kirikiri` layer; this codec sees
//! the UTF-16 text.
//!
//! # Tree
//!
//! The root is the saved value: an `Object` for a dictionary or an `Array` for an array.
//!
//! | TJS2 value | Node | Hint | Editable |
//! |---|---|---|---|
//! | dictionary `%[...]` | `Object` (read-only) | `TJS:dictionary` | no |
//! | array `[...]` | `Array` (read-only) | `TJS:array` | no |
//! | string `"..."` | `Str` | `TJS:string` | yes |
//! | integer | `Int` (I64) | `TJS:int` | yes |
//! | real | `Float` (F64) | `TJS:real` | yes |
//! | octet `<% ... %>` | `Bytes` (read-only) | `TJS:octet` | no |
//! | `void` / `null` | `Null` (read-only) | `TJS:void`, `TJS:null` | no |
//!
//! Dictionary keys must be string literals (what `saveStruct` writes) and are not separate nodes.
//! Containers are read-only, so an edit never changes the shape of the file.
//!
//! # Exact writing
//!
//! The original text is kept as the codec parameter `original`; every editable node records the
//! byte span of its token (a real includes its `/* ... */` comment). Writing replaces only the
//! tokens an edit changed and copies every other byte, including indentation and comments. The
//! file's text encoding (UTF-16LE, UTF-8, ...) is detected and an edit it cannot hold is refused.

mod read;
#[cfg(test)]
mod tests;
mod write;

use std::sync::Arc;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    Codec, Confidence, DecodeCtx, Decoded, Error, ErrorCode, ParamValue, Params, Registry, Result,
    SniffCtx, codes,
};

use crate::text_encoding::decode_sniff;

/// The text is not a TJS2 dictionary/array expression.
pub const TJS_PARSE: ErrorCode = ErrorCode::new("SP-TJS-001");
/// The document cannot be written back (missing original text, span out of range).
pub const TJS_WRITE: ErrorCode = ErrorCode::new("SP-TJS-002");

/// Prefix of every hint this codec writes.
pub const PREFIX: &str = "TJS:";

/// Registers `format.tjs2`.
pub fn register(reg: &mut Registry) {
    reg.add_codec(Arc::new(Tjs2Codec));
}

/// The `format.tjs2` codec.
#[derive(Clone, Copy, Debug, Default)]
pub struct Tjs2Codec;

/// Removes leading whitespace and an optional `(const)` prefix.
fn expression_start(text: &str) -> &str {
    let mut rest = text.trim_start();
    if let Some(after) = rest.strip_prefix("(const)") {
        rest = after.trim_start();
    }
    rest
}

impl Codec for Tjs2Codec {
    fn id(&self) -> &'static str {
        "format.tjs2"
    }

    fn sniff(&self, input: &[u8], ctx: &SniffCtx<'_>) -> Confidence {
        let Some(decoded) = decode_sniff(input) else {
            return Confidence::NONE;
        };
        let text = decoded.text.as_str();
        let had_const = text.trim_start().starts_with("(const)");
        let start = expression_start(text);
        let named = ctx.file_name.is_some_and(|name| {
            let name = name.to_ascii_lowercase();
            name.ends_with(".ksd") || name.ends_with(".kdt")
        });
        let dictionary = start
            .strip_prefix('%')
            .is_some_and(|rest| rest.trim_start().starts_with('['));
        let array = start.starts_with('[');
        match (dictionary, array, named, had_const) {
            (true, _, true, _) => Confidence::new(0.95),
            (true, _, false, _) => Confidence::new(0.9),
            (false, true, true, _) => Confidence::new(0.85),
            (false, true, false, true) => Confidence::new(0.75),
            // A bare `[` is also a JSON or YAML list; only a weak hint.
            (false, true, false, false) => Confidence::new(0.3),
            _ => Confidence::NONE,
        }
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<Decoded> {
        ctx.limits().check_input(len_u64(input.len()))?;
        let decoded = decode_sniff(input).ok_or_else(|| {
            Error::new(TJS_PARSE, "the TJS2 save text encoding is not recognised")
        })?;
        let mut doc = ctx.new_doc();
        let root = read::parse(&decoded.text, &mut doc, ctx.limits().max_depth)?;
        doc.set_root(root);
        ctx.charge_nodes(len_u64(doc.len()))?;
        let mut params = Params::new();
        params.set("original", input.to_vec());
        Ok(Decoded { doc, params })
    }

    fn encode(&self, doc: &saveprism_core::SaveDoc, params: &Params) -> Result<Vec<u8>> {
        let Some(ParamValue::Bytes(original)) = params.get("original") else {
            return Err(Error::new(TJS_WRITE, "original TJS2 text is missing"));
        };
        write::encode(doc, original)
    }
}

/// The error code used for a nesting limit.
pub(crate) fn depth_error(depth: usize) -> Error {
    Error::new(
        codes::CORE_LIMIT_DEPTH,
        format!("TJS2 nesting depth {depth} is above the limit"),
    )
}
