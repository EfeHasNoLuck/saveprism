//! `format.godot-variant`: Godot Engine binary `Variant` streams written by `FileAccess.store_var`
//! (`.save`, `.dat`, `.sav`).
//!
//! Written from the public "Binary serialization API" documentation and the type tables of Godot
//! 3.x and 4.x. A file is one or more Variants written back to back; Godot writes no magic number,
//! so the version is detected by which table parses the whole input cleanly (4.x is tried first).
//! Encrypted saves start with `GDEC` and are rejected with a clear error.
//!
//! Nothing is instantiated or executed: a serialized Object becomes a read-only group of
//! properties, never a Godot object.
//!
//! # Tree
//!
//! The root is an `Array` of the top-level Variants (hint `GODOT:variants`). Every node carries a
//! hint `GODOT:<kind>`:
//!
//! | Godot | Node | Hint |
//! |---|---|---|
//! | `null` | `Null` (read-only) | `GODOT:null` |
//! | `bool` | `Bool` | `GODOT:bool` |
//! | `int` | `Int` (I32 or I64 per the 64-bit flag) | `GODOT:int32`, `GODOT:int64` |
//! | `float` | `Float` (F32 or F64 per the 64-bit flag) | `GODOT:float32`, `GODOT:float64` |
//! | `String`, `StringName` | `Str` | `GODOT:string` |
//! | `Array` | `Array` (read-only) | `GODOT:array` |
//! | `Dictionary` | `Map` (read-only; keys stay nodes) | `GODOT:dictionary` |
//! | Packed arrays, `NodePath`, `Callable`, `Signal` | `Bytes` (read-only) | `GODOT:packed:<id>`, ... |
//! | `RID` | `Int` (read-only) | `GODOT:rid` |
//! | `Vector2`, `Vector3`, ... | `Object` of editable components (read-only) | `GODOT:math:<name>` |
//! | Object (full) | `Object` of properties (read-only, values read-only) | `GODOT:object:<class>` |
//! | Object (instance id) | `Int` (read-only) | `GODOT:object-id` |
//!
//! Editable: booleans, 32/64-bit integers, 32/64-bit floats, UTF-8 strings and the components of
//! math types. Containers stay read-only, so an edit never changes the shape of the file.
//!
//! # Exact writing
//!
//! The original byte stream is kept as the codec parameter `original`; every editable node records
//! the span of its payload. Writing patches only the nodes an edit changed and copies every other
//! byte, so unchanged files round-trip exactly. Typed-container descriptors, padding and shared
//! bits are never rewritten.

pub mod config;
pub mod resource_binary;
pub mod resource_text;
mod variant_text;

use std::sync::Arc;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    ByteSpan, Codec, Confidence, DecodeCtx, Decoded, Error, ErrorCode, FloatWidth, IntWidth,
    Limits, Node, NodeFlags, NodeId, ParamValue, Params, Registry, Result, SaveDoc, SniffCtx,
    TypeHint, Value, codes,
};

/// The input is not a Godot Variant stream (bad type, length, text or trailing bytes).
pub const GODOT_PARSE: ErrorCode = ErrorCode::new("SP-GODOT-001");
/// The save is encrypted (`GDEC`); decryption needs the game's key.
pub const GODOT_ENCRYPTED: ErrorCode = ErrorCode::new("SP-GODOT-002");
/// The document cannot be written back (missing original bytes, value does not fit its span).
pub const GODOT_WRITE: ErrorCode = ErrorCode::new("SP-GODOT-003");

const PREFIX: &str = "GODOT:";
const EXTENSIONS: [&str; 3] = [".save", ".dat", ".sav"];
/// Deepest nesting this reader accepts, independent of the configured limit.
const MAX_NESTING: usize = 4096;

/// 64-bit flag for integers, floats and (in 4.x) double-precision math types.
const FLAG_64: u32 = 1 << 16;

/// Registers the binary Godot Variant codec, the ConfigFile codec and the resource codecs.
pub fn register(reg: &mut Registry) {
    reg.add_codec(Arc::new(GodotCodec));
    config::register(reg);
    resource_text::register(reg);
    resource_binary::register(reg);
}

/// The `format.godot-variant` codec.
#[derive(Clone, Copy, Debug, Default)]
pub struct GodotCodec;

