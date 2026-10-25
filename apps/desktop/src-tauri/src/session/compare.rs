//! Read-only comparison of the open document with another save.
//!
//! The second file is detected and decoded with the same codecs as an open, but nothing is
//! written and no edit is possible on it. Values are paired by their JSON Pointer path; an entry
//! only one side has is reported as "only here" or "only there". At most [`MAX_ROWS`] rows are
//! returned; [`CompareResult::total`] still counts every difference, so the UI can say how many
//! were left out. A visit budget keeps a pathological file from taking forever.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use saveprism_core::{Node, NodeId, Path, SaveDoc, Value};
use serde::Serialize;

use super::{ApiError, Session, app_codes, preview, value_text};
use saveprism_codecs::es3::ES3_PASSWORD_REQUIRED;

/// Most rows one comparison returns.
pub const MAX_ROWS: usize = 5_000;
/// Most node pairs (or one-sided nodes) one comparison walks.
const MAX_VISITS: usize = 500_000;

/// Which side of the comparison a row describes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CompareKind {
    /// Both files have the path and the values differ.
    Different,
    /// Only the open document has this path.
    OnlyHere,
    /// Only the other file has this path.
    OnlyThere,
}

/// One difference between the two files.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompareRow {
    /// JSON Pointer of the value.
    pub path: String,
    /// Value in the open document, or `None` when only the other file has it.
    pub here: Option<String>,
    /// Value in the other file, or `None` when only the open document has it.
    pub there: Option<String>,
    /// What the row says.
    pub kind: CompareKind,
}

/// The differences between the open document and another file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompareResult {
    /// The rows to show, at most [`MAX_ROWS`].
    pub rows: Vec<CompareRow>,
    /// Every difference found, including the rows left out by the cap.
    pub total: usize,
    /// True when the cap or the visit budget left differences out.
    pub truncated: bool,
    /// File name of the other save.
    pub other_file_name: String,
}

impl Session {
    /// Compares the open document (pending edits included) with the file at `path`. That file is
    /// detected and decoded like an opened save, but it is never written and needs no password: an
    /// encrypted file is refused with a clear error. Nothing in the open document changes.
    pub fn compare_with(&self, path: &str) -> Result<CompareResult, ApiError> {
        let other = Session::open_path(PathBuf::from(path), None, None, self.renpy_keys.clone())
            .map_err(|error| {
            if error.code == ES3_PASSWORD_REQUIRED.as_str() {
                ApiError::new(
                    app_codes::COMPARE_ENCRYPTED,
                    "the file to compare with is encrypted, so it cannot be read without its password",
                    "Compare with an unencrypted copy of the save instead.",
                )
            } else {
                error
            }
        })?;
        let (rows, total, truncated) = compare_docs(&self.doc, &other.doc);
        Ok(CompareResult {
            rows,
            total,
            truncated,
            other_file_name: other.info().file_name,
        })
    }
}

/// The text a comparison row shows for a value: a scalar as its text, a container as its short
/// preview ("[3]", "{2}"), bytes as "<n bytes>" and a reference as "-> #n".
fn node_text(node: &Node) -> String {
    match &node.value {
        Value::Null => "null".to_string(),
        Value::Array(_) | Value::Object(_) | Value::Map(_) | Value::Bytes(_) | Value::Ref(_) => {
            preview(node)
        }
        _ => value_text(node).unwrap_or_default(),
    }
}

/// True when the two scalar values mean the same thing. Integer and float widths and number
/// spellings are ignored (1 and 1.0 are different values, as the game sees them).
fn same_scalar(a: &Node, b: &Node) -> bool {
    match (&a.value, &b.value) {
        (Value::Null, Value::Null) => true,
        (Value::Bool(left), Value::Bool(right)) => left == right,
        (Value::Int { v: left, .. }, Value::Int { v: right, .. }) => left == right,
        (Value::Float { v: left, .. }, Value::Float { v: right, .. }) => {
            left.to_bits() == right.to_bits()
        }
        (Value::Str(left), Value::Str(right)) => left == right,
        (Value::Bytes(left), Value::Bytes(right)) => left == right,
        (Value::Ref(left), Value::Ref(right)) => left == right,
        _ => false,
    }
}

fn is_container(value: &Value) -> bool {
    matches!(value, Value::Array(_) | Value::Object(_) | Value::Map(_))
}

/// Children with the path segment that addresses each one.
fn children(doc: &SaveDoc, id: NodeId) -> Vec<(String, NodeId)> {
    match doc.get(id).map(|node| &node.value) {
        Some(Value::Array(items)) => items
            .iter()
            .enumerate()
            .map(|(index, child)| (index.to_string(), *child))
            .collect(),
        Some(Value::Object(entries)) => entries
            .iter()
            .map(|(key, child)| (key.clone(), *child))
            .collect(),
        Some(Value::Map(entries)) => entries
            .iter()
            .enumerate()
            .map(|(index, (_, child))| (index.to_string(), *child))
            .collect(),
        _ => Vec::new(),
    }
}

