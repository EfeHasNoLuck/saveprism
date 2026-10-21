//! Edits: path-addressed changes to a [`SaveDoc`], applied atomically.
//!
//! Edits address nodes by [`Path`], never by `NodeId`, so AI patches and undo/redo stay
//! independent of the session-specific arena numbering.
//!
//! On a `Map`, `"<i>"` addresses the value of entry `i` and `"<i>.key"` its key node; changing a
//! key means `Set` on `"<i>.key"`.

use crate::doc::{FloatWidth, IntWidth, Node, NodeFlags, NodeId, SaveDoc, Value};
use crate::error::{Error, Result, codes};
use crate::limits::{Limits, len_u64};
use crate::path::{Path, parse_index};

/// Largest integer magnitude (2^53) that a 64-bit float stores exactly; `Set` refuses larger
/// integers for float nodes instead of rounding them silently.
const MAX_EXACT_FLOAT_INT: u128 = 1 << 53;

/// An owned value tree, used by edits, AI patches and tests.
#[derive(Clone, Debug, PartialEq)]
pub enum OwnedValue {
    /// No value.
    Null,
    /// Boolean.
    Bool(bool),
    /// Integer; the width of the target node decides the accepted range.
    Int(i128),
    /// Floating point number.
    Float(f64),
    /// Text.
    Str(String),
    /// Raw bytes.
    Bytes(Vec<u8>),
    /// List of values.
    Array(Vec<OwnedValue>),
    /// Text-keyed values in order.
    Object(Vec<(String, OwnedValue)>),
}

/// Where an `Insert` puts the new child.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Position {
    /// Before the element at this index (arrays); the index may equal the length.
    Index(usize),
    /// Under this key (objects).
    Key(String),
    /// After the last element (arrays).
    End,
}

/// One change to a document.
#[derive(Clone, Debug, PartialEq)]
pub enum Edit {
    /// Replaces the value at `path`; scalars keep the integer/float width of the existing node.
    /// A map key is changed by setting `"<map>/<i>.key"`.
    Set {
        /// Target node.
        path: Path,
        /// New value.
        value: OwnedValue,
    },
    /// Adds a child to the array or object at `parent`.
    Insert {
        /// Container node.
        parent: Path,
        /// Where to add the child.
        at: Position,
        /// The new child.
        value: OwnedValue,
    },
    /// Removes the node at `path` from its container.
    Remove {
        /// Node to remove.
        path: Path,
    },
    /// Renames the key of an object entry (map keys are changed with `Set` on `"<i>.key"`).
    RenameKey {
        /// Entry whose key changes.
        path: Path,
        /// The new key.
        new_key: String,
    },
}

/// Builds a fresh document from an owned value tree. The tree may be nested at most
/// `limits.max_depth` levels and the document gets `limits.max_nodes` as its node limit.
pub fn build_doc(value: &OwnedValue, limits: &Limits) -> Result<SaveDoc> {
    let mut doc = SaveDoc::with_node_limit(limits.max_nodes);
    let root = build_node(&mut doc, value, 0, limits.max_depth, false)?;
    doc.set_root(root);
    Ok(doc)
}

/// Applies all edits in order, atomically: they are applied to a copy, and the document
/// is replaced only when every edit succeeded. On error the document is unchanged.
///
/// `limits.max_depth` bounds the nesting of every value an edit supplies, and the document may
/// not grow beyond `limits.max_nodes` nodes (nor beyond its own node limit).
pub fn apply_edits(doc: &mut SaveDoc, edits: &[Edit], limits: &Limits) -> Result<()> {
    let mut work = doc.clone();
    let total = edits.len();
    for (index, edit) in edits.iter().enumerate() {
        apply_one(&mut work, edit, limits.max_depth).map_err(|error| {
            error.context(format!("while applying edit {}/{}", index + 1, total))
        })?;
    }
    if len_u64(work.len()) > limits.max_nodes {
        return Err(Error::new(
            codes::CORE_LIMIT_NODES,
            format!(
                "the edited document would have {} nodes, the limit is {}",
                work.len(),
                limits.max_nodes
            ),
        ));
    }
    *doc = work;
    Ok(())
}

fn apply_one(doc: &mut SaveDoc, edit: &Edit, max_depth: usize) -> Result<()> {
    match edit {
        Edit::Set { path, value } => set(doc, path, value, max_depth),
        Edit::Insert { parent, at, value } => insert(doc, parent, at, value, max_depth),
        Edit::Remove { path } => remove(doc, path),
        Edit::RenameKey { path, new_key } => rename_key(doc, path, new_key),
    }
}

fn edit_error(message: impl Into<String>) -> Error {
    Error::new(codes::CORE_EDIT_FAILED, message)
}

