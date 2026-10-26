//! `format.godot-resource`: Godot binary resources (`.res`).
//!
//! Written from Godot's MIT-licensed `ResourceFormatLoaderBinary` / `ResourceFormatSaverBinary`
//! sources and the public binary-resource documentation. A file starts with `RSRC` (or `RSCC`
//! when Godot compressed it), an endianness flag, a 64-bit-real flag, the engine and format
//! versions, the resource type, the metadata offset, the string table and the external/internal
//! resource tables; each internal resource is a class name and a list of properties stored as
//! binary Variants with the resource format's own type ids.
//!
//! Nothing is instantiated or executed: resources, references and containers are data only.
//!
//! # Tree
//!
//! The root is an `Object` with `header`, `strings`, `external` and `resources`. Every internal
//! resource is `{ path, type, properties }`, so a saved value lives at
//! `/resources/<n>/properties/<name>`.
//!
//! # Editing
//!
//! Only fixed-width values are editable in place: booleans, 32/64-bit integers, 32/64-bit floats
//! and the components of math types. Strings, arrays, dictionaries, packed arrays, node paths,
//! object references and headers stay read-only because changing them would move every following
//! offset. Writing patches only changed values and copies every other byte, so an unchanged file
//! is written back exactly.
//!
//! # Compression
//!
//! `RSCC` files use `FileAccessCompressed`: a little-endian header with the mode, block size and
//! uncompressed length, a table of compressed block sizes, the blocks and a trailing `RSCC`
//! marker. Deflate, Zstandard and gzip blocks are decoded and compressed again; FastLZ and Brotli
//! are refused with a clear error.

use std::io::Read;
use std::sync::Arc;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    ByteSpan, Codec, Confidence, DecodeCtx, Decoded, Error, ErrorCode, FloatWidth, IntWidth,
    Limits, Node, NodeFlags, NodeId, ParamValue, Params, Registry, Result, SaveDoc, SniffCtx,
    TypeHint, Value, codes,
};

use super::{GODOT_PARSE, GODOT_WRITE};

/// The `RSCC` stream uses a compression mode this build cannot decode.
pub const GODOT_RESOURCE_COMPRESSION: ErrorCode = ErrorCode::new("SP-GODOT-004");
/// The binary resource header or a resource table is malformed.
pub const GODOT_RESOURCE_LAYOUT: ErrorCode = ErrorCode::new("SP-GODOT-005");

/// The `format.godot-resource` codec.
#[derive(Clone, Copy, Debug, Default)]
pub struct GodotResourceCodec;

/// Registers the binary-resource codec.
pub fn register(reg: &mut Registry) {
    reg.add_codec(Arc::new(GodotResourceCodec));
}

const RES_PREFIX: &str = "GODOT:res:";
const MAX_NESTING: usize = 4096;

/// Resource-format Variant type ids (different from the `Variant` stream ids).
mod kind {
    pub(super) const NIL: u32 = 1;
    pub(super) const BOOL: u32 = 2;
    pub(super) const INT: u32 = 3;
    pub(super) const FLOAT: u32 = 4;
    pub(super) const STRING: u32 = 5;
    pub(super) const VECTOR2: u32 = 10;
    pub(super) const RECT2: u32 = 11;
    pub(super) const VECTOR3: u32 = 12;
    pub(super) const PLANE: u32 = 13;
    pub(super) const QUATERNION: u32 = 14;
    pub(super) const AABB: u32 = 15;
    pub(super) const BASIS: u32 = 16;
    pub(super) const TRANSFORM3D: u32 = 17;
    pub(super) const TRANSFORM2D: u32 = 18;
    pub(super) const COLOR: u32 = 20;
    pub(super) const NODE_PATH: u32 = 22;
    pub(super) const RID: u32 = 23;
    pub(super) const OBJECT: u32 = 24;
    pub(super) const DICTIONARY: u32 = 26;
    pub(super) const ARRAY: u32 = 30;
    pub(super) const PACKED_BYTE: u32 = 31;
    pub(super) const PACKED_INT32: u32 = 32;
    pub(super) const PACKED_FLOAT32: u32 = 33;
    pub(super) const PACKED_STRING: u32 = 34;
    pub(super) const PACKED_VECTOR3: u32 = 35;
    pub(super) const PACKED_COLOR: u32 = 36;
    pub(super) const PACKED_VECTOR2: u32 = 37;
    pub(super) const INT64: u32 = 40;
    pub(super) const DOUBLE: u32 = 41;
    pub(super) const CALLABLE: u32 = 42;
    pub(super) const SIGNAL: u32 = 43;
    pub(super) const STRING_NAME: u32 = 44;
    pub(super) const VECTOR2I: u32 = 45;
    pub(super) const RECT2I: u32 = 46;
    pub(super) const VECTOR3I: u32 = 47;
    pub(super) const PACKED_INT64: u32 = 48;
    pub(super) const PACKED_FLOAT64: u32 = 49;
    pub(super) const VECTOR4: u32 = 50;
    pub(super) const VECTOR4I: u32 = 51;
    pub(super) const PROJECTION: u32 = 52;
}

impl Codec for GodotResourceCodec {
    fn id(&self) -> &'static str {
        "format.godot-resource"
    }

    fn sniff(&self, input: &[u8], _ctx: &SniffCtx<'_>) -> Confidence {
        if !input.starts_with(b"RSRC") && !input.starts_with(b"RSCC") {
            return Confidence::NONE;
        }
        match probe(input) {
            Ok(()) => Confidence::HIGH,
            Err(()) => Confidence::NONE,
        }
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<Decoded> {
        ctx.limits().check_input(len_u64(input.len()))?;
        let compressed = input.starts_with(b"RSCC");
        if !compressed && !input.starts_with(b"RSRC") {
            return Err(Error::new(
                GODOT_PARSE,
                "the file is not a Godot binary resource (no RSRC magic)",
            ));
        }
        let (inner, container) = if compressed {
            let (data, info) = decompress_rscc(input, ctx.limits())?;
            (data, Some(info))
        } else {
            (input.to_vec(), None)
        };
        let (doc, parsed) = {
            let mut reader = Reader::new(&inner, ctx.limits().max_depth, ctx.limits().max_nodes);
            let parsed = reader.parse()?;
            ctx.charge_nodes(len_u64(reader.doc.len()))?;
            (reader.doc, parsed)
        };
        let mut params = Params::new();
        params.set("original", input.to_vec());
        params.set("inner", inner);
        params.set("version", parsed.version);
        params.set("big_endian", i64::from(parsed.big_endian));
        if let Some(info) = container {
            params.set("compressed", 1i64);
            params.set("rscc_mode", i64::from(info.mode));
            params.set("rscc_block_size", i64::from(info.block_size));
        }
        Ok(Decoded { doc, params })
    }

    fn encode(&self, doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
        let Some(ParamValue::Bytes(original)) = params.get("original") else {
            return Err(Error::new(
                GODOT_WRITE,
                "the original Godot binary resource is missing",
            ));
        };
        let Some(ParamValue::Bytes(inner)) = params.get("inner") else {
            return Err(Error::new(
                GODOT_WRITE,
                "the decoded Godot binary resource is missing",
            ));
        };
        let big_endian = matches!(params.get("big_endian"), Some(ParamValue::Int(1)));
        let patched = patch(inner, doc, big_endian)?;
        if patched == *inner {
            return Ok(original.clone());
        }
        let compressed = matches!(params.get("compressed"), Some(ParamValue::Int(1)));
        if !compressed {
            return Ok(patched);
        }
        let mode = match params.get("rscc_mode") {
            Some(ParamValue::Int(value)) => u32::try_from(*value)
                .map_err(|_| Error::new(GODOT_WRITE, "invalid RSCC compression mode"))?,
            _ => return Err(Error::new(GODOT_WRITE, "RSCC compression mode is missing")),
        };
        let block_size = match params.get("rscc_block_size") {
            Some(ParamValue::Int(value)) => u32::try_from(*value)
                .map_err(|_| Error::new(GODOT_WRITE, "invalid RSCC block size"))?,
            _ => return Err(Error::new(GODOT_WRITE, "RSCC block size is missing")),
        };
        compress_rscc(&patched, mode, block_size)
    }
}