/// The first child of each key, so a repeated key compares like the path resolves it.
fn object_children(doc: &SaveDoc, id: NodeId) -> HashMap<String, NodeId> {
    let mut map = HashMap::new();
    if let Some(Value::Object(entries)) = doc.get(id).map(|node| &node.value) {
        for (key, child) in entries {
            map.entry(key.clone()).or_insert(*child);
        }
    }
    map
}

/// The visit budget and the collected rows of one comparison.
struct Walk {
    rows: Vec<CompareRow>,
    total: usize,
    visits: usize,
    truncated: bool,
    seen: HashSet<(usize, usize)>,
}

impl Walk {
    fn new() -> Self {
        Self {
            rows: Vec::new(),
            total: 0,
            visits: 0,
            truncated: false,
            seen: HashSet::new(),
        }
    }

    /// True while there is walk budget left.
    fn has_budget(&mut self) -> bool {
        self.visits += 1;
        if self.visits > MAX_VISITS {
            self.truncated = true;
            return false;
        }
        true
    }

    fn push(&mut self, path: &Path, here: Option<&Node>, there: Option<&Node>) {
        self.total += 1;
        if self.rows.len() >= MAX_ROWS {
            self.truncated = true;
            return;
        }
        let kind = match (here, there) {
            (Some(_), Some(_)) => CompareKind::Different,
            (Some(_), None) => CompareKind::OnlyHere,
            (None, Some(_)) => CompareKind::OnlyThere,
            (None, None) => return,
        };
        self.rows.push(CompareRow {
            path: path.to_string(),
            here: here.map(node_text),
            there: there.map(node_text),
            kind,
        });
    }
}

