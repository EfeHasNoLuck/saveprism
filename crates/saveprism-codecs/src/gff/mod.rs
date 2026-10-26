//! `format.gff`: BioWare Aurora GFF V3.2 (Neverwinter Nights, KOTOR, Jade Empire,
//! The Witcher 1).
//!
//! The layout is implemented from BioWare's public "Generic File Format (GFF)" document,
//! distributed with the Neverwinter Nights toolset (mirrored at
//! <https://nwn.wiki/spaces/NWN1/pages/327727/File+Format+Specification+Bioware>, file
//! `Bioware_Aurora_GFF_Format.pdf`). The resource type table used by the ERF layer comes from
//! the same package's "Key and BIF" document. Everything here is an independent implementation;
//! no code or table was copied from a game, a closed-source editor or a GPL reader.
//!
//! # Tree
//!
//! The root is an `Object` (hint `GFF:root:<tag>`) holding the top-level struct's fields in file
//! order. A struct is an `Object` (hint `GFF:struct:<type id>`), a list is an `Array` (hint
//! `GFF:list`) of struct objects. Field labels are the object keys, so editing addresses a value
//! as `/Gold`, `/Inventory/0/Count` and so on.
//!
//! | GFF field type | Node | Hint | Editable |
//! |---|---|---|---|
//! | BYTE, CHAR, WORD, SHORT, DWORD, INT | `Int` | `GFF:byte` ... `GFF:int` | yes |
//! | DWORD64, INT64 | `Int` | `GFF:dword64`, `GFF:int64` | yes |
//! | FLOAT, DOUBLE | `Float` | `GFF:float`, `GFF:double` | yes |
//! | CExoString, ResRef | `Str` | `GFF:cexostring`, `GFF:resref` (`:latin1` when not UTF-8) | same byte length |
//! | CExoLocString | `Object` with `strref` and a read-only `strings` array | `GFF:cexolocstring` | embedded text, same byte length |
//! | VOID | `Bytes` | `GFF:void` | no |
//! | Struct | `Object` | `GFF:struct:<id>` | values only |
//! | List | `Array` | `GFF:list` | values only |
//!
//! A CExoLocString exposes `strref` (read-only `Int`, U32) and `strings`, an array of objects
//! with a read-only `id` (`Int`, U32; language times two plus gender) and an editable `text`.
//!
//! # Exact writing
//!
//! The decoder records the original bytes and a `ByteSpan` on every editable scalar. The writer
//! copies the original file and replaces only spans whose value changed. An unchanged document
//! therefore returns the original bytes exactly, including arrays, labels, offsets and any
//! padding this codec does not interpret. A string edit is refused with [`GFF_WRITE`] when its
//! encoded length differs from the original, because changing it would move every later offset;
//! containers are read-only so their shape never changes.
//!
//! Nothing in the file is executed: strings are decoded as text, lists are walked with a depth
//! limit and a cycle guard, and every offset is checked against the input before it is read.

mod read;
#[cfg(test)]
mod tests;
mod write;

use std::sync::Arc;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    Codec, Confidence, DecodeCtx, Decoded, ErrorCode, Params, Registry, Result, SaveDoc, SniffCtx,
};

/// The file is truncated, its header is inconsistent or a field does not parse.
pub const GFF_PARSE: ErrorCode = ErrorCode::new("SP-GFF-001");
/// A field uses a data type this codec does not read.
pub const GFF_UNSUPPORTED: ErrorCode = ErrorCode::new("SP-GFF-002");
/// An edit cannot be written back in place (wrong type, out of range, different string length).
pub const GFF_WRITE: ErrorCode = ErrorCode::new("SP-GFF-003");

/// Prefix of every hint this codec writes.
pub(crate) const HINT_PREFIX: &str = "GFF:";
/// Parameter holding the original file bytes for the lossless writer.
pub(crate) const P_ORIGINAL: &str = "gff:original";

/// Version this codec reads and writes.
pub(crate) const VERSION: &[u8; 4] = b"V3.2";
/// Size of the fixed GFF header.
pub(crate) const HEADER_LEN: usize = 56;
/// Size of a struct array element.
pub(crate) const STRUCT_LEN: usize = 12;
/// Size of a field array element.
pub(crate) const FIELD_LEN: usize = 12;
/// Size of a label array element.
pub(crate) const LABEL_LEN: usize = 16;