impl Codec for GodotCodec {
    fn id(&self) -> &'static str {
        "format.godot-variant"
    }

    fn sniff(&self, input: &[u8], ctx: &SniffCtx<'_>) -> Confidence {
        if input.starts_with(b"GDEC") {
            return Confidence::HIGH;
        }
        if input.is_empty() {
            return Confidence::NONE;
        }
        let limits = Limits::default();
        let named = ctx.file_name.is_some_and(|name| {
            let lower = name.to_ascii_lowercase();
            EXTENSIONS
                .iter()
                .any(|extension| lower.ends_with(extension))
        });
        for (version, confidence) in [(4u8, 0.65f32), (3, 0.6)] {
            let mut reader = Reader::new(input, version, limits.max_depth, limits.max_nodes);
            if reader.parse_all().is_ok() && reader.pos == input.len() {
                return Confidence::new(if named {
                    confidence
                } else if version == 4 {
                    Confidence::MEDIUM.value()
                } else {
                    Confidence::LOW.value()
                });
            }
        }
        Confidence::NONE
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<Decoded> {
        ctx.limits().check_input(len_u64(input.len()))?;
        if input.starts_with(b"GDEC") {
            return Err(Error::new(
                GODOT_ENCRYPTED,
                "This Godot save is encrypted with the game's key; decryption is not supported.",
            ));
        }
        if input.is_empty() {
            return Err(Error::new(GODOT_PARSE, "the Godot Variant stream is empty"));
        }
        let mut last = None;
        for version in [4u8, 3] {
            let mut reader = Reader::new(
                input,
                version,
                ctx.limits().max_depth,
                ctx.limits().max_nodes,
            );
            match reader.parse_all() {
                Ok(()) if reader.pos == input.len() => {
                    ctx.charge_nodes(len_u64(reader.doc.len()))?;
                    let mut params = Params::new();
                    params.set("original", input.to_vec());
                    params.set("version", i64::from(version));
                    return Ok(Decoded {
                        doc: reader.doc,
                        params,
                    });
                }
                Ok(()) => {
                    last = Some(Error::new(
                        GODOT_PARSE,
                        "trailing bytes after Godot Variants",
                    ));
                }
                Err(error) => last = Some(error),
            }
        }
        Err(last.unwrap_or_else(|| Error::new(GODOT_PARSE, "invalid Godot Variant stream")))
    }

    fn encode(&self, doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
        let Some(ParamValue::Bytes(original)) = params.get("original") else {
            return Err(Error::new(
                GODOT_WRITE,
                "the original Godot byte stream is missing",
            ));
        };
        let mut patches = Vec::new();
        let mut stack = vec![doc.root()];
        let mut visited = 0usize;
        while let Some(id) = stack.pop() {
            visited = visited.saturating_add(1);
            if visited > doc.len().saturating_add(1) {
                return Err(Error::new(GODOT_WRITE, "cyclic Godot tree is not writable"));
            }
            let Some(node) = doc.get(id) else {
                return Err(Error::new(GODOT_WRITE, "invalid node in Godot tree"));
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
            let Some(span) = node.span else {
                continue;
            };
            let start = usize::try_from(span.start)
                .map_err(|_| Error::new(GODOT_WRITE, "invalid node span"))?;
            let end = usize::try_from(span.end)
                .map_err(|_| Error::new(GODOT_WRITE, "invalid node span"))?;
            let old = original
                .get(start..end)
                .ok_or_else(|| Error::new(GODOT_WRITE, "node span exceeds original input"))?;
            let Some(replacement) = render(hint, &node.value)? else {
                continue;
            };
            if !hint.eq("GODOT:string") && replacement.len() != old.len() {
                return Err(Error::new(
                    GODOT_WRITE,
                    "a fixed-width Godot value does not fit its original span",
                ));
            }
            if replacement.as_slice() != old {
                patches.push((start, end, replacement));
            }
        }
        patches.sort_by_key(|(start, _, _)| *start);
        for pair in patches.windows(2) {
            if let (Some(a), Some(b)) = (pair.first(), pair.get(1))
                && a.1 > b.0
            {
                return Err(Error::new(GODOT_WRITE, "overlapping Godot value spans"));
            }
        }
        let mut out = original.clone();
        for (start, end, replacement) in patches.into_iter().rev() {
            out.splice(start..end, replacement);
        }
        Ok(out)
    }
}

/// Renders an edited leaf. `None` means the node is not an editable Godot scalar.
fn render(hint: &str, value: &Value) -> Result<Option<Vec<u8>>> {
    let bytes = match (hint, value) {
        ("GODOT:bool", Value::Bool(v)) => u32::from(*v).to_le_bytes().to_vec(),
        ("GODOT:int32", Value::Int { v, .. }) => i32::try_from(*v)
            .map_err(|_| Error::new(GODOT_WRITE, "integer does not fit 32 bits"))?
            .to_le_bytes()
            .to_vec(),
        ("GODOT:int64", Value::Int { v, .. }) => i64::try_from(*v)
            .map_err(|_| Error::new(GODOT_WRITE, "integer does not fit 64 bits"))?
            .to_le_bytes()
            .to_vec(),
        ("GODOT:float32", Value::Float { v, .. }) => (*v as f32).to_le_bytes().to_vec(),
        ("GODOT:float64", Value::Float { v, .. }) => v.to_le_bytes().to_vec(),
        ("GODOT:string", Value::Str(text)) => encode_string(text)?,
        _ => return Ok(None),
    };
    Ok(Some(bytes))
}

/// A Godot string payload: length, UTF-8 bytes, zero padding to a multiple of four.
fn encode_string(text: &str) -> Result<Vec<u8>> {
    let length = u32::try_from(text.len())
        .map_err(|_| Error::new(GODOT_WRITE, "the Godot string is too long"))?;
    let mut out = length.to_le_bytes().to_vec();
    out.extend_from_slice(text.as_bytes());
    out.resize(out.len().saturating_add((4 - (text.len() & 3)) & 3), 0);
    Ok(out)
}

/// Reads a Godot Variant stream into a document.
struct Reader<'a> {
    input: &'a [u8],
    pos: usize,
    version: u8,
    depth: usize,
    max_depth: usize,
    doc: SaveDoc,
}

impl<'a> Reader<'a> {
    fn new(input: &'a [u8], version: u8, max_depth: usize, max_nodes: u64) -> Self {
        Self {
            input,
            pos: 0,
            version,
            depth: 0,
            max_depth,
            doc: SaveDoc::with_node_limit(max_nodes),
        }
    }

    fn fail(&self, message: &str) -> Error {
        Error::new(GODOT_PARSE, format!("{message} at byte {}", self.pos))
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
            .ok_or_else(|| self.fail("truncated Godot Variant"))?;
        let start = self.pos;
        let out = self
            .input
            .get(start..end)
            .ok_or_else(|| self.fail("invalid byte range"))?;
        self.pos = end;
        Ok(out)
    }

    fn u32(&mut self) -> Result<u32> {
        let bytes = self.take(4)?;
        let array: [u8; 4] = bytes
            .try_into()
            .map_err(|_| self.fail("truncated integer"))?;
        Ok(u32::from_le_bytes(array))
    }

