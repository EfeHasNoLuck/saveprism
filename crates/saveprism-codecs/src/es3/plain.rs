//! Plain (unencrypted) Easy Save 3 text: JSON with `{"__type": "...", "value": ...}` entries.
//!
//! Decoding runs the JSON codec and then copies the tree, giving the `value` node of every
//! `__type` object the hint `ES3:<type>`. Encoding undoes the one thing the hint replaced
//! (`JSON:float`, which the JSON writer needs to keep `1.0` a float) and runs the JSON writer.
//!
//! A dictionary whose key type is not a string is written with bare keys (`{0 : 159}`). The JSON
//! layer accepts them only for ES3 and keeps them bare on the way back; the container is marked
//! read-only (its keys cannot be renamed or added to without knowing the dictionary's key type),
//! while its values stay editable.
//!
//! A dictionary whose key type is a struct or array keeps its keys as nodes (a `Value::Map`): the
//! key's fields stay visible, the key subtree is read-only and the values stay editable.

use std::collections::{HashMap, HashSet};

use saveprism_core::{
    Codec, DecodeCtx, Decoded, Error, FloatWidth, Node, NodeFlags, NodeId, Params, Result, SaveDoc,
    TypeHint, Value,
};

use super::{ES3_ROOT, ES3_UNSUPPORTED};
use crate::json::{self, JsonCodec};

const TYPE_KEY: &str = "__type";
const VALUE_KEY: &str = "value";

/// What the copy does to the nodes it passes.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Add `ES3:<type>` hints.
    Annotate,
    /// Turn floats back into `JSON:float` for the JSON writer.
    Restore,
}

enum Work {
    Enter(NodeId),
    Exit(NodeId),
}

