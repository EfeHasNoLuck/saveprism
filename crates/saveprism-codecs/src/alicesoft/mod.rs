//! `format.alicesoft`: AliceSoft System 3.x and System 4 save files.
//!
//! Layout sources (behaviour only; no code, table or constant copied):
//!
//! - The CC0 Kaitai Struct description of System 3.x `.asd` saves,
//!   <https://gist.github.com/kichikuou/e80b96fdc8184166c02c674ade907f4f> (magic, fixed header,
//!   string-variable block, variable pages).
//! - `nunuhara/libsys4` (GPL-2.0), `include/system4/savefile.h` and `src/savefile.c`, read for the
//!   System 4 envelope (`GD\x01\x01` + uncompressed size + zlib), the GSave section offsets and the
//!   `ain_data_type` values.
//! - `kichikuou/xsystem35-sdl2` (GPL-2.0), `src/savedata.c`, read for the System 3.x signature
//!   strings and the header field order.
//!
//! The implementation below is independent; it was written from those behavioural notes.
//!
//! # System 3.x `.asd`
//!
//! A 4096-byte little-endian header starts with a 32-byte signature (`System3.5 SavaData(c)ALICE-SOFT`,
//! `System3.5 SaveData(c)ALICE-SOFT` or `System3.8 SaveData(c)ALICE-SOFT`), then a version
//! (`0x350200`), a 28-byte game name, text attributes, 128 selection and 128 message window
//! rectangles, the absolute offset of the stack block, the absolute offset of the string-variable
//! block, and 256 absolute offsets of variable pages. Each block is a small header followed by its
//! data:
//!
//! - stack: `size` (u32) + 12 reserved bytes + `size` bytes;
//! - strings: `size` (u32) + `count` (u32) + `maxlen` (u32) + 4 reserved bytes + `count`
//!   NUL-terminated Shift-JIS strings;
//! - variable page: `size` in bytes (u32) + page number (u32) + 8 reserved bytes + `size / 2`
//!   little-endian u16 values.
//!
//! Numbers and Shift-JIS strings are editable. A string must keep the byte length it already has so
//! the absolute block offsets never move. The stack and unknown header bytes stay read-only.
//!
//! # System 4 `.asd` / `.as2` / `.asdbak`
//!
//! The outer envelope is `GD\x01\x01`, the uncompressed size (u32 little-endian) and a zlib stream.
//! An encrypted save has `0x1a` as the first compressed byte; it is detected and kept read-only
//! (the decryption key is not public). The decompressed body is a GSave (global variables) or an
//! RSave (resume state, `RSM\0`).
//!
//! A GSave (versions 4, 5 and 7) has a key and five counted sections addressed by absolute offsets:
//! records, globals, strings, arrays and key-values. Each global is `type` (u32), `value` (u32) and
//! a NUL-terminated name (plus a reserved word in versions 4 and 5). Integers, booleans, floats and
//! strings are editable; struct and array references stay read-only. A float is stored as its IEEE
//! bits. A string global's `value` is an index into the string table.
//!
//! An unchanged file writes back byte for byte. After an edit the System 4 body is written again
//! and compressed; the game only reads the decompressed body, so the compressed bytes need not
//! match the original.
//!
//! # Tree
//!
//! System 3.x: an `Object` `ASD:save` with `magic`, `version`, `game_name`, `stack` (read-only
//! bytes), `strings` (array of `ASD:string`) and `variables` (array of `ASD:page` objects holding
//! `ASD:number` values). System 4: an `Object` `ASD:system4` with `key`, `version`, `group`,
//! `globals` and `strings`; an encrypted or unknown body is a read-only `raw` byte node.

pub(crate) mod read;
#[cfg(test)]
mod tests;
mod write;

use std::sync::Arc;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    Codec, Confidence, DecodeCtx, Decoded, Error, ErrorCode, Params, Registry, Result, SaveDoc,
    SniffCtx,
};

use crate::text_encoding::TextEncoding;

/// A System 3.x or System 4 save does not parse.
pub const ASD_PARSE: ErrorCode = ErrorCode::new("SP-ASD-001");
/// A value cannot be written back (range, length or span).
pub const ASD_WRITE: ErrorCode = ErrorCode::new("SP-ASD-002");
/// The System 4 body is encrypted; the key is not public.
pub const ASD_ENCRYPTED: ErrorCode = ErrorCode::new("SP-ASD-003");

/// Prefix of every hint this codec writes.
pub const HINT_PREFIX: &str = "ASD:";
/// Codec parameter carrying the original bytes for the lossless writer.
pub const P_ORIGINAL: &str = "original";
/// Codec parameter carrying the decompressed System 4 body.
pub const P_RAW: &str = "raw";

