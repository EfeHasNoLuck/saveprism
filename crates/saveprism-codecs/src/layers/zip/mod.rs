//! `container.zip`: a save packed in a zip archive.
//!
//! Some games keep one data file (and sometimes a few small extras) inside a zip. The layer picks
//! the payload entry and returns its bytes; the rest of the pipeline decodes them.
//!
//! The payload is the only file entry, or, when there are several, the largest file entry whose
//! contents another registered codec or layer recognises. The chosen name is kept in the parameter
//! `entry`, the whole original archive in `archive`.
//!
//! Encoding writes only the chosen entry again (same compression method; its original compressed
//! bytes are kept while its contents are unchanged). CRCs, sizes, local headers and central
//! directory offsets are recomputed; every other entry is copied byte for byte, so an unchanged
//! archive comes back identical.
//!
//! Only stored and deflate entries are handled; zip64 and encryption are refused. Every declared
//! size is checked against the output budget before anything is allocated, and only the chosen
//! entry is decompressed (under that budget).

#[cfg(test)]
mod tests;

use std::sync::Arc;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    Confidence, DecodeCtx, Error, ErrorCode, Layer, Limits, Params, Registry, Result, SniffCtx,
};

use crate::zip_common::{
    Archive, Change, Entry, LOCAL_SIG, ZipCodes, parse, read_entry, rebuild, u32_at,
};

/// The archive is damaged, cut short or has no usable payload.
pub const ZIP_DAMAGED: ErrorCode = ErrorCode::new("SP-ZIP-001");
/// A zip feature this layer does not handle (zip64, encryption, unusual method).
pub const ZIP_UNSUPPORTED: ErrorCode = ErrorCode::new("SP-ZIP-002");
/// The archive could not be written again.
pub const ZIP_WRITE: ErrorCode = ErrorCode::new("SP-ZIP-003");
/// No file entry holds something another codec recognises.
pub const ZIP_NO_PAYLOAD: ErrorCode = ErrorCode::new("SP-ZIP-004");

const CODES: ZipCodes = ZipCodes {
    damaged: ZIP_DAMAGED,
    unsupported: ZIP_UNSUPPORTED,
    write: ZIP_WRITE,
};

/// Parameter: the whole original archive.
pub const ARCHIVE_PARAM: &str = "archive";
/// Parameter: name of the chosen payload entry.
pub const ENTRY_PARAM: &str = "entry";

const RENPY_LOG: &[u8] = b"log";
const RENPY_VERSION: &[u8] = b"renpy_version";

/// Registers the zip container layer.
pub fn register(reg: &mut Registry) {
    reg.add_layer(Arc::new(ZipLayer));
}

/// The `container.zip` layer.
#[derive(Clone, Copy, Debug, Default)]
pub struct ZipLayer;

fn zip_error(message: impl Into<String>, offset: usize) -> Error {
    crate::zip_common::zip_error(&CODES, message, offset)
}

fn no_payload() -> Error {
    Error::new(
        ZIP_NO_PAYLOAD,
        "no file entry holds a format another codec recognises",
    )
}

/// True when `data` is recognised by a registered codec or layer.
fn recognised(data: &[u8], name: &[u8], depth: usize, reg: &Registry) -> bool {
    let sniff_ctx = SniffCtx {
        file_name: std::str::from_utf8(name).ok(),
        depth,
    };
    reg.codecs()
        .filter(|codec| codec.id() != "format.raw")
        .any(|codec| codec.sniff(data, &sniff_ctx) > Confidence::NONE)
        || reg
            .layers()
            .any(|layer| layer.sniff(data, &sniff_ctx) > Confidence::NONE)
}

/// The file entries, largest first (ties keep the archive order).
fn by_size<'a>(entries: &[&'a Entry]) -> Vec<&'a Entry> {
    let mut ordered = entries.to_vec();
    ordered.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.local.cmp(&b.local)));
    ordered
}

/// Checks every declared size against the budget and returns the payload entry.
fn payload<'a>(archive: &'a Archive, input: &[u8], ctx: &DecodeCtx<'_>) -> Result<&'a Entry> {
    let budget = ctx.output_budget(len_u64(input.len()));
    for entry in &archive.entries {
        if len_u64(entry.size) > budget {
            return Err(Error::new(
                saveprism_core::codes::CORE_LIMIT_DECODED,
                format!(
                    "an entry would be {} bytes, the budget is {budget} bytes (possible decompression bomb)",
                    entry.size
                ),
            )
            .with_offset(len_u64(entry.local)));
        }
    }
    let files: Vec<&Entry> = archive
        .entries
        .iter()
        .filter(|entry| !entry.is_directory())
        .collect();
    match files.as_slice() {
        [] => Err(zip_error("the archive has no file entry", 0)),
        [only] => Ok(only),
        _ => {
            let mut reg = Registry::default();
            crate::register_all(&mut reg);
            for entry in by_size(&files) {
                let data = read_entry(input, entry, ctx, &CODES)?;
                if recognised(&data, &entry.name, ctx.depth().saturating_add(1), &reg) {
                    return Ok(entry);
                }
            }
            Err(no_payload())
        }
    }
}

impl Layer for ZipLayer {
    fn id(&self) -> &'static str {
        "container.zip"
    }

    /// A zip archive that is not a Ren'Py save; the usual save extensions raise the score.
    fn sniff(&self, input: &[u8], ctx: &SniffCtx<'_>) -> Confidence {
        if u32_at(input, 0) != Some(LOCAL_SIG) {
            return Confidence::NONE;
        }
        let Ok(archive) = parse(input, &CODES) else {
            return Confidence::NONE;
        };
        if archive.find(RENPY_LOG).is_some() && archive.find(RENPY_VERSION).is_some() {
            return Confidence::NONE;
        }
        if archive.entries.iter().all(|entry| entry.is_directory()) {
            return Confidence::NONE;
        }
        let named = ctx.file_name.is_some_and(|name| {
            let lower = name.to_ascii_lowercase();
            [".zip", ".sav", ".save", ".dat"]
                .iter()
                .any(|extension| lower.ends_with(extension))
        });
        Confidence::new(if named { 0.7 } else { 0.6 })
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<(Vec<u8>, Params)> {
        ctx.limits().check_input(len_u64(input.len()))?;
        let archive = parse(input, &CODES)?;
        let entry = payload(&archive, input, ctx)?;
        let data = read_entry(input, entry, ctx, &CODES)?;
        let mut params = Params::new();
        params.set(ARCHIVE_PARAM, input.to_vec());
        params.set(ENTRY_PARAM, entry.name.clone());
        Ok((data, params))
    }

    fn encode(&self, inner: &[u8], params: &Params) -> Result<Vec<u8>> {
        let input = params.bytes(ARCHIVE_PARAM)?;
        let name = params.bytes(ENTRY_PARAM)?;
        let archive = parse(input, &CODES)?;
        let entry = archive.find(name).ok_or_else(|| {
            Error::new(
                saveprism_core::codes::CORE_INVALID_PARAM,
                "the parameter \"entry\" names no entry of the archive",
            )
        })?;
        let limits = Limits {
            max_decoded_bytes: u64::MAX,
            max_ratio: u64::MAX,
            ..Limits::default()
        };
        let ctx = DecodeCtx::new(&limits);
        let original = read_entry(input, entry, &ctx, &CODES)?;
        if original == inner {
            return Ok(input.to_vec());
        }
        let changes = [Change {
            name,
            contents: inner,
        }];
        rebuild(input, &archive, &changes, &CODES)
    }
}
