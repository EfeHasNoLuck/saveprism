//! Exact writing: the original opcode stream with only the edited scalar opcodes replaced.
//!
//! The original stream is decoded again into the same tree, the two trees are walked side by side
//! and must have the same shape; every scalar whose value changed must be editable and gets new
//! opcode bytes. Protocol 4+ `FRAME` lengths are recomputed. Nothing else changes, so an unchanged
//! document gives the original bytes.

use std::collections::{BTreeMap, HashMap};

use saveprism_core::{Error, Limits, Node, NodeFlags, NodeId, Params, Result, SaveDoc, Value};

use super::ops::{self, Reader};
use super::{MAX_NESTING, PICKLE_WRITE, STREAM_PARAM, decode_stream};

fn write_error(message: impl Into<String>) -> Error {
    Error::new(PICKLE_WRITE, message)
}

/// Shortest two's complement little-endian bytes of `value` (empty for 0), as `LONG1` stores it.
pub(crate) fn long_bytes(value: i128) -> Vec<u8> {
    if value == 0 {
        return Vec::new();
    }
    let raw = value.to_le_bytes();
    let mut len = raw.len();
    while len > 1 {
        let (Some(&last), Some(&before)) = (raw.get(len - 1), raw.get(len - 2)) else {
            break;
        };
        let redundant =
            (last == 0x00 && before & 0x80 == 0) || (last == 0xFF && before & 0x80 != 0);
        if !redundant {
            break;
        }
        len -= 1;
    }
    raw.get(..len).unwrap_or_default().to_vec()
}

fn long_op(value: i128) -> Vec<u8> {
    let bytes = long_bytes(value);
    let mut out = Vec::with_capacity(bytes.len() + 5);
    match u8::try_from(bytes.len()) {
        Ok(len) => {
            out.push(ops::LONG1);
            out.push(len);
        }
        Err(_) => {
            out.push(ops::LONG4);
            out.extend_from_slice(&u32::try_from(bytes.len()).unwrap_or(0).to_le_bytes());
        }
    }
    out.extend_from_slice(&bytes);
    out
}

/// The binary integer opcode Python's pickler picks for `value`.
fn binint_op(value: i128, proto: u8) -> Vec<u8> {
    if let Ok(small) = u8::try_from(value) {
        return vec![ops::BININT1, small];
    }
    if let Ok(medium) = u16::try_from(value) {
        let mut out = vec![ops::BININT2];
        out.extend_from_slice(&medium.to_le_bytes());
        return out;
    }
    if let Ok(word) = i32::try_from(value) {
        let mut out = vec![ops::BININT];
        out.extend_from_slice(&word.to_le_bytes());
        return out;
    }
    if proto >= 2 {
        long_op(value)
    } else {
        format!("I{value}\n").into_bytes()
    }
}

fn int_op(code: u8, original: &[u8], value: i128, proto: u8) -> Result<Vec<u8>> {
    Ok(match code {
        ops::BININT1 | ops::BININT2 | ops::BININT => binint_op(value, proto),
        ops::LONG1 | ops::LONG4 => long_op(value),
        ops::INT => format!("I{value}\n").into_bytes(),
        ops::LONG => {
            if original.ends_with(b"L\n") {
                format!("L{value}L\n").into_bytes()
            } else {
                format!("L{value}\n").into_bytes()
            }
        }
        _ => return Err(write_error("an edited integer has no integer opcode")),
    })
}

fn bool_op(code: u8, value: bool) -> Result<Vec<u8>> {
    Ok(match (code, value) {
        (ops::NEWTRUE | ops::NEWFALSE, true) => vec![ops::NEWTRUE],
        (ops::NEWTRUE | ops::NEWFALSE, false) => vec![ops::NEWFALSE],
        (ops::INT, true) => b"I01\n".to_vec(),
        (ops::INT, false) => b"I00\n".to_vec(),
        _ => return Err(write_error("an edited boolean has no boolean opcode")),
    })
}

