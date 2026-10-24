//! `format.naninovel`: Naninovel (Unity visual novel engine) save slots.
//!
//! Naninovel serializes its game state to JSON. A slot is an `objectJsonMap` whose `keys` are
//! service type names and whose `values` are the services' state, each written as a JSON *string*
//! (see the public Naninovel state-management guide). The custom variables live in the
//! `Naninovel.CustomVariableManager+GameState` service: `LocalVariables` and `GlobalVariables`
//! arrays of `{name, scope, value}`, where `value.type` is 0 (text, `stringValue`), 1 (number,
//! `numericValue`) or 2 (boolean, `booleanValue`).
//!
//! This codec keeps `format.json`'s exact round trip and additionally inlines those state strings
//! as ordinary JSON objects, so the editor can list and edit values inside them through real
//! paths. An unchanged state is written back with its original text; a changed one is written as
//! compact JSON. Nothing from the save is executed.
//!
//! Layout sources (behaviour only; no code copied): Naninovel's public guide
//! <https://naninovel.com/guide/state-management> (JSON slots, game/global state) and
//! <https://naninovel.com/guide/custom-variables> (variable types and global `g_` variables),
//! plus the public third-party readers `truongthang2211/RPGSaveEditor` (MIT) and
//! `Zhehao-w/butter-manager` (MIT) only to confirm the `objectJsonMap` and variable field names.

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::sync::Arc;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    Codec, Confidence, DecodeCtx, Decoded, Error, ErrorCode, Node, NodeFlags, NodeId, Params,
    Registry, Result, SaveDoc, SniffCtx, TypeHint, Value,
};

use crate::json::{JsonCodec, compact_text, parse_text};

/// The save has an `objectJsonMap` that is not the expected `keys` / `values` pair.
pub const NANI_BAD_MAP: ErrorCode = ErrorCode::new("SP-NANI-001");
/// A state string could be parsed as JSON but could not be copied into the document.
pub const NANI_BAD_STATE: ErrorCode = ErrorCode::new("SP-NANI-002");
/// A changed state could not be written as JSON text.
pub const NANI_WRITE: ErrorCode = ErrorCode::new("SP-NANI-003");

/// Hint of a state node whose original text is kept in its `lexeme`.
pub const STATE_HINT: &str = "NANI:state";

/// Registers the Naninovel codec as `format.naninovel`.
pub fn register(reg: &mut Registry) {
    reg.add_codec(Arc::new(NaninovelCodec));
}

/// The `format.naninovel` codec.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct NaninovelCodec;

/// The type name of the custom-variable service (assembly part removed).
const CUSTOM_VARIABLES: &str = "Naninovel.CustomVariableManager+GameState";

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && haystack.windows(needle.len()).any(|w| w == needle)
}

fn is_ws(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r')
}

/// A type name without its assembly part.
pub fn short_name(name: &str) -> &str {
    name.split(',').next().unwrap_or(name).trim()
}

/// True for the custom-variable state of a game save.
pub fn is_custom_variables(key: &str) -> bool {
    short_name(key) == CUSTOM_VARIABLES
}

fn bad(message: impl Into<String>) -> Error {
    Error::new(NANI_BAD_MAP, message)
}

fn entry(doc: &SaveDoc, id: NodeId, key: &str) -> Option<NodeId> {
    match doc.get(id).map(|node| &node.value) {
        Some(Value::Object(entries)) => entries
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, child)| *child),
        _ => None,
    }
}

fn text(doc: &SaveDoc, id: NodeId) -> Option<&str> {
    match doc.get(id).map(|node| &node.value) {
        Some(Value::Str(text)) => Some(text.as_str()),
        _ => None,
    }
}

fn items(doc: &SaveDoc, id: NodeId) -> Option<&[NodeId]> {
    match doc.get(id).map(|node| &node.value) {
        Some(Value::Array(items)) => Some(items.as_slice()),
        _ => None,
    }
}

/// Copies the value at `source` (and its children) into `target`. The copied root is marked as a
/// Naninovel state and keeps `state_text` as its original text.
fn copy_node(
    source: &SaveDoc,
    target: &mut SaveDoc,
    source_root: NodeId,
    id: NodeId,
    state_text: &str,
    copied: &mut HashMap<NodeId, NodeId>,
) -> Result<NodeId> {
    if let Some(&done) = copied.get(&id) {
        return Ok(done);
    }
    let node = source
        .get(id)
        .ok_or_else(|| Error::new(NANI_BAD_STATE, "the state holder refers to a missing node"))?;
    let new_id = target.reserve()?;
    copied.insert(id, new_id);
    let mut value = node.value.clone();
    match &mut value {
        Value::Array(children) => {
            for child in children.iter_mut() {
                *child = copy_node(source, target, source_root, *child, state_text, copied)?;
            }
        }
        Value::Object(entries) => {
            for (_, child) in entries.iter_mut() {
                *child = copy_node(source, target, source_root, *child, state_text, copied)?;
            }
        }
        Value::Map(entries) => {
            for (key, child) in entries.iter_mut() {
                *key = copy_node(source, target, source_root, *key, state_text, copied)?;
                *child = copy_node(source, target, source_root, *child, state_text, copied)?;
            }
        }
        Value::Ref(child) => {
            *child = copy_node(source, target, source_root, *child, state_text, copied)?;
        }
        _ => {}
    }
    let is_root = id == source_root;
    let mut new_node = Node::new(value);
    new_node.type_hint = if is_root {
        Some(TypeHint::new(STATE_HINT))
    } else {
        node.type_hint.clone()
    };
    new_node.lexeme = if is_root {
        Some(state_text.into())
    } else {
        node.lexeme.clone()
    };
    new_node.flags = NodeFlags::empty();
    target.finish(new_id, new_node)?;
    Ok(new_id)
}

