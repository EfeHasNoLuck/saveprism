//! `format.gff4`: BioWare GFF V4.0/V4.1 (Dragon Age: Origins and Dragon Age II).
//!
//! The layout is implemented from the public Dragon Age toolset wiki page "GFF" (community
//! mirror: <https://github.com/toeofcharmander/DragonAgeToolsetWiki>, `Wiki/GFF.mediawiki`).
//! Version 4 is a different format from the V3.2 file in `format.gff`:
//!
//! - the magic is `GFF ` and the version `V4.0` or `V4.1` (the first five header fields are
//!   always big endian; the rest use the target platform's byte order);
//! - labels are four-byte numbers, there is no label array, field index array or list index
//!   array, and label values are not translated;
//! - fields carry a 16-bit type id and flags for "list", "struct" and "reference";
//! - the raw data block holds the struct data, so fields point into it by offset.
//!
//! This codec opens the tree **read-only**: the public document describes the container but not
//! the meaning of every label and type, and the game files carry platform-specific padding. Every
//! node is marked read-only and the writer returns the original bytes; SavePrism therefore never
//! guesses how to rewrite a Dragon Age save. Values are shown with their raw labels
//! (`/0x00001234`) and struct ids.
//!
//! All lengths and offsets are checked against the input, struct recursion is depth-limited and
//! cycles are refused. Nothing in the file is executed.

mod read;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    Codec, Confidence, DecodeCtx, Decoded, ErrorCode, Params, Registry, Result, SaveDoc, SniffCtx,
};

/// The file is truncated or its header is inconsistent.
pub const GFF4_PARSE: ErrorCode = ErrorCode::new("SP-GFF4-001");
/// A field uses a feature or type this codec does not read.
pub const GFF4_UNSUPPORTED: ErrorCode = ErrorCode::new("SP-GFF4-002");

/// Prefix of every hint this codec writes.
pub(crate) const HINT_PREFIX: &str = "GFF4:";
/// Parameter holding the original file bytes (the read-only writer returns them).
pub(crate) const P_ORIGINAL: &str = "gff4:original";
/// Fixed header length.
pub(crate) const HEADER_LEN: usize = 28;
/// Struct array element length.
pub(crate) const STRUCT_LEN: usize = 16;
/// Field array element length.
pub(crate) const FIELD_LEN: usize = 12;

pub(crate) const FLAG_LIST: u16 = 0x8000;
pub(crate) const FLAG_STRUCT: u16 = 0x4000;
pub(crate) const FLAG_REFERENCE: u16 = 0x2000;

/// Registers the GFF V4 codec as `format.gff4`.
pub fn register(reg: &mut Registry) {
    reg.add_codec(Arc::new(Gff4Codec));
}

/// The `format.gff4` codec.
#[derive(Clone, Copy, Debug, Default)]
pub struct Gff4Codec;

fn version(input: &[u8]) -> Option<&'static str> {
    if input.get(4..8) == Some(b"V4.0") {
        Some("V4.0")
    } else if input.get(4..8) == Some(b"V4.1") {
        Some("V4.1")
    } else {
        None
    }
}

impl Codec for Gff4Codec {
    fn id(&self) -> &'static str {
        "format.gff4"
    }

    /// The `GFF ` magic with `V4.0` or `V4.1` and a header whose arrays fit the input.
    fn sniff(&self, input: &[u8], ctx: &SniffCtx<'_>) -> Confidence {
        if input.get(..4) != Some(b"GFF ") || version(input).is_none() {
            return Confidence::NONE;
        }
        if input.len() < HEADER_LEN {
            return Confidence::NONE;
        }
        let endian = read::platform_endian(input);
        let Some(struct_count) = read::u32_at(input, 20, endian) else {
            return Confidence::NONE;
        };
        let Some(data_offset) = read::u32_at(input, 24, endian) else {
            return Confidence::NONE;
        };
        let Some(struct_bytes) = struct_count.checked_mul(STRUCT_LEN as u32) else {
            return Confidence::NONE;
        };
        let Some(struct_end) = (HEADER_LEN as u32).checked_add(struct_bytes) else {
            return Confidence::NONE;
        };
        let input_len = u32::try_from(input.len()).unwrap_or(u32::MAX);
        if struct_count == 0
            || usize::try_from(struct_end).map_or(true, |end| end > input.len())
            || data_offset < struct_end
            || data_offset > input_len
        {
            return Confidence::NONE;
        }
        let named = ctx
            .file_name
            .is_some_and(|name| name.to_ascii_lowercase().ends_with(".das"));
        Confidence::new(if named { 0.99 } else { 0.95 })
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<Decoded> {
        ctx.limits().check_input(len_u64(input.len()))?;
        let shared: &DecodeCtx<'_> = ctx;
        let (doc, params) = read::read_all(input, shared)?;
        ctx.charge_nodes(len_u64(doc.len()))?;
        Ok(Decoded { doc, params })
    }

    /// The tree is read-only, so an unchanged file is returned exactly.
    fn encode(&self, _doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
        params.bytes(P_ORIGINAL).map(<[u8]>::to_vec).map_err(|_| {
            saveprism_core::Error::new(GFF4_PARSE, "the original GFF4 bytes are missing")
        })
    }
}
