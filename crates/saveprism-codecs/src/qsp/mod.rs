//! `format.qsp`: Quest Soft Player (QSP) save files (`.sav`).
//!
//! Written from the public save/load implementation of the open-source QSP core
//! (`QSPFoundation/qsp`, files `game.c`, `coding.c`, `coding.h`, `variant.c`, `text.h`; Mozilla
//! Public License 2.0). Only behavior was read: the format is parsed to a data tree, nothing in it
//! is executed. The file is a CRLF-separated text save:
//!
//! - line 1 is the raw (not shifted) ASCII signature `QSPSAVEDGAME`;
//! - line 2 is the raw engine version (`5.9.6`, ...);
//! - every following field is *encoded*: each non-zero UTF-16 code unit (or byte, for the legacy
//!   single-byte files) has 5 subtracted, with `5` written as `-5` (`0xFFFB`) so it cannot become
//!   the CR/LF of the separator. Decoding adds 5 with the reverse special case;
//! - after the header: checksum, play time, selected action/object, view path, input, main
//!   description, variables window text, location, window state, timer interval, playlist,
//!   included files, actions (each with its code lines), objects, object groups, then the global
//!   variables. A variable is a name, a list of values and a list of index keys; a value is an
//!   untagged `UNDEF`, a tuple of values, a number or a string (the type code is written first).
//!
//! Modern QSP (5.x) writes UTF-16LE; legacy ANSI builds write a single-byte text (Windows-1251 on
//! Windows, KOI8-R elsewhere). Both encodings are read here; the Windows-1251 code page is used as
//! the single-byte text, the common case for old saves. The last line is empty, i.e. the file ends
//! with the separator.
//!
//! # Tree
//!
//! The root is an `Object` with the hint `QSP:save`. It holds, in file order:
//!
//! | Key | Node |
//! |---|---|
//! | `header`, `version` | `Str` (read-only, written raw) |
//! | `checksum`, `playtime_ms`, `selected_action`, `selected_object`, `windows_state`, `timer_interval` | `Int` |
//! | `view_path`, `input`, `description`, `vars_description`, `location` | `Str` |
//! | `playlist`, `includes` | `Array` (read-only) of `Str` |
//! | `actions` | `Array` (read-only) of `Object` `QSP:action` |
//! | `objects` | `Array` (read-only) of `Object` `QSP:object` |
//! | `object_groups` | `Array` (read-only) of `Object` `QSP:object_group` |
//! | `variables` | `Array` (read-only) of `Object` `QSP:variable` |
//!
//! An action holds `description`, `image`, `lines` (an `Array` of `QSP:line` objects with `text`
//! and `line_number`), `location` and `action_index`. An object holds `name` and `image`; a group
//! holds `name`, `description`, `image`, `updated_fields` and `objects_count`. A variable holds
//! `name` (read-only), `values` (an `Array` of values) and `keys` (an `Array` of `QSP:key`
//! objects with a read-only `index` and a value). A value carries a hint `QSP:undef`, `QSP:tuple`,
//! `QSP:num`, `QSP:bool`, `QSP:str`, `QSP:code` or `QSP:varref`; the last three are strings.
//!
//! Containers are read-only, so an edit never changes the shape. Editable leaves are integers and
//! strings (and numbers/strings inside tuples). `UNDEF` values, the signature, the version and a
//! variable's name are read-only.
//!
//! # Exact writing
//!
//! Every field is written in file order and every string is encoded as QSP encodes it, so an
//! unchanged document gives the original bytes. What the canonical writer cannot know is
//! recorded: an integer line whose text is not QSP's canonical decimal (`+007`, an empty line,
//! leading spaces) keeps its text as the node's lexeme (dropped when the value is edited), and a
//! non-canonical value type line is kept in the parameter `variant_types` by variant ordinal.

mod read;
mod write;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    Codec, Confidence, DecodeCtx, Decoded, ErrorCode, Params, Registry, Result, SaveDoc, SniffCtx,
};

