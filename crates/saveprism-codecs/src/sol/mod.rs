//! Adobe Flash Local Shared Objects (AMF0 / AMF3), parsed as data only.
//!
//! # Tree and editing
//!
//! The root is an `Object` keyed by the LSO's top-level property names. AMF objects become
//! `Object` nodes (the class is in the `SOL:` hint); dense arrays become `Array`; AMF3
//! associative properties are represented by an `Object` child. Object/string references are
//! `Ref` nodes and are never expanded. Dates, XML, ByteArray and dictionary values are read-only.
//! Containers and names are read-only; unreferenced numbers, booleans and strings are editable.
//! AMF3 integers are limited to the signed 29-bit range. Values whose safe rewrite would change
//! the string-reference table are read-only or reject that specific edit.
//!
//! # Exact writing and safety
//!
//! The codec retains the original byte stream and patches only edited scalar spans; this preserves
//! non-canonical U29 encodings, trait details, object references and terminators. It recomputes the
//! LSO length field after edits. Every length is checked against the remaining input before use,
//! and nesting and node counts use the core limits. Class code is never loaded; AMF3 externalizable
//! objects fail with an unsupported-format error.
//!
//! Errors: `SP-SOL-001` malformed or truncated input, `SP-SOL-002` unsupported AMF feature, and
//! `SP-SOL-003` an invalid or unrepresentable edit.

mod read;
mod write;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    Codec, Confidence, DecodeCtx, Decoded, Error, ErrorCode, Params, Registry, Result, SaveDoc,
    SniffCtx,
};

pub const SOL_PARSE: ErrorCode = ErrorCode::new("SP-SOL-001");
pub const SOL_UNSUPPORTED: ErrorCode = ErrorCode::new("SP-SOL-002");
pub const SOL_WRITE: ErrorCode = ErrorCode::new("SP-SOL-003");
pub(crate) const PREFIX: &str = "SOL:";
pub(crate) const ORIGINAL: &str = "sol:original";
pub(crate) const STRING_NO_EMPTY: &str = "sol:string-no-empty";

pub fn register(reg: &mut Registry) {
    reg.add_codec(Arc::new(FlashSolCodec));
}

#[derive(Clone, Copy, Debug, Default)]
pub struct FlashSolCodec;

impl Codec for FlashSolCodec {
    fn id(&self) -> &'static str {
        "format.flash-sol"
    }

    fn sniff(&self, input: &[u8], ctx: &SniffCtx<'_>) -> Confidence {
        if input.len() >= 10
            && input.get(..2) == Some(&[0, 0xbf])
            && input.get(6..10) == Some(b"TCSO")
        {
            return Confidence::new(0.99);
        }
        if ctx
            .file_name
            .is_some_and(|name| name.to_ascii_lowercase().ends_with(".sol"))
        {
            Confidence::new(0.35)
        } else {
            Confidence::NONE
        }
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<Decoded> {
        ctx.limits().check_input(len_u64(input.len()))?;
        let (doc, params) = read::decode(input, ctx.limits())?;
        ctx.charge_nodes(len_u64(doc.len().saturating_sub(1)))?;
        Ok(Decoded { doc, params })
    }

    fn encode(&self, doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
        write::encode(doc, params)
    }
}

pub(crate) fn parse_error(message: impl Into<String>, at: usize) -> Error {
    Error::new(SOL_PARSE, message).with_offset(len_u64(at))
}