/// Writes changed fixed-width values into `inner`.
fn patch(inner: &[u8], doc: &SaveDoc, big_endian: bool) -> Result<Vec<u8>> {
    let mut patches = Vec::new();
    let mut stack = vec![doc.root()];
    let mut visited = 0usize;
    while let Some(id) = stack.pop() {
        visited = visited.saturating_add(1);
        if visited > doc.len().saturating_add(1) {
            return Err(Error::new(GODOT_WRITE, "cyclic Godot resource tree"));
        }
        let Some(node) = doc.get(id) else {
            continue;
        };
        stack.extend(doc.children(id));
        if let Value::Map(entries) = &node.value {
            stack.extend(entries.iter().map(|(key, _)| *key));
        }
        if node.lexeme.is_some() || node.flags.contains(NodeFlags::READ_ONLY) {
            continue;
        }
        let Some(hint) = node.type_hint.as_ref().map(TypeHint::as_str) else {
            continue;
        };
        let Some(suffix) = hint.strip_prefix(RES_PREFIX) else {
            continue;
        };
        let Some(span) = node.span else {
            continue;
        };
        let rendered = match (suffix, &node.value) {
            ("bool", Value::Bool(value)) => Some(encode_u32(u32::from(*value), big_endian)),
            ("int32", Value::Int { v, .. }) => Some(
                i32::try_from(*v)
                    .map_err(|_| Error::new(GODOT_WRITE, "integer does not fit 32 bits"))?
                    .to_le_bytes()
                    .to_vec(),
            ),
            ("int64", Value::Int { v, .. }) => Some(
                i64::try_from(*v)
                    .map_err(|_| Error::new(GODOT_WRITE, "integer does not fit 64 bits"))?
                    .to_le_bytes()
                    .to_vec(),
            ),
            ("float32", Value::Float { v, .. }) => Some((*v as f32).to_le_bytes().to_vec()),
            ("float64", Value::Float { v, .. }) => Some(v.to_le_bytes().to_vec()),
            _ => None,
        };
        let Some(mut rendered) = rendered else {
            continue;
        };
        if big_endian {
            match suffix {
                "int32" | "float32" => rendered.reverse(),
                "int64" | "float64" => {
                    let mut ordered = Vec::with_capacity(8);
                    ordered.extend(rendered.iter().rev());
                    rendered = ordered;
                }
                _ => {}
            }
        }
        let start = usize::try_from(span.start)
            .map_err(|_| Error::new(GODOT_WRITE, "invalid Godot resource span"))?;
        let end = usize::try_from(span.end)
            .map_err(|_| Error::new(GODOT_WRITE, "invalid Godot resource span"))?;
        if end > inner.len() || rendered.len() != end.saturating_sub(start) {
            return Err(Error::new(
                GODOT_WRITE,
                "a fixed-width Godot resource value does not fit its original span",
            ));
        }
        if inner.get(start..end) != Some(rendered.as_slice()) {
            patches.push((start, end, rendered));
        }
    }
    patches.sort_by_key(|(start, _, _)| *start);
    for pair in patches.windows(2) {
        if let (Some(a), Some(b)) = (pair.first(), pair.get(1))
            && a.1 > b.0
        {
            return Err(Error::new(
                GODOT_WRITE,
                "overlapping Godot resource value spans",
            ));
        }
    }
    let mut out = inner.to_vec();
    for (start, end, replacement) in patches.into_iter().rev() {
        out.splice(start..end, replacement);
    }
    Ok(out)
}

fn encode_u32(value: u32, big_endian: bool) -> Vec<u8> {
    if big_endian {
        value.to_be_bytes().to_vec()
    } else {
        value.to_le_bytes().to_vec()
    }
}

struct Reader<'a> {
    input: &'a [u8],
    pos: usize,
    big_endian: bool,
    real64: bool,
    version: u8,
    format: u32,
    depth: usize,
    max_depth: usize,
    strings: Vec<String>,
    doc: SaveDoc,
}

struct Parsed {
    version: i64,
    big_endian: bool,
}

impl<'a> Reader<'a> {
    fn new(input: &'a [u8], max_depth: usize, max_nodes: u64) -> Self {
        Self {
            input,
            pos: 0,
            big_endian: false,
            real64: false,
            version: 4,
            format: 0,
            depth: 0,
            max_depth: max_depth.min(MAX_NESTING),
            strings: Vec::new(),
            doc: SaveDoc::with_node_limit(max_nodes),
        }
    }

    fn fail(&self, message: &str) -> Error {
        Error::new(
            GODOT_RESOURCE_LAYOUT,
            format!("{message} at byte {}", self.pos),
        )
        .with_offset(len_u64(self.pos))
    }

    fn remaining(&self) -> usize {
        self.input.len().saturating_sub(self.pos)
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8]> {
        let end = self
            .pos
            .checked_add(count)
            .filter(|end| *end <= self.input.len())
            .ok_or_else(|| self.fail("truncated Godot binary resource"))?;
        let start = self.pos;
        let out = self
            .input
            .get(start..end)
            .ok_or_else(|| self.fail("invalid byte range"))?;
        self.pos = end;
        Ok(out)
    }

    fn u16(&mut self) -> Result<u16> {
        let bytes: [u8; 2] = self
            .take(2)?
            .try_into()
            .map_err(|_| self.fail("truncated u16"))?;
        Ok(if self.big_endian {
            u16::from_be_bytes(bytes)
        } else {
            u16::from_le_bytes(bytes)
        })
    }

    fn u32(&mut self) -> Result<u32> {
        let bytes: [u8; 4] = self
            .take(4)?
            .try_into()
            .map_err(|_| self.fail("truncated u32"))?;
        Ok(if self.big_endian {
            u32::from_be_bytes(bytes)
        } else {
            u32::from_le_bytes(bytes)
        })
    }

