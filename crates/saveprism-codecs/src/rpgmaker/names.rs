//! Name dictionary read from the game's `data/*.json` files.
//!
//! `System.json` gives switch and variable names (by index), `Actors.json` and `Items.json`
//! give names by `id`. Read only; a missing file gives an empty table.

use std::collections::BTreeMap;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use saveprism_core::limits::len_u64;
use saveprism_core::{Codec, DecodeCtx, Error, Limits, NodeId, Result, SaveDoc, Value, codes};

use super::RPG_DATA_INVALID;
use crate::json::JsonCodec;

/// Names of switches, variables, actors and items, keyed by their number.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NameDictionary {
    switches: BTreeMap<u32, String>,
    variables: BTreeMap<u32, String>,
    actors: BTreeMap<u32, String>,
    items: BTreeMap<u32, String>,
}

impl NameDictionary {
    /// Reads `System.json`, `Actors.json` and `Items.json` from the game's `data` folder.
    /// A file that does not exist is skipped; a file that is too large or not valid JSON is an error.
    pub fn load(data_dir: &Path, limits: &Limits) -> Result<Self> {
        let mut dict = Self::default();
        if let Some(doc) = read_json(&data_dir.join("System.json"), limits)? {
            dict.switches = indexed_names(&doc, "switches");
            dict.variables = indexed_names(&doc, "variables");
        }
        if let Some(doc) = read_json(&data_dir.join("Actors.json"), limits)? {
            dict.actors = id_names(&doc);
        }
        if let Some(doc) = read_json(&data_dir.join("Items.json"), limits)? {
            dict.items = id_names(&doc);
        }
        Ok(dict)
    }

    /// Name of switch `id`.
    pub fn switch(&self, id: u32) -> Option<&str> {
        self.switches.get(&id).map(String::as_str)
    }

    /// Name of variable `id`.
    pub fn variable(&self, id: u32) -> Option<&str> {
        self.variables.get(&id).map(String::as_str)
    }

    /// Name of actor `id`.
    pub fn actor(&self, id: u32) -> Option<&str> {
        self.actors.get(&id).map(String::as_str)
    }

    /// Name of item `id`.
    pub fn item(&self, id: u32) -> Option<&str> {
        self.items.get(&id).map(String::as_str)
    }

    /// True when no name was found at all.
    pub fn is_empty(&self) -> bool {
        self.switches.is_empty()
            && self.variables.is_empty()
            && self.actors.is_empty()
            && self.items.is_empty()
    }
}

/// Reads and parses one data file. `None` when it does not exist.
fn read_json(path: &Path, limits: &Limits) -> Result<Option<SaveDoc>> {
    let io_error = |source: std::io::Error| {
        Error::new(
            codes::IO_READ,
            format!(
                "cannot read {}; check that the game's data folder is readable",
                path.display()
            ),
        )
        .with_source(source)
    };
    let size = match fs::metadata(path) {
        Ok(meta) => meta.len(),
        Err(source) if source.kind() == ErrorKind::NotFound => return Ok(None),
        Err(source) => return Err(io_error(source)),
    };
    limits.check_input(size)?;
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(source) if source.kind() == ErrorKind::NotFound => return Ok(None),
        Err(source) => return Err(io_error(source)),
    };
    limits.check_input(len_u64(bytes.len()))?;
    let mut ctx = DecodeCtx::new(limits);
    let decoded = JsonCodec.decode(&bytes, &mut ctx).map_err(|source| {
        Error::new(
            RPG_DATA_INVALID,
            format!(
                "{} is not valid JSON; the names of this file are skipped until it is repaired",
                path.display()
            ),
        )
        .with_source(source)
    })?;
    Ok(Some(decoded.doc))
}

fn object_entry(doc: &SaveDoc, id: NodeId, key: &str) -> Option<NodeId> {
    match &doc.get(id)?.value {
        Value::Object(entries) => entries
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, child)| *child),
        _ => None,
    }
}

fn array_items(doc: &SaveDoc, id: NodeId) -> &[NodeId] {
    match doc.get(id).map(|node| &node.value) {
        Some(Value::Array(items)) => items,
        _ => &[],
    }
}

fn text_of(doc: &SaveDoc, id: NodeId) -> Option<&str> {
    match &doc.get(id)?.value {
        Value::Str(text) if !text.is_empty() => Some(text.as_str()),
        _ => None,
    }
}

/// Names from the array under `key` of the root object, by array index.
fn indexed_names(doc: &SaveDoc, key: &str) -> BTreeMap<u32, String> {
    let mut out = BTreeMap::new();
    if let Some(list) = object_entry(doc, doc.root(), key) {
        for (index, item) in array_items(doc, list).iter().enumerate() {
            if let (Some(name), Ok(index)) = (text_of(doc, *item), u32::try_from(index)) {
                out.insert(index, name.to_string());
            }
        }
    }
    out
}

/// Names from a root array of `{"id": n, "name": "..."}` objects (`null` entries are skipped).
fn id_names(doc: &SaveDoc) -> BTreeMap<u32, String> {
    let mut out = BTreeMap::new();
    for item in array_items(doc, doc.root()) {
        let id = object_entry(doc, *item, "id")
            .and_then(|node| doc.get(node))
            .and_then(|node| match node.value {
                Value::Int { v, .. } => u32::try_from(v).ok(),
                _ => None,
            });
        let name = object_entry(doc, *item, "name").and_then(|node| text_of(doc, node));
        if let (Some(id), Some(name)) = (id, name) {
            out.insert(id, name.to_string());
        }
    }
    out
}
