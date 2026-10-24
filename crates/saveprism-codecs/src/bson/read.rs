//! The BSON reader (see the module documentation for the tree).

use saveprism_core::limits::len_u64;
use saveprism_core::{
    Error, FloatWidth, IntWidth, Limits, Node, NodeFlags, NodeId, Params, Result, SaveDoc,
    TypeHint, Value, codes,
};

use super::{BSON_PARSE, HINT_PREFIX};

fn parse_error(message: impl Into<String>, at: usize) -> Error {
    Error::new(BSON_PARSE, message).with_offset(len_u64(at))
}

fn hint(kind: &str) -> TypeHint {
    TypeHint::new(format!("{HINT_PREFIX}{kind}"))
}

pub(super) fn decode(input: &[u8], limits: &Limits) -> Result<(SaveDoc, Params)> {
    let mut parser = Parser {
        input,
        pos: 0,
        limits,
        doc: SaveDoc::with_node_limit(limits.max_nodes),
    };
    let root = parser.document(0, false)?;
    if parser.pos != input.len() {
        return Err(parse_error(
            "the BSON data continues after the first document",
            parser.pos,
        ));
    }
    parser.doc.set_root(root);
    Ok((parser.doc, Params::new()))
}

/// True when the declared length is the whole input and the document parses completely.
pub(super) fn plausible(input: &[u8], limits: &Limits) -> bool {
    match decode(input, limits) {
        Ok((doc, _)) => matches!(
            doc.get(doc.root()).and_then(|node| node.type_hint.as_ref()),
            Some(hint) if hint.as_str() == "BSON:document"
        ),
        Err(_) => false,
    }
}

struct Parser<'a> {
    input: &'a [u8],
    pos: usize,
    limits: &'a Limits,
    doc: SaveDoc,
}