/// Walks both documents in path order and collects the differences.
fn compare_docs(here: &SaveDoc, there: &SaveDoc) -> (Vec<CompareRow>, usize, bool) {
    let mut walk = Walk::new();
    let mut stack = vec![(here.root(), there.root(), Path::root())];
    while let Some((left, right, path)) = stack.pop() {
        if !walk.has_budget() {
            break;
        }
        let (Some(left_node), Some(right_node)) = (here.get(left), there.get(right)) else {
            continue;
        };
        if !walk.seen.insert((left.index(), right.index())) {
            continue;
        }
        let left_container = is_container(&left_node.value);
        let right_container = is_container(&right_node.value);
        if !left_container && !right_container {
            if !same_scalar(left_node, right_node) {
                walk.push(&path, Some(left_node), Some(right_node));
            }
            continue;
        }
        if !left_container || !right_container {
            walk.push(&path, Some(left_node), Some(right_node));
            continue;
        }
        match (&left_node.value, &right_node.value) {
            (Value::Object(_), Value::Object(_)) => {
                let left_children = object_children(here, left);
                let right_children = object_children(there, right);
                for (key, child) in &left_children {
                    let mut child_path = path.clone();
                    child_path.push(key.clone());
                    if let Some(other) = right_children.get(key) {
                        stack.push((*child, *other, child_path));
                    } else if let Some(node) = here.get(*child) {
                        walk.push(&child_path, Some(node), None);
                    }
                }
                for (key, child) in &right_children {
                    if left_children.contains_key(key) {
                        continue;
                    }
                    let mut child_path = path.clone();
                    child_path.push(key.clone());
                    if let Some(node) = there.get(*child) {
                        walk.push(&child_path, None, Some(node));
                    }
                }
            }
            (Value::Array(_), Value::Array(_)) | (Value::Map(_), Value::Map(_)) => {
                let left_children = children(here, left);
                let right_children = children(there, right);
                let longest = left_children.len().max(right_children.len());
                for index in (0..longest).rev() {
                    let mut child_path = path.clone();
                    if let Some((segment, _)) = left_children.get(index) {
                        child_path.push(segment.clone());
                    } else if let Some((segment, _)) = right_children.get(index) {
                        child_path.push(segment.clone());
                    }
                    match (left_children.get(index), right_children.get(index)) {
                        (Some((_, left_child)), Some((_, right_child))) => {
                            stack.push((*left_child, *right_child, child_path));
                        }
                        (Some((_, left_child)), None) => {
                            if let Some(node) = here.get(*left_child) {
                                walk.push(&child_path, Some(node), None);
                            }
                        }
                        (None, Some((_, right_child))) => {
                            if let Some(node) = there.get(*right_child) {
                                walk.push(&child_path, None, Some(node));
                            }
                        }
                        (None, None) => {}
                    }
                }
            }
            _ => walk.push(&path, Some(left_node), Some(right_node)),
        }
    }
    (walk.rows, walk.total, walk.truncated)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn env() -> (tempfile::TempDir, PathBuf) {
        let root = tempfile::tempdir().unwrap();
        let work = root.path().join("work");
        fs::create_dir_all(&work).unwrap();
        (root, work)
    }

    fn write(path: &std::path::Path, text: &str) {
        fs::write(path, text).unwrap();
    }

    fn open(path: &std::path::Path) -> Session {
        Session::open(path.to_str().unwrap(), None, None).unwrap()
    }

    #[test]
    fn two_json_saves_report_changed_and_one_sided_values() {
        let (_root, work) = env();
        let first = work.join("first.json");
        let second = work.join("second.json");
        write(
            &first,
            r#"{"name":"Ann","level":7,"items":["potion"],"old":true}"#,
        );
        write(
            &second,
            r#"{"name":"Ann","level":8,"items":["potion","ether"],"new":false}"#,
        );
        let session = open(&first);
        let result = session.compare_with(second.to_str().unwrap()).unwrap();
        assert_eq!(result.other_file_name, "second.json");
        assert!(!result.truncated);
        let then = |path: &str| {
            result
                .rows
                .iter()
                .find(|row| row.path == path)
                .unwrap_or_else(|| panic!("no row for {path}"))
        };
        assert_eq!(then("/level").kind, CompareKind::Different);
        assert_eq!(then("/level").here.as_deref(), Some("7"));
        assert_eq!(then("/level").there.as_deref(), Some("8"));
        assert_eq!(then("/items/1").kind, CompareKind::OnlyThere);
        assert_eq!(then("/items/1").there.as_deref(), Some("ether"));
        assert_eq!(then("/old").kind, CompareKind::OnlyHere);
        assert_eq!(then("/new").kind, CompareKind::OnlyThere);
        assert!(
            result.rows.iter().all(|row| row.path != "/name"),
            "the same name must not be listed"
        );
    }

    #[test]
    fn an_rpgmaker_mv_pair_reports_the_edited_value() {
        let (_root, work) = env();
        let original = work.join("one.rpgsave");
        fs::copy(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../fixtures/synthetic/rpgmaker-mv/file1.rpgsave"),
            &original,
        )
        .unwrap();
        let edited = work.join("two.rpgsave");
        fs::copy(&original, &edited).unwrap();

        let mut second = open(&edited);
        let gold = second.game_view().unwrap();
        let gold = match gold {
            crate::session::GameView::Rpgmaker { view } => view.gold,
            _ => panic!("the MV fixture must open as an RPG Maker save"),
        };
        let old = gold.value.parse::<i64>().unwrap();
        second
            .edit_value(&gold.path, &(old + 1).to_string())
            .unwrap();
        second.save(edited.to_str().unwrap(), true, None).unwrap();

        let first = open(&original);
        let result = first.compare_with(edited.to_str().unwrap()).unwrap();
        let row = result
            .rows
            .iter()
            .find(|row| row.path == gold.path)
            .unwrap_or_else(|| panic!("no row for {}", gold.path));
        assert_eq!(row.kind, CompareKind::Different);
        assert_eq!(row.here.as_deref(), Some(old.to_string().as_str()));
        assert_eq!(row.there.as_deref(), Some((old + 1).to_string().as_str()));
    }

    #[test]
    fn the_row_cap_keeps_the_total_count() {
        let (_root, work) = env();
        let first = work.join("big-one.json");
        let second = work.join("big-two.json");
        let one: Vec<String> = (0..MAX_ROWS + 10).map(|n| format!("{n}")).collect();
        let two: Vec<String> = (0..MAX_ROWS + 10).map(|n| format!("{}", n + 1)).collect();
        write(&first, &format!("[{}]", one.join(",")));
        write(&second, &format!("[{}]", two.join(",")));
        let session = open(&first);
        let result = session.compare_with(second.to_str().unwrap()).unwrap();
        assert_eq!(result.rows.len(), MAX_ROWS);
        assert_eq!(result.total, MAX_ROWS + 10);
        assert!(result.truncated);
    }

    #[test]
    fn an_encrypted_file_is_refused_with_a_clear_message() {
        let (_root, work) = env();
        let first = work.join("plain.json");
        write(&first, r#"{"a":1}"#);
        let encrypted = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../fixtures/keyed/es3-aes/settings.es3");
        let session = open(&first);
        let error = session
            .compare_with(encrypted.to_str().unwrap())
            .unwrap_err();
        assert_eq!(error.code, app_codes::COMPARE_ENCRYPTED);
        assert!(error.message.contains("encrypted"));
    }
}
