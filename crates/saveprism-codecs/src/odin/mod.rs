//! `format.odin`: Odin Serializer binary saves (Sirenix Odin Serializer).
//!
//! Written from the public, Apache-2.0 licensed `TeamSirenix/odin-serializer` repository
//! (<https://github.com/TeamSirenix/odin-serializer>): `BinaryEntryType.cs` for the entry codes
//! and `BinaryDataReader.cs` / `BinaryDataWriter.cs` for the payloads, strings, arrays, type
//! tables and references. Everything here is an independent Rust implementation; no C# code was
//! copied. The format is parsed as data only: no type is resolved, no assembly is loaded and
//! nothing in the file is executed.
//!
//! # Layout
//!
//! A stream is a sequence of entries until the end of the file (the writer never writes
//! `EndOfStream`; an explicit `0x31` is accepted at the end). Each entry starts with a
//! [`BinaryEntryType`] byte:
//!
//! - a named form writes a string name before the payload, an unnamed form writes no name;
//! - integers are little endian at their natural width (`sbyte`/`byte` 1, `short`/`ushort` 2,
//!   `int`/`uint` 4, `long`/`ulong` 8), floats 4 or 8, `decimal` 16, `char` a UTF-16 code unit,
//!   `bool` one byte, `guid` 16 bytes;
//! - a string is a `u8` width flag, an `i32` length and that many bytes (flag `0`, one byte per
//!   character) or UTF-16 code units (flag `1`, little endian);
//! - `StartOfArray` (`0x06`) is an `i64` element count, then the elements, then `EndOfArray`
//!   (`0x07`);
//! - `PrimitiveArray` (`0x08`) is an `i32` element count, an `i32` bytes per element and the raw
//!   bytes;
//! - `StartOfReferenceNode` (`0x01`/`0x02`) is a type entry and an `i32` reference id, then the
//!   members, then `EndOfNode` (`0x05`); `StartOfStructNode` (`0x03`/`0x04`) is a type entry (no
//!   id) and the members;
//! - a type entry is `UnnamedNull` (`0x2E`, no type), `TypeID` (`0x30`, `i32` id) or `TypeName`
//!   (`0x2F`, `i32` id and a string name; later nodes refer to it by id);
//! - internal references carry no payload; external references carry an `i32` index, 16 guid
//!   bytes or a string.
//!
//! # Tree
//!
//! The root is an `Object` with the hint `ODIN:stream`, holding every top-level entry in file
//! order. A named entry uses its name as the key; an unnamed one gets `#n` (its position) so it
//! stays visible and its order is kept.
//!
//! | Odin entry | Node | Hint | Editable |
//! |---|---|---|---|
//! | `sbyte` / `byte` / `short` / `ushort` / `int` / `uint` / `long` / `ulong` | `Int` with its width | `ODIN:sbyte`, `ODIN:byte`, ... | yes, in its range |
//! | `float` / `double` | `Float` (`F32` / `F64`) | `ODIN:float`, `ODIN:double` | yes |
//! | `string` | `Str` | `ODIN:string` | yes |
//! | `char` | `Int` (`U16`, the UTF-16 code unit) | `ODIN:char` | yes, 0 to 65,535 |
//! | `bool` | `Bool` | `ODIN:bool` | yes |
//! | `null` | `Null` | `ODIN:null` | no |
//! | `decimal` / `guid` | `Bytes` | `ODIN:decimal`, `ODIN:guid` | no |
//! | array | `Array` | `ODIN:array` | container read-only |
//! | primitive array | `Bytes` (the raw payload) | `ODIN:primitive-array` | no (the element type is not stored) |
//! | reference node / struct node | `Object` | `ODIN:reference`, `ODIN:struct` | container read-only |
//! | internal reference | `Null` (a back-reference by position, no payload) | `ODIN:internal-reference` | no |
//! | external reference by index | `Int` (`I32`) | `ODIN:external-reference-index` | no |
//! | external reference by guid | `Bytes` | `ODIN:external-reference-guid` | no |
//! | external reference by string | `Str` | `ODIN:external-reference-string` | no |
//!
//! A node shows its type metadata as read-only children before its members: `$type` (the .NET
//! type name, or the name a `TypeID` resolves to) and `$typeId` (the id of a `TypeID`); a
//! reference node also has `$id` (its reference id). The type-name table itself is read-only.
//! Containers are read-only, so an edit never changes the shape.
//!
//! # Exact writing
//!
//! The reader records the byte span of every editable payload. The writer copies the original
//! stream and replaces only the values whose nodes changed; entry names, type entries, ids,
//! arrays, references and every other byte are copied unchanged. An unchanged document
//! therefore encodes to the original bytes. A string keeps the width it was written with unless it
//! gains a character that needs UTF-16; an integer must stay in its original width.

mod read;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    Codec, Confidence, DecodeCtx, Decoded, Error, ErrorCode, NodeFlags, NodeId, Params, Registry,
    Result, SaveDoc, SniffCtx, Value,
};

/// The stream is truncated, an entry code is unknown or a length/type entry is malformed.
pub const ODIN_PARSE: ErrorCode = ErrorCode::new("SP-ODIN-001");
/// An editable value cannot be written back in place (wrong type, out of range, bad span).
pub const ODIN_WRITE: ErrorCode = ErrorCode::new("SP-ODIN-002");

/// Prefix of every hint this codec writes.
pub const HINT_PREFIX: &str = "ODIN:";

/// Deepest nesting read or written (keeps recursion bounded).
pub(crate) const MAX_NESTING: usize = 512;

/// Registers the Odin Serializer binary codec as `format.odin`.
pub fn register(reg: &mut Registry) {
    reg.add_codec(Arc::new(OdinCodec));
}

