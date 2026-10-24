//! BSON writing: the whole document is written again and every length is recomputed.

use saveprism_core::{Error, FloatWidth, Node, NodeId, Params, Result, SaveDoc, Value};

use super::BSON_WRITE;

fn write_error(message: impl Into<String>) -> Error {
    Error::new(BSON_WRITE, message)
}

pub(super) fn encode(doc: &SaveDoc, _params: &Params) -> Result<Vec<u8>> {
    let root = doc
        .get(doc.root())
        .ok_or_else(|| write_error("the document has no root"))?;
    if root.type_hint.as_ref().map(|hint| hint.as_str()) != Some("BSON:document") {
        return Err(write_error("the root is not a BSON document"));
    }
    write_document(doc, doc.root(), false)
}

fn write_document(doc: &SaveDoc, id: NodeId, array: bool) -> Result<Vec<u8>> {
    let node = doc
        .get(id)
        .ok_or_else(|| write_error(format!("node {} is missing", id.index())))?;
    let entries: Vec<(String, NodeId)> = match &node.value {
        Value::Object(entries) if !array => entries.clone(),
        Value::Array(items) if array => items
            .iter()
            .enumerate()
            .map(|(index, child)| (index.to_string(), *child))
            .collect(),
        _ => {
            return Err(write_error(
                "a BSON document or array has the wrong node type",
            ));
        }
    };
    let mut body = Vec::new();
    for (name, child) in entries {
        write_element(doc, &name, child, &mut body)?;
    }
    let length = body
        .len()
        .checked_add(5)
        .and_then(|value| i32::try_from(value).ok())
        .ok_or_else(|| write_error("the BSON document is too large"))?;
    let mut out = Vec::with_capacity(usize::try_from(length).unwrap_or(0));
    out.extend_from_slice(&length.to_le_bytes());
    out.extend_from_slice(&body);
    out.push(0);
    Ok(out)
}

fn write_element(doc: &SaveDoc, name: &str, child: NodeId, out: &mut Vec<u8>) -> Result<()> {
    if name.as_bytes().contains(&0) {
        return Err(write_error("a BSON element name contains a NUL"));
    }
    let node = doc
        .get(child)
        .ok_or_else(|| write_error(format!("node {} is missing", child.index())))?;
    let (kind, payload) = element_payload(doc, child, node)?;
    out.push(kind);
    out.extend_from_slice(name.as_bytes());
    out.push(0);
    out.extend_from_slice(&payload);
    Ok(())
}

fn hint_of(node: &Node) -> Option<&str> {
    node.type_hint.as_ref().map(|hint| hint.as_str())
}

