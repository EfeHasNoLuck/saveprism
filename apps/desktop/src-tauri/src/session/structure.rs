//! Adding and removing entries where the format allows it, each as one undo step.
//!
//! - Files read by `format.json` (plain JSON, RPG Maker MV/MZ files that are not saves): a value may
//!   be added to any object and any object entry may be removed. List items are not added or
//!   removed (the numbers of the items after them would move). RPG Maker MV/MZ files (JsonEx, where
//!   objects may be referred to by `@c` number) give only plain values back, never an object.
//! - Easy Save 3: top-level keys only, each written as `{"__type": "int", "value": 5}`.
//! - SugarCube: story variables in the first moment (see `sugarcube.rs`).
//! - RPG Maker inventory (`add_inventory`): MV/MZ `party._items` (and weapons, armors) objects,
//!   XP/VX/VX Ace `@items` hashes (through the Marshal codec, which lets integer-keyed hashes grow).
//!   An MV/MZ inventory entry can be removed; an XP/VX/VX Ace entry only when it was added in this
//!   session and is the last one (a count of 0 is what the game itself keeps).
//! - Everything else (binary formats, read-only files) keeps its shape.
//!
//! An added entry is a pending change of kind "added" at its value's path; a removed entry is one of
//! kind "removed" at its own path. Undo and redo take out and put back the very same nodes.

use std::collections::HashSet;

use saveprism_codecs::{es3, json, marshal};
use saveprism_core::{
    Edit, Limits, NodeFlags, NodeId, OwnedValue, Path, Position, SaveDoc, TypeHint, Value,
    apply_edits, detach, entry_at,
};

use super::game::{At, Ctx, FieldKind, object_entries};
use super::history::{EntryStep, Mutation};
use super::sugarcube::{StoryVars, SugarCubeView};
use super::{
    ApiError, Change, ChangeKind, HistoryResult, Session, app_codes, bad_path, parse_input,
    preview, quote, read_only_document, value_text,
};

/// Longest key of a new value, in characters.
const MAX_KEY_CHARS: usize = 200;
/// Highest database id of a new inventory entry.
const MAX_ENTRY_ID: i128 = 99_999;
/// Highest count of a new inventory entry (the games' own limit).
const MAX_COUNT: i128 = 99;
/// The type entry of an Easy Save 3 top-level entry.
const ES3_TYPE: &str = "__type";
/// The value entry of an Easy Save 3 top-level entry.
const ES3_VALUE: &str = "value";

/// How the open document takes new entries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// Nothing changes its shape (binary formats, read-only files).
    Fixed,
    /// JSON objects; `rpg`: an RPG Maker MV/MZ file (only plain values come and go).
    Json { rpg: bool },
    /// Easy Save 3: top-level keys.
    Es3,
    /// SugarCube: story variables of the first moment.
    Story,
}

/// What may be added and removed in the open document.
pub(super) struct Shaper {
    kind: Kind,
    story: Option<StoryVars>,
}

/// What may be added and removed among the children of one container.
pub(super) struct ChildShape {
    shaper: Shaper,
    parent: Path,
    parent_id: NodeId,
    repeated: HashSet<String>,
}

/// Keys that appear more than once among `entries`.
pub(super) fn repeated_keys(entries: &[(String, NodeId)]) -> HashSet<&str> {
    let mut seen = HashSet::new();
    let mut repeated = HashSet::new();
    for (key, _) in entries {
        if !seen.insert(key.as_str()) {
            repeated.insert(key.as_str());
        }
    }
    repeated
}

fn read_only(doc: &SaveDoc, id: NodeId) -> bool {
    doc.get(id)
        .is_some_and(|node| node.flags.contains(NodeFlags::READ_ONLY))
}

fn is_scalar(doc: &SaveDoc, id: NodeId) -> bool {
    doc.get(id).is_some_and(|node| {
        matches!(
            node.value,
            Value::Bool(_) | Value::Int { .. } | Value::Float { .. } | Value::Str(_)
        )
    })
}

fn not_addable() -> ApiError {
    ApiError::new(
        app_codes::NOT_ADDABLE,
        "values cannot be added here",
        "Values can be added to objects of JSON and Easy Save 3 files, to SugarCube story variables and to RPG Maker inventories.",
    )
}

fn not_removable(hint: &str) -> ApiError {
    ApiError::new(
        app_codes::NOT_REMOVABLE,
        "this value cannot be removed",
        hint.to_string(),
    )
}

const REMOVE_HINT: &str = "Set the value to 0, empty or off instead.";

