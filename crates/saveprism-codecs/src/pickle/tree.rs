//! Turns the object graph into the document tree (see the module documentation for the shape).

use saveprism_core::{
    ByteSpan, DecodeCtx, Error, FloatWidth, IntWidth, Node, NodeFlags, NodeId, Result, SaveDoc,
    TypeHint, Value, codes,
};

use super::vm::{BytesKind, Graph, How, Instance, Obj, ObjId, TextKind};
use super::{
    ARGS_KEY, CALLABLE_KEY, ENTRIES_KEY, HINT_PREFIX, ID_KEY, ITEMS_KEY, KWARGS_KEY, MAX_NESTING,
    MEMBERS_KEY, STATE_KEY,
};

/// Text of a big integer that is too long to convert.
const BIG_INT_TEXT: &str = "(integer too large to show)";

/// Calls `visit` for every object `obj` refers to.
fn for_each_edge(obj: &Obj, mut visit: impl FnMut(ObjId)) {
    match obj {
        Obj::Tuple(items) | Obj::List(items) | Obj::Set(items) | Obj::FrozenSet(items) => {
            items.iter().copied().for_each(visit);
        }
        Obj::Dict(entries) => {
            for (key, value) in entries {
                visit(*key);
                visit(*value);
            }
        }
        Obj::PersId(inner) => visit(*inner),
        Obj::Instance(instance) => {
            visit(instance.class);
            instance.args.into_iter().for_each(&mut visit);
            instance.kwargs.into_iter().for_each(&mut visit);
            instance.states.iter().copied().for_each(&mut visit);
            instance.items.iter().copied().for_each(&mut visit);
            for (key, value) in &instance.entries {
                visit(*key);
                visit(*value);
            }
            instance.members.iter().copied().for_each(visit);
        }
        _ => {}
    }
}

/// How many times each object reachable from the root is referred to (the root counts once).
fn reference_counts(graph: &Graph) -> Vec<u32> {
    let len = graph.objects.len();
    let mut counts = vec![0_u32; len];
    let mut visited = vec![false; len];
    if let Some(slot) = counts.get_mut(graph.root as usize) {
        *slot = 1;
    }
    let mut stack = vec![graph.root];
    while let Some(id) = stack.pop() {
        match visited.get_mut(id as usize) {
            Some(seen) if !*seen => *seen = true,
            _ => continue,
        }
        let Some(obj) = graph.get(id) else {
            continue;
        };
        for_each_edge(obj, |child| {
            if let Some(count) = counts.get_mut(child as usize) {
                *count = count.saturating_add(1);
            }
            if visited.get(child as usize) == Some(&false) {
                stack.push(child);
            }
        });
    }
    counts
}

fn hint(kind: &str, class: Option<&str>) -> TypeHint {
    match class {
        Some(class) => TypeHint::new(format!("{HINT_PREFIX}{kind}:{class}")),
        None => TypeHint::new(format!("{HINT_PREFIX}{kind}")),
    }
}

fn make_node(value: Value, hint: TypeHint, read_only: bool, span: Option<ByteSpan>) -> Node {
    let mut node = Node::new(value);
    node.type_hint = Some(hint);
    node.span = span;
    if read_only {
        node.flags.insert(NodeFlags::READ_ONLY);
    }
    node
}

struct Builder<'g, 'c, 'l> {
    graph: &'g Graph,
    ctx: &'c DecodeCtx<'l>,
    doc: SaveDoc,
    /// The node of the first place each object was written to the tree.
    nodes: Vec<Option<NodeId>>,
    refs: Vec<u32>,
}

impl<'g> Builder<'g, '_, '_> {
    fn shared(&self, id: ObjId) -> bool {
        self.refs.get(id as usize).copied().unwrap_or(0) > 1
    }

    fn remember(&mut self, id: ObjId, node: NodeId) {
        if let Some(slot) = self.nodes.get_mut(id as usize) {
            *slot = Some(node);
        }
    }

    fn known(&self, id: ObjId) -> Option<NodeId> {
        self.nodes.get(id as usize).copied().flatten()
    }

    fn check_depth(&self, depth: usize) -> Result<()> {
        if depth > MAX_NESTING {
            return Err(Error::new(
                codes::CORE_LIMIT_DEPTH,
                format!("nesting deeper than {MAX_NESTING} levels"),
            ));
        }
        self.ctx.check_depth(depth)
    }

