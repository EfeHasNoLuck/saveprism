//! Reading the outer `SavedGameData` JSON and inlining each record's data string.

use saveprism_core::{
    Codec, DecodeCtx, Decoded, Error, NodeFlags, NodeId, ParamValue, Params, Result, SaveDoc,
    TypeHint, Value,
};

use super::{JSON_HINT, LUA_HINT, PC_PARSE, PC_RECORD, RECORD_KEYS, lua};
use crate::json::{JsonCodec, compact_text};
use crate::text_common::copy_graph;

/// A nested record: how it is encoded and the original text it replaced.
pub(crate) struct Nested {
    pub(crate) kind: NestedKind,
    pub(crate) original: String,
    pub(crate) canonical: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NestedKind {
    Json,
    Lua,
    /// A record's data is a plain string with nothing to inline; it is written back untouched.
    Raw,
}

impl NestedKind {
    pub(crate) fn name(self) -> &'static str {
        match self {
            NestedKind::Json => "json",
            NestedKind::Lua => "lua",
            NestedKind::Raw => "raw",
        }
    }

    pub(crate) fn hint(self) -> &'static str {
        match self {
            NestedKind::Json => JSON_HINT,
            NestedKind::Lua => LUA_HINT,
            NestedKind::Raw => "",
        }
    }

    pub(crate) fn from_name(name: &str) -> Option<Self> {
        match name {
            "json" => Some(NestedKind::Json),
            "lua" => Some(NestedKind::Lua),
            "raw" => Some(NestedKind::Raw),
            _ => None,
        }
    }
}