/// Decodes plain ES3 text.
pub(super) fn decode(input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<Decoded> {
    let decoded = json::decode_es3(input, ctx)
        .map_err(|error| error.context("reading an Easy Save 3 file"))?;
    let root_is_object = decoded
        .doc
        .get(decoded.doc.root())
        .is_some_and(|node| matches!(node.value, Value::Object(_)));
    if !root_is_object {
        return Err(Error::new(
            ES3_ROOT,
            "the root of an Easy Save 3 file must be an object of keys; this file starts with \
             another kind of value. Check that the file is an .es3 save and is not encrypted",
        ));
    }
    let doc = rebuild(&decoded.doc, ctx.new_doc(), Mode::Annotate)?;
    Ok(Decoded {
        doc,
        params: decoded.params,
    })
}

/// Encodes a document back to plain ES3 text.
pub(super) fn encode(doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
    let plain = rebuild(doc, SaveDoc::new(), Mode::Restore)?;
    JsonCodec.encode(&plain, params)
}

/// Copies the tree reachable from the root into `dst` (children first, iterative).
fn rebuild(src: &SaveDoc, mut dst: SaveDoc, mode: Mode) -> Result<SaveDoc> {
    let mut hints: HashMap<NodeId, String> = HashMap::new();
    let mut read_only: HashSet<NodeId> = HashSet::new();
    let mut results: Vec<NodeId> = Vec::new();
    let mut work = vec![Work::Enter(src.root())];

    while let Some(item) = work.pop() {
        match item {
            Work::Enter(id) => {
                let node = src.get(id).ok_or_else(missing)?;
                match &node.value {
                    Value::Array(_) | Value::Object(_) | Value::Map(_) => {
                        if mode == Mode::Annotate {
                            if let Value::Object(entries) = &node.value {
                                note_hint(src, entries, &mut hints);
                            }
                            if let Value::Map(entries) = &node.value {
                                read_only.insert(id);
                                for (key, _) in entries {
                                    mark_subtree(src, *key, &mut read_only);
                                }
                            }
                        }
                        work.push(Work::Exit(id));
                        for child in child_ids(src, id).into_iter().rev() {
                            work.push(Work::Enter(child));
                        }
                    }
                    Value::Ref(_) => return Err(unsupported()),
                    _ => {
                        let copy = copy_node(node, id, &hints, &read_only, mode);
                        results.push(dst.push(copy)?);
                    }
                }
            }
            Work::Exit(id) => {
                let node = src.get(id).ok_or_else(missing)?;
                let children = child_ids(src, id);
                let count = children.len();
                let start = results.len().saturating_sub(count);
                let children = results.split_off(start);
                let value = match &node.value {
                    Value::Array(_) => Value::Array(children),
                    Value::Object(entries) => Value::Object(
                        entries
                            .iter()
                            .map(|(key, _)| key.clone())
                            .zip(children)
                            .collect(),
                    ),
                    Value::Map(_) => {
                        let mut pairs = Vec::with_capacity(children.len() / 2);
                        let mut iter = children.into_iter();
                        while let (Some(key), Some(value)) = (iter.next(), iter.next()) {
                            pairs.push((key, value));
                        }
                        Value::Map(pairs)
                    }
                    _ => return Err(unsupported()),
                };
                let mut copy = copy_node(node, id, &hints, &read_only, mode);
                copy.value = value;
                results.push(dst.push(copy)?);
            }
        }
    }

    let root = results.pop().ok_or_else(missing)?;
    dst.set_root(root);
    Ok(dst)
}

/// Child ids in copy order; a map yields each key followed by its value.
fn child_ids(src: &SaveDoc, id: NodeId) -> Vec<NodeId> {
    match src.get(id).map(|node| &node.value) {
        Some(Value::Map(entries)) => entries
            .iter()
            .flat_map(|(key, value)| [*key, *value])
            .collect(),
        _ => src.children(id),
    }
}

/// Adds every node of `id`'s subtree to `set` (iterative).
fn mark_subtree(src: &SaveDoc, id: NodeId, set: &mut HashSet<NodeId>) {
    let mut stack = vec![id];
    while let Some(id) = stack.pop() {
        if !set.insert(id) {
            continue;
        }
        if let Some(node) = src.get(id) {
            match &node.value {
                Value::Array(items) => stack.extend(items.iter().copied()),
                Value::Object(entries) => stack.extend(entries.iter().map(|(_, child)| *child)),
                Value::Map(entries) => {
                    for (key, value) in entries {
                        stack.push(*key);
                        stack.push(*value);
                    }
                }
                _ => {}
            }
        }
    }
}

/// If `entries` holds a string `__type` and a `value`, remembers the hint for the `value` node.
fn note_hint(src: &SaveDoc, entries: &[(String, NodeId)], hints: &mut HashMap<NodeId, String>) {
    let type_name = entries
        .iter()
        .find(|(key, _)| key == TYPE_KEY)
        .and_then(|(_, id)| src.get(*id))
        .and_then(|node| match &node.value {
            Value::Str(name) => Some(name.as_str()),
            _ => None,
        });
    let value_id = entries
        .iter()
        .find(|(key, _)| key == VALUE_KEY)
        .map(|(_, id)| *id);
    if let (Some(name), Some(value_id)) = (type_name, value_id) {
        hints.insert(value_id, format!("ES3:{name}"));
    }
}

fn copy_node(
    node: &Node,
    id: NodeId,
    hints: &HashMap<NodeId, String>,
    read_only: &HashSet<NodeId>,
    mode: Mode,
) -> Node {
    let mut copy = node.clone();
    match mode {
        Mode::Annotate => {
            if let Some(hint) = hints.get(&id) {
                if is_single(hint)
                    && let Value::Float { v, width } = &mut copy.value
                {
                    *v = f64::from(*v as f32);
                    *width = FloatWidth::F32;
                }
                copy.type_hint = Some(TypeHint::new(hint.as_str()));
            }
            if read_only.contains(&id) || is_bare_key_object(&copy) {
                copy.flags.insert(NodeFlags::READ_ONLY);
            }
        }
        Mode::Restore => {
            let es3_hint = copy
                .type_hint
                .as_ref()
                .is_some_and(|hint| hint.as_str().starts_with("ES3:"));
            if es3_hint && matches!(copy.value, Value::Float { .. }) {
                copy.type_hint = Some(TypeHint::new("JSON:float"));
            }
        }
    }
    copy
}

/// True for an Easy Save 3 number type stored as a 32-bit float.
fn is_single(hint: &str) -> bool {
    matches!(
        hint.strip_prefix("ES3:"),
        Some("float" | "System.Single" | "Single")
    )
}

/// True for an object whose keys were written bare (a dictionary whose key type is not a string).
/// Its keys cannot be renamed or added to without knowing the dictionary's key type, so the
/// container is read-only; its values stay editable.
fn is_bare_key_object(node: &Node) -> bool {
    matches!(node.value, Value::Object(_))
        && node
            .lexeme
            .as_deref()
            .is_some_and(|lexeme| lexeme.starts_with(json::BARE_KEY_MARKER))
}

fn missing() -> Error {
    Error::new(
        ES3_UNSUPPORTED,
        "internal error: a node of the document is missing; reload the file",
    )
}

fn unsupported() -> Error {
    Error::new(
        ES3_UNSUPPORTED,
        "Easy Save 3 text can only hold objects, arrays and plain values; the document has a map or \
         reference. Remove it or use another format",
    )
}
