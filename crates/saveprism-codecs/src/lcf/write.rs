//! LCF writer: the data tree back to bytes (see `mod.rs` for what is recorded for exact writing).

use saveprism_core::{Error, Node, NodeId, ParamValue, Params, Result, SaveDoc, Value};

use super::types::{self, TypeDef, type_def};
use super::{
    DATABASE_HEADER, ID_KEY, Kind, LCF_WRITE, P_END_MARK, P_RAW_BER, P_TEXT_ENCODING, P_TRAILING,
    RAW_PREFIX, SAVE_HEADER, parse_hint, push_ber, read_ber,
};
use crate::text_encoding::TextEncoding;

/// Deepest struct nesting written (the field tables nest three levels).
const MAX_NESTING: usize = 16;

fn write_error(message: impl Into<String>) -> Error {
    Error::new(LCF_WRITE, message)
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(text.get(index..index + 2)?, 16).ok())
        .collect()
}

/// `raw` when it is exactly one BER number of value `value`.
fn raw_if_equal(raw: &[u8], value: u32) -> Option<&[u8]> {
    let (read, used) = read_ber(raw)?;
    (read == value && used == raw.len()).then_some(raw)
}

fn len_u32(len: usize, what: &str) -> Result<u32> {
    u32::try_from(len).map_err(|_| write_error(format!("{what} is too long ({len} bytes)")))
}

struct Writer<'a> {
    doc: &'a SaveDoc,
    raws: Option<&'a Params>,
    /// Encoding of non-ASCII string edits.
    text_encoding: TextEncoding,
    /// Number of the next structural BER number, in the order the reader counted them.
    ordinal: i64,
}

impl<'a> Writer<'a> {
    fn node(&self, id: NodeId) -> Result<&'a Node> {
        self.doc
            .get(id)
            .ok_or_else(|| write_error(format!("node {} does not exist", id.index())))
    }

    fn take_ordinal(&mut self) -> i64 {
        let ordinal = self.ordinal;
        self.ordinal = self.ordinal.saturating_add(1);
        ordinal
    }

    /// Writes the structural number `value` counted as `ordinal`: the recorded bytes while they
    /// still hold `value`, else the canonical form.
    fn structural(&self, out: &mut Vec<u8>, ordinal: i64, value: u32) {
        let recorded = self
            .raws
            .and_then(|raws| raws.get(&ordinal.to_string()))
            .and_then(|raw| match raw {
                ParamValue::Bytes(bytes) => raw_if_equal(bytes, value),
                _ => None,
            });
        match recorded {
            Some(raw) => out.extend_from_slice(raw),
            None => push_ber(out, value),
        }
    }

    /// A BER integer node: its `raw:` bytes while they hold the value, else the canonical form.
    fn int(&self, out: &mut Vec<u8>, node: &Node, key: &str) -> Result<()> {
        let Value::Int { v, .. } = node.value else {
            return Err(write_error(format!("'{key}' must be a whole number")));
        };
        let value = i32::try_from(v)
            .map_err(|_| write_error(format!("{v} at '{key}' does not fit a 32-bit number")))?
            as u32;
        let raw = node
            .lexeme
            .as_deref()
            .and_then(|lexeme| lexeme.strip_prefix(RAW_PREFIX))
            .and_then(unhex);
        match raw.as_deref().and_then(|raw| raw_if_equal(raw, value)) {
            Some(raw) => out.extend_from_slice(raw),
            None => push_ber(out, value),
        }
        Ok(())
    }

    /// The data of the chunk `key` held by `id`.
    fn body(&mut self, id: NodeId, key: &str, depth: usize) -> Result<Vec<u8>> {
        let node = self.node(id)?;
        let kind = node
            .type_hint
            .as_ref()
            .and_then(|hint| parse_hint(hint.as_str()))
            .ok_or_else(|| write_error(format!("'{key}' has no LCF type")))?;
        let mut out = Vec::new();
        match (kind, &node.value) {
            (Kind::Int | Kind::Count, _) | (Kind::Bool, Value::Int { .. }) => {
                self.int(&mut out, node, key)?;
            }
            (Kind::Bool, Value::Bool(value)) => out.push(u8::from(*value)),
            (Kind::Int8, Value::Int { v, .. }) => {
                let byte = i8::try_from(*v)
                    .map_err(|_| write_error(format!("{v} at '{key}' does not fit 8 bits")))?;
                out.push(byte as u8);
            }
            (Kind::UInt8, Value::Int { v, .. }) => {
                let byte = u8::try_from(*v)
                    .map_err(|_| write_error(format!("{v} at '{key}' does not fit 8 bits")))?;
                out.push(byte);
            }
            (Kind::Double, Value::Float { v, .. }) => out.extend_from_slice(&v.to_le_bytes()),
            (Kind::Text, Value::Str(text)) => {
                let encoded = self.text_encoding.encode(text).map_err(|error| {
                    write_error(format!("the text at '{key}' cannot be written: {error}"))
                })?;
                out.extend_from_slice(&encoded);
            }
            (Kind::Text | Kind::Bytes, Value::Bytes(bytes)) => out.extend_from_slice(bytes),
            (Kind::Int16s | Kind::Int32s | Kind::UInt8s | Kind::Bools, Value::Array(items)) => {
                for item in items {
                    self.element(&mut out, kind, *item, key)?;
                }
            }
            (Kind::Struct(name), Value::Object(entries)) => {
                let ty = self.type_of(name, depth)?;
                self.chunks(&mut out, ty, entries, depth + 1)?;
                let end = self.take_ordinal();
                self.structural(&mut out, end, 0);
            }
            (Kind::Array(name), Value::Array(items)) => {
                let ty = self.type_of(name, depth)?;
                let count = self.take_ordinal();
                self.structural(&mut out, count, len_u32(items.len(), key)?);
                for item in items {
                    let Value::Object(entries) = &self.node(*item)?.value else {
                        return Err(write_error(format!(
                            "an element of '{key}' is not a struct"
                        )));
                    };
                    let Some(((first, id), rest)) = entries.split_first() else {
                        return Err(write_error(format!("an element of '{key}' has no id")));
                    };
                    if first != ID_KEY {
                        return Err(write_error(format!("an element of '{key}' has no id")));
                    }
                    self.int(&mut out, self.node(*id)?, ID_KEY)?;
                    self.chunks(&mut out, ty, rest, depth + 1)?;
                    let end = self.take_ordinal();
                    self.structural(&mut out, end, 0);
                }
            }
            _ => {
                return Err(write_error(format!(
                    "the value at '{key}' does not match its LCF type"
                )));
            }
        }
        Ok(out)
    }

    fn type_of(&self, name: &str, depth: usize) -> Result<&'static TypeDef> {
        if depth > MAX_NESTING {
            return Err(write_error("the structs are nested too deeply"));
        }
        type_def(name).ok_or_else(|| write_error(format!("unknown LCF struct type '{name}'")))
    }

    /// One element of a fixed-size vector.
    fn element(&self, out: &mut Vec<u8>, kind: Kind<'_>, id: NodeId, key: &str) -> Result<()> {
        let value = &self.node(id)?.value;
        let range = |v: &i128| write_error(format!("{v} in '{key}' does not fit the vector"));
        match (kind, value) {
            (Kind::Int16s, Value::Int { v, .. }) => {
                out.extend_from_slice(&i16::try_from(*v).map_err(|_| range(v))?.to_le_bytes());
            }
            (Kind::Int32s, Value::Int { v, .. }) => {
                out.extend_from_slice(&i32::try_from(*v).map_err(|_| range(v))?.to_le_bytes());
            }
            (Kind::UInt8s | Kind::Bools, Value::Int { v, .. }) => {
                out.push(u8::try_from(*v).map_err(|_| range(v))?);
            }
            (Kind::Bools, Value::Bool(value)) => out.push(u8::from(*value)),
            _ => {
                return Err(write_error(format!(
                    "an element of '{key}' does not match the vector type"
                )));
            }
        }
        Ok(())
    }

    /// The chunks of a struct of type `ty`.
    fn chunks(
        &mut self,
        out: &mut Vec<u8>,
        ty: &TypeDef,
        entries: &[(String, NodeId)],
        depth: usize,
    ) -> Result<()> {
        for (key, child) in entries {
            let id = ty
                .id_of(key)
                .ok_or_else(|| write_error(format!("'{key}' is not a chunk of {}", ty.name)))?;
            let id_ordinal = self.take_ordinal();
            let len_ordinal = self.take_ordinal();
            let body = self.body(*child, key, depth)?;
            self.structural(out, id_ordinal, id);
            self.structural(out, len_ordinal, len_u32(body.len(), key)?);
            out.extend_from_slice(&body);
        }
        Ok(())
    }
}

