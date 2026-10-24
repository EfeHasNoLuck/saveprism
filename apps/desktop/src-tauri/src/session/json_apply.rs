//! Applies the edited text of the JSON view (see `export`) to the open document.
//!
//! The parsed text is walked together with the document exactly the way `export` built the view:
//! the same names for repeated keys, the same `{"key", "value"}` pairs for maps, the same
//! placeholders, depth cut and visit budget. Only scalar values may differ, and only within their
//! own type. Every changed value goes through the checks of `edit_value` first; when anything is
//! wrong nothing is applied and the error names the JSON path. Otherwise all changes become
//! ordinary pending edits in one atomic step, so undo, the change list and Save work unchanged.
//! The text is only parsed into a tree; nothing in it is executed.

use std::collections::{HashMap, HashSet};
use std::fmt::Display;

use saveprism_core::{Edit, Node, NodeFlags, NodeId, OwnedValue, Path, Value, apply_edits};
use serde_json::Value as Json;

use super::export::{
    KeyNames, MAX_DEPTH, MAX_EXPORT_BYTES, TOO_DEEP, bytes_text, ref_text, scalar_float,
    scalar_int, too_big,
};
use super::game::{Bounds, check_bounds_in};
use super::history::{Mutation, scalar_owned};
use super::{ApiError, Change, Session, app_codes, parse_input, value_text};

/// The fix for any change of the structure.
const SHAPE_HINT: &str =
    "Change values only. Keep every field, list item and placeholder as it is.";

/// How the walk reached a node.
#[derive(Clone, Copy)]
struct Mode {
    /// The core path of the node resolves to it (no repeated key on the way), so it can be edited.
    addressable: bool,
    /// The node is (inside) the key of a map entry, which never changes.
    map_key: bool,
}

/// One value that changes.
struct Planned {
    id: NodeId,
    path: Path,
    value: OwnedValue,
}

/// The parallel walk over the document and the parsed text.
struct Walker<'a> {
    session: &'a Session,
    /// Visits left; the same budget as the view, so shared subtrees cannot blow up.
    budget: usize,
    /// Segments of the JSON view path, for messages.
    view: Vec<String>,
    /// Segments of the core path of the current node.
    core: Vec<String>,
    /// RPG Maker limits by path, built at the first changed number.
    bounds: Option<HashMap<String, Bounds>>,
    planned: Vec<Planned>,
}

fn path_of(segments: &[String]) -> Path {
    let mut path = Path::root();
    for segment in segments {
        path.push(segment.as_str());
    }
    path
}

/// The new value text when `json` changes the scalar `value`, `None` when it is the same.
/// `Err` names the kind of value the text has to stay.
fn changed_text(value: &Value, json: &Json) -> Result<Option<String>, &'static str> {
    match value {
        Value::Bool(current) => match json {
            Json::Bool(new) => Ok((new != current).then(|| new.to_string())),
            _ => Err("true or false"),
        },
        Value::Int { v, .. } => {
            let text = match json {
                // Digits past 64 bits are read as a float; written out again they reach the range
                // check ("out of range") instead of "not a whole number".
                Json::Number(number) => match number.as_f64() {
                    Some(big) if number.is_f64() && big.fract() == 0.0 && big.abs() >= 1e18 => {
                        format!("{big:.0}")
                    }
                    _ => number.to_string(),
                },
                // A whole number past 64 bits is shown as text.
                Json::String(text) if scalar_int(*v).is_string() => text.clone(),
                _ => return Err("a whole number"),
            };
            let same = text.trim().parse::<i128>().is_ok_and(|new| new == *v);
            Ok((!same).then_some(text))
        }
        Value::Float { v, .. } => match json {
            Json::Number(number) => {
                let same = number.as_f64().is_some_and(|new| new == *v);
                Ok((!same).then(|| number.to_string()))
            }
            // A float that is not finite is shown as text ("NaN"); only a number can replace it.
            Json::String(text) if matches!(scalar_float(*v), Json::String(shown) if shown == *text) => {
                Ok(None)
            }
            _ => Err("a number"),
        },
        Value::Str(current) => match json {
            Json::String(new) => Ok((new != current).then(|| new.clone())),
            _ => Err("text in quotes"),
        },
        _ => Ok(None),
    }
}

