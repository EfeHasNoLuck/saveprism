//! Parses the MS-NRBF record stream into a [`SaveDoc`]. No type is resolved, no method is
//! called: each record becomes a node and object ids become references between nodes.

use std::collections::HashMap;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    ByteSpan, DecodeCtx, Error, ErrorCode, FloatWidth, IntWidth, Node, NodeFlags, NodeId, Params,
    Result, SaveDoc, TypeHint, Value, codes,
};

use super::{HINT_PREFIX, MAX_NESTING, NRBF_BAD_REF, NRBF_PARSE, NRBF_UNSUPPORTED};

/// Nodes any file may have, however small; above it the count is capped by the input size.
const MIN_NODE_BUDGET: u64 = 65_536;
const HEADER: u8 = 0;
const CLASS_WITH_ID: u8 = 1;
const SYSTEM_CLASS_WITH_MEMBERS: u8 = 2;
const CLASS_WITH_MEMBERS: u8 = 3;
const SYSTEM_CLASS_WITH_MEMBERS_AND_TYPES: u8 = 4;
const CLASS_WITH_MEMBERS_AND_TYPES: u8 = 5;
const BINARY_OBJECT_STRING: u8 = 6;
const BINARY_ARRAY: u8 = 7;
const MEMBER_PRIMITIVE_TYPED: u8 = 8;
const MEMBER_REFERENCE: u8 = 9;
const OBJECT_NULL: u8 = 10;
const MESSAGE_END: u8 = 11;
const BINARY_LIBRARY: u8 = 12;
const OBJECT_NULL_MULTIPLE_256: u8 = 13;
const OBJECT_NULL_MULTIPLE: u8 = 14;
const ARRAY_SINGLE_PRIMITIVE: u8 = 15;
const ARRAY_SINGLE_OBJECT: u8 = 16;
const ARRAY_SINGLE_STRING: u8 = 17;
const METHOD_CALL: u8 = 21;
const METHOD_RETURN: u8 = 22;

const SINGLE: u8 = 0;
const JAGGED: u8 = 1;
const RECTANGULAR: u8 = 2;
const SINGLE_OFFSET: u8 = 3;
const JAGGED_OFFSET: u8 = 4;
const RECTANGULAR_OFFSET: u8 = 5;

/// What a class member holds: an inline primitive or a nested record.
#[derive(Clone, Copy, Debug)]
enum MemberKind {
    Primitive(u8),
    Record,
}

/// What `ClassWithId` reuses from an earlier class object.
#[derive(Clone, Debug)]
struct ClassMeta {
    name: String,
    library: Option<i32>,
    names: Vec<String>,
    kinds: Vec<MemberKind>,
}

struct PendingClass {
    node: NodeId,
    meta: ClassMeta,
    entries: Vec<(String, NodeId)>,
}

/// Reads a whole stream: header, records, references, then the graph root.
pub(super) fn read_all(input: &[u8], ctx: &DecodeCtx<'_>) -> Result<(SaveDoc, Params)> {
    let mut reader = Reader {
        input,
        pos: 0,
        doc: ctx.new_doc(),
        ctx,
        objects: HashMap::new(),
        metadata: HashMap::new(),
        libraries: HashMap::new(),
        pending_classes: Vec::new(),
        pending_refs: Vec::new(),
        ref_counts: HashMap::new(),
        strings: Vec::new(),
        root: 0,
    };
    reader.read_header()?;
    reader.read_stream()?;
    reader.finish()?;
    let mut params = Params::new();
    params.set("original", input.to_vec());
    let doc = reader.doc;
    Ok((doc, params))
}

struct Reader<'i, 'c, 'l> {
    input: &'i [u8],
    pos: usize,
    doc: SaveDoc,
    ctx: &'c DecodeCtx<'l>,
    /// Object id to the node that stands for it.
    objects: HashMap<i32, NodeId>,
    /// Class metadata of every class object, reused by `ClassWithId`.
    metadata: HashMap<i32, ClassMeta>,
    /// Binary library id to assembly name.
    libraries: HashMap<i32, String>,
    pending_classes: Vec<PendingClass>,
    /// `MemberReference` nodes whose target may not be defined yet.
    pending_refs: Vec<(NodeId, i32)>,
    /// How many `MemberReference` records name an object id.
    ref_counts: HashMap<i32, u32>,
    strings: Vec<(i32, NodeId)>,
    root: i32,
}

impl<'i> Reader<'i, '_, '_> {
    fn error(&self, code: ErrorCode, message: impl Into<String>, at: usize) -> Error {
        Error::new(code, message).with_offset(len_u64(at))
    }

