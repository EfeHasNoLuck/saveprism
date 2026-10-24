//! LCF reader: bytes to the data tree (see `mod.rs` for the tree).

use saveprism_core::{
    DecodeCtx, Error, FloatWidth, IntWidth, Node, NodeFlags, NodeId, ParamValue, Params, Result,
    SaveDoc, TypeHint, Value,
};

use super::types::{self, Field, TypeDef, key_of, type_def};
use super::{
    ID_KEY, Kind, LCF_PARSE, LCF_UNSUPPORTED, P_END_MARK, P_RAW_BER, P_TRAILING, RAW_PREFIX,
    is_canonical, read_ber,
};

/// One chunk: its id, the raw bytes of its id and length, and its data.
struct Chunk<'a> {
    id: u32,
    id_raw: &'a [u8],
    len_raw: &'a [u8],
    data: &'a [u8],
}

/// The chunks of one struct and the raw bytes of its end mark.
struct Chunks<'a> {
    chunks: Vec<Chunk<'a>>,
    end_raw: &'a [u8],
}

/// One element of an array of structs.
struct Element<'a> {
    id: u32,
    id_raw: &'a [u8],
    body: Chunks<'a>,
}

/// A position in a byte slice.
struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn rest(&self) -> &'a [u8] {
        self.data.get(self.pos..).unwrap_or_default()
    }

    fn at_end(&self) -> bool {
        self.rest().is_empty()
    }

    /// One BER number: the value and its raw bytes.
    fn ber(&mut self) -> Option<(u32, &'a [u8])> {
        let rest = self.rest();
        let (value, used) = read_ber(rest)?;
        self.pos = self.pos.checked_add(used)?;
        Some((value, rest.get(..used)?))
    }

    fn take(&mut self, len: u32) -> Option<&'a [u8]> {
        let len = usize::try_from(len).ok()?;
        let out = self.rest().get(..len)?;
        self.pos = self.pos.checked_add(len)?;
        Some(out)
    }

    /// One chunk header and its data; `None` when the bytes do not hold one. An id of 0 is
    /// returned with empty data and no length.
    fn chunk(&mut self) -> Option<Chunk<'a>> {
        let (id, id_raw) = self.ber()?;
        if id == 0 {
            return Some(Chunk {
                id,
                id_raw,
                len_raw: &[],
                data: &[],
            });
        }
        let (len, len_raw) = self.ber()?;
        let data = self.take(len)?;
        Some(Chunk {
            id,
            id_raw,
            len_raw,
            data,
        })
    }

    /// Chunks up to and including the end mark (id 0).
    fn chunks(&mut self) -> Option<Chunks<'a>> {
        let mut chunks = Vec::new();
        loop {
            let chunk = self.chunk()?;
            if chunk.id == 0 {
                return Some(Chunks {
                    chunks,
                    end_raw: chunk.id_raw,
                });
            }
            chunks.push(chunk);
        }
    }
}

/// `data` as exactly one struct: chunks, then the end mark as its last byte.
fn split_struct(data: &[u8]) -> Option<Chunks<'_>> {
    let mut cursor = Cursor::new(data);
    let body = cursor.chunks()?;
    cursor.at_end().then_some(body)
}

/// `data` as exactly one array of structs: the count, then each element.
fn split_array(data: &[u8]) -> Option<(&[u8], Vec<Element<'_>>)> {
    let mut cursor = Cursor::new(data);
    let (count, count_raw) = cursor.ber()?;
    let mut elements = Vec::new();
    // Every element takes at least two bytes, so a wrong count stops at the end of the data.
    for _ in 0..count {
        let (id, id_raw) = cursor.ber()?;
        let body = cursor.chunks()?;
        elements.push(Element { id, id_raw, body });
    }
    cursor.at_end().then_some((count_raw, elements))
}

