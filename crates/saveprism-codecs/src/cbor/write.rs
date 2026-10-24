//! Exact writing: the original item with only edited leaf spans replaced (see the module docs).

use std::collections::BTreeMap;

use saveprism_core::{
    Error, FloatWidth, Limits, Node, NodeFlags, NodeId, Params, Result, SaveDoc, Value,
};

use super::read;
use super::{CBOR_WRITE, ORIGINAL};

fn write_error(message: impl Into<String>) -> Error {
    Error::new(CBOR_WRITE, message)
}

pub(super) fn encode(doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
    let original = params.bytes(ORIGINAL)?;
    let (fresh, _) = read::decode(original, &Limits::default())?;
    let mut patches: BTreeMap<usize, (usize, Vec<u8>)> = BTreeMap::new();
    compare(doc, doc.root(), &fresh, fresh.root(), &mut patches)?;
    apply(original, patches)
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
                return Err(write_error("the number of array items changed"));
            }
            for (left, right) in x.iter().zip(y.iter()) {
                compare(doc, *left, fresh, *right, patches)?;
            }
            Ok(())
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
            Ok(())
        }
        (Value::Map(x), Value::Map(y)) => {
            if x.len() != y.len() {
                return Err(write_error("the number of map entries changed"));
            }
            for ((left_key, left), (right_key, right)) in x.iter().zip(y.iter()) {
                compare(doc, *left_key, fresh, *right_key, patches)?;
                compare(doc, *left, fresh, *right, patches)?;
            }
            Ok(())
        }
        (Value::Ref(_), _) | (_, Value::Ref(_)) => {
            Err(write_error("references are not part of a CBOR document"))
        }
        _ => {
            if same_scalar(&ours.value, &theirs.value) {
                return Ok(());
            }
            if ours.flags.contains(NodeFlags::READ_ONLY) {
                return Err(write_error("an edited value is read-only"));
            }
            let bytes = encode_value(&ours.value)?;
            let span = theirs
                .span
                .ok_or_else(|| write_error("an edited value has no byte span"))?;
            let start = usize::try_from(span.start)
                .map_err(|_| write_error("the byte span is out of range"))?;
            let end = usize::try_from(span.end)
                .map_err(|_| write_error("the byte span is out of range"))?;
            patches.insert(start, (end, bytes));
            Ok(())
        }
    }
}

fn encode_value(value: &Value) -> Result<Vec<u8>> {
    Ok(match value {
        Value::Bool(true) => vec![0xf5],
        Value::Bool(false) => vec![0xf4],
        Value::Int { v, .. } => encode_int(*v)?,
        Value::Float { v, width } => match width {
            FloatWidth::F32 => {
                let mut out = vec![0xfa];
                out.extend_from_slice(&(*v as f32).to_bits().to_be_bytes());
                out
            }
            FloatWidth::F64 => {
                let mut out = vec![0xfb];
                out.extend_from_slice(&v.to_bits().to_be_bytes());
                out
            }
        },
        Value::Str(text) => encode_text(text.as_bytes())?,
        _ => return Err(write_error("only scalars and strings can be edited")),
    })
}

fn encode_head(major: u8, value: u64) -> Result<Vec<u8>> {
    let base = major << 5;
    Ok(if value < 24 {
        vec![base | u8::try_from(value).unwrap_or(23)]
    } else if let Ok(byte) = u8::try_from(value) {
        vec![base | 24, byte]
    } else if let Ok(short) = u16::try_from(value) {
        let mut out = vec![base | 25];
        out.extend_from_slice(&short.to_be_bytes());
        out
    } else if let Ok(word) = u32::try_from(value) {
        let mut out = vec![base | 26];
        out.extend_from_slice(&word.to_be_bytes());
        out
    } else {
        let mut out = vec![base | 27];
        out.extend_from_slice(&value.to_be_bytes());
        out
    })
}

fn encode_int(value: i128) -> Result<Vec<u8>> {
    if value >= 0 {
        let argument =
            u64::try_from(value).map_err(|_| write_error("the integer does not fit in CBOR"))?;
        encode_head(0, argument)
    } else {
        let argument = (-1 - value)
            .try_into()
            .map_err(|_| write_error("the integer does not fit in CBOR"))?;
        encode_head(1, argument)
    }
}

fn encode_text(data: &[u8]) -> Result<Vec<u8>> {
    let length = u64::try_from(data.len()).map_err(|_| write_error("the text is too long"))?;
    let mut out = encode_head(3, length)?;
    out.extend_from_slice(data);
    Ok(out)
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