fn element_payload(doc: &SaveDoc, id: NodeId, node: &Node) -> Result<(u8, Vec<u8>)> {
    let hint = hint_of(node).ok_or_else(|| write_error("a BSON value has no type hint"))?;
    match hint {
        "BSON:double" => {
            let Value::Float { v, width } = node.value else {
                return Err(write_error("a double has the wrong value"));
            };
            let value = match width {
                FloatWidth::F64 => v,
                FloatWidth::F32 => f64::from(v as f32),
            };
            Ok((0x01, value.to_bits().to_le_bytes().to_vec()))
        }
        "BSON:string" => {
            let Value::Str(text) = &node.value else {
                return Err(write_error("a string has the wrong value"));
            };
            Ok((0x02, bson_string(text)?))
        }
        "BSON:document" => Ok((0x03, write_document(doc, id, false)?)),
        "BSON:array" => Ok((0x04, write_document(doc, id, true)?)),
        hint if hint.starts_with("BSON:binary:") => {
            let subtype = hint
                .strip_prefix("BSON:binary:")
                .and_then(|text| text.parse::<u8>().ok())
                .ok_or_else(|| write_error("a binary subtype hint is malformed"))?;
            let Value::Bytes(data) = &node.value else {
                return Err(write_error("a binary value is not bytes"));
            };
            let length = i32::try_from(data.len())
                .map_err(|_| write_error("the binary value is too large"))?;
            let mut payload = Vec::with_capacity(data.len() + 5);
            payload.extend_from_slice(&length.to_le_bytes());
            payload.push(subtype);
            payload.extend_from_slice(data);
            Ok((0x05, payload))
        }
        "BSON:undefined" => Ok((0x06, Vec::new())),
        "BSON:objectid" => {
            let Value::Bytes(data) = &node.value else {
                return Err(write_error("an ObjectId is not bytes"));
            };
            if data.len() != 12 {
                return Err(write_error("an ObjectId is not 12 bytes"));
            }
            Ok((0x07, data.clone()))
        }
        "BSON:bool" => {
            let Value::Bool(value) = node.value else {
                return Err(write_error("a boolean has the wrong value"));
            };
            Ok((0x08, vec![u8::from(value)]))
        }
        "BSON:date" => {
            let Value::Int { v, .. } = node.value else {
                return Err(write_error("a date has the wrong value"));
            };
            let value = i64::try_from(v).map_err(|_| write_error("the date does not fit"))?;
            Ok((0x09, value.to_le_bytes().to_vec()))
        }
        "BSON:null" => Ok((0x0a, Vec::new())),
        "BSON:regex" => Ok((0x0b, raw(node)?)),
        "BSON:dbpointer" => Ok((0x0c, raw(node)?)),
        "BSON:code" => {
            let Value::Str(text) = &node.value else {
                return Err(write_error("JavaScript code has the wrong value"));
            };
            Ok((0x0d, bson_string(text)?))
        }
        "BSON:symbol" => {
            let Value::Str(text) = &node.value else {
                return Err(write_error("a symbol has the wrong value"));
            };
            Ok((0x0e, bson_string(text)?))
        }
        "BSON:code-w-scope" => Ok((0x0f, raw(node)?)),
        "BSON:int32" => {
            let Value::Int { v, .. } = node.value else {
                return Err(write_error("an int32 has the wrong value"));
            };
            let value =
                i32::try_from(v).map_err(|_| write_error("the int32 value is out of range"))?;
            Ok((0x10, value.to_le_bytes().to_vec()))
        }
        "BSON:timestamp" => {
            let Value::Int { v, .. } = node.value else {
                return Err(write_error("a timestamp has the wrong value"));
            };
            let value =
                u64::try_from(v).map_err(|_| write_error("the timestamp is out of range"))?;
            Ok((0x11, value.to_le_bytes().to_vec()))
        }
        "BSON:int64" => {
            let Value::Int { v, .. } = node.value else {
                return Err(write_error("an int64 has the wrong value"));
            };
            let value =
                i64::try_from(v).map_err(|_| write_error("the int64 value is out of range"))?;
            Ok((0x12, value.to_le_bytes().to_vec()))
        }
        "BSON:decimal128" => {
            let Value::Bytes(data) = &node.value else {
                return Err(write_error("a decimal128 is not bytes"));
            };
            if data.len() != 16 {
                return Err(write_error("a decimal128 is not 16 bytes"));
            }
            Ok((0x13, data.clone()))
        }
        "BSON:maxkey" => Ok((0x7f, Vec::new())),
        "BSON:minkey" => Ok((0xff, Vec::new())),
        other => Err(write_error(format!("unknown BSON type hint {other}"))),
    }
}

fn bson_string(text: &str) -> Result<Vec<u8>> {
    let length = text
        .len()
        .checked_add(1)
        .and_then(|value| i32::try_from(value).ok())
        .ok_or_else(|| write_error("the BSON string is too long"))?;
    let mut out = Vec::with_capacity(text.len() + 5);
    out.extend_from_slice(&length.to_le_bytes());
    out.extend_from_slice(text.as_bytes());
    out.push(0);
    Ok(out)
}

fn raw(node: &Node) -> Result<Vec<u8>> {
    match &node.value {
        Value::Bytes(data) => Ok(data.clone()),
        _ => Err(write_error("a raw BSON value is not bytes")),
    }
}
