//! QSP writer: the data tree back to bytes (see `mod.rs` for what is recorded for exact writing).

use saveprism_core::{Error, Node, NodeId, ParamValue, Params, Result, SaveDoc, Value};

use super::read::{VariantKind, variant_kind_from_hint};
use super::{Encoding, MAX_NESTING, P_ENCODING, P_VARIANT_TYPES, QSP_WRITE, SEPARATOR, SHIFT};
use crate::text_encoding::TextEncoding;

fn write_error(message: impl Into<String>) -> Error {
    Error::new(QSP_WRITE, message)
}

fn encoding(params: &Params) -> Result<Encoding> {
    let name = match params.get(P_ENCODING) {
        Some(ParamValue::Str(name)) => name.as_str(),
        Some(_) => return Err(write_error("the encoding parameter must be text")),
        None => return Err(write_error("the encoding parameter is missing")),
    };
    Encoding::from_name(name).ok_or_else(|| write_error(format!("unknown text encoding '{name}'")))
}

/// The output text, before it is turned into bytes.
enum Out {
    Wide(Vec<u16>),
    Byte(Vec<u8>, TextEncoding),
}

impl Out {
    fn new(encoding: Encoding) -> Self {
        match encoding {
            Encoding::Utf16Le => Out::Wide(Vec::new()),
            Encoding::Ansi(codec) => Out::Byte(Vec::new(), codec),
        }
    }

    fn separator(&mut self) {
        match self {
            Out::Wide(units) => {
                units.push(u16::from(SEPARATOR[0]));
                units.push(u16::from(SEPARATOR[1]));
            }
            Out::Byte(bytes, _) => bytes.extend_from_slice(&SEPARATOR),
        }
    }

    /// A raw ASCII line (the signature and the version): no character shift.
    fn raw_ascii(&mut self, text: &str) -> Result<()> {
        for ch in text.chars() {
            if !ch.is_ascii() {
                return Err(write_error("the save header must be ASCII"));
            }
            match self {
                Out::Wide(units) => units.push(u16::from(ch as u8)),
                Out::Byte(bytes, _) => bytes.push(ch as u8),
            }
        }
        self.separator();
        Ok(())
    }

    /// One encoded line: QSP subtracts 5 from every non-zero character and writes 5 as `-5`.
    fn encoded(&mut self, text: &str) -> Result<()> {
        match self {
            Out::Wide(units) => {
                for unit in text.encode_utf16() {
                    units.push(encode_unit(unit));
                }
            }
            Out::Byte(bytes, codec) => {
                let encoded = codec
                    .encode(text)
                    .map_err(|error| write_error(error.to_string()))?;
                for byte in encoded {
                    bytes.push(encode_byte(byte));
                }
            }
        }
        self.separator();
        Ok(())
    }

    fn into_bytes(self) -> Vec<u8> {
        match self {
            Out::Wide(units) => {
                let mut bytes = Vec::with_capacity(units.len() * 2);
                for unit in units {
                    bytes.extend_from_slice(&unit.to_le_bytes());
                }
                bytes
            }
            Out::Byte(bytes, _) => bytes,
        }
    }
}

fn encode_unit(unit: u16) -> u16 {
    if unit == SHIFT {
        unit.wrapping_neg()
    } else if unit == 0 {
        0
    } else {
        unit.wrapping_sub(SHIFT)
    }
}

fn encode_byte(byte: u8) -> u8 {
    let shift = u8::try_from(SHIFT).unwrap_or(5);
    if byte == shift {
        byte.wrapping_neg()
    } else if byte == 0 {
        0
    } else {
        byte.wrapping_sub(shift)
    }
}

/// A node's object entries.
fn object_entries<'a>(node: &'a Node, key: &str) -> Result<&'a [(String, NodeId)]> {
    match &node.value {
        Value::Object(entries) => Ok(entries),
        _ => Err(write_error(format!("'{key}' must be an object"))),
    }
}

fn array_items<'a>(node: &'a Node, key: &str) -> Result<&'a [NodeId]> {
    match &node.value {
        Value::Array(items) => Ok(items),
        _ => Err(write_error(format!("'{key}' must be a list"))),
    }
}

