//! `container.renpy-save`: the zip archive of a Ren'Py save (`1-1-LT1.save`).
//!
//! Ren'Py writes a zip (deflate) with the entries `screenshot.png`, `extra_info`, `json`,
//! `renpy_version`, `log` and, since 7.6 / 8.1, `signatures`. `log` is the pickled game state
//! (`(roots, log)`); decoding gives its bytes, so the next step is `format.python-pickle`.
//!
//! The whole archive is kept in the parameter `archive`. Encoding with the same `log` bytes gives
//! the archive back unchanged. A changed `log` is compressed again and written in place; every other
//! entry is copied byte for byte, and CRC, sizes, offsets and the central directory are written
//! anew. The `signatures` entry is then written empty (same name, same place): Ren'Py refuses a
//! save whose signature does not match, but loads an unsigned one after asking the player whether
//! to trust it (parameter `signed` tells whether the original had signatures).
//!
//! Only the `log` entry is ever decompressed (under the output budget); every entry's declared size
//! is checked against that budget. Zip64, encrypted entries and archives with data before the first
//! entry or between the entries and the central directory are refused.

#[cfg(test)]
mod tests;

use std::io::Write as _;
use std::sync::Arc;

use flate2::Compression;
use flate2::read::DeflateDecoder;
use flate2::write::DeflateEncoder;
use saveprism_core::limits::len_u64;
use saveprism_core::{
    Confidence, DecodeCtx, Error, ErrorCode, Layer, Limits, Params, Registry, Result, SniffCtx,
};

/// The data is not a readable zip archive (damaged, cut short, CRC mismatch).
pub const RENPY_ZIP: ErrorCode = ErrorCode::new("SP-RENPY-001");
/// A zip feature this layer does not handle (zip64, encryption, unusual layout or method).
pub const RENPY_UNSUPPORTED: ErrorCode = ErrorCode::new("SP-RENPY-002");
/// The archive could not be written again.
pub const RENPY_WRITE: ErrorCode = ErrorCode::new("SP-RENPY-003");

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

const LOCAL_SIG: u32 = 0x0403_4b50;
const CENTRAL_SIG: u32 = 0x0201_4b50;
const END_SIG: u32 = 0x0605_4b50;
const ZIP64_LOCATOR_SIG: u32 = 0x0706_4b50;
const LOCAL_FIXED: usize = 30;
const CENTRAL_FIXED: usize = 46;
const END_FIXED: usize = 22;
const MAX_COMMENT: usize = 0xFFFF;
const FLAG_ENCRYPTED: u16 = 0x0001;
const FLAG_DESCRIPTOR: u16 = 0x0008;
const METHOD_STORED: u16 = 0;
const METHOD_DEFLATE: u16 = 8;
/// zlib's default level, what Python's `zipfile` uses for `ZIP_DEFLATED`.
const DEFLATE_LEVEL: u32 = 6;

/// Registers the Ren'Py save layer.
pub fn register(reg: &mut Registry) {
    reg.add_layer(Arc::new(RenpySaveLayer));
}

/// The `container.renpy-save` layer.
#[derive(Clone, Copy, Debug, Default)]
pub struct RenpySaveLayer;

fn zip_error(message: impl Into<String>, offset: usize) -> Error {
    Error::new(RENPY_ZIP, message).with_offset(len_u64(offset))
}

fn unsupported(message: impl Into<String>, offset: usize) -> Error {
    Error::new(RENPY_UNSUPPORTED, message).with_offset(len_u64(offset))
}

fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    let raw = bytes.get(at..at.checked_add(2)?)?;
    Some(u16::from_le_bytes(<[u8; 2]>::try_from(raw).ok()?))
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    let raw = bytes.get(at..at.checked_add(4)?)?;
    Some(u32::from_le_bytes(<[u8; 4]>::try_from(raw).ok()?))
}

fn to_usize(value: u32) -> usize {
    usize::try_from(value).unwrap_or(usize::MAX)
}

/// One entry of the archive.
#[derive(Clone, Debug)]
struct Entry {
    name: Vec<u8>,
    /// Its central directory record.
    central: (usize, usize),
    flags: u16,
    method: u16,
    crc: u32,
    compressed: usize,
    size: usize,
    /// Offset of its local header.
    local: usize,
    /// Start of its compressed data.
    data: usize,
    /// Where the next entry (or the central directory) begins.
    span_end: usize,
}

