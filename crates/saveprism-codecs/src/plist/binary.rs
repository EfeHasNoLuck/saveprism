//! Binary property lists (`bplist00`), parsed as read-only data.
//!
//! Layout from the published Apple binary property list format: an 8-byte `bplist00` header, an
//! object table and a 32-byte trailer naming the offset-table position, integer widths and the
//! top object. Objects are markers with a type nibble and a size nibble; sizes of 15 use the next
//! integer object. Every offset and length is bounds-checked; object references are cached so a
//! shared or cyclic object graph never recurses forever.

use std::collections::HashMap;

use saveprism_core::{
    DecodeCtx, Error, FloatWidth, IntWidth, NodeId, Result, SaveDoc, Value, codes,
};

use super::{PLIST_PARSE, PREFIX};
use crate::text_common::read_only;

const HEADER: &[u8; 8] = b"bplist00";
const TRAILER: usize = 32;
const MAX_DEPTH: usize = 256;

/// Parses a binary plist into a fully read-only tree.
pub(crate) fn parse(input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<SaveDoc> {
    if !input.starts_with(HEADER) {
        return Err(Error::new(PLIST_PARSE, "not a bplist00 file"));
    }
    if input.len() < HEADER.len() + TRAILER {
        return Err(Error::new(PLIST_PARSE, "truncated bplist trailer"));
    }
    let trailer_at = input.len() - TRAILER;
    let trailer = input
        .get(trailer_at..)
        .ok_or_else(|| Error::new(PLIST_PARSE, "truncated bplist trailer"))?;
    let offset_size = usize::from(byte(trailer, 6)?);
    let ref_size = usize::from(byte(trailer, 7)?);
    let object_count = be_u64(trailer, 8)?;
    let top = be_u64(trailer, 16)?;
    let table_at = be_u64(trailer, 24)?;
    if !(1..=8).contains(&offset_size) || !(1..=8).contains(&ref_size) {
        return Err(Error::new(PLIST_PARSE, "invalid bplist integer widths"));
    }
    let object_count = usize::try_from(object_count)
        .map_err(|_| Error::new(PLIST_PARSE, "object count does not fit this platform"))?;
    if object_count == 0 || object_count > input.len() {
        return Err(Error::new(PLIST_PARSE, "implausible bplist object count"));
    }
    let table_at = usize::try_from(table_at)
        .map_err(|_| Error::new(PLIST_PARSE, "offset table position does not fit"))?;
    let table_len = object_count
        .checked_mul(offset_size)
        .ok_or_else(|| Error::new(PLIST_PARSE, "offset table size overflow"))?;
    if table_at
        .checked_add(table_len)
        .is_none_or(|end| end > trailer_at)
    {
        return Err(Error::new(PLIST_PARSE, "offset table outside the file"));
    }
    let mut offsets = Vec::with_capacity(object_count);
    for index in 0..object_count {
        let at = table_at
            .checked_add(index.saturating_mul(offset_size))
            .ok_or_else(|| Error::new(PLIST_PARSE, "offset table overflow"))?;
        let raw = input
            .get(at..at.saturating_add(offset_size))
            .ok_or_else(|| Error::new(PLIST_PARSE, "truncated offset table"))?;
        let offset = be_uint(raw)?;
        let offset = usize::try_from(offset)
            .map_err(|_| Error::new(PLIST_PARSE, "object offset does not fit"))?;
        if offset < HEADER.len() || offset >= table_at.min(trailer_at) {
            return Err(Error::new(
                PLIST_PARSE,
                "bplist object offset outside the data",
            ));
        }
        offsets.push(offset);
    }
    let top = usize::try_from(top)
        .map_err(|_| Error::new(PLIST_PARSE, "top object index does not fit"))?;
    if top >= offsets.len() {
        return Err(Error::new(PLIST_PARSE, "top object index is out of range"));
    }
    let mut doc = ctx.new_doc();
    let mut reader = Reader {
        input,
        offsets,
        ref_size,
        limit: table_at.min(trailer_at),
        cache: HashMap::new(),
        active: Vec::new(),
        max_depth: ctx.limits().max_depth.min(MAX_DEPTH),
    };
    let root_id = reader.object(&mut doc, top, 0)?;
    doc.set_root(root_id);
    Ok(doc)
}

struct Reader<'a> {
    input: &'a [u8],
    offsets: Vec<usize>,
    ref_size: usize,
    limit: usize,
    cache: HashMap<usize, NodeId>,
    active: Vec<usize>,
    max_depth: usize,
}

