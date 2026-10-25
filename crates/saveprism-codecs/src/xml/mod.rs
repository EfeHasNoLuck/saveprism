//! `format.xml`: XML save files (Stardew-style config and save documents).
//!
//! Written from the public XML 1.0 specification and the W3C "XML Entity Definitions for
//! Characters" list of the five predefined entities. Text is tokenized with `quick-xml` (MIT); the
//! tree is built here from the token spans. Nothing is evaluated, and no external entity or DTD is
//! ever fetched or resolved (XXE is structurally impossible).
//!
//! # Tree
//!
//! The root is a read-only `Object` (`XML:document`) with the document element under its tag name.
//! An element is a read-only `Object` (`XML:element`); attributes are keys prefixed with `@`, text
//! is the key `#text`, CDATA `#cdata`, comments `#comment`, processing instructions `#pi`, the XML
//! declaration `#declaration` and the doctype `#doctype`. A repeated key is numbered in document
//! order: the second `<item>` is `item#2`, the third `item#3`, and so on.
//!
//! Editable: attribute values and plain character data (`XML:attribute`, `XML:text`); they are
//! re-escaped with the five predefined entities. Entities in existing text are decoded for the
//! value but their bytes are left alone until the value is edited. CDATA, comments, declarations,
//! processing instructions and a doctype are read-only, as are all containers, so an edit never
//! changes the document shape.
//!
//! # Exact writing
//!
//! The original text is the codec parameter `original`. Writing replaces only the token spans of
//! edited leaves and copies every other byte, including comments, indentation and line endings.

pub mod read;
#[cfg(test)]
pub mod tests;
pub mod write;

use std::sync::Arc;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    Codec, Confidence, DecodeCtx, Decoded, Error, ErrorCode, ParamValue, Params, Registry, Result,
    SaveDoc, SniffCtx,
};

/// The input is not well-formed XML.
pub const XML_PARSE: ErrorCode = ErrorCode::new("SP-XML-001");
/// The document cannot be written back.
pub const XML_WRITE: ErrorCode = ErrorCode::new("SP-XML-002");

/// Hint prefix of this codec.
pub(crate) const PREFIX: &str = "XML:";

/// Registers the `format.xml` codec.
pub fn register(reg: &mut Registry) {
    reg.add_codec(Arc::new(XmlCodec));
}

/// The `format.xml` codec.
#[derive(Clone, Copy, Debug, Default)]
pub struct XmlCodec;

impl Codec for XmlCodec {
    fn id(&self) -> &'static str {
        "format.xml"
    }

    fn sniff(&self, input: &[u8], ctx: &SniffCtx<'_>) -> Confidence {
        let Some(decoded) = crate::text_encoding::decode_sniff(input) else {
            return Confidence::NONE;
        };
        let text = decoded.text.as_str();
        let start = text.trim_start();
        if !start.starts_with('<') {
            return Confidence::NONE;
        }
        let lower = start.get(..64).unwrap_or(start).to_ascii_lowercase();
        // The plist codec owns plist documents; keep this an alternative only.
        if lower.starts_with("<plist")
            || (lower.starts_with("<!doctype plist"))
            || lower.contains("<!doctype plist")
        {
            return Confidence::new(0.4);
        }
        let named = ctx.file_name.is_some_and(|name| {
            let name = name.to_ascii_lowercase();
            name.ends_with(".xml") || name.ends_with(".xsd") || name.ends_with(".config")
        });
        if !start.contains('>') {
            return Confidence::NONE;
        }
        let declared = lower.starts_with("<?xml") || lower.starts_with("<!doctype");
        let element_like = declared
            || start
                .get(1..)
                .and_then(|rest| rest.chars().next())
                .is_some_and(|character| {
                    character.is_ascii_alphabetic() || matches!(character, '_' | ':')
                });
        let score = match (declared, element_like, named) {
            (_, false, _) => return Confidence::NONE,
            (true, true, true) => 0.95,
            (true, true, false) => 0.85,
            (false, true, true) => 0.8,
            (false, true, false) => 0.62,
        };
        Confidence::new(score)
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<Decoded> {
        ctx.limits().check_input(len_u64(input.len()))?;
        let decoded = crate::text_encoding::decode_sniff(input)
            .ok_or_else(|| Error::new(XML_PARSE, "XML text encoding could not be detected"))?;
        let doc = read::parse(&decoded.text, ctx)?;
        ctx.charge_nodes(len_u64(doc.len()))?;
        let mut params = Params::new();
        params.set("original", input.to_vec());
        params.set("text_encoding", decoded.encoding.name());
        Ok(Decoded { doc, params })
    }

    fn encode(&self, doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
        let Some(ParamValue::Bytes(original)) = params.get("original") else {
            return Err(Error::new(XML_WRITE, "the original XML text is missing"));
        };
        let decoded = crate::text_common::decode_original(original, XML_WRITE)?;
        let mut patches = write::encode(doc, decoded.text.as_bytes())?;
        crate::text_common::apply_encoded_patches(original, &decoded, XML_WRITE, &mut patches)
    }
}