impl Entry {
    fn is(&self, name: &str) -> bool {
        self.name == name.as_bytes()
    }
}

/// The parsed layout of an archive.
struct Archive {
    entries: Vec<Entry>,
    /// Offset of the end of central directory record.
    end: usize,
}

impl Archive {
    fn find(&self, name: &str) -> Option<&Entry> {
        self.entries.iter().find(|entry| entry.is(name))
    }
}

/// The end of central directory record: its offset, or an error.
fn find_end(input: &[u8]) -> Result<usize> {
    let last = input
        .len()
        .checked_sub(END_FIXED)
        .ok_or_else(|| zip_error("the archive is too short", 0))?;
    let first = last.saturating_sub(MAX_COMMENT);
    for at in (first..=last).rev() {
        if u32_at(input, at) == Some(END_SIG)
            && u16_at(input, at + 20).map(usize::from) == Some(input.len() - at - END_FIXED)
        {
            return Ok(at);
        }
    }
    Err(zip_error(
        "the archive has no end of central directory",
        last,
    ))
}

fn parse(input: &[u8]) -> Result<Archive> {
    let end = find_end(input)?;
    let field16 = |offset: usize| u16_at(input, end + offset).unwrap_or(u16::MAX);
    let field32 = |offset: usize| u32_at(input, end + offset).unwrap_or(u32::MAX);
    let (disk, central_disk, here, total) = (field16(4), field16(6), field16(8), field16(10));
    let (central_size, central_start) = (field32(12), field32(16));
    if here == u16::MAX
        || total == u16::MAX
        || central_size == u32::MAX
        || central_start == u32::MAX
        || (end >= 20 && u32_at(input, end - 20) == Some(ZIP64_LOCATOR_SIG))
    {
        return Err(unsupported("zip64 archives are not supported", end));
    }
    if disk != 0 || central_disk != 0 || here != total {
        return Err(unsupported("multi-disk archives are not supported", end));
    }
    let central_start = to_usize(central_start);
    if central_start.checked_add(to_usize(central_size)) != Some(end) {
        return Err(unsupported(
            "the central directory does not end where the end record starts",
            end,
        ));
    }

    let mut entries: Vec<Entry> = Vec::with_capacity(usize::from(total).min(4096));
    let mut cursor = central_start;
    for _ in 0..total {
        let at = cursor;
        if u32_at(input, at) != Some(CENTRAL_SIG) {
            return Err(zip_error("a central directory record is damaged", at));
        }
        let field16 = |offset: usize| u16_at(input, at + offset).unwrap_or(0);
        let field32 = |offset: usize| u32_at(input, at + offset).unwrap_or(u32::MAX);
        let flags = field16(8);
        let method = field16(10);
        let (name_len, extra_len, comment_len) = (
            usize::from(field16(28)),
            usize::from(field16(30)),
            usize::from(field16(32)),
        );
        let (crc, compressed, size, local) = (field32(16), field32(20), field32(24), field32(42));
        if compressed == u32::MAX || size == u32::MAX || local == u32::MAX {
            return Err(unsupported("zip64 entries are not supported", at));
        }
        if field16(34) != 0 {
            return Err(unsupported("multi-disk archives are not supported", at));
        }
        if flags & FLAG_ENCRYPTED != 0 {
            return Err(unsupported("encrypted entries are not supported", at));
        }
        let record_end = at + CENTRAL_FIXED + name_len + extra_len + comment_len;
        if record_end > end {
            return Err(zip_error("a central directory record is cut short", at));
        }
        let name = input
            .get(at + CENTRAL_FIXED..at + CENTRAL_FIXED + name_len)
            .ok_or_else(|| zip_error("a central directory record is cut short", at))?
            .to_vec();
        entries.push(Entry {
            name,
            central: (at, record_end),
            flags,
            method,
            crc,
            compressed: to_usize(compressed),
            size: to_usize(size),
            local: to_usize(local),
            data: 0,
            span_end: 0,
        });
        cursor = record_end;
    }
    if cursor != end {
        return Err(unsupported(
            "the central directory has data after its records",
            cursor,
        ));
    }

    // Entries must follow each other from offset 0 in central directory order.
    let starts: Vec<usize> = entries.iter().map(|entry| entry.local).collect();
    let mut previous: Option<usize> = None;
    for (index, entry) in entries.iter_mut().enumerate() {
        let local = entry.local;
        let expected_first = previous.is_none() && local != 0;
        if expected_first || previous.is_some_and(|before| local <= before) {
            return Err(unsupported(
                "the entries are not stored one after another from the start",
                local,
            ));
        }
        previous = Some(local);
        if u32_at(input, local) != Some(LOCAL_SIG) {
            return Err(zip_error("a local header is damaged", local));
        }
        let name_len = usize::from(u16_at(input, local + 26).unwrap_or(0));
        let extra_len = usize::from(u16_at(input, local + 28).unwrap_or(0));
        let data = local + LOCAL_FIXED + name_len + extra_len;
        if input.get(local + LOCAL_FIXED..local + LOCAL_FIXED + name_len)
            != Some(entry.name.as_slice())
        {
            return Err(zip_error(
                "a local header names another entry than the central directory",
                local,
            ));
        }
        let span_end = starts.get(index + 1).copied().unwrap_or(central_start);
        if data
            .checked_add(entry.compressed)
            .is_none_or(|data_end| data_end > span_end)
        {
            return Err(zip_error("an entry's data runs past its place", local));
        }
        entry.data = data;
        entry.span_end = span_end;
    }
    Ok(Archive { entries, end })
}