    fn u64(&mut self) -> Result<u64> {
        let bytes: [u8; 8] = self
            .take(8)?
            .try_into()
            .map_err(|_| self.fail("truncated u64"))?;
        Ok(if self.big_endian {
            u64::from_be_bytes(bytes)
        } else {
            u64::from_le_bytes(bytes)
        })
    }

    fn length(&mut self) -> Result<usize> {
        let at = self.pos;
        let value = self.u32()?;
        let length =
            usize::try_from(value).map_err(|_| self.fail("length does not fit this platform"))?;
        if length > self.remaining() {
            return Err(Error::new(
                GODOT_RESOURCE_LAYOUT,
                format!("length {length} is larger than the rest of the file"),
            )
            .with_offset(len_u64(at)));
        }
        Ok(length)
    }

    fn unicode_string(&mut self) -> Result<String> {
        let length = self.length()?;
        let bytes = self.take(length)?;
        let bytes = match bytes.iter().position(|byte| *byte == 0) {
            Some(end) => bytes.get(..end).unwrap_or_default(),
            None => bytes,
        };
        std::str::from_utf8(bytes)
            .map(str::to_owned)
            .map_err(|_| self.fail("Godot resource string is not UTF-8"))
    }

    fn string_ref(&mut self) -> Result<String> {
        let id = self.u32()?;
        if id & 0x8000_0000 != 0 {
            let length = usize::try_from(id & 0x7fff_ffff)
                .map_err(|_| self.fail("inline string length does not fit this platform"))?;
            let bytes = self.take(length)?;
            return std::str::from_utf8(bytes)
                .map(str::to_owned)
                .map_err(|_| self.fail("Godot resource string is not UTF-8"));
        }
        let index = usize::try_from(id).map_err(|_| self.fail("bad string index"))?;
        self.strings
            .get(index)
            .cloned()
            .ok_or_else(|| self.fail("string index outside the string table"))
    }

    fn add(
        &mut self,
        value: Value,
        hint: &str,
        start: usize,
        end: usize,
        editable: bool,
    ) -> Result<NodeId> {
        let mut node = Node::new(value);
        node.type_hint = Some(TypeHint::new(format!("{RES_PREFIX}{hint}")));
        node.span = Some(ByteSpan {
            start: len_u64(start),
            end: len_u64(end),
        });
        if editable {
            node.lexeme = Some("godot:original".into());
        } else {
            node.flags.insert(NodeFlags::READ_ONLY);
        }
        self.doc.push(node)
    }

    fn read_only(&mut self, value: Value, hint: &str, start: usize) -> Result<NodeId> {
        self.add(value, hint, start, self.pos, false)
    }

    fn parse(&mut self) -> Result<Parsed> {
        if self.take(4)? != b"RSRC" {
            return Err(Error::new(
                GODOT_PARSE,
                "the decompressed Godot resource has no RSRC magic",
            ));
        }
        self.big_endian = self.u32()? != 0;
        self.real64 = self.u32()? != 0;
        let major = self.u32()?;
        let minor = self.u32()?;
        self.format = self.u32()?;
        if major == 0 || self.format == 0 {
            return Err(self.fail("implausible Godot resource version"));
        }
        let new_layout = self.format >= 4;
        self.version = if new_layout { 4 } else { 3 };
        let resource_type = self.unicode_string()?;
        let metadata_offset = self.u64()?;
        let mut header_entries: Vec<(String, NodeId)> = Vec::new();
        let initial = vec![
            ("version".to_owned(), header_int(i64::from(major))),
            ("minor".to_owned(), header_int(i64::from(minor))),
            ("format".to_owned(), header_int(i64::from(self.format))),
            ("type".to_owned(), header_str(resource_type.clone())),
            (
                "metadata_offset".to_owned(),
                header_int(i64::try_from(metadata_offset).unwrap_or(i64::MAX)),
            ),
            ("big_endian".to_owned(), Value::Bool(self.big_endian)),
            ("real64".to_owned(), Value::Bool(self.real64)),
        ];
        for (key, value) in initial {
            let id = self.doc.push(read_only_node(value, "header"))?;
            header_entries.push((key, id));
        }
        let mut flags = 0u32;
        let mut script_class = String::new();
        if new_layout {
            flags = self.u32()?;
            let uid = self.u64()?;
            if flags & 8 != 0 {
                script_class = self.unicode_string()?;
            }
            for _ in 0..11 {
                self.u32()?;
            }
            self.real64 = flags & 4 != 0;
            let id = self
                .doc
                .push(read_only_node(header_int(i64::from(flags)), "header"))?;
            header_entries.push(("flags".to_owned(), id));
            let id = self.doc.push(read_only_node(
                Value::Int {
                    v: i128::from(uid),
                    width: IntWidth::U64,
                },
                "header",
            ))?;
            header_entries.push(("uid".to_owned(), id));
            if !script_class.is_empty() {
                let id = self
                    .doc
                    .push(read_only_node(header_str(script_class), "header"))?;
                header_entries.push(("script_class".to_owned(), id));
            }
        } else {
            for _ in 0..14 {
                self.u32()?;
            }
        }
        let string_count = usize::try_from(self.u32()?)
            .map_err(|_| self.fail("string table size does not fit this platform"))?;
        if string_count > self.remaining() / 4 {
            return Err(self.fail("implausible string table size"));
        }
        self.strings.reserve(string_count);
        for _ in 0..string_count {
            let text = self.unicode_string()?;
            self.strings.push(text);
        }
        let string_nodes = self.strings.clone();
        let external_count = usize::try_from(self.u32()?)
            .map_err(|_| self.fail("external resource count does not fit this platform"))?;
        if external_count > self.remaining() / 8 {
            return Err(self.fail("implausible external resource count"));
        }
        let mut externals = Vec::with_capacity(external_count);
        for _ in 0..external_count {
            let rtype = self.unicode_string()?;
            let path = self.unicode_string()?;
            let mut entries = vec![
                (
                    "type".to_owned(),
                    self.doc
                        .push(read_only_node(Value::Str(rtype), "external"))?,
                ),
                (
                    "path".to_owned(),
                    self.doc
                        .push(read_only_node(Value::Str(path), "external"))?,
                ),
            ];
            if new_layout && flags & 2 != 0 {
                let id = self.u64()?;
                entries.push((
                    "uid".to_owned(),
                    self.doc.push(read_only_node(
                        Value::Int {
                            v: i128::from(id),
                            width: IntWidth::U64,
                        },
                        "external",
                    ))?,
                ));
            }
            let wrapper = self
                .doc
                .push(read_only_node(Value::Object(entries), "external-entry"))?;
            externals.push(wrapper);
        }
        let internal_count = usize::try_from(self.u32()?)
            .map_err(|_| self.fail("internal resource count does not fit this platform"))?;
        if internal_count > self.remaining() / 12 {
            return Err(self.fail("implausible internal resource count"));
        }
        let mut internals = Vec::with_capacity(internal_count);
        for _ in 0..internal_count {
            let path = self.unicode_string()?;
            let offset = self.u64()?;
            internals.push((path, offset));
        }
        let mut resources = Vec::with_capacity(internals.len());
        for (path, offset) in internals {
            let offset = usize::try_from(offset)
                .map_err(|_| self.fail("internal resource offset does not fit this platform"))?;
            if offset > self.input.len() {
                return Err(self.fail("internal resource offset is outside the file"));
            }
            self.pos = offset;
            let class = self.unicode_string()?;
            let count = usize::try_from(self.u32()?)
                .map_err(|_| self.fail("property count does not fit this platform"))?;
            if count > self.remaining() / 8 {
                return Err(self.fail("implausible property count"));
            }
            let mut properties = Vec::with_capacity(count);
            for _ in 0..count {
                let name = self.string_ref()?;
                let value = self.variant(false)?;
                properties.push((name, value));
            }
            let properties = self
                .doc
                .push(read_only_node(Value::Object(properties), "properties"))?;
            let entries = vec![
                (
                    "path".to_owned(),
                    self.doc
                        .push(read_only_node(Value::Str(path), "resource"))?,
                ),
                (
                    "type".to_owned(),
                    self.doc
                        .push(read_only_node(Value::Str(class), "resource"))?,
                ),
                ("properties".to_owned(), properties),
            ];
            let wrapper = self
                .doc
                .push(read_only_node(Value::Object(entries), "resource-entry"))?;
            resources.push(wrapper);
        }
        let header_object = self
            .doc
            .push(read_only_node(Value::Object(header_entries), "header"))?;
        let strings_array = string_nodes
            .into_iter()
            .map(|text| {
                self.doc
                    .push(read_only_node(Value::Str(text), "string-table"))
            })
            .collect::<Result<Vec<_>>>()?;
        let strings = self.doc.push(read_only_node(
            Value::Array(strings_array),
            "string-table-list",
        ))?;
        let external = self
            .doc
            .push(read_only_node(Value::Array(externals), "external-list"))?;
        let resources = self
            .doc
            .push(read_only_node(Value::Array(resources), "resource-list"))?;
        let root_entries = vec![
            ("header".to_owned(), header_object),
            ("strings".to_owned(), strings),
            ("external".to_owned(), external),
            ("resources".to_owned(), resources),
        ];
        let root = self
            .doc
            .push(read_only_node(Value::Object(root_entries), "root"))?;
        self.doc.set_root(root);
        Ok(Parsed {
            version: i64::from(self.version),
            big_endian: self.big_endian,
        })
    }

