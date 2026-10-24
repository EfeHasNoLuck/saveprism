//! `format.lcf`: RPG Maker 2000 / 2003 LCF data, the format of the saves (`SaveNN.lsd`, header
//! `LcfSaveData`) and of the game database (`RPG_RT.ldb`, header `LcfDataBase`).
//!
//! Written from the public description of the format (the open-source liblcf project). The file
//! is parsed to a data tree only; nothing in it is executed.
//!
//! # Format
//!
//! Numbers are BER-like: the `int32` taken as `uint32`, written as 7-bit groups, most significant
//! first, the high bit set on every byte but the last; at most 5 bytes. A file is a BER length and
//! a header string, then top-level chunks. A chunk is a BER id, a BER length and that many bytes.
//! A struct is chunks ended by id 0 (one `00` byte); the top level has no end mark (the reader
//! stops at the end of the file or at id 0). An array of structs is a BER count, then per element
//! a BER id, the element's chunks and its end mark. Fixed-size vectors are little-endian with no
//! count (a separate "size" chunk holds it). Strings are raw bytes (Shift-JIS or Windows-1252).
//!
//! # Tree
//!
//! The root is an `Object` with the hint `LCF:struct:Save` or `LCF:struct:Database`. Every struct
//! is an `Object` keyed by field name (see `types.rs`), or by `0x%02x` for ids not in the table;
//! size chunks are named `<field>_size`. Array elements are `Object`s whose first entry `(id)` is
//! the element id. Hints:
//!
//! | Data | Node | Hint |
//! |---|---|---|
//! | struct, array of structs | `Object`, `Array` (read-only) | `LCF:struct:<Type>`, `LCF:array:<Type>` |
//! | BER integer, enum, reference | `Int` (`I32`) | `LCF:int` |
//! | size of a vector, element id | `Int` (`I32`, read-only) | `LCF:count`, `LCF:int` |
//! | boolean | `Bool` when the data is exactly `00` / `01`, else `Int` | `LCF:bool` |
//! | 1-byte integer | `Int` (`I8` / `U8`) | `LCF:int8`, `LCF:uint8` |
//! | double | `Float` (`F64`) | `LCF:double` |
//! | string | `Str` when printable ASCII, else `Bytes` (read-only) | `LCF:string` |
//! | vector | `Array` (read-only) of `Int` / `Bool` | `LCF:int16[]`, `LCF:int32[]`, `LCF:uint8[]`, `LCF:bool[]` |
//! | unknown chunk, data that does not parse exactly | `Bytes` (read-only) | `LCF:bytes` |
//!
//! Containers are read-only, so an edit never changes the shape. Structs of types with no field
//! table (screen, pictures, map info, ...) keep each chunk as read-only bytes.
//!
//! # Exact writing
//!
//! The writer is canonical and builds each body before its length, so every enclosing length
//! follows an edit. What it cannot know is recorded: an integer stored with leading `0x80` groups
//! keeps its bytes as the lexeme `raw:<hex>` (dropped when the value is edited); a non-canonical
//! header length, chunk id, chunk length, array count or struct end mark is kept in the parameter
//! table `raw_ber` (number of the BER in reader order → bytes) and used while it still holds the
//! same value. A top-level end mark and the bytes after it are kept in `end_mark` and `trailing`.

mod read;
mod types;
mod write;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    Codec, Confidence, DecodeCtx, Decoded, ErrorCode, Params, Registry, Result, SaveDoc, SniffCtx,
};

/// The data is truncated or malformed (bad BER number, chunk longer than the file).
pub const LCF_PARSE: ErrorCode = ErrorCode::new("SP-LCF-001");
/// The header names an LCF file this codec does not read (map, map tree).
pub const LCF_UNSUPPORTED: ErrorCode = ErrorCode::new("SP-LCF-002");
/// The document cannot be written as LCF data (a value changed its type or does not fit).
pub const LCF_WRITE: ErrorCode = ErrorCode::new("SP-LCF-003");

/// Prefix of every hint this codec writes.
pub const HINT_PREFIX: &str = "LCF:";
/// Key of the element id of an array element.
pub const ID_KEY: &str = "(id)";
/// Header of a save.
pub const SAVE_HEADER: &[u8] = b"LcfSaveData";
/// Header of a game database.
pub const DATABASE_HEADER: &[u8] = b"LcfDataBase";

/// Prefix of the lexeme that keeps the original bytes of a non-canonical integer.
pub(crate) const RAW_PREFIX: &str = "raw:";
/// Parameter: non-canonical structural BER numbers by reader order.
pub(crate) const P_RAW_BER: &str = "raw_ber";
/// Parameter: the top-level end mark, when the file has one.
pub(crate) const P_END_MARK: &str = "end_mark";
/// Parameter: bytes after the top-level end mark.
pub(crate) const P_TRAILING: &str = "trailing";
/// Longest BER number.
const MAX_BER: usize = 5;

