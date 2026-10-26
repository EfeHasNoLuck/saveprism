//! Patches fixed-size LiteDB values in place and refuses every other change.

use std::collections::BTreeMap;

use saveprism_core::{
    ByteSpan, FloatWidth, IntWidth, Limits, NodeFlags, NodeId, ParamValue, Params, Result, SaveDoc,
    Value,
};

use super::read::Indexed;
use super::{ORIGINAL, read, write_error};

struct Patch {
    span: ByteSpan,
    bytes: Vec<u8>,
}

/// Patches the fixed-size edits; unchanged documents return the original bytes.
pub(super) fn encode(doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
    let original = match params.get(ORIGINAL) {
        Some(ParamValue::Bytes(bytes)) => bytes.as_slice(),
        _ => return Err(write_error("original LiteDB bytes are missing")),
    };
    let limits = Limits::default();
    limits.check_input(saveprism_core::limits::len_u64(original.len()))?;
    let baseline = read::decode(original, &limits)?;
    if doc.tree_eq(&baseline) {
        return Ok(original.to_vec());
    }
    let plan = read::index_plan(original, &limits)?;
    let mut patches = Vec::new();
    walk(
        doc,
        doc.root(),
        &baseline,
        baseline.root(),
        &plan,
        None,
        None,
        &mut patches,
    )?;
    if patches.is_empty() {
        return Ok(original.to_vec());
    }
    let mut out = original.to_vec();
    for patch in patches {
        let start = usize::try_from(patch.span.start)
            .map_err(|_| write_error("a value span start is out of range"))?;
        let end = usize::try_from(patch.span.end)
            .map_err(|_| write_error("a value span end is out of range"))?;
        let target = out
            .get_mut(start..end)
            .ok_or_else(|| write_error("a value span lies outside the file"))?;
        if target.len() != patch.bytes.len() {
            return Err(write_error("a value span has an unexpected length"));
        }
        target.copy_from_slice(&patch.bytes);
    }
    Ok(out)
}

#[allow(clippy::too_many_arguments)]
fn walk(
    doc: &SaveDoc,
    doc_id: NodeId,
    baseline: &SaveDoc,
    base_id: NodeId,
    plan: &BTreeMap<String, Indexed>,
    collection: Option<&str>,
    top_field: Option<&str>,
    patches: &mut Vec<Patch>,
) -> Result<()> {
    let doc_node = doc
        .get(doc_id)
        .ok_or_else(|| write_error("the document has a missing node"))?;
    let base_node = baseline
        .get(base_id)
        .ok_or_else(|| write_error("the baseline has a missing node"))?;
    match (&doc_node.value, &base_node.value) {
        (Value::Object(doc_entries), Value::Object(base_entries)) => {
            if doc_entries.len() != base_entries.len() {
                return Err(write_error("documents cannot gain or lose fields"));
            }
            for ((doc_key, doc_child), (base_key, base_child)) in
                doc_entries.iter().zip(base_entries.iter())
            {
                if doc_key != base_key {
                    return Err(write_error("field names cannot change"));
                }
                let (next_collection, next_top) = if collection.is_none() {
                    (Some(doc_key.as_str()), None)
                } else if top_field.is_none() {
                    (collection, Some(doc_key.as_str()))
                } else {
                    (collection, top_field)
                };
                walk(
                    doc,
                    *doc_child,
                    baseline,
                    *base_child,
                    plan,
                    next_collection,
                    next_top,
                    patches,
                )?;
            }
            Ok(())
        }
        (Value::Array(doc_items), Value::Array(base_items)) => {
            if doc_items.len() != base_items.len() {
                return Err(write_error("arrays cannot gain or lose items"));
            }
            for (doc_child, base_child) in doc_items.iter().zip(base_items.iter()) {
                walk(
                    doc,
                    *doc_child,
                    baseline,
                    *base_child,
                    plan,
                    collection,
                    top_field,
                    patches,
                )?;
            }
            Ok(())
        }
        (doc_value, base_value) => {
            if same_value(doc_value, base_value) {
                return Ok(());
            }
            if base_node.flags.contains(NodeFlags::READ_ONLY) {
                return Err(write_error(
                    "this value is read-only: its type, length or index would change",
                ));
            }
            let indexed = top_field == Some("_id")
                || collection.is_some_and(|name| match plan.get(name) {
                    Some(Indexed::All) => true,
                    Some(Indexed::Fields(fields)) => {
                        top_field.is_some_and(|field| fields.iter().any(|name| name == field))
                    }
                    None => true,
                });
            if indexed {
                return Err(write_error(
                    "this field is covered by a collection index, so changing it in place would leave the index stale",
                ));
            }
            let bytes = patch_bytes(base_node, doc_value)?;
            let span = base_node.span.ok_or_else(|| {
                write_error("this value has no in-place location, so it cannot be edited")
            })?;
            if span.end <= span.start {
                return Err(write_error("this value has an empty in-place location"));
            }
            patches.push(Patch { span, bytes });
            Ok(())
        }
    }
}

fn same_value(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Null, Value::Null) => true,
        (Value::Bool(a), Value::Bool(b)) => a == b,
        (Value::Int { v: a, .. }, Value::Int { v: b, .. }) => a == b,
        (Value::Float { v: a, .. }, Value::Float { v: b, .. }) => a.to_bits() == b.to_bits(),
        (Value::Str(a), Value::Str(b)) => a == b,
        (Value::Bytes(a), Value::Bytes(b)) => a == b,
        _ => false,
    }
}

fn patch_bytes(base: &saveprism_core::Node, edited: &Value) -> Result<Vec<u8>> {
    match (&base.value, edited) {
        (
            Value::Int {
                width: IntWidth::I32,
                ..
            },
            Value::Int { v, .. },
        ) => {
            let value = i32::try_from(*v)
                .map_err(|_| write_error("the integer does not fit in 32 bits"))?;
            Ok(value.to_le_bytes().to_vec())
        }
        (
            Value::Int {
                width: IntWidth::I64,
                ..
            },
            Value::Int { v, .. },
        ) => {
            let value = i64::try_from(*v)
                .map_err(|_| write_error("the integer does not fit in 64 bits"))?;
            Ok(value.to_le_bytes().to_vec())
        }
        (
            Value::Float {
                width: FloatWidth::F64,
                ..
            },
            Value::Float { v, .. },
        ) => Ok(v.to_le_bytes().to_vec()),
        (Value::Bool(_), Value::Bool(value)) => Ok(vec![u8::from(*value)]),
        _ => Err(write_error(
            "only a fixed-size integer, double or boolean can change without moving data",
        )),
    }
}