/// The `format.odin` codec.
#[derive(Clone, Copy, Debug, Default)]
pub struct OdinCodec;

impl Codec for OdinCodec {
    fn id(&self) -> &'static str {
        "format.odin"
    }

    /// There is no magic number. The bytes must parse completely as an Odin binary stream, which
    /// is a weak but honest structural signal.
    fn sniff(&self, input: &[u8], _ctx: &SniffCtx<'_>) -> Confidence {
        if input.is_empty() || !read::plausible(input) {
            return Confidence::NONE;
        }
        Confidence::new(0.4)
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<Decoded> {
        ctx.limits().check_input(len_u64(input.len()))?;
        let (doc, params) = read::read_all(input, ctx)?;
        Ok(Decoded { doc, params })
    }

    fn encode(&self, doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
        let original = params.bytes("original")?;
        let mut patches = Vec::new();
        for index in 0..doc.len() {
            let Some(id) = NodeId::from_index(index) else {
                break;
            };
            let Some(node) = doc.get(id) else {
                continue;
            };
            if node.flags.contains(NodeFlags::READ_ONLY) {
                continue;
            }
            let Some(span) = node.span else {
                continue;
            };
            let start = usize::try_from(span.start)
                .map_err(|_| Error::new(ODIN_WRITE, "invalid Odin value span"))?;
            let end = usize::try_from(span.end)
                .map_err(|_| Error::new(ODIN_WRITE, "invalid Odin value span"))?;
            if start > end || end > original.len() {
                return Err(Error::new(ODIN_WRITE, "invalid Odin value span"));
            }
            let replacement = encode_leaf(node, original.get(start..end).unwrap_or_default())?;
            if original.get(start..end) != Some(replacement.as_slice()) {
                patches.push((start, end, replacement));
            }
        }
        patches.sort_by_key(|patch| std::cmp::Reverse(patch.0));
        let mut out = original.to_vec();
        for (start, end, replacement) in patches {
            out.splice(start..end, replacement);
        }
        Ok(out)
    }
}

/// The hint without its `ODIN:` prefix.
fn hint_kind(node: &saveprism_core::Node) -> &str {
    node.type_hint
        .as_ref()
        .and_then(|hint| hint.as_str().strip_prefix(HINT_PREFIX))
        .unwrap_or("")
}

/// Encodes one editable payload back into the bytes that belong at its span.
fn encode_leaf(node: &saveprism_core::Node, original: &[u8]) -> Result<Vec<u8>> {
    let kind = hint_kind(node);
    let out = match (&node.value, kind) {
        (Value::Int { v, .. }, "sbyte") => {
            vec![i8::try_from(*v).map_err(|_| range_error("sbyte"))? as u8]
        }
        (Value::Int { v, .. }, "byte") => {
            vec![u8::try_from(*v).map_err(|_| range_error("byte"))?]
        }
        (Value::Int { v, .. }, "short") => i16::try_from(*v)
            .map_err(|_| range_error("short"))?
            .to_le_bytes()
            .to_vec(),
        (Value::Int { v, .. }, "ushort") => u16::try_from(*v)
            .map_err(|_| range_error("ushort"))?
            .to_le_bytes()
            .to_vec(),
        (Value::Int { v, .. }, "int") => i32::try_from(*v)
            .map_err(|_| range_error("int"))?
            .to_le_bytes()
            .to_vec(),
        (Value::Int { v, .. }, "uint") => u32::try_from(*v)
            .map_err(|_| range_error("uint"))?
            .to_le_bytes()
            .to_vec(),
        (Value::Int { v, .. }, "long") => i64::try_from(*v)
            .map_err(|_| range_error("long"))?
            .to_le_bytes()
            .to_vec(),
        (Value::Int { v, .. }, "ulong") => u64::try_from(*v)
            .map_err(|_| range_error("ulong"))?
            .to_le_bytes()
            .to_vec(),
        (Value::Float { v, .. }, "float") => (*v as f32).to_le_bytes().to_vec(),
        (Value::Float { v, .. }, "double") => v.to_le_bytes().to_vec(),
        (Value::Bool(value), "bool") => vec![u8::from(*value)],
        (Value::Int { v, .. }, "char") => u16::try_from(*v)
            .map_err(|_| range_error("char"))?
            .to_le_bytes()
            .to_vec(),
        (Value::Str(text), "string") => encode_string(text, original.first().copied())?,
        _ => {
            return Err(Error::new(
                ODIN_WRITE,
                "this Odin value is not editable or changed its type",
            ));
        }
    };
    Ok(out)
}

fn range_error(kind: &str) -> Error {
    Error::new(ODIN_WRITE, format!("the value does not fit an Odin {kind}"))
}

/// `u8` width flag + `i32` length + UTF-8 (flag 0) or UTF-16LE code units (flag 1). The original
/// width is kept when the text still fits it, so an unchanged value gives the same bytes.
fn encode_string(text: &str, original_flag: Option<u8>) -> Result<Vec<u8>> {
    let eight_bit = text.chars().all(|ch| u32::from(ch) <= 0xFF) && original_flag != Some(1);
    let mut out = Vec::new();
    if eight_bit {
        out.push(0);
        let length = i32::try_from(text.len())
            .map_err(|_| Error::new(ODIN_WRITE, "the string is too long"))?;
        out.extend_from_slice(&length.to_le_bytes());
        out.extend(text.chars().map(|ch| ch as u8));
    } else {
        out.push(1);
        let units: Vec<u16> = text.encode_utf16().collect();
        let length = i32::try_from(units.len())
            .map_err(|_| Error::new(ODIN_WRITE, "the string is too long"))?;
        out.extend_from_slice(&length.to_le_bytes());
        for unit in units {
            out.extend_from_slice(&unit.to_le_bytes());
        }
    }
    Ok(out)
}
