//! RPG Maker MV and MZ saves.
//!
//! - MV (`.rpgsave`): LZ-String (base64) over JSON. The recipe is `compression.lz-string > format.json`.
//! - MZ (`.rmmzsave`): zlib bytes written as a UTF-8 "binary string". The recipe is
//!   `encoding.js-binary-string > compression.zlib > format.json` (this module adds the first layer).
//!
//! JsonEx (the RPG Maker serialiser) writes meta keys that start with `@`: `@` (class name),
//! `@c` (object id), `@r` (reference to an id) and `@a` (wrapped array). `format.json` keeps them
//! like any other key. The thin codec `format.rpgmaker-json` decodes and encodes exactly like
//! `format.json` and only marks the value node of every `@...` key with the hint `JsonEx:meta`.
//! `@r` references are kept, never followed.
//!
//! `format.rpgmaker-json` is picked by the detector when the JSON text holds a JsonEx marker
//! (`"@":"Game_`, `"@c":` or `"@a":`) in its first 64 KB; its `sniff` score is above `format.json`'s.
//! Plain JSON without a marker stays `format.json`.
//!
//! Compression of MZ saves: pako and miniz_oxide may produce different deflate bytes, so an MZ save
//! that the zlib layer cannot reproduce exactly is still written back as an equal document
//! (the round-trip guard reports "semantic").

mod binstr;
mod names;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use saveprism_core::{
    Codec, Confidence, DecodeCtx, Decoded, Error, ErrorCode, NodeId, Params, Registry, Result,
    SaveDoc, SniffCtx, TypeHint, Value,
};

use crate::json::JsonCodec;

pub use binstr::BinaryStringLayer;
pub use names::NameDictionary;

/// The text is not valid UTF-8, so it is not an MZ binary string.
pub const RPG_BINSTR_UTF8: ErrorCode = ErrorCode::new("SP-RPG-001");
/// The text holds a character above U+00FF, so it is not an MZ binary string.
pub const RPG_BINSTR_RANGE: ErrorCode = ErrorCode::new("SP-RPG-002");
/// A game data file (`System.json`, ...) is not valid JSON.
pub const RPG_DATA_INVALID: ErrorCode = ErrorCode::new("SP-RPG-003");
/// The JsonEx meta pass could not rebuild the document.
pub const RPG_META_FAILED: ErrorCode = ErrorCode::new("SP-RPG-004");

/// Type hint of the value node of a JsonEx meta key (`@`, `@c`, `@r`, `@a`).
pub const JSONEX_META: &str = "JsonEx:meta";

/// Registers the binary-string layer and the `format.rpgmaker-json` codec.
pub fn register(reg: &mut Registry) {
    reg.add_layer(Arc::new(BinaryStringLayer));
    reg.add_codec(Arc::new(RpgMakerJsonCodec));
}

/// How many leading bytes `sniff` looks at.
const SNIFF_WINDOW: usize = 64 * 1024;

/// JsonEx markers searched by `sniff`.
const MARKERS: [&[u8]; 3] = [b"\"@\":\"Game_", b"\"@c\":", b"\"@a\":"];

/// `format.rpgmaker-json`: `format.json` plus the `JsonEx:meta` hint.
#[derive(Clone, Copy, Debug, Default)]
pub struct RpgMakerJsonCodec;

fn meta_failed(message: &str) -> Error {
    Error::new(RPG_META_FAILED, message)
}

fn mapped(remap: &[Option<NodeId>], id: NodeId) -> Result<NodeId> {
    remap
        .get(id.index())
        .copied()
        .flatten()
        .ok_or_else(|| meta_failed("the document order is not children first"))
}

/// Copies `src` and gives the value node of every `@...` key the `JsonEx:meta` hint.
/// Only nodes reachable from the root are copied; children always come before their parents
/// in a document from the JSON codec, so copying in id order keeps every reference valid.
fn mark_meta(src: &SaveDoc, ctx: &DecodeCtx<'_>) -> Result<SaveDoc> {
    let mut marked = vec![false; src.len()];
    let mut seen = vec![false; src.len()];
    let mut reachable: Vec<NodeId> = Vec::new();
    let mut stack = vec![src.root()];
    while let Some(id) = stack.pop() {
        match seen.get_mut(id.index()) {
            Some(slot) if !*slot => *slot = true,
            _ => continue,
        }
        reachable.push(id);
        if let Some(node) = src.get(id) {
            if let Value::Object(entries) = &node.value {
                for (key, child) in entries {
                    if key.starts_with('@')
                        && let Some(slot) = marked.get_mut(child.index())
                    {
                        *slot = true;
                    }
                }
            }
            stack.extend(src.children(id));
            if let Value::Ref(target) = &node.value {
                stack.push(*target);
            }
        }
    }
    reachable.sort();

    let mut out = ctx.new_doc();
    let mut remap: Vec<Option<NodeId>> = vec![None; src.len()];
    for id in reachable {
        let Some(original) = src.get(id) else {
            return Err(meta_failed("a node of the document is missing"));
        };
        let mut node = original.clone();
        node.value = match &original.value {
            Value::Array(items) => Value::Array(
                items
                    .iter()
                    .map(|item| mapped(&remap, *item))
                    .collect::<Result<Vec<_>>>()?,
            ),
            Value::Object(entries) => Value::Object(
                entries
                    .iter()
                    .map(|(key, child)| Ok((key.clone(), mapped(&remap, *child)?)))
                    .collect::<Result<Vec<_>>>()?,
            ),
            Value::Map(entries) => Value::Map(
                entries
                    .iter()
                    .map(|(key, child)| Ok((mapped(&remap, *key)?, mapped(&remap, *child)?)))
                    .collect::<Result<Vec<_>>>()?,
            ),
            Value::Ref(target) => Value::Ref(mapped(&remap, *target)?),
            other => other.clone(),
        };
        let is_float = node
            .type_hint
            .as_ref()
            .is_some_and(|hint| hint.as_str() == "JSON:float");
        if marked.get(id.index()).copied().unwrap_or(false) && !is_float {
            node.type_hint = Some(TypeHint::new(JSONEX_META));
        }
        let new_id = out.push(node)?;
        if let Some(slot) = remap.get_mut(id.index()) {
            *slot = Some(new_id);
        }
    }
    out.set_root(mapped(&remap, src.root())?);
    Ok(out)
}

impl Codec for RpgMakerJsonCodec {
    fn id(&self) -> &'static str {
        "format.rpgmaker-json"
    }

    /// JSON-looking input with a JsonEx marker in the first 64 KB scores above `format.json`
    /// (whose best score is 0.9); anything else is `NONE`. The file name is not used.
    fn sniff(&self, input: &[u8], _ctx: &SniffCtx<'_>) -> Confidence {
        let body = input.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(input);
        let first = body
            .iter()
            .find(|byte| !matches!(byte, b' ' | b'\t' | b'\n' | b'\r'))
            .copied();
        if !matches!(first, Some(b'{' | b'[')) {
            return Confidence::NONE;
        }
        let head = body.get(..SNIFF_WINDOW).unwrap_or(body);
        let marked = MARKERS
            .iter()
            .any(|marker| head.windows(marker.len()).any(|window| window == *marker));
        if marked {
            Confidence::new(0.95)
        } else {
            Confidence::NONE
        }
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<Decoded> {
        let decoded = JsonCodec.decode(input, ctx)?;
        let doc = mark_meta(&decoded.doc, ctx)?;
        Ok(Decoded {
            doc,
            params: decoded.params,
        })
    }

    fn encode(&self, doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
        JsonCodec.encode(doc, params)
    }
}