    fn enter(&mut self) -> Result<()> {
        if self.depth >= self.max_depth {
            return Err(Error::new(
                codes::CORE_LIMIT_DEPTH,
                format!("Godot resource nesting limit of {MAX_NESTING} exceeded"),
            )
            .with_offset(len_u64(self.pos)));
        }
        self.depth += 1;
        Ok(())
    }

    fn variant(&mut self, locked: bool) -> Result<NodeId> {
        self.enter()?;
        let result = self.variant_inner(locked);
        self.depth -= 1;
        result
    }

    fn variant_inner(&mut self, locked: bool) -> Result<NodeId> {
        let start = self.pos;
        let tag = self.u32()?;
        match tag {
            kind::NIL => self.add(Value::Null, "null", start, self.pos, false),
            kind::BOOL => {
                let payload = self.pos;
                let value = self.u32()?;
                if value > 1 {
                    return Err(self.fail("invalid Godot boolean"));
                }
                self.add(Value::Bool(value == 1), "bool", payload, self.pos, !locked)
            }
            kind::INT => self.int32(locked),
            kind::INT64 => self.int64(locked),
            kind::FLOAT => self.real(locked),
            kind::DOUBLE => {
                let payload = self.pos;
                let value = f64::from_bits(self.u64()?);
                self.add(
                    Value::Float {
                        v: value,
                        width: FloatWidth::F64,
                    },
                    "float64",
                    payload,
                    self.pos,
                    !locked,
                )
            }
            kind::STRING => {
                let payload = self.pos;
                let text = self.unicode_string()?;
                self.add(Value::Str(text), "string", payload, self.pos, false)
            }
            kind::STRING_NAME => {
                let payload = self.pos;
                let text = self.unicode_string()?;
                self.add(Value::Str(text), "string-name", payload, self.pos, false)
            }
            kind::VECTOR2 => self.real_components("Vector2", &["x", "y"], locked, start),
            kind::RECT2 => self.real_components(
                "Rect2",
                &["position.x", "position.y", "size.x", "size.y"],
                locked,
                start,
            ),
            kind::VECTOR3 => self.real_components("Vector3", &["x", "y", "z"], locked, start),
            kind::PLANE => self.real_components(
                "Plane",
                &["normal.x", "normal.y", "normal.z", "d"],
                locked,
                start,
            ),
            kind::QUATERNION => {
                self.real_components("Quaternion", &["x", "y", "z", "w"], locked, start)
            }
            kind::AABB => self.real_components(
                "AABB",
                &[
                    "position.x",
                    "position.y",
                    "position.z",
                    "size.x",
                    "size.y",
                    "size.z",
                ],
                locked,
                start,
            ),
            kind::BASIS => self.real_components(
                "Basis",
                &[
                    "rows.x.x", "rows.x.y", "rows.x.z", "rows.y.x", "rows.y.y", "rows.y.z",
                    "rows.z.x", "rows.z.y", "rows.z.z",
                ],
                locked,
                start,
            ),
            kind::TRANSFORM3D => self.real_components(
                "Transform3D",
                &[
                    "basis.rows.x.x",
                    "basis.rows.x.y",
                    "basis.rows.x.z",
                    "basis.rows.y.x",
                    "basis.rows.y.y",
                    "basis.rows.y.z",
                    "basis.rows.z.x",
                    "basis.rows.z.y",
                    "basis.rows.z.z",
                    "origin.x",
                    "origin.y",
                    "origin.z",
                ],
                locked,
                start,
            ),
            kind::TRANSFORM2D => self.real_components(
                "Transform2D",
                &[
                    "columns.0.x",
                    "columns.0.y",
                    "columns.1.x",
                    "columns.1.y",
                    "columns.2.x",
                    "columns.2.y",
                ],
                locked,
                start,
            ),
            kind::COLOR => {
                if self.version == 3 {
                    self.real_components("Color", &["r", "g", "b", "a"], locked, start)
                } else {
                    self.float32_components("Color", &["r", "g", "b", "a"], locked, start)
                }
            }
            kind::VECTOR2I => self.int_components("Vector2i", &["x", "y"], locked, start),
            kind::RECT2I => self.int_components(
                "Rect2i",
                &["position.x", "position.y", "size.x", "size.y"],
                locked,
                start,
            ),
            kind::VECTOR3I => self.int_components("Vector3i", &["x", "y", "z"], locked, start),
            kind::VECTOR4 => self.real_components("Vector4", &["x", "y", "z", "w"], locked, start),
            kind::VECTOR4I => self.int_components("Vector4i", &["x", "y", "z", "w"], locked, start),
            kind::PROJECTION => self.real_components(
                "Projection",
                &[
                    "columns.0.x",
                    "columns.0.y",
                    "columns.0.z",
                    "columns.0.w",
                    "columns.1.x",
                    "columns.1.y",
                    "columns.1.z",
                    "columns.1.w",
                    "columns.2.x",
                    "columns.2.y",
                    "columns.2.z",
                    "columns.2.w",
                    "columns.3.x",
                    "columns.3.y",
                    "columns.3.z",
                    "columns.3.w",
                ],
                locked,
                start,
            ),
            kind::NODE_PATH => self.node_path(start),
            kind::RID => {
                let payload = self.pos;
                let value = self.u32()?;
                self.add(
                    Value::Int {
                        v: i128::from(value),
                        width: IntWidth::U32,
                    },
                    "rid",
                    payload,
                    self.pos,
                    false,
                )
            }
            kind::OBJECT => self.object(start),
            kind::DICTIONARY => {
                let count = usize::try_from(self.u32()? & 0x7fff_ffff)
                    .map_err(|_| self.fail("dictionary count does not fit this platform"))?;
                if count > self.remaining() / 8 {
                    return Err(self.fail("implausible dictionary count"));
                }
                let mut entries = Vec::with_capacity(count);
                for _ in 0..count {
                    let key = self.variant(locked)?;
                    let value = self.variant(locked)?;
                    entries.push((key, value));
                }
                self.add(Value::Map(entries), "dictionary", start, self.pos, false)
            }
            kind::ARRAY => {
                let count = usize::try_from(self.u32()? & 0x7fff_ffff)
                    .map_err(|_| self.fail("array count does not fit this platform"))?;
                if count > self.remaining() / 4 {
                    return Err(self.fail("implausible array count"));
                }
                let mut items = Vec::with_capacity(count);
                for _ in 0..count {
                    items.push(self.variant(locked)?);
                }
                self.add(Value::Array(items), "array", start, self.pos, false)
            }
            kind::PACKED_BYTE => self.packed(kind::PACKED_BYTE, start),
            kind::PACKED_INT32 => self.packed(kind::PACKED_INT32, start),
            kind::PACKED_FLOAT32 => self.packed(kind::PACKED_FLOAT32, start),
            kind::PACKED_STRING => self.packed_strings(start),
            kind::PACKED_VECTOR3 => self.packed(kind::PACKED_VECTOR3, start),
            kind::PACKED_COLOR => self.packed(kind::PACKED_COLOR, start),
            kind::PACKED_VECTOR2 => self.packed(kind::PACKED_VECTOR2, start),
            kind::PACKED_INT64 => self.packed(kind::PACKED_INT64, start),
            kind::PACKED_FLOAT64 => self.packed(kind::PACKED_FLOAT64, start),
            kind::CALLABLE | kind::SIGNAL => {
                self.add(Value::Null, "callable", start, self.pos, false)
            }
            _ => Err(self.fail("unknown Godot resource Variant type")),
        }
    }

