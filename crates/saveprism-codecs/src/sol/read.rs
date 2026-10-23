use std::collections::{HashMap, HashSet};

use saveprism_core::limits::len_u64;
use saveprism_core::{
    Error, IntWidth, Limits, Node, NodeFlags, NodeId, ParamValue, Params, Result, SaveDoc,
    TypeHint, Value,
};

use super::{ORIGINAL, PREFIX, SOL_PARSE, SOL_UNSUPPORTED, STRING_NO_EMPTY, parse_error};

const AMF0: &str = "SOL:amf0";
const AMF3: &str = "SOL:amf3";

pub(super) fn decode(input: &[u8], limits: &Limits) -> Result<(SaveDoc, Params)> {
    if input.len() < 18 || input.get(..2) != Some(&[0, 0xbf]) || input.get(6..10) != Some(b"TCSO") {
        return Err(parse_error("not an Adobe Local Shared Object", 0));
    }
    let declared = be_u32(input, 2)? as usize;
    if declared != input.len().saturating_sub(6) {
        return Err(parse_error("LSO header length does not match the file", 2));
    }
    if input.get(10..16) != Some(&[0, 4, 0, 0, 0, 0]) {
        return Err(Error::new(
            SOL_UNSUPPORTED,
            "unsupported LSO header version",
        ));
    }
    let mut pos = 16;
    let name_len = be_u16(input, pos)? as usize;
    pos += 2;
    let name_end = pos
        .checked_add(name_len)
        .filter(|end| *end <= input.len())
        .ok_or_else(|| parse_error("truncated LSO name", pos))?;
    std::str::from_utf8(
        input
            .get(pos..name_end)
            .ok_or_else(|| parse_error("truncated LSO name", pos))?,
    )
    .map_err(|_| parse_error("LSO name is not UTF-8", pos))?;
    pos = name_end;
    let version = be_u32(input, pos)?;
    pos += 4;
    if version != 0 && version != 3 {
        return Err(Error::new(
            SOL_UNSUPPORTED,
            format!("AMF version {version} is not supported"),
        ));
    }
    let mut parser = Parser::new(input, pos, limits);
    let mut entries = Vec::new();
    while parser.pos < input.len() {
        let key = if version == 0 {
            parser.amf0_name()?
        } else {
            parser.amf3_string()?.0
        };
        let value = if version == 0 {
            parser.amf0_value(1)?
        } else {
            parser.amf3_value(1)?
        };
        if parser.byte()? != 0 {
            return Err(parse_error(
                "invalid LSO property terminator",
                parser.pos - 1,
            ));
        }
        entries.push((key, value));
    }
    parser.lock_referenced_strings()?;
    parser.lock_referenced_objects()?;
    let mut root = Node::new(Value::Object(entries));
    root.type_hint = Some(TypeHint::new(if version == 0 { AMF0 } else { AMF3 }));
    root.flags.insert(NodeFlags::READ_ONLY);
    let root = parser.doc.push(root)?;
    parser.doc.set_root(root);
    let mut params = Params::new();
    params.set(ORIGINAL, input.to_vec());
    params.set(
        STRING_NO_EMPTY,
        std::mem::take(&mut parser.no_empty_strings)
            .into_iter()
            .map(|index| ParamValue::Int(i64::from(index)))
            .collect::<Vec<_>>(),
    );
    Ok((parser.doc, params))
}

#[derive(Clone)]
struct Trait {
    class: String,
    sealed: Vec<String>,
    dynamic: bool,
}
struct Parser<'a, 'l> {
    input: &'a [u8],
    pos: usize,
    limits: &'l Limits,
    doc: SaveDoc,
    amf0_objects: Vec<NodeId>,
    amf3_objects: Vec<NodeId>,
    strings: Vec<(String, NodeId)>,
    traits: Vec<Trait>,
    string_ref_targets: Vec<NodeId>,
    string_values: HashMap<NodeId, NodeId>,
    string_references: Vec<usize>,
    string_definitions: Vec<(NodeId, usize, bool)>,
    no_empty_strings: Vec<u32>,
    object_ref_targets: Vec<NodeId>,
}