fn child(entries: &[(String, NodeId)], key: &str) -> Result<NodeId> {
    entries
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, id)| *id)
        .ok_or_else(|| write_error(format!("the entry '{key}' is missing")))
}

fn text_of<'a>(node: &'a Node, key: &str) -> Result<&'a str> {
    match &node.value {
        Value::Str(text) => Ok(text.as_str()),
        _ => Err(write_error(format!("'{key}' must be text"))),
    }
}

fn int_of(node: &Node, key: &str) -> Result<i64> {
    match node.value {
        Value::Int { v, .. } => i64::try_from(v)
            .map_err(|_| write_error(format!("the number at '{key}' does not fit 64 bits"))),
        _ => Err(write_error(format!("'{key}' must be a whole number"))),
    }
}

struct Writer<'a> {
    doc: &'a SaveDoc,
    params: &'a Params,
    out: Out,
    variant_ordinal: u64,
}

impl<'a> Writer<'a> {
    fn node(&self, id: NodeId) -> Result<&'a Node> {
        self.doc
            .get(id)
            .ok_or_else(|| write_error(format!("node {} does not exist", id.index())))
    }

    fn entries(&self, id: NodeId, key: &str) -> Result<&'a [(String, NodeId)]> {
        object_entries(self.node(id)?, key)
    }

    fn items(&self, id: NodeId, key: &str) -> Result<&'a [NodeId]> {
        array_items(self.node(id)?, key)
    }

    fn text(&mut self, entries: &[(String, NodeId)], key: &str) -> Result<()> {
        let id = child(entries, key)?;
        let text = text_of(self.node(id)?, key)?;
        self.out.encoded(text)
    }

    fn int(&mut self, entries: &[(String, NodeId)], key: &str) -> Result<()> {
        let id = child(entries, key)?;
        let node = self.node(id)?;
        let value = int_of(node, key)?;
        let text = match node.lexeme.as_deref() {
            Some(lexeme) => lexeme,
            None => return self.out.encoded(&value.to_string()),
        };
        self.out.encoded(text)
    }

    fn strings(&mut self, entries: &[(String, NodeId)], key: &str) -> Result<()> {
        let id = child(entries, key)?;
        let items = self.items(id, key)?;
        self.out.encoded(&items.len().to_string())?;
        for item in items {
            let text = text_of(self.node(*item)?, key)?;
            self.out.encoded(text)?;
        }
        Ok(())
    }

    fn actions(&mut self, entries: &[(String, NodeId)], key: &str) -> Result<()> {
        let id = child(entries, key)?;
        let items = self.items(id, key)?;
        self.out.encoded(&items.len().to_string())?;
        for item in items {
            let action = self.entries(*item, key)?;
            self.text(action, "description")?;
            self.text(action, "image")?;
            let lines_id = child(action, "lines")?;
            let lines = self.items(lines_id, "lines")?;
            self.out.encoded(&lines.len().to_string())?;
            for line_id in lines {
                let line = self.entries(*line_id, "line")?;
                self.text(line, "text")?;
                self.int(line, "line_number")?;
            }
            self.text(action, "location")?;
            self.int(action, "action_index")?;
        }
        Ok(())
    }

    fn objects(&mut self, entries: &[(String, NodeId)], key: &str) -> Result<()> {
        let id = child(entries, key)?;
        let items = self.items(id, key)?;
        self.out.encoded(&items.len().to_string())?;
        for item in items {
            let object = self.entries(*item, key)?;
            self.text(object, "name")?;
            self.text(object, "image")?;
        }
        Ok(())
    }

    fn object_groups(&mut self, entries: &[(String, NodeId)], key: &str) -> Result<()> {
        let id = child(entries, key)?;
        let items = self.items(id, key)?;
        self.out.encoded(&items.len().to_string())?;
        for item in items {
            let group = self.entries(*item, key)?;
            self.text(group, "name")?;
            self.text(group, "description")?;
            self.text(group, "image")?;
            self.int(group, "updated_fields")?;
            self.int(group, "objects_count")?;
        }
        Ok(())
    }

    fn variables(&mut self, entries: &[(String, NodeId)], key: &str) -> Result<()> {
        let id = child(entries, key)?;
        let items = self.items(id, key)?;
        self.out.encoded(&items.len().to_string())?;
        for item in items {
            let variable = self.entries(*item, key)?;
            self.text(variable, "name")?;
            let values_id = child(variable, "values")?;
            let values = self.items(values_id, "values")?;
            self.out.encoded(&values.len().to_string())?;
            for value in values {
                self.variant(*value, 1)?;
            }
            let keys_id = child(variable, "keys")?;
            let keys = self.items(keys_id, "keys")?;
            self.out.encoded(&keys.len().to_string())?;
            for key_id in keys {
                let key_entries = self.entries(*key_id, "key")?;
                self.int(key_entries, "index")?;
                let value = child(key_entries, "key")?;
                self.variant(value, 1)?;
            }
        }
        Ok(())
    }

    /// One value: its type line, then the payload. Tuples nest.
    fn variant(&mut self, id: NodeId, depth: usize) -> Result<()> {
        if depth > MAX_NESTING {
            return Err(write_error("values are nested too deeply"));
        }
        let node = self.node(id)?;
        let hint = node
            .type_hint
            .as_ref()
            .map(|hint| hint.as_str())
            .unwrap_or_default();
        let (code, kind) = variant_kind_from_hint(hint)
            .ok_or_else(|| write_error(format!("'{hint}' is not a QSP value hint")))?;
        let ordinal = self.variant_ordinal;
        self.variant_ordinal = self.variant_ordinal.saturating_add(1);
        let recorded = match self.params.get(P_VARIANT_TYPES) {
            Some(ParamValue::Table(table)) => match table.get(&ordinal.to_string()) {
                Some(ParamValue::Str(text)) => Some(text.as_str()),
                Some(_) => return Err(write_error("a variant type parameter must be text")),
                None => None,
            },
            Some(_) => return Err(write_error("the variant type parameter must be a table")),
            None => None,
        };
        let type_text = recorded
            .map(|text| text.to_string())
            .unwrap_or_else(|| code.to_string());
        self.out.encoded(&type_text)?;

        match kind {
            VariantKind::Undef => match node.value {
                Value::Null => Ok(()),
                _ => Err(write_error("an UNDEF value must be null")),
            },
            VariantKind::Tuple => {
                let items = array_items(node, "tuple")?;
                self.out.encoded(&items.len().to_string())?;
                for item in items {
                    self.variant(*item, depth + 1)?;
                }
                Ok(())
            }
            VariantKind::Num | VariantKind::Bool => self.written_int(node, "value"),
            VariantKind::Str | VariantKind::Code | VariantKind::VarRef => {
                let text = text_of(node, "value")?;
                self.out.encoded(text)
            }
        }
    }

    fn written_int(&mut self, node: &Node, key: &str) -> Result<()> {
        let value = int_of(node, key)?;
        let text = match node.lexeme.as_deref() {
            Some(lexeme) => lexeme.to_string(),
            None => value.to_string(),
        };
        self.out.encoded(&text)
    }
}

