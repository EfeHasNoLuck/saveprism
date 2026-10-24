//! Reads Marshal data into a [`SaveDoc`]. Nothing is instantiated or evaluated: every value
//! becomes a node, object links stay `Ref` nodes and are never expanded.

use std::collections::HashMap;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    DecodeCtx, Error, ErrorCode, FloatWidth, IntWidth, Node, NodeFlags, NodeId, ParamValue, Params,
    Result, SaveDoc, TypeHint, Value, codes,
};

use super::write::{
    P_FLOAT_TAILS, P_SYMBOL_ASCII, P_SYMBOL_LINKS, P_SYMBOL_REDEFS, P_SYMBOL_UTF8, RAW_PREFIX,
    float_text, hex, int_bytes,
};
use super::{
    DATA_KEY, DEFAULT_KEY, Enc, HINT_PREFIX, MAX_NESTING, RUBY_BAD_LINK, RUBY_PARSE,
    RUBY_UNSUPPORTED, VALUE_KEY, VERSION,
};

/// Reads the whole input: one dump after another until the end.
pub(super) fn read_all(input: &[u8], ctx: &DecodeCtx<'_>) -> Result<(SaveDoc, Params)> {
    let mut reader = Reader {
        input,
        pos: 0,
        doc: ctx.new_doc(),
        ctx,
        symbols: Vec::new(),
        first_symbol: HashMap::new(),
        objects: Vec::new(),
        symbol_ordinal: 0,
        float_ordinal: 0,
        redefs: Vec::new(),
        links: Params::new(),
        utf8: Vec::new(),
        ascii: Vec::new(),
        tails: Params::new(),
    };
    let mut dumps = Vec::new();
    while reader.pos < input.len() {
        let rest = input.get(reader.pos..).unwrap_or(&[]);
        if rest.starts_with(&VERSION) {
            reader.symbols.clear();
            reader.first_symbol.clear();
            reader.objects.clear();
            reader.pos += VERSION.len();
            dumps.push(reader.value(1)?);
            continue;
        }
        if dumps.is_empty() {
            return Err(match rest.first() {
                Some(4) => Error::new(
                    RUBY_UNSUPPORTED,
                    format!(
                        "Marshal version 4.{} is not supported (only 4.8)",
                        rest.get(1).copied().unwrap_or(0)
                    ),
                ),
                _ => Error::new(RUBY_PARSE, "not Ruby Marshal data (no 04 08 header)"),
            });
        }
        let trailing = rest.to_vec();
        reader.pos = input.len();
        dumps.push(reader.push(Value::Bytes(trailing), "trailing", None, true)?);
    }
    if dumps.is_empty() {
        return Err(Error::new(RUBY_PARSE, "the file is empty"));
    }
    let root = reader.push(Value::Array(dumps), "dumps", None, true)?;
    reader.doc.set_root(root);
    let params = reader.params();
    Ok((reader.doc, params))
}

/// A value whose node waits for its encoding: a String, Regexp or `_dump` data that an `I`
/// wrapper may give `:E`.
struct Leaf {
    /// The registered (reserved) id; `None` for `_dump` data, which is numbered when finished.
    id: Option<NodeId>,
    kind: LeafKind,
    bytes: Vec<u8>,
}

enum LeafKind {
    String,
    Regexp(u8),
    UserDef(String),
}

enum Built {
    Done(NodeId),
    Leaf(Leaf),
}

/// A hash key: a symbol (kept as text until the form of the hash is known) or a node.
enum Key {
    Symbol(String),
    Node(NodeId),
}

struct Reader<'i, 'c, 'l> {
    input: &'i [u8],
    pos: usize,
    doc: SaveDoc,
    ctx: &'c DecodeCtx<'l>,
    /// Symbol table of the current dump.
    symbols: Vec<String>,
    first_symbol: HashMap<String, usize>,
    /// Object table of the current dump (`@` links index it).
    objects: Vec<NodeId>,
    symbol_ordinal: i64,
    float_ordinal: i64,
    redefs: Vec<ParamValue>,
    links: Params,
    utf8: Vec<ParamValue>,
    ascii: Vec<ParamValue>,
    tails: Params,
}

