//! Parses an Odin binary stream into a [`SaveDoc`].
//!
//! Nothing is resolved or executed: entries become values, a node's type entry becomes read-only
//! metadata and references stay read-only numbers or bytes.

use std::collections::HashMap;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    ByteSpan, DecodeCtx, Error, FloatWidth, IntWidth, Node, NodeFlags, NodeId, Params, Result,
    SaveDoc, TypeHint, Value,
};

use super::{HINT_PREFIX, MAX_NESTING, ODIN_PARSE};

const NAMED_START_OF_REFERENCE_NODE: u8 = 0x01;
const UNNAMED_START_OF_REFERENCE_NODE: u8 = 0x02;
const NAMED_START_OF_STRUCT_NODE: u8 = 0x03;
const UNNAMED_START_OF_STRUCT_NODE: u8 = 0x04;
const END_OF_NODE: u8 = 0x05;
const START_OF_ARRAY: u8 = 0x06;
const END_OF_ARRAY: u8 = 0x07;
const PRIMITIVE_ARRAY: u8 = 0x08;
const NAMED_INTERNAL_REFERENCE: u8 = 0x09;
const UNNAMED_INTERNAL_REFERENCE: u8 = 0x0A;
const NAMED_EXTERNAL_REFERENCE_BY_INDEX: u8 = 0x0B;
const UNNAMED_EXTERNAL_REFERENCE_BY_INDEX: u8 = 0x0C;
const NAMED_EXTERNAL_REFERENCE_BY_GUID: u8 = 0x0D;
const UNNAMED_EXTERNAL_REFERENCE_BY_GUID: u8 = 0x0E;
const NAMED_SBYTE: u8 = 0x0F;
const UNNAMED_SBYTE: u8 = 0x10;
const NAMED_BYTE: u8 = 0x11;
const UNNAMED_BYTE: u8 = 0x12;
const NAMED_SHORT: u8 = 0x13;
const UNNAMED_SHORT: u8 = 0x14;
const NAMED_USHORT: u8 = 0x15;
const UNNAMED_USHORT: u8 = 0x16;
const NAMED_INT: u8 = 0x17;
const UNNAMED_INT: u8 = 0x18;
const NAMED_UINT: u8 = 0x19;
const UNNAMED_UINT: u8 = 0x1A;
const NAMED_LONG: u8 = 0x1B;
const UNNAMED_LONG: u8 = 0x1C;
const NAMED_ULONG: u8 = 0x1D;
const UNNAMED_ULONG: u8 = 0x1E;
const NAMED_FLOAT: u8 = 0x1F;
const UNNAMED_FLOAT: u8 = 0x20;
const NAMED_DOUBLE: u8 = 0x21;
const UNNAMED_DOUBLE: u8 = 0x22;
const NAMED_DECIMAL: u8 = 0x23;
const UNNAMED_DECIMAL: u8 = 0x24;
const NAMED_CHAR: u8 = 0x25;
const UNNAMED_CHAR: u8 = 0x26;
const NAMED_STRING: u8 = 0x27;
const UNNAMED_STRING: u8 = 0x28;
const NAMED_GUID: u8 = 0x29;
const UNNAMED_GUID: u8 = 0x2A;
const NAMED_BOOLEAN: u8 = 0x2B;
const UNNAMED_BOOLEAN: u8 = 0x2C;
const NAMED_NULL: u8 = 0x2D;
const UNNAMED_NULL: u8 = 0x2E;
const TYPE_NAME: u8 = 0x2F;
const TYPE_ID: u8 = 0x30;
const END_OF_STREAM: u8 = 0x31;
const NAMED_EXTERNAL_REFERENCE_BY_STRING: u8 = 0x32;
const UNNAMED_EXTERNAL_REFERENCE_BY_STRING: u8 = 0x33;

/// One member read from the stream, before it is pushed into its container.
struct Entry {
    key: String,
    node: Node,
}

/// A type entry of a node.
enum TypeEntry {
    None,
    Name(String),
    Id(i32),
}

/// Reads a whole stream into a document. The original bytes are kept for exact writing.
pub(super) fn read_all(input: &[u8], ctx: &DecodeCtx<'_>) -> Result<(SaveDoc, Params)> {
    let mut reader = Reader {
        input,
        pos: 0,
        doc: ctx.new_doc(),
        types: HashMap::new(),
        ctx,
    };
    reader.read_root()?;
    let mut params = Params::new();
    params.set("original", input.to_vec());
    Ok((reader.doc, params))
}