    fn int32(&mut self, locked: bool) -> Result<NodeId> {
        let payload = self.pos;
        let value = self.u32()?;
        self.add(
            Value::Int {
                v: i128::from(value as i32),
                width: IntWidth::I32,
            },
            "int32",
            payload,
            self.pos,
            !locked,
        )
    }

    fn int64(&mut self, locked: bool) -> Result<NodeId> {
        let payload = self.pos;
        let value = self.u64()?;
        self.add(
            Value::Int {
                v: i128::from(value as i64),
                width: IntWidth::I64,
            },
            "int64",
            payload,
            self.pos,
            !locked,
        )
    }

    fn real(&mut self, locked: bool) -> Result<NodeId> {
        if self.real64 {
            let payload = self.pos;
            let value = f64::from_bits(self.u64()?);
            let node = self.add(
                Value::Float {
                    v: value,
                    width: FloatWidth::F64,
                },
                "float64",
                payload,
                self.pos,
                !locked,
            )?;
            Ok(node)
        } else {
            let payload = self.pos;
            let value = f32::from_bits(self.u32()?);
            let node = self.add(
                Value::Float {
                    v: f64::from(value),
                    width: FloatWidth::F32,
                },
                "float32",
                payload,
                self.pos,
                !locked,
            )?;
            Ok(node)
        }
    }

    fn real_components(
        &mut self,
        name: &str,
        components: &[&str],
        locked: bool,
        start: usize,
    ) -> Result<NodeId> {
        let mut entries = Vec::new();
        for component in components {
            let id = self.real(locked)?;
            entries.push(((*component).to_owned(), id));
        }
        self.math_node(name, start, entries)
    }

    fn float32_components(
        &mut self,
        name: &str,
        components: &[&str],
        locked: bool,
        start: usize,
    ) -> Result<NodeId> {
        let mut entries = Vec::new();
        for component in components {
            let payload = self.pos;
            let value = f32::from_bits(self.u32()?);
            let id = self.add(
                Value::Float {
                    v: f64::from(value),
                    width: FloatWidth::F32,
                },
                "float32",
                payload,
                self.pos,
                !locked,
            )?;
            entries.push(((*component).to_owned(), id));
        }
        self.math_node(name, start, entries)
    }

    fn int_components(
        &mut self,
        name: &str,
        components: &[&str],
        locked: bool,
        start: usize,
    ) -> Result<NodeId> {
        let mut entries = Vec::new();
        for component in components {
            let id = self.int32(locked)?;
            entries.push(((*component).to_owned(), id));
        }
        self.math_node(name, start, entries)
    }

    fn math_node(
        &mut self,
        name: &str,
        start: usize,
        entries: Vec<(String, NodeId)>,
    ) -> Result<NodeId> {
        self.add(
            Value::Object(entries),
            &format!("math:{name}"),
            start,
            self.pos,
            false,
        )
    }

    fn node_path(&mut self, start: usize) -> Result<NodeId> {
        let name_count = self.u16()?;
        let mut subname_count = self.u16()?;
        let absolute = subname_count & 0x8000 != 0;
        subname_count &= 0x7fff;
        if self.format < 3 {
            subname_count = subname_count.saturating_add(1);
        }
        let total = usize::from(name_count).saturating_add(usize::from(subname_count));
        if total > self.remaining() / 4 {
            return Err(self.fail("implausible NodePath name count"));
        }
        let mut names = Vec::with_capacity(usize::from(name_count));
        for _ in 0..name_count {
            names.push(self.string_ref()?);
        }
        let mut subnames = Vec::with_capacity(usize::from(subname_count));
        for _ in 0..subname_count {
            subnames.push(self.string_ref()?);
        }
        let names = self.push_strings(names, "node-path:name")?;
        let subnames = self.push_strings(subnames, "node-path:subname")?;
        let flags = self.doc.push(read_only_node(
            Value::Int {
                v: i128::from(absolute),
                width: IntWidth::U8,
            },
            "node-path:absolute",
        ))?;
        let entries = vec![
            ("names".to_owned(), names),
            ("subnames".to_owned(), subnames),
            ("absolute".to_owned(), flags),
        ];
        self.add(Value::Object(entries), "node-path", start, self.pos, false)
    }