impl Parser<'_> {
    fn take(&mut self, count: usize, what: &str) -> Result<&[u8]> {
        let start = self.pos;
        let end = start
            .checked_add(count)
            .ok_or_else(|| parse_error(format!("the {what} is too long"), start))?;
        let slice = self
            .input
            .get(start..end)
            .ok_or_else(|| parse_error(format!("the {what} is cut short"), start))?;
        self.pos = end;
        Ok(slice)
    }

    fn byte(&mut self, what: &str) -> Result<u8> {
        let bytes = self.take(1, what)?;
        bytes
            .first()
            .copied()
            .ok_or_else(|| parse_error(format!("the {what} is cut short"), self.pos))
    }

    fn le_i32(&mut self, what: &str) -> Result<i32> {
        let bytes = self.take(4, what)?;
        let array: [u8; 4] = bytes
            .try_into()
            .map_err(|_| parse_error(format!("the {what} is cut short"), self.pos))?;
        Ok(i32::from_le_bytes(array))
    }

    fn le_u64(&mut self, what: &str) -> Result<u64> {
        let bytes = self.take(8, what)?;
        let array: [u8; 8] = bytes
            .try_into()
            .map_err(|_| parse_error(format!("the {what} is cut short"), self.pos))?;
        Ok(u64::from_le_bytes(array))
    }

    fn le_i64(&mut self, what: &str) -> Result<i64> {
        self.le_u64(what).map(|value| value as i64)
    }

    fn check_depth(&self, depth: usize) -> Result<()> {
        if depth > self.limits.max_depth {
            return Err(Error::new(
                codes::CORE_LIMIT_DEPTH,
                format!("nesting deeper than {} levels", self.limits.max_depth),
            ));
        }
        Ok(())
    }

    fn push(&mut self, value: Value, kind: &str, read_only: bool) -> Result<NodeId> {
        let mut node = Node::new(value);
        node.type_hint = Some(hint(kind));
        if read_only {
            node.flags.insert(NodeFlags::READ_ONLY);
        }
        self.doc.push(node)
    }

    fn cstring(&mut self, what: &str) -> Result<String> {
        let start = self.pos;
        let rest = self
            .input
            .get(start..)
            .ok_or_else(|| parse_error(format!("the {what} is cut short"), start))?;
        let length = rest
            .iter()
            .position(|byte| *byte == 0)
            .ok_or_else(|| parse_error(format!("the {what} has no terminator"), start))?;
        let text = String::from_utf8(rest.get(..length).unwrap_or_default().to_vec())
            .map_err(|_| parse_error(format!("the {what} is not UTF-8"), start))?;
        self.pos = start + length + 1;
        Ok(text)
    }

    fn string(&mut self, what: &str) -> Result<String> {
        let start = self.pos;
        let length = self.le_i32(what)?;
        if length < 1 {
            return Err(parse_error(
                format!("the {what} length is smaller than 1"),
                start,
            ));
        }
        let length = usize::try_from(length)
            .map_err(|_| parse_error(format!("the {what} is too long"), start))?;
        let data = self.take(length, what)?;
        let (last, text) = data
            .split_last()
            .ok_or_else(|| parse_error(format!("the {what} is cut short"), start))?;
        if *last != 0 {
            return Err(parse_error(
                format!("the {what} has no terminating NUL"),
                start,
            ));
        }
        String::from_utf8(text.to_vec())
            .map_err(|_| parse_error(format!("the {what} is not UTF-8"), start))
    }

    fn document(&mut self, depth: usize, array: bool) -> Result<NodeId> {
        self.check_depth(depth)?;
        let start = self.pos;
        let length = self.le_i32("document length")?;
        if length < 5 {
            return Err(parse_error("the document length is smaller than 5", start));
        }
        let length =
            usize::try_from(length).map_err(|_| parse_error("the document is too long", start))?;
        let end = start
            .checked_add(length)
            .ok_or_else(|| parse_error("the document is too long", start))?;
        if end > self.input.len() {
            return Err(parse_error("the document extends past the input", start));
        }
        let content_end = end - 1;
        let mut entries: Vec<(String, NodeId)> = Vec::new();
        while self.pos < content_end {
            let kind = self.byte("element type")?;
            if kind == 0 {
                return Err(parse_error(
                    "a zero element type appears before the document end",
                    self.pos - 1,
                ));
            }
            let name = self.cstring("element name")?;
            let value = self.value(kind, depth + 1)?;
            entries.push((name, value));
        }
        if self.pos != content_end {
            return Err(parse_error(
                "an element extends past the document end",
                self.pos,
            ));
        }
        if self.byte("document terminator")? != 0 {
            return Err(parse_error(
                "the document has no terminating zero byte",
                content_end,
            ));
        }
        if self.pos != end {
            return Err(parse_error("the document length is wrong", start));
        }
        if array {
            let items = entries.into_iter().map(|(_, value)| value).collect();
            self.push(Value::Array(items), "array", true)
        } else {
            self.push(Value::Object(entries), "document", true)
        }
    }

    fn value(&mut self, kind: u8, depth: usize) -> Result<NodeId> {
        match kind {
            0x01 => {
                let bits = self.le_u64("double")?;
                self.push(
                    Value::Float {
                        v: f64::from_bits(bits),
                        width: FloatWidth::F64,
                    },
                    "double",
                    false,
                )
            }
            0x02 => {
                let text = self.string("string")?;
                self.push(Value::Str(text), "string", false)
            }
            0x03 => self.document(depth, false),
            0x04 => self.document(depth, true),
            0x05 => {
                let length = self.le_i32("binary length")?;
                if length < 0 {
                    return Err(parse_error("the binary length is negative", self.pos - 4));
                }
                let length = usize::try_from(length)
                    .map_err(|_| parse_error("the binary is too long", self.pos - 4))?;
                let subtype = self.byte("binary subtype")?;
                let data = self.take(length, "binary data")?.to_vec();
                self.push(Value::Bytes(data), &format!("binary:{subtype}"), true)
            }
            0x06 => self.push(Value::Null, "undefined", true),
            0x07 => {
                let data = self.take(12, "ObjectId")?.to_vec();
                self.push(Value::Bytes(data), "objectid", true)
            }
            0x08 => {
                let value = self.byte("boolean")?;
                match value {
                    0 => self.push(Value::Bool(false), "bool", false),
                    1 => self.push(Value::Bool(true), "bool", false),
                    _ => Err(parse_error("a boolean is neither 0 nor 1", self.pos - 1)),
                }
            }
            0x09 => {
                let value = self.le_i64("date")?;
                let mut node = Node::new(Value::Int {
                    v: i128::from(value),
                    width: IntWidth::I64,
                });
                node.type_hint = Some(hint("date"));
                node.flags.insert(NodeFlags::READ_ONLY);
                self.doc.push(node)
            }
            0x0a => self.push(Value::Null, "null", true),
            0x0b => {
                let start = self.pos;
                self.cstring("regex pattern")?;
                self.cstring("regex options")?;
                let raw = self
                    .input
                    .get(start..self.pos)
                    .ok_or_else(|| parse_error("the regex is cut short", start))?
                    .to_vec();
                self.push(Value::Bytes(raw), "regex", true)
            }
            0x0c => {
                let start = self.pos;
                self.string("DBPointer namespace")?;
                self.take(12, "DBPointer id")?;
                let raw = self
                    .input
                    .get(start..self.pos)
                    .ok_or_else(|| parse_error("the DBPointer is cut short", start))?
                    .to_vec();
                self.push(Value::Bytes(raw), "dbpointer", true)
            }
            0x0d => {
                let text = self.string("JavaScript code")?;
                self.push(Value::Str(text), "code", true)
            }
            0x0e => {
                let text = self.string("symbol")?;
                self.push(Value::Str(text), "symbol", true)
            }
            0x0f => {
                let start = self.pos;
                let total = self.le_i32("code with scope length")?;
                if total < 4 {
                    return Err(parse_error("the code with scope is too short", start));
                }
                let total = usize::try_from(total)
                    .map_err(|_| parse_error("the code with scope is too long", start))?;
                let end = start
                    .checked_add(total)
                    .ok_or_else(|| parse_error("the code with scope is too long", start))?;
                if end > self.input.len() {
                    return Err(parse_error("the code with scope is cut short", start));
                }
                let raw = self
                    .input
                    .get(start..end)
                    .ok_or_else(|| parse_error("the code with scope is cut short", start))?
                    .to_vec();
                self.pos = end;
                self.push(Value::Bytes(raw), "code-w-scope", true)
            }
            0x10 => {
                let value = self.le_i32("int32")?;
                self.push(
                    Value::Int {
                        v: i128::from(value),
                        width: IntWidth::I32,
                    },
                    "int32",
                    false,
                )
            }
            0x11 => {
                let value = self.le_u64("timestamp")?;
                let mut node = Node::new(Value::Int {
                    v: i128::from(value),
                    width: IntWidth::U64,
                });
                node.type_hint = Some(hint("timestamp"));
                node.flags.insert(NodeFlags::READ_ONLY);
                self.doc.push(node)
            }
            0x12 => {
                let value = self.le_i64("int64")?;
                self.push(
                    Value::Int {
                        v: i128::from(value),
                        width: IntWidth::I64,
                    },
                    "int64",
                    false,
                )
            }
            0x13 => {
                let data = self.take(16, "decimal128")?.to_vec();
                self.push(Value::Bytes(data), "decimal128", true)
            }
            0x7f => self.push(Value::Null, "maxkey", true),
            0xff => self.push(Value::Null, "minkey", true),
            _ => Err(parse_error(
                format!("the BSON element type 0x{kind:02x} is not supported"),
                self.pos - 1,
            )),
        }
    }
}
