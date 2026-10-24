//! The MessagePack reader (see the module documentation for the tree).

use saveprism_core::limits::len_u64;
use saveprism_core::{
    ByteSpan, Error, FloatWidth, IntWidth, Limits, Node, NodeFlags, NodeId, Params, Result,
    SaveDoc, TypeHint, Value, codes,
};

use super::{HINT_PREFIX, MSGPACK_PARSE, ORIGINAL};

/// A parse error at `at`.
fn parse_error(message: impl Into<String>, at: usize) -> Error {
    Error::new(MSGPACK_PARSE, message).with_offset(len_u64(at))
}

fn hint(kind: &str) -> TypeHint {
    TypeHint::new(format!("{HINT_PREFIX}{kind}"))
}

/// Decodes a whole MessagePack stream into a document and its original bytes.
pub(super) fn decode(input: &[u8], limits: &Limits) -> Result<(SaveDoc, Params)> {
    let mut parser = Parser {
        input,
        pos: 0,
        limits,
        doc: SaveDoc::with_node_limit(limits.max_nodes),
    };
    let root = parser.value(0, false)?;
    if parser.pos != input.len() {
        return Err(parse_error(
            "the MessagePack data continues after the first value",
            parser.pos,
        ));
    }
    parser.doc.set_root(root);
    let mut params = Params::new();
    params.set(ORIGINAL, input.to_vec());
    Ok((parser.doc, params))
}

