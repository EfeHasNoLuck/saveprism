//! Undo and redo of the edits of the open document.
//!
//! Every user action that changes values is one step: a single edit (this includes setting one value
//! back), "Undo all", one Apply of the JSON view, or adding or removing one entry. A value step holds
//! `(path, before, after)` for each value it changed, as typed values, so undoing needs no text
//! parsing and cannot fail on a value that was valid when it was typed. An entry step holds the
//! entry's own nodes ([`Detached`]): undo and redo take the very same nodes out and put them back, so
//! every detail of the entry (type hints, widths, text) comes back. "Undo all" after entries were
//! added or removed swaps the whole document. Undo and redo keep the pending list in step; a new
//! action empties the redo list. The history is bounded (steps and values) and starts empty when a
//! file is opened or saved.

use std::collections::VecDeque;

use saveprism_core::{
    Detached, Edit, OwnedValue, Path, SaveDoc, Value, apply_edits, detach, reattach,
};
use serde::Serialize;

use super::{ApiError, Change, Field, Session, bad_path, value_text};

/// Most steps the history keeps; older ones are dropped.
pub const MAX_STEPS: usize = 200;
/// Most values all steps together keep (a huge JSON Apply must not hold the memory of the file again).
pub const MAX_VALUES: usize = 100_000;
/// Most fields one undo or redo sends back; a bigger step makes the UI read everything again.
pub const MAX_FIELDS: usize = 500;

/// An entry that a step added or removed.
#[derive(Debug)]
pub(super) struct EntryStep {
    /// Core path of the container.
    pub parent: String,
    /// Core path of the entry.
    pub path: String,
    /// The entry's nodes: the same nodes go out and come back.
    pub entry: Detached,
    /// True: the step added the entry; false: it removed it.
    pub added: bool,
    /// The pending changes at and below `path` before the step.
    pub pending_before: Vec<Change>,
    /// The pending changes at and below `path` after the step.
    pub pending_after: Vec<Change>,
}

/// The other side of an "Undo all" that threw added or removed entries away.
#[derive(Debug)]
pub(super) struct DocStep {
    /// The document on the other side of the step (swapped on undo and redo).
    pub doc: SaveDoc,
    /// The pending changes on the other side of the step.
    pub pending: Vec<Change>,
}

/// One change a step made.
#[derive(Debug)]
pub(super) enum Mutation {
    /// A scalar value changed.
    Set {
        /// Core path of the value.
        path: String,
        /// The value before the step.
        before: OwnedValue,
        /// The value after the step.
        after: OwnedValue,
    },
    /// An entry was added or removed (always alone in its step).
    Entry(Box<EntryStep>),
    /// The whole document changed (always alone in its step).
    Document(Box<DocStep>),
}

impl Mutation {
    /// How much the history counts this mutation against [`MAX_VALUES`].
    fn weight(&self) -> usize {
        match self {
            Mutation::Set { .. } => 1,
            Mutation::Entry(entry) => 1 + entry.pending_before.len() + entry.pending_after.len(),
            Mutation::Document(step) => step.doc.len().max(1) + step.pending.len(),
        }
    }

    fn structural(&self) -> bool {
        !matches!(self, Mutation::Set { .. })
    }
}

fn weight(step: &[Mutation]) -> usize {
    step.iter().map(Mutation::weight).sum()
}

/// The typed value of a scalar node, `None` for containers, bytes, null and references.
pub(super) fn scalar_owned(value: &Value) -> Option<OwnedValue> {
    match value {
        Value::Bool(v) => Some(OwnedValue::Bool(*v)),
        Value::Int { v, .. } => Some(OwnedValue::Int(*v)),
        Value::Float { v, .. } => Some(OwnedValue::Float(*v)),
        Value::Str(text) => Some(OwnedValue::Str(text.clone())),
        _ => None,
    }
}

/// True when `candidate` is `path` or a path below it.
pub(super) fn is_under(candidate: &str, path: &str) -> bool {
    candidate == path
        || candidate
            .strip_prefix(path)
            .is_some_and(|rest| rest.starts_with('/'))
}

