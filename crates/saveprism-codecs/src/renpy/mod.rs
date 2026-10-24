//! `container.renpy-save`: the zip archive of a Ren'Py save (`1-1-LT1.save`).
//!
//! Ren'Py writes a zip (deflate) with the entries `screenshot.png`, `extra_info`, `json`,
//! `renpy_version`, `log` and, since 7.6 / 8.1, `signatures`. `log` is the pickled game state
//! (`(roots, log)`); decoding gives its bytes, so the next step is `format.python-pickle`.
//!
//! The whole archive is kept in the parameter `archive`. Encoding with the same `log` bytes gives
//! the archive back unchanged. A changed `log` is compressed again and written in place; every other
//! entry is copied byte for byte, and CRC, sizes, offsets and the central directory are written
//! anew. When the archive has a `signatures` entry and the caller passes the text of the player's
//! `security_keys.txt` in the parameter `sign_key`, the entry is signed with those keys (see
//! [`sign`] and `renpy/savetoken.py`); otherwise it is written empty. Empty means Ren'Py asks the
//! player whether to trust the edited save; a signature made with a key in the player's
//! `security_keys.txt` loads without that question (parameter `signed` tells whether the original
//! had signatures). Saves from before Ren'Py 7.6 have no entry and are left as they are.
//!
//! Only the `log` entry is ever decompressed (under the output budget); every entry's declared size
//! is checked against that budget. Zip64, encrypted entries and archives with data before the first
//! entry or between the entries and the central directory are refused.

pub mod sign;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    Confidence, DecodeCtx, Error, ErrorCode, Layer, Limits, Params, Registry, Result, SniffCtx,
};

use crate::zip_common::{
    Archive, Change, Entry, LOCAL_SIG, ZipCodes, parse, read_entry, rebuild, u32_at, unsupported,
    zip_error,
};

/// The data is not a readable zip archive (damaged, cut short, CRC mismatch).
pub const RENPY_ZIP: ErrorCode = ErrorCode::new("SP-RENPY-001");
/// A zip feature this layer does not handle (zip64, encryption, unusual layout or method).
pub const RENPY_UNSUPPORTED: ErrorCode = ErrorCode::new("SP-RENPY-002");
/// The archive could not be written again.
pub const RENPY_WRITE: ErrorCode = ErrorCode::new("SP-RENPY-003");
/// The given Ren'Py key file holds no usable signing key.
pub const RENPY_KEY: ErrorCode = ErrorCode::new("SP-RENPY-004");

/// Name of the entry that holds the pickled game state.
pub const LOG_ENTRY: &str = "log";
/// Name of the entry that holds the save's signatures (Ren'Py 7.6 / 8.1 and later).
pub const SIGNATURES_ENTRY: &str = "signatures";
/// Name of the entry with the Ren'Py version.
pub const VERSION_ENTRY: &str = "renpy_version";
/// Parameter: the whole original archive.
pub const ARCHIVE_PARAM: &str = "archive";
/// Parameter: true when the original `signatures` entry was not empty.
pub const SIGNED_PARAM: &str = "signed";
/// Parameter: the text of the player's `security_keys.txt`, whose `signing-key` lines sign an
/// edited save. Set only for the write; the decode recipe never holds it.
pub const SIGN_KEY_PARAM: &str = "sign_key";

const CODES: ZipCodes = ZipCodes {
    damaged: RENPY_ZIP,
    unsupported: RENPY_UNSUPPORTED,
    write: RENPY_WRITE,
};

/// Registers the Ren'Py save layer.
pub fn register(reg: &mut Registry) {
    reg.add_layer(Arc::new(RenpySaveLayer));
}

/// The `container.renpy-save` layer.
#[derive(Clone, Copy, Debug, Default)]
pub struct RenpySaveLayer;

/// Checks the archive is a Ren'Py save and returns its `log` entry.
fn log_entry<'a>(archive: &'a Archive, input: &[u8], ctx: &DecodeCtx<'_>) -> Result<&'a Entry> {
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
    let mut logs = archive
        .entries
        .iter()
        .filter(|entry| entry.is(LOG_ENTRY.as_bytes()));
    let log = logs.next().ok_or_else(|| {
        zip_error(
            &CODES,
            "the archive has no log entry (not a Ren'Py save)",
            0,
        )
    })?;
    if logs.next().is_some() {
        return Err(unsupported(&CODES, "the archive has two log entries", 0));
    }
    Ok(log)
}

impl Layer for RenpySaveLayer {
    fn id(&self) -> &'static str {
        "container.renpy-save"
    }

    /// A zip archive with a `log` entry; a `renpy_version` entry and the `.save` extension raise
    /// the score.
    fn sniff(&self, input: &[u8], ctx: &SniffCtx<'_>) -> Confidence {
        if u32_at(input, 0) != Some(LOCAL_SIG) {
            return Confidence::NONE;
        }
        let Ok(archive) = parse(input, &CODES) else {
            return Confidence::NONE;
        };
        if archive.find(LOG_ENTRY.as_bytes()).is_none() {
            return Confidence::NONE;
        }
        let named = ctx
            .file_name
            .is_some_and(|name| name.to_ascii_lowercase().ends_with(".save"));
        let base = if archive.find(VERSION_ENTRY.as_bytes()).is_some() {
            0.9
        } else {
            0.5
        };
        Confidence::new(if named { base + 0.05 } else { base })
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<(Vec<u8>, Params)> {
        ctx.limits().check_input(len_u64(input.len()))?;
        let archive = parse(input, &CODES)?;
        let log = log_entry(&archive, input, ctx)?;
        let data = read_entry(input, log, ctx, &CODES)?;
        let signed = archive
            .find(SIGNATURES_ENTRY.as_bytes())
            .is_some_and(|entry| entry.size > 0);
        let mut params = Params::new();
        params.set(ARCHIVE_PARAM, input.to_vec());
        params.set(SIGNED_PARAM, signed);
        Ok((data, params))
    }

    fn encode(&self, inner: &[u8], params: &Params) -> Result<Vec<u8>> {
        let input = params.bytes(ARCHIVE_PARAM)?;
        let archive = parse(input, &CODES)?;
        let limits = Limits {
            max_decoded_bytes: u64::MAX,
            max_ratio: u64::MAX,
            ..Limits::default()
        };
        let ctx = DecodeCtx::new(&limits);
        let log = log_entry(&archive, input, &ctx)?;
        let original = read_entry(input, log, &ctx, &CODES)?;
        if original == inner {
            return Ok(input.to_vec());
        }
        let signatures = match (
            archive.find(SIGNATURES_ENTRY.as_bytes()),
            params.opt_str(SIGN_KEY_PARAM)?,
        ) {
            (Some(_), Some(text)) => {
                let keys = sign::parse_key_file(text);
                if keys.is_empty() {
                    return Err(Error::new(
                        RENPY_KEY,
                        "the Ren'Py key file has no usable signing-key line",
                    ));
                }
                sign::sign_data(inner, &keys).into_bytes()
            }
            _ => Vec::new(),
        };
        let changes = [
            Change {
                name: LOG_ENTRY.as_bytes(),
                contents: inner,
            },
            Change {
                name: SIGNATURES_ENTRY.as_bytes(),
                contents: &signatures,
            },
        ];
        rebuild(input, &archive, &changes, &CODES)
    }
}