/// The uncompressed bytes of `entry`, read under the output budget of `ctx`.
fn read_entry(input: &[u8], entry: &Entry, ctx: &DecodeCtx<'_>) -> Result<Vec<u8>> {
    let raw = input
        .get(entry.data..entry.data + entry.compressed)
        .ok_or_else(|| zip_error("an entry is cut short", entry.data))?;
    let data = match entry.method {
        METHOD_STORED => raw.to_vec(),
        METHOD_DEFLATE => ctx
            .read_bounded(DeflateDecoder::new(raw), len_u64(input.len()), RENPY_ZIP)
            .map_err(|error| error.with_offset(len_u64(entry.data)))?,
        other => {
            return Err(unsupported(
                format!("compression method {other} is not supported"),
                entry.local,
            ));
        }
    };
    if data.len() != entry.size {
        return Err(zip_error(
            "an entry's size does not match the central directory",
            entry.data,
        ));
    }
    let mut crc = flate2::Crc::new();
    crc.update(&data);
    if crc.sum() != entry.crc {
        return Err(zip_error("an entry's CRC-32 does not match", entry.data));
    }
    Ok(data)
}

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
    let mut logs = archive.entries.iter().filter(|entry| entry.is(LOG_ENTRY));
    let log = logs
        .next()
        .ok_or_else(|| zip_error("the archive has no log entry (not a Ren'Py save)", 0))?;
    if logs.next().is_some() {
        return Err(unsupported("the archive has two log entries", 0));
    }
    Ok(log)
}

fn deflate(data: &[u8]) -> Result<Vec<u8>> {
    let failed = |source: std::io::Error| {
        Error::new(RENPY_WRITE, "compressing the log failed").with_source(source)
    };
    let mut encoder = DeflateEncoder::new(Vec::new(), Compression::new(DEFLATE_LEVEL));
    encoder.write_all(data).map_err(failed)?;
    encoder.finish().map_err(failed)
}

fn put_u16(bytes: &mut [u8], at: usize, value: u16) {
    if let Some(slot) = bytes.get_mut(at..at + 2) {
        slot.copy_from_slice(&value.to_le_bytes());
    }
}

fn put_u32(bytes: &mut [u8], at: usize, value: u32) {
    if let Some(slot) = bytes.get_mut(at..at + 4) {
        slot.copy_from_slice(&value.to_le_bytes());
    }
}

fn fit_u32(value: usize) -> Result<u32> {
    u32::try_from(value)
        .map_err(|_| Error::new(RENPY_WRITE, "the archive would be larger than 4 GiB"))
}

/// The new data and header fields of a rewritten entry.
struct Rewritten {
    flags: u16,
    crc: u32,
    compressed: u32,
    size: u32,
}