pub(crate) const T_BYTE: u32 = 0;
pub(crate) const T_CHAR: u32 = 1;
pub(crate) const T_WORD: u32 = 2;
pub(crate) const T_SHORT: u32 = 3;
pub(crate) const T_DWORD: u32 = 4;
pub(crate) const T_INT: u32 = 5;
pub(crate) const T_DWORD64: u32 = 6;
pub(crate) const T_INT64: u32 = 7;
pub(crate) const T_FLOAT: u32 = 8;
pub(crate) const T_DOUBLE: u32 = 9;
pub(crate) const T_CEXOSTRING: u32 = 10;
pub(crate) const T_RESREF: u32 = 11;
pub(crate) const T_CEXOLOCSTRING: u32 = 12;
pub(crate) const T_VOID: u32 = 13;
pub(crate) const T_STRUCT: u32 = 14;
pub(crate) const T_LIST: u32 = 15;

/// Registers the GFF V3.2 codec as `format.gff`.
pub fn register(reg: &mut Registry) {
    reg.add_codec(Arc::new(GffCodec));
}

/// The `format.gff` codec.
#[derive(Clone, Copy, Debug, Default)]
pub struct GffCodec;

/// Extensions that normally hold a GFF V3.2 file.
const GFF_EXTENSIONS: &[&str] = &[
    "bic", "ifo", "git", "gic", "are", "uti", "utc", "utd", "utp", "utm", "utw", "utt", "ute",
    "uts", "jrl", "dlg", "fac", "itp", "gui", "dft", "set", "ptm", "ptt", "gff", "caf",
];

fn named_gff(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let extension = lower.rsplit_once('.').map(|(_, extension)| extension);
    extension.is_some_and(|extension| GFF_EXTENSIONS.contains(&extension))
}

fn is_printable_tag(tag: &[u8]) -> bool {
    tag.iter()
        .all(|&byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b' ')
}

fn plausible_header(input: &[u8]) -> bool {
    if input.len() < HEADER_LEN || input.get(4..8) != Some(VERSION) {
        return false;
    }
    let Some(tag) = input.get(..4) else {
        return false;
    };
    if !is_printable_tag(tag) {
        return false;
    }
    let sections = [
        (read::u32_at(input, 8), read::u32_at(input, 12), STRUCT_LEN),
        (read::u32_at(input, 16), read::u32_at(input, 20), FIELD_LEN),
        (read::u32_at(input, 24), read::u32_at(input, 28), LABEL_LEN),
        (read::u32_at(input, 32), read::u32_at(input, 36), 1),
        (read::u32_at(input, 40), read::u32_at(input, 44), 1),
        (read::u32_at(input, 48), read::u32_at(input, 52), 1),
    ];
    let mut ok = true;
    for (offset, count, item) in sections {
        let (Some(offset), Some(count)) = (offset, count) else {
            return false;
        };
        let Some(size) = u64::from(count).checked_mul(item as u64) else {
            return false;
        };
        let Some(end) = u64::from(offset).checked_add(size) else {
            return false;
        };
        if end > u64::try_from(input.len()).unwrap_or(u64::MAX) {
            ok = false;
        }
    }
    let Some(struct_count) = read::u32_at(input, 12) else {
        return false;
    };
    ok && struct_count > 0
}

impl Codec for GffCodec {
    fn id(&self) -> &'static str {
        "format.gff"
    }

    /// `V3.2` plus a printable four-byte tag and six sections that fit the input.
    fn sniff(&self, input: &[u8], ctx: &SniffCtx<'_>) -> Confidence {
        if !plausible_header(input) {
            return Confidence::NONE;
        }
        let named = ctx.file_name.is_some_and(named_gff);
        Confidence::new(if named { 0.98 } else { 0.9 })
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<Decoded> {
        ctx.limits().check_input(len_u64(input.len()))?;
        let shared: &DecodeCtx<'_> = ctx;
        let (doc, params) = read::read_all(input, shared)?;
        ctx.charge_nodes(len_u64(doc.len()))?;
        Ok(Decoded { doc, params })
    }

    fn encode(&self, doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
        write::encode(doc, params)
    }
}
