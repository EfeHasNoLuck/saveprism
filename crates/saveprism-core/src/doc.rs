//! The universal save document: an arena of typed nodes.
//!
//! Nodes are added in one of two ways, so callers never need a mutable reference to a node that
//! is still being built:
//!
//! - [`SaveDoc::push`]: children first, then the container that lists them (trees).
//! - [`SaveDoc::reserve`] then [`SaveDoc::finish`]: the id exists before the node is filled, so a
//!   container can refer to itself or to an ancestor (cyclic pickle / Marshal graphs).
//!
//! Every document has a node limit; `push` and `reserve` fail with `CORE_LIMIT_NODES` beyond it.
//! Edits go through [`SaveDoc::set_value`] (which drops the stale `lexeme`) or through
//! [`apply_edits`](crate::apply_edits).

use std::collections::BTreeSet;

use crate::error::{Error, Result, codes};
use crate::limits::{Limits, len_u64};
use crate::path::{Path, parse_index};

/// Index of a node inside one [`SaveDoc`]. Only meaningful for the document that produced it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(u32);

impl NodeId {
    /// The arena index of the node.
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// Integer width of a node; `Unbounded` is used by text formats that have no fixed width.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IntWidth {
    /// Signed 8-bit.
    I8,
    /// Signed 16-bit.
    I16,
    /// Signed 32-bit.
    I32,
    /// Signed 64-bit.
    I64,
    /// Unsigned 8-bit.
    U8,
    /// Unsigned 16-bit.
    U16,
    /// Unsigned 32-bit.
    U32,
    /// Unsigned 64-bit.
    U64,
    /// No fixed width (text formats); any `i128` fits.
    Unbounded,
}

impl IntWidth {
    /// Inclusive value range, or `None` for `Unbounded`.
    pub fn range(self) -> Option<(i128, i128)> {
        match self {
            IntWidth::I8 => Some((i128::from(i8::MIN), i128::from(i8::MAX))),
            IntWidth::I16 => Some((i128::from(i16::MIN), i128::from(i16::MAX))),
            IntWidth::I32 => Some((i128::from(i32::MIN), i128::from(i32::MAX))),
            IntWidth::I64 => Some((i128::from(i64::MIN), i128::from(i64::MAX))),
            IntWidth::U8 => Some((0, i128::from(u8::MAX))),
            IntWidth::U16 => Some((0, i128::from(u16::MAX))),
            IntWidth::U32 => Some((0, i128::from(u32::MAX))),
            IntWidth::U64 => Some((0, i128::from(u64::MAX))),
            IntWidth::Unbounded => None,
        }
    }

    /// True when `value` fits this width.
    pub fn contains(self, value: i128) -> bool {
        match self.range() {
            Some((low, high)) => (low..=high).contains(&value),
            None => true,
        }
    }
}

/// Floating point width of a node.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FloatWidth {
    /// 32-bit float.
    F32,
    /// 64-bit float.
    F64,
}

/// Format-specific type name in the form `"<domain>:<name>"`, e.g. `"JSON:float"` or `"UE:IntProperty"`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TypeHint(Box<str>);

impl TypeHint {
    /// Creates a hint from its textual form.
    pub fn new(hint: impl Into<Box<str>>) -> Self {
        Self(hint.into())
    }

    /// The textual form.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Per-node flag set (`READ_ONLY`, `SYNTHETIC`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct NodeFlags(u8);

impl NodeFlags {
    /// The node must not be edited.
    pub const READ_ONLY: NodeFlags = NodeFlags(1);
    /// The node was created by an edit and has no counterpart in the original bytes.
    pub const SYNTHETIC: NodeFlags = NodeFlags(2);

    /// A flag set with no flag.
    pub const fn empty() -> Self {
        Self(0)
    }

    /// True when every flag of `other` is set.
    pub fn contains(self, other: NodeFlags) -> bool {
        self.0 & other.0 == other.0
    }

    /// Sets every flag of `other`.
    pub fn insert(&mut self, other: NodeFlags) {
        self.0 |= other.0;
    }

    /// Clears every flag of `other`.
    pub fn remove(&mut self, other: NodeFlags) {
        self.0 &= !other.0;
    }
}

/// Byte range of a node in the original input, used for lossless patching.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ByteSpan {
    /// First byte (inclusive).
    pub start: u64,
    /// One past the last byte (exclusive).
    pub end: u64,
}