    fn u64(&mut self) -> Result<u64> {
        let bytes = self.take(8)?;
        let array: [u8; 8] = bytes
            .try_into()
            .map_err(|_| self.fail("truncated integer"))?;
        Ok(u64::from_le_bytes(array))
    }

    fn length(&mut self, at: usize) -> Result<usize> {
        let value = self.u32()?;
        let length =
            usize::try_from(value).map_err(|_| self.fail("length does not fit this platform"))?;
        if length > self.remaining() {
            return Err(Error::new(
                GODOT_PARSE,
                format!("length {length} is larger than the rest of the file"),
            )
            .with_offset(len_u64(at)));
        }
        Ok(length)
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
        node.type_hint = Some(TypeHint::new(format!("{PREFIX}{hint}")));
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

    fn parse_all(&mut self) -> Result<()> {
        let mut roots = Vec::new();
        while self.pos < self.input.len() {
            roots.push(self.variant(false)?);
        }
        let root = self
            .doc
            .push(read_only_node(Value::Array(roots), "variants"))?;
        self.doc.set_root(root);
        Ok(())
    }

    fn variant(&mut self, locked: bool) -> Result<NodeId> {
        if self.depth >= self.max_depth.min(MAX_NESTING) {
            return Err(Error::new(
                codes::CORE_LIMIT_DEPTH,
                format!("Godot Variant nesting limit of {MAX_NESTING} exceeded"),
            )
            .with_offset(len_u64(self.pos)));
        }
        self.depth += 1;
        let result = self.variant_inner(locked);
        self.depth -= 1;
        result
    }

    fn variant_inner(&mut self, locked: bool) -> Result<NodeId> {
        let start = self.pos;
        let header = self.u32()?;
        let kind = header & 0xffff;
        let flags = header >> 16;
        let width64 = header & FLAG_64 != 0;
        match kind {
            0 => self.add(Value::Null, "null", start, self.pos, false),
            1 => {
                let payload = self.pos;
                let value = self.u32()?;
                if value > 1 {
                    return Err(self.fail("invalid Godot boolean"));
                }
                self.add(Value::Bool(value == 1), "bool", payload, self.pos, !locked)
            }
            2 => self.integer(start, width64, locked),
            3 => self.float(start, width64, locked),
            4 => self.string(start, locked),
            21 if self.version == 4 => self.string(start, locked),
            22 if self.version == 4 => self.node_path(start, locked),
            5..=19 if self.version == 4 => self.math(kind, start, width64, locked),
            20 if self.version == 4 => self.math(kind, start, width64, locked),
            23 if self.version == 4 => self.rid(start, 8),
            24 if self.version == 4 => self.object(start, flags, locked),
            25 if self.version == 4 => self.read_only(Value::Null, "callable", start),
            26 if self.version == 4 => self.signal(start),
            27 if self.version == 4 => self.container(kind, start, flags, locked),
            28 if self.version == 4 => self.container(kind, start, flags, locked),
            29..=38 if self.version == 4 => self.packed(kind, start, flags),
            5..=14 if self.version == 3 => self.math(kind, start, false, locked),
            15 if self.version == 3 => self.node_path(start, locked),
            16 if self.version == 3 => self.rid(start, 4),
            17 if self.version == 3 => self.object(start, flags, locked),
            18 | 19 if self.version == 3 => self.container(kind, start, flags, locked),
            20..=26 if self.version == 3 => self.packed(kind, start, flags),
            _ => Err(self.fail("unknown Godot Variant type")),
        }
    }

    fn integer(&mut self, _start: usize, width64: bool, locked: bool) -> Result<NodeId> {
        let payload = self.pos;
        let (value, width, hint) = if width64 {
            (
                i128::from(i64::from_le_bytes(
                    self.take(8)?
                        .try_into()
                        .map_err(|_| self.fail("truncated int64"))?,
                )),
                IntWidth::I64,
                "int64",
            )
        } else {
            (
                i128::from(i32::from_le_bytes(
                    self.take(4)?
                        .try_into()
                        .map_err(|_| self.fail("truncated int32"))?,
                )),
                IntWidth::I32,
                "int32",
            )
        };
        self.add(
            Value::Int { v: value, width },
            hint,
            payload,
            self.pos,
            !locked,
        )
    }

    fn float(&mut self, _start: usize, width64: bool, locked: bool) -> Result<NodeId> {
        let payload = self.pos;
        let (value, width, hint) = if width64 {
            (
                f64::from_le_bytes(
                    self.take(8)?
                        .try_into()
                        .map_err(|_| self.fail("truncated float64"))?,
                ),
                FloatWidth::F64,
                "float64",
            )
        } else {
            (
                f64::from(f32::from_le_bytes(
                    self.take(4)?
                        .try_into()
                        .map_err(|_| self.fail("truncated float32"))?,
                )),
                FloatWidth::F32,
                "float32",
            )
        };
        self.add(
            Value::Float { v: value, width },
            hint,
            payload,
            self.pos,
            !locked,
        )
    }

    fn string(&mut self, _start: usize, locked: bool) -> Result<NodeId> {
        let payload = self.pos;
        let text = self.string_payload()?;
        self.add(Value::Str(text), "string", payload, self.pos, !locked)
    }

    fn string_payload(&mut self) -> Result<String> {
        let at = self.pos;
        let length = self.length(at)?;
        let bytes = self.take(length)?;
        let padding = (4 - (length & 3)) & 3;
        self.take(padding)?;
        std::str::from_utf8(bytes)
            .map(str::to_owned)
            .map_err(|_| self.fail("Godot string is not UTF-8"))
    }

    fn string_with_length(&mut self, length: usize) -> Result<String> {
        let bytes = self.take(length)?;
        let padding = (4 - (length & 3)) & 3;
        self.take(padding)?;
        std::str::from_utf8(bytes)
            .map(str::to_owned)
            .map_err(|_| self.fail("Godot string is not UTF-8"))
    }

    fn node_path(&mut self, start: usize, locked: bool) -> Result<NodeId> {
        let first = self.u32()?;
        if first & 0x8000_0000 == 0 {
            let length = usize::try_from(first)
                .map_err(|_| self.fail("NodePath length does not fit this platform"))?;
            let text = self.string_with_length(length)?;
            return self.add(Value::Str(text), "node-path:old", start, self.pos, !locked);
        }
        let name_count = usize::try_from(first & 0x7fff_ffff)
            .map_err(|_| self.fail("NodePath name count does not fit this platform"))?;
        let subname_count = usize::try_from(self.u32()?)
            .map_err(|_| self.fail("NodePath subname count does not fit this platform"))?;
        let np_flags = self.u32()?;
        let extra = usize::from(np_flags & 2 != 0);
        let total = name_count
            .checked_add(subname_count)
            .and_then(|count| count.checked_add(extra))
            .ok_or_else(|| self.fail("NodePath name count overflow"))?;
        if total > self.remaining() / 4 {
            return Err(self.fail("implausible NodePath name count"));
        }
        let mut names = Vec::with_capacity(name_count);
        for _ in 0..name_count {
            let text = self.string_payload()?;
            names.push(
                self.doc
                    .push(read_only_node(Value::Str(text), "node-path:name"))?,
            );
        }
        let mut subnames = Vec::with_capacity(subname_count.saturating_add(extra));
        for _ in 0..subname_count.saturating_add(extra) {
            let text = self.string_payload()?;
            subnames.push(
                self.doc
                    .push(read_only_node(Value::Str(text), "node-path:subname"))?,
            );
        }
        let names = self
            .doc
            .push(read_only_node(Value::Array(names), "node-path:names"))?;
        let subnames = self
            .doc
            .push(read_only_node(Value::Array(subnames), "node-path:subnames"))?;
        let flags_node = self.doc.push(read_only_node(
            Value::Int {
                v: i128::from(np_flags),
                width: IntWidth::U32,
            },
            "node-path:flags",
        ))?;
        let entries = vec![
            ("names".to_owned(), names),
            ("subnames".to_owned(), subnames),
            ("flags".to_owned(), flags_node),
        ];
        self.add(Value::Object(entries), "node-path", start, self.pos, false)
    }

    fn rid(&mut self, start: usize, width: usize) -> Result<NodeId> {
        let (value, int_width) = if width == 8 {
            (i128::from(self.u64()?), IntWidth::U64)
        } else {
            (i128::from(self.u32()?), IntWidth::U32)
        };
        self.add(
            Value::Int {
                v: value,
                width: int_width,
            },
            "rid",
            start,
            self.pos,
            false,
        )
    }

    fn signal(&mut self, start: usize) -> Result<NodeId> {
        let name = self.string_payload()?;
        let id = self.u64()?;
        let name_node = self
            .doc
            .push(read_only_node(Value::Str(name), "signal:name"))?;
        let id_node = self.doc.push(read_only_node(
            Value::Int {
                v: i128::from(id),
                width: IntWidth::U64,
            },
            "signal:object-id",
        ))?;
        let entries = vec![
            ("name".to_owned(), name_node),
            ("object_id".to_owned(), id_node),
        ];
        self.add(Value::Object(entries), "signal", start, self.pos, false)
    }

    fn object(&mut self, start: usize, flags: u32, _locked: bool) -> Result<NodeId> {
        if flags & 1 != 0 {
            let id = self.u64()?;
            return self.add(
                Value::Int {
                    v: i128::from(id),
                    width: IntWidth::U64,
                },
                "object-id",
                start,
                self.pos,
                false,
            );
        }
        let at = self.pos;
        let class_length = self.length(at)?;
        if class_length == 0 {
            return self.add(Value::Null, "object:null", start, self.pos, false);
        }
        let class = self.string_with_length(class_length)?;
        let count = usize::try_from(self.u32()?)
            .map_err(|_| self.fail("object property count does not fit this platform"))?;
        if count > self.remaining() / 8 {
            return Err(self.fail("implausible Godot object property count"));
        }
        let mut properties = Vec::with_capacity(count);
        for _ in 0..count {
            let key = self.string_payload()?;
            let value = self.variant(true)?;
            properties.push((key, value));
        }
        self.add(
            Value::Object(properties),
            &format!("object:{class}"),
            start,
            self.pos,
            false,
        )
    }

    fn container(&mut self, kind: u32, start: usize, flags: u32, locked: bool) -> Result<NodeId> {
        if self.version == 4 {
            if kind == 28 {
                self.container_type(flags & 0b11)?;
            } else {
                self.container_type(flags & 0b11)?;
                self.container_type((flags >> 2) & 0b11)?;
            }
        }
        let raw = self.u32()?;
        let count = usize::try_from(raw & 0x7fff_ffff)
            .map_err(|_| self.fail("container count does not fit this platform"))?;
        let array_kind = if self.version == 4 { 28 } else { 19 };
        if kind == array_kind {
            if count > self.remaining() / 4 {
                return Err(self.fail("implausible Godot array count"));
            }
            let mut items = Vec::with_capacity(count);
            for _ in 0..count {
                items.push(self.variant(locked)?);
            }
            self.add(Value::Array(items), "array", start, self.pos, false)
        } else {
            if count > self.remaining() / 8 {
                return Err(self.fail("implausible Godot dictionary count"));
            }
            let mut entries = Vec::with_capacity(count);
            for _ in 0..count {
                let key = self.variant(locked)?;
                let value = self.variant(locked)?;
                entries.push((key, value));
            }
            self.add(Value::Map(entries), "dictionary", start, self.pos, false)
        }
    }

    /// Reads (and discards) a typed-container descriptor: a builtin type id or a class/script name.
    fn container_type(&mut self, field: u32) -> Result<()> {
        match field {
            0 => Ok(()),
            1 => {
                let at = self.pos;
                let builtin = self.u32()?;
                if builtin > 38 {
                    return Err(Error::new(
                        GODOT_PARSE,
                        format!("unknown Godot builtin type {builtin} in a typed container"),
                    )
                    .with_offset(len_u64(at)));
                }
                Ok(())
            }
            2 | 3 => {
                self.string_payload()?;
                Ok(())
            }
            _ => Err(self.fail("invalid typed-container field")),
        }
    }

    fn packed(&mut self, kind: u32, start: usize, flags: u32) -> Result<NodeId> {
        let count = usize::try_from(self.u32()?)
            .map_err(|_| self.fail("packed array count does not fit this platform"))?;
        if self.version == 3 && kind == 23 {
            return self.packed_strings(count, start);
        }
        if self.version == 4 && kind == 34 {
            return self.packed_strings(count, start);
        }
        if self.version == 3 && kind == 20 {
            self.packed_bytes(count, start)?;
            return self.read_only(
                Value::Bytes(self.input.get(start..self.pos).unwrap_or_default().to_vec()),
                "packed:byte",
                start,
            );
        }
        if self.version == 4 && kind == 29 {
            self.packed_bytes(count, start)?;
            return self.read_only(
                Value::Bytes(self.input.get(start..self.pos).unwrap_or_default().to_vec()),
                "packed:byte",
                start,
            );
        }
        let width64 = flags & 1 != 0;
        let stride = match (self.version, kind) {
            (3, 21 | 22) => 4,
            (3, 24) => 8,
            (3, 25) => 12,
            (3, 26) => 16,
            (4, 30 | 32) => 4,
            (4, 31 | 33) => 8,
            (4, 35) => {
                if width64 {
                    16
                } else {
                    8
                }
            }
            (4, 36) => {
                if width64 {
                    24
                } else {
                    12
                }
            }
            (4, 37) => 16,
            (4, 38) => {
                if width64 {
                    32
                } else {
                    16
                }
            }
            _ => return Err(self.fail("unknown Godot packed array type")),
        };
        let bytes = count
            .checked_mul(stride)
            .filter(|total| *total <= self.remaining())
            .ok_or_else(|| self.fail("packed array length exceeds the input"))?;
        self.take(bytes)?;
        self.read_only(
            Value::Bytes(self.input.get(start..self.pos).unwrap_or_default().to_vec()),
            &format!("packed:{kind}"),
            start,
        )
    }

    fn packed_bytes(&mut self, count: usize, _start: usize) -> Result<()> {
        if count > self.remaining() {
            return Err(self.fail("packed byte array length exceeds the input"));
        }
        self.take(count)?;
        let padding = (4 - (count & 3)) & 3;
        self.take(padding)?;
        Ok(())
    }

    fn packed_strings(&mut self, count: usize, start: usize) -> Result<NodeId> {
        if count > self.remaining() / 4 {
            return Err(self.fail("implausible packed string count"));
        }
        let mut items = Vec::with_capacity(count);
        for _ in 0..count {
            let mut text = self.string_payload()?;
            if text.ends_with('\0') {
                text.pop();
            }
            items.push(
                self.doc
                    .push(read_only_node(Value::Str(text), "packed:string"))?,
            );
        }
        let items = self
            .doc
            .push(read_only_node(Value::Array(items), "packed:string-array"))?;
        self.doc.push(read_only_node(
            Value::Array(Vec::new()),
            "packed:string-marker",
        ))?;
        self.add(
            Value::Array(vec![items]),
            "packed:string-array",
            start,
            self.pos,
            false,
        )
    }

    fn math(&mut self, kind: u32, start: usize, width64: bool, locked: bool) -> Result<NodeId> {
        let width = if width64 {
            FloatWidth::F64
        } else {
            FloatWidth::F32
        };
        match (self.version, kind) {
            (4, 5) | (3, 5) => self.vector2(width, locked, start),
            (4, 6) => self.vector2i(locked, start),
            (4, 7) | (3, 6) => {
                let position = self.vector2(width, locked, self.pos)?;
                let size = self.vector2(width, locked, self.pos)?;
                self.math_node("Rect2", start, vec![("position", position), ("size", size)])
            }
            (4, 8) => {
                let position = self.vector2i(locked, self.pos)?;
                let size = self.vector2i(locked, self.pos)?;
                self.math_node(
                    "Rect2i",
                    start,
                    vec![("position", position), ("size", size)],
                )
            }
            (4, 9) | (3, 7) => self.vector3(width, locked, start),
            (4, 10) => self.vector3i(locked, start),
            (4, 11) | (3, 8) => {
                let x = self.vector2(width, locked, self.pos)?;
                let y = self.vector2(width, locked, self.pos)?;
                let origin = self.vector2(width, locked, self.pos)?;
                self.math_node(
                    "Transform2D",
                    start,
                    vec![("x", x), ("y", y), ("origin", origin)],
                )
            }
            (4, 12) => self.vector4(width, locked, start),
            (4, 13) => self.vector4i(locked, start),
            (4, 14) | (3, 9) => {
                let normal = self.vector3(width, locked, self.pos)?;
                let distance = self.float_component(width, locked)?;
                self.math_node("Plane", start, vec![("normal", normal), ("d", distance)])
            }
            (4, 15) | (3, 10) => {
                let x = self.float_component(width, locked)?;
                let y = self.float_component(width, locked)?;
                let z = self.float_component(width, locked)?;
                let w = self.float_component(width, locked)?;
                self.math_node(
                    "Quaternion",
                    start,
                    vec![("x", x), ("y", y), ("z", z), ("w", w)],
                )
            }
            (4, 16) | (3, 11) => {
                let position = self.vector3(width, locked, self.pos)?;
                let size = self.vector3(width, locked, self.pos)?;
                self.math_node("AABB", start, vec![("position", position), ("size", size)])
            }
            (4, 17) | (3, 12) => {
                let x = self.vector3(width, locked, self.pos)?;
                let y = self.vector3(width, locked, self.pos)?;
                let z = self.vector3(width, locked, self.pos)?;
                self.math_node("Basis", start, vec![("x", x), ("y", y), ("z", z)])
            }
            (4, 18) | (3, 13) => {
                let basis_start = self.pos;
                let x = self.vector3(width, locked, self.pos)?;
                let y = self.vector3(width, locked, self.pos)?;
                let z = self.vector3(width, locked, self.pos)?;
                let basis =
                    self.math_node("Basis", basis_start, vec![("x", x), ("y", y), ("z", z)])?;
                let origin = self.vector3(width, locked, self.pos)?;
                self.math_node(
                    "Transform3D",
                    start,
                    vec![("basis", basis), ("origin", origin)],
                )
            }
            (4, 19) => {
                let x = self.vector4(width, locked, self.pos)?;
                let y = self.vector4(width, locked, self.pos)?;
                let z = self.vector4(width, locked, self.pos)?;
                let w = self.vector4(width, locked, self.pos)?;
                self.math_node(
                    "Projection",
                    start,
                    vec![("x", x), ("y", y), ("z", z), ("w", w)],
                )
            }
            (4, 20) | (3, 14) => {
                let r = self.float_component(FloatWidth::F32, locked)?;
                let g = self.float_component(FloatWidth::F32, locked)?;
                let b = self.float_component(FloatWidth::F32, locked)?;
                let a = self.float_component(FloatWidth::F32, locked)?;
                self.math_node("Color", start, vec![("r", r), ("g", g), ("b", b), ("a", a)])
            }
            _ => Err(self.fail("unknown Godot math type")),
        }
    }

    fn float_component(&mut self, width: FloatWidth, locked: bool) -> Result<NodeId> {
        let start = self.pos;
        let value = match width {
            FloatWidth::F32 => f64::from(f32::from_le_bytes(
                self.take(4)?
                    .try_into()
                    .map_err(|_| self.fail("truncated float component"))?,
            )),
            FloatWidth::F64 => f64::from_le_bytes(
                self.take(8)?
                    .try_into()
                    .map_err(|_| self.fail("truncated float component"))?,
            ),
        };
        let hint = match width {
            FloatWidth::F32 => "float32",
            FloatWidth::F64 => "float64",
        };
        self.add(
            Value::Float { v: value, width },
            hint,
            start,
            self.pos,
            !locked,
        )
    }

    fn int_component(&mut self, locked: bool) -> Result<NodeId> {
        let start = self.pos;
        let value = i32::from_le_bytes(
            self.take(4)?
                .try_into()
                .map_err(|_| self.fail("truncated integer component"))?,
        );
        self.add(
            Value::Int {
                v: i128::from(value),
                width: IntWidth::I32,
            },
            "int32",
            start,
            self.pos,
            !locked,
        )
    }

    fn vector2(&mut self, width: FloatWidth, locked: bool, start: usize) -> Result<NodeId> {
        let x = self.float_component(width, locked)?;
        let y = self.float_component(width, locked)?;
        self.math_node("Vector2", start, vec![("x", x), ("y", y)])
    }

    fn vector2i(&mut self, locked: bool, start: usize) -> Result<NodeId> {
        let x = self.int_component(locked)?;
        let y = self.int_component(locked)?;
        self.math_node("Vector2i", start, vec![("x", x), ("y", y)])
    }

    fn vector3(&mut self, width: FloatWidth, locked: bool, start: usize) -> Result<NodeId> {
        let x = self.float_component(width, locked)?;
        let y = self.float_component(width, locked)?;
        let z = self.float_component(width, locked)?;
        self.math_node("Vector3", start, vec![("x", x), ("y", y), ("z", z)])
    }

    fn vector3i(&mut self, locked: bool, start: usize) -> Result<NodeId> {
        let x = self.int_component(locked)?;
        let y = self.int_component(locked)?;
        let z = self.int_component(locked)?;
        self.math_node("Vector3i", start, vec![("x", x), ("y", y), ("z", z)])
    }

    fn vector4(&mut self, width: FloatWidth, locked: bool, start: usize) -> Result<NodeId> {
        let x = self.float_component(width, locked)?;
        let y = self.float_component(width, locked)?;
        let z = self.float_component(width, locked)?;
        let w = self.float_component(width, locked)?;
        self.math_node(
            "Vector4",
            start,
            vec![("x", x), ("y", y), ("z", z), ("w", w)],
        )
    }

    fn vector4i(&mut self, locked: bool, start: usize) -> Result<NodeId> {
        let x = self.int_component(locked)?;
        let y = self.int_component(locked)?;
        let z = self.int_component(locked)?;
        let w = self.int_component(locked)?;
        self.math_node(
            "Vector4i",
            start,
            vec![("x", x), ("y", y), ("z", z), ("w", w)],
        )
    }

    fn math_node(
        &mut self,
        name: &str,
        start: usize,
        entries: Vec<(&str, NodeId)>,
    ) -> Result<NodeId> {
        let entries = entries
            .into_iter()
            .map(|(key, id)| (key.to_owned(), id))
            .collect();
        self.add(
            Value::Object(entries),
            &format!("math:{name}"),
            start,
            self.pos,
            false,
        )
    }
}

fn read_only_node(value: Value, hint: &str) -> Node {
    let mut node = Node::new(value);
    node.type_hint = Some(TypeHint::new(format!("{PREFIX}{hint}")));
    node.flags.insert(NodeFlags::READ_ONLY);
    node
}

#[cfg(test)]
mod tests {
    use super::*;
    use saveprism_core::{Edit, OwnedValue, Path};