/// True when `input` is one complete MessagePack value whose root is an array or map.
pub(super) fn plausible(input: &[u8], limits: &Limits) -> bool {
    match decode(input, limits) {
        Ok((doc, _)) => matches!(
            doc.get(doc.root()).map(|node| &node.value),
            Some(Value::Array(_) | Value::Map(_) | Value::Object(_))
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

    fn u16(&mut self, what: &str) -> Result<u16> {
        let bytes = self.take(2, what)?;
        let array: [u8; 2] = bytes
            .try_into()
            .map_err(|_| parse_error(format!("the {what} is cut short"), self.pos))?;
        Ok(u16::from_be_bytes(array))
    }

    fn u32(&mut self, what: &str) -> Result<u32> {
        let bytes = self.take(4, what)?;
        let array: [u8; 4] = bytes
            .try_into()
            .map_err(|_| parse_error(format!("the {what} is cut short"), self.pos))?;
        Ok(u32::from_be_bytes(array))
    }

    fn u64(&mut self, what: &str) -> Result<u64> {
        let bytes = self.take(8, what)?;
        let array: [u8; 8] = bytes
            .try_into()
            .map_err(|_| parse_error(format!("the {what} is cut short"), self.pos))?;
        Ok(u64::from_be_bytes(array))
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

    fn push(&mut self, value: Value, kind: &str, start: usize, read_only: bool) -> Result<NodeId> {
        let mut node = Node::new(value);
        node.type_hint = Some(hint(kind));
        node.span = Some(ByteSpan {
            start: len_u64(start),
            end: len_u64(self.pos),
        });
        if read_only {
            node.flags.insert(NodeFlags::READ_ONLY);
        }
        self.doc.push(node)
    }

    fn text(&mut self, length: usize, start: usize, read_only: bool) -> Result<NodeId> {
        let bytes = self.take(length, "string")?.to_vec();
        match String::from_utf8(bytes) {
            Ok(text) => self.push(Value::Str(text), "str", start, read_only),
            Err(error) => self.push(Value::Bytes(error.into_bytes()), "str-bytes", start, true),
        }
    }

    /// Reads one value. `read_only` marks map keys.
    fn value(&mut self, depth: usize, read_only: bool) -> Result<NodeId> {
        self.check_depth(depth)?;
        let start = self.pos;
        let code = self.byte("format byte")?;
        match code {
            0x00..=0x7f => self.push(
                Value::Int {
                    v: i128::from(code),
                    width: IntWidth::U8,
                },
                "int",
                start,
                read_only,
            ),
            0xe0..=0xff => self.push(
                Value::Int {
                    v: i128::from(code as i8),
                    width: IntWidth::I8,
                },
                "int",
                start,
                read_only,
            ),
            0x80..=0x8f => self.map(usize::from(code & 0x0f), depth, start),
            0x90..=0x9f => self.array(usize::from(code & 0x0f), depth, start),
            0xa0..=0xbf => self.text(usize::from(code & 0x1f), start, read_only),
            0xc0 => self.push(Value::Null, "nil", start, true),
            0xc1 => Err(parse_error("the never-used format byte 0xc1", start)),
            0xc2 => self.push(Value::Bool(false), "bool", start, read_only),
            0xc3 => self.push(Value::Bool(true), "bool", start, read_only),
            0xc4 => {
                let length = usize::from(self.byte("bin length")?);
                self.binary(length, start)
            }
            0xc5 => {
                let length = usize::from(self.u16("bin length")?);
                self.binary(length, start)
            }
            0xc6 => {
                let length = usize::try_from(self.u32("bin length")?)
                    .map_err(|_| parse_error("the bin length is too large", start))?;
                self.binary(length, start)
            }
            0xc7 => {
                let length = usize::from(self.byte("ext length")?);
                let kind = self.byte("ext type")?;
                self.extension(length, kind, start)
            }
            0xc8 => {
                let length = usize::from(self.u16("ext length")?);
                let kind = self.byte("ext type")?;
                self.extension(length, kind, start)
            }
            0xc9 => {
                let length = usize::try_from(self.u32("ext length")?)
                    .map_err(|_| parse_error("the ext length is too large", start))?;
                let kind = self.byte("ext type")?;
                self.extension(length, kind, start)
            }
            0xca => {
                let bits = self.u32("float 32")?;
                let v = f64::from(f32::from_bits(bits));
                self.push(
                    Value::Float {
                        v,
                        width: FloatWidth::F32,
                    },
                    "float",
                    start,
                    read_only,
                )
            }
            0xcb => {
                let bits = self.u64("float 64")?;
                self.push(
                    Value::Float {
                        v: f64::from_bits(bits),
                        width: FloatWidth::F64,
                    },
                    "float",
                    start,
                    read_only,
                )
            }
            0xcc => {
                let value = self.byte("uint 8")?;
                self.push(
                    Value::Int {
                        v: i128::from(value),
                        width: IntWidth::U8,
                    },
                    "int",
                    start,
                    read_only,
                )
            }
            0xcd => {
                let value = self.u16("uint 16")?;
                self.push(
                    Value::Int {
                        v: i128::from(value),
                        width: IntWidth::U16,
                    },
                    "int",
                    start,
                    read_only,
                )
            }
            0xce => {
                let value = self.u32("uint 32")?;
                self.push(
                    Value::Int {
                        v: i128::from(value),
                        width: IntWidth::U32,
                    },
                    "int",
                    start,
                    read_only,
                )
            }
            0xcf => {
                let value = self.u64("uint 64")?;
                self.push(
                    Value::Int {
                        v: i128::from(value),
                        width: IntWidth::U64,
                    },
                    "int",
                    start,
                    read_only,
                )
            }
            0xd0 => {
                let value = self.byte("int 8")? as i8;
                self.push(
                    Value::Int {
                        v: i128::from(value),
                        width: IntWidth::I8,
                    },
                    "int",
                    start,
                    read_only,
                )
            }
            0xd1 => {
                let value = self.u16("int 16")? as i16;
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
            0xd2 => {
                let value = self.u32("int 32")? as i32;
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
            0xd3 => {
                let value = self.u64("int 64")? as i64;
                self.push(
                    Value::Int {
                        v: i128::from(value),
                        width: IntWidth::I64,
                    },
                    "int",
                    start,
                    read_only,
                )
            }
            0xd4 => {
                let kind = self.byte("ext type")?;
                self.extension(1, kind, start)
            }
            0xd5 => {
                let kind = self.byte("ext type")?;
                self.extension(2, kind, start)
            }
            0xd6 => {
                let kind = self.byte("ext type")?;
                self.extension(4, kind, start)
            }
            0xd7 => {
                let kind = self.byte("ext type")?;
                self.extension(8, kind, start)
            }
            0xd8 => {
                let kind = self.byte("ext type")?;
                self.extension(16, kind, start)
            }
            0xd9 => {
                let length = usize::from(self.byte("str 8 length")?);
                self.text(length, start, read_only)
            }
            0xda => {
                let length = usize::from(self.u16("str 16 length")?);
                self.text(length, start, read_only)
            }
            0xdb => {
                let length = usize::try_from(self.u32("str 32 length")?)
                    .map_err(|_| parse_error("the str length is too large", start))?;
                self.text(length, start, read_only)
            }
            0xdc => {
                let length = usize::from(self.u16("array 16 length")?);
                self.array(length, depth, start)
            }
            0xdd => {
                let length = usize::try_from(self.u32("array 32 length")?)
                    .map_err(|_| parse_error("the array length is too large", start))?;
                self.array(length, depth, start)
            }
            0xde => {
                let length = usize::from(self.u16("map 16 length")?);
                self.map(length, depth, start)
            }
            0xdf => {
                let length = usize::try_from(self.u32("map 32 length")?)
                    .map_err(|_| parse_error("the map length is too large", start))?;
                self.map(length, depth, start)
            }
        }
    }

    fn array(&mut self, length: usize, depth: usize, start: usize) -> Result<NodeId> {
        let mut items = Vec::with_capacity(length.min(1024));
        for _ in 0..length {
            items.push(self.value(depth + 1, false)?);
        }
        self.push(Value::Array(items), "array", start, true)
    }

    fn map(&mut self, length: usize, depth: usize, start: usize) -> Result<NodeId> {
        let mut entries = Vec::with_capacity(length.min(1024));
        for _ in 0..length {
            let key = self.value(depth + 1, true)?;
            let value = self.value(depth + 1, false)?;
            entries.push((key, value));
        }
        self.push(Value::Map(entries), "map", start, true)
    }

    fn binary(&mut self, length: usize, start: usize) -> Result<NodeId> {
        let bytes = self.take(length, "bin data")?.to_vec();
        self.push(Value::Bytes(bytes), "bin", start, true)
    }

    fn extension(&mut self, length: usize, kind: u8, start: usize) -> Result<NodeId> {
        let bytes = self.take(length, "ext data")?.to_vec();
        self.push(
            Value::Bytes(bytes),
            &format!("ext:{}", i8::from_be_bytes([kind])),
            start,
            true,
        )
    }
}