pub(crate) fn decode(input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<Decoded> {
    let decoded = JsonCodec
        .decode(input, ctx)
        .map_err(|error| error.context("reading a Pixel Crushers save"))?;
    let mut doc = decoded.doc;
    let nested = inline_records(&mut doc, ctx)?;
    let mut params = decoded.params;
    params.set("nested", nested_param(&nested));
    Ok(Decoded { doc, params })
}

/// Takes the records list out of the outer document, or fails when it is missing.
pub(crate) fn records_array(doc: &SaveDoc) -> Result<NodeId> {
    let root = doc.root();
    let entries = match doc.get(root).map(|node| &node.value) {
        Some(Value::Object(entries)) => entries,
        _ => {
            return Err(Error::new(
                PC_PARSE,
                "the root of a Pixel Crushers save must be a SavedGameData object",
            ));
        }
    };
    for key in RECORD_KEYS {
        if let Some((_, child)) = entries.iter().find(|(name, _)| name == key)
            && matches!(
                doc.get(*child).map(|node| &node.value),
                Some(Value::Array(_))
            )
        {
            return Ok(*child);
        }
    }
    Err(Error::new(
        PC_PARSE,
        "the save has no saveRecords list (is this a Pixel Crushers SavedGameData file?)",
    ))
}

/// Finds the `data` node of every record, in order.
pub(crate) fn record_data_nodes(doc: &SaveDoc) -> Result<Vec<NodeId>> {
    let array = records_array(doc)?;
    let items = match doc.get(array).map(|node| &node.value) {
        Some(Value::Array(items)) => items,
        _ => return Err(Error::new(PC_PARSE, "saveRecords is not a list")),
    };
    let mut nodes = Vec::with_capacity(items.len());
    for item in items {
        let data = match doc.get(*item).map(|node| &node.value) {
            Some(Value::Object(entries)) => entries
                .iter()
                .find(|(key, _)| key == "data")
                .map(|(_, child)| *child),
            _ => None,
        };
        let id = data.ok_or_else(|| {
            Error::new(
                PC_RECORD,
                "a save record has no `data` string; the SavedGameData layout is not recognised",
            )
        })?;
        nodes.push(id);
    }
    Ok(nodes)
}

fn inline_records(doc: &mut SaveDoc, ctx: &mut DecodeCtx<'_>) -> Result<Vec<Nested>> {
    let array = records_array(doc)?;
    let items = match doc.get(array).map(|node| &node.value) {
        Some(Value::Array(items)) => items.clone(),
        _ => return Err(Error::new(PC_PARSE, "saveRecords is not a list")),
    };
    let mut nested = Vec::new();
    for item in items {
        let Some(entry_index) = record_entry_index(doc, item) else {
            return Err(Error::new(PC_RECORD, "a save record has no `data` string"));
        };
        let data_id = match doc.get(item).map(|node| &node.value) {
            Some(Value::Object(entries)) => entries
                .get(entry_index)
                .map(|(_, child)| *child)
                .ok_or_else(|| Error::new(PC_RECORD, "a save record lost its data entry"))?,
            _ => return Err(Error::new(PC_RECORD, "a save record is not an object")),
        };
        let text = match doc.get(data_id).map(|node| &node.value) {
            Some(Value::Str(text)) => text.clone(),
            Some(_) => {
                return Err(Error::new(
                    PC_RECORD,
                    "a save record's `data` is not a string",
                ));
            }
            None => {
                return Err(Error::new(
                    PC_RECORD,
                    "a save record refers to a missing node",
                ));
            }
        };
        let trimmed = text.trim_start();
        let inlined = if trimmed.starts_with('{') || trimmed.starts_with('[') {
            match JsonCodec.decode(text.as_bytes(), ctx) {
                Ok(parsed) => {
                    let is_container = parsed.doc.get(parsed.doc.root()).is_some_and(|node| {
                        matches!(node.value, Value::Object(_) | Value::Array(_))
                    });
                    if is_container {
                        let canonical = compact_text(&parsed.doc, parsed.doc.root())?;
                        let canonical = String::from_utf8(canonical).map_err(|_| {
                            Error::new(PC_RECORD, "a nested JSON record is not UTF-8")
                        })?;
                        Some((parsed.doc, NestedKind::Json, Some(canonical)))
                    } else {
                        None
                    }
                }
                Err(_) => None,
            }
        } else if lua::looks_like_lua(&text) {
            let parsed = lua::parse(&text, ctx)?;
            Some((parsed, NestedKind::Lua, None))
        } else {
            None
        };
        match inlined {
            Some((parsed, kind, canonical)) => {
                let copied = copy_graph(&parsed, doc, parsed.root(), PC_RECORD)?;
                mark_root(doc, copied, kind)?;
                replace_data(doc, item, entry_index, copied)?;
                nested.push(Nested {
                    kind,
                    original: text,
                    canonical,
                });
            }
            None => nested.push(Nested {
                kind: NestedKind::Raw,
                original: text,
                canonical: None,
            }),
        }
    }
    Ok(nested)
}

fn record_entry_index(doc: &SaveDoc, item: NodeId) -> Option<usize> {
    match doc.get(item).map(|node| &node.value) {
        Some(Value::Object(entries)) => entries.iter().position(|(key, _)| key == "data"),
        _ => None,
    }
}

fn mark_root(doc: &mut SaveDoc, id: NodeId, kind: NestedKind) -> Result<()> {
    doc.set_type_hint(id, Some(TypeHint::new(kind.hint())))?;
    if let Some(node) = doc.get(id) {
        let flags = if node.flags.contains(NodeFlags::READ_ONLY) {
            NodeFlags::empty()
        } else {
            node.flags
        };
        doc.set_flags(id, flags)?;
    }
    Ok(())
}

fn replace_data(
    doc: &mut SaveDoc,
    item: NodeId,
    entry_index: usize,
    new_child: NodeId,
) -> Result<()> {
    let entries = match doc.get(item).map(|node| &node.value) {
        Some(Value::Object(entries)) => {
            let mut entries = entries.clone();
            if let Some(entry) = entries.get_mut(entry_index) {
                entry.1 = new_child;
            }
            entries
        }
        _ => return Err(Error::new(PC_RECORD, "a save record is not an object")),
    };
    doc.set_value(item, Value::Object(entries))
}

fn nested_param(nested: &[Nested]) -> ParamValue {
    let mut list = Vec::with_capacity(nested.len());
    for item in nested {
        let mut table = Params::new();
        table.set("kind", item.kind.name());
        table.set("original", item.original.clone());
        if let Some(canonical) = &item.canonical {
            table.set("canonical", canonical.clone());
        }
        list.push(ParamValue::Table(table));
    }
    ParamValue::List(list)
}