fn float_op(code: u8, value: f64) -> Result<Vec<u8>> {
    Ok(match code {
        ops::BINFLOAT => {
            let mut out = vec![ops::BINFLOAT];
            out.extend_from_slice(&value.to_be_bytes());
            out
        }
        ops::FLOAT => format!("F{value:?}\n").into_bytes(),
        _ => return Err(write_error("an edited float has no float opcode")),
    })
}

/// Python's protocol 0 text escaping: `raw-unicode-escape` after replacing the characters that
/// would end or confuse the line.
pub(crate) fn raw_unicode_escape_encode(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    for c in text.chars() {
        let code = u32::from(c);
        match c {
            '\\' | '\0' | '\n' | '\r' | '\u{1a}' => {
                out.extend_from_slice(format!("\\u{code:04x}").as_bytes());
            }
            _ if code < 0x100 => out.push(u8::try_from(code).unwrap_or(b'?')),
            _ if code <= 0xFFFF => out.extend_from_slice(format!("\\u{code:04x}").as_bytes()),
            _ => out.extend_from_slice(format!("\\U{code:08x}").as_bytes()),
        }
    }
    out
}

fn counted(code: u8, prefix: &[u8], data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(1 + prefix.len() + data.len());
    out.push(code);
    out.extend_from_slice(prefix);
    out.extend_from_slice(data);
    out
}

fn str_op(code: u8, text: &str, proto: u8) -> Result<Vec<u8>> {
    let data = text.as_bytes();
    let short = u8::try_from(data.len()).ok();
    let word = u32::try_from(data.len()).ok();
    let signed = i32::try_from(data.len()).ok();
    let long = u64::try_from(data.len()).unwrap_or(u64::MAX).to_le_bytes();
    Ok(match code {
        ops::SHORT_BINUNICODE | ops::BINUNICODE | ops::BINUNICODE8 => match (code, short, word) {
            (ops::SHORT_BINUNICODE, Some(len), _) => counted(code, &[len], data),
            (ops::SHORT_BINUNICODE | ops::BINUNICODE, _, Some(len)) => {
                counted(ops::BINUNICODE, &len.to_le_bytes(), data)
            }
            _ if proto >= 4 || code == ops::BINUNICODE8 => counted(ops::BINUNICODE8, &long, data),
            _ => return Err(write_error("the text is too long for this pickle protocol")),
        },
        ops::UNICODE => {
            let mut out = vec![ops::UNICODE];
            out.extend_from_slice(&raw_unicode_escape_encode(text));
            out.push(b'\n');
            out
        }
        ops::SHORT_BINSTRING | ops::BINSTRING => match (code, short, signed) {
            (ops::SHORT_BINSTRING, Some(len), _) => counted(code, &[len], data),
            (_, _, Some(len)) => counted(ops::BINSTRING, &len.to_le_bytes(), data),
            _ => return Err(write_error("the text is too long for a Python 2 string")),
        },
        _ => return Err(write_error("an edited text has no text opcode")),
    })
}

/// New opcode bytes for an edited scalar whose opcode is `original` (opcode byte first).
fn scalar_op(original: &[u8], value: &Value, proto: u8) -> Result<Vec<u8>> {
    let code = original.first().copied().unwrap_or(0);
    match value {
        Value::Bool(v) => bool_op(code, *v),
        Value::Int { v, .. } => int_op(code, original, *v, proto),
        Value::Float { v, .. } => float_op(code, *v),
        Value::Str(text) => str_op(code, text, proto),
        _ => Err(write_error("only booleans, numbers and text can be edited")),
    }
}

/// True when two scalar values are the same (floats by their bits).
fn same_scalar(a: &Value, b: &Value) -> Option<bool> {
    Some(match (a, b) {
        (Value::Null, Value::Null) => true,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Int { v: x, .. }, Value::Int { v: y, .. }) => x == y,
        (Value::Float { v: x, .. }, Value::Float { v: y, .. }) => x.to_bits() == y.to_bits(),
        (Value::Str(x), Value::Str(y)) => x == y,
        (Value::Bytes(x), Value::Bytes(y)) => x == y,
        _ => return None,
    })
}