impl Reader<'_> {
    fn object(&mut self, doc: &mut SaveDoc, index: usize, depth: usize) -> Result<NodeId> {
        if depth > self.max_depth {
            return Err(Error::new(
                codes::CORE_LIMIT_DEPTH,
                format!("bplist nesting limit of {} exceeded", self.max_depth),
            ));
        }
        let offset = *self
            .offsets
            .get(index)
            .ok_or_else(|| Error::new(PLIST_PARSE, "bplist object reference out of range"))?;
        if let Some(id) = self.cache.get(&offset) {
            return Ok(*id);
        }
        if self.active.contains(&offset) {
            return doc.push(read_only(
                Value::Str("<cycle>".to_owned()),
                &format!("{PREFIX}cycle"),
            ));
        }
        self.active.push(offset);
        let result = self.object_inner(doc, offset, depth);
        self.active.pop();
        let id = result?;
        self.cache.insert(offset, id);
        Ok(id)
    }

    fn object_inner(&mut self, doc: &mut SaveDoc, offset: usize, depth: usize) -> Result<NodeId> {
        let mut at = offset;
        let marker = self.u8(&mut at)?;
        let kind = marker >> 4;
        let info = marker & 0x0f;
        match kind {
            0x0 => match info {
                0x0 | 0xf => self.push(doc, Value::Null, "null"),
                0x8 => self.push(doc, Value::Bool(false), "bool"),
                0x9 => self.push(doc, Value::Bool(true), "bool"),
                _ => Err(Error::new(PLIST_PARSE, "unknown bplist simple object")),
            },
            0x1 => {
                let value = self.integer(&mut at, info)?;
                self.push(doc, value, "int")
            }
            0x2 => {
                let value = match info {
                    2 => f64::from(self.f32(&mut at)?),
                    3 => self.f64(&mut at)?,
                    _ => {
                        return Err(Error::new(PLIST_PARSE, "unknown bplist real width"));
                    }
                };
                self.push(
                    doc,
                    Value::Float {
                        v: value,
                        width: if info == 2 {
                            FloatWidth::F32
                        } else {
                            FloatWidth::F64
                        },
                    },
                    "real",
                )
            }
            0x3 => {
                if info != 3 {
                    return Err(Error::new(PLIST_PARSE, "invalid bplist date"));
                }
                let seconds = self.f64(&mut at)?;
                self.push(
                    doc,
                    Value::Str(format!("{seconds} seconds since 2001-01-01")),
                    "date",
                )
            }
            0x4 => {
                let count = self.count(&mut at, info)?;
                let bytes = self.take(&mut at, count)?.to_vec();
                self.push(doc, Value::Bytes(bytes), "data")
            }
            0x5 => {
                let count = self.count(&mut at, info)?;
                let bytes = self.take(&mut at, count)?;
                let text = String::from_utf8_lossy(bytes).into_owned();
                self.push(doc, Value::Str(text), "string")
            }
            0x6 => {
                let count = self.count(&mut at, info)?;
                let bytes = self.take(&mut at, count.saturating_mul(2))?;
                let mut units = Vec::with_capacity(count);
                for pair in bytes.as_chunks::<2>().0 {
                    units.push(u16::from_be_bytes(*pair));
                }
                let text = String::from_utf16_lossy(&units);
                self.push(doc, Value::Str(text), "string")
            }
            0x8 => {
                let width = info.saturating_add(1);
                let bytes = self.take(&mut at, usize::from(width))?;
                let value = be_uint(bytes)?;
                self.push(
                    doc,
                    Value::Int {
                        v: i128::from(value),
                        width: IntWidth::U64,
                    },
                    "uid",
                )
            }
            0xa..=0xc => {
                let count = self.count(&mut at, info)?;
                self.ensure_refs(count)?;
                let mut items = Vec::with_capacity(count);
                for _ in 0..count {
                    let reference = self.reference(&mut at)?;
                    items.push(self.object(doc, reference, depth + 1)?);
                }
                self.push(doc, Value::Array(items), "array")
            }
            0xd => {
                let count = self.count(&mut at, info)?;
                self.ensure_refs(count.saturating_mul(2))?;
                let mut keys = Vec::with_capacity(count);
                for _ in 0..count {
                    let reference = self.reference(&mut at)?;
                    keys.push(self.object(doc, reference, depth + 1)?);
                }
                let mut entries = Vec::with_capacity(count);
                for key in keys {
                    let reference = self.reference(&mut at)?;
                    let value = self.object(doc, reference, depth + 1)?;
                    let text = key_text(doc, key);
                    entries.push((text, value));
                }
                self.push(doc, Value::Object(entries), "dict")
            }
            _ => Err(Error::new(PLIST_PARSE, "unknown bplist object type")),
        }
    }

    fn push(&mut self, doc: &mut SaveDoc, value: Value, hint: &str) -> Result<NodeId> {
        doc.push(read_only(value, &format!("{PREFIX}{hint}")))
    }

    fn ensure_refs(&self, count: usize) -> Result<()> {
        if count > self.limit / self.ref_size.max(1) {
            return Err(Error::new(
                PLIST_PARSE,
                "implausible bplist reference count",
            ));
        }
        Ok(())
    }

    fn count(&mut self, at: &mut usize, info: u8) -> Result<usize> {
        if info != 0x0f {
            return Ok(usize::from(info));
        }
        let marker = self.u8(at)?;
        if marker >> 4 != 0x1 {
            return Err(Error::new(PLIST_PARSE, "bplist length is not an integer"));
        }
        let value = self.integer(at, marker & 0x0f)?;
        match value {
            Value::Int { v, .. } => usize::try_from(v)
                .map_err(|_| Error::new(PLIST_PARSE, "bplist length is negative or too large")),
            _ => Err(Error::new(PLIST_PARSE, "bplist length is not an integer")),
        }
    }

    fn integer(&mut self, at: &mut usize, info: u8) -> Result<Value> {
        let width = 1usize
            .checked_shl(u32::from(info))
            .filter(|width| *width <= 16)
            .ok_or_else(|| Error::new(PLIST_PARSE, "invalid bplist integer width"))?;
        let bytes = self.take(at, width)?;
        if width == 16 {
            return Ok(Value::Bytes(bytes.to_vec()));
        }
        let raw = be_uint(bytes)?;
        let value = i128::from(raw);
        Ok(Value::Int {
            v: value,
            width: IntWidth::Unbounded,
        })
    }

    fn reference(&mut self, at: &mut usize) -> Result<usize> {
        let bytes = self.take(at, self.ref_size)?;
        let value = be_uint(bytes)?;
        usize::try_from(value).map_err(|_| Error::new(PLIST_PARSE, "object reference does not fit"))
    }

    fn take(&mut self, at: &mut usize, count: usize) -> Result<&[u8]> {
        let end = at
            .checked_add(count)
            .filter(|end| *end <= self.limit)
            .ok_or_else(|| Error::new(PLIST_PARSE, "bplist object runs past the data"))?;
        let bytes = self
            .input
            .get(*at..end)
            .ok_or_else(|| Error::new(PLIST_PARSE, "truncated bplist object"))?;
        *at = end;
        Ok(bytes)
    }

    fn u8(&mut self, at: &mut usize) -> Result<u8> {
        let bytes = self.take(at, 1)?;
        bytes
            .first()
            .copied()
            .ok_or_else(|| Error::new(PLIST_PARSE, "truncated bplist marker"))
    }

    fn f32(&mut self, at: &mut usize) -> Result<f32> {
        let bytes = self.take(at, 4)?;
        let array: [u8; 4] = bytes
            .try_into()
            .map_err(|_| Error::new(PLIST_PARSE, "truncated bplist real"))?;
        Ok(f32::from_be_bytes(array))
    }

    fn f64(&mut self, at: &mut usize) -> Result<f64> {
        let bytes = self.take(at, 8)?;
        let array: [u8; 8] = bytes
            .try_into()
            .map_err(|_| Error::new(PLIST_PARSE, "truncated bplist real"))?;
        Ok(f64::from_be_bytes(array))
    }
}

