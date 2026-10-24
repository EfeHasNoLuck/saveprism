use super::{JAVA_PARSE, JAVA_UNSUPPORTED, PREFIX};

const MAX_DEPTH: usize = 1024;
use saveprism_core::{
    ByteSpan, DecodeCtx, Error, FloatWidth, IntWidth, Node, NodeFlags, NodeId, Params, Result,
    SaveDoc, TypeHint, Value,
};

#[derive(Clone)]
struct Field {
    code: u8,
    name: String,
}
#[derive(Clone)]
struct Desc {
    name: String,
    flags: u8,
    fields: Vec<Field>,
    parent: Option<Box<Desc>>,
}
#[derive(Clone)]
enum Handle {
    Node(NodeId),
    Desc(Desc),
}

pub(super) fn decode(input: &[u8], ctx: &DecodeCtx<'_>) -> Result<(SaveDoc, Params)> {
    if input.get(..4) != Some(&[0xac, 0xed, 0, 5]) {
        return Err(Error::new(JAVA_PARSE, "invalid Java serialization header"));
    }
    let mut p = Parser {
        input,
        pos: 4,
        doc: ctx.new_doc(),
        handles: Vec::new(),
        references: Vec::new(),
        ctx,
    };
    let mut values = Vec::new();
    while p.pos < input.len() {
        values.push(p.content(1)?);
    }
    for id in p.references.drain(..) {
        if let Some(node) = p.doc.get(id).filter(|node| {
            node.type_hint
                .as_ref()
                .is_some_and(|hint| hint.as_str() == "JAVASER:string")
        }) {
            let mut flags = node.flags;
            flags.insert(NodeFlags::READ_ONLY);
            p.doc.set_flags(id, flags)?;
        }
    }
    let root = p.push(Value::Array(values), "stream", 0, input.len(), false)?;
    p.doc.set_root(root);
    Ok((p.doc, Params::new()))
}