/// Inlines every state value that is JSON text. Returns the state node ids with their key names.
fn expand(doc: &mut SaveDoc, ctx: &mut DecodeCtx<'_>) -> Result<Vec<(NodeId, String)>> {
    let root = doc.root();
    let map =
        entry(doc, root, "objectJsonMap").ok_or_else(|| bad("missing the objectJsonMap object"))?;
    let keys = entry(doc, map, "keys").ok_or_else(|| bad("missing objectJsonMap.keys"))?;
    let values = entry(doc, map, "values").ok_or_else(|| bad("missing objectJsonMap.values"))?;
    let key_items = items(doc, keys)
        .ok_or_else(|| bad("objectJsonMap.keys is not an array"))?
        .to_vec();
    let mut value_items = items(doc, values)
        .ok_or_else(|| bad("objectJsonMap.values is not an array"))?
        .to_vec();
    if key_items.len() != value_items.len() {
        return Err(bad(format!(
            "objectJsonMap has {} keys and {} values",
            key_items.len(),
            value_items.len()
        )));
    }
    if !key_items
        .iter()
        .any(|id| text(doc, *id).is_some_and(|key| short_name(key).starts_with("Naninovel.")))
    {
        return Err(bad("the state map holds no Naninovel service"));
    }

    let mut states = Vec::new();
    for (index, value) in value_items.iter_mut().enumerate() {
        let Some(key) = key_items
            .get(index)
            .and_then(|id| text(doc, *id))
            .map(str::to_string)
        else {
            continue;
        };
        let Some(inner) = text(doc, *value).map(str::to_string) else {
            continue;
        };
        let trimmed = inner.trim_start();
        if !(trimmed.starts_with('{') || trimmed.starts_with('[')) {
            continue;
        }
        let Ok(parsed) = parse_text(&inner, ctx) else {
            continue;
        };
        let inner_root = parsed.root();
        let Some(inner_node) = parsed.get(inner_root) else {
            continue;
        };
        if !matches!(inner_node.value, Value::Object(_) | Value::Array(_)) {
            continue;
        }
        ctx.charge_nodes(len_u64(parsed.len()))?;
        let mut copied = HashMap::new();
        let new_id = copy_node(&parsed, doc, inner_root, inner_root, &inner, &mut copied)?;
        *value = new_id;
        states.push((new_id, key));
    }
    doc.set_value(values, Value::Array(value_items))?;
    Ok(states)
}

fn canonical(text: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(text)
        .ok()
        .and_then(|value| serde_json::to_string(&value).ok())
}

fn same_json(original: &str, current: &[u8]) -> bool {
    let Ok(current) = std::str::from_utf8(current) else {
        return false;
    };
    match (canonical(original), canonical(current)) {
        (Some(left), Some(right)) => left == right,
        _ => original == current,
    }
}

impl Codec for NaninovelCodec {
    fn id(&self) -> &'static str {
        "format.naninovel"
    }

    fn sniff(&self, input: &[u8], _ctx: &SniffCtx<'_>) -> Confidence {
        let body = input.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(input);
        let first = body.iter().position(|byte| !is_ws(*byte));
        if first.and_then(|index| body.get(index)) != Some(&b'{') {
            return Confidence::NONE;
        }
        if !contains(body, b"\"objectJsonMap\"") {
            return Confidence::NONE;
        }
        let map = contains(body, b"\"keys\"") && contains(body, b"\"values\"");
        if contains(body, b"Naninovel.") && map {
            Confidence::new(0.97)
        } else if map {
            Confidence::new(0.6)
        } else {
            Confidence::NONE
        }
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<Decoded> {
        let mut decoded = JsonCodec.decode(input, ctx)?;
        expand(&mut decoded.doc, ctx)?;
        Ok(decoded)
    }

    fn encode(&self, doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
        let mut plain = doc.clone();
        let states: Vec<NodeId> = (0..plain.len())
            .filter_map(NodeId::from_index)
            .filter(|id| {
                plain
                    .get(*id)
                    .and_then(|node| node.type_hint.as_ref())
                    .is_some_and(|hint| hint.as_str() == STATE_HINT)
            })
            .collect();
        for id in states {
            let Some(node) = plain.get(id) else {
                continue;
            };
            let original = node.lexeme.clone();
            let current = compact_text(&plain, id)?;
            let written = match original {
                Some(original) if same_json(&original, &current) => original.into_string(),
                _ => String::from_utf8(current).map_err(|_| {
                    Error::new(NANI_WRITE, "a changed Naninovel state is not valid UTF-8")
                })?,
            };
            plain.set_value(id, Value::Str(written))?;
        }
        JsonCodec.encode(&plain, params)
    }
}