fn hint(kind: &str, arg: Option<&str>) -> TypeHint {
    match arg {
        Some(arg) => TypeHint::new(format!("{HINT_PREFIX}{kind}:{arg}")),
        None => TypeHint::new(format!("{HINT_PREFIX}{kind}")),
    }
}

impl<'i> Reader<'i, '_, '_> {
    fn error(&self, code: ErrorCode, message: impl Into<String>, at: usize) -> Error {
        Error::new(code, message).with_offset(len_u64(at))
    }

    fn truncated(&self) -> Error {
        self.error(
            RUBY_PARSE,
            "the Marshal data ends too early (truncated file)",
            self.pos,
        )
    }

    fn remaining(&self) -> usize {
        self.input.len().saturating_sub(self.pos)
    }

    fn peek(&self) -> Option<u8> {
        self.input.get(self.pos).copied()
    }

    fn byte(&mut self) -> Result<u8> {
        let byte = self.peek().ok_or_else(|| self.truncated())?;
        self.pos += 1;
        Ok(byte)
    }

    fn take(&mut self, count: usize) -> Result<&'i [u8]> {
        if count > self.remaining() {
            return Err(self.truncated());
        }
        let start = self.pos;
        self.pos += count;
        let input: &'i [u8] = self.input;
        input.get(start..self.pos).ok_or_else(|| self.truncated())
    }

    /// A Marshal "long": the packed integer of Fixnums, lengths and indices.
    fn long(&mut self) -> Result<i64> {
        let head = self.byte()? as i8;
        match head {
            0 => Ok(0),
            1..=4 => {
                let count = head.unsigned_abs();
                let mut value: i64 = 0;
                for shift in 0..count {
                    value |= i64::from(self.byte()?) << (8 * u32::from(shift));
                }
                Ok(value)
            }
            -4..=-1 => {
                let count = head.unsigned_abs();
                let mut value: i64 = -1;
                for shift in 0..count {
                    let bits = 8 * u32::from(shift);
                    value &= !(0xff_i64 << bits);
                    value |= i64::from(self.byte()?) << bits;
                }
                Ok(value)
            }
            5.. => Ok(i64::from(head) - 5),
            _ => Ok(i64::from(head) + 5),
        }
    }

    /// A count of things that each take at least `unit` bytes: never more than the input holds.
    fn count(&mut self, unit: usize) -> Result<usize> {
        let at = self.pos;
        let value = self.long()?;
        let count = usize::try_from(value)
            .map_err(|_| self.error(RUBY_PARSE, format!("negative length {value}"), at))?;
        if count.saturating_mul(unit) > self.remaining() {
            return Err(self.error(
                RUBY_PARSE,
                format!("length {count} is larger than the rest of the file"),
                at,
            ));
        }
        Ok(count)
    }

    /// A byte sequence: a long length, then that many bytes.
    fn byte_seq(&mut self) -> Result<Vec<u8>> {
        let count = self.count(1)?;
        Ok(self.take(count)?.to_vec())
    }

    fn depth(&self, depth: usize) -> Result<()> {
        if depth > MAX_NESTING {
            return Err(Error::new(
                codes::CORE_LIMIT_DEPTH,
                format!(
                    "nesting depth {depth} is above the Marshal reader's limit of {MAX_NESTING}"
                ),
            )
            .with_offset(len_u64(self.pos)));
        }
        self.ctx
            .check_depth(depth)
            .map_err(|error| error.with_offset(len_u64(self.pos)))
    }

    fn node(value: Value, kind: &str, arg: Option<&str>, read_only: bool) -> Node {
        let mut node = Node::new(value);
        node.type_hint = Some(hint(kind, arg));
        if read_only {
            node.flags.insert(NodeFlags::READ_ONLY);
        }
        node
    }

    fn push(
        &mut self,
        value: Value,
        kind: &str,
        arg: Option<&str>,
        read_only: bool,
    ) -> Result<NodeId> {
        let at = self.pos;
        self.doc
            .push(Self::node(value, kind, arg, read_only))
            .map_err(|error| error.with_offset(len_u64(at)))
    }

    /// Reserves a node and enters it in the object table (the next `@` index).
    fn register(&mut self) -> Result<NodeId> {
        let at = self.pos;
        let id = self
            .doc
            .reserve()
            .map_err(|error| error.with_offset(len_u64(at)))?;
        self.objects.push(id);
        Ok(id)
    }

    fn finish(&mut self, id: NodeId, node: Node) -> Result<NodeId> {
        self.doc.finish(id, node)?;
        Ok(id)
    }

    fn lock(&mut self, id: NodeId) -> Result<()> {
        if let Some(node) = self.doc.get(id) {
            let mut flags = node.flags;
            flags.insert(NodeFlags::READ_ONLY);
            self.doc.set_flags(id, flags)?;
        }
        Ok(())
    }

    /// Marks `id` and everything written inline below it read-only (a hash key: changing any part
    /// of it changes the key, and two keys that become equal would merge when Ruby loads the file).
    /// `@` links are written as links, not inline, so their targets are not followed.
    fn lock_tree(&mut self, id: NodeId) -> Result<()> {
        let mut stack = vec![id];
        let mut budget = self.doc.len();
        while let Some(next) = stack.pop() {
            if budget == 0 {
                break;
            }
            budget -= 1;
            self.lock(next)?;
            match self.doc.get(next).map(|node| &node.value) {
                Some(Value::Array(items)) => stack.extend(items.iter().copied()),
                Some(Value::Object(entries)) => stack.extend(entries.iter().map(|(_, id)| *id)),
                Some(Value::Map(entries)) => {
                    stack.extend(entries.iter().flat_map(|(key, value)| [*key, *value]));
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn params(&mut self) -> Params {
        let mut params = Params::new();
        if !self.redefs.is_empty() {
            params.set(P_SYMBOL_REDEFS, std::mem::take(&mut self.redefs));
        }
        if !self.links.is_empty() {
            params.set(P_SYMBOL_LINKS, std::mem::take(&mut self.links));
        }
        if !self.utf8.is_empty() {
            params.set(P_SYMBOL_UTF8, std::mem::take(&mut self.utf8));
        }
        if !self.ascii.is_empty() {
            params.set(P_SYMBOL_ASCII, std::mem::take(&mut self.ascii));
        }
        if !self.tails.is_empty() {
            params.set(P_FLOAT_TAILS, std::mem::take(&mut self.tails));
        }
        params
    }

    // ----- symbols -------------------------------------------------------------------------

    fn starts_symbol(&self) -> bool {
        match self.peek() {
            Some(b':' | b';') => true,
            Some(b'I') => self.input.get(self.pos + 1) == Some(&b':'),
            _ => false,
        }
    }

    fn symbol_name(&mut self) -> Result<String> {
        let at = self.pos;
        let bytes = self.byte_seq()?;
        String::from_utf8(bytes).map_err(|_| {
            self.error(
                RUBY_UNSUPPORTED,
                "a symbol name is not UTF-8 text; this file cannot be opened yet",
                at,
            )
        })
    }

    fn define(&mut self, ordinal: i64, name: &str) {
        if self.first_symbol.contains_key(name) {
            self.redefs.push(ParamValue::Int(ordinal));
        } else {
            self.first_symbol
                .insert(name.to_string(), self.symbols.len());
        }
        self.symbols.push(name.to_string());
    }

    /// One symbol occurrence: `:` (new), `;` (link) or `I:` (new, with `:E`).
    fn symbol(&mut self, depth: usize) -> Result<String> {
        self.depth(depth)?;
        let ordinal = self.symbol_ordinal;
        self.symbol_ordinal += 1;
        let at = self.pos;
        match self.byte()? {
            b':' => {
                let name = self.symbol_name()?;
                self.define(ordinal, &name);
                Ok(name)
            }
            b';' => {
                let index = self.long()?;
                let name = usize::try_from(index)
                    .ok()
                    .and_then(|index| self.symbols.get(index))
                    .cloned()
                    .ok_or_else(|| {
                        self.error(
                            RUBY_BAD_LINK,
                            format!("symbol link {index} points to no earlier symbol"),
                            at,
                        )
                    })?;
                let first = self.first_symbol.get(&name).copied();
                if first.and_then(|first| i64::try_from(first).ok()) != Some(index) {
                    self.links.set(ordinal.to_string(), index);
                }
                Ok(name)
            }
            b'I' => {
                if self.byte()? != b':' {
                    return Err(self.error(RUBY_PARSE, "expected a symbol after 'I'", at));
                }
                let name = self.symbol_name()?;
                self.define(ordinal, &name);
                let unsupported = |reader: &Self| {
                    reader.error(
                        RUBY_UNSUPPORTED,
                        "a symbol with an encoding other than UTF-8 or US-ASCII is not supported",
                        at,
                    )
                };
                if self.long()? != 1 {
                    return Err(unsupported(self));
                }
                if self.symbol(depth + 1)? != "E" {
                    return Err(unsupported(self));
                }
                match self.byte()? {
                    b'T' => self.utf8.push(ParamValue::Int(ordinal)),
                    b'F' => self.ascii.push(ParamValue::Int(ordinal)),
                    _ => return Err(unsupported(self)),
                }
                Ok(name)
            }
            other => Err(self.error(
                RUBY_PARSE,
                format!("expected a symbol, found type byte 0x{other:02x}"),
                at,
            )),
        }
    }

    // ----- values --------------------------------------------------------------------------

    /// One complete value.
    fn value(&mut self, depth: usize) -> Result<NodeId> {
        match self.inner(depth)? {
            Built::Done(id) => Ok(id),
            Built::Leaf(leaf) => self.finish_leaf(leaf, Enc::Binary, false),
        }
    }

    /// Reads one value; a String, Regexp or `_dump` value comes back unfinished (see [`Leaf`]).
    /// Every case is its own small method: the recursion goes through here once per nesting
    /// level, so this frame stays small.
    fn inner(&mut self, depth: usize) -> Result<Built> {
        self.depth(depth)?;
        let start = self.pos;
        let tag = self.byte()?;
        let id = match tag {
            b'0' => self.push(Value::Null, "nil", None, true)?,
            b'T' | b'F' => self.push(Value::Bool(tag == b'T'), "bool", None, false)?,
            b'i' => self.fixnum(start)?,
            b'l' => self.bignum(start)?,
            b'f' => self.float(start)?,
            b'"' | b'/' | b'u' => return self.leaf(tag, depth).map(Built::Leaf),
            b':' | b';' => self.symbol_value(start, depth)?,
            b'I' if self.peek() == Some(b':') => self.symbol_value(start, depth)?,
            b'I' => self.ivars(depth)?,
            b'@' => self.link(start)?,
            b'[' => self.array(depth)?,
            b'{' | b'}' => self.hash(depth, tag == b'}')?,
            b'o' | b'S' => self.object(depth, tag == b'o')?,
            b'U' | b'd' => self.with_data(depth, tag == b'U')?,
            b'C' | b'e' => self.wrapper(depth, tag == b'C')?,
            b'c' | b'm' | b'M' => self.class_ref(tag)?,
            other => return Err(self.unknown(other, start)),
        };
        Ok(Built::Done(id))
    }

    fn unknown(&self, tag: u8, start: usize) -> Error {
        self.error(
            RUBY_PARSE,
            format!("unknown Marshal type byte 0x{tag:02x}"),
            start,
        )
    }

    fn fixnum(&mut self, start: usize) -> Result<NodeId> {
        let value = self.long()?;
        self.int(i128::from(value), start, None)
    }

    /// `"`, `/` or `u`: the bytes, kept until an `I` wrapper says which encoding they have.
    fn leaf(&mut self, tag: u8, depth: usize) -> Result<Leaf> {
        match tag {
            b'u' => {
                let class = self.symbol(depth + 1)?;
                let bytes = self.byte_seq()?;
                Ok(Leaf {
                    id: None,
                    kind: LeafKind::UserDef(class),
                    bytes,
                })
            }
            b'/' => {
                let id = self.register()?;
                let bytes = self.byte_seq()?;
                let options = self.byte()?;
                Ok(Leaf {
                    id: Some(id),
                    kind: LeafKind::Regexp(options),
                    bytes,
                })
            }
            _ => {
                let id = self.register()?;
                let bytes = self.byte_seq()?;
                Ok(Leaf {
                    id: Some(id),
                    kind: LeafKind::String,
                    bytes,
                })
            }
        }
    }

    fn symbol_value(&mut self, start: usize, depth: usize) -> Result<NodeId> {
        self.pos = start;
        let name = self.symbol(depth)?;
        self.push(Value::Str(name), "symbol", None, true)
    }

    /// `U` (marshal_dump data) or `d` (Data object): numbered before their data.
    fn with_data(&mut self, depth: usize, user_marshal: bool) -> Result<NodeId> {
        let class = self.symbol(depth + 1)?;
        let id = self.register()?;
        let data = self.value(depth + 1)?;
        let kind = if user_marshal { "usermarshal" } else { "data" };
        let node = Self::node(
            Value::Object(vec![(DATA_KEY.to_string(), data)]),
            kind,
            Some(&class),
            true,
        );
        self.finish(id, node)
    }

    /// `C` (user class) or `e` (extended): a name around a value; not objects of their own.
    fn wrapper(&mut self, depth: usize, user_class: bool) -> Result<NodeId> {
        let name = self.symbol(depth + 1)?;
        let inner = self.value(depth + 1)?;
        let kind = if user_class { "userclass" } else { "extended" };
        let value = Value::Object(vec![(VALUE_KEY.to_string(), inner)]);
        self.push(value, kind, Some(&name), true)
    }

    fn class_ref(&mut self, tag: u8) -> Result<NodeId> {
        let id = self.register()?;
        let bytes = self.byte_seq()?;
        let kind = match tag {
            b'c' => "class",
            b'm' => "module",
            _ => "oldmodule",
        };
        let node = Self::node(text_or_bytes(bytes), kind, None, true);
        self.finish(id, node)
    }

    /// An integer node. `registered` is the reserved id of a Bignum (`l` is an object).
    /// The original bytes become the `raw:` lexeme when they are not the canonical form.
    fn int(&mut self, value: i128, start: usize, registered: Option<NodeId>) -> Result<NodeId> {
        let raw = self.input.get(start..self.pos).unwrap_or(&[]);
        let mut node = Self::node(
            Value::Int {
                v: value,
                width: IntWidth::Unbounded,
            },
            "int",
            None,
            false,
        );
        if int_bytes(value) != raw {
            node.lexeme = Some(format!("{RAW_PREFIX}{}", hex(raw)).into());
        }
        match registered {
            Some(id) => self.finish(id, node),
            None => {
                let at = start;
                self.doc
                    .push(node)
                    .map_err(|error| error.with_offset(len_u64(at)))
            }
        }
    }

    fn bignum(&mut self, start: usize) -> Result<NodeId> {
        let id = self.register()?;
        let sign_at = self.pos;
        let negative = match self.byte()? {
            b'+' => false,
            b'-' => true,
            other => {
                return Err(self.error(
                    RUBY_PARSE,
                    format!("a Bignum sign must be '+' or '-', found 0x{other:02x}"),
                    sign_at,
                ));
            }
        };
        let shorts = self.count(2)?;
        let data = self.take(shorts.saturating_mul(2))?;
        let used = data
            .iter()
            .rposition(|byte| *byte != 0)
            .map_or(0, |last| last + 1);
        let magnitude = if used <= 16 {
            let mut buffer = [0_u8; 16];
            for (slot, byte) in buffer.iter_mut().zip(data.iter()) {
                *slot = *byte;
            }
            Some(u128::from_le_bytes(buffer))
        } else {
            None
        };
        let value = magnitude.and_then(|magnitude| {
            if negative {
                0_i128.checked_sub_unsigned(magnitude)
            } else {
                i128::try_from(magnitude).ok()
            }
        });
        match value {
            Some(value) => self.int(value, start, Some(id)),
            None => {
                let payload = self.input.get(start + 1..self.pos).unwrap_or(&[]).to_vec();
                let node = Self::node(Value::Bytes(payload), "bignum", None, true);
                self.finish(id, node)
            }
        }
    }

    fn float(&mut self, start: usize) -> Result<NodeId> {
        let id = self.register()?;
        let bytes = self.byte_seq()?;
        let ordinal = self.float_ordinal;
        self.float_ordinal += 1;
        let split = bytes
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(bytes.len());
        let (text, tail) = bytes.split_at(split);
        let bad = || {
            Error::new(RUBY_PARSE, "a Float holds text that is not a number")
                .with_offset(len_u64(start))
        };
        let text = std::str::from_utf8(text).map_err(|_| bad())?;
        let value = match text {
            "inf" => f64::INFINITY,
            "-inf" => f64::NEG_INFINITY,
            "nan" => f64::NAN,
            other => other.parse::<f64>().map_err(|_| bad())?,
        };
        let mut node = Self::node(
            Value::Float {
                v: value,
                width: FloatWidth::F64,
            },
            "float",
            None,
            false,
        );
        if !tail.is_empty() {
            self.tails.set(ordinal.to_string(), tail.to_vec());
        }
        if !tail.is_empty() || float_text(value) != text {
            node.lexeme = Some(text.into());
        }
        self.finish(id, node)
    }

    fn link(&mut self, start: usize) -> Result<NodeId> {
        let index = self.long()?;
        let target = usize::try_from(index)
            .ok()
            .and_then(|index| self.objects.get(index))
            .copied()
            .ok_or_else(|| {
                self.error(
                    RUBY_BAD_LINK,
                    format!("object link @{index} points to no earlier object"),
                    start,
                )
            })?;
        // A linked integer is a Bignum object; it must not become a Fixnum (which has no link).
        if matches!(
            self.doc.get(target).map(|node| &node.value),
            Some(Value::Int { .. })
        ) {
            self.lock(target)?;
        }
        self.push(Value::Ref(target), "link", None, true)
    }

    fn array(&mut self, depth: usize) -> Result<NodeId> {
        let id = self.register()?;
        let count = self.count(1)?;
        let mut items = Vec::with_capacity(count.min(4096));
        for _ in 0..count {
            items.push(self.value(depth + 1)?);
        }
        let node = Self::node(Value::Array(items), "array", None, true);
        self.finish(id, node)
    }

    fn hash(&mut self, depth: usize, with_default: bool) -> Result<NodeId> {
        let id = self.register()?;
        let count = self.count(2)?;
        let mut pairs: Vec<(Key, NodeId)> = Vec::with_capacity(count.min(4096));
        for _ in 0..count {
            let key = if self.starts_symbol() {
                Key::Symbol(self.symbol(depth + 1)?)
            } else {
                Key::Node(self.value(depth + 1)?)
            };
            let value = self.value(depth + 1)?;
            pairs.push((key, value));
        }
        let default = if with_default {
            Some(self.value(depth + 1)?)
        } else {
            None
        };
        let all_symbols =
            !pairs.is_empty() && pairs.iter().all(|(key, _)| matches!(key, Key::Symbol(_)));
        let value = if all_symbols {
            let mut entries: Vec<(String, NodeId)> = pairs
                .into_iter()
                .map(|(key, value)| match key {
                    Key::Symbol(name) => (name, value),
                    Key::Node(_) => (String::new(), value),
                })
                .collect();
            if let Some(default) = default {
                entries.push((DEFAULT_KEY.to_string(), default));
            }
            Value::Object(entries)
        } else {
            let mut entries = Vec::with_capacity(pairs.len() + 1);
            for (key, value) in pairs {
                let key = match key {
                    Key::Symbol(name) => self.push(Value::Str(name), "symbol", None, true)?,
                    Key::Node(key) => {
                        self.lock_tree(key)?;
                        key
                    }
                };
                entries.push((key, value));
            }
            if let Some(default) = default {
                let key = self.push(Value::Str(DEFAULT_KEY.to_string()), "default", None, true)?;
                entries.push((key, default));
            }
            Value::Map(entries)
        };
        let kind = if with_default { "hash+default" } else { "hash" };
        let mut node = Self::node(value, kind, None, true);
        if !with_default && self.int_keyed(&node.value) {
            node.flags.insert(NodeFlags::GROWABLE);
        }
        self.finish(id, node)
    }

    /// True for a `Map` whose keys are all integers written inline (an empty hash too): such a
    /// hash may take new `Int => Int` entries (see `insert_int_entry`).
    fn int_keyed(&self, value: &Value) -> bool {
        let Value::Map(entries) = value else {
            return false;
        };
        entries.iter().all(|(key, _)| {
            self.doc.get(*key).is_some_and(|node| {
                matches!(node.value, Value::Int { .. })
                    && node.type_hint.as_ref().map(TypeHint::as_str) == Some("RUBY:int")
            })
        })
    }

    fn object(&mut self, depth: usize, plain: bool) -> Result<NodeId> {
        let class = self.symbol(depth + 1)?;
        let id = self.register()?;
        let count = self.count(2)?;
        let mut entries = Vec::with_capacity(count.min(4096));
        for _ in 0..count {
            let name = self.symbol(depth + 1)?;
            let value = self.value(depth + 1)?;
            entries.push((name, value));
        }
        let kind = if plain { "object" } else { "struct" };
        let node = Self::node(Value::Object(entries), kind, Some(&class), true);
        self.finish(id, node)
    }

    /// `I`: a value followed by instance variables. A String, Regexp or `_dump` value with only
    /// `:E => true/false` keeps its encoding in its hint; anything else becomes an `ivars` node.
    fn ivars(&mut self, depth: usize) -> Result<NodeId> {
        let inner = self.inner(depth + 1)?;
        let count = self.count(2)?;
        let mut entries: Vec<(String, NodeId)> = Vec::with_capacity(count.min(4096) + 1);
        let mut pending = Some(inner);
        for index in 0..count {
            let name = self.symbol(depth + 1)?;
            let flag = match self.peek() {
                Some(b'T') => Some(true),
                Some(b'F') => Some(false),
                _ => None,
            };
            let foldable = count == 1
                && index == 0
                && name == "E"
                && flag.is_some()
                && matches!(pending, Some(Built::Leaf(_)));
            if foldable && let (Some(flag), Some(Built::Leaf(leaf))) = (flag, pending.take()) {
                self.pos += 1;
                let enc = if flag { Enc::Utf8 } else { Enc::Ascii };
                return self.finish_leaf(leaf, enc, false);
            }
            let value = self.value(depth + 1)?;
            if name == "E" || name == "encoding" {
                self.lock(value)?;
            }
            entries.push((name, value));
        }
        let inner_id = match pending {
            Some(Built::Leaf(leaf)) => self.finish_leaf(leaf, Enc::Binary, true)?,
            Some(Built::Done(id)) => {
                self.lock_innermost(id)?;
                id
            }
            None => {
                return Err(self.error(
                    RUBY_PARSE,
                    "instance variables lost their value",
                    self.pos,
                ));
            }
        };
        entries.insert(0, (VALUE_KEY.to_string(), inner_id));
        self.push(Value::Object(entries), "ivars", None, true)
    }

    /// Marks the value inside `userclass` / `extended` wrappers read-only: its encoding is not
    /// one this codec knows, so its text must not be changed.
    fn lock_innermost(&mut self, mut id: NodeId) -> Result<()> {
        // Wrappers nest at most `MAX_NESTING` deep, so this walk always reaches the innermost value.
        for _ in 0..=MAX_NESTING {
            let next = match self.doc.get(id) {
                Some(node) => {
                    let wrapper = node.type_hint.as_ref().is_some_and(|hint| {
                        hint.as_str().starts_with("RUBY:userclass:")
                            || hint.as_str().starts_with("RUBY:extended:")
                    });
                    match &node.value {
                        Value::Object(entries) if wrapper => entries.first().map(|(_, id)| *id),
                        _ => None,
                    }
                }
                None => None,
            };
            match next {
                Some(next) => id = next,
                None => break,
            }
        }
        self.lock(id)
    }

    /// Turns a leaf into its node. `locked` makes a string read-only (unknown encoding).
    fn finish_leaf(&mut self, leaf: Leaf, enc: Enc, locked: bool) -> Result<NodeId> {
        let id = match leaf.id {
            Some(id) => id,
            None => self.register()?,
        };
        let suffix = enc.suffix();
        let node = match leaf.kind {
            LeafKind::String => {
                let ascii_ok = enc != Enc::Ascii || leaf.bytes.is_ascii();
                let value = text_or_bytes(leaf.bytes);
                let editable = !locked && ascii_ok && matches!(value, Value::Str(_));
                Self::node(value, &format!("string{suffix}"), None, !editable)
            }
            LeafKind::Regexp(options) => Self::node(
                text_or_bytes(leaf.bytes),
                &format!("regexp{suffix}"),
                Some(&options.to_string()),
                true,
            ),
            LeafKind::UserDef(class) => Self::node(
                Value::Bytes(leaf.bytes),
                &format!("userdef{suffix}"),
                Some(&class),
                true,
            ),
        };
        self.finish(id, node)
    }
}

/// UTF-8 bytes as text, anything else as bytes.
fn text_or_bytes(bytes: Vec<u8>) -> Value {
    match String::from_utf8(bytes) {
        Ok(text) => Value::Str(text),
        Err(error) => Value::Bytes(error.into_bytes()),
    }
}
