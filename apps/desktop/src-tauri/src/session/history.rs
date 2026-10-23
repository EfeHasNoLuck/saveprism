//! Undo and redo of the edits of the open document.
//!
//! Every user action that changes values is one step: a single edit (this includes setting one value
//! back), "Undo all", or one Apply of the JSON view. A step holds `(path, before, after)` for each
//! value it changed, as typed values, so undoing needs no text parsing and cannot fail on a value
//! that was valid when it was typed. Undo and redo set the paths back through the same atomic edit
//! as any other change and keep the pending list in step; a new action empties the redo list.
//! The history is bounded (steps and values) and starts empty when a file is opened or saved.

use std::collections::VecDeque;

use saveprism_core::{Edit, OwnedValue, Path, Value, apply_edits};
use serde::Serialize;

use super::{ApiError, Change, Field, Session, bad_path, value_text};

/// Most steps the history keeps; older ones are dropped.
pub const MAX_STEPS: usize = 200;
/// Most values all steps together keep (a huge JSON Apply must not hold the memory of the file again).
pub const MAX_VALUES: usize = 100_000;
/// Most fields one undo or redo sends back; a bigger step makes the UI read everything again.
pub const MAX_FIELDS: usize = 500;

/// One value a step changed.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Mutation {
    /// Core path of the value.
    pub path: String,
    /// The value before the step.
    pub before: OwnedValue,
    /// The value after the step.
    pub after: OwnedValue,
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

/// The undo and redo lists of a session.
#[derive(Default)]
pub(super) struct History {
    /// Oldest first; the last step is the next one to undo.
    undo: VecDeque<Vec<Mutation>>,
    /// The last step is the next one to redo.
    redo: Vec<Vec<Mutation>>,
    /// Number of values in `undo`.
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
        if step.len() > MAX_VALUES {
            self.undo.clear();
            self.values = 0;
            return;
        }
        self.push_undo(step);
        while self.undo.len() > MAX_STEPS || self.values > MAX_VALUES {
            let Some(oldest) = self.undo.pop_front() else {
                break;
            };
            self.values = self.values.saturating_sub(oldest.len());
        }
    }

    fn push_undo(&mut self, step: Vec<Mutation>) {
        self.values = self.values.saturating_add(step.len());
        self.undo.push_back(step);
    }

    fn pop_undo(&mut self) -> Option<Vec<Mutation>> {
        let step = self.undo.pop_back()?;
        self.values = self.values.saturating_sub(step.len());
        Some(step)
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

/// What `undo` and `redo` return.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryResult {
    /// The values the step changed, as they are now (empty for "nothing to do" and when `reload`).
    pub fields: Vec<Field>,
    /// The step changed too many values to list: read the document again.
    pub reload: bool,
    /// Number of pending changes now.
    pub pending: usize,
    /// Whether another undo or redo is possible now.
    #[serde(flatten)]
    pub state: HistoryState,
}

impl Session {
    /// Whether there is something to undo or redo.
    pub fn history_state(&self) -> HistoryState {
        self.history.state()
    }

    /// Takes back the last action (a single edit, "Undo all" or one JSON Apply). With nothing to
    /// undo it changes nothing.
    pub fn undo(&mut self) -> Result<HistoryResult, ApiError> {
        let Some(step) = self.history.pop_undo() else {
            return Ok(self.history_result(Vec::new()));
        };
        match self.apply_step(&step, true) {
            Ok(paths) => {
                self.history.redo.push(step);
                Ok(self.history_result(paths))
            }
            Err(error) => {
                // The edit is atomic, so the document is as it was: the step stays undoable.
                self.history.push_undo(step);
                Err(error)
            }
        }
    }

    /// Does again the action that was taken back last. With nothing to redo it changes nothing.
    pub fn redo(&mut self) -> Result<HistoryResult, ApiError> {
        let Some(step) = self.history.redo.pop() else {
            return Ok(self.history_result(Vec::new()));
        };
        match self.apply_step(&step, false) {
            Ok(paths) => {
                self.history.push_undo(step);
                Ok(self.history_result(paths))
            }
            Err(error) => {
                self.history.redo.push(step);
                Err(error)
            }
        }
    }

    /// Sets every value of `step` to its `before` (undo) or `after` (redo) value in one atomic edit
    /// and updates the pending list. Returns the paths that were set.
    fn apply_step(&mut self, step: &[Mutation], undo: bool) -> Result<Vec<String>, ApiError> {
        let mut edits = Vec::with_capacity(step.len());
        let mut touched = Vec::with_capacity(step.len());
        for mutation in step {
            let path = Path::parse(&mutation.path)?;
            let id = self
                .doc
                .resolve(&path)
                .ok_or_else(|| bad_path(&mutation.path))?;
            let old = self.doc.get(id).and_then(value_text).unwrap_or_default();
            touched.push((id, mutation.path.clone(), old));
            let value = if undo {
                &mutation.before
            } else {
                &mutation.after
            };
            edits.push(Edit::Set {
                path,
                value: value.clone(),
            });
        }
        apply_edits(&mut self.doc, &edits, &self.limits)?;
        let changes: Vec<Change> = touched
            .into_iter()
            .map(|(id, path, old)| Change {
                path,
                old,
                new: self.doc.get(id).and_then(value_text).unwrap_or_default(),
            })
            .collect();
        let paths = changes.iter().map(|change| change.path.clone()).collect();
        self.record_changes(changes);
        Ok(paths)
    }

    fn history_result(&self, paths: Vec<String>) -> HistoryResult {
        let reload = paths.len() > MAX_FIELDS;
        let fields = if reload {
            Vec::new()
        } else {
            self.fields_info(&paths)
        };
        HistoryResult {
            fields,
            reload,
            pending: self.pending.len(),
            state: self.history.state(),
        }
    }
}