/// True when `input` is a complete, structurally valid Odin binary stream. Used by `sniff`; it
/// walks the stream without building a document.
pub(super) fn plausible(input: &[u8]) -> bool {
    let mut scan = Scan {
        input,
        pos: 0,
        types: HashMap::new(),
        nodes: 0,
    };
    scan.stream(0, 0).is_some()
}

struct Scan<'i> {
    input: &'i [u8],
    pos: usize,
    types: HashMap<i32, ()>,
    nodes: u64,
}

impl Scan<'_> {
    /// Walks `count` entries (`None` = until a terminator) inside an array or node. `stop` is the
    /// expected end code, or 0 at the top level.
    fn stream(&mut self, depth: usize, stop: u8) -> Option<usize> {
        if depth > MAX_NESTING {
            return None;
        }
        let mut count = 0_usize;
        loop {
            let Some(&code) = self.input.get(self.pos) else {
                return if stop == 0 { Some(count) } else { None };
            };
            if code == END_OF_STREAM {
                self.pos += 1;
                if stop == 0 && self.pos == self.input.len() {
                    return Some(count);
                }
                return None;
            }
            if stop != 0 && code == stop {
                self.pos += 1;
                return Some(count);
            }
            self.entry(depth)?;
            count = count.checked_add(1)?;
            self.nodes = self.nodes.checked_add(1)?;
            if self.nodes > 10_000_000 {
                return None;
            }
        }
    }

    fn entry(&mut self, depth: usize) -> Option<()> {
        let code = self.byte()?;
        let named = is_named(code);
        if named {
            self.string()?;
        }
        match code {
            NAMED_START_OF_REFERENCE_NODE | UNNAMED_START_OF_REFERENCE_NODE => {
                self.type_entry()?;
                self.skip(4)?;
                self.stream(depth + 1, END_OF_NODE)?;
            }
            NAMED_START_OF_STRUCT_NODE | UNNAMED_START_OF_STRUCT_NODE => {
                self.type_entry()?;
                self.stream(depth + 1, END_OF_NODE)?;
            }
            START_OF_ARRAY => {
                let length = self.i64()?;
                let count = self.stream(depth + 1, END_OF_ARRAY)?;
                if length < 0 || usize::try_from(length).ok()? != count {
                    return None;
                }
            }
            PRIMITIVE_ARRAY => {
                let count = self.i32()?;
                let bytes = self.i32()?;
                if count < 0 || bytes < 0 {
                    return None;
                }
                let total = usize::try_from(count)
                    .ok()?
                    .checked_mul(usize::try_from(bytes).ok()?)?;
                self.skip(total)?;
            }
            NAMED_INTERNAL_REFERENCE | UNNAMED_INTERNAL_REFERENCE => {}
            NAMED_EXTERNAL_REFERENCE_BY_INDEX | UNNAMED_EXTERNAL_REFERENCE_BY_INDEX => {
                self.skip(4)?;
            }
            NAMED_EXTERNAL_REFERENCE_BY_GUID | UNNAMED_EXTERNAL_REFERENCE_BY_GUID => {
                self.skip(16)?;
            }
            NAMED_EXTERNAL_REFERENCE_BY_STRING | UNNAMED_EXTERNAL_REFERENCE_BY_STRING => {
                self.string()?;
            }
            NAMED_SBYTE | UNNAMED_SBYTE | NAMED_BYTE | UNNAMED_BYTE => self.skip(1)?,
            NAMED_SHORT | UNNAMED_SHORT | NAMED_USHORT | UNNAMED_USHORT | NAMED_CHAR
            | UNNAMED_CHAR => self.skip(2)?,
            NAMED_INT | UNNAMED_INT | NAMED_UINT | UNNAMED_UINT | NAMED_FLOAT | UNNAMED_FLOAT => {
                self.skip(4)?;
            }
            NAMED_LONG | UNNAMED_LONG | NAMED_ULONG | UNNAMED_ULONG | NAMED_DOUBLE
            | UNNAMED_DOUBLE => self.skip(8)?,
            NAMED_DECIMAL | UNNAMED_DECIMAL | NAMED_GUID | UNNAMED_GUID => self.skip(16)?,
            NAMED_STRING | UNNAMED_STRING => {
                self.string()?;
            }
            NAMED_BOOLEAN | UNNAMED_BOOLEAN => self.skip(1)?,
            NAMED_NULL | UNNAMED_NULL => {}
            _ => return None,
        }
        Some(())
    }

    fn type_entry(&mut self) -> Option<()> {
        match self.byte()? {
            TYPE_NAME => {
                let id = self.i32()?;
                self.types.insert(id, ());
                self.string()?;
                Some(())
            }
            TYPE_ID => {
                let id = self.i32()?;
                self.types.get(&id).map(|_| ())
            }
            UNNAMED_NULL => Some(()),
            _ => None,
        }
    }

    fn string(&mut self) -> Option<()> {
        let wide = self.byte()? != 0;
        let length = self.i32()?;
        if length < 0 {
            return None;
        }
        let units = usize::try_from(length).ok()?;
        let bytes = if wide { units.checked_mul(2)? } else { units };
        self.skip(bytes)?;
        // The string must be valid text in the width the file declares.
        let start = self.pos.checked_sub(bytes)?;
        let slice = self.input.get(start..self.pos)?;
        if wide {
            let (pairs, remainder) = slice.as_chunks::<2>();
            if !remainder.is_empty() {
                return None;
            }
            let mut units = Vec::with_capacity(pairs.len());
            for pair in pairs {
                units.push(u16::from_le_bytes(*pair));
            }
            String::from_utf16(&units).ok()?;
        } else {
            std::str::from_utf8(slice).ok()?;
        }
        Some(())
    }

    fn i32(&mut self) -> Option<i32> {
        let bytes = self.input.get(self.pos..self.pos + 4)?;
        self.pos += 4;
        Some(i32::from_le_bytes([
            *bytes.first()?,
            *bytes.get(1)?,
            *bytes.get(2)?,
            *bytes.get(3)?,
        ]))
    }

    fn i64(&mut self) -> Option<i64> {
        let bytes = self.input.get(self.pos..self.pos + 8)?;
        self.pos += 8;
        let mut raw = [0_u8; 8];
        for (slot, byte) in raw.iter_mut().zip(bytes) {
            *slot = *byte;
        }
        Some(i64::from_le_bytes(raw))
    }

    fn byte(&mut self) -> Option<u8> {
        let byte = *self.input.get(self.pos)?;
        self.pos += 1;
        Some(byte)
    }

    fn skip(&mut self, count: usize) -> Option<()> {
        self.pos = self.pos.checked_add(count)?;
        if self.pos > self.input.len() {
            return None;
        }
        Some(())
    }
}

