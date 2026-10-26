//! Exact Bitser writing: unchanged bytes are copied, an edited scalar or string replaces only its
//! own span.

use std::collections::BTreeMap;

use saveprism_core::{Error, FloatWidth, Node, NodeFlags, NodeId, Params, Result, SaveDoc, Value};

use super::read;
use super::{BITSER_WRITE, ORIGINAL};

fn write_error(message: impl Into<String>) -> Error {
    Error::new(BITSER_WRITE, message)
}

fn node(doc: &SaveDoc, id: NodeId) -> Result<&Node> {
    doc.get(id)
        .ok_or_else(|| write_error(format!("node {} is missing", id.index())))
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

/// Encodes a number the way Bitser's `write_number` does (used for array lengths and edited ints).
fn encode_number(value: i128) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    if (-27..=100).contains(&value) {
        out.push(u8::try_from(value + 27).unwrap_or(0));
    } else if let Ok(short) = i16::try_from(value) {
        out.push(250);
        out.extend_from_slice(&short.to_le_bytes());
    } else if let Ok(long) = i32::try_from(value) {
        out.push(245);
        out.extend_from_slice(&long.to_le_bytes());
    } else {
        return Err(write_error("the number does not fit a Bitser integer"));
    }
    Ok(out)
}

fn encode_string(text: &str) -> Result<Vec<u8>> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() + 5);
    if let Ok(length) = u8::try_from(bytes.len()) {
        if length < 32 {
            out.push(192 + length);
        } else {
            out.push(244);
            out.extend_from_slice(&encode_number(i128::from(length))?);
        }
    } else {
        out.push(244);
        out.extend_from_slice(&encode_number(
            i128::try_from(bytes.len()).unwrap_or(i128::MAX),
        )?);
    }
    out.extend_from_slice(bytes);
    Ok(out)
}

fn encode_scalar(value: &Value) -> Result<Vec<u8>> {
    Ok(match value {
        Value::Bool(true) => vec![249],
        Value::Bool(false) => vec![248],
        Value::Int { v, .. } => encode_number(*v)?,
        Value::Float { v, width } => match width {
            FloatWidth::F64 => {
                let mut out = vec![246];
                out.extend_from_slice(&v.to_le_bytes());
                out
            }
            FloatWidth::F32 => {
                return Err(write_error("Bitser has no 32-bit float"));
            }
        },
        Value::Str(text) => encode_string(text)?,
        _ => {
            return Err(write_error(
                "only numbers, booleans and strings can be edited",
            ));
        }
    })
}

fn compare(
    doc: &SaveDoc,
    current: NodeId,
    fresh: &SaveDoc,
    original: NodeId,
    patches: &mut BTreeMap<usize, (usize, Vec<u8>)>,
) -> Result<()> {
    let ours = node(doc, current)?;
    let theirs = node(fresh, original)?;
    match (&ours.value, &theirs.value) {
        (Value::Array(x), Value::Array(y)) => {
            if x.len() != y.len() {
                return Err(write_error("the number of array elements changed"));
            }
            for (left, right) in x.iter().zip(y.iter()) {
                compare(doc, *left, fresh, *right, patches)?;
            }
        }
        (Value::Object(x), Value::Object(y)) => {
            if x.len() != y.len() {
                return Err(write_error("the number of object entries changed"));
            }
            for ((left_key, left), (right_key, right)) in x.iter().zip(y.iter()) {
                if left_key != right_key {
                    return Err(write_error("an object key changed"));
                }
                compare(doc, *left, fresh, *right, patches)?;
            }
        }
        (Value::Map(x), Value::Map(y)) => {
            if x.len() != y.len() {
                return Err(write_error("the number of map entries changed"));
            }
            for ((left_key, left), (right_key, right)) in x.iter().zip(y.iter()) {
                compare(doc, *left_key, fresh, *right_key, patches)?;
                compare(doc, *left, fresh, *right, patches)?;
            }
        }
        (Value::Ref(_), Value::Ref(_)) => {}
        (Value::Ref(_), _) | (_, Value::Ref(_)) => {
            return Err(write_error("a reference changed its place or type"));
        }
        _ => {
            if same_scalar(&ours.value, &theirs.value) {
                return Ok(());
            }
            if ours.flags.contains(NodeFlags::READ_ONLY) {
                return Err(write_error("an edited value is read-only"));
            }
            let bytes = encode_scalar(&ours.value)?;
            let span = theirs
                .span
                .ok_or_else(|| write_error("an edited value has no byte span"))?;
            let start = usize::try_from(span.start)
                .map_err(|_| write_error("the byte span is out of range"))?;
            let end = usize::try_from(span.end)
                .map_err(|_| write_error("the byte span is out of range"))?;
            patches.insert(start, (end, bytes));
        }
    }
    Ok(())
}

fn apply(original: &[u8], patches: BTreeMap<usize, (usize, Vec<u8>)>) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(original.len());
    let mut at = 0_usize;
    for (start, (end, bytes)) in patches {
        if start < at || end < start || end > original.len() {
            return Err(write_error(
                "the edited byte spans overlap or are out of range",
            ));
        }
        out.extend_from_slice(
            original
                .get(at..start)
                .ok_or_else(|| write_error("the edited byte span is out of range"))?,
        );
        out.extend_from_slice(&bytes);
        at = end;
    }
    out.extend_from_slice(
        original
            .get(at..)
            .ok_or_else(|| write_error("the edited byte span is out of range"))?,
    );
    Ok(out)
}

/// Encodes `doc`, keeping every byte of the original that no edit changed.
pub(super) fn encode(doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
    let original = params.bytes(ORIGINAL)?;
    let (fresh, _) = read::decode(original, &saveprism_core::Limits::default())?;
    let mut patches = BTreeMap::new();
    compare(doc, doc.root(), &fresh, fresh.root(), &mut patches)?;
    apply(original, patches)
}
