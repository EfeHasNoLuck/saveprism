//! The Bitser reader (see `mod.rs` for the tree and the rules).

use saveprism_core::limits::len_u64;
use saveprism_core::{
    ByteSpan, Error, FloatWidth, IntWidth, Limits, Node, NodeFlags, NodeId, Params, Result,
    SaveDoc, TypeHint, Value,
};

use super::{BITSER_PARSE, HINT_PREFIX, ORIGINAL};

fn parse_error(message: impl Into<String>, at: usize) -> Error {
    Error::new(BITSER_PARSE, message).with_offset(len_u64(at))
}

fn hint(name: &str) -> TypeHint {
    TypeHint::new(format!("{HINT_PREFIX}{name}"))
}

/// Decodes a whole Bitser stream.
pub(super) fn decode(input: &[u8], limits: &Limits) -> Result<(SaveDoc, Params)> {
    if input.is_empty() {
        return Err(parse_error("the Bitser data is empty", 0));
    }
    let mut reader = Reader {
        input,
        pos: 0,
        seen: Vec::new(),
        doc: SaveDoc::with_node_limit(limits.max_nodes),
        max_depth: limits.max_depth,
    };
    let root = reader.value(0, false)?;
    if reader.pos != input.len() {
        return Err(parse_error(
            "the Bitser data continues after the first value",
            reader.pos,
        ));
    }
    reader.doc.set_root(root);
    let mut params = Params::new();
    params.set(ORIGINAL, input.to_vec());
    Ok((reader.doc, params))
}

/// True when a complete stream whose root is a table or class instance was parsed.
pub(super) fn plausible(input: &[u8], limits: &Limits) -> bool {
    match decode(input, limits) {
        Ok((doc, _)) => doc
            .get(doc.root())
            .and_then(|node| node.type_hint.as_ref())
            .is_some_and(|hint| {
                hint.as_str() == "BITSER:table" || hint.as_str() == "BITSER:instance"
            }),
        Err(_) => false,
    }
}

struct Reader<'a> {
    input: &'a [u8],
    pos: usize,
    seen: Vec<Option<NodeId>>,
    doc: SaveDoc,
    max_depth: usize,
}