/// The data is truncated, has a bad header or a field does not parse (bad number, invalid UTF-16,
/// wrong count).
pub const QSP_PARSE: ErrorCode = ErrorCode::new("SP-QSP-001");
/// A field uses a value type this codec does not read.
pub const QSP_UNSUPPORTED: ErrorCode = ErrorCode::new("SP-QSP-002");
/// The document cannot be written as a QSP save (a value changed its type, a string is not
/// representable in the file's single-byte code page, a number does not fit).
pub const QSP_WRITE: ErrorCode = ErrorCode::new("SP-QSP-003");

/// Prefix of every hint this codec writes.
pub const HINT_PREFIX: &str = "QSP:";

/// The raw signature of a save (not character-shifted).
pub const SIGNATURE: &str = "QSPSAVEDGAME";
/// Signature of a UTF-16LE save: the ASCII bytes with a NUL after each, then the CRLF separator.
pub(crate) const UTF16_MAGIC: [u8; 28] = [
    0x51, 0x00, 0x53, 0x00, 0x50, 0x00, 0x53, 0x00, 0x41, 0x00, 0x56, 0x00, 0x45, 0x00, 0x44, 0x00,
    0x47, 0x00, 0x41, 0x00, 0x4D, 0x00, 0x45, 0x00, 0x0D, 0x00, 0x0A, 0x00,
];
/// Signature followed by the CRLF separator of a single-byte save.
pub(crate) const ANSI_MAGIC: &[u8] = b"QSPSAVEDGAME\r\n";
/// The CRLF separator, written between every field.
pub(crate) const SEPARATOR: [u8; 2] = [0x0D, 0x0A];
/// The character shift QSP applies to every non-zero encoded character (`QSP_CODREMOV`).
pub(crate) const SHIFT: u16 = 5;
/// Deepest nesting of tuple values read or written.
pub(crate) const MAX_NESTING: usize = 64;

/// Parameter: text encoding, `"utf16le"` or `"ansi"`.
pub(crate) const P_ENCODING: &str = "encoding";
/// Parameter: non-canonical value type lines, by variant ordinal (in file order).
pub(crate) const P_VARIANT_TYPES: &str = "variant_types";

/// Registers the QSP codec as `format.qsp`.
pub fn register(reg: &mut Registry) {
    reg.add_codec(Arc::new(QspCodec));
}

/// The `format.qsp` codec.
#[derive(Clone, Copy, Debug, Default)]
pub struct QspCodec;

/// The text encoding of a save.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Encoding {
    /// UTF-16 little endian (modern QSP).
    Utf16Le,
    /// One byte per character (legacy QSP). The code page is detected per file; Windows-1251 and
    /// KOI8-R are the two the QSP builds use.
    Ansi(crate::text_encoding::TextEncoding),
}

impl Encoding {
    fn name(self) -> &'static str {
        match self {
            Encoding::Utf16Le => "utf16le",
            Encoding::Ansi(crate::text_encoding::TextEncoding::Koi8R) => "ansi-koi8r",
            Encoding::Ansi(_) => "ansi",
        }
    }

    fn from_name(name: &str) -> Option<Self> {
        match name {
            "utf16le" => Some(Encoding::Utf16Le),
            "ansi" => Some(Encoding::Ansi(
                crate::text_encoding::TextEncoding::Windows1251,
            )),
            "ansi-koi8r" => Some(Encoding::Ansi(crate::text_encoding::TextEncoding::Koi8R)),
            _ => None,
        }
    }
}

impl Codec for QspCodec {
    fn id(&self) -> &'static str {
        "format.qsp"
    }

    /// The exact signature and separator, in UTF-16LE or single-byte form.
    fn sniff(&self, input: &[u8], ctx: &SniffCtx<'_>) -> Confidence {
        let magic = input.starts_with(&UTF16_MAGIC) || input.starts_with(ANSI_MAGIC);
        if !magic {
            return Confidence::NONE;
        }
        let named = ctx
            .file_name
            .is_some_and(|name| name.to_ascii_lowercase().ends_with(".sav"));
        Confidence::new(if named { 0.96 } else { 0.9 })
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