    fn word(value: u32, out: &mut Vec<u8>) {
        out.extend_from_slice(&value.to_le_bytes());
    }

    fn string_payload(text: &str, out: &mut Vec<u8>) {
        word(u32::try_from(text.len()).unwrap_or_default(), out);
        out.extend_from_slice(text.as_bytes());
        out.resize(out.len() + ((4 - (text.len() & 3)) & 3), 0);
    }

    fn string_value(text: &str) -> Vec<u8> {
        let mut out = Vec::new();
        word(4, &mut out);
        string_payload(text, &mut out);
        out
    }

    fn dictionary(entries: &[(Vec<u8>, Vec<u8>)]) -> Vec<u8> {
        let mut out = Vec::new();
        word(27, &mut out);
        word(u32::try_from(entries.len()).unwrap_or_default(), &mut out);
        for (key, value) in entries {
            out.extend_from_slice(key);
            out.extend_from_slice(value);
        }
        out
    }

    fn decode(bytes: &[u8]) -> Decoded {
        let limits = Limits::default();
        let mut ctx = DecodeCtx::new(&limits);
        GodotCodec
            .decode(bytes, &mut ctx)
            .expect("synthetic Variant parses")
    }

    fn encode(decoded: &Decoded) -> Vec<u8> {
        GodotCodec
            .encode(&decoded.doc, &decoded.params)
            .expect("document writes")
    }