/// Replacement bytes by the offset of the opcode they replace: (end of that opcode, new bytes).
type Replacements = BTreeMap<usize, (usize, Vec<u8>)>;

struct Walk<'a> {
    original: &'a SaveDoc,
    edited: &'a SaveDoc,
    stream: &'a [u8],
    proto: u8,
    forward: HashMap<NodeId, NodeId>,
    backward: HashMap<NodeId, NodeId>,
    replacements: Replacements,
}

fn shape_error() -> Error {
    write_error(
        "the document no longer has the shape of the pickle (only values can change, not containers, keys or types)",
    )
}

impl Walk<'_> {
    fn node(doc: &SaveDoc, id: NodeId) -> Result<&Node> {
        doc.get(id).ok_or_else(shape_error)
    }

    fn edit(&mut self, original: &Node, edited: &Node) -> Result<()> {
        if original.flags.contains(NodeFlags::READ_ONLY) {
            return Err(write_error("a read-only value was changed"));
        }
        let span = original
            .span
            .ok_or_else(|| write_error("an edited value has no opcode in the pickle"))?;
        let start = usize::try_from(span.start).map_err(|_| shape_error())?;
        let end = usize::try_from(span.end).map_err(|_| shape_error())?;
        let bytes = self.stream.get(start..end).ok_or_else(shape_error)?;
        let new = scalar_op(bytes, &edited.value, self.proto)?;
        if self.replacements.insert(start, (end, new)).is_some() {
            return Err(write_error("one opcode was edited twice"));
        }
        Ok(())
    }

    fn run(&mut self) -> Result<()> {
        let mut stack = vec![(self.original.root(), self.edited.root())];
        while let Some((a, b)) = stack.pop() {
            match (self.forward.get(&a), self.backward.get(&b)) {
                (None, None) => {
                    self.forward.insert(a, b);
                    self.backward.insert(b, a);
                }
                (Some(x), Some(y)) if *x == b && *y == a => continue,
                _ => return Err(shape_error()),
            }
            let original = Self::node(self.original, a)?;
            let edited = Self::node(self.edited, b)?;
            if original.type_hint != edited.type_hint {
                return Err(shape_error());
            }
            match (&original.value, &edited.value) {
                (Value::Array(x), Value::Array(y)) => {
                    if x.len() != y.len() {
                        return Err(shape_error());
                    }
                    stack.extend(x.iter().copied().zip(y.iter().copied()));
                }
                (Value::Object(x), Value::Object(y)) => {
                    if x.len() != y.len() {
                        return Err(shape_error());
                    }
                    for ((key_x, child_x), (key_y, child_y)) in x.iter().zip(y) {
                        if key_x != key_y {
                            return Err(shape_error());
                        }
                        stack.push((*child_x, *child_y));
                    }
                }
                (Value::Map(x), Value::Map(y)) => {
                    if x.len() != y.len() {
                        return Err(shape_error());
                    }
                    for ((key_x, value_x), (key_y, value_y)) in x.iter().zip(y) {
                        stack.push((*key_x, *key_y));
                        stack.push((*value_x, *value_y));
                    }
                }
                (Value::Ref(x), Value::Ref(y)) => stack.push((*x, *y)),
                (x, y) => match same_scalar(x, y) {
                    Some(true) => {}
                    Some(false) if !matches!(x, Value::Null | Value::Bytes(_)) => {
                        self.edit(original, edited)?;
                    }
                    _ => return Err(shape_error()),
                },
            }
        }
        Ok(())
    }
}