fn read_only_error(path: &Path) -> Error {
    Error::new(
        codes::CORE_READ_ONLY,
        format!("node at '{path}' is read-only"),
    )
}

fn find(doc: &SaveDoc, path: &Path) -> Result<NodeId> {
    doc.resolve(path).ok_or_else(|| {
        Error::new(
            codes::CORE_INVALID_PATH,
            format!("path '{path}' does not exist"),
        )
    })
}

fn is_read_only(doc: &SaveDoc, id: NodeId) -> bool {
    doc.get(id)
        .is_some_and(|node| node.flags.contains(NodeFlags::READ_ONLY))
}

/// What the existing node tells `Set` about the width of a scalar.
enum Existing {
    Int(IntWidth),
    Float(FloatWidth),
    Other,
}

/// Converts `v` to the float width of the node. An `F32` node stores the value narrowed to
/// `f32`; a finite value that does not fit `f32` is refused instead of becoming infinite.
fn float_for_width(v: f64, width: FloatWidth, path: &Path) -> Result<f64> {
    match width {
        FloatWidth::F64 => Ok(v),
        FloatWidth::F32 => {
            let narrowed = f64::from(v as f32);
            if v.is_finite() && !narrowed.is_finite() {
                return Err(edit_error(format!(
                    "value {v} is too large for the 32-bit float at '{path}'"
                )));
            }
            Ok(narrowed)
        }
    }
}

fn set(doc: &mut SaveDoc, path: &Path, value: &OwnedValue, max_depth: usize) -> Result<()> {
    let id = find(doc, path)?;
    if is_read_only(doc, id) {
        return Err(read_only_error(path));
    }
    let existing = match doc.get(id).map(|node| &node.value) {
        Some(Value::Int { width, .. }) => Existing::Int(*width),
        Some(Value::Float { width, .. }) => Existing::Float(*width),
        _ => Existing::Other,
    };
    let new_value = match (value, existing) {
        (OwnedValue::Int(v), Existing::Int(width)) => {
            if !width.contains(*v) {
                return Err(edit_error(format!(
                    "value {v} does not fit {width:?} at '{path}'"
                )));
            }
            Value::Int { v: *v, width }
        }
        (OwnedValue::Int(v), Existing::Float(width)) => {
            if v.unsigned_abs() > MAX_EXACT_FLOAT_INT {
                return Err(edit_error(format!(
                    "integer {v} cannot be stored exactly in the float node at '{path}' (limit is 2^53)"
                )));
            }
            Value::Float {
                v: float_for_width(*v as f64, width, path)?,
                width,
            }
        }
        (OwnedValue::Float(v), Existing::Float(width)) => Value::Float {
            v: float_for_width(*v, width, path)?,
            width,
        },
        (OwnedValue::Float(_), Existing::Int(width)) => {
            return Err(edit_error(format!(
                "cannot store a float in the {width:?} integer node at '{path}'"
            )));
        }
        (other, _) => build_value(doc, other, 0, max_depth)?,
    };
    doc.set_value(id, new_value)
}

fn insert(
    doc: &mut SaveDoc,
    parent: &Path,
    at: &Position,
    value: &OwnedValue,
    max_depth: usize,
) -> Result<()> {
    let parent_id = find(doc, parent)?;
    if is_read_only(doc, parent_id) {
        return Err(read_only_error(parent));
    }
    match doc.get(parent_id).map(|node| &node.value) {
        Some(Value::Array(_) | Value::Object(_)) => {}
        Some(Value::Map(_)) => {
            return Err(edit_error(format!(
                "inserting into the map at '{parent}' is not supported yet"
            )));
        }
        _ => {
            return Err(edit_error(format!(
                "the node at '{parent}' is not an array or object"
            )));
        }
    }
    let child = build_node(doc, value, 0, max_depth, true)?;
    let node = doc
        .node_mut(parent_id)
        .ok_or_else(|| edit_error("parent node disappeared"))?;
    match (&mut node.value, at) {
        (Value::Array(items), Position::Index(index)) => {
            if *index > items.len() {
                return Err(edit_error(format!(
                    "index {index} is past the end of the array at '{parent}' (length {})",
                    items.len()
                )));
            }
            items.insert(*index, child);
        }
        (Value::Array(items), Position::End) => items.push(child),
        (Value::Object(entries), Position::Key(key)) => {
            if entries.iter().any(|(existing, _)| existing == key) {
                return Err(edit_error(format!(
                    "key '{key}' already exists in the object at '{parent}'"
                )));
            }
            entries.push((key.clone(), child));
        }
        (Value::Array(_), Position::Key(_)) => {
            return Err(edit_error(format!(
                "an array at '{parent}' needs an index or the end, not a key"
            )));
        }
        (Value::Object(_), _) => {
            return Err(edit_error(format!(
                "an object at '{parent}' needs a key to insert under"
            )));
        }
        _ => return Err(edit_error("the parent is not an array or object")),
    }
    Ok(())
}