    fn push_strings(&mut self, values: Vec<String>, hint: &str) -> Result<NodeId> {
        let mut items = Vec::with_capacity(values.len());
        for text in values {
            items.push(self.doc.push(read_only_node(Value::Str(text), hint))?);
        }
        self.doc
            .push(read_only_node(Value::Array(items), "node-path:list"))
    }

    fn object(&mut self, start: usize) -> Result<NodeId> {
        let object_type = self.u32()?;
        match object_type {
            0 => self.add(Value::Null, "object:empty", start, self.pos, false),
            1 => {
                let rtype = self.unicode_string()?;
                let path = self.unicode_string()?;
                let entries = vec![
                    (
                        "type".to_owned(),
                        self.doc.push(read_only_node(Value::Str(rtype), "object"))?,
                    ),
                    (
                        "path".to_owned(),
                        self.doc.push(read_only_node(Value::Str(path), "object"))?,
                    ),
                ];
                self.add(
                    Value::Object(entries),
                    "object:external",
                    start,
                    self.pos,
                    false,
                )
            }
            2 | 3 => {
                let index = self.u32()?;
                let entries = vec![(
                    "index".to_owned(),
                    self.doc.push(read_only_node(
                        Value::Int {
                            v: i128::from(index),
                            width: IntWidth::U32,
                        },
                        "object",
                    ))?,
                )];
                let hint = if object_type == 2 {
                    "object:internal"
                } else {
                    "object:external-index"
                };
                self.add(Value::Object(entries), hint, start, self.pos, false)
            }
            _ => Err(self.fail("unknown Godot resource object reference")),
        }
    }

    fn packed_strings(&mut self, start: usize) -> Result<NodeId> {
        let count = usize::try_from(self.u32()?)
            .map_err(|_| self.fail("packed string count does not fit this platform"))?;
        if count > self.remaining() / 4 {
            return Err(self.fail("implausible packed string count"));
        }
        let mut items = Vec::with_capacity(count);
        for _ in 0..count {
            let text = self.unicode_string()?;
            items.push(
                self.doc
                    .push(read_only_node(Value::Str(text), "packed:string"))?,
            );
        }
        let items = self
            .doc
            .push(read_only_node(Value::Array(items), "packed:string-list"))?;
        self.add(
            Value::Array(vec![items]),
            "packed:string-array",
            start,
            self.pos,
            false,
        )
    }

    fn packed(&mut self, tag: u32, start: usize) -> Result<NodeId> {
        let count = usize::try_from(self.u32()?)
            .map_err(|_| self.fail("packed array count does not fit this platform"))?;
        let stride = if self.version == 3 {
            match tag {
                kind::PACKED_BYTE => 1,
                kind::PACKED_INT32 => 4,
                kind::PACKED_FLOAT32 => self.real_width(),
                kind::PACKED_VECTOR3 => self.real_width().saturating_mul(3),
                kind::PACKED_COLOR => self.real_width().saturating_mul(4),
                kind::PACKED_VECTOR2 => self.real_width().saturating_mul(2),
                _ => return Err(self.fail("unknown packed array type")),
            }
        } else {
            match tag {
                kind::PACKED_BYTE => 1,
                kind::PACKED_INT32 => 4,
                kind::PACKED_FLOAT32 => 4,
                kind::PACKED_VECTOR3 => self.real_width().saturating_mul(3),
                kind::PACKED_COLOR => 16,
                kind::PACKED_VECTOR2 => self.real_width().saturating_mul(2),
                kind::PACKED_INT64 => 8,
                kind::PACKED_FLOAT64 => 8,
                _ => return Err(self.fail("unknown packed array type")),
            }
        };
        let bytes = if tag == kind::PACKED_BYTE {
            count
        } else {
            count
                .checked_mul(stride)
                .ok_or_else(|| self.fail("packed array size overflow"))?
        };
        if bytes > self.remaining() {
            return Err(self.fail("packed array length exceeds the input"));
        }
        self.take(bytes)?;
        if tag == kind::PACKED_BYTE {
            let padding = (4 - (bytes & 3)) & 3;
            self.take(padding)?;
        }
        self.read_only(
            Value::Bytes(self.input.get(start..self.pos).unwrap_or_default().to_vec()),
            &format!("packed:{tag}"),
            start,
        )
    }

    fn real_width(&self) -> usize {
        if self.real64 { 8 } else { 4 }
    }
}

fn header_int(value: i64) -> Value {
    Value::Int {
        v: i128::from(value),
        width: IntWidth::I64,
    }
}

fn header_str(value: String) -> Value {
    Value::Str(value)
}

fn read_only_node(value: Value, hint: &str) -> Node {
    let mut node = Node::new(value);
    node.type_hint = Some(TypeHint::new(format!("{RES_PREFIX}{hint}")));
    node.flags.insert(NodeFlags::READ_ONLY);
    node
}

/// Validates just enough of the header that a random `RSRC` prefix is not claimed.
fn probe(input: &[u8]) -> std::result::Result<(), ()> {
    let mut reader = Reader::new(
        input,
        Limits::default().max_depth,
        Limits::default().max_nodes,
    );
    if input.starts_with(b"RSCC") {
        reader.pos = 4;
        let mode = read_le_u32(input, &mut reader.pos)?;
        let block_size = read_le_u32(input, &mut reader.pos)?;
        let total = read_le_u32(input, &mut reader.pos)?;
        if block_size == 0 || total == 0 || mode > 4 {
            return Err(());
        }
        return Ok(());
    }
    reader.pos = 4;
    let big = reader.u32().map_err(|_| ())? != 0;
    let _ = big;
    reader.big_endian = big;
    reader.u32().map_err(|_| ())?;
    let major = reader.u32().map_err(|_| ())?;
    reader.u32().map_err(|_| ())?;
    let format = reader.u32().map_err(|_| ())?;
    if major == 0 || format == 0 || format > 8 {
        return Err(());
    }
    reader.unicode_string().map_err(|_| ())?;
    Ok(())
}

fn read_le_u32(input: &[u8], at: &mut usize) -> std::result::Result<u32, ()> {
    let end = at.checked_add(4).ok_or(())?;
    let bytes = input.get(*at..end).ok_or(())?;
    let array: [u8; 4] = bytes.try_into().map_err(|_| ())?;
    *at = end;
    Ok(u32::from_le_bytes(array))
}

struct RsccInfo {
    mode: u32,
    block_size: u32,
}

