//! `format.nrbf`: .NET Remoting Binary Format data (MS-NRBF), the stream that
//! `BinaryFormatter` writes. Many Unity and .NET games store saves in it (`.dat`, `.sav`,
//! `.save`, `.bin`, no extension); RAGS saves (`.rsv`) use the same record grammar.
//!
//! Written from the public specification ([MS-NRBF]: .NET Remoting: Binary Format Data
//! Structure). The stream is parsed as records only: no assembly is loaded, no CLR type is
//! resolved or instantiated, nothing in the file is executed. Every record that names a class
//! is kept as an object with the class and assembly in its hint.
//!
//! # Tree
//!
//! The root is the object the stream header names as `RootId`; objects may be defined after
//! they are referenced. Every node carries a hint `NRBF:<kind>`:
//!
//! | Record | Node | Hint |
//! |---|---|---|
//! | class record | `Object` of members | `NRBF:class:<Name>@<Library>` (no `@` for a system class) |
//! | `BinaryArray`, `ArraySingle*` | `Array` | `NRBF:array` |
//! | `BinaryObjectString` | `Str` | `NRBF:string` |
//! | `MemberReference` | `Ref` (read-only) | `NRBF:ref` |
//! | `ObjectNull`, `ObjectNullMultiple*` | `Null` (read-only), one per covered member | `NRBF:null` |
//! | primitive value | `Bool`, `Int`, `Float`, `Str` | `NRBF:primitive:<PrimitiveTypeEnum>` |
//!
//! `System.Collections.Generic.List`1` shows `_items` only up to `_size`; the full array stays
//! reachable through a reference to it. Dictionary saves keep their array of key/value objects,
//! so every key and value stays editable where it is.
//!
//! Containers are read-only (no insert, remove or rename), so an edit never changes the record
//! shape. Editable: every fixed-width primitive (keeping its type and range), `Boolean`,
//! `Single`/`Double`, `Char`, inline primitive strings and a `BinaryObjectString` that is not
//! shared through several `MemberReference` records. `Decimal`, `DateTime`, `TimeSpan` and
//! shared strings are read-only.
//!
//! # Exact writing
//!
//! The reader keeps the byte span of every editable value. The writer copies the original
//! stream and replaces only the values whose nodes changed; every other record (metadata,
//! object ids, order, MessageEnd, trailing bytes) is copied unchanged, so an unchanged
//! document is byte-identical and an edit cannot renumber the graph.

mod read;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    Codec, Confidence, DecodeCtx, Decoded, Error, ErrorCode, NodeFlags, NodeId, Params, Registry,
    Result, SaveDoc, SniffCtx, Value,
};

/// The data is truncated, malformed or inconsistent (bad length, bad UTF-8, duplicate id).
pub const NRBF_PARSE: ErrorCode = ErrorCode::new("SP-NRBF-001");
/// A record this codec does not read (MethodCall/MethodReturn, unknown record type, version).
pub const NRBF_UNSUPPORTED: ErrorCode = ErrorCode::new("SP-NRBF-002");
/// The document cannot be written as NRBF data (a value changed its type or is out of range).
pub const NRBF_WRITE: ErrorCode = ErrorCode::new("SP-NRBF-003");
/// A reference names an object, class metadata or library that does not exist.
pub const NRBF_BAD_REF: ErrorCode = ErrorCode::new("SP-NRBF-004");

/// Prefix of every hint this codec writes.
pub const HINT_PREFIX: &str = "NRBF:";

/// Stack of the thread that reads or writes: both recurse once per nesting level.
const STACK_BYTES: usize = 64 * 1024 * 1024;
/// Deepest nesting read or written whatever the limits say (keeps the recursion on that stack).
pub(crate) const MAX_NESTING: usize = 4096;
const EXTENSIONS: [&str; 1] = [".rsv"];

/// Registers the .NET BinaryFormatter codec as `format.nrbf`.
pub fn register(reg: &mut Registry) {
    reg.add_codec(Arc::new(NrbfCodec));
}

/// The `format.nrbf` codec.
#[derive(Clone, Copy, Debug, Default)]
pub struct NrbfCodec;

/// True for a byte that starts the stream header and its version 1.0.
fn is_header(input: &[u8], at: usize) -> bool {
    let Some(header) = input.get(at..at + 17) else {
        return false;
    };
    header.first() == Some(&0)
        && header.get(9..13).is_some_and(|major| major == [1, 0, 0, 0])
        && header
            .get(13..17)
            .is_some_and(|minor| minor == [0, 0, 0, 0])
}

