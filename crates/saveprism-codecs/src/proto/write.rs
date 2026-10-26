//! Exact Protocol Buffers writing: unchanged fields are copied from the original bytes, an edited
//! field is re-encoded and every enclosing message gets a new length prefix.

use saveprism_core::{Error, Node, NodeFlags, NodeId, Params, Result, SaveDoc, Value};

use super::read;
use super::{ORIGINAL, PROTO_WRITE};

fn write_error(message: impl Into<String>) -> Error {
    Error::new(PROTO_WRITE, message)
}

fn node(doc: &SaveDoc, id: NodeId) -> Result<&Node> {
    doc.get(id)
        .ok_or_else(|| write_error(format!("node {} is missing", id.index())))
}

fn entries(node: &Node) -> Result<&[(String, NodeId)]> {
    match &node.value {
        Value::Object(entries) => Ok(entries),
        _ => Err(write_error("a protobuf field is not an object")),
    }
}

fn child(entries: &[(String, NodeId)], key: &str) -> Result<NodeId> {
    entries
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, id)| *id)
        .ok_or_else(|| write_error(format!("the field entry '{key}' is missing")))
}

fn int_of(node: &Node, key: &str) -> Result<u64> {
    match node.value {
        Value::Int { v, .. } => u64::try_from(v)
            .map_err(|_| write_error(format!("'{key}' is not an unsigned 64-bit value"))),
        _ => Err(write_error(format!("'{key}' is not a number"))),
    }
}

fn same_scalar(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Null, Value::Null) => true,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Int { v: x, .. }, Value::Int { v: y, .. }) => x == y,
        (Value::Float { v: x, .. }, Value::Float { v: y, .. }) => x.to_bits() == y.to_bits(),
        (Value::Str(x), Value::Str(y)) => x == y,
        (Value::Bytes(x), Value::Bytes(y)) => x == y,
        _ => false,
    }
}

fn encode_varint(mut value: u64, out: &mut Vec<u8>) {
    loop {
        let mut byte = u8::try_from(value & 0x7f).unwrap_or(0);
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            break;
        }
    }
}

fn tag(number: u64, wire: u64) -> Vec<u8> {
    let mut out = Vec::new();
    encode_varint((number << 3) | (wire & 7), &mut out);
    out
}

fn original_slice<'a>(original: &'a [u8], node: &Node, what: &str) -> Result<&'a [u8]> {
    let span = node
        .span
        .ok_or_else(|| write_error(format!("{what} has no byte span")))?;
    let start = usize::try_from(span.start).map_err(|_| write_error("the byte span is too big"))?;
    let end = usize::try_from(span.end).map_err(|_| write_error("the byte span is too big"))?;
    original
        .get(start..end)
        .ok_or_else(|| write_error("a byte span points outside the original data"))
}

/// Writes a changed field from its tree value.
fn encode_field(doc: &SaveDoc, value_id: NodeId, number: u64, wire: u64) -> Result<Vec<u8>> {
    let value = node(doc, value_id)?;
    if value.flags.contains(NodeFlags::READ_ONLY) {
        return Err(write_error("the edited field value is read-only"));
    }
    let mut out = tag(number, wire);
    match (wire, &value.value) {
        (0, Value::Int { v, .. }) => {
            if *v < 0 {
                return Err(write_error("a varint cannot be negative"));
            }
            let value =
                u64::try_from(*v).map_err(|_| write_error("the varint does not fit 64 bits"))?;
            encode_varint(value, &mut out);
        }
        (1, Value::Int { v, .. }) => {
            let value = u64::try_from(*v)
                .map_err(|_| write_error("the fixed64 value does not fit 64 bits"))?;
            out.extend_from_slice(&value.to_le_bytes());
        }
        (5, Value::Int { v, .. }) => {
            let value = u32::try_from(*v)
                .map_err(|_| write_error("the fixed32 value does not fit 32 bits"))?;
            out.extend_from_slice(&value.to_le_bytes());
        }
        (2, Value::Str(text)) => {
            encode_varint(u64::try_from(text.len()).unwrap_or(u64::MAX), &mut out);
            out.extend_from_slice(text.as_bytes());
        }
        (2, Value::Bytes(bytes)) => {
            encode_varint(u64::try_from(bytes.len()).unwrap_or(u64::MAX), &mut out);
            out.extend_from_slice(bytes);
        }
        _ => {
            return Err(write_error(
                "only varints, fixed values, strings and byte fields can be edited",
            ));
        }
    }
    Ok(out)
}

fn encode_message(
    doc: &SaveDoc,
    current: NodeId,
    fresh: &SaveDoc,
    original: NodeId,
    original_bytes: &[u8],
) -> Result<(Vec<u8>, bool)> {
    let ours = node(doc, current)?;
    let theirs = node(fresh, original)?;
    let (ours, theirs) = match (&ours.value, &theirs.value) {
        (Value::Array(ours), Value::Array(theirs)) => (ours, theirs),
        _ => return Err(write_error("a message changed its shape")),
    };
    if ours.len() != theirs.len() {
        return Err(write_error("the number of fields in a message changed"));
    }
    let mut out = Vec::new();
    let mut changed = false;
    for (current_field, fresh_field) in ours.iter().zip(theirs.iter()) {
        let current_node = node(doc, *current_field)?;
        let fresh_node = node(fresh, *fresh_field)?;
        let current_entries = entries(current_node)?;
        let fresh_entries = entries(fresh_node)?;
        let number = int_of(node(doc, child(current_entries, "number")?)?, "number")?;
        let wire = int_of(node(doc, child(current_entries, "wire")?)?, "wire")?;
        let fresh_number = int_of(node(fresh, child(fresh_entries, "number")?)?, "number")?;
        let fresh_wire = int_of(node(fresh, child(fresh_entries, "wire")?)?, "wire")?;
        if number != fresh_number || wire != fresh_wire {
            return Err(write_error("a field number or wire type changed"));
        }
        let current_value = child(current_entries, "value")?;
        let fresh_value = child(fresh_entries, "value")?;
        let current_value_node = node(doc, current_value)?;
        let fresh_value_node = node(fresh, fresh_value)?;
        match (&current_value_node.value, &fresh_value_node.value) {
            (Value::Array(_), Value::Array(_)) => {
                let (payload, inner_changed) =
                    encode_message(doc, current_value, fresh, fresh_value, original_bytes)?;
                if inner_changed {
                    out.extend_from_slice(&tag(number, wire));
                    encode_varint(u64::try_from(payload.len()).unwrap_or(u64::MAX), &mut out);
                    out.extend_from_slice(&payload);
                    changed = true;
                } else {
                    out.extend_from_slice(original_slice(
                        original_bytes,
                        fresh_node,
                        "an unchanged field",
                    )?);
                }
            }
            _ => {
                if same_scalar(&current_value_node.value, &fresh_value_node.value) {
                    out.extend_from_slice(original_slice(
                        original_bytes,
                        fresh_node,
                        "an unchanged field",
                    )?);
                } else {
                    let bytes = encode_field(doc, current_value, number, wire)?;
                    out.extend_from_slice(&bytes);
                    changed = true;
                }
            }
        }
    }
    Ok((out, changed))
}

/// Encodes `doc`, keeping every unchanged field's original bytes.
pub(super) fn encode(doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
    let original_bytes = params.bytes(ORIGINAL)?;
    let (fresh, _) = read::decode(original_bytes, &saveprism_core::Limits::default())?;
    let (bytes, _) = encode_message(doc, doc.root(), &fresh, fresh.root(), original_bytes)?;
    Ok(bytes)
}