impl Shaper {
    /// True when the entry `key` (value `child`) of the container `parent_id` at `parent` can be
    /// removed; `repeated`: the key appears more than once there.
    pub(super) fn removable(
        &self,
        doc: &SaveDoc,
        parent: &Path,
        parent_id: NodeId,
        key: &str,
        child: NodeId,
        repeated: bool,
    ) -> bool {
        if repeated || object_entries(doc, parent_id).is_none() {
            return false;
        }
        if read_only(doc, parent_id) || read_only(doc, child) {
            return false;
        }
        match self.kind {
            Kind::Fixed => false,
            Kind::Json { rpg } => !rpg || is_scalar(doc, child),
            Kind::Es3 => parent.is_root(),
            Kind::Story => self
                .story
                .as_ref()
                .is_some_and(|story| parent.to_string() == story.first && story.first_only(key)),
        }
    }

    /// True when a value can be added to the node `id` at `path`.
    pub(super) fn addable(&self, doc: &SaveDoc, path: &Path, id: NodeId) -> bool {
        if object_entries(doc, id).is_none() || read_only(doc, id) {
            return false;
        }
        match self.kind {
            Kind::Fixed => false,
            Kind::Json { .. } => true,
            Kind::Es3 => path.is_root(),
            Kind::Story => self
                .story
                .as_ref()
                .is_some_and(|story| path.to_string() == story.first),
        }
    }
}

impl ChildShape {
    /// [`Shaper::removable`] for the child `key` of this container.
    pub(super) fn removable(&self, doc: &SaveDoc, key: &str, child: NodeId) -> bool {
        self.shaper.removable(
            doc,
            &self.parent,
            self.parent_id,
            key,
            child,
            self.repeated.contains(key),
        )
    }

    /// [`Shaper::addable`].
    pub(super) fn addable(&self, doc: &SaveDoc, path: &Path, id: NodeId) -> bool {
        self.shaper.addable(doc, path, id)
    }
}

/// Where an inventory entry goes.
enum Inventory {
    /// An MV/MZ `{"<id>": count}` object.
    Object(At),
    /// An XP/VX/VX Ace `{id => count}` hash.
    Hash(At),
}

impl Session {
    /// What may be added and removed in the open document.
    pub(super) fn shaper(&self, ctx: &Ctx<'_>) -> Shaper {
        let codec = self.recipe.steps.last().map(|step| step.kind.as_str());
        let story = if codec == Some("format.json") {
            ctx.story_vars()
        } else {
            None
        };
        let kind = match codec {
            _ if !self.editable => Kind::Fixed,
            Some("format.json") if story.is_some() => Kind::Story,
            Some("format.json") => Kind::Json {
                rpg: self.rpg_engine().is_some(),
            },
            Some("format.es3") => Kind::Es3,
            _ => Kind::Fixed,
        };
        Shaper { kind, story }
    }

