//! Java Object Serialization streams parsed as data; no class is loaded or instantiated.
//!
//! The codec retains the input and patches scalar byte spans, preserving descriptor flags,
//! non-canonical details, annotations, references and block data exactly.

mod read;
mod write;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    Codec, Confidence, DecodeCtx, Decoded, ErrorCode, Params, Registry, Result, SaveDoc, SniffCtx,
};

pub const JAVA_PARSE: ErrorCode = ErrorCode::new("SP-JAVA-001");
pub const JAVA_UNSUPPORTED: ErrorCode = ErrorCode::new("SP-JAVA-002");
pub const JAVA_WRITE: ErrorCode = ErrorCode::new("SP-JAVA-003");
pub(crate) const PREFIX: &str = "JAVASER:";
pub(crate) const ORIGINAL: &str = "javaser:original";

pub fn register(reg: &mut Registry) {
    reg.add_codec(Arc::new(JavaSerializationCodec));
}

#[derive(Clone, Copy, Debug, Default)]
pub struct JavaSerializationCodec;

impl Codec for JavaSerializationCodec {
    fn id(&self) -> &'static str {
        "format.java-serialization"
    }
    fn sniff(&self, input: &[u8], _: &SniffCtx<'_>) -> Confidence {
        if input.starts_with(&[0xac, 0xed, 0, 5]) {
            Confidence::new(0.99)
        } else {
            Confidence::NONE
        }
    }
    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<Decoded> {
        ctx.limits().check_input(len_u64(input.len()))?;
        let (doc, mut params) = read::decode(input, ctx)?;
        params.set(ORIGINAL, input.to_vec());
        ctx.charge_nodes(len_u64(doc.len().saturating_sub(1)))?;
        Ok(Decoded { doc, params })
    }
    fn encode(&self, doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
        write::encode(doc, params)
    }
}