/// Writes `archive` again with new contents for the `log` entry and an empty `signatures` entry.
fn rebuild(input: &[u8], archive: &Archive, log: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(input.len() + log.len() / 4);
    let mut offsets = Vec::with_capacity(archive.entries.len());
    let mut rewritten: Vec<Option<Rewritten>> = Vec::with_capacity(archive.entries.len());
    for entry in &archive.entries {
        offsets.push(fit_u32(out.len())?);
        let contents: Option<&[u8]> = if entry.is(LOG_ENTRY) {
            Some(log)
        } else if entry.is(SIGNATURES_ENTRY) {
            Some(&[])
        } else {
            None
        };
        let Some(contents) = contents else {
            out.extend_from_slice(input.get(entry.local..entry.span_end).unwrap_or_default());
            rewritten.push(None);
            continue;
        };
        let data = match entry.method {
            METHOD_DEFLATE => deflate(contents)?,
            METHOD_STORED => contents.to_vec(),
            other => {
                return Err(Error::new(
                    RENPY_WRITE,
                    format!("compression method {other} cannot be written"),
                ));
            }
        };
        let mut crc = flate2::Crc::new();
        crc.update(contents);
        let fields = Rewritten {
            flags: entry.flags & !FLAG_DESCRIPTOR,
            crc: crc.sum(),
            compressed: fit_u32(data.len())?,
            size: fit_u32(contents.len())?,
        };
        let mut header = input
            .get(entry.local..entry.data)
            .ok_or_else(|| Error::new(RENPY_WRITE, "a local header is missing"))?
            .to_vec();
        put_u16(&mut header, 6, fields.flags);
        put_u32(&mut header, 14, fields.crc);
        put_u32(&mut header, 18, fields.compressed);
        put_u32(&mut header, 22, fields.size);
        out.extend_from_slice(&header);
        out.extend_from_slice(&data);
        rewritten.push(Some(fields));
    }
    let central_start = fit_u32(out.len())?;
    for ((entry, offset), fields) in archive.entries.iter().zip(&offsets).zip(&rewritten) {
        let mut record = input
            .get(entry.central.0..entry.central.1)
            .ok_or_else(|| Error::new(RENPY_WRITE, "a central directory record is missing"))?
            .to_vec();
        put_u32(&mut record, 42, *offset);
        if let Some(fields) = fields {
            put_u16(&mut record, 8, fields.flags);
            put_u32(&mut record, 16, fields.crc);
            put_u32(&mut record, 20, fields.compressed);
            put_u32(&mut record, 24, fields.size);
        }
        out.extend_from_slice(&record);
    }
    let central_size = fit_u32(out.len())?.saturating_sub(central_start);
    let mut end = input.get(archive.end..).unwrap_or_default().to_vec();
    put_u32(&mut end, 12, central_size);
    put_u32(&mut end, 16, central_start);
    out.extend_from_slice(&end);
    Ok(out)
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
        let Ok(archive) = parse(input) else {
            return Confidence::NONE;
        };
        if archive.find(LOG_ENTRY).is_none() {
            return Confidence::NONE;
        }
        let named = ctx
            .file_name
            .is_some_and(|name| name.to_ascii_lowercase().ends_with(".save"));
        let base = if archive.find(VERSION_ENTRY).is_some() {
            0.9
        } else {
            0.5
        };
        Confidence::new(if named { base + 0.05 } else { base })
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<(Vec<u8>, Params)> {
        ctx.limits().check_input(len_u64(input.len()))?;
        let archive = parse(input)?;
        let log = log_entry(&archive, input, ctx)?;
        let data = read_entry(input, log, ctx)?;
        let signed = archive
            .find(SIGNATURES_ENTRY)
            .is_some_and(|entry| entry.size > 0);
        let mut params = Params::new();
        params.set(ARCHIVE_PARAM, input.to_vec());
        params.set(SIGNED_PARAM, signed);
        Ok((data, params))
    }

    fn encode(&self, inner: &[u8], params: &Params) -> Result<Vec<u8>> {
        let input = params.bytes(ARCHIVE_PARAM)?;
        let archive = parse(input)?;
        let limits = Limits {
            max_decoded_bytes: u64::MAX,
            max_ratio: u64::MAX,
            ..Limits::default()
        };
        let ctx = DecodeCtx::new(&limits);
        let log = log_entry(&archive, input, &ctx)?;
        let original = read_entry(input, log, &ctx)?;
        if original == inner {
            return Ok(input.to_vec());
        }
        rebuild(input, &archive, inner)
    }
}