/// Registers the LCF codec as `format.lcf`.
pub fn register(reg: &mut Registry) {
    reg.add_codec(Arc::new(LcfCodec));
}

/// The `format.lcf` codec.
#[derive(Clone, Copy, Debug, Default)]
pub struct LcfCodec;

/// The kind of a node, read from its hint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind<'a> {
    Struct(&'a str),
    Array(&'a str),
    Int,
    Count,
    Bool,
    Int8,
    UInt8,
    Double,
    Text,
    Bytes,
    Int16s,
    Int32s,
    UInt8s,
    Bools,
}

impl Kind<'_> {
    /// The hint text of this kind.
    pub(crate) fn hint(self) -> String {
        let tail = match self {
            Kind::Struct(name) => return format!("{HINT_PREFIX}struct:{name}"),
            Kind::Array(name) => return format!("{HINT_PREFIX}array:{name}"),
            Kind::Int => "int",
            Kind::Count => "count",
            Kind::Bool => "bool",
            Kind::Int8 => "int8",
            Kind::UInt8 => "uint8",
            Kind::Double => "double",
            Kind::Text => "string",
            Kind::Bytes => "bytes",
            Kind::Int16s => "int16[]",
            Kind::Int32s => "int32[]",
            Kind::UInt8s => "uint8[]",
            Kind::Bools => "bool[]",
        };
        format!("{HINT_PREFIX}{tail}")
    }
}

/// Parses an `LCF:` hint; `None` for anything this codec did not write.
pub(crate) fn parse_hint(hint: &str) -> Option<Kind<'_>> {
    let rest = hint.strip_prefix(HINT_PREFIX)?;
    if let Some(name) = rest.strip_prefix("struct:") {
        return Some(Kind::Struct(name));
    }
    if let Some(name) = rest.strip_prefix("array:") {
        return Some(Kind::Array(name));
    }
    Some(match rest {
        "int" => Kind::Int,
        "count" => Kind::Count,
        "bool" => Kind::Bool,
        "int8" => Kind::Int8,
        "uint8" => Kind::UInt8,
        "double" => Kind::Double,
        "string" => Kind::Text,
        "bytes" => Kind::Bytes,
        "int16[]" => Kind::Int16s,
        "int32[]" => Kind::Int32s,
        "uint8[]" => Kind::UInt8s,
        "bool[]" => Kind::Bools,
        _ => return None,
    })
}

/// Reads one BER number at the start of `data`: the value and its length in bytes. `None` when it
/// is truncated, longer than 5 bytes or larger than `u32`.
pub(crate) fn read_ber(data: &[u8]) -> Option<(u32, usize)> {
    let mut value: u64 = 0;
    for (index, byte) in data.iter().take(MAX_BER).enumerate() {
        value = (value << 7) | u64::from(byte & 0x7F);
        if byte & 0x80 == 0 {
            return u32::try_from(value).ok().map(|value| (value, index + 1));
        }
    }
    None
}

/// Appends the canonical BER form of `value`.
pub(crate) fn push_ber(out: &mut Vec<u8>, value: u32) {
    let mut groups = [0_u8; MAX_BER];
    let mut count = 0;
    let mut rest = value;
    for slot in &mut groups {
        *slot = (rest & 0x7F) as u8;
        count += 1;
        rest >>= 7;
        if rest == 0 {
            break;
        }
    }
    for (index, group) in groups.iter().take(count).enumerate().rev() {
        out.push(if index == 0 { *group } else { group | 0x80 });
    }
}

/// True when `raw` (one whole BER number) is the canonical form: no leading zero group.
pub(crate) fn is_canonical(raw: &[u8]) -> bool {
    raw.first() != Some(&0x80)
}

/// The supported header at the start of `input`: the root type name and where the chunks start.
fn header(input: &[u8]) -> Option<(&'static str, usize)> {
    let (len, used) = read_ber(input)?;
    let end = used.checked_add(usize::try_from(len).ok()?)?;
    match input.get(used..end)? {
        text if text == SAVE_HEADER => Some((types::SAVE, end)),
        text if text == DATABASE_HEADER => Some((types::DATABASE, end)),
        _ => None,
    }
}

impl Codec for LcfCodec {
    fn id(&self) -> &'static str {
        "format.lcf"
    }

    /// The header `LcfSaveData` or `LcfDataBase`.
    fn sniff(&self, input: &[u8], _ctx: &SniffCtx<'_>) -> Confidence {
        if header(input).is_some() {
            Confidence::new(0.99)
        } else {
            Confidence::NONE
        }
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<Decoded> {
        ctx.limits().check_input(len_u64(input.len()))?;
        let (doc, params) = read::decode(input, ctx)?;
        ctx.charge_nodes(len_u64(doc.len()))?;
        Ok(Decoded { doc, params })
    }

    fn encode(&self, doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
        write::encode(doc, params)
    }
}
