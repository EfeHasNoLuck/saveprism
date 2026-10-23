//! SugarCube 2 (Twine) saves as a story view.
//!
//! A SugarCube save is `LZString.compressToBase64(JSON.stringify(save))` with `save` =
//! `{title or desc, date, id, state, version?, metadata?}`. The play history is `state.delta`:
//! `delta[0]` is a full moment `{title, variables, ...}` and every later `delta[i]` is the difference
//! of moment `i` to moment `i - 1`, as SugarCube's `Diff.diff` writes it (`null` when nothing
//! changed). Inside a difference a key holds:
//! - `0`: the key was deleted;
//! - `[2, value]`: the key is set to `value` (a changed scalar, a new key, a value whose type
//!   changed, a replaced `Map`, `Set`, `Date` or class);
//! - `[3, time]`: the same for a `Date` (old saves);
//! - `[1, from, to]` under a `~` key: items were removed from an array;
//! - a plain object: the difference of an object or array that was partly changed.
//!
//! `state.index` is the moment the game resumes at. Every story variable is shown with the value
//! the game loads there and is edited at the node that holds that value (its "last writer"), so an
//! edit never changes the structure of the file. Values that were partly changed during play, and
//! values kept as revive tuples (`["(revive:map)", [...]]`), are read-only here; the JSON view can
//! still change them. Nothing in the save is executed.

use std::collections::{HashMap, HashSet};

use saveprism_core::{NodeId, Value};
use serde::Serialize;

use super::game::{At, Ctx, MAX_VALUE_ROWS, NoteKind, RowNote, UNLIMITED, ValueRow};
use super::{Session, child_count, truncate};

/// Longest save title or passage name sent to the UI, in characters.
const MAX_TITLE_CHARS: usize = 200;
/// Longest `Date` text sent to the UI, in characters.
const MAX_DATE_CHARS: usize = 40;
/// `Diff` operation: delete the key.
const OP_DELETE: i32 = 0;
/// `Diff` operation: set the key to the copied value.
const OP_COPY: i32 = 2;

/// The SugarCube story view.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SugarCubeView {
    /// The save's title (`title`, or `desc` in SugarCube 2.37).
    pub title: Option<String>,
    /// The passage the game resumes at.
    pub passage: Option<String>,
    /// When the game was saved, in milliseconds since 1970.
    pub date: Option<i64>,
    /// Number of moments in the history.
    pub moments: usize,
    /// The moment the game resumes at (0-based).
    pub index: usize,
    /// The story variables with their values at that moment.
    pub variables: Vec<ValueRow>,
}

/// The node that holds the current value of a variable.
enum Writer {
    /// The whole value: in the first moment, or copied by a later one.
    Value(At),
    /// The variable was deleted.
    Deleted,
    /// Only part of the value is here (a nested difference), or the shape is not one SugarCube
    /// writes (an unknown operation, a repeated key).
    Changed(At),
}

/// The parts of a SugarCube save the view reads.
struct Story {
    root: At,
    /// The moments of `state.delta`, `None` for a moment without changes (`null`).
    moments: Vec<Option<At>>,
    /// The moment the game resumes at.
    index: usize,
}

fn child(at: &At, id: NodeId, segment: impl Into<String>) -> At {
    let mut path = at.1.clone();
    path.push(segment.into());
    (id, path)
}

/// True when `value` is the number `n` (JavaScript does not tell `2` from `2.0`).
fn is_number(value: &Value, n: i32) -> bool {
    match value {
        Value::Int { v, .. } => *v == i128::from(n),
        Value::Float { v, .. } => *v == f64::from(n),
        _ => false,
    }
}