    fn round_trip(bytes: &[u8]) -> Decoded {
        let decoded = decode(bytes);
        assert_eq!(encode(&decoded), bytes);
        decoded
    }

    fn set(decoded: &mut Decoded, path: &str, value: OwnedValue) {
        saveprism_core::apply_edits(
            &mut decoded.doc,
            &[Edit::Set {
                path: Path::parse(path).expect("path"),
                value,
            }],
            &Limits::default(),
        )
        .expect("edit applies");
    }

    #[test]
    fn parses_multiple_values_and_edits_scalars_exactly() {
        let mut bytes = Vec::new();
        word(2 | (1 << 16), &mut bytes);
        bytes.extend_from_slice(&9_007_199_254_740_991i64.to_le_bytes());
        word(4, &mut bytes);
        string_payload("é", &mut bytes);
        let mut decoded = round_trip(&bytes);
        set(&mut decoded, "/0", OwnedValue::Int(8));
        let out = encode(&decoded);
        assert_eq!(&out[4..12], &8i64.to_le_bytes());
        assert_eq!(&out[12..], &bytes[12..]);
    }

    #[test]
    fn edits_all_binary_scalar_kinds_and_keeps_widths() {
        let mut bytes = Vec::new();
        word(1, &mut bytes);
        word(0, &mut bytes);
        word(2, &mut bytes);
        bytes.extend_from_slice(&(-3i32).to_le_bytes());
        word(3, &mut bytes);
        bytes.extend_from_slice(&1.5f32.to_le_bytes());
        word(4, &mut bytes);
        string_payload("a", &mut bytes);
        let mut decoded = round_trip(&bytes);
        set(&mut decoded, "/0", OwnedValue::Bool(true));
        set(&mut decoded, "/1", OwnedValue::Int(8));
        set(&mut decoded, "/2", OwnedValue::Float(2.25));
        set(&mut decoded, "/3", OwnedValue::Str("é".into()));
        let encoded = encode(&decoded);
        let reread = decode(&encoded);
        assert!(decoded.doc.tree_eq(&reread.doc));
        assert_eq!(encoded.get(4..8), Some(1u32.to_le_bytes().as_slice()));
    }