/// The undo and redo lists of a session.
#[derive(Default)]
pub(super) struct History {
    /// Oldest first; the last step is the next one to undo.
    undo: VecDeque<Vec<Mutation>>,
    /// The last step is the next one to redo.
    redo: Vec<Vec<Mutation>>,
    /// Weight of the steps in `undo`.
    values: usize,
}

impl History {
    /// Records a new action: the redo list is emptied and the oldest steps go when over the bounds.
    /// A step that alone is over the value bound cannot be undone, and older steps cannot be undone
    /// past it either, so the whole history ends.
    pub(super) fn record(&mut self, step: Vec<Mutation>) {
        if step.is_empty() {
            return;
        }
        self.redo.clear();
        if weight(&step) > MAX_VALUES {
            self.undo.clear();
            self.values = 0;
            return;
        }
        self.push_undo(step);
        while self.undo.len() > MAX_STEPS || self.values > MAX_VALUES {
            let Some(oldest) = self.undo.pop_front() else {
                break;
            };
            self.values = self.values.saturating_sub(weight(&oldest));
        }
    }

    fn push_undo(&mut self, step: Vec<Mutation>) {
        self.values = self.values.saturating_add(weight(&step));
        self.undo.push_back(step);
    }

    fn pop_undo(&mut self) -> Option<Vec<Mutation>> {
        let step = self.undo.pop_back()?;
        self.values = self.values.saturating_sub(weight(&step));
        Some(step)
    }

    /// True when a step that can be undone added or removed entries or swapped the document: its
    /// undo needs the nodes of the document as it is now, so the document must not be read again.
    pub(super) fn has_structural(&self) -> bool {
        self.undo.iter().flatten().any(Mutation::structural)
    }

    fn state(&self) -> HistoryState {
        HistoryState {
            can_undo: !self.undo.is_empty(),
            can_redo: !self.redo.is_empty(),
        }
    }
}

/// What the UI needs to enable the Undo and Redo buttons.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryState {
    /// There is a step to undo.
    pub can_undo: bool,
    /// There is a step to redo.
    pub can_redo: bool,
}

/// What `undo`, `redo` and the other history actions return.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryResult {
    /// The values the step changed, as they are now (empty for "nothing to do" and when `reload`).
    pub fields: Vec<Field>,
    /// The step changed too many values to list: read the document again.
    pub reload: bool,
    /// The step added or removed entries: read the view again (lists, groups and counts).
    pub structural: bool,
    /// Number of pending changes now.
    pub pending: usize,
    /// Whether another undo or redo is possible now.
    #[serde(flatten)]
    pub state: HistoryState,
}

/// What applying a step did.
struct Applied {
    paths: Vec<String>,
    structural: bool,
}

impl Session {
    /// Whether there is something to undo or redo.
    pub fn history_state(&self) -> HistoryState {
        self.history.state()
    }

    /// Takes back the last action (a single edit, "Undo all", one JSON Apply, an added or removed
    /// entry). With nothing to undo it changes nothing.
    pub fn undo(&mut self) -> Result<HistoryResult, ApiError> {
        let Some(mut step) = self.history.pop_undo() else {
            return Ok(self.history_result(Vec::new()));
        };
        match self.apply_step(&mut step, true) {
            Ok(applied) => {
                self.history.redo.push(step);
                Ok(self.applied_result(applied))
            }
            Err(error) => {
                // Every way of applying a step is atomic, so the document is as it was: the step
                // stays undoable.
                self.history.push_undo(step);
                Err(error)
            }
        }
    }

    /// Does again the action that was taken back last. With nothing to redo it changes nothing.
    pub fn redo(&mut self) -> Result<HistoryResult, ApiError> {
        let Some(mut step) = self.history.redo.pop() else {
            return Ok(self.history_result(Vec::new()));
        };
        match self.apply_step(&mut step, false) {
            Ok(applied) => {
                self.history.push_undo(step);
                Ok(self.applied_result(applied))
            }
            Err(error) => {
                self.history.redo.push(step);
                Err(error)
            }
        }
    }