fn decompress_rscc(input: &[u8], limits: &Limits) -> Result<(Vec<u8>, RsccInfo)> {
    if input.len() < 16 {
        return Err(Error::new(
            GODOT_RESOURCE_LAYOUT,
            "the RSCC header is truncated",
        ));
    }
    let mut at = 4usize;
    let mode = read_le_u32(input, &mut at)
        .map_err(|_| Error::new(GODOT_RESOURCE_LAYOUT, "truncated RSCC mode"))?;
    let block_size = read_le_u32(input, &mut at)
        .map_err(|_| Error::new(GODOT_RESOURCE_LAYOUT, "truncated RSCC block size"))?;
    let total = read_le_u32(input, &mut at)
        .map_err(|_| Error::new(GODOT_RESOURCE_LAYOUT, "truncated RSCC length"))?;
    if block_size == 0 {
        return Err(Error::new(
            GODOT_RESOURCE_LAYOUT,
            "the RSCC block size is zero",
        ));
    }
    limits.check_output(len_u64(input.len()), u64::from(total))?;
    let total = usize::try_from(total).map_err(|_| {
        Error::new(
            GODOT_RESOURCE_LAYOUT,
            "RSCC length does not fit this platform",
        )
    })?;
    let block_size = usize::try_from(block_size).map_err(|_| {
        Error::new(
            GODOT_RESOURCE_LAYOUT,
            "RSCC block size does not fit this platform",
        )
    })?;
    let blocks = total / block_size + 1;
    let table_end = at
        .checked_add(blocks.saturating_mul(4))
        .ok_or_else(|| Error::new(GODOT_RESOURCE_LAYOUT, "RSCC block table overflow"))?;
    if table_end > input.len() {
        return Err(Error::new(
            GODOT_RESOURCE_LAYOUT,
            "the RSCC block table is truncated",
        ));
    }
    let mut sizes = Vec::with_capacity(blocks);
    for _ in 0..blocks {
        sizes.push(
            read_le_u32(input, &mut at)
                .map_err(|_| Error::new(GODOT_RESOURCE_LAYOUT, "truncated RSCC block table"))?,
        );
    }
    let mut out = Vec::with_capacity(total);
    for (index, size) in sizes.into_iter().enumerate() {
        let size = usize::try_from(size)
            .map_err(|_| Error::new(GODOT_RESOURCE_LAYOUT, "RSCC block size overflow"))?;
        let end = at
            .checked_add(size)
            .filter(|end| *end <= input.len())
            .ok_or_else(|| Error::new(GODOT_RESOURCE_LAYOUT, "truncated RSCC block"))?;
        let data = input
            .get(at..end)
            .ok_or_else(|| Error::new(GODOT_RESOURCE_LAYOUT, "invalid RSCC block"))?;
        at = end;
        let target = if blocks == 1 {
            total
        } else if index + 1 == blocks {
            total % block_size
        } else {
            block_size
        };
        let block = decompress_block(data, mode, target)?;
        if block.len() != target {
            return Err(Error::new(
                GODOT_RESOURCE_LAYOUT,
                format!(
                    "RSCC block {} decoded to {} bytes, expected {target}",
                    index,
                    block.len()
                ),
            ));
        }
        out.extend_from_slice(&block);
    }
    if out.len() != total {
        return Err(Error::new(
            GODOT_RESOURCE_LAYOUT,
            "the RSCC stream does not decode to its declared length",
        ));
    }
    Ok((
        out,
        RsccInfo {
            mode,
            block_size: u32::try_from(block_size).unwrap_or(u32::MAX),
        },
    ))
}

fn decompress_block(data: &[u8], mode: u32, target: usize) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(target);
    let mut reader: Box<dyn Read> = match mode {
        1 => Box::new(flate2::read::ZlibDecoder::new(data)),
        2 => {
            let decoded = zstd::bulk::decompress(data, target).map_err(|error| {
                Error::new(
                    GODOT_RESOURCE_LAYOUT,
                    format!("invalid Zstandard RSCC block: {error}"),
                )
            })?;
            return Ok(decoded);
        }
        3 => Box::new(flate2::read::GzDecoder::new(data)),
        0 | 4 => {
            return Err(Error::new(
                GODOT_RESOURCE_COMPRESSION,
                if mode == 0 {
                    "This Godot resource uses FastLZ compression, which is not supported."
                } else {
                    "This Godot resource uses Brotli compression, which is not supported."
                },
            ));
        }
        other => {
            return Err(Error::new(
                GODOT_RESOURCE_COMPRESSION,
                format!("unknown Godot RSCC compression mode {other}"),
            ));
        }
    };
    reader
        .by_ref()
        .take(u64::try_from(target).unwrap_or(u64::MAX).saturating_add(1))
        .read_to_end(&mut out)
        .map_err(|error| {
            Error::new(
                GODOT_RESOURCE_LAYOUT,
                format!("invalid compressed RSCC block: {error}"),
            )
        })?;
    Ok(out)
}

fn compress_rscc(inner: &[u8], mode: u32, block_size: u32) -> Result<Vec<u8>> {
    if block_size == 0 {
        return Err(Error::new(GODOT_WRITE, "RSCC block size is zero"));
    }
    if !matches!(mode, 1..=3) {
        return Err(Error::new(
            GODOT_RESOURCE_COMPRESSION,
            "RSCC compression mode cannot be reproduced",
        ));
    }
    let block_size = usize::try_from(block_size)
        .map_err(|_| Error::new(GODOT_WRITE, "RSCC block size does not fit this platform"))?;
    let total = inner.len();
    let blocks = total / block_size + 1;
    let mut out = Vec::new();
    out.extend_from_slice(b"RSCC");
    out.extend_from_slice(&mode.to_le_bytes());
    out.extend_from_slice(&u32::try_from(block_size).unwrap_or(u32::MAX).to_le_bytes());
    out.extend_from_slice(&u32::try_from(total).unwrap_or(u32::MAX).to_le_bytes());
    let mut sizes = Vec::with_capacity(blocks);
    let mut payload = Vec::new();
    for index in 0..blocks {
        let start = index.saturating_mul(block_size);
        let length = if index + 1 == blocks {
            total % block_size
        } else {
            block_size
        };
        let end = start.saturating_add(length).min(total);
        let block = inner.get(start..end).unwrap_or_default();
        let compressed = compress_block(block, mode)?;
        sizes.push(u32::try_from(compressed.len()).unwrap_or(u32::MAX));
        payload.extend_from_slice(&compressed);
    }
    for size in sizes {
        out.extend_from_slice(&size.to_le_bytes());
    }
    out.extend_from_slice(&payload);
    out.extend_from_slice(b"RSCC");
    Ok(out)
}