    #[test]
    fn godot3_type_22_is_a_float_pool_not_a_node_path() {
        // Godot 3: Dictionary (18) {"hp": PoolRealArray (22) [1.5, 2.5]}; in Godot 4, 22 is NodePath.
        let mut bytes = Vec::new();
        word(18, &mut bytes);
        word(1, &mut bytes);
        bytes.extend_from_slice(&string_value("hp"));
        word(22, &mut bytes);
        word(2, &mut bytes);
        bytes.extend_from_slice(&1.5f32.to_le_bytes());
        bytes.extend_from_slice(&2.5f32.to_le_bytes());
        let decoded = round_trip(&bytes);
        let at = |path: &str| {
            let id = decoded.doc.resolve(&Path::parse(path).unwrap()).unwrap();
            decoded.doc.get(id).unwrap().value.clone()
        };
        // The whole pool (header, count, two f32) is one read-only value, not a NodePath.
        assert!(matches!(at("/0/0"), Value::Bytes(ref raw) if raw.len() == 16));
        assert!(matches!(at("/0"), Value::Map(ref entries) if entries.len() == 1));
    }

    #[test]
    fn godot4_dictionary_keys_stay_nodes_and_vector_components_edit() {
        let mut bytes = DictionaryBuilder4::new();
        let key = {
            let mut out = Vec::new();
            word(2, &mut out);
            out.extend_from_slice(&7i32.to_le_bytes());
            out
        };
        bytes.entry(key, string_value("seven"));
        let value = {
            let mut out = Vec::new();
            word(5, &mut out);
            out.extend_from_slice(&1.0f32.to_le_bytes());
            out.extend_from_slice(&2.0f32.to_le_bytes());
            out
        };
        bytes.entry(string_value("position"), value);
        let bytes = bytes.finish();
        let mut decoded = round_trip(&bytes);
        assert!(matches!(
            decoded.doc.get(decoded.doc.root()).map(|n| &n.value),
            Some(Value::Array(_))
        ));
        set(&mut decoded, "/0/0.key", OwnedValue::Int(9));
        set(&mut decoded, "/0/1/x", OwnedValue::Float(4.5));
        let out = encode(&decoded);
        assert_eq!(out.get(12..16), Some(9i32.to_le_bytes().as_slice()));
        let reread = decode(&out);
        assert!(decoded.doc.tree_eq(&reread.doc));
    }

