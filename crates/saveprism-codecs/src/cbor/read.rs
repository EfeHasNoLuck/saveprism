//! The CBOR reader (see the module documentation for the tree).

use saveprism_core::limits::len_u64;
use saveprism_core::{
    ByteSpan, Error, FloatWidth, IntWidth, Limits, Node, NodeFlags, NodeId, Params, Result,
    SaveDoc, TypeHint, Value, codes,
};

use super::{CBOR_PARSE, HINT_PREFIX, ORIGINAL};

fn parse_error(message: impl Into<String>, at: usize) -> Error {
    Error::new(CBOR_PARSE, message).with_offset(len_u64(at))
}

fn hint(kind: &str) -> TypeHint {
    TypeHint::new(format!("{HINT_PREFIX}{kind}"))
}

/// Decodes one complete CBOR item into a document and its original bytes.
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
            "the CBOR data continues after the first item",
            parser.pos,
        ));
    }
    parser.doc.set_root(root);
    let mut params = Params::new();
    params.set(ORIGINAL, input.to_vec());
    Ok((parser.doc, params))
}

/// True when `input` is one complete well-formed CBOR item whose root is a container or a tag.
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

    fn peek(&self) -> Option<u8> {
        self.input.get(self.pos).copied()
    }

    fn big_uint(&mut self, width: usize, what: &str) -> Result<u64> {
        let bytes = self.take(width, what)?;
        let mut value = 0_u64;
        for byte in bytes {
            value = (value << 8) | u64::from(*byte);
        }
        Ok(value)
    }

    fn argument(&mut self, additional: u8, at: usize) -> Result<u64> {
        match additional {
            0..=23 => Ok(u64::from(additional)),
            24 => Ok(u64::from(self.byte("one-byte argument")?)),
            25 => self.big_uint(2, "two-byte argument"),
            26 => self.big_uint(4, "four-byte argument"),
            27 => self.big_uint(8, "eight-byte argument"),
            28..=30 => Err(parse_error(
                "a reserved additional-information value (28 to 30) is not well-formed",
                at,
            )),
            _ => Err(parse_error(
                "additional information 31 is not an argument",
                at,
            )),
        }
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

    /// Consumes a definite-length string body of `length` bytes.
    fn bytes_body(&mut self, length: usize, start: usize, kind: &str) -> Result<NodeId> {
        let data = self.take(length, "string body")?.to_vec();
        self.push(Value::Bytes(data), kind, start, true)
    }

    fn text_body(&mut self, length: usize, start: usize, kind: &str) -> Result<NodeId> {
        let data = self.take(length, "text body")?.to_vec();
        match String::from_utf8(data) {
            Ok(text) => self.push(Value::Str(text), kind, start, false),
            Err(error) => self.push(
                Value::Bytes(error.into_bytes()),
                "text-invalid",
                start,
                true,
            ),
        }
    }

    fn byte_string(&mut self, additional: u8, start: usize) -> Result<NodeId> {
        if additional == 31 {
            let mut data = Vec::new();
            loop {
                match self.peek() {
                    Some(0xff) => {
                        self.pos += 1;
                        break;
                    }
                    Some(byte) if byte >> 5 == 2 => {
                        self.pos += 1;
                        let length = self.argument(byte & 0x1f, self.pos - 1)?;
                        let length = usize::try_from(length)
                            .map_err(|_| parse_error("the byte string is too long", start))?;
                        data.extend_from_slice(self.take(length, "byte string chunk")?);
                    }
                    Some(_) => {
                        return Err(parse_error(
                            "an indefinite byte string has a chunk of another type",
                            self.pos,
                        ));
                    }
                    None => {
                        return Err(parse_error(
                            "an indefinite byte string has no break",
                            self.pos,
                        ));
                    }
                }
            }
            self.push(Value::Bytes(data), "bytes:indef", start, true)
        } else {
            let length = self.argument(additional, start)?;
            let length = usize::try_from(length)
                .map_err(|_| parse_error("the byte string is too long", start))?;
            self.bytes_body(length, start, "bytes")
        }
    }

    fn text_string(&mut self, additional: u8, start: usize, read_only: bool) -> Result<NodeId> {
        if additional == 31 {
            let mut data = Vec::new();
            loop {
                match self.peek() {
                    Some(0xff) => {
                        self.pos += 1;
                        break;
                    }
                    Some(byte) if byte >> 5 == 3 => {
                        self.pos += 1;
                        let length = self.argument(byte & 0x1f, self.pos - 1)?;
                        let length = usize::try_from(length)
                            .map_err(|_| parse_error("the text string is too long", start))?;
                        data.extend_from_slice(self.take(length, "text string chunk")?);
                    }
                    Some(_) => {
                        return Err(parse_error(
                            "an indefinite text string has a chunk of another type",
                            self.pos,
                        ));
                    }
                    None => {
                        return Err(parse_error(
                            "an indefinite text string has no break",
                            self.pos,
                        ));
                    }
                }
            }
            match String::from_utf8(data) {
                Ok(text) => self.push(Value::Str(text), "text:indef", start, read_only),
                Err(error) => self.push(
                    Value::Bytes(error.into_bytes()),
                    "text-invalid",
                    start,
                    true,
                ),
            }
        } else {
            let length = self.argument(additional, start)?;
            let length = usize::try_from(length)
                .map_err(|_| parse_error("the text string is too long", start))?;
            let node = self.text_body(length, start, "text")?;
            if read_only {
                self.mark_read_only(node)?;
            }
            Ok(node)
        }
    }

    fn mark_read_only(&mut self, id: NodeId) -> Result<()> {
        let node = self
            .doc
            .get(id)
            .cloned()
            .ok_or_else(|| parse_error("the node disappeared", self.pos))?;
        let mut flags = node.flags;
        flags.insert(NodeFlags::READ_ONLY);
        self.doc.set_flags(id, flags)
    }

    fn array(&mut self, additional: u8, depth: usize, start: usize) -> Result<NodeId> {
        let mut items = Vec::new();
        if additional == 31 {
            loop {
                if self.peek() == Some(0xff) {
                    self.pos += 1;
                    break;
                }
                if self.peek().is_none() {
                    return Err(parse_error("an indefinite array has no break", self.pos));
                }
                items.push(self.value(depth + 1, false)?);
            }
            self.push(Value::Array(items), "array:indef", start, true)
        } else {
            let length = self.argument(additional, start)?;
            let length = usize::try_from(length)
                .map_err(|_| parse_error("the array length is too large", start))?;
            items.reserve(length.min(1024));
            for _ in 0..length {
                items.push(self.value(depth + 1, false)?);
            }
            self.push(Value::Array(items), "array", start, true)
        }
    }

    fn map(&mut self, additional: u8, depth: usize, start: usize) -> Result<NodeId> {
        let mut entries = Vec::new();
        if additional == 31 {
            loop {
                if self.peek() == Some(0xff) {
                    self.pos += 1;
                    break;
                }
                if self.peek().is_none() {
                    return Err(parse_error("an indefinite map has no break", self.pos));
                }
                let key = self.value(depth + 1, true)?;
                if self.peek() == Some(0xff) {
                    return Err(parse_error(
                        "an indefinite map has a key without a value",
                        self.pos,
                    ));
                }
                let value = self.value(depth + 1, false)?;
                entries.push((key, value));
            }
            self.push(Value::Map(entries), "map:indef", start, true)
        } else {
            let length = self.argument(additional, start)?;
            let length = usize::try_from(length)
                .map_err(|_| parse_error("the map length is too large", start))?;
            entries.reserve(length.min(1024));
            for _ in 0..length {
                let key = self.value(depth + 1, true)?;
                let value = self.value(depth + 1, false)?;
                entries.push((key, value));
            }
            self.push(Value::Map(entries), "map", start, true)
        }
    }

    fn tag(&mut self, additional: u8, depth: usize, start: usize) -> Result<NodeId> {
        let number = self.argument(additional, start)?;
        let child = self.value(depth + 1, false)?;
        let value = Value::Object(vec![("(tag)".to_string(), child)]);
        let mut node = Node::new(value);
        node.type_hint = Some(hint(&format!("tag:{number}")));
        node.span = Some(ByteSpan {
            start: len_u64(start),
            end: len_u64(self.pos),
        });
        node.flags.insert(NodeFlags::READ_ONLY);
        self.doc.push(node)
    }

    fn simple(&mut self, additional: u8, start: usize, read_only: bool) -> Result<NodeId> {
        match additional {
            20 => self.push(Value::Bool(false), "bool", start, read_only),
            21 => self.push(Value::Bool(true), "bool", start, read_only),
            22 => self.push(Value::Null, "null", start, true),
            23 => self.push(Value::Null, "undefined", start, true),
            0..=19 => {
                let value = self.argument(additional, start)?;
                self.push(
                    Value::Int {
                        v: i128::from(value),
                        width: IntWidth::U8,
                    },
                    "simple",
                    start,
                    true,
                )
            }
            24 => {
                let value = self.byte("simple value")?;
                if value < 32 {
                    return Err(parse_error(
                        "a two-byte simple value below 32 is not well-formed",
                        start,
                    ));
                }
                self.push(
                    Value::Int {
                        v: i128::from(value),
                        width: IntWidth::U8,
                    },
                    "simple",
                    start,
                    true,
                )
            }
            25 => {
                let bits = self.big_uint(2, "float 16")?;
                let bits = u16::try_from(bits)
                    .map_err(|_| parse_error("the half float is malformed", start))?;
                let v = half_to_f64(bits);
                self.push(
                    Value::Float {
                        v,
                        width: FloatWidth::F32,
                    },
                    "float16",
                    start,
                    true,
                )
            }
            26 => {
                let bits = self.big_uint(4, "float 32")?;
                let bits = u32::try_from(bits)
                    .map_err(|_| parse_error("the float 32 is malformed", start))?;
                self.push(
                    Value::Float {
                        v: f64::from(f32::from_bits(bits)),
                        width: FloatWidth::F32,
                    },
                    "float",
                    start,
                    read_only,
                )
            }
            27 => {
                let bits = self.big_uint(8, "float 64")?;
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
            28..=30 => Err(parse_error(
                "a reserved simple value (28 to 30) is not well-formed",
                start,
            )),
            _ => Err(parse_error(
                "a break stop code appears where an item is expected",
                start,
            )),
        }
    }

    fn value(&mut self, depth: usize, read_only: bool) -> Result<NodeId> {
        self.check_depth(depth)?;
        let start = self.pos;
        let initial = self.byte("initial byte")?;
        let major = initial >> 5;
        let additional = initial & 0x1f;
        match major {
            0 => {
                let value = self.argument(additional, start)?;
                self.push(
                    Value::Int {
                        v: i128::from(value),
                        width: unsigned_width(additional),
                    },
                    "int",
                    start,
                    read_only,
                )
            }
            1 => {
                let value = self.argument(additional, start)?;
                let value = -1 - i128::from(value);
                self.push(
                    Value::Int {
                        v: value,
                        width: IntWidth::I64,
                    },
                    "int",
                    start,
                    read_only,
                )
            }
            2 => self.byte_string(additional, start),
            3 => self.text_string(additional, start, read_only),
            4 => self.array(additional, depth, start),
            5 => self.map(additional, depth, start),
            6 => self.tag(additional, depth, start),
            7 => self.simple(additional, start, read_only),
            _ => Err(parse_error("unknown CBOR major type", start)),
        }
    }
}

fn unsigned_width(additional: u8) -> IntWidth {
    match additional {
        24 => IntWidth::U8,
        25 => IntWidth::U16,
        26 => IntWidth::U32,
        27 => IntWidth::U64,
        _ => IntWidth::U8,
    }
}

/// Converts an IEEE 754 binary16 value to the same value as a binary64 number.
pub(super) fn half_to_f64(bits: u16) -> f64 {
    let sign = if bits & 0x8000 == 0 { 1.0 } else { -1.0 };
    let exponent = i32::from((bits >> 10) & 0x1f);
    let mantissa = f64::from(bits & 0x03ff);
    if exponent == 0 {
        sign * 2_f64.powi(-14) * (mantissa / 1024.0)
    } else if exponent == 31 {
        if mantissa == 0.0 {
            sign * f64::INFINITY
        } else {
            f64::NAN
        }
    } else {
        sign * 2_f64.powi(exponent - 15) * (1.0 + mantissa / 1024.0)
    }
}