    fn scalar(
        &mut self,
        id: ObjId,
        value: Value,
        hint: TypeHint,
        read_only: bool,
        span: Option<ByteSpan>,
    ) -> Result<NodeId> {
        let node = self.doc.push(make_node(value, hint, read_only, span))?;
        self.remember(id, node);
        Ok(node)
    }

    /// The text of a `Global`, `None` for anything else.
    fn global_name(&self, id: ObjId) -> Option<String> {
        match self.graph.get(id)? {
            Obj::Global { module, name } => Some(format!("{module}.{name}")),
            _ => None,
        }
    }

    /// True for a dict whose keys are all exact texts (written as an `Object`).
    fn text_keys(&self, entries: &[(ObjId, ObjId)]) -> bool {
        entries
            .iter()
            .all(|(key, _)| matches!(self.graph.get(*key), Some(Obj::Str { editable: true, .. })))
    }

    fn key_text(&self, key: ObjId) -> String {
        match self.graph.get(key) {
            Some(Obj::Str { text, .. }) => text.clone(),
            _ => String::new(),
        }
    }

    /// The value of a dict-like node: an `Object` when every key is text, else a `Map` whose key
    /// nodes are read-only.
    fn dict_value(
        &mut self,
        entries: &[(ObjId, ObjId)],
        read_only: bool,
        depth: usize,
    ) -> Result<Value> {
        if self.text_keys(entries) {
            let mut out = Vec::with_capacity(entries.len());
            for (key, value) in entries {
                let text = self.key_text(*key);
                let child = self.build(*value, read_only, depth + 1)?;
                out.push((text, child));
            }
            Ok(Value::Object(out))
        } else {
            let mut out = Vec::with_capacity(entries.len());
            for (key, value) in entries {
                let key_node = self.build(*key, true, depth + 1)?;
                let child = self.build(*value, read_only, depth + 1)?;
                out.push((key_node, child));
            }
            Ok(Value::Map(out))
        }
    }

    fn list_value(&mut self, items: &[ObjId], read_only: bool, depth: usize) -> Result<Value> {
        let mut out = Vec::with_capacity(items.len());
        for item in items {
            out.push(self.build(*item, read_only, depth + 1)?);
        }
        Ok(Value::Array(out))
    }

    /// A container that is not an object of the graph (the `(items)` of an instance).
    fn synthetic(&mut self, value: Value, hint: TypeHint, read_only: bool) -> Result<NodeId> {
        self.doc.push(make_node(value, hint, read_only, None))
    }

    /// Writes object `id` (or a `Ref` to where it was written before). `read_only` marks a part
    /// that must not change (constructor arguments, dict keys, set members).
    fn build(&mut self, id: ObjId, read_only: bool, depth: usize) -> Result<NodeId> {
        if let Some(existing) = self.known(id) {
            return self.doc.push(make_node(
                Value::Ref(existing),
                hint("ref", None),
                true,
                None,
            ));
        }
        self.check_depth(depth)?;
        let locked = read_only || self.shared(id);
        let graph = self.graph;
        let obj = graph.get(id).ok_or_else(|| {
            Error::new(
                codes::CORE_DOC_BUILD,
                format!("object {id} is missing from the graph"),
            )
        })?;
        let int = |v: i128| Value::Int {
            v,
            width: IntWidth::Unbounded,
        };
        match obj {
            Obj::None => self.scalar(id, Value::Null, hint("none", None), true, None),
            Obj::Bool(value, span) => self.scalar(
                id,
                Value::Bool(*value),
                hint("bool", None),
                locked,
                Some(*span),
            ),
            Obj::Int(value, span) => {
                self.scalar(id, int(*value), hint("int", None), locked, Some(*span))
            }
            Obj::BigInt(text) => self.scalar(
                id,
                Value::Str(text.clone().unwrap_or_else(|| BIG_INT_TEXT.to_string())),
                hint("bigint", None),
                true,
                None,
            ),
            Obj::Float(value, span) => self.scalar(
                id,
                Value::Float {
                    v: *value,
                    width: FloatWidth::F64,
                },
                hint("float", None),
                locked,
                Some(*span),
            ),
            Obj::Str {
                text,
                kind,
                span,
                editable,
            } => {
                let name = match kind {
                    TextKind::Unicode => "str",
                    TextKind::ByteStr => "bytestr",
                };
                self.scalar(
                    id,
                    Value::Str(text.clone()),
                    hint(name, None),
                    locked || !editable,
                    Some(*span),
                )
            }
            Obj::Bytes(data, kind) => {
                let name = match kind {
                    BytesKind::Bytes => "bytes",
                    BytesKind::ByteArray => "bytearray",
                    BytesKind::ByteStr => "bytestr",
                };
                self.scalar(id, Value::Bytes(data.clone()), hint(name, None), true, None)
            }
            Obj::Global { module, name } => self.scalar(
                id,
                Value::Str(format!("{module}.{name}")),
                hint("global", None),
                true,
                None,
            ),
            Obj::Ext(code) => {
                self.scalar(id, int(i128::from(*code)), hint("ext", None), true, None)
            }
            Obj::PersIdText(text) => self.scalar(
                id,
                Value::Str(text.clone()),
                hint("persid", None),
                true,
                None,
            ),
            Obj::Tuple(_)
            | Obj::List(_)
            | Obj::Dict(_)
            | Obj::Set(_)
            | Obj::FrozenSet(_)
            | Obj::Instance(_)
            | Obj::PersId(_) => self.container(id, obj, read_only, depth),
        }
    }

