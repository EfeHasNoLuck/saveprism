//! Read-only parts of a decoded GVAS document.
//!
//! The file details (`header`, `schemas`, `extra` and `root.save_game_type`) describe the file,
//! not the game: a change there makes a save the game cannot load. The key of a map entry
//! (`{"key": ..., "value": ...}` in `uesave`'s JSON) names the entry; changing it can repeat a
//! key. Both are marked [`NodeFlags::READ_ONLY`], so no edit reaches them. Encoding ignores the
//! flags.

use saveprism_core::{NodeFlags, NodeId, Result, SaveDoc, Value};

/// Top-level entries that describe the file.
const DETAILS: [&str; 3] = ["header", "schemas", "extra"];

fn entry(doc: &SaveDoc, id: NodeId, key: &str) -> Option<NodeId> {
    match &doc.get(id)?.value {
        Value::Object(entries) => entries
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, child)| *child),
        _ => None,
    }
}

/// Marks `id` and everything below it read-only.
fn lock_tree(doc: &mut SaveDoc, id: NodeId) -> Result<()> {
    let mut stack = vec![id];
    let mut budget = doc.len();
    while let Some(next) = stack.pop() {
        if budget == 0 {
            break;
        }
        budget -= 1;
        let Some(node) = doc.get(next) else {
            continue;
        };
        let mut flags = node.flags;
        flags.insert(NodeFlags::READ_ONLY);
        doc.set_flags(next, flags)?;
        stack.extend(doc.children(next));
    }
    Ok(())
}

/// The key nodes of a map (`[{"key": k, "value": v}, ...]`), or `None` for anything else.
fn map_keys(doc: &SaveDoc, id: NodeId) -> Option<Vec<NodeId>> {
    let Value::Array(items) = &doc.get(id)?.value else {
        return None;
    };
    if items.is_empty() {
        return None;
    }
    items
        .iter()
        .map(|item| match &doc.get(*item)?.value {
            Value::Object(entries) => match entries.as_slice() {
                [(key, key_id), (value, _)] if key == "key" && value == "value" => Some(*key_id),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

/// Marks the file details and the map keys of a decoded GVAS document read-only.
pub(crate) fn lock(doc: &mut SaveDoc) -> Result<()> {
    let root = doc.root();
    for key in DETAILS {
        if let Some(id) = entry(doc, root, key) {
            lock_tree(doc, id)?;
        }
    }
    let Some(save_root) = entry(doc, root, "root") else {
        return Ok(());
    };
    if let Some(id) = entry(doc, save_root, "save_game_type") {
        lock_tree(doc, id)?;
    }
    let Some(properties) = entry(doc, save_root, "properties") else {
        return Ok(());
    };
    let mut stack = vec![properties];
    let mut budget = doc.len();
    while let Some(next) = stack.pop() {
        if budget == 0 {
            break;
        }
        budget -= 1;
        if let Some(keys) = map_keys(doc, next) {
            for key in keys {
                lock_tree(doc, key)?;
            }
        }
        stack.extend(doc.children(next));
    }
    Ok(())
}