/// True for an entry code that writes a name before its payload.
fn is_named(code: u8) -> bool {
    matches!(
        code,
        NAMED_START_OF_REFERENCE_NODE
            | NAMED_START_OF_STRUCT_NODE
            | NAMED_INTERNAL_REFERENCE
            | NAMED_EXTERNAL_REFERENCE_BY_INDEX
            | NAMED_EXTERNAL_REFERENCE_BY_GUID
            | NAMED_SBYTE
            | NAMED_BYTE
            | NAMED_SHORT
            | NAMED_USHORT
            | NAMED_INT
            | NAMED_UINT
            | NAMED_LONG
            | NAMED_ULONG
            | NAMED_FLOAT
            | NAMED_DOUBLE
            | NAMED_DECIMAL
            | NAMED_CHAR
            | NAMED_STRING
            | NAMED_GUID
            | NAMED_BOOLEAN
            | NAMED_NULL
            | NAMED_EXTERNAL_REFERENCE_BY_STRING
    )
}

struct Reader<'i, 'c, 'l> {
    input: &'i [u8],
    pos: usize,
    doc: SaveDoc,
    types: HashMap<i32, String>,
    ctx: &'c DecodeCtx<'l>,
}

impl Reader<'_, '_, '_> {
    fn read_root(&mut self) -> Result<()> {
        let entries = self.entries(0, 0)?;
        let mut children = Vec::with_capacity(entries.len());
        for entry in entries {
            children.push((entry.key, self.doc.push(entry.node)?));
        }
        let root = self.doc.push(Node {
            value: Value::Object(children),
            type_hint: Some(TypeHint::new(format!("{HINT_PREFIX}stream"))),
            lexeme: None,
            span: None,
            flags: NodeFlags::READ_ONLY,
        })?;
        self.doc.set_root(root);
        Ok(())
    }

    /// Reads entries until `stop` (an array/node end) or the end of the file at the top level.
    fn entries(&mut self, depth: usize, stop: u8) -> Result<Vec<Entry>> {
        self.ctx.check_depth(depth)?;
        if depth > MAX_NESTING {
            return Err(parse_error("the nesting is too deep"));
        }
        let mut out = Vec::new();
        loop {
            let Some(&code) = self.input.get(self.pos) else {
                if stop == 0 {
                    return Ok(out);
                }
                return Err(parse_error("the stream ends in the middle of a container"));
            };
            if code == END_OF_STREAM {
                self.pos += 1;
                if stop == 0 && self.pos == self.input.len() {
                    return Ok(out);
                }
                return Err(parse_error("an unexpected end-of-stream entry"));
            }
            if stop != 0 && code == stop {
                self.pos += 1;
                return Ok(out);
            }
            let index = out.len();
            let entry = self.entry(depth, index)?;
            out.push(entry);
        }
    }

    fn entry(&mut self, depth: usize, index: usize) -> Result<Entry> {
        let code = self.byte()?;
        let name = if is_named(code) {
            Some(self.string()?)
        } else {
            None
        };
        let key = name.unwrap_or_else(|| format!("#{index}"));
        let node = match code {
            NAMED_START_OF_REFERENCE_NODE | UNNAMED_START_OF_REFERENCE_NODE => {
                let type_entry = self.type_entry()?;
                let id = self.i32()?;
                let members = self.entries(depth + 1, END_OF_NODE)?;
                self.node_object(true, type_entry, Some(id), members)?
            }
            NAMED_START_OF_STRUCT_NODE | UNNAMED_START_OF_STRUCT_NODE => {
                let type_entry = self.type_entry()?;
                let members = self.entries(depth + 1, END_OF_NODE)?;
                self.node_object(false, type_entry, None, members)?
            }
            START_OF_ARRAY => {
                let length = self.i64()?;
                let members = self.entries(depth + 1, END_OF_ARRAY)?;
                if length < 0
                    || usize::try_from(length).map_err(|_| parse_error("bad array length"))?
                        != members.len()
                {
                    return Err(parse_error("the array length does not match its elements"));
                }
                let mut children = Vec::with_capacity(members.len());
                for member in members {
                    children.push(self.doc.push(member.node)?);
                }
                Node {
                    value: Value::Array(children),
                    type_hint: Some(TypeHint::new(format!("{HINT_PREFIX}array"))),
                    lexeme: None,
                    span: None,
                    flags: NodeFlags::READ_ONLY,
                }
            }
            PRIMITIVE_ARRAY => {
                let count = self.i32()?;
                let bytes = self.i32()?;
                if count < 0 || bytes < 0 {
                    return Err(parse_error("a negative primitive array size"));
                }
                let total = usize::try_from(count)
                    .ok()
                    .and_then(|count| count.checked_mul(usize::try_from(bytes).ok()?))
                    .ok_or_else(|| parse_error("a primitive array size overflows"))?;
                let payload = self.take(total)?;
                Node {
                    value: Value::Bytes(payload.to_vec()),
                    type_hint: Some(TypeHint::new(format!(
                        "{HINT_PREFIX}primitive-array:{count}x{bytes}"
                    ))),
                    lexeme: None,
                    span: None,
                    flags: NodeFlags::READ_ONLY,
                }
            }
            NAMED_INTERNAL_REFERENCE | UNNAMED_INTERNAL_REFERENCE => Node {
                value: Value::Null,
                type_hint: Some(TypeHint::new(format!("{HINT_PREFIX}internal-reference"))),
                lexeme: None,
                span: None,
                flags: NodeFlags::READ_ONLY,
            },
            NAMED_EXTERNAL_REFERENCE_BY_INDEX | UNNAMED_EXTERNAL_REFERENCE_BY_INDEX => {
                let value = self.i32()?;
                Node {
                    value: Value::Int {
                        v: i128::from(value),
                        width: IntWidth::I32,
                    },
                    type_hint: Some(TypeHint::new(format!(
                        "{HINT_PREFIX}external-reference-index"
                    ))),
                    lexeme: None,
                    span: None,
                    flags: NodeFlags::READ_ONLY,
                }
            }
            NAMED_EXTERNAL_REFERENCE_BY_GUID | UNNAMED_EXTERNAL_REFERENCE_BY_GUID => {
                let payload = self.take(16)?.to_vec();
                Node {
                    value: Value::Bytes(payload),
                    type_hint: Some(TypeHint::new(format!(
                        "{HINT_PREFIX}external-reference-guid"
                    ))),
                    lexeme: None,
                    span: None,
                    flags: NodeFlags::READ_ONLY,
                }
            }
            NAMED_EXTERNAL_REFERENCE_BY_STRING | UNNAMED_EXTERNAL_REFERENCE_BY_STRING => {
                let text = self.string()?;
                Node {
                    value: Value::Str(text),
                    type_hint: Some(TypeHint::new(format!(
                        "{HINT_PREFIX}external-reference-string"
                    ))),
                    lexeme: None,
                    span: None,
                    flags: NodeFlags::READ_ONLY,
                }
            }
            NAMED_SBYTE | UNNAMED_SBYTE => self.int(IntWidth::I8, "sbyte")?,
            NAMED_BYTE | UNNAMED_BYTE => self.int(IntWidth::U8, "byte")?,
            NAMED_SHORT | UNNAMED_SHORT => self.int(IntWidth::I16, "short")?,
            NAMED_USHORT | UNNAMED_USHORT => self.int(IntWidth::U16, "ushort")?,
            NAMED_INT | UNNAMED_INT => self.int(IntWidth::I32, "int")?,
            NAMED_UINT | UNNAMED_UINT => self.int(IntWidth::U32, "uint")?,
            NAMED_LONG | UNNAMED_LONG => self.int(IntWidth::I64, "long")?,
            NAMED_ULONG | UNNAMED_ULONG => self.int(IntWidth::U64, "ulong")?,
            NAMED_FLOAT | UNNAMED_FLOAT => self.float(FloatWidth::F32, "float")?,
            NAMED_DOUBLE | UNNAMED_DOUBLE => self.float(FloatWidth::F64, "double")?,
            NAMED_DECIMAL | UNNAMED_DECIMAL => self.bytes(16, "decimal")?,
            NAMED_CHAR | UNNAMED_CHAR => self.char()?,
            NAMED_STRING | UNNAMED_STRING => self.str()?,
            NAMED_GUID | UNNAMED_GUID => self.bytes(16, "guid")?,
            NAMED_BOOLEAN | UNNAMED_BOOLEAN => {
                let value = self.byte()? != 0;
                Node {
                    value: Value::Bool(value),
                    type_hint: Some(TypeHint::new(format!("{HINT_PREFIX}bool"))),
                    lexeme: None,
                    span: self.span(self.pos - 1, self.pos),
                    flags: NodeFlags::empty(),
                }
            }
            NAMED_NULL | UNNAMED_NULL => Node {
                value: Value::Null,
                type_hint: Some(TypeHint::new(format!("{HINT_PREFIX}null"))),
                lexeme: None,
                span: None,
                flags: NodeFlags::READ_ONLY,
            },
            _ => return Err(parse_error("an unknown or misplaced entry code")),
        };
        Ok(Entry { key, node })
    }

    fn node_object(
        &mut self,
        reference: bool,
        type_entry: TypeEntry,
        id: Option<i32>,
        members: Vec<Entry>,
    ) -> Result<Node> {
        let mut children: Vec<(String, NodeId)> = Vec::new();
        match type_entry {
            TypeEntry::None => {}
            TypeEntry::Name(name) => {
                let child = self.doc.push(Node {
                    value: Value::Str(name),
                    type_hint: Some(TypeHint::new(format!("{HINT_PREFIX}type-name"))),
                    lexeme: None,
                    span: None,
                    flags: NodeFlags::READ_ONLY,
                })?;
                children.push(("$type".to_string(), child));
            }
            TypeEntry::Id(type_id) => {
                let child = self.doc.push(Node {
                    value: Value::Int {
                        v: i128::from(type_id),
                        width: IntWidth::I32,
                    },
                    type_hint: Some(TypeHint::new(format!("{HINT_PREFIX}type-id"))),
                    lexeme: None,
                    span: None,
                    flags: NodeFlags::READ_ONLY,
                })?;
                children.push(("$typeId".to_string(), child));
                if let Some(name) = self.types.get(&type_id) {
                    let child = self.doc.push(Node {
                        value: Value::Str(name.clone()),
                        type_hint: Some(TypeHint::new(format!("{HINT_PREFIX}type-name"))),
                        lexeme: None,
                        span: None,
                        flags: NodeFlags::READ_ONLY,
                    })?;
                    children.push(("$type".to_string(), child));
                }
            }
        }
        if reference {
            let reference_id = id.unwrap_or_default();
            let child = self.doc.push(Node {
                value: Value::Int {
                    v: i128::from(reference_id),
                    width: IntWidth::I32,
                },
                type_hint: Some(TypeHint::new(format!("{HINT_PREFIX}reference-id"))),
                lexeme: None,
                span: None,
                flags: NodeFlags::READ_ONLY,
            })?;
            children.push(("$id".to_string(), child));
        }
        for member in members {
            let child = self.doc.push(member.node)?;
            children.push((member.key, child));
        }
        Ok(Node {
            value: Value::Object(children),
            type_hint: Some(TypeHint::new(format!(
                "{HINT_PREFIX}{}",
                if reference { "reference" } else { "struct" }
            ))),
            lexeme: None,
            span: None,
            flags: NodeFlags::READ_ONLY,
        })
    }

    fn int(&mut self, width: IntWidth, kind: &str) -> Result<Node> {
        let start = self.pos;
        let value = match width {
            IntWidth::I8 => i128::from(self.byte()? as i8),
            IntWidth::U8 => i128::from(self.byte()?),
            IntWidth::I16 => {
                let raw = self.take(2)?;
                i128::from(i16::from_le_bytes([
                    *raw.first()
                        .ok_or_else(|| parse_error("short is truncated"))?,
                    *raw.get(1)
                        .ok_or_else(|| parse_error("short is truncated"))?,
                ]))
            }
            IntWidth::U16 => {
                let raw = self.take(2)?;
                i128::from(u16::from_le_bytes([
                    *raw.first()
                        .ok_or_else(|| parse_error("ushort is truncated"))?,
                    *raw.get(1)
                        .ok_or_else(|| parse_error("ushort is truncated"))?,
                ]))
            }
            IntWidth::I32 => i128::from(self.i32()?),
            IntWidth::U32 => {
                let raw = self.take(4)?;
                let mut bytes = [0_u8; 4];
                for (slot, byte) in bytes.iter_mut().zip(raw) {
                    *slot = *byte;
                }
                i128::from(u32::from_le_bytes(bytes))
            }
            IntWidth::I64 => i128::from(self.i64()?),
            IntWidth::U64 => {
                let raw = self.take(8)?;
                let mut bytes = [0_u8; 8];
                for (slot, byte) in bytes.iter_mut().zip(raw) {
                    *slot = *byte;
                }
                i128::from(u64::from_le_bytes(bytes))
            }
            IntWidth::Unbounded => {
                return Err(parse_error("Odin has no unbounded integer width"));
            }
        };
        Ok(Node {
            value: Value::Int { v: value, width },
            type_hint: Some(TypeHint::new(format!("{HINT_PREFIX}{kind}"))),
            lexeme: None,
            span: self.span(start, self.pos),
            flags: NodeFlags::empty(),
        })
    }

    fn float(&mut self, width: FloatWidth, kind: &str) -> Result<Node> {
        let start = self.pos;
        let value = match width {
            FloatWidth::F32 => {
                let raw = self.take(4)?;
                let mut bytes = [0_u8; 4];
                for (slot, byte) in bytes.iter_mut().zip(raw) {
                    *slot = *byte;
                }
                f64::from(f32::from_le_bytes(bytes))
            }
            FloatWidth::F64 => {
                let raw = self.take(8)?;
                let mut bytes = [0_u8; 8];
                for (slot, byte) in bytes.iter_mut().zip(raw) {
                    *slot = *byte;
                }
                f64::from_le_bytes(bytes)
            }
        };
        Ok(Node {
            value: Value::Float { v: value, width },
            type_hint: Some(TypeHint::new(format!("{HINT_PREFIX}{kind}"))),
            lexeme: None,
            span: self.span(start, self.pos),
            flags: NodeFlags::empty(),
        })
    }

    fn char(&mut self) -> Result<Node> {
        let start = self.pos;
        let raw = self.take(2)?;
        let unit = u16::from_le_bytes([
            *raw.first()
                .ok_or_else(|| parse_error("char is truncated"))?,
            *raw.get(1).ok_or_else(|| parse_error("char is truncated"))?,
        ]);
        Ok(Node {
            value: Value::Int {
                v: i128::from(unit),
                width: IntWidth::U16,
            },
            type_hint: Some(TypeHint::new(format!("{HINT_PREFIX}char"))),
            lexeme: None,
            span: self.span(start, self.pos),
            flags: NodeFlags::empty(),
        })
    }

    fn str(&mut self) -> Result<Node> {
        let start = self.pos;
        let text = self.string()?;
        Ok(Node {
            value: Value::Str(text),
            type_hint: Some(TypeHint::new(format!("{HINT_PREFIX}string"))),
            lexeme: None,
            span: self.span(start, self.pos),
            flags: NodeFlags::empty(),
        })
    }

    fn bytes(&mut self, count: usize, kind: &str) -> Result<Node> {
        let payload = self.take(count)?.to_vec();
        Ok(Node {
            value: Value::Bytes(payload),
            type_hint: Some(TypeHint::new(format!("{HINT_PREFIX}{kind}"))),
            lexeme: None,
            span: None,
            flags: NodeFlags::READ_ONLY,
        })
    }

    fn type_entry(&mut self) -> Result<TypeEntry> {
        match self.byte()? {
            TYPE_NAME => {
                let id = self.i32()?;
                let name = self.string()?;
                self.types.insert(id, name.clone());
                Ok(TypeEntry::Name(name))
            }
            TYPE_ID => {
                let id = self.i32()?;
                Ok(TypeEntry::Id(id))
            }
            UNNAMED_NULL => Ok(TypeEntry::None),
            _ => Err(parse_error("a node has no valid type entry")),
        }
    }

    fn string(&mut self) -> Result<String> {
        let wide = self.byte()? != 0;
        let length = self.i32()?;
        if length < 0 {
            return Err(parse_error("a negative string length"));
        }
        let units = usize::try_from(length).map_err(|_| parse_error("a string is too long"))?;
        if wide {
            let bytes = units
                .checked_mul(2)
                .ok_or_else(|| parse_error("a string is too long"))?;
            let raw = self.take(bytes)?;
            let (pairs, remainder) = raw.as_chunks::<2>();
            if !remainder.is_empty() {
                return Err(parse_error("a UTF-16 string has an odd byte length"));
            }
            let mut units = Vec::with_capacity(pairs.len());
            for pair in pairs {
                units.push(u16::from_le_bytes(*pair));
            }
            String::from_utf16(&units)
                .map_err(|_| parse_error("a string has an unpaired UTF-16 surrogate"))
        } else {
            let raw = self.take(units)?;
            String::from_utf8(raw.to_vec()).map_err(|_| parse_error("an 8-bit string is not UTF-8"))
        }
    }

    fn i32(&mut self) -> Result<i32> {
        let raw = self.take(4)?;
        let mut bytes = [0_u8; 4];
        for (slot, byte) in bytes.iter_mut().zip(raw) {
            *slot = *byte;
        }
        Ok(i32::from_le_bytes(bytes))
    }

    fn i64(&mut self) -> Result<i64> {
        let raw = self.take(8)?;
        let mut bytes = [0_u8; 8];
        for (slot, byte) in bytes.iter_mut().zip(raw) {
            *slot = *byte;
        }
        Ok(i64::from_le_bytes(bytes))
    }

    fn byte(&mut self) -> Result<u8> {
        let byte = *self
            .input
            .get(self.pos)
            .ok_or_else(|| parse_error("the stream is truncated"))?;
        self.pos += 1;
        Ok(byte)
    }

    fn take(&mut self, count: usize) -> Result<&[u8]> {
        let end = self
            .pos
            .checked_add(count)
            .ok_or_else(|| parse_error("an entry size overflows"))?;
        let slice = self
            .input
            .get(self.pos..end)
            .ok_or_else(|| parse_error("the stream is truncated"))?;
        self.pos = end;
        Ok(slice)
    }

    fn span(&self, start: usize, end: usize) -> Option<ByteSpan> {
        Some(ByteSpan {
            start: len_u64(start),
            end: len_u64(end),
        })
    }
}

fn parse_error(message: &str) -> Error {
    Error::new(ODIN_PARSE, message)
}