impl Codec for NrbfCodec {
    fn id(&self) -> &'static str {
        "format.nrbf"
    }

    /// Record type 0 with MajorVersion 1 and MinorVersion 0 at offset 0. The `.rsv` extension
    /// (RAGS) raises the score slightly.
    fn sniff(&self, input: &[u8], ctx: &SniffCtx<'_>) -> Confidence {
        if !is_header(input, 0) {
            return Confidence::NONE;
        }
        let named = ctx.file_name.is_some_and(|name| {
            let lower = name.to_ascii_lowercase();
            EXTENSIONS
                .iter()
                .any(|extension| lower.ends_with(extension))
        });
        if named {
            Confidence::new(0.97)
        } else {
            Confidence::HIGH
        }
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<Decoded> {
        ctx.limits().check_input(len_u64(input.len()))?;
        let shared: &DecodeCtx<'_> = ctx;
        let (doc, params) = on_big_stack(|| read::read_all(input, shared))?;
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
                .map_err(|_| Error::new(NRBF_WRITE, "invalid NRBF value span"))?;
            let end = usize::try_from(span.end)
                .map_err(|_| Error::new(NRBF_WRITE, "invalid NRBF value span"))?;
            if start > end || end > original.len() {
                return Err(Error::new(NRBF_WRITE, "invalid NRBF value span"));
            }
            let replacement = encode_leaf(node, end - start)?;
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

fn parse_primitive(hint: &str) -> Option<u8> {
    let rest = hint.strip_prefix(HINT_PREFIX)?;
    rest.strip_prefix("primitive:")?.parse().ok()
}

/// True for hints whose value the writer can put back into the stream.
fn editable_hint(hint: &str) -> bool {
    matches!(
        parse_primitive(hint),
        Some(1 | 2 | 3 | 6 | 7 | 8 | 9 | 10 | 11 | 14 | 15 | 16 | 18)
    ) || hint == format!("{HINT_PREFIX}string")
}

fn write_error(message: impl Into<String>) -> Error {
    Error::new(NRBF_WRITE, message)
}

/// Encodes one node back into the bytes that belong at its span.
fn encode_leaf(node: &saveprism_core::Node, old_len: usize) -> Result<Vec<u8>> {
    let hint = node.type_hint.as_ref().map_or("", |hint| hint.as_str());
    if !editable_hint(hint) {
        return Err(write_error("this NRBF value is not editable"));
    }
    let out = match (&node.value, parse_primitive(hint)) {
        (Value::Bool(value), Some(1)) => vec![u8::from(*value)],
        (Value::Int { v, .. }, Some(2)) => {
            vec![u8::try_from(*v).map_err(|_| write_error("Byte is out of range"))?]
        }
        (Value::Int { v, .. }, Some(7)) => i16::try_from(*v)
            .map_err(|_| write_error("Int16 is out of range"))?
            .to_le_bytes()
            .to_vec(),
        (Value::Int { v, .. }, Some(8)) => i32::try_from(*v)
            .map_err(|_| write_error("Int32 is out of range"))?
            .to_le_bytes()
            .to_vec(),
        (Value::Int { v, .. }, Some(9)) => i64::try_from(*v)
            .map_err(|_| write_error("Int64 is out of range"))?
            .to_le_bytes()
            .to_vec(),
        (Value::Int { v, .. }, Some(10)) => {
            vec![i8::try_from(*v).map_err(|_| write_error("SByte is out of range"))? as u8]
        }
        (Value::Float { v, .. }, Some(11)) => (*v as f32).to_le_bytes().to_vec(),
        (Value::Float { v, .. }, Some(6)) => v.to_le_bytes().to_vec(),
        (Value::Int { v, .. }, Some(14)) => u16::try_from(*v)
            .map_err(|_| write_error("UInt16 is out of range"))?
            .to_le_bytes()
            .to_vec(),
        (Value::Int { v, .. }, Some(15)) => u32::try_from(*v)
            .map_err(|_| write_error("UInt32 is out of range"))?
            .to_le_bytes()
            .to_vec(),
        (Value::Int { v, .. }, Some(16)) => u64::try_from(*v)
            .map_err(|_| write_error("UInt64 is out of range"))?
            .to_le_bytes()
            .to_vec(),
        (Value::Str(text), Some(3)) => {
            let mut chars = text.chars();
            let Some(ch) = chars.next() else {
                return Err(write_error("Char must be exactly one character"));
            };
            if chars.next().is_some() {
                return Err(write_error("Char must be exactly one character"));
            }
            let mut bytes = [0_u8; 4];
            ch.encode_utf8(&mut bytes).as_bytes().to_vec()
        }
        (Value::Str(text), Some(18) | None) => {
            // The span of a string holds the length-prefixed payload, not the record header.
            let mut payload = Vec::new();
            put_string(&mut payload, text)?;
            return Ok(payload);
        }
        _ => return Err(write_error("an editable NRBF value changed its type")),
    };
    if parse_primitive(hint) != Some(3) && out.len() != old_len {
        return Err(write_error("a fixed-width NRBF value changed its size"));
    }
    Ok(out)
}

/// Length-prefixed UTF-8 text: 7-bit length then the bytes.
fn put_string(out: &mut Vec<u8>, value: &str) -> Result<()> {
    let mut len = u32::try_from(value.len()).map_err(|_| write_error("string is too long"))?;
    while len >= 0x80 {
        out.push((len as u8) | 0x80);
        len >>= 7;
    }
    out.push(len as u8);
    out.extend_from_slice(value.as_bytes());
    Ok(())
}

/// Runs `work` on a thread with a large stack, so deep (but allowed) nesting cannot overflow the
/// caller's stack.
fn on_big_stack<T: Send>(work: impl FnOnce() -> Result<T> + Send) -> Result<T> {
    std::thread::scope(|scope| {
        let handle = std::thread::Builder::new()
            .name("nrbf".to_string())
            .stack_size(STACK_BYTES)
            .spawn_scoped(scope, work)
            .map_err(|source| {
                Error::new(NRBF_PARSE, "could not start the NRBF worker thread").with_source(source)
            })?;
        handle.join().unwrap_or_else(|_| {
            Err(Error::new(
                NRBF_PARSE,
                "the NRBF worker stopped unexpectedly",
            ))
        })
    })
}