pub(super) fn encode(doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
    let encoding = encoding(params)?;
    let root = doc
        .get(doc.root())
        .ok_or_else(|| write_error("the document has no root"))?;
    let entries = object_entries(root, "root")?;
    let mut writer = Writer {
        doc,
        params,
        out: Out::new(encoding),
        variant_ordinal: 0,
    };

    let header = child(entries, "header")?;
    writer
        .out
        .raw_ascii(text_of(writer.node(header)?, "header")?)?;
    let version = child(entries, "version")?;
    writer
        .out
        .raw_ascii(text_of(writer.node(version)?, "version")?)?;

    writer.int(entries, "checksum")?;
    writer.int(entries, "playtime_ms")?;
    writer.int(entries, "selected_action")?;
    writer.int(entries, "selected_object")?;
    writer.text(entries, "view_path")?;
    writer.text(entries, "input")?;
    writer.text(entries, "description")?;
    writer.text(entries, "vars_description")?;
    writer.text(entries, "location")?;
    writer.int(entries, "windows_state")?;
    writer.int(entries, "timer_interval")?;
    writer.strings(entries, "playlist")?;
    writer.strings(entries, "includes")?;
    writer.actions(entries, "actions")?;
    writer.objects(entries, "objects")?;
    writer.object_groups(entries, "object_groups")?;
    writer.variables(entries, "variables")?;

    Ok(writer.out.into_bytes())
}