/// Copies `stream` opcode by opcode, putting in the replacements and new `FRAME` lengths.
fn rebuild(stream: &[u8], replacements: &Replacements) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(stream.len());
    let mut used = 0_usize;
    let mut reader = Reader::new(stream);
    while let Some(op) = reader.next_op()? {
        if op.code == ops::FRAME {
            let old = usize::try_from(ops::le_u64(reader.arg(&op))).map_err(|_| shape_error())?;
            let body_end = op.end.checked_add(old).ok_or_else(shape_error)?;
            let mut length = i128::try_from(old).map_err(|_| shape_error())?;
            for (start, (end, bytes)) in replacements.range(op.end..body_end) {
                let removed = i128::try_from(end.saturating_sub(*start)).unwrap_or(0);
                let added = i128::try_from(bytes.len()).unwrap_or(0);
                length += added - removed;
            }
            let length = u64::try_from(length).map_err(|_| shape_error())?;
            out.push(ops::FRAME);
            out.extend_from_slice(&length.to_le_bytes());
            continue;
        }
        match replacements.get(&op.start) {
            Some((end, bytes)) => {
                if *end != op.end {
                    return Err(shape_error());
                }
                out.extend_from_slice(bytes);
                used += 1;
            }
            None => out.extend_from_slice(stream.get(op.start..op.end).unwrap_or_default()),
        }
    }
    if used != replacements.len() {
        return Err(shape_error());
    }
    Ok(out)
}

/// Writes `doc` back as pickle bytes (see the module documentation).
pub(crate) fn encode(doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
    let stream = params.bytes(STREAM_PARAM)?;
    // The stream decoded once already; decode it again without tighter limits than that.
    let limits = Limits {
        max_input_bytes: u64::MAX,
        max_decoded_bytes: u64::MAX,
        max_depth: MAX_NESTING,
        max_nodes: u64::MAX,
        ..Limits::default()
    };
    let (original, proto) = decode_stream(stream, &limits)?;
    let mut walk = Walk {
        original: &original,
        edited: doc,
        stream,
        proto,
        forward: HashMap::new(),
        backward: HashMap::new(),
        replacements: Replacements::new(),
    };
    walk.run()?;
    if walk.replacements.is_empty() {
        return Ok(stream.to_vec());
    }
    rebuild(stream, &walk.replacements)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_bytes_are_minimal() {
        assert_eq!(long_bytes(0), Vec::<u8>::new());
        assert_eq!(long_bytes(1), vec![1]);
        assert_eq!(long_bytes(127), vec![127]);
        assert_eq!(long_bytes(128), vec![128, 0]);
        assert_eq!(long_bytes(255), vec![255, 0]);
        assert_eq!(long_bytes(-1), vec![255]);
        assert_eq!(long_bytes(-128), vec![128]);
        assert_eq!(long_bytes(-129), vec![127, 255]);
        assert_eq!(long_bytes(1 << 70), vec![0, 0, 0, 0, 0, 0, 0, 0, 64]);
    }

    #[test]
    fn binint_picks_the_smallest_opcode() {
        assert_eq!(binint_op(5, 2), vec![ops::BININT1, 5]);
        assert_eq!(binint_op(300, 2), vec![ops::BININT2, 44, 1]);
        assert_eq!(binint_op(-1, 2), vec![ops::BININT, 255, 255, 255, 255]);
        assert_eq!(binint_op(1 << 40, 2), vec![ops::LONG1, 6, 0, 0, 0, 0, 0, 1]);
        assert_eq!(binint_op(1 << 40, 1), b"I1099511627776\n".to_vec());
    }

    #[test]
    fn strings_grow_into_wider_opcodes() {
        let long = "x".repeat(300);
        let out = str_op(ops::SHORT_BINUNICODE, &long, 4).unwrap();
        assert_eq!(out.first(), Some(&ops::BINUNICODE));
        assert_eq!(out.len(), 305);
        let out = str_op(ops::SHORT_BINSTRING, &long, 2).unwrap();
        assert_eq!(out.first(), Some(&ops::BINSTRING));
        assert_eq!(
            str_op(ops::UNICODE, "a\\b\n\u{e9}\u{20ac}\u{1F600}", 0).unwrap(),
            b"Va\\u005cb\\u000a\xe9\\u20ac\\U0001f600\n".to_vec()
        );
    }
}