    struct DictionaryBuilder4 {
        entries: Vec<(Vec<u8>, Vec<u8>)>,
    }

    impl DictionaryBuilder4 {
        fn new() -> Self {
            Self {
                entries: Vec::new(),
            }
        }
        fn entry(&mut self, key: Vec<u8>, value: Vec<u8>) {
            self.entries.push((key, value));
        }
        fn finish(self) -> Vec<u8> {
            dictionary(&self.entries)
        }
    }

    #[test]
    fn godot3_and_godot4_are_told_apart() {
        let value = string_value("old");
        let mut three = Vec::new();
        word(18, &mut three);
        word(1, &mut three);
        three.extend_from_slice(&string_value("player"));
        three.extend_from_slice(&value);
        let decoded = round_trip(&three);
        assert_eq!(decoded.params.get("version"), Some(&ParamValue::Int(3)));

        let four = dictionary(&[(string_value("player"), value)]);
        let decoded = round_trip(&four);
        assert_eq!(decoded.params.get("version"), Some(&ParamValue::Int(4)));
        assert!(matches!(
            decoded.doc.get(decoded.doc.root()).map(|n| &n.value),
            Some(Value::Array(_))
        ));
    }

    #[test]
    fn generated_binary_fixtures_round_trip_with_the_right_version() {
        for (name, version) in [("godot4.dat", 4), ("godot3.sav", 3)] {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../fixtures/synthetic/godot")
                .join(name);
            let bytes = std::fs::read(path).expect("fixture exists");
            let decoded = round_trip(&bytes);
            assert_eq!(
                decoded.params.get("version"),
                Some(&ParamValue::Int(version)),
                "{name}"
            );
        }
    }