/// Splits a non-root path into its parent path and last segment.
fn split_parent(path: &Path) -> Result<(Path, String)> {
    match (path.parent(), path.last()) {
        (Some(parent), Some(last)) => Ok((parent, last.to_string())),
        _ => Err(edit_error("the root cannot be removed or renamed")),
    }
}

fn remove(doc: &mut SaveDoc, path: &Path) -> Result<()> {
    let (parent, segment) = split_parent(path)?;
    let parent_id = find(doc, &parent)?;
    let target_id = find(doc, path)?;
    if is_read_only(doc, parent_id) || is_read_only(doc, target_id) {
        return Err(read_only_error(path));
    }
    let node = doc
        .node_mut(parent_id)
        .ok_or_else(|| edit_error("parent node disappeared"))?;
    match &mut node.value {
        Value::Array(items) => {
            let index = parse_index(&segment)
                .filter(|index| *index < items.len())
                .ok_or_else(|| edit_error(format!("no array element at '{path}'")))?;
            items.remove(index);
        }
        Value::Object(entries) => {
            let index = entries
                .iter()
                .position(|(key, _)| *key == segment)
                .ok_or_else(|| edit_error(format!("no object entry at '{path}'")))?;
            entries.remove(index);
        }
        Value::Map(entries) => {
            if segment.ends_with(".key") {
                return Err(edit_error(format!(
                    "a map key cannot be removed on its own; remove the entry instead (path without '.key' for '{path}')"
                )));
            }
            let index = parse_index(&segment)
                .filter(|index| *index < entries.len())
                .ok_or_else(|| edit_error(format!("no map entry at '{path}'")))?;
            entries.remove(index);
        }
        _ => {
            return Err(edit_error(format!(
                "the parent of '{path}' is not a container"
            )));
        }
    }
    Ok(())
}

fn rename_key(doc: &mut SaveDoc, path: &Path, new_key: &str) -> Result<()> {
    let (parent, segment) = split_parent(path)?;
    let parent_id = find(doc, &parent)?;
    let target_id = find(doc, path)?;
    if is_read_only(doc, parent_id) || is_read_only(doc, target_id) {
        return Err(read_only_error(path));
    }
    let node = doc
        .node_mut(parent_id)
        .ok_or_else(|| edit_error("parent node disappeared"))?;
    match &mut node.value {
        Value::Object(entries) => {
            let index = entries
                .iter()
                .position(|(key, _)| *key == segment)
                .ok_or_else(|| edit_error(format!("no object entry at '{path}'")))?;
            let clash = entries
                .iter()
                .enumerate()
                .any(|(other, (key, _))| other != index && key == new_key);
            if clash {
                return Err(edit_error(format!(
                    "key '{new_key}' already exists next to '{path}'"
                )));
            }
            let entry = entries
                .get_mut(index)
                .ok_or_else(|| edit_error("entry disappeared"))?;
            entry.0 = new_key.to_string();
            Ok(())
        }
        Value::Map(_) => {
            // Map keys are nodes, not text: they change with `Set` on "<i>.key".
            let entry = segment.strip_suffix(".key").unwrap_or(&segment);
            let mut key_path = parent.clone();
            key_path.push(format!("{entry}.key"));
            Err(edit_error(format!(
                "map keys are not renamed with RenameKey; use Set on '{key_path}' instead"
            )))
        }
        _ => Err(edit_error(format!(
            "the parent of '{path}' is not an object"
        ))),
    }
}

/// Pushes the nodes for `value` into `doc` and returns the id of its top node.
fn build_node(
    doc: &mut SaveDoc,
    value: &OwnedValue,
    depth: usize,
    max_depth: usize,
    synthetic: bool,
) -> Result<NodeId> {
    let built = build_value(doc, value, depth, max_depth)?;
    let mut node = Node::new(built);
    if synthetic {
        node.flags.insert(NodeFlags::SYNTHETIC);
    }
    doc.push(node)
}