impl Ctx<'_> {
    fn value(&self, at: &At) -> Option<&Value> {
        self.doc.get(at.0).map(|node| &node.value)
    }

    /// The entry `key` of the object at `at`; `None` when it is missing, repeated (JavaScript keeps
    /// the last one, the paths here the first) or `at` is not an object.
    fn unique(&self, at: &At, key: &str) -> Option<At> {
        let Some(Value::Object(entries)) = self.value(at) else {
            return None;
        };
        let mut found = None;
        for (name, id) in entries {
            if name == key {
                if found.is_some() {
                    return None;
                }
                found = Some(*id);
            }
        }
        found.map(|id| child(at, id, key))
    }

    /// True when the object at `at` has an entry `key` (once or more).
    fn has(&self, at: &At, key: &str) -> bool {
        matches!(self.value(at), Some(Value::Object(entries)) if entries.iter().any(|(name, _)| name == key))
    }

    fn is_object(&self, at: &At) -> bool {
        matches!(self.value(at), Some(Value::Object(_)))
    }

    fn text(&self, at: &At) -> Option<String> {
        match self.value(at)? {
            Value::Str(text) => Some(truncate(text, MAX_TITLE_CHARS)),
            _ => None,
        }
    }

    /// The payload of a `[2, value]` copy operation.
    fn copied(&self, at: &At) -> Option<At> {
        let Some(Value::Array(items)) = self.value(at) else {
            return None;
        };
        let [op, payload] = items.as_slice() else {
            return None;
        };
        let op = self.doc.get(*op)?;
        is_number(&op.value, OP_COPY).then(|| child(at, *payload, "1"))
    }

    /// The moment structure, or `None` when the document is not a SugarCube save: a root object
    /// with `state.delta`, a non-empty array whose first moment has a `variables` object, and every
    /// moment up to the current one an object (or `null`) with at most one `variables` object.
    fn story(&self) -> Option<Story> {
        let root: At = (self.doc.root(), saveprism_core::Path::root());
        let state = self.unique(&root, "state")?;
        let delta = self.unique(&state, "delta")?;
        let Some(Value::Array(items)) = self.value(&delta) else {
            return None;
        };
        if items.is_empty() {
            return None;
        }
        let first = child(&delta, *items.first()?, "0");
        if !self.is_object(&self.unique(&first, "variables")?) {
            return None;
        }
        let last = items.len() - 1;
        // A missing, negative or too large index: the game cannot resume there; show the last.
        let index = self
            .unique(&state, "index")
            .and_then(|at| match self.value(&at) {
                Some(Value::Int { v, .. }) => usize::try_from(*v).ok(),
                _ => None,
            })
            .filter(|index| *index <= last)
            .unwrap_or(last);
        let mut moments = Vec::with_capacity(items.len());
        for (position, id) in items.iter().enumerate() {
            let at = child(&delta, *id, position.to_string());
            let moment = match self.value(&at)? {
                Value::Null if position > 0 => None,
                Value::Object(entries) => {
                    let count = entries.iter().filter(|(k, _)| k == "variables").count();
                    let variables = self.unique(&at, "variables");
                    if position <= index
                        && (count > 1 || variables.is_some_and(|v| !self.is_object(&v)))
                    {
                        return None;
                    }
                    Some(at)
                }
                _ if position > index => None,
                _ => return None,
            };
            moments.push(moment);
        }
        Some(Story {
            root,
            moments,
            index,
        })
    }

    /// The `variables` entries of a moment, in file order.
    fn variables(&self, moment: Option<&At>) -> Vec<(String, At)> {
        let Some(variables) = moment.and_then(|at| self.unique(at, "variables")) else {
            return Vec::new();
        };
        match self.value(&variables) {
            Some(Value::Object(entries)) => entries
                .iter()
                .map(|(name, id)| (name.clone(), child(&variables, *id, name.as_str())))
                .collect(),
            _ => Vec::new(),
        }
    }

    /// What a later moment's difference says about a variable.
    fn later_writer(&self, at: At) -> Writer {
        match self.value(&at) {
            Some(value) if is_number(value, OP_DELETE) => Writer::Deleted,
            _ => match self.copied(&at) {
                Some(payload) => Writer::Value(payload),
                None => Writer::Changed(at),
            },
        }
    }

    /// The passage of the current moment: the last title copied by a moment up to it, else the
    /// title of the first moment.
    fn passage(&self, story: &Story) -> Option<String> {
        for moment in story.moments.iter().take(story.index + 1).skip(1).rev() {
            let Some(at) = moment else {
                continue;
            };
            if !self.has(at, "title") {
                continue;
            }
            let title = self.unique(at, "title")?;
            return self.copied(&title).and_then(|payload| self.text(&payload));
        }
        let first = story.moments.first()?.as_ref()?;
        self.text(&self.unique(first, "title")?)
    }

    /// A revive tuple (`["(revive:map)", [...]]`) as a note.
    fn revive_note(&self, at: &At) -> Option<RowNote> {
        let Some(Value::Array(items)) = self.value(at) else {
            return None;
        };
        let [tag, payload] = items.as_slice() else {
            return None;
        };
        let Some(Value::Str(tag)) = self.doc.get(*tag).map(|node| &node.value) else {
            return None;
        };
        let name = tag.strip_prefix("(revive:")?.strip_suffix(')')?;
        let payload = self.doc.get(*payload).map(|node| &node.value);
        let count = match payload {
            Some(Value::Array(items)) => Some(items.len()),
            _ => None,
        };
        let note = |kind, count, text| RowNote { kind, count, text };
        Some(match (name, payload) {
            ("map", _) if count.is_some() => note(NoteKind::Map, count, None),
            ("set", _) if count.is_some() => note(NoteKind::Set, count, None),
            ("date", Some(Value::Str(text))) => {
                note(NoteKind::Date, None, Some(truncate(text, MAX_DATE_CHARS)))
            }
            _ => note(NoteKind::Code, None, None),
        })
    }

    /// The row of a variable whose whole current value is at `at`.
    fn value_row_at(&self, name: String, at: &At) -> ValueRow {
        let read_only = |kind| ValueRow {
            name: name.clone(),
            path: at.1.to_string(),
            field: None,
            children: 0,
            note: Some(RowNote {
                kind,
                count: None,
                text: None,
            }),
        };
        if let Some(note) = self.revive_note(at) {
            return ValueRow {
                note: Some(note),
                ..read_only(NoteKind::Code)
            };
        }
        match self.value(at) {
            Some(Value::Null) => read_only(NoteKind::Empty),
            Some(value @ (Value::Object(_) | Value::Array(_))) => {
                let children = child_count(value);
                if children == 0 {
                    return read_only(NoteKind::Empty);
                }
                ValueRow {
                    name,
                    path: at.1.to_string(),
                    field: None,
                    children,
                    note: None,
                }
            }
            _ => match self.field(at, UNLIMITED) {
                Some(field) => ValueRow {
                    name,
                    path: at.1.to_string(),
                    field: Some(field),
                    children: 0,
                    note: None,
                },
                None => read_only(NoteKind::Changed),
            },
        }
    }

    /// The story view, or `None` when the document is not a SugarCube save.
    pub(super) fn sugarcube_view(&self) -> Option<SugarCubeView> {
        let story = self.story()?;
        let index = story.index;

        // The last writer of every name, from the current moment back to the second one. A name
        // repeated inside one moment cannot be edited safely (JavaScript keeps the last entry).
        let mut writers: HashMap<String, (usize, Writer)> = HashMap::new();
        for position in (1..=index).rev() {
            for (name, at) in self.variables(story.moments.get(position).and_then(Option::as_ref)) {
                match writers.get_mut(&name) {
                    None => {
                        let writer = self.later_writer(at);
                        writers.insert(name, (position, writer));
                    }
                    Some((seen, writer)) if *seen == position => *writer = Writer::Changed(at),
                    Some(_) => {}
                }
            }
        }
        let first = self.variables(story.moments.first().and_then(Option::as_ref));
        let mut repeated = HashSet::new();
        let mut names = HashSet::new();
        for (name, _) in &first {
            if !names.insert(name.as_str()) {
                repeated.insert(name.clone());
            }
        }
        for (name, at) in &first {
            if !writers.contains_key(name) {
                let writer = if repeated.contains(name) {
                    Writer::Changed(at.clone())
                } else {
                    Writer::Value(at.clone())
                };
                writers.insert(name.clone(), (0, writer));
            }
        }

        // Names in the order they first appear: the first moment, then the later ones.
        let mut order: Vec<String> = Vec::new();
        let mut listed: HashSet<String> = HashSet::new();
        for position in 0..=index {
            let moment = story.moments.get(position).and_then(Option::as_ref);
            for (name, _) in self.variables(moment) {
                if listed.insert(name.clone()) {
                    order.push(name);
                }
            }
        }

        let mut variables = Vec::new();
        for name in order {
            if variables.len() >= MAX_VALUE_ROWS {
                break;
            }
            let Some((_, writer)) = writers.remove(&name) else {
                continue;
            };
            match writer {
                Writer::Deleted => {}
                Writer::Value(at) => variables.push(self.value_row_at(name, &at)),
                Writer::Changed(at) => variables.push(ValueRow {
                    name,
                    path: at.1.to_string(),
                    field: None,
                    children: 0,
                    note: Some(RowNote {
                        kind: NoteKind::Changed,
                        count: None,
                        text: None,
                    }),
                }),
            }
        }

        let title = self
            .unique(&story.root, "title")
            .or_else(|| self.unique(&story.root, "desc"))
            .and_then(|at| self.text(&at));
        let date = self
            .unique(&story.root, "date")
            .and_then(|at| match self.value(&at) {
                Some(Value::Int { v, .. }) => i64::try_from(*v).ok(),
                _ => None,
            });
        Some(SugarCubeView {
            title,
            passage: self.passage(&story),
            date,
            moments: story.moments.len(),
            index,
            variables,
        })
    }
}

impl Session {
    /// True when the open document is a SugarCube save (by its shape, not its file name).
    pub(super) fn is_sugarcube(&self) -> bool {
        self.game_ctx().story().is_some()
    }
}