/// The value of a node. Containers refer to their children by [`NodeId`].
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// No value.
    Null,
    /// Boolean.
    Bool(bool),
    /// Integer with its original width.
    Int {
        /// The value.
        v: i128,
        /// The width it is stored with.
        width: IntWidth,
    },
    /// Floating point number with its original width.
    Float {
        /// The value.
        v: f64,
        /// The width it is stored with.
        width: FloatWidth,
    },
    /// Text.
    Str(String),
    /// Raw bytes.
    Bytes(Vec<u8>),
    /// Ordered list of children.
    Array(Vec<NodeId>),
    /// Text-keyed children; order and repeated keys are preserved.
    Object(Vec<(String, NodeId)>),
    /// Children keyed by arbitrary nodes (GVAS maps, pickle dicts, Ruby hashes).
    Map(Vec<(NodeId, NodeId)>),
    /// Reference to another node (JsonEx, pickle, Marshal back-references).
    Ref(NodeId),
}

/// One node of the document.
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    /// The value.
    pub value: Value,
    /// Format-specific type name, if the codec provides one.
    pub type_hint: Option<TypeHint>,
    /// Original text of a scalar when it differs from the codec's canonical form (`"1.0"`).
    pub lexeme: Option<Box<str>>,
    /// Original byte range, if the codec tracks it.
    pub span: Option<ByteSpan>,
    /// Node flags.
    pub flags: NodeFlags,
}

impl Node {
    /// Creates a node with no hint, lexeme, span or flags.
    pub fn new(value: Value) -> Self {
        Self {
            value,
            type_hint: None,
            lexeme: None,
            span: None,
            flags: NodeFlags::empty(),
        }
    }
}

/// The universal document tree (arena). Fields are private; build it with [`SaveDoc::push`]
/// or [`SaveDoc::reserve`] / [`SaveDoc::finish`].
#[derive(Clone, Debug)]
pub struct SaveDoc {
    nodes: Vec<Node>,
    root: NodeId,
    max_nodes: u64,
    /// Ids handed out by `reserve` that `finish` has not filled yet.
    pending: BTreeSet<u32>,
}

impl Default for SaveDoc {
    fn default() -> Self {
        Self::new()
    }
}

impl SaveDoc {
    /// Creates a document whose root is a `Null` node (node 0), with the default node limit
    /// (`Limits::default().max_nodes`).
    pub fn new() -> Self {
        Self::with_node_limit(Limits::default().max_nodes)
    }

    /// Like [`SaveDoc::new`] with an explicit node limit. The root node counts as one node.
    pub fn with_node_limit(max: u64) -> Self {
        Self {
            nodes: vec![Node::new(Value::Null)],
            root: NodeId(0),
            max_nodes: max,
            pending: BTreeSet::new(),
        }
    }

    /// The node limit of this document.
    pub fn node_limit(&self) -> u64 {
        self.max_nodes
    }

    /// The index the next node would get, or `CORE_LIMIT_NODES` when the node limit or the
    /// `u32` id space is used up.
    fn next_index(&self) -> Result<u32> {
        let len = self.nodes.len();
        let index = u32::try_from(len).ok().filter(|index| *index < u32::MAX);
        match index {
            Some(index) if len_u64(len) < self.max_nodes => Ok(index),
            _ => Err(Error::new(
                codes::CORE_LIMIT_NODES,
                format!("document has more than {} nodes (limit)", self.max_nodes),
            )),
        }
    }

    /// Adds a finished node and returns its id. Push children first, then the container that
    /// lists them. Fails with `CORE_LIMIT_NODES` when the node limit is reached.
    pub fn push(&mut self, node: Node) -> Result<NodeId> {
        let index = self.next_index()?;
        self.nodes.push(node);
        Ok(NodeId(index))
    }

    /// Reserves a node id with a `Null` placeholder that must be filled with [`SaveDoc::finish`].
    /// The id can be used inside other nodes (or the node itself) before it is filled, which is
    /// how cyclic graphs are built. Fails with `CORE_LIMIT_NODES` when the node limit is reached.
    pub fn reserve(&mut self) -> Result<NodeId> {
        let index = self.next_index()?;
        self.nodes.push(Node::new(Value::Null));
        self.pending.insert(index);
        Ok(NodeId(index))
    }

    /// Fills a node that was reserved and not filled yet; works once per reservation.
    /// Fails with `CORE_DOC_BUILD` for ids that were never reserved or are already filled.
    pub fn finish(&mut self, id: NodeId, node: Node) -> Result<()> {
        if !self.pending.remove(&id.0) {
            return Err(Error::new(
                codes::CORE_DOC_BUILD,
                format!(
                    "node {} was not reserved or is already finished; only reserved nodes can be finished, once",
                    id.index()
                ),
            ));
        }
        match self.nodes.get_mut(id.index()) {
            Some(slot) => {
                *slot = node;
                Ok(())
            }
            None => Err(Error::new(
                codes::CORE_DOC_BUILD,
                format!("node {} does not exist", id.index()),
            )),
        }
    }