/// System 3.x signatures (each occupies the 32-byte header field).
pub const SYS35_SIGNATURES: [&[u8]; 2] = [
    b"System3.5 SavaData(c)ALICE-SOFT",
    b"System3.5 SaveData(c)ALICE-SOFT",
];
/// System 3.8 signature.
pub const SYS38_SIGNATURE: &[u8] = b"System3.8 SaveData(c)ALICE-SOFT";
/// The data version written by System 3.5 - 3.9 (`0x350200`).
pub const SYS3_VERSION: u32 = 0x0035_0200;
/// Fixed size of the System 3.x header.
pub const SYS3_HEADER: usize = 4096;
/// First byte of a System 4 envelope.
pub const SYS4_MAGIC: &[u8] = b"GD\x01\x01";
/// Marker of a resume save inside a System 4 envelope.
pub const RSAVE_MAGIC: &[u8] = b"RSM\0";
/// First compressed byte of an encrypted System 4 save.
pub const SYS4_ENCRYPTED_MARK: u8 = 0x1a;
/// Marker of an Artemis save envelope.
pub const ARTEMIS_MAGICS: [&[u8]; 2] = [b"BOWS", b"BOWG"];

/// Registers the AliceSoft codec.
pub fn register(reg: &mut Registry) {
    reg.add_codec(Arc::new(AliceSoftCodec));
}

/// The `format.alicesoft` codec.
#[derive(Clone, Copy, Debug, Default)]
pub struct AliceSoftCodec;

/// True when `input` starts with one of the System 3.x signatures and has the right version.
pub fn is_system3(input: &[u8]) -> bool {
    let head = input.get(..SYS3_HEADER);
    let Some(head) = head else {
        return false;
    };
    let Some(signature) = head.get(..32) else {
        return false;
    };
    let signed = SYS35_SIGNATURES
        .iter()
        .chain(std::iter::once(&SYS38_SIGNATURE))
        .any(|known| {
            let mut expected = [0_u8; 32];
            for (slot, byte) in expected.iter_mut().zip(known.iter().copied()) {
                *slot = byte;
            }
            signature == expected
        });
    signed && read::u32_at(head, 32) == Some(SYS3_VERSION)
}

/// True for a System 4 envelope (`GD\x01\x01` and an 8-byte header).
pub fn is_system4(input: &[u8]) -> bool {
    input.starts_with(SYS4_MAGIC) && input.len() >= 8
}

/// True for the Artemis outer envelope (`BOWS` / `BOWG` + zlib body).
pub fn is_artemis(input: &[u8]) -> bool {
    ARTEMIS_MAGICS.iter().any(|magic| input.starts_with(magic))
        && input.get(12).is_some_and(|byte| byte & 0x0f == 8)
}

fn bad(message: impl Into<String>, offset: usize) -> Error {
    Error::new(ASD_PARSE, message).with_offset(len_u64(offset))
}

impl Codec for AliceSoftCodec {
    fn id(&self) -> &'static str {
        "format.alicesoft"
    }

    fn sniff(&self, input: &[u8], ctx: &SniffCtx<'_>) -> Confidence {
        if is_system3(input) {
            return Confidence::CERTAIN;
        }
        if is_system4(input) {
            return Confidence::new(0.98);
        }
        let _ = ctx;
        Confidence::NONE
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<Decoded> {
        ctx.limits().check_input(len_u64(input.len()))?;
        if is_system3(input) {
            let (doc, params) = read::system3(input, ctx)?;
            return Ok(Decoded { doc, params });
        }
        if is_system4(input) {
            let (doc, params) = read::system4(input, ctx)?;
            return Ok(Decoded { doc, params });
        }
        Err(bad("not an AliceSoft System 3.x / System 4 save", 0))
    }

    fn encode(&self, doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
        write::encode(doc, params)
    }
}

/// Decodes a fixed-format NUL-padded header string.
pub(crate) fn header_text(bytes: &[u8]) -> Option<String> {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    let text = bytes.get(..end)?;
    if text.is_empty() {
        return Some(String::new());
    }
    TextEncoding::ShiftJis
        .decode(text)
        .or_else(|| std::str::from_utf8(text).ok().map(str::to_owned))
}

/// Decodes a Shift-JIS byte run, keeping the bytes when they are not valid text.
pub(crate) fn shift_jis_text(bytes: &[u8]) -> Option<String> {
    TextEncoding::ShiftJis.decode(bytes)
}

/// Encodes text as Shift-JIS.
pub(crate) fn shift_jis_bytes(text: &str) -> Result<Vec<u8>> {
    TextEncoding::ShiftJis.encode(text)
}