    fn container(
        &mut self,
        id: ObjId,
        obj: &'g Obj,
        read_only: bool,
        depth: usize,
    ) -> Result<NodeId> {
        let node = self.doc.reserve()?;
        self.remember(id, node);
        let (value, hint, flag) = match obj {
            Obj::Tuple(items) => (
                self.list_value(items, read_only, depth)?,
                hint("tuple", None),
                read_only,
            ),
            Obj::List(items) => (
                self.list_value(items, read_only, depth)?,
                hint("list", None),
                read_only,
            ),
            Obj::Set(items) => (
                self.list_value(items, true, depth)?,
                hint("set", None),
                true,
            ),
            Obj::FrozenSet(items) => (
                self.list_value(items, true, depth)?,
                hint("frozenset", None),
                true,
            ),
            Obj::Dict(entries) => (
                self.dict_value(entries, read_only, depth)?,
                hint("dict", None),
                read_only,
            ),
            Obj::PersId(inner) => {
                let child = self.build(*inner, true, depth + 1)?;
                (
                    Value::Object(vec![(ID_KEY.to_string(), child)]),
                    hint("persid", None),
                    true,
                )
            }
            Obj::Instance(instance) => {
                let (value, hint) = self.instance(instance, read_only, depth)?;
                (value, hint, read_only)
            }
            _ => (Value::Null, hint("none", None), true),
        };
        self.doc.finish(node, make_node(value, hint, flag, None))?;
        Ok(node)
    }

    /// The class shown for an instance, and whether its arguments are bookkeeping only (the
    /// `copyreg._reconstructor` arguments of protocol 0 and 1 objects, or an empty tuple).
    fn class_of(&self, instance: &Instance) -> (Option<String>, bool) {
        let args = instance.args.and_then(|args| self.graph.get(args));
        if instance.how == How::Reduce
            && let Some(Obj::Global { module, name }) = self.graph.get(instance.class)
            && name == "_reconstructor"
            && (module == "copy_reg" || module == "copyreg")
            && let Some(Obj::Tuple(items)) = args
            && let Some(class) = items.first().and_then(|first| self.global_name(*first))
        {
            return (Some(class), true);
        }
        let trivial = match args {
            None => true,
            Some(Obj::Tuple(items)) => items.is_empty(),
            _ => false,
        };
        (self.global_name(instance.class), trivial)
    }

    /// True for a state that carries nothing (`None` or an empty dict).
    fn trivial_state(&self, id: ObjId) -> bool {
        match self.graph.get(id) {
            Some(Obj::None) => true,
            Some(Obj::Dict(entries)) => entries.is_empty(),
            _ => false,
        }
    }