    /// Number of reserved nodes that have not been finished. A decoded document must report 0.
    pub fn unfinished_count(&self) -> usize {
        self.pending.len()
    }

    /// Makes `id` the root of the document.
    pub fn set_root(&mut self, id: NodeId) {
        self.root = id;
    }

    /// The root node id.
    pub fn root(&self) -> NodeId {
        self.root
    }

    /// Number of nodes in the arena, including nodes that are no longer reachable from the root.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// True when the arena holds no node.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// The node with this id.
    pub fn get(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(id.index())
    }

    /// Reads a node by its stable arena position. Intended for format writers that must inspect
    /// every decoded leaf without exposing a mutable node ID constructor.
    pub fn get_index(&self, index: usize) -> Option<&Node> {
        self.nodes.get(index)
    }

    /// Mutable access for the crate's own edit code; never exposed to callers.
    pub(crate) fn node_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.nodes.get_mut(id.index())
    }

    /// Child ids in display order. `Object` and `Map` yield their value nodes (entry `i` is child `i`);
    /// `Ref` and scalars have no children.
    pub fn children(&self, id: NodeId) -> Vec<NodeId> {
        match self.get(id).map(|node| &node.value) {
            Some(Value::Array(items)) => items.clone(),
            Some(Value::Object(entries)) => entries.iter().map(|(_, child)| *child).collect(),
            Some(Value::Map(entries)) => entries.iter().map(|(_, child)| *child).collect(),
            _ => Vec::new(),
        }
    }

    /// Resolves a path from the root. `Ref` nodes are not followed.
    ///
    /// On a `Map`, `"<i>"` is the value of entry `i` and `"<i>.key"` is its key node.
    pub fn resolve(&self, path: &Path) -> Option<NodeId> {
        let mut current = self.root;
        for segment in path.segments() {
            current = self.child_by_segment(current, segment)?;
        }
        self.get(current).map(|_| current)
    }

    fn child_by_segment(&self, id: NodeId, segment: &str) -> Option<NodeId> {
        match &self.get(id)?.value {
            Value::Object(entries) => entries
                .iter()
                .find(|(key, _)| key.as_str() == segment)
                .map(|(_, child)| *child),
            Value::Array(items) => parse_index(segment)
                .and_then(|index| items.get(index))
                .copied(),
            Value::Map(entries) => match segment.strip_suffix(".key") {
                Some(index) => parse_index(index)
                    .and_then(|index| entries.get(index))
                    .map(|(key, _)| *key),
                None => parse_index(segment)
                    .and_then(|index| entries.get(index))
                    .map(|(_, value)| *value),
            },
            _ => None,
        }
    }

    /// Replaces the value of a node and drops its `lexeme`. Fails with `CORE_READ_ONLY` on read-only nodes.
    pub fn set_value(&mut self, id: NodeId, value: Value) -> Result<()> {
        let node = self.node_mut(id).ok_or_else(|| missing_node(id))?;
        if node.flags.contains(NodeFlags::READ_ONLY) {
            return Err(Error::new(
                codes::CORE_READ_ONLY,
                format!("node {} is read-only", id.index()),
            ));
        }
        node.value = value;
        node.lexeme = None;
        Ok(())
    }

    /// Replaces the flags of a node.
    pub fn set_flags(&mut self, id: NodeId, flags: NodeFlags) -> Result<()> {
        let node = self.node_mut(id).ok_or_else(|| missing_node(id))?;
        node.flags = flags;
        Ok(())
    }

    /// Structural equality of two documents, starting at their roots.
    ///
    /// Compared: structure, values, integer and float widths, `type_hint`. Ignored:
    /// `lexeme`, `span`, `flags` and the numbering of `NodeId`s. Floats compare by `to_bits`.
    /// The walk uses an explicit stack, so very deep trees cannot overflow the call stack.
    ///
    /// The walk pairs nodes one to one (node `a` of `self` with node `b` of `other`):
    /// a pair is compared once and never again, so shared children (a DAG) cost nothing extra
    /// and cycles terminate. The pairing must stay consistent: if `a` was paired with `b`,
    /// meeting `a` again opposite any other node (or `b` opposite any other node) is a mismatch,
    /// so the sharing structure of both documents has to be the same.
    /// A `Ref` pair compares equal only when the two targets are themselves paired, which the
    /// walk enforces by also visiting the targets as a pair. A `Ref` to a node outside the
    /// containment tree is therefore compared as well. JsonEx (M2) may refine this.
    pub fn tree_eq(&self, other: &SaveDoc) -> bool {
        let mut left_to_right = vec![UNPAIRED; self.nodes.len()];
        let mut right_to_left = vec![UNPAIRED; other.nodes.len()];
        let mut stack = vec![(self.root, other.root)];
        while let Some((left_id, right_id)) = stack.pop() {
            let (left, right) = match (self.get(left_id), other.get(right_id)) {
                (Some(left), Some(right)) => (left, right),
                (None, None) => continue,
                _ => return false,
            };
            let (Some(forward), Some(backward)) = (
                left_to_right.get_mut(left_id.index()),
                right_to_left.get_mut(right_id.index()),
            ) else {
                return false;
            };
            if *forward == UNPAIRED && *backward == UNPAIRED {
                *forward = right_id.0;
                *backward = left_id.0;
            } else if *forward == right_id.0 && *backward == left_id.0 {
                continue;
            } else {
                return false;
            }
            if left.type_hint != right.type_hint {
                return false;
            }
            match (&left.value, &right.value) {
                (Value::Null, Value::Null) => {}
                (Value::Ref(a), Value::Ref(b)) => stack.push((*a, *b)),
                (Value::Bool(a), Value::Bool(b)) => {
                    if a != b {
                        return false;
                    }
                }
                (Value::Int { v: a, width: wa }, Value::Int { v: b, width: wb }) => {
                    if a != b || wa != wb {
                        return false;
                    }
                }
                (Value::Float { v: a, width: wa }, Value::Float { v: b, width: wb }) => {
                    if a.to_bits() != b.to_bits() || wa != wb {
                        return false;
                    }
                }
                (Value::Str(a), Value::Str(b)) => {
                    if a != b {
                        return false;
                    }
                }
                (Value::Bytes(a), Value::Bytes(b)) => {
                    if a != b {
                        return false;
                    }
                }
                (Value::Array(a), Value::Array(b)) => {
                    if a.len() != b.len() {
                        return false;
                    }
                    stack.extend(a.iter().copied().zip(b.iter().copied()));
                }
                (Value::Object(a), Value::Object(b)) => {
                    if a.len() != b.len() {
                        return false;
                    }
                    for ((key_a, child_a), (key_b, child_b)) in a.iter().zip(b.iter()) {
                        if key_a != key_b {
                            return false;
                        }
                        stack.push((*child_a, *child_b));
                    }
                }
                (Value::Map(a), Value::Map(b)) => {
                    if a.len() != b.len() {
                        return false;
                    }
                    for ((key_a, value_a), (key_b, value_b)) in a.iter().zip(b.iter()) {
                        stack.push((*key_a, *key_b));
                        stack.push((*value_a, *value_b));
                    }
                }
                _ => return false,
            }
        }
        true
    }
}

