//! `format.plist`: Apple property lists, XML and binary (`bplist00`).
//!
//! Written from the public Apple property list documentation and the published XML plist DTD.
//! XML plists are parsed with `quick-xml` (MIT) into the plist types; a binary plist is parsed as
//! data from its documented object/offset tables (see `binary.rs`).
//!
//! # Tree
//!
//! The root is the single value inside `<plist>`. A dict is a read-only `Object`
//! (`PLIST:dict`), an array a read-only `Array` (`PLIST:array`). Editable leaves are strings,
//! integers, reals and booleans (`PLIST:string`, `PLIST:int`, `PLIST:float`, `PLIST:bool`);
//! `<data>`, `<date>` and every binary-plist node are read-only. A repeated dict key is kept as a
//! second entry with the same key (a plist dict is ordered and may repeat a key).
//!
//! # Exact writing
//!
//! For XML plists the original text is the codec parameter `original`; writing replaces only the
//! spans of edited leaves and copies all other bytes. A `true`/`false` element is replaced as a
//! whole so its tag changes with the value. Binary plists are read-only: writing gives the exact
//! original bytes (the offset table makes in-place edits unsafe), so the round-trip guard passes but
//! editing is refused with a reason.

pub mod binary;
#[cfg(test)]
pub mod tests;
pub mod xml;

use std::sync::Arc;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    Codec, Confidence, DecodeCtx, Decoded, Error, ErrorCode, NodeFlags, ParamValue, Params,
    Registry, Result, SaveDoc, SniffCtx, TypeHint,
};

use crate::text_common::{apply_patches, span_range};

/// The input is not a property list.
pub const PLIST_PARSE: ErrorCode = ErrorCode::new("SP-PLIST-001");
/// The document cannot be written back.
pub const PLIST_WRITE: ErrorCode = ErrorCode::new("SP-PLIST-002");

pub(crate) const PREFIX: &str = "PLIST:";

/// Registers `format.plist`.
pub fn register(reg: &mut Registry) {
    reg.add_codec(Arc::new(PlistCodec));
}

/// The `format.plist` codec.
#[derive(Clone, Copy, Debug, Default)]
pub struct PlistCodec;

impl Codec for PlistCodec {
    fn id(&self) -> &'static str {
        "format.plist"
    }

    fn sniff(&self, input: &[u8], ctx: &SniffCtx<'_>) -> Confidence {
        if input.starts_with(b"bplist00") {
            return Confidence::new(0.98);
        }
        let body = input.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(input);
        let Ok(text) = std::str::from_utf8(body) else {
            return Confidence::NONE;
        };
        let start = text.trim_start();
        if !start.starts_with('<') {
            return Confidence::NONE;
        }
        let head = start.get(..512).unwrap_or(start).to_ascii_lowercase();
        let named = ctx
            .file_name
            .is_some_and(|name| name.to_ascii_lowercase().ends_with(".plist"));
        if head.contains("<plist") || head.contains("<!doctype plist") {
            return Confidence::new(0.97);
        }
        if named {
            Confidence::new(0.9)
        } else {
            Confidence::NONE
        }
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<Decoded> {
        ctx.limits().check_input(len_u64(input.len()))?;
        let doc = if input.starts_with(b"bplist00") {
            binary::parse(input, ctx)?
        } else {
            xml::parse(input, ctx)?
        };
        ctx.charge_nodes(len_u64(doc.len()))?;
        let mut params = Params::new();
        params.set("original", input.to_vec());
        Ok(Decoded { doc, params })
    }

    fn encode(&self, doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
        let Some(ParamValue::Bytes(original)) = params.get("original") else {
            return Err(Error::new(PLIST_WRITE, "the original plist is missing"));
        };
        if original.starts_with(b"bplist00") {
            return Ok(original.clone());
        }
        let mut patches = Vec::new();
        let mut stack = vec![doc.root()];
        let mut visited = 0usize;
        while let Some(id) = stack.pop() {
            visited = visited.saturating_add(1);
            if visited > doc.len().saturating_add(1) {
                return Err(Error::new(PLIST_WRITE, "cyclic plist tree"));
            }
            let Some(node) = doc.get(id) else {
                return Err(Error::new(PLIST_WRITE, "invalid node in the plist tree"));
            };
            stack.extend(doc.children(id));
            if node.flags.contains(NodeFlags::READ_ONLY) || node.lexeme.is_some() {
                continue;
            }
            let Some(hint) = node.type_hint.as_ref().map(TypeHint::as_str) else {
                continue;
            };
            let Some(rendered) = xml::render(hint, &node.value) else {
                continue;
            };
            let (start, end) = span_range(node, PLIST_WRITE)?;
            let old = original
                .get(start..end)
                .ok_or_else(|| Error::new(PLIST_WRITE, "value span outside the original text"))?;
            if old != rendered.as_bytes() {
                patches.push((start, end, rendered.into_bytes()));
            }
        }
        apply_patches(original, PLIST_WRITE, &mut patches)
    }
}