    fn parse(&self, message: impl Into<String>) -> Error {
        self.error(NRBF_PARSE, message, self.pos)
    }

    fn truncated(&self) -> Error {
        self.parse("the NRBF data ends too early (truncated file)")
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
        self.input
            .get(start..self.pos)
            .ok_or_else(|| self.truncated())
    }

    fn i32(&mut self) -> Result<i32> {
        let bytes = self.take(4)?;
        let Ok(bytes) = <[u8; 4]>::try_from(bytes) else {
            return Err(self.truncated());
        };
        Ok(i32::from_le_bytes(bytes))
    }

    fn u32(&mut self) -> Result<u32> {
        let bytes = self.take(4)?;
        let Ok(bytes) = <[u8; 4]>::try_from(bytes) else {
            return Err(self.truncated());
        };
        Ok(u32::from_le_bytes(bytes))
    }

    fn i16(&mut self) -> Result<i16> {
        let bytes = self.take(2)?;
        let Ok(bytes) = <[u8; 2]>::try_from(bytes) else {
            return Err(self.truncated());
        };
        Ok(i16::from_le_bytes(bytes))
    }

    fn u16(&mut self) -> Result<u16> {
        let bytes = self.take(2)?;
        let Ok(bytes) = <[u8; 2]>::try_from(bytes) else {
            return Err(self.truncated());
        };
        Ok(u16::from_le_bytes(bytes))
    }

    fn i64(&mut self) -> Result<i64> {
        let bytes = self.take(8)?;
        let Ok(bytes) = <[u8; 8]>::try_from(bytes) else {
            return Err(self.truncated());
        };
        Ok(i64::from_le_bytes(bytes))
    }

    fn u64(&mut self) -> Result<u64> {
        let bytes = self.take(8)?;
        let Ok(bytes) = <[u8; 8]>::try_from(bytes) else {
            return Err(self.truncated());
        };
        Ok(u64::from_le_bytes(bytes))
    }

    fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_bits(self.u32()?))
    }

    fn f64(&mut self) -> Result<f64> {
        Ok(f64::from_bits(self.u64()?))
    }

    /// A non-negative count from an `Int32`.
    fn count(&mut self, what: &str) -> Result<usize> {
        let value = self.i32()?;
        let count = usize::try_from(value)
            .map_err(|_| self.parse(format!("{what} is negative ({value})")))?;
        if len_u64(count) > self.ctx.limits().max_nodes {
            return Err(Error::new(
                codes::CORE_LIMIT_NODES,
                format!("{what} {count} is above the node limit"),
            ));
        }
        Ok(count)
    }

    fn check_depth(&self, depth: usize) -> Result<()> {
        self.ctx.check_depth(depth)?;
        if depth > MAX_NESTING {
            return Err(self.parse(format!(
                "nesting depth {depth} is above the NRBF limit of {MAX_NESTING}"
            )));
        }
        Ok(())
    }

    /// A 7-bit encoded length (at most five bytes, at most `i32::MAX`).
    fn lp_len(&mut self) -> Result<usize> {
        let mut value: u64 = 0;
        for shift in 0..5 {
            let byte = self.byte()?;
            value |= u64::from(byte & 0x7F) << (7 * shift);
            if byte & 0x80 == 0 {
                if value > i32::MAX as u64 {
                    return Err(self.parse("a length prefix is larger than i32::MAX"));
                }
                return usize::try_from(value)
                    .map_err(|_| self.parse("a length prefix does not fit a usize"));
            }
        }
        Err(self.parse("a length prefix uses more than five bytes"))
    }

    fn lp_string(&mut self) -> Result<String> {
        let length = self.lp_len()?;
        let bytes = self.take(length)?;
        std::str::from_utf8(bytes)
            .map(str::to_string)
            .map_err(|_| self.parse("a length-prefixed string is not valid UTF-8"))
    }

    /// One UTF-8 encoded character (the `Char` primitive).
    fn char(&mut self) -> Result<String> {
        let first = self.peek().ok_or_else(|| self.truncated())?;
        let length = match first {
            0x00..=0x7F => 1,
            0xC0..=0xDF => 2,
            0xE0..=0xEF => 3,
            0xF0..=0xF7 => 4,
            _ => return Err(self.parse("a Char value is not valid UTF-8")),
        };
        let bytes = self.take(length)?;
        let text = std::str::from_utf8(bytes)
            .map_err(|_| self.parse("a Char value is not valid UTF-8"))?;
        let mut chars = text.chars();
        let Some(ch) = chars.next() else {
            return Err(self.parse("a Char value is empty"));
        };
        if chars.next().is_some() {
            return Err(self.parse("a Char value holds more than one character"));
        }
        Ok(ch.to_string())
    }

    fn hint(text: &str) -> TypeHint {
        TypeHint::new(format!("{HINT_PREFIX}{text}"))
    }

    fn scalar(
        &mut self,
        value: Value,
        hint: &str,
        span: Option<(usize, usize)>,
        read_only: bool,
    ) -> Result<NodeId> {
        let mut node = Node::new(value);
        node.type_hint = Some(Self::hint(hint));
        if let Some((start, end)) = span {
            node.span = Some(ByteSpan {
                start: len_u64(start),
                end: len_u64(end),
            });
        }
        if read_only {
            node.flags.insert(NodeFlags::READ_ONLY);
        }
        self.doc.push(node)
    }

    fn nulls(&mut self, count: usize) -> Result<Vec<NodeId>> {
        if count == 0 {
            return Err(self.parse("an ObjectNullMultiple record covers no member"));
        }
        // A five-byte record can stand for billions of nulls, one node each: cap the nodes by the
        // input size (like a decompression ratio), so a tiny file cannot fill the memory.
        let budget = len_u64(self.input.len())
            .saturating_mul(self.ctx.limits().max_ratio)
            .max(MIN_NODE_BUDGET)
            .min(self.ctx.limits().max_nodes);
        if len_u64(self.doc.len().saturating_add(count)) > budget {
            return Err(Error::new(
                codes::CORE_LIMIT_NODES,
                format!("an ObjectNullMultiple record of {count} nulls is too large for this file"),
            ));
        }
        let mut nodes = Vec::new();
        for _ in 0..count {
            nodes.push(self.scalar(Value::Null, "null", None, true)?);
        }
        Ok(nodes)
    }

    fn read_header(&mut self) -> Result<()> {
        let at = self.pos;
        let record = self.byte()?;
        if record != HEADER {
            return Err(Error::new(
                NRBF_PARSE,
                "not .NET BinaryFormatter data (no SerializedStreamHeader)",
            )
            .with_offset(len_u64(at)));
        }
        self.root = self.i32()?;
        let _header_id = self.i32()?;
        let major = self.i32()?;
        let minor = self.i32()?;
        if major != 1 || minor != 0 {
            return Err(Error::new(
                NRBF_UNSUPPORTED,
                format!("NRBF version {major}.{minor} is not supported (only 1.0)"),
            )
            .with_offset(len_u64(at)));
        }
        Ok(())
    }

    /// Records after the header, until MessageEnd or the end of input.
    fn read_stream(&mut self) -> Result<()> {
        while let Some(record) = self.peek() {
            match record {
                MESSAGE_END => {
                    self.pos += 1;
                    break;
                }
                BINARY_LIBRARY => {
                    self.pos += 1;
                    self.read_binary_library()?;
                }
                HEADER => {
                    return Err(self.parse("a second SerializedStreamHeader record"));
                }
                METHOD_CALL | METHOD_RETURN => {
                    return Err(Error::new(
                        NRBF_UNSUPPORTED,
                        "NRBF method calls and returns are not supported in a save file",
                    )
                    .with_offset(len_u64(self.pos)));
                }
                _ => {
                    self.read_value_record(1)?;
                }
            }
        }
        Ok(())
    }

    fn read_binary_library(&mut self) -> Result<()> {
        let id = self.i32()?;
        let name = self.lp_string()?;
        self.libraries.insert(id, name);
        Ok(())
    }

    /// Reserves a node id for an object before its members are read, so the object can be
    /// referenced from inside itself or from a forward `MemberReference`.
    fn reserve_object(&mut self, object_id: i32) -> Result<NodeId> {
        if self.objects.contains_key(&object_id) {
            return Err(self.parse(format!("object id {object_id} is defined twice")));
        }
        let node = self.doc.reserve()?;
        self.objects.insert(object_id, node);
        Ok(node)
    }

    fn finish_array(&mut self, node: NodeId, children: Vec<NodeId>) -> Result<()> {
        let mut value = Node::new(Value::Array(children));
        value.type_hint = Some(Self::hint("array"));
        self.doc.finish(node, value)
    }

    /// Reads one record that stands for a value. The result has one node, except for the
    /// null-multiple records, which cover several members at once.
    fn read_value_record(&mut self, depth: usize) -> Result<Vec<NodeId>> {
        self.check_depth(depth)?;
        let at = self.pos;
        let record = self.byte()?;
        match record {
            CLASS_WITH_ID => {
                let node = self.read_class_with_id(depth)?;
                Ok(vec![node])
            }
            SYSTEM_CLASS_WITH_MEMBERS
            | CLASS_WITH_MEMBERS
            | SYSTEM_CLASS_WITH_MEMBERS_AND_TYPES
            | CLASS_WITH_MEMBERS_AND_TYPES => {
                let node = self.read_class(record, depth)?;
                Ok(vec![node])
            }
            BINARY_OBJECT_STRING => {
                let node = self.read_binary_object_string()?;
                Ok(vec![node])
            }
            BINARY_ARRAY => {
                let node = self.read_binary_array(depth)?;
                Ok(vec![node])
            }
            MEMBER_PRIMITIVE_TYPED => {
                let ptype = self.primitive_type()?;
                let node = self.read_primitive(ptype)?;
                Ok(vec![node])
            }
            MEMBER_REFERENCE => {
                let object_id = self.i32()?;
                let node = self.doc.reserve()?;
                *self.ref_counts.entry(object_id).or_insert(0) += 1;
                self.pending_refs.push((node, object_id));
                Ok(vec![node])
            }
            OBJECT_NULL => Ok(vec![self.scalar(Value::Null, "null", None, true)?]),
            OBJECT_NULL_MULTIPLE_256 => {
                let count = usize::from(self.byte()?);
                self.nulls(count)
            }
            OBJECT_NULL_MULTIPLE => {
                let count = self.count("ObjectNullMultiple count")?;
                self.nulls(count)
            }
            ARRAY_SINGLE_PRIMITIVE => {
                let node = self.read_array_single_primitive()?;
                Ok(vec![node])
            }
            ARRAY_SINGLE_OBJECT | ARRAY_SINGLE_STRING => {
                let node = self.read_array_single_records(depth)?;
                Ok(vec![node])
            }
            HEADER => Err(self.parse("a SerializedStreamHeader record inside a value")),
            BINARY_LIBRARY => Err(self.parse("a BinaryLibrary record inside a value")),
            MESSAGE_END => Err(self.parse("a MessageEnd record inside a value")),
            METHOD_CALL | METHOD_RETURN => Err(Error::new(
                NRBF_UNSUPPORTED,
                "NRBF method calls and returns are not supported in a save file",
            )
            .with_offset(len_u64(at))),
            other => Err(Error::new(
                NRBF_UNSUPPORTED,
                format!("unknown NRBF record type {other}"),
            )
            .with_offset(len_u64(at))),
        }
    }

    fn read_binary_object_string(&mut self) -> Result<NodeId> {
        let object_id = self.i32()?;
        if self.objects.contains_key(&object_id) {
            return Err(self.parse(format!("object id {object_id} is defined twice")));
        }
        let start = self.pos;
        let text = self.lp_string()?;
        let end = self.pos;
        let node = self.scalar(Value::Str(text), "string", Some((start, end)), false)?;
        self.objects.insert(object_id, node);
        self.strings.push((object_id, node));
        Ok(node)
    }

    /// `ClassInfo`: object id, class name, member count and member names.
    fn read_class_info(&mut self) -> Result<(i32, String, Vec<String>)> {
        let object_id = self.i32()?;
        let name = self.lp_string()?;
        let count = self.count("member count")?;
        // Each member name needs at least its length byte.
        if count > self.remaining() {
            return Err(self.truncated());
        }
        let mut names = Vec::new();
        for _ in 0..count {
            names.push(self.lp_string()?);
        }
        Ok((object_id, name, names))
    }

    fn primitive_type(&mut self) -> Result<u8> {
        let at = self.pos;
        let ptype = self.byte()?;
        if matches!(
            ptype,
            1 | 2 | 3 | 5 | 6 | 7 | 8 | 9 | 10 | 11 | 12 | 13 | 14 | 15 | 16 | 17 | 18
        ) {
            Ok(ptype)
        } else {
            Err(Error::new(
                NRBF_UNSUPPORTED,
                format!("unknown NRBF primitive type {ptype}"),
            )
            .with_offset(len_u64(at)))
        }
    }

    /// `MemberTypeInfo`: the binary type of every member, then the additional information.
    fn read_member_types(&mut self, count: usize) -> Result<Vec<MemberKind>> {
        if count > self.remaining() {
            return Err(self.truncated());
        }
        let mut types = Vec::new();
        for _ in 0..count {
            types.push(self.byte()?);
        }
        let mut kinds = Vec::new();
        for binary_type in types {
            let kind = match binary_type {
                // Primitive: an inline value of the given primitive type.
                0 => MemberKind::Primitive(self.primitive_type()?),
                // PrimitiveArray: a BinaryArray record (its own record carries the type).
                7 => {
                    self.primitive_type()?;
                    MemberKind::Record
                }
                // SystemClass: a type name follows.
                3 => {
                    self.lp_string()?;
                    MemberKind::Record
                }
                // Class: a type name and a library id follow.
                4 => {
                    self.lp_string()?;
                    self.i32()?;
                    MemberKind::Record
                }
                // String, Object, ObjectArray, StringArray: no additional information.
                1 | 2 | 5 | 6 => MemberKind::Record,
                other => {
                    return Err(Error::new(
                        NRBF_UNSUPPORTED,
                        format!("unknown NRBF binary type {other}"),
                    )
                    .with_offset(len_u64(self.pos)));
                }
            };
            kinds.push(kind);
        }
        Ok(kinds)
    }

    fn read_member_values(
        &mut self,
        names: &[String],
        kinds: &[MemberKind],
        depth: usize,
    ) -> Result<Vec<NodeId>> {
        let mut values = Vec::new();
        while values.len() < names.len() {
            let index = values.len();
            match kinds.get(index).copied().unwrap_or(MemberKind::Record) {
                MemberKind::Primitive(ptype) => {
                    values.push(self.read_primitive(ptype)?);
                }
                MemberKind::Record => {
                    let nodes = self.read_value_record(depth + 1)?;
                    if nodes.is_empty() {
                        return Err(self.parse("a record covers no class member"));
                    }
                    if nodes.len() > names.len() - index {
                        return Err(self.parse("a null-multiple record covers too many members"));
                    }
                    if nodes.len() > 1
                        && kinds.get(index..index + nodes.len()).is_some_and(|kinds| {
                            kinds
                                .iter()
                                .any(|kind| matches!(kind, MemberKind::Primitive(_)))
                        })
                    {
                        return Err(self.parse("a null-multiple record covers a primitive member"));
                    }
                    values.extend(nodes);
                }
            }
        }
        Ok(values)
    }

    fn read_class(&mut self, record: u8, depth: usize) -> Result<NodeId> {
        let (object_id, name, names) = self.read_class_info()?;
        let (kinds, library) = match record {
            SYSTEM_CLASS_WITH_MEMBERS => (vec![MemberKind::Record; names.len()], None),
            CLASS_WITH_MEMBERS => {
                let library = self.i32()?;
                (vec![MemberKind::Record; names.len()], Some(library))
            }
            SYSTEM_CLASS_WITH_MEMBERS_AND_TYPES => (self.read_member_types(names.len())?, None),
            CLASS_WITH_MEMBERS_AND_TYPES => {
                let kinds = self.read_member_types(names.len())?;
                let library = self.i32()?;
                (kinds, Some(library))
            }
            _ => return Err(self.parse("not a class record")),
        };
        let node = self.reserve_object(object_id)?;
        let meta = ClassMeta {
            name: name.clone(),
            library,
            names: names.clone(),
            kinds: kinds.clone(),
        };
        self.metadata.insert(object_id, meta.clone());
        let values = self.read_member_values(&names, &kinds, depth)?;
        let entries = names.into_iter().zip(values).collect();
        self.pending_classes.push(PendingClass {
            node,
            meta,
            entries,
        });
        Ok(node)
    }

    fn read_class_with_id(&mut self, depth: usize) -> Result<NodeId> {
        let object_id = self.i32()?;
        let metadata_id = self.i32()?;
        let meta = self.metadata.get(&metadata_id).cloned().ok_or_else(|| {
            self.error(
                NRBF_BAD_REF,
                format!("ClassWithId names unknown class metadata {metadata_id}"),
                self.pos,
            )
        })?;
        let node = self.reserve_object(object_id)?;
        self.metadata.insert(object_id, meta.clone());
        let values = self.read_member_values(&meta.names, &meta.kinds, depth)?;
        let entries = meta.names.iter().cloned().zip(values).collect();
        self.pending_classes.push(PendingClass {
            node,
            meta,
            entries,
        });
        Ok(node)
    }

    fn read_primitive(&mut self, ptype: u8) -> Result<NodeId> {
        let start = self.pos;
        let hint = format!("primitive:{ptype}");
        match ptype {
            1 => {
                let value = self.byte()? != 0;
                self.scalar(Value::Bool(value), &hint, Some((start, self.pos)), false)
            }
            2 => {
                let value = self.byte()?;
                self.scalar(
                    Value::Int {
                        v: i128::from(value),
                        width: IntWidth::U8,
                    },
                    &hint,
                    Some((start, self.pos)),
                    false,
                )
            }
            3 => {
                let value = self.char()?;
                self.scalar(Value::Str(value), &hint, Some((start, self.pos)), false)
            }
            5 => {
                let length = self.lp_len()?;
                let bytes = self.take(length)?.to_vec();
                self.scalar(Value::Bytes(bytes), &hint, None, true)
            }
            6 => {
                let value = self.f64()?;
                self.scalar(
                    Value::Float {
                        v: value,
                        width: FloatWidth::F64,
                    },
                    &hint,
                    Some((start, self.pos)),
                    false,
                )
            }
            7 => {
                let value = self.i16()?;
                self.scalar(
                    Value::Int {
                        v: i128::from(value),
                        width: IntWidth::I16,
                    },
                    &hint,
                    Some((start, self.pos)),
                    false,
                )
            }
            8 => {
                let value = self.i32()?;
                self.scalar(
                    Value::Int {
                        v: i128::from(value),
                        width: IntWidth::I32,
                    },
                    &hint,
                    Some((start, self.pos)),
                    false,
                )
            }
            9 => {
                let value = self.i64()?;
                self.scalar(
                    Value::Int {
                        v: i128::from(value),
                        width: IntWidth::I64,
                    },
                    &hint,
                    Some((start, self.pos)),
                    false,
                )
            }
            10 => {
                let value = self.byte()? as i8;
                self.scalar(
                    Value::Int {
                        v: i128::from(value),
                        width: IntWidth::I8,
                    },
                    &hint,
                    Some((start, self.pos)),
                    false,
                )
            }
            11 => {
                let value = self.f32()?;
                self.scalar(
                    Value::Float {
                        v: f64::from(value),
                        width: FloatWidth::F32,
                    },
                    &hint,
                    Some((start, self.pos)),
                    false,
                )
            }
            12 => {
                let value = self.i64()?;
                self.scalar(
                    Value::Int {
                        v: i128::from(value),
                        width: IntWidth::I64,
                    },
                    &hint,
                    None,
                    true,
                )
            }
            13 => {
                let value = self.u64()?;
                self.scalar(
                    Value::Int {
                        v: i128::from(value),
                        width: IntWidth::U64,
                    },
                    &hint,
                    None,
                    true,
                )
            }
            14 => {
                let value = self.u16()?;
                self.scalar(
                    Value::Int {
                        v: i128::from(value),
                        width: IntWidth::U16,
                    },
                    &hint,
                    Some((start, self.pos)),
                    false,
                )
            }
            15 => {
                let value = self.u32()?;
                self.scalar(
                    Value::Int {
                        v: i128::from(value),
                        width: IntWidth::U32,
                    },
                    &hint,
                    Some((start, self.pos)),
                    false,
                )
            }
            16 => {
                let value = self.u64()?;
                self.scalar(
                    Value::Int {
                        v: i128::from(value),
                        width: IntWidth::U64,
                    },
                    &hint,
                    Some((start, self.pos)),
                    false,
                )
            }
            17 => self.scalar(Value::Null, &hint, None, true),
            18 => {
                let value = self.lp_string()?;
                self.scalar(Value::Str(value), &hint, Some((start, self.pos)), false)
            }
            other => Err(Error::new(
                NRBF_UNSUPPORTED,
                format!("unknown NRBF primitive type {other}"),
            )
            .with_offset(len_u64(start))),
        }
    }

    /// An array's element type: MS-NRBF forbids Null (17) and String (18) here, and a Null
    /// element takes no bytes, so a tiny file could ask for millions of nodes.
    fn array_primitive_type(&mut self) -> Result<u8> {
        let at = self.pos;
        let ptype = self.primitive_type()?;
        if matches!(ptype, 17 | 18) {
            return Err(self
                .parse(format!("an array of primitive type {ptype}"))
                .with_offset(len_u64(at)));
        }
        Ok(ptype)
    }

    fn read_array_single_primitive(&mut self) -> Result<NodeId> {
        let object_id = self.i32()?;
        let length = self.count("array length")?;
        let ptype = self.array_primitive_type()?;
        let node = self.reserve_object(object_id)?;
        let mut children = Vec::new();
        for _ in 0..length {
            children.push(self.read_primitive(ptype)?);
        }
        self.finish_array(node, children)?;
        Ok(node)
    }

    fn read_array_single_records(&mut self, depth: usize) -> Result<NodeId> {
        let object_id = self.i32()?;
        let length = self.count("array length")?;
        let node = self.reserve_object(object_id)?;
        let mut children = Vec::new();
        while children.len() < length {
            let nodes = self.read_value_record(depth + 1)?;
            if nodes.is_empty() || nodes.len() > length - children.len() {
                return Err(self.parse("an array has more elements than its length"));
            }
            children.extend(nodes);
        }
        self.finish_array(node, children)?;
        Ok(node)
    }

    fn read_binary_array(&mut self, depth: usize) -> Result<NodeId> {
        let object_id = self.i32()?;
        let array_type = self.byte()?;
        if !matches!(
            array_type,
            SINGLE | JAGGED | RECTANGULAR | SINGLE_OFFSET | JAGGED_OFFSET | RECTANGULAR_OFFSET
        ) {
            return Err(Error::new(
                NRBF_UNSUPPORTED,
                format!("unknown NRBF binary array type {array_type}"),
            )
            .with_offset(len_u64(self.pos)));
        }
        let rank = self.count("array rank")?;
        if rank == 0 {
            return Err(self.parse("a BinaryArray has rank 0"));
        }
        if rank > self.ctx.limits().max_depth.max(1) {
            return Err(Error::new(
                codes::CORE_LIMIT_DEPTH,
                format!("array rank {rank} is above the depth limit"),
            ));
        }
        let mut lengths = Vec::new();
        for _ in 0..rank {
            lengths.push(self.count("array length")?);
        }
        if matches!(
            array_type,
            SINGLE_OFFSET | JAGGED_OFFSET | RECTANGULAR_OFFSET
        ) {
            for _ in 0..rank {
                let _lower_bound = self.i32()?;
            }
        }
        let binary_type = self.byte()?;
        let primitive = match binary_type {
            0 => Some(self.array_primitive_type()?),
            7 => {
                self.primitive_type()?;
                None
            }
            3 => {
                self.lp_string()?;
                None
            }
            4 => {
                self.lp_string()?;
                self.i32()?;
                None
            }
            1 | 2 | 5 | 6 => None,
            other => {
                return Err(Error::new(
                    NRBF_UNSUPPORTED,
                    format!("unknown NRBF binary array type {other}"),
                )
                .with_offset(len_u64(self.pos)));
            }
        };
        let node = self.reserve_object(object_id)?;

        if matches!(array_type, JAGGED | JAGGED_OFFSET) {
            if rank != 1 {
                return Err(self.parse("a jagged BinaryArray has more than one dimension"));
            }
            let length = *lengths
                .first()
                .ok_or_else(|| self.parse("a jagged BinaryArray has no dimension"))?;
            let mut children = Vec::new();
            while children.len() < length {
                let nodes = self.read_value_record(depth + 1)?;
                if nodes.is_empty() || nodes.len() > length - children.len() {
                    return Err(self.parse("a jagged array has more elements than its length"));
                }
                children.extend(nodes);
            }
            self.finish_array(node, children)?;
            return Ok(node);
        }

        let mut total = 1_usize;
        for &length in &lengths {
            total = total
                .checked_mul(length)
                .ok_or_else(|| self.parse("the array size overflows"))?;
        }
        if len_u64(total) > self.ctx.limits().max_nodes {
            return Err(Error::new(
                codes::CORE_LIMIT_NODES,
                format!("array with {total} elements is above the node limit"),
            ));
        }
        let mut flat = Vec::new();
        for _ in 0..total {
            match primitive {
                Some(ptype) => flat.push(self.read_primitive(ptype)?),
                None => {
                    let nodes = self.read_value_record(depth + 1)?;
                    if nodes.len() != 1 {
                        return Err(self.parse("a null-multiple record inside a BinaryArray"));
                    }
                    flat.extend(nodes);
                }
            }
        }
        let children = self.nest(
            lengths,
            flat,
            matches!(array_type, RECTANGULAR | RECTANGULAR_OFFSET),
        )?;
        self.finish_array(node, children)?;
        Ok(node)
    }

    /// Turns the flat element list of a rectangular array into nested arrays. A single
    /// dimension stays flat; a zero-sized rectangular array becomes one empty array.
    fn nest(
        &mut self,
        lengths: Vec<usize>,
        mut level: Vec<NodeId>,
        rectangular: bool,
    ) -> Result<Vec<NodeId>> {
        if !rectangular || lengths.len() <= 1 || level.is_empty() {
            return Ok(level);
        }
        // The outermost dimension is the record's own list of elements; every inner dimension
        // becomes one more array node.
        for &dimension in lengths.get(1..).unwrap_or(&[]).iter().rev() {
            if dimension == 0 {
                return Err(self.parse("a rectangular array dimension is zero"));
            }
            if !level.len().is_multiple_of(dimension) {
                return Err(self.parse("a rectangular array has inconsistent dimensions"));
            }
            let mut next = Vec::new();
            for chunk in level.chunks(dimension) {
                let node = self.doc.reserve()?;
                let mut array = Node::new(Value::Array(chunk.to_vec()));
                array.type_hint = Some(Self::hint("array"));
                self.doc.finish(node, array)?;
                next.push(node);
            }
            level = next;
        }
        Ok(level)
    }

    /// Fills reserved references, finishes class objects now that every library name is known,
    /// marks shared strings read-only and points the document at the header's root object.
    fn finish(&mut self) -> Result<()> {
        for (node, object_id) in std::mem::take(&mut self.pending_refs) {
            let Some(target) = self.objects.get(&object_id).copied() else {
                return Err(Error::new(
                    NRBF_BAD_REF,
                    format!("MemberReference names unknown object id {object_id}"),
                ));
            };
            let mut value = Node::new(Value::Ref(target));
            value.type_hint = Some(Self::hint("ref"));
            value.flags.insert(NodeFlags::READ_ONLY);
            self.doc.finish(node, value)?;
        }

        if self.objects.is_empty() {
            return Err(Error::new(NRBF_PARSE, "the stream defines no object"));
        }

        let pending = std::mem::take(&mut self.pending_classes);
        for mut class in pending {
            if class
                .meta
                .name
                .starts_with("System.Collections.Generic.List`1")
                || class.meta.name.ends_with("List`1")
            {
                self.trim_list(&mut class.entries)?;
            } else if class
                .meta
                .name
                .starts_with("System.Collections.Generic.Dictionary`2")
                || class.meta.name.ends_with("Dictionary`2")
            {
                self.dereference_entry(&mut class.entries, "KeyValuePairs");
            }
            let library = class
                .meta
                .library
                .and_then(|id| self.libraries.get(&id).cloned());
            let hint = match library {
                Some(library) => format!("class:{}@{}", class.meta.name, library),
                None => format!("class:{}", class.meta.name),
            };
            let mut value = Node::new(Value::Object(class.entries));
            value.type_hint = Some(Self::hint(&hint));
            self.doc.finish(class.node, value)?;
        }

        let strings = std::mem::take(&mut self.strings);
        for (object_id, node) in strings {
            if self.ref_counts.get(&object_id).copied().unwrap_or(0) >= 2 {
                self.doc.set_flags(node, NodeFlags::READ_ONLY)?;
            }
        }

        let Some(root) = self.objects.get(&self.root).copied() else {
            return Err(Error::new(
                NRBF_BAD_REF,
                format!("the stream header names unknown root object {}", self.root),
            ));
        };
        self.doc.set_root(root);
        Ok(())
    }

    /// Replaces a `MemberReference` entry with the object it names. Used for the two generic
    /// collection members the UI shows as data (List items and Dictionary pairs).
    fn dereference_entry(&self, entries: &mut [(String, NodeId)], key: &str) -> Option<NodeId> {
        let (_, node) = entries.iter_mut().find(|(name, _)| name == key)?;
        if let Some(Value::Ref(target)) = self.doc.get(*node).map(|node| &node.value) {
            *node = *target;
        }
        Some(*node)
    }

    /// `System.Collections.Generic.List`1` keeps `_items` longer than `_size`; show only the
    /// live part. The full array remains reachable through references to it.
    fn trim_list(&mut self, entries: &mut [(String, NodeId)]) -> Result<()> {
        let Some(items) = self.dereference_entry(entries, "_items") else {
            return Ok(());
        };
        let size =
            entries
                .iter()
                .find(|(name, _)| name == "_size")
                .and_then(
                    |(_, node)| match self.doc.get(*node).map(|node| &node.value) {
                        Some(Value::Int { v, .. }) if *v >= 0 => usize::try_from(*v).ok(),
                        _ => None,
                    },
                );
        let Some(size) = size else {
            return Ok(());
        };
        let Some(index) = entries.iter().position(|(name, _)| name == "_items") else {
            return Ok(());
        };
        let Some(Node {
            value: Value::Array(children),
            type_hint,
            ..
        }) = self.doc.get(items).cloned()
        else {
            return Ok(());
        };
        if size >= children.len() {
            return Ok(());
        }
        let mut trimmed = Node::new(Value::Array(
            children.get(..size).unwrap_or(&children).to_vec(),
        ));
        trimmed.type_hint = type_hint;
        let node = self.doc.push(trimmed)?;
        if let Some(entry) = entries.get_mut(index) {
            entry.1 = node;
        }
        Ok(())
    }
}