/// Encodes a document read by this codec.
pub(super) fn encode(doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
    let root = doc
        .get(doc.root())
        .ok_or_else(|| write_error("the document has no root"))?;
    let name = match root
        .type_hint
        .as_ref()
        .and_then(|hint| parse_hint(hint.as_str()))
    {
        Some(Kind::Struct(name)) => name,
        _ => return Err(write_error("the root is not an LCF save or database")),
    };
    let header = match name {
        types::SAVE => SAVE_HEADER,
        types::DATABASE => DATABASE_HEADER,
        _ => return Err(write_error("the root is not an LCF save or database")),
    };
    let Value::Object(entries) = &root.value else {
        return Err(write_error("the root is not an LCF save or database"));
    };
    let ty = type_def(name).ok_or_else(|| write_error("missing root type"))?;
    let raws = match params.get(P_RAW_BER) {
        Some(ParamValue::Table(table)) => Some(table),
        None => None,
        Some(_) => return Err(write_error("the parameter 'raw_ber' must be a table")),
    };
    let text_encoding = params
        .opt_str(P_TEXT_ENCODING)?
        .and_then(TextEncoding::from_name)
        .unwrap_or(TextEncoding::Windows1252);
    let mut writer = Writer {
        doc,
        raws,
        text_encoding,
        ordinal: 0,
    };
    let mut out = Vec::new();
    let header_ordinal = writer.take_ordinal();
    writer.structural(
        &mut out,
        header_ordinal,
        len_u32(header.len(), "the header")?,
    );
    out.extend_from_slice(header);
    writer.chunks(&mut out, ty, entries, 0)?;
    if let Some(end) = params.opt_bytes(P_END_MARK)? {
        out.extend_from_slice(end);
        if let Some(trailing) = params.opt_bytes(P_TRAILING)? {
            out.extend_from_slice(trailing);
        }
    }
    Ok(out)
}