    #[test]
    fn full_objects_are_read_only_data_and_instance_ids_stay_read_only() {
        let mut object = Vec::new();
        word(24, &mut object);
        string_payload("Player", &mut object);
        word(1, &mut object);
        string_payload("hp", &mut object);
        word(2, &mut object);
        object.extend_from_slice(&5i32.to_le_bytes());
        let decoded = round_trip(&object);
        let hp = decoded
            .doc
            .resolve(&Path::parse("/0/hp").expect("path"))
            .expect("hp property");
        assert!(decoded.doc.get(hp).is_some_and(|node| {
            node.flags.contains(NodeFlags::READ_ONLY)
                && matches!(node.value, Value::Int { v: 5, .. })
        }));
        assert!(matches!(
            decoded.doc.get(decoded.doc.root()).map(|node| &node.value),
            Some(Value::Array(_))
        ));

        let mut instance = Vec::new();
        word(24 | (1 << 16), &mut instance);
        instance.extend_from_slice(&99u64.to_le_bytes());
        let decoded = round_trip(&instance);
        assert!(matches!(
            decoded.doc.get(decoded.doc.root()).map(|node| &node.value),
            Some(Value::Array(_))
        ));
    }

    #[test]
    fn rejects_encrypted_header_clearly() {
        let error = GodotCodec
            .decode(b"GDEC\0\0\0\0", &mut DecodeCtx::new(&Limits::default()))
            .expect_err("encrypted data is rejected");
        assert_eq!(error.code(), GODOT_ENCRYPTED);
        assert!(error.to_string().contains("game's key"));
    }

    #[test]
    fn rejects_truncated_and_implausible_values() {
        for bytes in [
            &[2, 0, 0][..],
            &[19, 0, 0, 0, 255, 255, 255, 127][..],
            &[28, 0, 0, 0, 255, 255, 255, 127][..],
        ] {
            assert!(
                GodotCodec
                    .decode(bytes, &mut DecodeCtx::new(&Limits::default()))
                    .is_err()
            );
        }
    }

    use proptest::prelude::*;

    fn writes_back(bytes: &[u8]) {
        if let Ok(decoded) = GodotCodec.decode(bytes, &mut DecodeCtx::new(&Limits::default())) {
            let written = GodotCodec
                .encode(&decoded.doc, &decoded.params)
                .expect("decoded tree writes");
            let again = GodotCodec
                .decode(&written, &mut DecodeCtx::new(&Limits::default()))
                .expect("written tree decodes");
            assert!(decoded.doc.tree_eq(&again.doc));
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(512))]

        #[test]
        fn random_input_never_panics(body in proptest::collection::vec(any::<u8>(), 0..256)) {
            writes_back(&body);
        }

        #[test]
        fn variant_like_input_never_panics(
            body in proptest::collection::vec(
                prop_oneof![
                    proptest::sample::select(vec![0u8, 1, 2, 3, 4, 5, 9, 17, 18, 19, 24, 27, 28, 29, 34, 37]),
                    0u8..=40,
                    any::<u8>(),
                ],
                0..96,
            )
        ) {
            writes_back(&body);
        }
    }
}