    /// Applies a step backwards (undo) or forwards (redo).
    fn apply_step(&mut self, step: &mut [Mutation], undo: bool) -> Result<Applied, ApiError> {
        match step {
            [Mutation::Entry(entry)] => self.apply_entry(entry, undo),
            [Mutation::Document(other)] => {
                // The other side becomes this side and the other way round.
                std::mem::swap(&mut self.doc, &mut other.doc);
                std::mem::swap(&mut self.pending, &mut other.pending);
                Ok(Applied {
                    paths: Vec::new(),
                    structural: true,
                })
            }
            _ => self.apply_values(step, undo).map(|paths| Applied {
                paths,
                structural: false,
            }),
        }
    }

    /// Takes the entry of `step` out or puts it back, and sets the pending changes at and below it
    /// to what they were on that side of the step.
    fn apply_entry(&mut self, step: &EntryStep, undo: bool) -> Result<Applied, ApiError> {
        let parent = Path::parse(&step.parent)?;
        // Redo of an add and undo of a remove put the entry in; the other two take it out.
        if step.added != undo {
            reattach(&mut self.doc, &parent, &step.entry)?;
        } else {
            let taken = detach(&mut self.doc, &Path::parse(&step.path)?)?;
            if taken != step.entry {
                // Not the entry of this step: put it back and stop.
                reattach(&mut self.doc, &parent, &taken)?;
                return Err(ApiError::internal());
            }
        }
        let pending = if undo {
            &step.pending_before
        } else {
            &step.pending_after
        };
        self.replace_pending_under(&step.path, pending.clone());
        let mut paths = vec![step.path.clone()];
        paths.extend(pending.iter().map(|change| change.path.clone()));
        Ok(Applied {
            paths,
            structural: true,
        })
    }

    /// Replaces the pending changes at and below `path` with `with`.
    pub(super) fn replace_pending_under(&mut self, path: &str, with: Vec<Change>) {
        self.pending.retain(|change| !is_under(&change.path, path));
        self.pending.extend(with);
    }

    /// The pending changes at and below `path`.
    pub(super) fn pending_under(&self, path: &str) -> Vec<Change> {
        self.pending
            .iter()
            .filter(|change| is_under(&change.path, path))
            .cloned()
            .collect()
    }

    /// Sets every value of `step` to its `before` (undo) or `after` (redo) value in one atomic edit
    /// and updates the pending list. Returns the paths that were set.
    fn apply_values(&mut self, step: &[Mutation], undo: bool) -> Result<Vec<String>, ApiError> {
        let mut edits = Vec::with_capacity(step.len());
        let mut touched = Vec::with_capacity(step.len());
        for mutation in step {
            let Mutation::Set {
                path: text,
                before,
                after,
            } = mutation
            else {
                return Err(ApiError::internal());
            };
            let path = Path::parse(text)?;
            let id = self.doc.resolve(&path).ok_or_else(|| bad_path(text))?;
            let old = self.doc.get(id).and_then(value_text).unwrap_or_default();
            touched.push((id, text.clone(), old));
            let value = if undo { before } else { after };
            edits.push(Edit::Set {
                path,
                value: value.clone(),
            });
        }
        apply_edits(&mut self.doc, &edits, &self.limits)?;
        let changes: Vec<Change> = touched
            .into_iter()
            .map(|(id, path, old)| {
                let new = self.doc.get(id).and_then(value_text).unwrap_or_default();
                Change::set(path, old, new)
            })
            .collect();
        let paths = changes.iter().map(|change| change.path.clone()).collect();
        self.record_changes(changes);
        Ok(paths)
    }

    fn applied_result(&self, applied: Applied) -> HistoryResult {
        let mut result = self.history_result(applied.paths);
        result.structural = applied.structural;
        result
    }

    pub(super) fn history_result(&self, paths: Vec<String>) -> HistoryResult {
        let reload = paths.len() > MAX_FIELDS;
        let fields = if reload {
            Vec::new()
        } else {
            self.fields_info(&paths)
        };
        HistoryResult {
            fields,
            reload,
            structural: false,
            pending: self.pending.len(),
            state: self.history.state(),
        }
    }
}