/// Marks a node that `tree_eq` has not paired yet (never a real id: ids stay below `u32::MAX`).
const UNPAIRED: u32 = u32::MAX;

fn missing_node(id: NodeId) -> Error {
    Error::new(
        codes::CORE_EDIT_FAILED,
        format!("node {} does not exist", id.index()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scalar(doc: &mut SaveDoc, value: Value) -> NodeId {
        doc.push(Node::new(value)).unwrap()
    }

    fn int(doc: &mut SaveDoc, v: i128) -> NodeId {
        scalar(
            doc,
            Value::Int {
                v,
                width: IntWidth::Unbounded,
            },
        )
    }

    /// `{"a": [1, "x"], "/b": 7, "~c": true}`
    fn sample() -> SaveDoc {
        let mut doc = SaveDoc::new();
        let one = int(&mut doc, 1);
        let x = scalar(&mut doc, Value::Str("x".into()));
        let list = doc.push(Node::new(Value::Array(vec![one, x]))).unwrap();
        let seven = int(&mut doc, 7);
        let flag = scalar(&mut doc, Value::Bool(true));
        let root = doc
            .push(Node::new(Value::Object(vec![
                ("a".into(), list),
                ("/b".into(), seven),
                ("~c".into(), flag),
            ])))
            .unwrap();
        doc.set_root(root);
        doc
    }

    fn at(doc: &SaveDoc, path: &str) -> Option<NodeId> {
        doc.resolve(&Path::parse(path).unwrap())
    }

    #[test]
    fn new_document_has_null_root() {
        let doc = SaveDoc::new();
        assert_eq!(doc.get(doc.root()).unwrap().value, Value::Null);
        assert_eq!(doc.len(), 1);
        assert!(!doc.is_empty());
    }

    #[test]
    fn resolve_walks_objects_arrays_and_escapes() {
        let doc = sample();
        assert_eq!(at(&doc, ""), Some(doc.root()));
        let one = at(&doc, "/a/0").unwrap();
        assert!(matches!(
            doc.get(one).unwrap().value,
            Value::Int { v: 1, .. }
        ));
        let seven = at(&doc, "/~1b").unwrap();
        assert!(matches!(
            doc.get(seven).unwrap().value,
            Value::Int { v: 7, .. }
        ));
        let flag = at(&doc, "/~0c").unwrap();
        assert_eq!(doc.get(flag).unwrap().value, Value::Bool(true));
        assert_eq!(at(&doc, "/a/2"), None);
        assert_eq!(at(&doc, "/a/01"), None);
        assert_eq!(at(&doc, "/a/x"), None);
        assert_eq!(at(&doc, "/missing"), None);
        assert_eq!(at(&doc, "/~1b/deeper"), None);
    }

    #[test]
    fn resolve_uses_first_match_for_repeated_keys() {
        let mut doc = SaveDoc::new();
        let first = int(&mut doc, 1);
        let second = int(&mut doc, 2);
        let root = doc
            .push(Node::new(Value::Object(vec![
                ("k".into(), first),
                ("k".into(), second),
            ])))
            .unwrap();
        doc.set_root(root);
        assert_eq!(at(&doc, "/k"), Some(first));
    }

    #[test]
    fn resolve_map_uses_entry_index_and_value_node() {
        let mut doc = SaveDoc::new();
        let key = scalar(&mut doc, Value::Str("k".into()));
        let value = int(&mut doc, 9);
        let root = doc.push(Node::new(Value::Map(vec![(key, value)]))).unwrap();
        doc.set_root(root);
        assert_eq!(at(&doc, "/0"), Some(value));
        assert_eq!(at(&doc, "/1"), None);
        assert_eq!(doc.children(root), vec![value]);
    }

    #[test]
    fn resolve_map_key_nodes_with_the_key_suffix() {
        let mut doc = SaveDoc::new();
        let key = scalar(&mut doc, Value::Str("k".into()));
        let value = int(&mut doc, 9);
        let key2 = int(&mut doc, 5);
        let value2 = int(&mut doc, 6);
        let root = doc
            .push(Node::new(Value::Map(vec![(key, value), (key2, value2)])))
            .unwrap();
        doc.set_root(root);
        assert_eq!(at(&doc, "/0.key"), Some(key));
        assert_eq!(at(&doc, "/1.key"), Some(key2));
        assert_eq!(at(&doc, "/1"), Some(value2));
        assert_eq!(at(&doc, "/2.key"), None);
        assert_eq!(at(&doc, "/01.key"), None);
        assert_eq!(at(&doc, "/.key"), None);
        assert_eq!(at(&doc, "/x.key"), None);
        // The suffix means nothing for objects: it is an ordinary key there.
        let mut object = SaveDoc::new();
        let leaf = int(&mut object, 1);
        let root = object
            .push(Node::new(Value::Object(vec![("0.key".into(), leaf)])))
            .unwrap();
        object.set_root(root);
        assert_eq!(at(&object, "/0.key"), Some(leaf));
    }

    #[test]
    fn children_of_each_container() {
        let doc = sample();
        assert_eq!(doc.children(doc.root()).len(), 3);
        let list = at(&doc, "/a").unwrap();
        assert_eq!(doc.children(list).len(), 2);
        let leaf = at(&doc, "/a/0").unwrap();
        assert!(doc.children(leaf).is_empty());
    }

    #[test]
    fn set_value_clears_lexeme() {
        let mut doc = SaveDoc::new();
        let mut node = Node::new(Value::Float {
            v: 1.0,
            width: FloatWidth::F64,
        });
        node.lexeme = Some("1.0".into());
        let id = doc.push(node).unwrap();
        doc.set_root(id);
        doc.set_value(
            id,
            Value::Float {
                v: 2.0,
                width: FloatWidth::F64,
            },
        )
        .unwrap();
        assert!(doc.get(id).unwrap().lexeme.is_none());
    }

    #[test]
    fn set_value_rejects_read_only_nodes() {
        let mut doc = SaveDoc::new();
        let id = int(&mut doc, 1);
        let mut flags = NodeFlags::empty();
        flags.insert(NodeFlags::READ_ONLY);
        doc.set_flags(id, flags).unwrap();
        let error = doc.set_value(id, Value::Null).unwrap_err();
        assert_eq!(error.code(), codes::CORE_READ_ONLY);

        flags.remove(NodeFlags::READ_ONLY);
        doc.set_flags(id, flags).unwrap();
        doc.set_value(id, Value::Null).unwrap();
    }

    #[test]
    fn set_value_on_missing_node_fails() {
        let mut doc = SaveDoc::new();
        let error = doc.set_value(NodeId(99), Value::Null).unwrap_err();
        assert_eq!(error.code(), codes::CORE_EDIT_FAILED);
    }

    #[test]
    fn flags_behave_like_a_set() {
        let mut flags = NodeFlags::empty();
        assert!(!flags.contains(NodeFlags::READ_ONLY));
        flags.insert(NodeFlags::READ_ONLY);
        flags.insert(NodeFlags::SYNTHETIC);
        assert!(flags.contains(NodeFlags::READ_ONLY));
        assert!(flags.contains(NodeFlags::SYNTHETIC));
        flags.remove(NodeFlags::READ_ONLY);
        assert!(!flags.contains(NodeFlags::READ_ONLY));
        assert!(flags.contains(NodeFlags::SYNTHETIC));
    }

    #[test]
    fn int_width_ranges() {
        assert!(IntWidth::I8.contains(127));
        assert!(!IntWidth::I8.contains(128));
        assert!(!IntWidth::U8.contains(-1));
        assert!(IntWidth::U64.contains(i128::from(u64::MAX)));
        assert!(!IntWidth::U64.contains(i128::from(u64::MAX) + 1));
        assert!(IntWidth::Unbounded.contains(i128::MAX));
    }

    #[test]
    fn tree_eq_ignores_lexeme_span_flags_and_numbering() {
        let a = sample();
        let mut b = SaveDoc::new();
        // Different arena order and numbering: build the same tree back to front.
        let flag = b.push(Node::new(Value::Bool(true))).unwrap();
        let seven = int(&mut b, 7);
        let x = scalar(&mut b, Value::Str("x".into()));
        let one = {
            let mut node = Node::new(Value::Int {
                v: 1,
                width: IntWidth::Unbounded,
            });
            node.lexeme = Some("1".into());
            node.span = Some(ByteSpan { start: 3, end: 4 });
            node.flags.insert(NodeFlags::SYNTHETIC);
            b.push(node).unwrap()
        };
        let list = b.push(Node::new(Value::Array(vec![one, x]))).unwrap();
        let root = b
            .push(Node::new(Value::Object(vec![
                ("a".into(), list),
                ("/b".into(), seven),
                ("~c".into(), flag),
            ])))
            .unwrap();
        b.set_root(root);
        assert!(a.tree_eq(&b));
        assert!(b.tree_eq(&a));
    }

    #[test]
    fn tree_eq_detects_differences() {
        let a = sample();

        let mut value_differs = sample();
        let id = at(&value_differs, "/a/1").unwrap();
        value_differs.set_value(id, Value::Str("y".into())).unwrap();
        assert!(!a.tree_eq(&value_differs));

        let mut width_differs = sample();
        let id = at(&width_differs, "/~1b").unwrap();
        width_differs
            .set_value(
                id,
                Value::Int {
                    v: 7,
                    width: IntWidth::I32,
                },
            )
            .unwrap();
        assert!(!a.tree_eq(&width_differs));

        let mut hint_differs = sample();
        let id = at(&hint_differs, "/~1b").unwrap();
        hint_differs.node_mut(id).unwrap().type_hint = Some(TypeHint::new("JSON:int"));
        assert!(!a.tree_eq(&hint_differs));

        let mut key_differs = sample();
        let root = key_differs.root();
        key_differs.node_mut(root).unwrap().value =
            Value::Object(vec![("zzz".into(), at(&a, "/a").unwrap())]);
        assert!(!a.tree_eq(&key_differs));
    }

    #[test]
    fn tree_eq_compares_floats_by_bits() {
        let make = |v: f64| {
            let mut doc = SaveDoc::new();
            let id = scalar(
                &mut doc,
                Value::Float {
                    v,
                    width: FloatWidth::F64,
                },
            );
            doc.set_root(id);
            doc
        };
        assert!(make(f64::NAN).tree_eq(&make(f64::NAN)));
        assert!(!make(0.0).tree_eq(&make(-0.0)));
        assert!(make(1.5).tree_eq(&make(1.5)));
    }

    #[test]
    fn tree_eq_survives_very_deep_chains() {
        let mut a = SaveDoc::new();
        let leaf = a.push(Node::new(Value::Null)).unwrap();
        let mut current = leaf;
        for _ in 0..100_000 {
            current = a.push(Node::new(Value::Array(vec![current]))).unwrap();
        }
        a.set_root(current);

        let mut b = a.clone();
        assert!(a.tree_eq(&b));

        b.set_value(leaf, Value::Bool(true)).unwrap();
        assert!(!a.tree_eq(&b));
    }

    #[test]
    fn tree_eq_refs_with_the_same_target_are_equal() {
        // A: [Str("t"), Null, Ref(first)]
        let mut a = SaveDoc::new();
        let target = scalar(&mut a, Value::Str("t".into()));
        let other = scalar(&mut a, Value::Null);
        let reference = scalar(&mut a, Value::Ref(target));
        let root = scalar(&mut a, Value::Array(vec![target, other, reference]));
        a.set_root(root);

        // B: the same shape, nodes created in a different order.
        let mut b = SaveDoc::new();
        let other = scalar(&mut b, Value::Null);
        let target = scalar(&mut b, Value::Str("t".into()));
        let reference = scalar(&mut b, Value::Ref(target));
        let root = scalar(&mut b, Value::Array(vec![target, other, reference]));
        b.set_root(root);

        assert!(a.tree_eq(&b));
        assert!(b.tree_eq(&a));
    }

    #[test]
    fn tree_eq_refs_with_different_targets_are_not_equal() {
        // Both targets look identical (Null); only the reference topology differs.
        let build = |ref_to_first: bool| {
            let mut doc = SaveDoc::new();
            let first = scalar(&mut doc, Value::Null);
            let second = scalar(&mut doc, Value::Null);
            let target = if ref_to_first { first } else { second };
            let reference = scalar(&mut doc, Value::Ref(target));
            let root = scalar(&mut doc, Value::Array(vec![first, second, reference]));
            doc.set_root(root);
            doc
        };
        assert!(build(true).tree_eq(&build(true)));
        assert!(!build(true).tree_eq(&build(false)));
        assert!(!build(false).tree_eq(&build(true)));
    }

    #[test]
    fn tree_eq_compares_ref_targets_outside_the_containment_tree() {
        let build = |text: &str| {
            let mut doc = SaveDoc::new();
            let orphan = scalar(&mut doc, Value::Str(text.into()));
            let root = scalar(&mut doc, Value::Ref(orphan));
            doc.set_root(root);
            doc
        };
        assert!(build("a").tree_eq(&build("a")));
        assert!(!build("a").tree_eq(&build("b")));
    }

    #[test]
    fn tree_eq_handles_a_ref_that_points_to_itself() {
        let build = || {
            let mut doc = SaveDoc::new();
            let id = doc.reserve().unwrap();
            doc.finish(id, Node::new(Value::Ref(id))).unwrap();
            doc.set_root(id);
            doc
        };
        let a = build();
        assert!(a.tree_eq(&build()));

        // A self-reference is not the same as a reference to some other Null node.
        let mut b = SaveDoc::new();
        let leaf = scalar(&mut b, Value::Null);
        let root = scalar(&mut b, Value::Ref(leaf));
        b.set_root(root);
        assert!(!a.tree_eq(&b));
        assert!(!b.tree_eq(&a));
    }

    #[test]
    fn tree_eq_handles_containment_cycles() {
        let build_cycle = || {
            let mut doc = SaveDoc::new();
            let id = doc.reserve().unwrap();
            doc.finish(id, Node::new(Value::Array(vec![id]))).unwrap();
            doc.set_root(id);
            doc
        };
        let a = build_cycle();
        assert!(a.tree_eq(&build_cycle()));

        // outer -> inner -> inner: the root is not part of the cycle, unlike in `a`.
        let mut b = SaveDoc::new();
        let inner = b.reserve().unwrap();
        let outer = b.push(Node::new(Value::Array(vec![inner]))).unwrap();
        b.finish(inner, Node::new(Value::Array(vec![inner])))
            .unwrap();
        b.set_root(outer);
        assert!(!a.tree_eq(&b));
        assert!(!b.tree_eq(&a));
    }

    #[test]
    fn tree_eq_does_not_pay_for_shared_children() {
        // 64 levels of "[x, x]": 2^64 paths but only 65 nodes.
        let build = |leaf: Value| {
            let mut doc = SaveDoc::new();
            let mut current = scalar(&mut doc, leaf);
            for _ in 0..64 {
                current = scalar(&mut doc, Value::Array(vec![current, current]));
            }
            doc.set_root(current);
            doc
        };
        assert!(build(Value::Null).tree_eq(&build(Value::Null)));
        assert!(!build(Value::Null).tree_eq(&build(Value::Bool(true))));
    }

    #[test]
    fn tree_eq_requires_the_same_sharing_structure() {
        let mut shared = SaveDoc::new();
        let leaf = scalar(&mut shared, Value::Null);
        let root = scalar(&mut shared, Value::Array(vec![leaf, leaf]));
        shared.set_root(root);

        let mut separate = SaveDoc::new();
        let first = scalar(&mut separate, Value::Null);
        let second = scalar(&mut separate, Value::Null);
        let root = scalar(&mut separate, Value::Array(vec![first, second]));
        separate.set_root(root);

        assert!(!shared.tree_eq(&separate));
        assert!(!separate.tree_eq(&shared));
    }

    #[test]
    fn reserve_and_finish_build_a_cycle() {
        let mut doc = SaveDoc::new();
        let id = doc.reserve().unwrap();
        assert_eq!(doc.unfinished_count(), 1);
        doc.finish(id, Node::new(Value::Array(vec![id]))).unwrap();
        assert_eq!(doc.unfinished_count(), 0);
        doc.set_root(id);
        assert_eq!(doc.children(id), vec![id]);
        assert_eq!(doc.len(), 2);
    }

    #[test]
    fn reserved_ids_can_be_used_before_they_are_filled() {
        let mut doc = SaveDoc::new();
        let parent = doc.reserve().unwrap();
        let child = doc.push(Node::new(Value::Array(vec![parent]))).unwrap();
        doc.finish(parent, Node::new(Value::Array(vec![child])))
            .unwrap();
        doc.set_root(parent);
        assert_eq!(doc.children(parent), vec![child]);
        assert_eq!(doc.children(child), vec![parent]);
    }

    #[test]
    fn finish_rejects_bad_use() {
        let mut doc = SaveDoc::new();
        let pushed = scalar(&mut doc, Value::Null);
        // A node made with `push` was never reserved.
        let error = doc
            .finish(pushed, Node::new(Value::Bool(true)))
            .unwrap_err();
        assert_eq!(error.code(), codes::CORE_DOC_BUILD);
        // The root node was never reserved either.
        let root = doc.root();
        let error = doc.finish(root, Node::new(Value::Bool(true))).unwrap_err();
        assert_eq!(error.code(), codes::CORE_DOC_BUILD);
        // An id that does not exist.
        let error = doc.finish(NodeId(99), Node::new(Value::Null)).unwrap_err();
        assert_eq!(error.code(), codes::CORE_DOC_BUILD);

        // Finishing twice fails the second time and keeps the first value.
        let id = doc.reserve().unwrap();
        doc.finish(id, Node::new(Value::Bool(true))).unwrap();
        let error = doc.finish(id, Node::new(Value::Bool(false))).unwrap_err();
        assert_eq!(error.code(), codes::CORE_DOC_BUILD);
        assert_eq!(doc.get(id).unwrap().value, Value::Bool(true));
        assert_eq!(doc.unfinished_count(), 0);
    }

    #[test]
    fn unfinished_nodes_are_counted() {
        let mut doc = SaveDoc::new();
        let a = doc.reserve().unwrap();
        let _b = doc.reserve().unwrap();
        let _c = doc.reserve().unwrap();
        assert_eq!(doc.unfinished_count(), 3);
        doc.finish(a, Node::new(Value::Null)).unwrap();
        assert_eq!(doc.unfinished_count(), 2);
    }

    #[test]
    fn push_and_reserve_stop_at_the_node_limit() {
        // The root node counts: limit 3 leaves room for two more nodes.
        let mut doc = SaveDoc::with_node_limit(3);
        assert_eq!(doc.node_limit(), 3);
        doc.push(Node::new(Value::Null)).unwrap();
        let reserved = doc.reserve().unwrap();
        assert_eq!(doc.len(), 3);
        assert_eq!(
            doc.push(Node::new(Value::Null)).unwrap_err().code(),
            codes::CORE_LIMIT_NODES
        );
        assert_eq!(doc.reserve().unwrap_err().code(), codes::CORE_LIMIT_NODES);
        // A rejected push leaves no node behind, and finishing a reservation still works.
        assert_eq!(doc.len(), 3);
        doc.finish(reserved, Node::new(Value::Null)).unwrap();
    }

    #[test]
    fn new_uses_the_default_node_limit() {
        assert_eq!(SaveDoc::new().node_limit(), Limits::default().max_nodes);
        let mut tiny = SaveDoc::with_node_limit(0);
        assert_eq!(tiny.len(), 1);
        assert_eq!(
            tiny.push(Node::new(Value::Null)).unwrap_err().code(),
            codes::CORE_LIMIT_NODES
        );
    }
}