impl Walker<'_> {
    /// The JSON view path of the current node (`/` for the top level).
    fn pointer(&self) -> String {
        if self.view.is_empty() {
            "/".to_string()
        } else {
            path_of(&self.view).to_string()
        }
    }

    fn refuse(&self, code: &str, reason: impl Display, hint: impl Into<String>) -> ApiError {
        ApiError::new(code, format!("{}: {reason}", self.pointer()), hint)
    }

    fn shape(&self, reason: impl Display) -> ApiError {
        self.refuse(app_codes::JSON_SHAPE, reason, SHAPE_HINT)
    }

    /// An error of the shared value checks, with the JSON path in front.
    fn at_pointer(&self, error: ApiError) -> ApiError {
        self.refuse(&error.code, error.message, error.hint)
    }

    fn placeholder(&self, json: &Json, shown: &str) -> Result<(), ApiError> {
        match json {
            Json::String(text) if text == shown => Ok(()),
            _ => Err(self.shape("this placeholder cannot be edited")),
        }
    }

    /// Mirrors `export::convert`: same depth cut, budget and child order.
    fn walk(&mut self, id: NodeId, json: &Json, depth: usize, mode: Mode) -> Result<(), ApiError> {
        let next = depth.saturating_add(1);
        if depth > MAX_DEPTH {
            return self.placeholder(json, TOO_DEEP);
        }
        if self.budget == 0 {
            return Err(too_big("too many values"));
        }
        self.budget = self.budget.saturating_sub(1);
        let session = self.session;
        let Some(node) = session.doc.get(id) else {
            return match json {
                Json::Null => Ok(()),
                _ => Err(self.shape("this placeholder cannot be edited")),
            };
        };
        match &node.value {
            Value::Null => match json {
                Json::Null => Ok(()),
                _ => Err(self.shape("an empty (null) value cannot be edited")),
            },
            Value::Bytes(bytes) => self.placeholder(json, &bytes_text(bytes.len())),
            Value::Ref(target) => self.placeholder(json, &ref_text(*target)),
            Value::Bool(_) | Value::Int { .. } | Value::Float { .. } | Value::Str(_) => {
                self.scalar(id, node, json, mode)
            }
            Value::Array(items) => {
                let Json::Array(list) = json else {
                    return Err(self.shape("a list cannot become another kind of value"));
                };
                if list.len() != items.len() {
                    return Err(self.shape(format!(
                        "adding or removing list items is not supported (the list has {} items)",
                        items.len()
                    )));
                }
                for (index, (child, item)) in items.iter().zip(list).enumerate() {
                    self.view.push(index.to_string());
                    self.core.push(index.to_string());
                    self.walk(*child, item, next, mode)?;
                    self.core.pop();
                    self.view.pop();
                }
                Ok(())
            }
            Value::Object(entries) => {
                let Json::Object(fields) = json else {
                    return Err(self.shape("a group of fields cannot become another kind of value"));
                };
                let mut names = KeyNames::default();
                let shown: Vec<String> = entries.iter().map(|(key, _)| names.name(key)).collect();
                // The set of fields first, so a missing or added field is reported before values.
                if let Some(missing) = shown
                    .iter()
                    .find(|name| !fields.contains_key(name.as_str()))
                {
                    self.view.push(missing.clone());
                    return Err(self.shape(
                        "this field is missing; adding or removing fields is not supported",
                    ));
                }
                if fields.len() != shown.len() {
                    let known: HashSet<&str> = shown.iter().map(String::as_str).collect();
                    if let Some(extra) = fields.keys().find(|key| !known.contains(key.as_str())) {
                        self.view.push(extra.clone());
                        return Err(self.shape("adding or removing fields is not supported"));
                    }
                }
                // A repeated key resolves to its first entry, so later entries are not addressable.
                let mut seen: HashSet<&str> = HashSet::new();
                for ((key, child), name) in entries.iter().zip(&shown) {
                    let first = seen.insert(key.as_str());
                    let Some(item) = fields.get(name.as_str()) else {
                        continue;
                    };
                    self.view.push(name.clone());
                    self.core.push(key.clone());
                    let child_mode = Mode {
                        addressable: mode.addressable && first,
                        ..mode
                    };
                    self.walk(*child, item, next, child_mode)?;
                    self.core.pop();
                    self.view.pop();
                }
                Ok(())
            }
            Value::Map(entries) => {
                let Json::Array(list) = json else {
                    return Err(self.shape("a map cannot become another kind of value"));
                };
                if list.len() != entries.len() {
                    return Err(self.shape(format!(
                        "adding or removing map entries is not supported (the map has {} entries)",
                        entries.len()
                    )));
                }
                for (index, ((key, child), pair)) in entries.iter().zip(list).enumerate() {
                    self.view.push(index.to_string());
                    let parts = match pair {
                        Json::Object(fields) if fields.len() == 2 => {
                            fields.get("key").zip(fields.get("value"))
                        }
                        _ => None,
                    };
                    let Some((key_json, value_json)) = parts else {
                        return Err(self.shape(
                            "a map entry must keep exactly its \"key\" and \"value\" fields",
                        ));
                    };
                    self.view.push("key".to_string());
                    let key_mode = Mode {
                        addressable: false,
                        map_key: true,
                    };
                    self.walk(*key, key_json, next, key_mode)?;
                    self.view.pop();
                    // On a map, the core path segment `<index>` is the value of the entry.
                    self.view.push("value".to_string());
                    self.core.push(index.to_string());
                    self.walk(*child, value_json, next, mode)?;
                    self.core.pop();
                    self.view.pop();
                    self.view.pop();
                }
                Ok(())
            }
        }
    }

    /// A boolean, number or text: plans the change after the checks of `edit_value`.
    fn scalar(&mut self, id: NodeId, node: &Node, json: &Json, mode: Mode) -> Result<(), ApiError> {
        let text = match changed_text(&node.value, json) {
            Ok(Some(text)) => text,
            Ok(None) => return Ok(()),
            Err(kind) => {
                return Err(self.refuse(
                    app_codes::BAD_VALUE,
                    format!("this value has to stay {kind}"),
                    "Change only the value; its type cannot change.",
                ));
            }
        };
        if mode.map_key {
            return Err(self.shape("map keys cannot be edited"));
        }
        if !mode.addressable {
            return Err(self.refuse(
                app_codes::NOT_EDITABLE_NODE,
                "this value sits under a repeated key and cannot be edited",
                "Only values under the first of repeated keys can be changed.",
            ));
        }
        if node.flags.contains(NodeFlags::READ_ONLY) {
            return Err(self.refuse(
                app_codes::NOT_EDITABLE_NODE,
                "this value is read-only",
                "Leave this value as it is.",
            ));
        }
        let value = parse_input(&node.value, &text).map_err(|error| self.at_pointer(error))?;
        let path = path_of(&self.core);
        let session = self.session;
        let bounds = self.bounds.get_or_insert_with(|| session.rpg_bounds());
        let checked = check_bounds_in(bounds, &path, node, &value);
        checked.map_err(|error| self.at_pointer(error))?;
        self.planned.push(Planned { id, path, value });
        Ok(())
    }
}