fn key_text(doc: &SaveDoc, id: NodeId) -> String {
    match doc.get(id).map(|node| &node.value) {
        Some(Value::Str(text)) => text.clone(),
        Some(Value::Int { v, .. }) => v.to_string(),
        Some(Value::Bool(value)) => value.to_string(),
        _ => "<key>".to_owned(),
    }
}

fn byte(bytes: &[u8], at: usize) -> Result<u8> {
    bytes
        .get(at)
        .copied()
        .ok_or_else(|| Error::new(PLIST_PARSE, "truncated bplist trailer"))
}

fn be_u64(bytes: &[u8], at: usize) -> Result<u64> {
    let slice = bytes
        .get(at..at.saturating_add(8))
        .ok_or_else(|| Error::new(PLIST_PARSE, "truncated bplist trailer"))?;
    let array: [u8; 8] = slice
        .try_into()
        .map_err(|_| Error::new(PLIST_PARSE, "truncated bplist trailer"))?;
    Ok(u64::from_be_bytes(array))
}

fn be_uint(bytes: &[u8]) -> Result<u64> {
    if bytes.is_empty() || bytes.len() > 8 {
        return Err(Error::new(PLIST_PARSE, "invalid bplist integer width"));
    }
    let mut value = 0u64;
    for byte in bytes {
        value = value
            .checked_mul(256)
            .and_then(|value| value.checked_add(u64::from(*byte)))
            .ok_or_else(|| Error::new(PLIST_PARSE, "bplist integer overflow"))?;
    }
    Ok(value)
}