impl<'a, 'l> Parser<'a, 'l> {
    fn new(input: &'a [u8], pos: usize, limits: &'l Limits) -> Self {
        Self {
            input,
            pos,
            limits,
            doc: SaveDoc::with_node_limit(limits.max_nodes),
            amf0_objects: Vec::new(),
            amf3_objects: Vec::new(),
            strings: Vec::new(),
            traits: Vec::new(),
            string_ref_targets: Vec::new(),
            string_values: HashMap::new(),
            string_references: Vec::new(),
            string_definitions: Vec::new(),
            no_empty_strings: Vec::new(),
            object_ref_targets: Vec::new(),
        }
    }
    fn byte(&mut self) -> Result<u8> {
        let at = self.pos;
        let b = *self
            .input
            .get(at)
            .ok_or_else(|| parse_error("unexpected end of LSO", at))?;
        self.pos += 1;
        Ok(b)
    }
    fn take(&mut self, len: usize) -> Result<&'a [u8]> {
        let at = self.pos;
        let end = at
            .checked_add(len)
            .filter(|end| *end <= self.input.len())
            .ok_or_else(|| parse_error("truncated AMF value", at))?;
        self.pos = end;
        self.input
            .get(at..end)
            .ok_or_else(|| parse_error("truncated AMF value", at))
    }
    fn u16(&mut self) -> Result<u16> {
        let bytes = self
            .take(2)?
            .try_into()
            .map_err(|_| parse_error("truncated AMF value", self.pos))?;
        Ok(u16::from_be_bytes(bytes))
    }
    fn u32(&mut self) -> Result<u32> {
        let bytes = self
            .take(4)?
            .try_into()
            .map_err(|_| parse_error("truncated AMF value", self.pos))?;
        Ok(u32::from_be_bytes(bytes))
    }
    fn text(&mut self, len: usize) -> Result<String> {
        let at = self.pos;
        let bytes = self.take(len)?;
        String::from_utf8(bytes.to_vec()).map_err(|_| parse_error("AMF string is not UTF-8", at))
    }
    fn node(
        &mut self,
        value: Value,
        kind: &str,
        start: usize,
        end: usize,
        editable: bool,
    ) -> Result<NodeId> {
        let mut n = Node::new(value);
        n.type_hint = Some(TypeHint::new(format!("{PREFIX}{kind}")));
        n.span = Some(saveprism_core::ByteSpan {
            start: len_u64(start),
            end: len_u64(end),
        });
        if !editable {
            n.flags.insert(NodeFlags::READ_ONLY);
        }
        self.doc.push(n).map_err(|e| e.with_offset(len_u64(start)))
    }
    fn depth(&self, depth: usize) -> Result<()> {
        if depth > self.limits.max_depth {
            return Err(Error::new(
                saveprism_core::codes::CORE_LIMIT_DEPTH,
                format!(
                    "nesting depth {depth} is above the limit of {}",
                    self.limits.max_depth
                ),
            )
            .with_offset(len_u64(self.pos)));
        }
        Ok(())
    }
    fn amf0_name(&mut self) -> Result<String> {
        let len = self.u16()? as usize;
        self.text(len)
    }
    fn amf0_pairs(&mut self, depth: usize, end_marker: bool) -> Result<Vec<(String, NodeId)>> {
        let mut entries = Vec::new();
        loop {
            self.depth(depth)?;
            if end_marker
                && self
                    .pos
                    .checked_add(3)
                    .is_some_and(|end| self.input.get(self.pos..end) == Some(&[0, 0, 9]))
            {
                self.pos += 3;
                break;
            }
            if self.pos >= self.input.len() {
                return Err(parse_error("unterminated AMF0 object", self.pos));
            }
            let name = self.amf0_name()?;
            let value = self.amf0_value(depth + 1)?;
            entries.push((name, value));
        }
        Ok(entries)
    }
    fn amf0_value(&mut self, depth: usize) -> Result<NodeId> {
        self.depth(depth)?;
        let start = self.pos;
        let marker = self.byte()?;
        match marker {
            0 => {
                let v = f64::from_bits(u64::from_be_bytes(
                    self.take(8)?
                        .try_into()
                        .map_err(|_| parse_error("truncated AMF0 number", start))?,
                ));
                self.node(
                    Value::Float {
                        v,
                        width: saveprism_core::FloatWidth::F64,
                    },
                    "amf0-number",
                    start,
                    self.pos,
                    true,
                )
            }
            1 => {
                let b = self.byte()?;
                if b > 1 {
                    return Err(parse_error("invalid AMF0 boolean", self.pos - 1));
                }
                self.node(Value::Bool(b != 0), "amf0-bool", start, self.pos, true)
            }
            2 | 12 => {
                let len = if marker == 2 {
                    self.u16()? as usize
                } else {
                    usize::try_from(self.u32()?)
                        .map_err(|_| parse_error("AMF0 string length is too large", self.pos))?
                };
                let s = self.text(len)?;
                self.node(
                    Value::Str(s),
                    if marker == 2 {
                        "amf0-string"
                    } else {
                        "amf0-long-string"
                    },
                    start,
                    self.pos,
                    true,
                )
            }
            3 | 8 | 16 => {
                let id = self.doc.reserve()?;
                self.amf0_objects.push(id);
                let class = if marker == 16 {
                    self.amf0_name()?
                } else {
                    String::new()
                };
                if marker == 8 {
                    let _ = self.u32()?;
                }
                let entries = self.amf0_pairs(depth + 1, true)?;
                let mut n = Node::new(Value::Object(entries));
                n.type_hint = Some(TypeHint::new(format!("{PREFIX}amf0-object:{class}")));
                n.span = Some(saveprism_core::ByteSpan {
                    start: len_u64(start),
                    end: len_u64(self.pos),
                });
                n.flags.insert(NodeFlags::READ_ONLY);
                self.doc.finish(id, n)?;
                Ok(id)
            }
            5 | 6 => self.node(
                Value::Null,
                if marker == 5 {
                    "amf0-null"
                } else {
                    "amf0-undefined"
                },
                start,
                self.pos,
                false,
            ),
            7 => {
                let index = self.u16()? as usize;
                self.reference(index, false, start)
            }
            10 => {
                let count = self.u32()? as usize;
                self.count(count, 1)?;
                let id = self.doc.reserve()?;
                self.amf0_objects.push(id);
                let mut values = Vec::with_capacity(count.min(4096));
                for _ in 0..count {
                    values.push(self.amf0_value(depth + 1)?);
                }
                let mut node = Node::new(Value::Array(values));
                node.type_hint = Some(TypeHint::new(format!("{PREFIX}amf0-array")));
                node.span = Some(saveprism_core::ByteSpan {
                    start: len_u64(start),
                    end: len_u64(self.pos),
                });
                node.flags.insert(NodeFlags::READ_ONLY);
                self.doc.finish(id, node)?;
                Ok(id)
            }
            11 => {
                let value = f64::from_bits(u64::from_be_bytes(
                    self.take(8)?
                        .try_into()
                        .map_err(|_| parse_error("truncated AMF0 date", start))?,
                ));
                self.take(2)?;
                let id = self.node(
                    Value::Float {
                        v: value,
                        width: saveprism_core::FloatWidth::F64,
                    },
                    "amf0-date",
                    start,
                    self.pos,
                    false,
                )?;
                self.amf0_objects.push(id);
                Ok(id)
            }
            15 => {
                let len = self.u32()? as usize;
                self.take(len)?;
                let id = self.node(
                    Value::Bytes(self.input.get(start..self.pos).unwrap_or(&[]).to_vec()),
                    "amf0-xml",
                    start,
                    self.pos,
                    false,
                )?;
                self.amf0_objects.push(id);
                Ok(id)
            }
            17 => self.amf3_value(depth + 1),
            other => Err(Error::new(
                SOL_UNSUPPORTED,
                format!("unsupported AMF0 marker 0x{other:02x}"),
            )
            .with_offset(len_u64(start))),
        }
    }
    fn count(&self, count: usize, min_bytes: usize) -> Result<()> {
        let remaining = self.input.len().saturating_sub(self.pos);
        if count > remaining / min_bytes.max(1) {
            return Err(parse_error(
                "AMF collection count exceeds remaining input",
                self.pos,
            ));
        }
        if count as u64 > self.limits.max_nodes {
            return Err(Error::new(
                saveprism_core::codes::CORE_LIMIT_NODES,
                "AMF collection exceeds node limit",
            ));
        }
        Ok(())
    }
    fn reference(&mut self, index: usize, amf3: bool, start: usize) -> Result<NodeId> {
        let table = if amf3 {
            &self.amf3_objects
        } else {
            &self.amf0_objects
        };
        let target = *table.get(index).ok_or_else(|| {
            Error::new(
                SOL_PARSE,
                format!("object reference {index} is out of range"),
            )
            .with_offset(len_u64(start))
        })?;
        self.object_ref_targets.push(target);
        self.node(
            Value::Ref(target),
            if amf3 { "amf3-ref" } else { "amf0-ref" },
            start,
            self.pos,
            false,
        )
    }
    fn u29(&mut self) -> Result<u32> {
        let mut value = 0u32;
        for _ in 0..3 {
            let b = self.byte()?;
            if b & 0x80 == 0 {
                return Ok((value << 7) | u32::from(b));
            }
            value = (value << 7) | u32::from(b & 0x7f);
        }
        Ok((value << 8) | u32::from(self.byte()?))
    }
    fn amf3_string(&mut self) -> Result<(String, Option<NodeId>)> {
        let header = self.u29()?;
        if header & 1 == 0 {
            let index = (header >> 1) as usize;
            let (value, id) = self.strings.get(index).cloned().ok_or_else(|| {
                Error::new(
                    SOL_PARSE,
                    format!("string reference {index} is out of range"),
                )
                .with_offset(len_u64(self.pos))
            })?;
            self.string_ref_targets.push(id);
            self.string_references.push(index);
            return Ok((value, Some(id)));
        }
        let len = (header >> 1) as usize;
        if len == 0 {
            return Ok((String::new(), None));
        }
        let start = self.pos;
        let value = self.text(len)?;
        let id = self.node(
            Value::Str(value.clone()),
            "amf3-string-table",
            start,
            self.pos,
            false,
        )?;
        self.strings.push((value.clone(), id));
        Ok((value, Some(id)))
    }
    fn lock_referenced_strings(&mut self) -> Result<()> {
        for id in self.string_ref_targets.drain(..) {
            if let Some(value) = self.string_values.get(&id).copied()
                && let Some(n) = self.doc.get(value)
            {
                let mut flags = n.flags;
                flags.insert(NodeFlags::READ_ONLY);
                self.doc.set_flags(value, flags)?;
            }
        }
        for (id, index, was_empty) in std::mem::take(&mut self.string_definitions) {
            let invalidates_later_reference = self.string_references.iter().any(|referenced| {
                if was_empty {
                    *referenced >= index
                } else {
                    *referenced > index
                }
            });
            if was_empty && invalidates_later_reference {
                if let Some(node) = self.doc.get(id) {
                    let mut flags = node.flags;
                    flags.insert(NodeFlags::READ_ONLY);
                    self.doc.set_flags(id, flags)?;
                }
            } else if !was_empty && invalidates_later_reference {
                self.no_empty_strings.push(
                    u32::try_from(id.index()).map_err(|_| {
                        Error::new(SOL_PARSE, "node id exceeds the supported range")
                    })?,
                );
            }
        }
        Ok(())
    }
    fn lock_referenced_objects(&mut self) -> Result<()> {
        let mut seen = HashSet::new();
        let mut stack = std::mem::take(&mut self.object_ref_targets);
        while let Some(id) = stack.pop() {
            if !seen.insert(id) {
                continue;
            }
            let Some(node) = self.doc.get(id) else {
                continue;
            };
            let mut flags = node.flags;
            let value = node.value.clone();
            flags.insert(NodeFlags::READ_ONLY);
            self.doc.set_flags(id, flags)?;
            match value {
                Value::Array(children) => stack.extend(children),
                Value::Object(entries) => stack.extend(entries.into_iter().map(|(_, child)| child)),
                Value::Map(entries) => {
                    stack.extend(entries.into_iter().flat_map(|(key, value)| [key, value]))
                }
                Value::Ref(_)
                | Value::Null
                | Value::Bool(_)
                | Value::Int { .. }
                | Value::Float { .. }
                | Value::Str(_)
                | Value::Bytes(_) => {}
            }
        }
        Ok(())
    }
    fn amf3_value(&mut self, depth: usize) -> Result<NodeId> {
        self.depth(depth)?;
        let start = self.pos;
        let marker = self.byte()?;
        match marker {
            0 => self.node(Value::Null, "amf3-undefined", start, self.pos, false),
            1 => self.node(Value::Null, "amf3-null", start, self.pos, false),
            2 | 3 => self.node(Value::Bool(marker == 3), "amf3-bool", start, self.pos, true),
            4 => {
                let raw = self.u29()?;
                let signed = ((raw << 3) as i32 >> 3) as i64;
                self.node(
                    Value::Int {
                        v: i128::from(signed),
                        width: IntWidth::I32,
                    },
                    "amf3-int",
                    start,
                    self.pos,
                    true,
                )
            }
            5 => {
                let v = f64::from_bits(u64::from_be_bytes(
                    self.take(8)?
                        .try_into()
                        .map_err(|_| parse_error("truncated AMF3 double", start))?,
                ));
                self.node(
                    Value::Float {
                        v,
                        width: saveprism_core::FloatWidth::F64,
                    },
                    "amf3-double",
                    start,
                    self.pos,
                    true,
                )
            }
            6 => {
                let (text, table_id) = self.amf3_string()?;
                if self.input.get(start + 1).is_some_and(|b| b & 1 == 0)
                    && let Some(target) = table_id
                {
                    return self.node(Value::Ref(target), "amf3-ref", start, self.pos, false);
                }
                let was_empty = text.is_empty();
                let table_index = table_id
                    .and_then(|target| self.strings.iter().position(|(_, id)| *id == target))
                    .unwrap_or(self.strings.len());
                let id = self.node(Value::Str(text), "amf3-string", start, self.pos, true)?;
                self.string_definitions.push((id, table_index, was_empty));
                if let Some(target) = table_id {
                    self.string_values.insert(target, id);
                }
                Ok(id)
            }
            7 | 11 => self.amf3_binary(
                start,
                if marker == 7 {
                    "amf3-xml-doc"
                } else {
                    "amf3-xml"
                },
            ),
            8 => {
                let header = self.u29()?;
                if header & 1 == 0 {
                    return self.reference((header >> 1) as usize, true, start);
                }
                let value = f64::from_bits(u64::from_be_bytes(
                    self.take(8)?
                        .try_into()
                        .map_err(|_| parse_error("truncated AMF3 date", start))?,
                ));
                let id = self.node(
                    Value::Float {
                        v: value,
                        width: saveprism_core::FloatWidth::F64,
                    },
                    "amf3-date",
                    start,
                    self.pos,
                    false,
                )?;
                self.amf3_objects.push(id);
                Ok(id)
            }
            9 => self.amf3_array(start, depth),
            10 => self.amf3_object(start, depth),
            12 => self.amf3_binary(start, "amf3-byte-array"),
            13..=16 => self.amf3_vector(start, depth, marker),
            17 => self.amf3_dictionary(start, depth),
            other => Err(Error::new(
                SOL_UNSUPPORTED,
                format!("unsupported AMF3 marker 0x{other:02x}"),
            )
            .with_offset(len_u64(start))),
        }
    }
    fn amf3_binary(&mut self, start: usize, kind: &str) -> Result<NodeId> {
        let header = self.u29()?;
        if header & 1 == 0 {
            return self.reference((header >> 1) as usize, true, start);
        }
        let len = (header >> 1) as usize;
        self.count(len, 1)?;
        let bytes = self.take(len)?.to_vec();
        let id = self.node(Value::Bytes(bytes), kind, start, self.pos, false)?;
        self.amf3_objects.push(id);
        Ok(id)
    }
    fn amf3_array(&mut self, start: usize, depth: usize) -> Result<NodeId> {
        let header = self.u29()?;
        if header & 1 == 0 {
            return self.reference((header >> 1) as usize, true, start);
        }
        let dense = (header >> 1) as usize;
        self.count(dense, 1)?;
        let id = self.doc.reserve()?;
        self.amf3_objects.push(id);
        let mut assoc = Vec::new();
        loop {
            let (name, _) = self.amf3_string()?;
            if name.is_empty() {
                break;
            }
            assoc.push((name, self.amf3_value(depth + 1)?));
        }
        let mut dense_values = Vec::with_capacity(dense.min(4096));
        for _ in 0..dense {
            dense_values.push(self.amf3_value(depth + 1)?);
        }
        let mut node = Node::new(Value::Array(dense_values));
        node.type_hint = Some(TypeHint::new(format!("{PREFIX}amf3-array")));
        node.span = Some(saveprism_core::ByteSpan {
            start: len_u64(start),
            end: len_u64(self.pos),
        });
        node.flags.insert(NodeFlags::READ_ONLY);
        if !assoc.is_empty() {
            let object = self.doc.push(Node::new(Value::Object(assoc)))?;
            if let Value::Array(ref mut children) = node.value {
                children.insert(0, object);
            }
        }
        self.doc.finish(id, node)?;
        Ok(id)
    }
    fn amf3_object(&mut self, start: usize, depth: usize) -> Result<NodeId> {
        let header = self.u29()?;
        if header & 1 == 0 {
            return self.reference((header >> 1) as usize, true, start);
        }
        let trait_ref = header & 2 == 0;
        let tr = if trait_ref {
            self.traits
                .get((header >> 2) as usize)
                .cloned()
                .ok_or_else(|| parse_error("AMF3 trait reference is out of range", start))?
        } else {
            let dynamic = header & 8 != 0;
            let count = (header >> 4) as usize;
            self.count(count, 1)?;
            let class = self.amf3_string()?.0;
            let mut sealed = Vec::with_capacity(count.min(4096));
            for _ in 0..count {
                sealed.push(self.amf3_string()?.0);
            }
            let tr = Trait {
                class,
                sealed,
                dynamic,
            };
            if header & 4 == 0 {
                self.traits.push(tr.clone());
            }
            tr
        };
        if header & 4 != 0 {
            return Err(Error::new(
                SOL_UNSUPPORTED,
                format!(
                    "AMF3 externalizable class '{}' cannot be read without its class definition",
                    tr.class
                ),
            )
            .with_offset(len_u64(start)));
        }
        let id = self.doc.reserve()?;
        self.amf3_objects.push(id);
        let mut entries = Vec::new();
        for name in &tr.sealed {
            entries.push((name.clone(), self.amf3_value(depth + 1)?));
        }
        if tr.dynamic {
            loop {
                let name = self.amf3_string()?.0;
                if name.is_empty() {
                    break;
                }
                entries.push((name, self.amf3_value(depth + 1)?));
            }
        }
        let mut node = Node::new(Value::Object(entries));
        node.type_hint = Some(TypeHint::new(format!("{PREFIX}amf3-object:{}", tr.class)));
        node.span = Some(saveprism_core::ByteSpan {
            start: len_u64(start),
            end: len_u64(self.pos),
        });
        node.flags.insert(NodeFlags::READ_ONLY);
        self.doc.finish(id, node)?;
        Ok(id)
    }
    fn amf3_vector(&mut self, start: usize, depth: usize, marker: u8) -> Result<NodeId> {
        let header = self.u29()?;
        if header & 1 == 0 {
            return self.reference((header >> 1) as usize, true, start);
        }
        let count = (header >> 1) as usize;
        self.count(count, 1)?;
        let fixed = self.byte()? != 0;
        let id = self.doc.reserve()?;
        self.amf3_objects.push(id);
        let mut values = Vec::with_capacity(count.min(4096));
        for _ in 0..count {
            let item_start = self.pos;
            if marker == 16 {
                let _type_name = self.amf3_string()?;
                values.push(self.amf3_value(depth + 1)?);
                continue;
            }
            let value = match marker {
                13 => Value::Int {
                    v: i128::from(self.u32()? as i32),
                    width: IntWidth::I32,
                },
                14 => Value::Int {
                    v: i128::from(self.u32()?),
                    width: IntWidth::U32,
                },
                _ => {
                    let b = self
                        .take(8)?
                        .try_into()
                        .map_err(|_| parse_error("truncated AMF3 vector value", item_start))?;
                    Value::Float {
                        v: f64::from_bits(u64::from_be_bytes(b)),
                        width: saveprism_core::FloatWidth::F64,
                    }
                }
            };
            let _ = fixed;
            let kind = match marker {
                13 => "amf3-vector-i32",
                14 => "amf3-vector-u32",
                _ => "amf3-vector-double",
            };
            values.push(self.node(value, kind, item_start, self.pos, true)?);
        }
        let mut n = Node::new(Value::Array(values));
        n.type_hint = Some(TypeHint::new(format!("{PREFIX}amf3-vector-{marker}")));
        n.span = Some(saveprism_core::ByteSpan {
            start: len_u64(start),
            end: len_u64(self.pos),
        });
        n.flags.insert(NodeFlags::READ_ONLY);
        self.doc.finish(id, n)?;
        Ok(id)
    }
    fn amf3_dictionary(&mut self, start: usize, depth: usize) -> Result<NodeId> {
        let header = self.u29()?;
        if header & 1 == 0 {
            return self.reference((header >> 1) as usize, true, start);
        }
        let count = (header >> 1) as usize;
        self.count(count, 2)?;
        let _weak = self.byte()?;
        let id = self.doc.reserve()?;
        self.amf3_objects.push(id);
        let mut entries = Vec::with_capacity(count.min(4096));
        for _ in 0..count {
            let key = self.amf3_value(depth + 1)?;
            let value = self.amf3_value(depth + 1)?;
            entries.push((key, value));
        }
        let mut n = Node::new(Value::Map(entries));
        n.type_hint = Some(TypeHint::new(format!("{PREFIX}amf3-dictionary")));
        n.span = Some(saveprism_core::ByteSpan {
            start: len_u64(start),
            end: len_u64(self.pos),
        });
        n.flags.insert(NodeFlags::READ_ONLY);
        self.doc.finish(id, n)?;
        Ok(id)
    }
}

fn be_u16(input: &[u8], at: usize) -> Result<u16> {
    let s = input
        .get(at..at.saturating_add(2))
        .ok_or_else(|| parse_error("truncated LSO header", at))?;
    let bytes = s
        .try_into()
        .map_err(|_| parse_error("truncated LSO header", at))?;
    Ok(u16::from_be_bytes(bytes))
}
fn be_u32(input: &[u8], at: usize) -> Result<u32> {
    let s = input
        .get(at..at.saturating_add(4))
        .ok_or_else(|| parse_error("truncated LSO header", at))?;
    let bytes = s
        .try_into()
        .map_err(|_| parse_error("truncated LSO header", at))?;
    Ok(u32::from_be_bytes(bytes))
}