fn syntax_error(error: &serde_json::Error) -> ApiError {
    let text = error.to_string();
    if text.starts_with("recursion limit exceeded") {
        return ApiError::new(
            app_codes::JSON_TEXT,
            "the JSON text is nested deeper than 128 levels",
            "This file is nested too deeply to edit as JSON. Edit its values in the lists instead.",
        );
    }
    ApiError::new(
        app_codes::JSON_TEXT,
        format!("the JSON text is not valid: {text}"),
        format!(
            "Fix the text at line {}, column {}, then apply again.",
            error.line(),
            error.column()
        ),
    )
}

impl Session {
    /// Applies the edited JSON view `text` (see [`Session::export_json`]). Only scalar values may
    /// change, each within its own type and limits; fields, list items, map keys and placeholders
    /// stay as they are. Everything is checked first: on any problem nothing is applied and the
    /// error names the JSON path. Returns the number of values that changed (0 for no change).
    pub fn apply_json(&mut self, text: &str) -> Result<usize, ApiError> {
        if !self.editable {
            return Err(self.read_only_error());
        }
        if text.len() > MAX_EXPORT_BYTES {
            return Err(too_big("the edited text is longer than the limit"));
        }
        // serde_json stops at 128 levels of nesting, so a hostile text cannot exhaust the stack.
        let json: Json = serde_json::from_str(text).map_err(|error| syntax_error(&error))?;

        let mut walker = Walker {
            session: self,
            budget: self.doc.len(),
            view: Vec::new(),
            core: Vec::new(),
            bounds: None,
            planned: Vec::new(),
        };
        let root_mode = Mode {
            addressable: true,
            map_key: false,
        };
        walker.walk(self.doc.root(), &json, 0, root_mode)?;
        let planned = walker.planned;
        if planned.is_empty() {
            return Ok(0);
        }

        let mut touched = Vec::with_capacity(planned.len());
        let mut edits = Vec::with_capacity(planned.len());
        for item in planned {
            let node = self.doc.get(item.id);
            let old = node.and_then(value_text).unwrap_or_default();
            let before = node.and_then(|node| scalar_owned(&node.value));
            touched.push((item.id, item.path.to_string(), old, before));
            edits.push(Edit::Set {
                path: item.path,
                value: item.value,
            });
        }
        // One atomic step: on an error the document is unchanged.
        apply_edits(&mut self.doc, &edits, &self.limits)?;
        let mut changes = Vec::with_capacity(touched.len());
        let mut step = Vec::with_capacity(touched.len());
        for (id, path, old, before) in touched {
            let node = self.doc.get(id);
            let new = node.and_then(value_text).unwrap_or_default();
            let after = node.and_then(|node| scalar_owned(&node.value));
            if let (Some(before), Some(after)) = (before, after) {
                step.push(Mutation {
                    path: path.clone(),
                    before,
                    after,
                });
            }
            changes.push(Change { path, old, new });
        }
        let count = changes.len();
        self.record_changes(changes);
        // The whole Apply is one undo step.
        self.history.record(step);
        Ok(count)
    }
}