fn compress_block(block: &[u8], mode: u32) -> Result<Vec<u8>> {
    match mode {
        1 => {
            use std::io::Write;
            let mut encoder =
                flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
            encoder
                .write_all(block)
                .map_err(|error| Error::new(GODOT_WRITE, format!("zlib encode failed: {error}")))?;
            encoder
                .finish()
                .map_err(|error| Error::new(GODOT_WRITE, format!("zlib encode failed: {error}")))
        }
        2 => zstd::bulk::compress(block, 3)
            .map_err(|error| Error::new(GODOT_WRITE, format!("zstd encode failed: {error}"))),
        3 => {
            use std::io::Write;
            let mut encoder =
                flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
            encoder
                .write_all(block)
                .map_err(|error| Error::new(GODOT_WRITE, format!("gzip encode failed: {error}")))?;
            encoder
                .finish()
                .map_err(|error| Error::new(GODOT_WRITE, format!("gzip encode failed: {error}")))
        }
        _ => Err(Error::new(
            GODOT_RESOURCE_COMPRESSION,
            "RSCC compression mode cannot be reproduced",
        )),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use saveprism_core::{Edit, OwnedValue, Path};

    fn fixture(name: &str) -> Vec<u8> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/synthetic/godot")
            .join(name);
        std::fs::read(path).expect("fixture exists")
    }

    fn decode(bytes: &[u8]) -> Decoded {
        GodotResourceCodec
            .decode(bytes, &mut DecodeCtx::new(&Limits::default()))
            .expect("binary resource parses")
    }

    fn encode(decoded: &Decoded) -> Vec<u8> {
        GodotResourceCodec
            .encode(&decoded.doc, &decoded.params)
            .expect("binary resource writes")
    }

    fn set(decoded: &mut Decoded, path: &str, value: OwnedValue) -> saveprism_core::Result<()> {
        saveprism_core::apply_edits(
            &mut decoded.doc,
            &[Edit::Set {
                path: Path::parse(path).expect("path"),
                value,
            }],
            &Limits::default(),
        )
    }

    #[test]
    fn sniff_accepts_rsrc_and_rscc_by_content() {
        for name in [
            "resource4.res",
            "resource4-deflate.res",
            "resource4-gzip.res",
        ] {
            let bytes = fixture(name);
            let ctx = SniffCtx {
                file_name: Some("save.res"),
                depth: 0,
            };
            assert_eq!(
                GodotResourceCodec.sniff(&bytes, &ctx),
                Confidence::HIGH,
                "{name}"
            );
        }
        assert_eq!(
            GodotResourceCodec.sniff(
                b"not a resource",
                &SniffCtx {
                    file_name: None,
                    depth: 0
                }
            ),
            Confidence::NONE
        );
    }

    #[test]
    fn generated_binary_resources_round_trip_exactly() {
        for (name, version) in [
            ("resource4.res", 4),
            ("resource3.res", 3),
            ("resource4-deflate.res", 4),
            ("resource4-gzip.res", 4),
        ] {
            let bytes = fixture(name);
            let decoded = decode(&bytes);
            assert_eq!(encode(&decoded), bytes, "{name}");
            assert_eq!(
                decoded.params.get("version"),
                Some(&ParamValue::Int(version)),
                "{name}"
            );
        }
    }

    #[test]
    fn edits_fixed_width_values_in_place() {
        let bytes = fixture("resource4.res");
        let mut decoded = decode(&bytes);
        set(
            &mut decoded,
            "/resources/0/properties/gold",
            OwnedValue::Int(4321),
        )
        .expect("int edits");
        set(
            &mut decoded,
            "/resources/0/properties/hp",
            OwnedValue::Float(2.0),
        )
        .expect("float edits");
        set(
            &mut decoded,
            "/resources/0/properties/alive",
            OwnedValue::Bool(false),
        )
        .expect("bool edits");
        let output = encode(&decoded);
        assert_eq!(output.len(), bytes.len());
        let reread = decode(&output);
        assert!(decoded.doc.tree_eq(&reread.doc));
    }

    #[test]
    fn strings_and_containers_stay_read_only() {
        let bytes = fixture("resource4.res");
        let mut decoded = decode(&bytes);
        assert!(
            set(
                &mut decoded,
                "/resources/0/properties/player_name",
                OwnedValue::Str("x".into())
            )
            .is_err()
        );
        assert!(
            set(
                &mut decoded,
                "/resources/0/properties/inventory/1",
                OwnedValue::Int(9)
            )
            .is_ok()
        );
        assert!(
            set(
                &mut decoded,
                "/resources/0/properties/reference",
                OwnedValue::Int(0)
            )
            .is_err()
        );
    }

    #[test]
    fn compressed_edit_is_compressed_again_and_decodes() {
        for name in ["resource4-deflate.res", "resource4-gzip.res"] {
            let bytes = fixture(name);
            let mut decoded = decode(&bytes);
            set(
                &mut decoded,
                "/resources/0/properties/level",
                OwnedValue::Int(77),
            )
            .expect("level edits");
            let output = encode(&decoded);
            assert!(output.starts_with(b"RSCC"), "{name}");
            let reread = decode(&output);
            assert!(decoded.doc.tree_eq(&reread.doc), "{name}");
        }
    }

    #[test]
    fn fastlz_compression_is_refused_clearly() {
        let bytes = fixture(".fastlz-header.rscc");
        let error = GodotResourceCodec
            .decode(&bytes, &mut DecodeCtx::new(&Limits::default()))
            .expect_err("FastLZ is not supported");
        assert_eq!(error.code(), GODOT_RESOURCE_COMPRESSION);
        assert!(error.to_string().contains("FastLZ"));
    }

    #[test]
    fn rejects_truncated_resources() {
        for bytes in [
            &b"RSRC"[..],
            &b"RSRC\0\0\0\0"[..],
            &fixture("resource4.res")[..20],
            &b"RSCC\0\0\0\0\0\0\0\0"[..],
        ] {
            assert!(
                GodotResourceCodec
                    .decode(bytes, &mut DecodeCtx::new(&Limits::default()))
                    .is_err()
            );
        }
    }

    use proptest::prelude::*;

    fn writes_back(bytes: &[u8]) {
        if let Ok(decoded) =
            GodotResourceCodec.decode(bytes, &mut DecodeCtx::new(&Limits::default()))
        {
            let written = GodotResourceCodec
                .encode(&decoded.doc, &decoded.params)
                .expect("decoded binary resource writes");
            let again = GodotResourceCodec
                .decode(&written, &mut DecodeCtx::new(&Limits::default()))
                .expect("written binary resource decodes");
            assert!(decoded.doc.tree_eq(&again.doc));
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        #[test]
        fn random_input_never_panics(body in proptest::collection::vec(any::<u8>(), 0..256)) {
            writes_back(&body);
        }

        #[test]
        fn resource_like_input_never_panics(
            body in proptest::collection::vec(
                prop_oneof![
                    proptest::sample::select(b"RSRC".to_vec()),
                    proptest::sample::select(vec![0u8, 1, 2, 3, 4, 5, 10, 20, 22, 24, 26, 30, 31, 32, 33, 34, 40, 41, 47, 50, 52]),
                    0u8..=64,
                ],
                0..128,
            )
        ) {
            writes_back(&body);
        }
    }
}