struct Parser<'a, 'c, 'l> {
    input: &'a [u8],
    pos: usize,
    doc: SaveDoc,
    handles: Vec<Handle>,
    references: Vec<NodeId>,
    ctx: &'c DecodeCtx<'l>,
}
impl Parser<'_, '_, '_> {
    fn err(&self, m: impl Into<String>, at: usize) -> Error {
        Error::new(JAVA_PARSE, m).with_offset(at as u64)
    }
    fn take(&mut self, n: usize) -> Result<&[u8]> {
        let start = self.pos;
        let end = start
            .checked_add(n)
            .filter(|e| *e <= self.input.len())
            .ok_or_else(|| self.err("truncated Java serialization stream", start))?;
        self.pos = end;
        self.input
            .get(start..end)
            .ok_or_else(|| self.err("truncated Java serialization stream", start))
    }
    fn u8(&mut self) -> Result<u8> {
        self.take(1)?
            .first()
            .copied()
            .ok_or_else(|| self.err("truncated byte", self.pos))
    }
    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_be_bytes(
            self.take(2)?
                .try_into()
                .map_err(|_| self.err("truncated u16", self.pos))?,
        ))
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_be_bytes(
            self.take(4)?
                .try_into()
                .map_err(|_| self.err("truncated u32", self.pos))?,
        ))
    }
    fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_be_bytes(
            self.take(8)?
                .try_into()
                .map_err(|_| self.err("truncated u64", self.pos))?,
        ))
    }
    fn node(
        &mut self,
        v: Value,
        kind: &str,
        start: usize,
        end: usize,
        editable: bool,
    ) -> Result<NodeId> {
        let mut n = Node::new(v);
        n.type_hint = Some(TypeHint::new(format!("{PREFIX}{kind}")));
        n.span = Some(ByteSpan {
            start: start as u64,
            end: end as u64,
        });
        if !editable {
            n.flags.insert(NodeFlags::READ_ONLY);
        }
        self.doc.push(n).map_err(|e| e.with_offset(start as u64))
    }
    fn handle(&mut self, h: Handle) {
        self.handles.push(h);
    }
    fn depth(&self, d: usize) -> Result<()> {
        self.ctx
            .check_depth(d.min(MAX_DEPTH + 1))
            .and_then(|()| {
                if d <= MAX_DEPTH {
                    Ok(())
                } else {
                    Err(Error::new(
                        saveprism_core::codes::CORE_LIMIT_DEPTH,
                        "Java serialization nesting exceeds the codec safety cap",
                    ))
                }
            })
            .map_err(|e| e.with_offset(self.pos as u64))
    }
    fn utf(&mut self) -> Result<String> {
        let len = usize::from(self.u16()?);
        let at = self.pos;
        let bytes = self.take(len)?;
        decode_mutf8(bytes).ok_or_else(|| self.err("invalid modified UTF-8 text", at))
    }
    fn content(&mut self, depth: usize) -> Result<NodeId> {
        self.depth(depth)?;
        let start = self.pos;
        let token = self.u8()?;
        match token {
            0x70 => self.node(Value::Null, "null", start, self.pos, false),
            0x71 => {
                let h = self
                    .u32()?
                    .checked_sub(0x7e0000)
                    .ok_or_else(|| self.err("invalid handle", start))?
                    as usize;
                match self
                    .handles
                    .get(h)
                    .cloned()
                    .ok_or_else(|| self.err("handle is outside the table", start))?
                {
                    Handle::Node(target) => {
                        self.references.push(target);
                        self.node(Value::Ref(target), "reference", start, self.pos, false)
                    }
                    _ => self.node(Value::Null, "reference-metadata", start, self.pos, false),
                }
            }
            0x72 => {
                let d = self.class_desc_body(start, depth + 1)?;
                let id = self.desc_node(&d, start)?;
                Ok(id)
            }
            0x73 => self.object(start, depth + 1),
            0x74 | 0x7c => self.string(start, token == 0x7c),
            0x75 => self.array(start, depth + 1),
            0x76 => {
                let desc = self
                    .descriptor(depth)?
                    .ok_or_else(|| self.err("class has a null descriptor", start))?;
                let id = self.node(Value::Str(desc.name), "class", start, self.pos, false)?;
                self.handle(Handle::Node(id));
                Ok(id)
            }
            0x7e => self.enum_value(start, depth + 1),
            0x77 | 0x7a => {
                let n = if token == 0x77 {
                    usize::from(self.u8()?)
                } else {
                    usize::try_from(self.u32()?)
                        .map_err(|_| self.err("block length overflow", start))?
                };
                if n > self.input.len().saturating_sub(self.pos) {
                    return Err(self.err("block data length exceeds input", start));
                }
                let bytes = self.take(n)?.to_vec();
                self.node(Value::Bytes(bytes), "block-data", start, self.pos, false)
            }
            0x79 => {
                self.handles.clear();
                self.node(Value::Null, "reset", start, self.pos, false)
            }
            0x7b => Err(
                Error::new(JAVA_UNSUPPORTED, "TC_EXCEPTION streams are unsupported")
                    .with_offset(start as u64),
            ),
            0x78 => self.node(Value::Null, "end-block-data", start, self.pos, false),
            0x7d => {
                let desc = self.proxy_desc(start, depth + 1)?;
                self.desc_node(&desc, start)
            }
            x => Err(self.err(format!("unsupported serialization token 0x{x:02x}"), start)),
        }
    }
    fn string(&mut self, start: usize, long: bool) -> Result<NodeId> {
        let len = if long {
            usize::try_from(self.u64()?).map_err(|_| self.err("string length overflow", start))?
        } else {
            usize::from(self.u16()?)
        };
        if len > self.input.len().saturating_sub(self.pos) {
            return Err(self.err("string length exceeds input", start));
        }
        let raw = self.take(len)?;
        let text =
            decode_mutf8(raw).ok_or_else(|| self.err("invalid modified UTF-8 string", start))?;
        let mut node = Node::new(Value::Str(text.clone()));
        node.type_hint = Some(TypeHint::new(format!("{PREFIX}string")));
        node.span = Some(ByteSpan {
            start: start as u64,
            end: self.pos as u64,
        });
        node.lexeme = Some(text.clone().into_boxed_str());
        let id = self
            .doc
            .push(node)
            .map_err(|e| e.with_offset(start as u64))?;
        self.handle(Handle::Node(id));
        Ok(id)
    }
    fn class_desc_body(&mut self, start: usize, depth: usize) -> Result<Desc> {
        self.depth(depth)?;
        let name = self.utf()?;
        let _uid = self.u64()?;
        let flags = self.u8()?;
        let count = usize::from(self.u16()?);
        if count > self.input.len().saturating_sub(self.pos) / 3 {
            return Err(self.err("class field count exceeds remaining input", start));
        }
        if count as u64 > self.ctx.limits().max_nodes {
            return Err(Error::new(
                saveprism_core::codes::CORE_LIMIT_NODES,
                "class field count exceeds the node limit",
            )
            .with_offset(start as u64));
        }
        let desc = Desc {
            name,
            flags,
            fields: Vec::new(),
            parent: None,
        };
        let handle_index = self.handles.len();
        self.handle(Handle::Desc(desc.clone()));
        let mut fields = Vec::with_capacity(count);
        for _ in 0..count {
            let code = self.u8()?;
            let field_name = self.utf()?;
            if matches!(code, b'L' | b'[') {
                let _ = self.content(depth + 1)?;
            }
            fields.push(Field {
                code,
                name: field_name,
            });
        }
        let _ = self.annotation(depth + 1)?;
        let parent_token = self.u8()?;
        let parent = match parent_token {
            0x70 => None,
            0x71 => {
                let h = self
                    .u32()?
                    .checked_sub(0x7e0000)
                    .ok_or_else(|| self.err("invalid class descriptor handle", self.pos - 4))?
                    as usize;
                match self.handles.get(h) {
                    Some(Handle::Desc(d)) => Some(Box::new(d.clone())),
                    _ => {
                        return Err(self.err("class descriptor reference is invalid", self.pos - 5));
                    }
                }
            }
            0x72 => Some(Box::new(self.class_desc_body(self.pos - 1, depth + 1)?)),
            _ => return Err(self.err("invalid superclass descriptor", self.pos - 1)),
        };
        let result = Desc {
            name: desc.name,
            flags,
            fields,
            parent,
        };
        if let Some(slot) = self.handles.get_mut(handle_index) {
            *slot = Handle::Desc(result.clone());
        }
        Ok(result)
    }
    fn descriptor(&mut self, depth: usize) -> Result<Option<Desc>> {
        let token = self.u8()?;
        match token {
            0x70 => Ok(None),
            0x71 => {
                let at = self.pos - 1;
                let h = self
                    .u32()?
                    .checked_sub(0x7e0000)
                    .ok_or_else(|| self.err("invalid class descriptor handle", at))?
                    as usize;
                match self.handles.get(h) {
                    Some(Handle::Desc(d)) => Ok(Some(d.clone())),
                    _ => Err(self.err("class descriptor reference is invalid", at)),
                }
            }
            0x72 => self.class_desc_body(self.pos - 1, depth + 1).map(Some),
            _ => Err(self.err("expected class descriptor", self.pos - 1)),
        }
    }
    fn enum_value(&mut self, start: usize, depth: usize) -> Result<NodeId> {
        let desc = self
            .descriptor(depth)?
            .ok_or_else(|| self.err("enum has a null descriptor", start))?;
        let id = self.doc.reserve()?;
        self.handle(Handle::Node(id));
        let name = self.content(depth + 1)?;
        let mut node = Node::new(Value::Object(vec![("name".into(), name)]));
        node.type_hint = Some(TypeHint::new(format!("{PREFIX}enum:{}", desc.name)));
        node.flags.insert(NodeFlags::READ_ONLY);
        node.span = Some(ByteSpan {
            start: start as u64,
            end: self.pos as u64,
        });
        self.doc.finish(id, node)?;
        Ok(id)
    }
    fn proxy_desc(&mut self, start: usize, depth: usize) -> Result<Desc> {
        self.depth(depth)?;
        let count = usize::try_from(self.u32()?)
            .map_err(|_| self.err("proxy interface count overflow", start))?;
        if count as u64 > self.ctx.limits().max_nodes {
            return Err(Error::new(
                saveprism_core::codes::CORE_LIMIT_NODES,
                "proxy interface count exceeds the node limit",
            )
            .with_offset(start as u64));
        }
        if count > self.input.len().saturating_sub(self.pos) / 2 {
            return Err(self.err("proxy interface count exceeds input", start));
        }
        let mut names = Vec::with_capacity(count);
        for _ in 0..count {
            names.push(self.utf()?);
        }
        let desc = Desc {
            name: format!("proxy({})", names.join(",")),
            flags: 0x02,
            fields: Vec::new(),
            parent: None,
        };
        let handle_index = self.handles.len();
        self.handle(Handle::Desc(desc.clone()));
        let _ = self.annotation(depth + 1)?;
        let parent = self.descriptor(depth + 1)?;
        let result = Desc {
            parent: parent.map(Box::new),
            ..desc
        };
        if let Some(slot) = self.handles.get_mut(handle_index) {
            *slot = Handle::Desc(result.clone());
        }
        Ok(result)
    }
    fn object(&mut self, start: usize, depth: usize) -> Result<NodeId> {
        let desc = self
            .descriptor(depth)?
            .ok_or_else(|| self.err("object has a null class descriptor", start))?;
        let id = self.doc.reserve()?;
        self.handle(Handle::Node(id));
        let mut fields = Vec::new();
        let mut custom_data = Vec::new();
        let mut hierarchy = Vec::new();
        let mut d = Some(&desc);
        while let Some(cur) = d {
            hierarchy.push(cur.clone());
            d = cur.parent.as_deref();
        }
        hierarchy.reverse();
        for cls in hierarchy {
            for f in &cls.fields {
                let value = self.field_value(f.code, depth + 1)?;
                fields.push((f.name.clone(), value));
            }
            if cls.flags & 0x01 != 0 {
                custom_data.extend(self.annotation(depth + 1)?);
            }
            if cls.flags & 0x04 != 0 && cls.flags & 0x08 == 0 {
                return Err(Error::new(
                    JAVA_UNSUPPORTED,
                    "protocol 1 Externalizable class data is unsupported",
                )
                .with_offset(start as u64));
            }
            if cls.flags & 0x04 != 0 {
                custom_data.extend(self.annotation(depth + 1)?);
            }
        }
        match desc.name.as_str() {
            "java.util.ArrayList" | "java.util.Vector" | "java.util.HashSet" => {
                let start_at = self.pos;
                let items = if desc.name == "java.util.HashSet" {
                    custom_data.as_slice()
                } else {
                    custom_data.get(1..).unwrap_or(&[])
                };
                let elements =
                    self.container(Value::Array(items.to_vec()), "elements", start_at)?;
                fields.push(("elements".into(), elements));
            }
            "java.util.HashMap"
            | "java.util.LinkedHashMap"
            | "java.util.TreeMap"
            | "java.util.Hashtable" => {
                let start_at = self.pos;
                let data = custom_data.get(1..).unwrap_or(&[]);
                let mut pairs = Vec::new();
                for (key, value) in data.iter().step_by(2).zip(data.iter().skip(1).step_by(2)) {
                    pairs.push(self.container(
                        Value::Array(vec![*key, *value]),
                        "entry",
                        start_at,
                    )?);
                }
                let entries = self.container(Value::Array(pairs), "entries", start_at)?;
                fields.push(("entries".into(), entries));
            }
            _ if !custom_data.is_empty() => {
                let annotation = self.container(Value::Array(custom_data), "custom-data", start)?;
                fields.push(("(customData)".into(), annotation));
            }
            _ => {}
        }
        let mut node = Node::new(Value::Object(fields));
        node.type_hint = Some(TypeHint::new(format!("{PREFIX}object:{}", desc.name)));
        node.span = Some(ByteSpan {
            start: start as u64,
            end: self.pos as u64,
        });
        self.doc.finish(id, node)?;
        Ok(id)
    }
    fn field_value(&mut self, code: u8, depth: usize) -> Result<NodeId> {
        let start = self.pos;
        let value = match code {
            b'B' => Value::Int {
                v: i128::from(self.u8()? as i8),
                width: IntWidth::I8,
            },
            b'C' => Value::Int {
                v: i128::from(self.u16()?),
                width: IntWidth::U16,
            },
            b'S' => Value::Int {
                v: i128::from(self.u16()? as i16),
                width: IntWidth::I16,
            },
            b'I' => Value::Int {
                v: i128::from(self.u32()? as i32),
                width: IntWidth::I32,
            },
            b'J' => Value::Int {
                v: i128::from(self.u64()? as i64),
                width: IntWidth::I64,
            },
            b'Z' => {
                let b = self.u8()?;
                if b > 1 {
                    return Err(self.err("invalid boolean field", start));
                }
                Value::Bool(b == 1)
            }
            b'F' => Value::Float {
                v: f32::from_bits(self.u32()?) as f64,
                width: FloatWidth::F32,
            },
            b'D' => Value::Float {
                v: f64::from_bits(self.u64()?),
                width: FloatWidth::F64,
            },
            b'L' | b'[' => return self.content(depth),
            _ => return Err(self.err("invalid field type code", start)),
        };
        self.node(
            value,
            match code {
                b'B' => "byte",
                b'C' => "char",
                b'S' => "short",
                b'I' => "int",
                b'J' => "long",
                b'Z' => "boolean",
                b'F' => "float",
                _ => "double",
            },
            start,
            self.pos,
            true,
        )
    }
    fn array(&mut self, start: usize, depth: usize) -> Result<NodeId> {
        let desc = self
            .descriptor(depth)?
            .ok_or_else(|| self.err("array has a null descriptor", start))?;
        let id = self.doc.reserve()?;
        self.handle(Handle::Node(id));
        let len =
            usize::try_from(self.u32()?).map_err(|_| self.err("array length overflow", start))?;
        if len as u64 > self.ctx.limits().max_nodes {
            return Err(Error::new(
                saveprism_core::codes::CORE_LIMIT_NODES,
                "array element count exceeds the node limit",
            )
            .with_offset(start as u64));
        }
        if len > self.input.len().saturating_sub(self.pos) {
            return Err(self.err("array length exceeds remaining input", start));
        }
        let code = desc.name.as_bytes().get(1).copied().unwrap_or(0);
        let mut values = Vec::with_capacity(len);
        for _ in 0..len {
            values.push(
                if matches!(code, b'B' | b'C' | b'S' | b'I' | b'J' | b'Z' | b'F' | b'D') {
                    self.field_value(code, depth + 1)?
                } else {
                    self.content(depth + 1)?
                },
            );
        }
        let mut node = Node::new(Value::Array(values));
        node.type_hint = Some(TypeHint::new(format!("{PREFIX}array")));
        node.span = Some(ByteSpan {
            start: start as u64,
            end: self.pos as u64,
        });
        node.flags.insert(NodeFlags::READ_ONLY);
        self.doc.finish(id, node)?;
        Ok(id)
    }
    fn annotation(&mut self, depth: usize) -> Result<Vec<NodeId>> {
        let mut values = Vec::new();
        loop {
            if self.input.get(self.pos) == Some(&0x78) {
                self.pos += 1;
                return Ok(values);
            }
            let before = self.pos;
            values.push(self.content(depth + 1)?);
            if self.pos == before {
                return Err(self.err("annotation parser made no progress", before));
            }
        }
    }
    fn container(&mut self, value: Value, kind: &str, at: usize) -> Result<NodeId> {
        let mut node = Node::new(value);
        node.type_hint = Some(TypeHint::new(format!("{PREFIX}{kind}")));
        node.flags.insert(NodeFlags::READ_ONLY);
        node.span = Some(ByteSpan {
            start: at as u64,
            end: at as u64,
        });
        self.doc.push(node).map_err(|e| e.with_offset(at as u64))
    }
    fn desc_node(&mut self, desc: &Desc, start: usize) -> Result<NodeId> {
        self.node(
            Value::Str(desc.name.clone()),
            "class-descriptor",
            start,
            self.pos,
            false,
        )
    }
    fn push(
        &mut self,
        v: Value,
        hint: &str,
        start: usize,
        end: usize,
        editable: bool,
    ) -> Result<NodeId> {
        self.node(v, hint, start, end, editable)
    }
}

fn decode_mutf8(bytes: &[u8]) -> Option<String> {
    let mut units = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let a = *bytes.get(i)?;
        i += 1;
        let u = if a & 0x80 == 0 {
            u16::from(a)
        } else if a & 0xe0 == 0xc0 {
            let b = *bytes.get(i)?;
            i += 1;
            if b & 0xc0 != 0x80 {
                return None;
            }
            (u16::from(a & 0x1f) << 6) | u16::from(b & 0x3f)
        } else if a & 0xf0 == 0xe0 {
            let b = *bytes.get(i)?;
            let c = *bytes.get(i + 1)?;
            i += 2;
            if b & 0xc0 != 0x80 || c & 0xc0 != 0x80 {
                return None;
            }
            (u16::from(a & 0x0f) << 12) | (u16::from(b & 0x3f) << 6) | u16::from(c & 0x3f)
        } else {
            return None;
        };
        units.push(u);
    }
    String::from_utf16(&units).ok()
}