    /// What may be added and removed among the children of the container `parent_id` at `parent`.
    pub(super) fn shape_rules(&self, parent: &Path, parent_id: NodeId) -> ChildShape {
        let repeated = object_entries(&self.doc, parent_id)
            .map(|entries| {
                repeated_keys(entries)
                    .into_iter()
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        ChildShape {
            shaper: self.shaper(&self.game_ctx()),
            parent: parent.clone(),
            parent_id,
            repeated,
        }
    }

    /// Marks the story variables that can be removed and where new ones go.
    pub(super) fn mark_story_rows(&self, ctx: &Ctx<'_>, view: &mut SugarCubeView) {
        let shaper = self.shaper(ctx);
        let Some(story) = shaper.story.as_ref().filter(|_| shaper.kind == Kind::Story) else {
            return;
        };
        let Ok(first) = Path::parse(&story.first) else {
            return;
        };
        let Some(first_id) = self.doc.resolve(&first) else {
            return;
        };
        view.add_parent = Some(story.first.clone());
        for row in &mut view.variables {
            let Ok(path) = Path::parse(&row.path) else {
                continue;
            };
            if path.parent().map(|parent| parent.to_string()).as_deref() != Some(&story.first) {
                continue;
            }
            let (Some(key), Some(child)) = (path.last(), self.doc.resolve(&path)) else {
                continue;
            };
            row.removable = shaper.removable(&self.doc, &first, first_id, key, child, false);
        }
    }

    /// Adds the value `key` (of kind `kind`, typed as `text`) to the object at `parent`: any object
    /// of a JSON file, the top of an Easy Save 3 file, or the story variables of a SugarCube save.
    /// The new value counts as an added change and is one undo step.
    pub fn add_value(
        &mut self,
        parent: &str,
        key: &str,
        kind: FieldKind,
        text: &str,
    ) -> Result<HistoryResult, ApiError> {
        if !self.editable {
            return Err(read_only_document());
        }
        let parent_path = Path::parse(parent)?;
        let parent_id = self
            .doc
            .resolve(&parent_path)
            .ok_or_else(|| bad_path(parent))?;
        let shaper = self.shaper(&self.game_ctx());
        if !shaper.addable(&self.doc, &parent_path, parent_id) {
            return Err(not_addable());
        }
        if key.trim().is_empty() {
            return Err(ApiError::new(
                app_codes::BAD_VALUE,
                "the new value has no name",
                "Type a name for the new value.",
            ));
        }
        if key.chars().count() > MAX_KEY_CHARS {
            return Err(ApiError::new(
                app_codes::BAD_VALUE,
                format!("the name is longer than {MAX_KEY_CHARS} characters"),
                "Type a shorter name.",
            ));
        }
        let value = parse_new(kind, text)?;
        let taken = object_entries(&self.doc, parent_id)
            .is_some_and(|entries| entries.iter().any(|(name, _)| name == key));
        let story_taken = shaper.kind == Kind::Story
            && shaper.story.as_ref().is_some_and(|story| !story.is_free(key));
        if taken || story_taken {
            return Err(ApiError::new(
                app_codes::ENTRY_EXISTS,
                format!("\"{}\" is there already", quote(key)),
                "Pick another name, or change the value that is there.",
            ));
        }
        let mut entry_path = parent_path.clone();
        entry_path.push(key);
        let (stored, value_path, hint) = if shaper.kind == Kind::Es3 {
            let type_name = es3::primitive_type(&value).ok_or_else(not_addable)?;
            let wrapped = OwnedValue::Object(vec![
                (ES3_TYPE.to_string(), OwnedValue::Str(type_name.to_string())),
                (ES3_VALUE.to_string(), value),
            ]);
            let mut value_path = entry_path.clone();
            value_path.push(ES3_VALUE);
            (wrapped, value_path, Some(es3::value_hint(type_name)))
        } else {
            let hint = json::new_value_hint(&value);
            (value, entry_path.clone(), hint)
        };
        let edit = Edit::Insert {
            parent: parent_path.clone(),
            at: Position::Key(key.to_string()),
            value: stored,
        };
        self.add_entry(&parent_path, entry_path, value_path, hint, |doc, limits| {
            apply_edits(doc, std::slice::from_ref(&edit), limits)
        })
    }

    /// Adds `count` (1 to 99) of the database entry `id` to the party's `list` ("items",
    /// "weapons" or "armors") of an RPG Maker save. One undo step.
    pub fn add_inventory(
        &mut self,
        list: &str,
        id: i64,
        count: &str,
    ) -> Result<HistoryResult, ApiError> {
        if !self.editable {
            return Err(read_only_document());
        }
        let id = i128::from(id);
        if !(1..=MAX_ENTRY_ID).contains(&id) {
            return Err(ApiError::new(
                app_codes::OUT_OF_RANGE,
                format!("{id} is not a database id (1 to 99,999)"),
                "Type an id from 1 to 99,999.",
            ));
        }
        let count = match count.trim().parse::<i128>() {
            Ok(count) if (1..=MAX_COUNT).contains(&count) => count,
            Ok(count) => {
                return Err(ApiError::new(
                    app_codes::OUT_OF_RANGE,
                    format!("{count} is outside the range of a count (1 to 99)"),
                    "Type a count from 1 to 99.",
                ));
            }
            Err(_) => {
                return Err(ApiError::new(
                    app_codes::BAD_VALUE,
                    format!("\"{}\" is not a whole number", quote(count)),
                    "Type a count from 1 to 99.",
                ));
            }
        };
        let target = self.inventory(list)?.ok_or_else(not_addable)?;
        let exists = || {
            ApiError::new(
                app_codes::ENTRY_EXISTS,
                format!("#{id} is in this list already"),
                "Change its count instead.",
            )
        };
        match target {
            Inventory::Object((map_id, map_path)) => {
                let entries = object_entries(&self.doc, map_id).ok_or_else(not_addable)?;
                let key = id.to_string();
                if entries.iter().any(|(name, _)| *name == key) {
                    return Err(exists());
                }
                let mut entry_path = map_path.clone();
                entry_path.push(key.as_str());
                let edit = Edit::Insert {
                    parent: map_path.clone(),
                    at: Position::Key(key),
                    value: OwnedValue::Int(count),
                };
                let hint = json::new_value_hint(&OwnedValue::Int(count));
                self.add_entry(&map_path, entry_path.clone(), entry_path, hint, |doc, limits| {
                    apply_edits(doc, std::slice::from_ref(&edit), limits)
                })
            }
            Inventory::Hash((map_id, map_path)) => {
                let Some(Value::Map(entries)) = self.doc.get(map_id).map(|node| &node.value) else {
                    return Err(not_addable());
                };
                let has = entries.iter().any(|(key, _)| {
                    matches!(self.doc.get(*key).map(|node| &node.value), Some(Value::Int { v, .. }) if *v == id)
                });
                if has {
                    return Err(exists());
                }
                let mut entry_path = map_path.clone();
                entry_path.push(entries.len().to_string());
                self.add_entry(
                    &map_path,
                    entry_path.clone(),
                    entry_path,
                    None,
                    |doc, limits| {
                        marshal::insert_int_entry(doc, &map_path, id, count, limits).map(|_| ())
                    },
                )
            }
        }
    }

    /// The party container of an inventory list, `None` when the save has none.
    fn inventory(&self, list: &str) -> Result<Option<Inventory>, ApiError> {
        let (json_key, ruby_key) = match list {
            "items" => ("_items", "@items"),
            "weapons" => ("_weapons", "@weapons"),
            "armors" => ("_armors", "@armors"),
            _ => {
                return Err(ApiError::new(
                    app_codes::BAD_VALUE,
                    format!("\"{}\" is not an inventory list", quote(list)),
                    "Add to items, weapons or armors.",
                ));
            }
        };
        let ctx = self.game_ctx();
        if self.rpg_engine().is_some() && !self.is_sugarcube() {
            let root: At = (self.doc.root(), Path::root());
            let found = ctx
                .entry(&root, "party")
                .and_then(|party| ctx.entry(&party, json_key))
                .filter(|at| !read_only(&self.doc, at.0));
            return Ok(found.map(Inventory::Object));
        }
        let found = self
            .rgss_engine(&ctx)
            .and_then(|engine| ctx.rgss_party(engine))
            .and_then(|party| ctx.entry(&party, ruby_key));
        Ok(found.map(Inventory::Hash))
    }

    /// Adds an entry with `insert` (atomic), gives its value `hint`, and records it as one undo
    /// step with an "added" change at `value_path`.
    fn add_entry(
        &mut self,
        parent: &Path,
        entry_path: Path,
        value_path: Path,
        hint: Option<TypeHint>,
        insert: impl FnOnce(&mut SaveDoc, &Limits) -> saveprism_core::Result<()>,
    ) -> Result<HistoryResult, ApiError> {
        let entry_text = entry_path.to_string();
        let pending_before = self.pending_under(&entry_text);
        insert(&mut self.doc, &self.limits)?;
        if let Some(hint) = hint
            && let Some(id) = self.doc.resolve(&value_path)
        {
            self.doc.set_type_hint(id, Some(hint))?;
        }
        let entry = entry_at(&self.doc, &entry_path)?;
        let new = self
            .doc
            .resolve(&value_path)
            .and_then(|id| self.doc.get(id))
            .and_then(value_text)
            .unwrap_or_default();
        let value_text_path = value_path.to_string();
        // An entry removed before and added again counts as added.
        let pending_after = vec![Change {
            path: value_text_path.clone(),
            old: String::new(),
            new,
            kind: ChangeKind::Added,
        }];
        self.replace_pending_under(&entry_text, pending_after.clone());
        self.history
            .record(vec![Mutation::Entry(Box::new(EntryStep {
                parent: parent.to_string(),
                path: entry_text,
                entry,
                added: true,
                pending_before,
                pending_after,
            }))]);
        Ok(self.structural_result(vec![value_text_path]))
    }

    /// Removes the value at `path`: an object entry of a JSON file, a top-level Easy Save 3 key (its
    /// row's path is the `value` inside it), a SugarCube story variable that lives in the first
    /// moment alone, an MV/MZ inventory entry, or an entry added in this session. One undo step.
    pub fn remove_value(&mut self, path: &str) -> Result<HistoryResult, ApiError> {
        if !self.editable {
            return Err(read_only_document());
        }
        let parsed = Path::parse(path)?;
        self.doc.resolve(&parsed).ok_or_else(|| bad_path(path))?;
        let shaper = self.shaper(&self.game_ctx());
        let entry_path = self.entry_to_remove(parsed, &shaper);
        let entry_text = entry_path.to_string();
        let parent = entry_path
            .parent()
            .ok_or_else(|| not_removable(REMOVE_HINT))?;
        let parent_id = self
            .doc
            .resolve(&parent)
            .ok_or_else(|| bad_path(path))?;
        let child = self
            .doc
            .resolve(&entry_path)
            .ok_or_else(|| bad_path(path))?;
        let pending_before = self.pending_under(&entry_text);
        let added = pending_before
            .iter()
            .any(|change| change.kind == ChangeKind::Added);
        if added {
            // A map gives back its last entry only, so no later entry is renumbered.
            if let Some(Value::Map(entries)) = self.doc.get(parent_id).map(|node| &node.value)
                && entries.last().map(|(_, value)| *value) != Some(child)
            {
                return Err(not_removable(
                    "Undo the entries added after it first, or set its count to 0.",
                ));
            }
        } else {
            let key = entry_path.last().unwrap_or_default();
            let repeated = object_entries(&self.doc, parent_id)
                .is_some_and(|entries| entries.iter().filter(|(name, _)| name == key).count() > 1);
            if !shaper.removable(&self.doc, &parent, parent_id, key, child, repeated) {
                return Err(not_removable(REMOVE_HINT));
            }
        }
        let old = self.removed_text(&pending_before, &entry_text, child);
        let entry = detach(&mut self.doc, &entry_path)?;
        let pending_after = if added {
            Vec::new()
        } else {
            vec![Change {
                path: entry_text.clone(),
                old,
                new: String::new(),
                kind: ChangeKind::Removed,
            }]
        };
        self.replace_pending_under(&entry_text, pending_after.clone());
        self.history
            .record(vec![Mutation::Entry(Box::new(EntryStep {
                parent: parent.to_string(),
                path: entry_text.clone(),
                entry,
                added: false,
                pending_before,
                pending_after,
            }))]);
        Ok(self.structural_result(vec![entry_text]))
    }

    /// The entry a remove takes out: an Easy Save 3 row's `value` belongs to its top-level key.
    fn entry_to_remove(&self, path: Path, shaper: &Shaper) -> Path {
        if shaper.kind != Kind::Es3 || path.last() != Some(ES3_VALUE) {
            return path;
        }
        let Some(wrapper) = path.parent() else {
            return path;
        };
        let top = wrapper.parent().is_some_and(|parent| parent.is_root());
        let typed = self
            .doc
            .resolve(&wrapper)
            .and_then(|id| object_entries(&self.doc, id))
            .is_some_and(|entries| entries.iter().any(|(key, _)| key == ES3_TYPE));
        if top && typed { wrapper } else { path }
    }

    /// What a removed entry was: the first value of an edited one, else its value text or size.
    fn removed_text(&self, pending: &[Change], entry: &str, child: NodeId) -> String {
        let inner = format!("{entry}/{ES3_VALUE}");
        if let Some(change) = pending
            .iter()
            .find(|change| change.kind == ChangeKind::Set && (change.path == entry || change.path == inner))
        {
            return change.old.clone();
        }
        // An Easy Save 3 key shows its value.
        let shown = object_entries(&self.doc, child)
            .filter(|entries| entries.iter().any(|(key, _)| key == ES3_TYPE))
            .and_then(|entries| entries.iter().find(|(key, _)| key == ES3_VALUE))
            .map_or(child, |(_, value)| *value);
        self.doc
            .get(shown)
            .map(|node| value_text(node).unwrap_or_else(|| preview(node)))
            .unwrap_or_default()
    }

    fn structural_result(&self, paths: Vec<String>) -> HistoryResult {
        let mut result = self.history_result(paths);
        result.structural = true;
        result
    }
}

/// Parses the text of a new value of kind `kind` (whole numbers within the 64-bit range).
fn parse_new(kind: FieldKind, text: &str) -> Result<OwnedValue, ApiError> {
    let template = match kind {
        FieldKind::Int => Value::Int {
            v: 0,
            width: saveprism_core::IntWidth::I64,
        },
        FieldKind::Float => Value::Float {
            v: 0.0,
            width: saveprism_core::FloatWidth::F64,
        },
        FieldKind::Bool => Value::Bool(false),
        FieldKind::String => Value::Str(String::new()),
    };
    parse_input(&template, text)
}