    /// The attribute dict of a single state, when it can be shown inline.
    fn inline_state(&self, state: ObjId) -> Option<&'g [(ObjId, ObjId)]> {
        if self.shared(state) || self.known(state).is_some() {
            return None;
        }
        let graph = self.graph;
        match graph.get(state)? {
            Obj::Dict(entries) if self.text_keys(entries) => Some(entries),
            _ => None,
        }
    }

    /// The `({member: True, ...},)` state of a Ren'Py `RevertableSet`.
    fn set_state(&self, class: &str, state: ObjId) -> Option<&'g [(ObjId, ObjId)]> {
        if !class.ends_with("RevertableSet") || self.shared(state) {
            return None;
        }
        let graph = self.graph;
        let Obj::Tuple(items) = graph.get(state)? else {
            return None;
        };
        let [inner] = items.as_slice() else {
            return None;
        };
        if self.shared(*inner) || self.known(*inner).is_some() {
            return None;
        }
        match graph.get(*inner)? {
            Obj::Dict(entries) => Some(entries),
            _ => None,
        }
    }

    fn instance(
        &mut self,
        instance: &'g Instance,
        read_only: bool,
        depth: usize,
    ) -> Result<(Value, TypeHint)> {
        let (class, args_trivial) = self.class_of(instance);
        let callable = class.is_none();
        let class_text = class.unwrap_or_else(|| "?".to_string());
        let states: Vec<ObjId> = instance
            .states
            .iter()
            .copied()
            .filter(|state| !self.trivial_state(*state))
            .collect();
        let extras = callable || !args_trivial || instance.kwargs.is_some();
        let has_items = !instance.items.is_empty();
        let has_entries = !instance.entries.is_empty();
        let has_members = !instance.members.is_empty();
        let class = Some(class_text.as_str());

        if !extras {
            match (states.as_slice(), has_items, has_entries, has_members) {
                ([], true, false, false) => {
                    let value = self.list_value(&instance.items, read_only, depth)?;
                    return Ok((value, hint("list", class)));
                }
                ([], false, true, false) => {
                    let value = self.dict_value(&instance.entries, read_only, depth)?;
                    return Ok((value, hint("dict", class)));
                }
                ([], false, false, true) => {
                    let value = self.list_value(&instance.members, true, depth)?;
                    return Ok((value, hint("set", class)));
                }
                ([], false, false, false) => {
                    return Ok((Value::Object(Vec::new()), hint("object", class)));
                }
                ([state], false, false, false) => {
                    if let Some(entries) = self.inline_state(*state) {
                        let value = self.dict_value(entries, read_only, depth)?;
                        return Ok((value, hint("object", class)));
                    }
                    if let Some(entries) = self.set_state(&class_text, *state) {
                        let keys: Vec<ObjId> = entries.iter().map(|(key, _)| *key).collect();
                        let value = self.list_value(&keys, true, depth)?;
                        return Ok((value, hint("set", class)));
                    }
                }
                _ => {}
            }
        }

        // Anything else: every part under its own key.
        let mut out: Vec<(String, NodeId)> = Vec::new();
        if callable {
            out.push((
                CALLABLE_KEY.to_string(),
                self.build(instance.class, true, depth + 1)?,
            ));
        }
        if !args_trivial && let Some(args) = instance.args {
            out.push((ARGS_KEY.to_string(), self.build(args, true, depth + 1)?));
        }
        if let Some(kwargs) = instance.kwargs {
            out.push((KWARGS_KEY.to_string(), self.build(kwargs, true, depth + 1)?));
        }
        for (index, state) in states.iter().enumerate() {
            let key = if index == 0 {
                STATE_KEY.to_string()
            } else {
                format!("{STATE_KEY} {}", index + 1)
            };
            out.push((key, self.build(*state, read_only, depth + 1)?));
        }
        if has_items {
            self.check_depth(depth + 1)?;
            let value = self.list_value(&instance.items, read_only, depth + 1)?;
            let node = self.synthetic(value, hint("list", None), read_only)?;
            out.push((ITEMS_KEY.to_string(), node));
        }
        if has_entries {
            self.check_depth(depth + 1)?;
            let value = self.dict_value(&instance.entries, read_only, depth + 1)?;
            let node = self.synthetic(value, hint("dict", None), read_only)?;
            out.push((ENTRIES_KEY.to_string(), node));
        }
        if has_members {
            self.check_depth(depth + 1)?;
            let value = self.list_value(&instance.members, true, depth + 1)?;
            let node = self.synthetic(value, hint("set", None), true)?;
            out.push((MEMBERS_KEY.to_string(), node));
        }
        Ok((Value::Object(out), hint("instance", class)))
    }
}

/// Builds the document of `graph`; the root is the unpickled top-level object.
pub(crate) fn build(graph: &Graph, ctx: &DecodeCtx<'_>) -> Result<SaveDoc> {
    let mut builder = Builder {
        graph,
        ctx,
        doc: ctx.new_doc(),
        nodes: vec![None; graph.objects.len()],
        refs: reference_counts(graph),
    };
    let root = builder.build(graph.root, false, 0)?;
    let mut doc = builder.doc;
    doc.set_root(root);
    Ok(doc)
}