impl Reader<'_> {
    fn take(&mut self, count: usize, what: &str) -> Result<&[u8]> {
        let start = self.pos;
        let end = start
            .checked_add(count)
            .filter(|end| *end <= self.input.len())
            .ok_or_else(|| parse_error(format!("the {what} is cut short"), start))?;
        let bytes = self
            .input
            .get(start..end)
            .ok_or_else(|| parse_error(format!("the {what} is cut short"), start))?;
        self.pos = end;
        Ok(bytes)
    }

    fn byte(&mut self) -> Result<u8> {
        self.take(1, "format byte")
            .map(|bytes| bytes.first().copied().unwrap_or(0))
    }

    fn push(
        &mut self,
        value: Value,
        hint_name: &str,
        start: usize,
        read_only: bool,
    ) -> Result<NodeId> {
        let mut node = Node::new(value);
        node.type_hint = Some(hint(hint_name));
        node.span = Some(ByteSpan {
            start: len_u64(start),
            end: len_u64(self.pos),
        });
        if read_only {
            node.flags.insert(NodeFlags::READ_ONLY);
        }
        self.doc.push(node)
    }

    fn add_to_seen(&mut self, id: NodeId) -> NodeId {
        self.seen.push(Some(id));
        id
    }

    fn seen_value(&self, index: usize, at: usize) -> Result<NodeId> {
        self.seen
            .get(index)
            .copied()
            .flatten()
            .ok_or_else(|| parse_error("a Bitser reference points at no value", at))
    }

    fn length(&self, id: NodeId, at: usize, what: &str) -> Result<usize> {
        match self.doc.get(id).map(|node| &node.value) {
            Some(Value::Int { v, .. }) => usize::try_from(*v)
                .map_err(|_| parse_error(format!("the {what} is not a length"), at)),
            _ => Err(parse_error(format!("the {what} is not a number"), at)),
        }
    }

    fn value(&mut self, depth: usize, read_only: bool) -> Result<NodeId> {
        if depth > self.max_depth {
            return Err(Error::new(
                saveprism_core::codes::CORE_LIMIT_DEPTH,
                format!("Bitser nesting is deeper than {} levels", self.max_depth),
            ));
        }
        let start = self.pos;
        let code = self.byte()?;
        match code {
            0..=127 => self.push(
                Value::Int {
                    v: i128::from(i16::from(code) - 27),
                    width: IntWidth::I16,
                },
                "int",
                start,
                read_only,
            ),
            128..=191 => {
                let target = self.seen_value(usize::from(code) - 128, start)?;
                self.push(Value::Ref(target), "ref", start, true)
            }
            192..=223 => {
                let length = usize::from(code - 192);
                self.string(length, start, read_only)
            }
            224..=239 => {
                let length = usize::from(code - 224);
                let name = self.text(length, start)?;
                let node = self.push(Value::Str(name), "resource", start, true)?;
                Ok(self.add_to_seen(node))
            }
            240 => self.table(depth, start, false, false),
            241 => self.long_resource(depth, start),
            242 => self.table(depth, start, false, true),
            243 => {
                let index = self.value(depth + 1, false)?;
                let index = self.length(index, start, "reference index")?;
                let target = self.seen_value(index, start)?;
                self.push(Value::Ref(target), "ref", start, true)
            }
            244 => {
                let length_id = self.value(depth + 1, false)?;
                let length = self.length(length_id, start, "string length")?;
                self.string(length, start, read_only)
            }
            245 => {
                let bytes = self.take(4, "int32")?;
                let value = i32::from_le_bytes(
                    bytes
                        .try_into()
                        .map_err(|_| parse_error("the int32 is cut short", start))?,
                );
                self.push(
                    Value::Int {
                        v: i128::from(value),
                        width: IntWidth::I32,
                    },
                    "int",
                    start,
                    read_only,
                )
            }
            246 => {
                let bytes = self.take(8, "double")?;
                let value = f64::from_le_bytes(
                    bytes
                        .try_into()
                        .map_err(|_| parse_error("the double is cut short", start))?,
                );
                self.push(
                    Value::Float {
                        v: value,
                        width: FloatWidth::F64,
                    },
                    "float",
                    start,
                    read_only,
                )
            }
            247 => self.push(Value::Null, "nil", start, true),
            248 => self.push(Value::Bool(false), "bool", start, read_only),
            249 => self.push(Value::Bool(true), "bool", start, read_only),
            250 => {
                let bytes = self.take(2, "int16")?;
                let value = i16::from_le_bytes(
                    bytes
                        .try_into()
                        .map_err(|_| parse_error("the int16 is cut short", start))?,
                );
                self.push(
                    Value::Int {
                        v: i128::from(value),
                        width: IntWidth::I16,
                    },
                    "int",
                    start,
                    read_only,
                )
            }
            251 | 252 => Err(parse_error(
                format!("unsupported Bitser type byte {code}"),
                start,
            )),
            253 => self.table(depth, start, true, false),
            254 => self.extension(depth, start),
            255 => Err(parse_error(
                "unsupported serialized type 255 (additional types)",
                start,
            )),
        }
    }

    fn text(&mut self, length: usize, start: usize) -> Result<String> {
        let bytes = self.take(length, "string")?;
        std::str::from_utf8(bytes)
            .map(str::to_owned)
            .map_err(|_| parse_error("the resource name is not UTF-8", start))
    }

    fn string(&mut self, length: usize, start: usize, read_only: bool) -> Result<NodeId> {
        let bytes = self.take(length, "string")?.to_vec();
        let node = match String::from_utf8(bytes) {
            Ok(text) => self.push(Value::Str(text), "string", start, read_only)?,
            Err(error) => self.push(
                Value::Bytes(error.into_bytes()),
                "string-bytes",
                start,
                true,
            )?,
        };
        Ok(self.add_to_seen(node))
    }

    fn long_resource(&mut self, depth: usize, start: usize) -> Result<NodeId> {
        let slot = self.seen.len();
        self.seen.push(None);
        let name_id = self.value(depth + 1, true)?;
        let name = match self.doc.get(name_id).map(|node| &node.value) {
            Some(Value::Str(text)) => text.clone(),
            _ => return Err(parse_error("the resource name is not text", start)),
        };
        let resource = self.push(Value::Str(name), "resource", start, true)?;
        if let Some(entry) = self.seen.get_mut(slot) {
            *entry = Some(resource);
        }
        Ok(resource)
    }

    fn extension(&mut self, depth: usize, start: usize) -> Result<NodeId> {
        let id = self.value(depth + 1, true)?;
        let value = self.value(depth + 1, true)?;
        let object = Value::Object(vec![("id".to_owned(), id), ("value".to_owned(), value)]);
        self.push(object, "extension", start, true)
    }

    fn table(
        &mut self,
        depth: usize,
        start: usize,
        with_metatable: bool,
        instance: bool,
    ) -> Result<NodeId> {
        let table = self.doc.reserve()?;
        self.seen.push(Some(table));
        let class = if instance {
            Some(self.value(depth + 1, true)?)
        } else {
            None
        };
        let length_id = self.value(depth + 1, false)?;
        let length = self.length(length_id, start, "table array length")?;
        let mut items = Vec::with_capacity(length.min(1024));
        for _ in 0..length {
            items.push(self.value(depth + 1, false)?);
        }
        let key_length_id = self.value(depth + 1, false)?;
        let key_length = self.length(key_length_id, start, "table key count")?;
        let mut entries = Vec::with_capacity(key_length.min(1024));
        for _ in 0..key_length {
            let key = self.value(depth + 1, true)?;
            let value = self.value(depth + 1, false)?;
            entries.push((key, value));
        }
        let metatable = if with_metatable {
            Some(self.value(depth + 1, true)?)
        } else {
            None
        };
        let array = self.push(Value::Array(items), "array", start, true)?;
        let map = self.push(Value::Map(entries), "map", start, true)?;
        let mut object = vec![("array".to_owned(), array), ("map".to_owned(), map)];
        if let Some(class) = class {
            object.push(("class".to_owned(), class));
        }
        if let Some(metatable) = metatable {
            object.push(("metatable".to_owned(), metatable));
        }
        let mut node = Node::new(Value::Object(object));
        node.type_hint = Some(hint(if instance { "instance" } else { "table" }));
        node.span = Some(ByteSpan {
            start: len_u64(start),
            end: len_u64(self.pos),
        });
        node.flags.insert(NodeFlags::READ_ONLY);
        self.doc.finish(table, node)?;
        Ok(table)
    }
}