/// Converts `value` into a [`Value`], pushing the nodes of its children into `doc`.
/// Children created here are always marked `SYNTHETIC`, except in [`build_doc`] where
/// the whole document is new. Fails with `CORE_LIMIT_DEPTH` below `max_depth` levels.
fn build_value(
    doc: &mut SaveDoc,
    value: &OwnedValue,
    depth: usize,
    max_depth: usize,
) -> Result<Value> {
    if depth > max_depth {
        return Err(Error::new(
            codes::CORE_LIMIT_DEPTH,
            format!("edit value is nested deeper than {max_depth} levels (limit)"),
        ));
    }
    Ok(match value {
        OwnedValue::Null => Value::Null,
        OwnedValue::Bool(v) => Value::Bool(*v),
        OwnedValue::Int(v) => Value::Int {
            v: *v,
            width: IntWidth::Unbounded,
        },
        OwnedValue::Float(v) => Value::Float {
            v: *v,
            width: FloatWidth::F64,
        },
        OwnedValue::Str(v) => Value::Str(v.clone()),
        OwnedValue::Bytes(v) => Value::Bytes(v.clone()),
        OwnedValue::Array(items) => {
            let mut ids = Vec::with_capacity(items.len());
            for item in items {
                ids.push(build_node(doc, item, depth + 1, max_depth, true)?);
            }
            Value::Array(ids)
        }
        OwnedValue::Object(entries) => {
            let mut ids = Vec::with_capacity(entries.len());
            for (key, item) in entries {
                ids.push((
                    key.clone(),
                    build_node(doc, item, depth + 1, max_depth, true)?,
                ));
            }
            Value::Object(ids)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test-local versions with default limits; they shadow the glob-imported functions.
    fn apply_edits(doc: &mut SaveDoc, edits: &[Edit]) -> Result<()> {
        super::apply_edits(doc, edits, &Limits::default())
    }

    fn build_doc(value: &OwnedValue) -> Result<SaveDoc> {
        super::build_doc(value, &Limits::default())
    }

    fn p(text: &str) -> Path {
        Path::parse(text).unwrap()
    }

    /// Compact JSON-like dump of the tree below the root, for readable assertions.
    fn dump(doc: &SaveDoc) -> String {
        fn walk(doc: &SaveDoc, id: NodeId, out: &mut String) {
            match doc.get(id).map(|node| &node.value) {
                None => out.push_str("<missing>"),
                Some(Value::Null) => out.push_str("null"),
                Some(Value::Bool(v)) => out.push_str(&v.to_string()),
                Some(Value::Int { v, .. }) => out.push_str(&v.to_string()),
                Some(Value::Float { v, .. }) => out.push_str(&format!("{v:?}")),
                Some(Value::Str(v)) => out.push_str(&format!("{v:?}")),
                Some(Value::Bytes(v)) => out.push_str(&format!("bytes{v:?}")),
                Some(Value::Array(items)) => {
                    out.push('[');
                    for (i, item) in items.iter().enumerate() {
                        if i > 0 {
                            out.push(',');
                        }
                        walk(doc, *item, out);
                    }
                    out.push(']');
                }
                Some(Value::Object(entries)) => {
                    out.push('{');
                    for (i, (key, item)) in entries.iter().enumerate() {
                        if i > 0 {
                            out.push(',');
                        }
                        out.push_str(&format!("{key:?}:"));
                        walk(doc, *item, out);
                    }
                    out.push('}');
                }
                Some(Value::Map(entries)) => {
                    out.push_str("map(");
                    for (i, (key, item)) in entries.iter().enumerate() {
                        if i > 0 {
                            out.push(',');
                        }
                        walk(doc, *key, out);
                        out.push('=');
                        walk(doc, *item, out);
                    }
                    out.push(')');
                }
                Some(Value::Ref(_)) => out.push_str("ref"),
            }
        }
        let mut out = String::new();
        walk(doc, doc.root(), &mut out);
        out
    }

    fn object(entries: Vec<(&str, OwnedValue)>) -> OwnedValue {
        OwnedValue::Object(
            entries
                .into_iter()
                .map(|(key, value)| (key.to_string(), value))
                .collect(),
        )
    }

    fn strs(items: &[&str]) -> OwnedValue {
        OwnedValue::Array(items.iter().map(|s| OwnedValue::Str((*s).into())).collect())
    }

    fn sample() -> SaveDoc {
        let mut doc = build_doc(&object(vec![
            ("gold", OwnedValue::Int(5)),
            ("items", strs(&["a", "b"])),
            ("name", OwnedValue::Str("x".into())),
        ]))
        .unwrap();
        // Give the gold node a fixed width so range checks apply.
        let gold = doc.resolve(&p("/gold")).unwrap();
        doc.set_value(
            gold,
            Value::Int {
                v: 5,
                width: IntWidth::I32,
            },
        )
        .unwrap();
        doc
    }

    const SAMPLE_DUMP: &str = r#"{"gold":5,"items":["a","b"],"name":"x"}"#;

    #[test]
    fn build_doc_round_trips_through_dump() {
        assert_eq!(dump(&sample()), SAMPLE_DUMP);
    }

    #[test]
    fn set_replaces_value_and_clears_lexeme() {
        let mut doc = sample();
        let id = doc.resolve(&p("/name")).unwrap();
        doc.node_mut(id).unwrap().lexeme = Some("x".into());
        apply_edits(
            &mut doc,
            &[Edit::Set {
                path: p("/name"),
                value: OwnedValue::Str("y".into()),
            }],
        )
        .unwrap();
        assert_eq!(dump(&doc), r#"{"gold":5,"items":["a","b"],"name":"y"}"#);
        assert!(doc.get(id).unwrap().lexeme.is_none());
    }

    #[test]
    fn set_keeps_integer_width() {
        let mut doc = sample();
        apply_edits(
            &mut doc,
            &[Edit::Set {
                path: p("/gold"),
                value: OwnedValue::Int(10),
            }],
        )
        .unwrap();
        let id = doc.resolve(&p("/gold")).unwrap();
        assert_eq!(
            doc.get(id).unwrap().value,
            Value::Int {
                v: 10,
                width: IntWidth::I32
            }
        );
    }

    #[test]
    fn set_rejects_out_of_range_for_the_node_width() {
        let mut doc = build_doc(&OwnedValue::Array(vec![OwnedValue::Null])).unwrap();
        let slot = doc.resolve(&p("/0")).unwrap();
        doc.set_value(
            slot,
            Value::Int {
                v: 1,
                width: IntWidth::I8,
            },
        )
        .unwrap();
        let before = dump(&doc);
        let error = apply_edits(
            &mut doc,
            &[Edit::Set {
                path: p("/0"),
                value: OwnedValue::Int(300),
            }],
        )
        .unwrap_err();
        assert_eq!(error.code(), codes::CORE_EDIT_FAILED);
        assert_eq!(dump(&doc), before);

        apply_edits(
            &mut doc,
            &[Edit::Set {
                path: p("/0"),
                value: OwnedValue::Int(-128),
            }],
        )
        .unwrap();
        assert_eq!(dump(&doc), "[-128]");
    }

    #[test]
    fn set_float_into_integer_node_fails_and_int_into_float_node_works() {
        let mut doc = sample();
        let error = apply_edits(
            &mut doc,
            &[Edit::Set {
                path: p("/gold"),
                value: OwnedValue::Float(1.5),
            }],
        )
        .unwrap_err();
        assert_eq!(error.code(), codes::CORE_EDIT_FAILED);

        let mut floats = build_doc(&OwnedValue::Array(vec![OwnedValue::Float(1.0)])).unwrap();
        apply_edits(
            &mut floats,
            &[Edit::Set {
                path: p("/0"),
                value: OwnedValue::Int(2),
            }],
        )
        .unwrap();
        assert_eq!(dump(&floats), "[2.0]");
    }

    #[test]
    fn set_can_replace_a_scalar_with_a_subtree() {
        let mut doc = sample();
        apply_edits(
            &mut doc,
            &[Edit::Set {
                path: p("/name"),
                value: strs(&["p", "q"]),
            }],
        )
        .unwrap();
        assert_eq!(
            dump(&doc),
            r#"{"gold":5,"items":["a","b"],"name":["p","q"]}"#
        );
    }

    #[test]
    fn insert_remove_and_rename_work_together() {
        let mut doc = sample();
        apply_edits(
            &mut doc,
            &[
                Edit::Set {
                    path: p("/gold"),
                    value: OwnedValue::Int(10),
                },
                Edit::Insert {
                    parent: p("/items"),
                    at: Position::End,
                    value: OwnedValue::Str("c".into()),
                },
                Edit::Insert {
                    parent: p("/items"),
                    at: Position::Index(0),
                    value: OwnedValue::Str("z".into()),
                },
                Edit::Insert {
                    parent: p(""),
                    at: Position::Key("level".into()),
                    value: OwnedValue::Int(3),
                },
                Edit::Remove { path: p("/name") },
                Edit::RenameKey {
                    path: p("/gold"),
                    new_key: "coins".into(),
                },
            ],
        )
        .unwrap();
        assert_eq!(
            dump(&doc),
            r#"{"coins":10,"items":["z","a","b","c"],"level":3}"#
        );
        assert!(doc.resolve(&p("/gold")).is_none());
        assert!(doc.resolve(&p("/coins")).is_some());
    }

    #[test]
    fn inserted_nodes_are_marked_synthetic() {
        let mut doc = sample();
        apply_edits(
            &mut doc,
            &[Edit::Insert {
                parent: p("/items"),
                at: Position::End,
                value: OwnedValue::Str("c".into()),
            }],
        )
        .unwrap();
        let id = doc.resolve(&p("/items/2")).unwrap();
        assert!(doc.get(id).unwrap().flags.contains(NodeFlags::SYNTHETIC));
    }

    #[test]
    fn remove_from_array_shifts_following_elements() {
        let mut doc = sample();
        apply_edits(
            &mut doc,
            &[Edit::Remove {
                path: p("/items/0"),
            }],
        )
        .unwrap();
        assert_eq!(dump(&doc), r#"{"gold":5,"items":["b"],"name":"x"}"#);
    }

    #[test]
    fn failed_second_edit_rolls_back_the_first() {
        let mut doc = sample();
        let error = apply_edits(
            &mut doc,
            &[
                Edit::Set {
                    path: p("/gold"),
                    value: OwnedValue::Int(11),
                },
                Edit::Remove {
                    path: p("/nonexistent"),
                },
            ],
        )
        .unwrap_err();
        assert_eq!(error.code(), codes::CORE_INVALID_PATH);
        assert!(error.context_chain()[0].contains("edit 2/2"));
        assert_eq!(dump(&doc), SAMPLE_DUMP);
    }

    #[test]
    fn invalid_edits_report_edit_failed() {
        let cases = [
            // index past the end
            Edit::Insert {
                parent: p("/items"),
                at: Position::Index(3),
                value: OwnedValue::Null,
            },
            // key into array
            Edit::Insert {
                parent: p("/items"),
                at: Position::Key("k".into()),
                value: OwnedValue::Null,
            },
            // end into object
            Edit::Insert {
                parent: p(""),
                at: Position::End,
                value: OwnedValue::Null,
            },
            // duplicate key
            Edit::Insert {
                parent: p(""),
                at: Position::Key("gold".into()),
                value: OwnedValue::Null,
            },
            // scalar parent
            Edit::Insert {
                parent: p("/name"),
                at: Position::End,
                value: OwnedValue::Null,
            },
            // root removal
            Edit::Remove { path: p("") },
            // rename into an existing key
            Edit::RenameKey {
                path: p("/gold"),
                new_key: "name".into(),
            },
            // rename an array element
            Edit::RenameKey {
                path: p("/items/0"),
                new_key: "k".into(),
            },
        ];
        for edit in cases {
            let mut doc = sample();
            let error = apply_edits(&mut doc, std::slice::from_ref(&edit)).unwrap_err();
            assert_eq!(error.code(), codes::CORE_EDIT_FAILED, "{edit:?}");
            assert_eq!(dump(&doc), SAMPLE_DUMP, "{edit:?}");
        }
    }

    #[test]
    fn read_only_nodes_cannot_be_edited() {
        let mut doc = sample();
        let name = doc.resolve(&p("/name")).unwrap();
        doc.set_flags(name, NodeFlags::READ_ONLY).unwrap();
        let items = doc.resolve(&p("/items")).unwrap();
        doc.set_flags(items, NodeFlags::READ_ONLY).unwrap();

        for edit in [
            Edit::Set {
                path: p("/name"),
                value: OwnedValue::Null,
            },
            Edit::Remove { path: p("/name") },
            Edit::RenameKey {
                path: p("/name"),
                new_key: "n".into(),
            },
            Edit::Insert {
                parent: p("/items"),
                at: Position::End,
                value: OwnedValue::Null,
            },
            Edit::Remove {
                path: p("/items/0"),
            },
        ] {
            let error = apply_edits(&mut doc, std::slice::from_ref(&edit)).unwrap_err();
            assert_eq!(error.code(), codes::CORE_READ_ONLY, "{edit:?}");
        }
        assert_eq!(dump(&doc), SAMPLE_DUMP);
    }

    /// `map("k"=1, 5=2)`
    fn map_doc() -> SaveDoc {
        let mut doc = SaveDoc::new();
        let key = doc.push(Node::new(Value::Str("k".into()))).unwrap();
        let value = doc
            .push(Node::new(Value::Int {
                v: 1,
                width: IntWidth::Unbounded,
            }))
            .unwrap();
        let key2 = doc
            .push(Node::new(Value::Int {
                v: 5,
                width: IntWidth::I32,
            }))
            .unwrap();
        let value2 = doc
            .push(Node::new(Value::Int {
                v: 2,
                width: IntWidth::Unbounded,
            }))
            .unwrap();
        let root = doc
            .push(Node::new(Value::Map(vec![(key, value), (key2, value2)])))
            .unwrap();
        doc.set_root(root);
        doc
    }

    #[test]
    fn map_containers_reject_insert_and_rename_but_allow_remove() {
        let mut doc = map_doc();
        let before = dump(&doc);

        let error = apply_edits(
            &mut doc,
            &[Edit::Insert {
                parent: p(""),
                at: Position::End,
                value: OwnedValue::Null,
            }],
        )
        .unwrap_err();
        assert_eq!(error.code(), codes::CORE_EDIT_FAILED);
        assert!(error.message().contains("not supported yet"));
        assert_eq!(dump(&doc), before);

        apply_edits(&mut doc, &[Edit::Remove { path: p("/0") }]).unwrap();
        assert_eq!(dump(&doc), "map(5=2)");
    }

    #[test]
    fn rename_key_on_a_map_suggests_set_on_the_key_path() {
        let mut doc = map_doc();
        let before = dump(&doc);
        for path in ["/0", "/1.key"] {
            let error = apply_edits(
                &mut doc,
                &[Edit::RenameKey {
                    path: p(path),
                    new_key: "x".into(),
                }],
            )
            .unwrap_err();
            assert_eq!(error.code(), codes::CORE_EDIT_FAILED, "{path}");
            let expected = if path == "/0" { "/0.key" } else { "/1.key" };
            assert!(error.message().contains("Set"), "{}", error.message());
            assert!(error.message().contains(expected), "{}", error.message());
        }
        assert_eq!(dump(&doc), before);
    }

    #[test]
    fn set_on_a_map_key_path_changes_the_key() {
        let mut doc = map_doc();
        apply_edits(
            &mut doc,
            &[
                Edit::Set {
                    path: p("/0.key"),
                    value: OwnedValue::Str("renamed".into()),
                },
                // The numeric key keeps its I32 width.
                Edit::Set {
                    path: p("/1.key"),
                    value: OwnedValue::Int(7),
                },
            ],
        )
        .unwrap();
        assert_eq!(dump(&doc), r#"map("renamed"=1,7=2)"#);
        let key = doc.resolve(&p("/1.key")).unwrap();
        assert_eq!(
            doc.get(key).unwrap().value,
            Value::Int {
                v: 7,
                width: IntWidth::I32
            }
        );

        // The width still guards the key.
        let error = apply_edits(
            &mut doc,
            &[Edit::Set {
                path: p("/1.key"),
                value: OwnedValue::Int(i128::from(i64::MAX)),
            }],
        )
        .unwrap_err();
        assert_eq!(error.code(), codes::CORE_EDIT_FAILED);
        assert_eq!(dump(&doc), r#"map("renamed"=1,7=2)"#);
    }

    #[test]
    fn a_read_only_map_key_cannot_be_set_and_a_key_cannot_be_removed_alone() {
        let mut doc = map_doc();
        let key = doc.resolve(&p("/0.key")).unwrap();
        doc.set_flags(key, NodeFlags::READ_ONLY).unwrap();
        let error = apply_edits(
            &mut doc,
            &[Edit::Set {
                path: p("/0.key"),
                value: OwnedValue::Null,
            }],
        )
        .unwrap_err();
        assert_eq!(error.code(), codes::CORE_READ_ONLY);

        let mut doc = map_doc();
        let error = apply_edits(&mut doc, &[Edit::Remove { path: p("/0.key") }]).unwrap_err();
        assert_eq!(error.code(), codes::CORE_EDIT_FAILED);
        assert_eq!(dump(&doc), r#"map("k"=1,5=2)"#);
    }

    #[test]
    fn too_deep_edit_values_are_rejected() {
        let nested = |levels: usize| {
            let mut value = OwnedValue::Null;
            for _ in 0..levels {
                value = OwnedValue::Array(vec![value]);
            }
            value
        };
        let set = |value| {
            [Edit::Set {
                path: p("/name"),
                value,
            }]
        };

        // Default limits: 512 levels allowed, 600 are not.
        let mut doc = sample();
        let error = apply_edits(&mut doc, &set(nested(600))).unwrap_err();
        assert_eq!(error.code(), codes::CORE_LIMIT_DEPTH);
        assert_eq!(dump(&doc), SAMPLE_DUMP);

        // The limit comes from the caller.
        let shallow = Limits {
            max_depth: 3,
            ..Limits::default()
        };
        let error = super::apply_edits(&mut doc, &set(nested(10)), &shallow).unwrap_err();
        assert_eq!(error.code(), codes::CORE_LIMIT_DEPTH);
        assert_eq!(dump(&doc), SAMPLE_DUMP);
        super::apply_edits(&mut doc, &set(nested(3)), &shallow).unwrap();
        assert_eq!(
            dump(&doc),
            r#"{"gold":5,"items":["a","b"],"name":[[[null]]]}"#
        );

        let error = super::build_doc(&nested(10), &shallow).unwrap_err();
        assert_eq!(error.code(), codes::CORE_LIMIT_DEPTH);
    }

    #[test]
    fn edits_cannot_grow_the_document_past_the_node_limit() {
        let mut doc = sample();
        let before = dump(&doc);
        // Allow exactly one node more than the sample has now.
        let tight = Limits {
            max_nodes: u64::try_from(doc.len()).unwrap() + 1,
            ..Limits::default()
        };
        let insert = |text: &str| Edit::Insert {
            parent: p("/items"),
            at: Position::End,
            value: OwnedValue::Str(text.into()),
        };
        let error = super::apply_edits(&mut doc, &[insert("c"), insert("d")], &tight).unwrap_err();
        assert_eq!(error.code(), codes::CORE_LIMIT_NODES);
        assert_eq!(dump(&doc), before);

        super::apply_edits(&mut doc, &[insert("c")], &tight).unwrap();
        assert_eq!(dump(&doc), r#"{"gold":5,"items":["a","b","c"],"name":"x"}"#);
    }

    #[test]
    fn edits_respect_the_documents_own_node_limit() {
        let mut doc = SaveDoc::with_node_limit(3);
        let slot = doc.push(Node::new(Value::Array(Vec::new()))).unwrap();
        doc.set_root(slot);
        let insert = Edit::Insert {
            parent: p(""),
            at: Position::End,
            value: OwnedValue::Array(vec![OwnedValue::Null, OwnedValue::Null]),
        };
        // Needs 3 new nodes, only 1 is left.
        let error = apply_edits(&mut doc, std::slice::from_ref(&insert)).unwrap_err();
        assert_eq!(error.code(), codes::CORE_LIMIT_NODES);
        assert_eq!(dump(&doc), "[]");
    }

    #[test]
    fn integers_above_two_to_the_53_are_refused_for_float_nodes() {
        let mut doc = build_doc(&OwnedValue::Array(vec![OwnedValue::Float(0.5)])).unwrap();
        let exact = 1_i128 << 53;
        for ok in [exact, -exact, 0, 7] {
            apply_edits(
                &mut doc,
                &[Edit::Set {
                    path: p("/0"),
                    value: OwnedValue::Int(ok),
                }],
            )
            .unwrap();
        }
        assert_eq!(dump(&doc), "[7.0]");
        for bad in [exact + 1, -exact - 1, i128::MAX, i128::MIN] {
            let error = apply_edits(
                &mut doc,
                &[Edit::Set {
                    path: p("/0"),
                    value: OwnedValue::Int(bad),
                }],
            )
            .unwrap_err();
            assert_eq!(error.code(), codes::CORE_EDIT_FAILED, "{bad}");
        }
        assert_eq!(dump(&doc), "[7.0]");
    }

    fn f32_doc() -> SaveDoc {
        let mut doc = build_doc(&OwnedValue::Array(vec![OwnedValue::Float(0.5)])).unwrap();
        let slot = doc.resolve(&p("/0")).unwrap();
        doc.set_value(
            slot,
            Value::Float {
                v: 0.5,
                width: FloatWidth::F32,
            },
        )
        .unwrap();
        doc
    }

    #[test]
    fn f32_nodes_store_the_narrowed_value() {
        let mut doc = f32_doc();
        apply_edits(
            &mut doc,
            &[Edit::Set {
                path: p("/0"),
                value: OwnedValue::Float(0.1),
            }],
        )
        .unwrap();
        let id = doc.resolve(&p("/0")).unwrap();
        assert_eq!(
            doc.get(id).unwrap().value,
            Value::Float {
                v: f64::from(0.1_f32),
                width: FloatWidth::F32
            }
        );

        // An integer goes through the same narrowing: 2^24 + 1 is not an f32.
        apply_edits(
            &mut doc,
            &[Edit::Set {
                path: p("/0"),
                value: OwnedValue::Int((1 << 24) + 1),
            }],
        )
        .unwrap();
        assert_eq!(
            doc.get(id).unwrap().value,
            Value::Float {
                v: 16_777_216.0,
                width: FloatWidth::F32
            }
        );

        // NaN and infinities were not produced by overflow, so they pass unchanged.
        apply_edits(
            &mut doc,
            &[Edit::Set {
                path: p("/0"),
                value: OwnedValue::Float(f64::INFINITY),
            }],
        )
        .unwrap();
    }

    #[test]
    fn f32_nodes_refuse_values_that_overflow_to_infinity() {
        let mut doc = f32_doc();
        let before = dump(&doc);
        for bad in [1.0e40, -1.0e40, f64::MAX] {
            let error = apply_edits(
                &mut doc,
                &[Edit::Set {
                    path: p("/0"),
                    value: OwnedValue::Float(bad),
                }],
            )
            .unwrap_err();
            assert_eq!(error.code(), codes::CORE_EDIT_FAILED, "{bad}");
            assert_eq!(dump(&doc), before);
        }
        // The same value fits an F64 node.
        let mut wide = build_doc(&OwnedValue::Array(vec![OwnedValue::Float(0.5)])).unwrap();
        apply_edits(
            &mut wide,
            &[Edit::Set {
                path: p("/0"),
                value: OwnedValue::Float(1.0e40),
            }],
        )
        .unwrap();
    }

    #[test]
    fn empty_edit_list_is_a_no_op() {
        let mut doc = sample();
        apply_edits(&mut doc, &[]).unwrap();
        assert_eq!(dump(&doc), SAMPLE_DUMP);
    }
}