/// Exactly one BER number.
fn single_ber(data: &[u8]) -> Option<u32> {
    let (value, used) = read_ber(data)?;
    (used == data.len()).then_some(value)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// True for text the tree shows as an editable string.
pub(crate) fn is_printable_ascii(bytes: &[u8]) -> bool {
    bytes.iter().all(|byte| (0x20..=0x7E).contains(byte))
}

fn parse_error(message: impl Into<String>) -> Error {
    Error::new(LCF_PARSE, message)
}

struct Reader {
    doc: SaveDoc,
    /// Number of the next structural BER number (header length, chunk ids and lengths, array
    /// counts, end marks) in reader order.
    ordinal: i64,
    raws: Params,
}

impl Reader {
    fn push(&mut self, value: Value, kind: Kind<'_>, read_only: bool) -> Result<NodeId> {
        let mut node = Node::new(value);
        node.type_hint = Some(TypeHint::new(kind.hint()));
        if read_only {
            node.flags.insert(NodeFlags::READ_ONLY);
        }
        self.doc.push(node)
    }

    /// Counts one structural BER number; records its bytes when they are not canonical.
    fn structural(&mut self, raw: &[u8]) {
        if !is_canonical(raw) {
            self.raws.set(self.ordinal.to_string(), raw.to_vec());
        }
        self.ordinal = self.ordinal.saturating_add(1);
    }

    /// A BER integer node; non-canonical bytes become the `raw:` lexeme.
    fn int(&mut self, value: u32, raw: &[u8], kind: Kind<'_>, read_only: bool) -> Result<NodeId> {
        let mut node = Node::new(Value::Int {
            v: i128::from(value as i32),
            width: IntWidth::I32,
        });
        node.type_hint = Some(TypeHint::new(kind.hint()));
        if !is_canonical(raw) {
            node.lexeme = Some(format!("{RAW_PREFIX}{}", hex(raw)).into());
        }
        if read_only {
            node.flags.insert(NodeFlags::READ_ONLY);
        }
        self.doc.push(node)
    }

    fn bytes(&mut self, data: &[u8], kind: Kind<'_>) -> Result<NodeId> {
        self.push(Value::Bytes(data.to_vec()), kind, true)
    }

    /// The node of one chunk's data: the field's kind when the data parses exactly as it, else
    /// read-only bytes.
    fn value(&mut self, field: Option<&Field>, data: &[u8]) -> Result<NodeId> {
        let Some(field) = field else {
            return self.bytes(data, Kind::Bytes);
        };
        let kind = field.kind;
        match kind {
            Kind::Int | Kind::Count => match single_ber(data) {
                Some(value) => self.int(value, data, kind, kind == Kind::Count),
                None => self.bytes(data, Kind::Bytes),
            },
            Kind::Bool => match (data, single_ber(data)) {
                ([byte @ (0 | 1)], _) => self.push(Value::Bool(*byte == 1), kind, false),
                (_, Some(value)) => self.int(value, data, kind, false),
                _ => self.bytes(data, Kind::Bytes),
            },
            Kind::Int8 | Kind::UInt8 => match data {
                [byte] => {
                    let (v, width) = if kind == Kind::Int8 {
                        (i128::from(*byte as i8), IntWidth::I8)
                    } else {
                        (i128::from(*byte), IntWidth::U8)
                    };
                    self.push(Value::Int { v, width }, kind, false)
                }
                _ => self.bytes(data, Kind::Bytes),
            },
            Kind::Double => match <[u8; 8]>::try_from(data) {
                Ok(bytes) => self.push(
                    Value::Float {
                        v: f64::from_le_bytes(bytes),
                        width: FloatWidth::F64,
                    },
                    kind,
                    false,
                ),
                Err(_) => self.bytes(data, Kind::Bytes),
            },
            Kind::Text => match std::str::from_utf8(data) {
                Ok(text) if is_printable_ascii(data) => {
                    self.push(Value::Str(text.to_string()), kind, false)
                }
                _ => self.bytes(data, kind),
            },
            Kind::Bytes => self.bytes(data, kind),
            Kind::Int16s | Kind::Int32s | Kind::UInt8s | Kind::Bools => self.vector(kind, data),
            Kind::Struct(name) => match (type_def(name), split_struct(data)) {
                (Some(ty), Some(body)) => self.structure(ty, None, body),
                _ => self.bytes(data, Kind::Bytes),
            },
            Kind::Array(name) => match (type_def(name), split_array(data)) {
                (Some(ty), Some((count_raw, elements))) => self.array(ty, count_raw, elements),
                _ => self.bytes(data, Kind::Bytes),
            },
        }
    }

    /// A fixed-size little-endian vector.
    fn vector(&mut self, kind: Kind<'_>, data: &[u8]) -> Result<NodeId> {
        let size = match kind {
            Kind::Int16s => 2,
            Kind::Int32s => 4,
            _ => 1,
        };
        if !data.len().is_multiple_of(size) {
            return self.bytes(data, Kind::Bytes);
        }
        let mut items = Vec::new();
        for element in data.chunks_exact(size) {
            let value = match (kind, element) {
                (Kind::Int16s, [a, b]) => Value::Int {
                    v: i128::from(i16::from_le_bytes([*a, *b])),
                    width: IntWidth::I16,
                },
                (Kind::Int32s, [a, b, c, d]) => Value::Int {
                    v: i128::from(i32::from_le_bytes([*a, *b, *c, *d])),
                    width: IntWidth::I32,
                },
                (Kind::Bools, [byte @ (0 | 1)]) => Value::Bool(*byte == 1),
                (_, [byte]) => Value::Int {
                    v: i128::from(*byte),
                    width: IntWidth::U8,
                },
                _ => return Err(parse_error("vector element of the wrong size")),
            };
            items.push(self.doc.push(Node::new(value))?);
        }
        self.push(Value::Array(items), kind, true)
    }

    /// A struct; `first` is the `(id)` entry of an array element.
    fn structure(
        &mut self,
        ty: &'static TypeDef,
        first: Option<NodeId>,
        body: Chunks<'_>,
    ) -> Result<NodeId> {
        let mut entries = Vec::new();
        if let Some(id) = first {
            entries.push((ID_KEY.to_string(), id));
        }
        for chunk in body.chunks {
            self.structural(chunk.id_raw);
            self.structural(chunk.len_raw);
            let node = self.value(ty.by_id(chunk.id), chunk.data)?;
            entries.push((key_of(ty, chunk.id), node));
        }
        self.structural(body.end_raw);
        self.push(Value::Object(entries), Kind::Struct(ty.name), true)
    }

    fn array(
        &mut self,
        ty: &'static TypeDef,
        count_raw: &[u8],
        elements: Vec<Element<'_>>,
    ) -> Result<NodeId> {
        self.structural(count_raw);
        let mut items = Vec::new();
        for element in elements {
            let id = self.int(element.id, element.id_raw, Kind::Int, true)?;
            items.push(self.structure(ty, Some(id), element.body)?);
        }
        self.push(Value::Array(items), Kind::Array(ty.name), true)
    }
}

/// Decodes a whole LCF file.
pub(super) fn decode(input: &[u8], ctx: &DecodeCtx<'_>) -> Result<(SaveDoc, Params)> {
    let mut reader = Reader {
        doc: ctx.new_doc(),
        ordinal: 0,
        raws: Params::new(),
    };
    let mut cursor = Cursor::new(input);
    let (len, len_raw) = cursor
        .ber()
        .ok_or_else(|| parse_error("the header length is truncated or not a valid number"))?;
    reader.structural(len_raw);
    let text = cursor
        .take(len)
        .ok_or_else(|| parse_error("the header is longer than the file"))?;
    let root_type = if text == super::SAVE_HEADER {
        types::SAVE
    } else if text == super::DATABASE_HEADER {
        types::DATABASE
    } else {
        let shown = String::from_utf8_lossy(text.get(..32).unwrap_or(text)).into_owned();
        return Err(Error::new(
            LCF_UNSUPPORTED,
            format!(
                "'{shown}' is not an RPG Maker 2000/2003 save or database (LcfSaveData, LcfDataBase)"
            ),
        ));
    };
    let ty = type_def(root_type).ok_or_else(|| parse_error("missing root type"))?;

    let mut params = Params::new();
    let mut entries = Vec::new();
    while !cursor.at_end() {
        let start = cursor.pos;
        let chunk = cursor.chunk().ok_or_else(|| {
            parse_error(format!(
                "the chunk at byte {start} is truncated or has a bad id or length"
            ))
        })?;
        if chunk.id == 0 {
            params.set(P_END_MARK, chunk.id_raw.to_vec());
            params.set(P_TRAILING, cursor.rest().to_vec());
            break;
        }
        reader.structural(chunk.id_raw);
        reader.structural(chunk.len_raw);
        let node = reader.value(ty.by_id(chunk.id), chunk.data)?;
        entries.push((key_of(ty, chunk.id), node));
    }
    let root = reader.push(Value::Object(entries), Kind::Struct(ty.name), true)?;
    reader.doc.set_root(root);
    if !reader.raws.is_empty() {
        params.set(P_RAW_BER, ParamValue::Table(reader.raws));
    }
    Ok((reader.doc, params))
}
