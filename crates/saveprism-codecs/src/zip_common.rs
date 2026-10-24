//! Shared reader and writer for the zip containers used by `container.renpy-save` and
//! `container.zip`.
//!
//! Only the classic zip layout is handled: local headers followed by the entries from offset 0, the
//! central directory at the end and no zip64, encryption or multi-disk archives. Every length is
//! checked before it is used, and only one entry is ever decompressed (under the caller's output
//! budget).

use std::io::Write as _;

use flate2::Compression;
use flate2::read::DeflateDecoder;
use flate2::write::DeflateEncoder;
use saveprism_core::limits::len_u64;
use saveprism_core::{DecodeCtx, Error, ErrorCode, Result};

pub(crate) const LOCAL_SIG: u32 = 0x0403_4b50;
pub(crate) const CENTRAL_SIG: u32 = 0x0201_4b50;
pub(crate) const END_SIG: u32 = 0x0605_4b50;
pub(crate) const ZIP64_LOCATOR_SIG: u32 = 0x0706_4b50;
pub(crate) const LOCAL_FIXED: usize = 30;
pub(crate) const CENTRAL_FIXED: usize = 46;
pub(crate) const END_FIXED: usize = 22;
pub(crate) const MAX_COMMENT: usize = 0xFFFF;
pub(crate) const FLAG_ENCRYPTED: u16 = 0x0001;
pub(crate) const FLAG_DESCRIPTOR: u16 = 0x0008;
pub(crate) const METHOD_STORED: u16 = 0;
pub(crate) const METHOD_DEFLATE: u16 = 8;
/// zlib's default level, what Python's `zipfile` uses for `ZIP_DEFLATED`.
pub(crate) const DEFLATE_LEVEL: u32 = 6;

/// Error codes a caller uses for the three kinds of failure of this module.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ZipCodes {
    /// The archive is damaged or cut short.
    pub damaged: ErrorCode,
    /// A zip feature this module does not handle.
    pub unsupported: ErrorCode,
    /// The archive could not be written again.
    pub write: ErrorCode,
}

pub(crate) fn zip_error(codes: &ZipCodes, message: impl Into<String>, offset: usize) -> Error {
    Error::new(codes.damaged, message).with_offset(len_u64(offset))
}

pub(crate) fn unsupported(codes: &ZipCodes, message: impl Into<String>, offset: usize) -> Error {
    Error::new(codes.unsupported, message).with_offset(len_u64(offset))
}

pub(crate) fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    let raw = bytes.get(at..at.checked_add(2)?)?;
    Some(u16::from_le_bytes(<[u8; 2]>::try_from(raw).ok()?))
}

pub(crate) fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    let raw = bytes.get(at..at.checked_add(4)?)?;
    Some(u32::from_le_bytes(<[u8; 4]>::try_from(raw).ok()?))
}

fn to_usize(value: u32) -> usize {
    usize::try_from(value).unwrap_or(usize::MAX)
}

/// One entry of the archive.
#[derive(Clone, Debug)]
pub(crate) struct Entry {
    pub(crate) name: Vec<u8>,
    /// Its central directory record.
    pub(crate) central: (usize, usize),
    pub(crate) flags: u16,
    pub(crate) method: u16,
    pub(crate) crc: u32,
    pub(crate) compressed: usize,
    pub(crate) size: usize,
    /// Offset of its local header.
    pub(crate) local: usize,
    /// Start of its compressed data.
    pub(crate) data: usize,
    /// Where the next entry (or the central directory) begins.
    pub(crate) span_end: usize,
}

impl Entry {
    pub(crate) fn is(&self, name: &[u8]) -> bool {
        self.name == name
    }

    pub(crate) fn is_directory(&self) -> bool {
        self.name.last() == Some(&b'/')
    }
}

/// The parsed layout of an archive.
pub(crate) struct Archive {
    pub(crate) entries: Vec<Entry>,
    /// Offset of the end of central directory record.
    pub(crate) end: usize,
}

impl Archive {
    pub(crate) fn find(&self, name: &[u8]) -> Option<&Entry> {
        self.entries.iter().find(|entry| entry.is(name))
    }
}

/// The end of central directory record: its offset, or an error.
fn find_end(input: &[u8], codes: &ZipCodes) -> Result<usize> {
    let last = input
        .len()
        .checked_sub(END_FIXED)
        .ok_or_else(|| zip_error(codes, "the archive is too short", 0))?;
    let first = last.saturating_sub(MAX_COMMENT);
    for at in (first..=last).rev() {
        if u32_at(input, at) == Some(END_SIG)
            && u16_at(input, at + 20).map(usize::from) == Some(input.len() - at - END_FIXED)
        {
            return Ok(at);
        }
    }
    Err(zip_error(
        codes,
        "the archive has no end of central directory",
        last,
    ))
}

pub(crate) fn parse(input: &[u8], codes: &ZipCodes) -> Result<Archive> {
    let end = find_end(input, codes)?;
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
        return Err(unsupported(codes, "zip64 archives are not supported", end));
    }
    if disk != 0 || central_disk != 0 || here != total {
        return Err(unsupported(
            codes,
            "multi-disk archives are not supported",
            end,
        ));
    }
    let central_start = to_usize(central_start);
    if central_start.checked_add(to_usize(central_size)) != Some(end) {
        return Err(unsupported(
            codes,
            "the central directory does not end where the end record starts",
            end,
        ));
    }

    let mut entries: Vec<Entry> = Vec::with_capacity(usize::from(total).min(4096));
    let mut cursor = central_start;
    for _ in 0..total {
        let at = cursor;
        if u32_at(input, at) != Some(CENTRAL_SIG) {
            return Err(zip_error(
                codes,
                "a central directory record is damaged",
                at,
            ));
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
            return Err(unsupported(codes, "zip64 entries are not supported", at));
        }
        if field16(34) != 0 {
            return Err(unsupported(
                codes,
                "multi-disk archives are not supported",
                at,
            ));
        }
        if flags & FLAG_ENCRYPTED != 0 {
            return Err(unsupported(
                codes,
                "encrypted entries are not supported",
                at,
            ));
        }
        let record_end = at + CENTRAL_FIXED + name_len + extra_len + comment_len;
        if record_end > end {
            return Err(zip_error(
                codes,
                "a central directory record is cut short",
                at,
            ));
        }
        let name = input
            .get(at + CENTRAL_FIXED..at + CENTRAL_FIXED + name_len)
            .ok_or_else(|| zip_error(codes, "a central directory record is cut short", at))?
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
            codes,
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
                codes,
                "the entries are not stored one after another from the start",
                local,
            ));
        }
        previous = Some(local);
        if u32_at(input, local) != Some(LOCAL_SIG) {
            return Err(zip_error(codes, "a local header is damaged", local));
        }
        let name_len = usize::from(u16_at(input, local + 26).unwrap_or(0));
        let extra_len = usize::from(u16_at(input, local + 28).unwrap_or(0));
        let data = local + LOCAL_FIXED + name_len + extra_len;
        if input.get(local + LOCAL_FIXED..local + LOCAL_FIXED + name_len)
            != Some(entry.name.as_slice())
        {
            return Err(zip_error(
                codes,
                "a local header names another entry than the central directory",
                local,
            ));
        }
        let span_end = starts.get(index + 1).copied().unwrap_or(central_start);
        if data
            .checked_add(entry.compressed)
            .is_none_or(|data_end| data_end > span_end)
        {
            return Err(zip_error(
                codes,
                "an entry's data runs past its place",
                local,
            ));
        }
        entry.data = data;
        entry.span_end = span_end;
    }
    Ok(Archive { entries, end })
}

/// The uncompressed bytes of `entry`, read under the output budget of `ctx`.
pub(crate) fn read_entry(
    input: &[u8],
    entry: &Entry,
    ctx: &DecodeCtx<'_>,
    codes: &ZipCodes,
) -> Result<Vec<u8>> {
    let raw = input
        .get(entry.data..entry.data + entry.compressed)
        .ok_or_else(|| zip_error(codes, "an entry is cut short", entry.data))?;
    let data = match entry.method {
        METHOD_STORED => raw.to_vec(),
        METHOD_DEFLATE => ctx
            .read_bounded(
                DeflateDecoder::new(raw),
                len_u64(input.len()),
                codes.damaged,
            )
            .map_err(|error| error.with_offset(len_u64(entry.data)))?,
        other => {
            return Err(unsupported(
                codes,
                format!("compression method {other} is not supported"),
                entry.local,
            ));
        }
    };
    if data.len() != entry.size {
        return Err(zip_error(
            codes,
            "an entry's size does not match the central directory",
            entry.data,
        ));
    }
    let mut crc = flate2::Crc::new();
    crc.update(&data);
    if crc.sum() != entry.crc {
        return Err(zip_error(
            codes,
            "an entry's CRC-32 does not match",
            entry.data,
        ));
    }
    Ok(data)
}

fn deflate(data: &[u8], codes: &ZipCodes) -> Result<Vec<u8>> {
    let failed = |source: std::io::Error| {
        Error::new(codes.write, "compressing an entry failed").with_source(source)
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

fn fit_u32(value: usize, codes: &ZipCodes) -> Result<u32> {
    u32::try_from(value)
        .map_err(|_| Error::new(codes.write, "the archive would be larger than 4 GiB"))
}

/// New contents for one entry.
pub(crate) struct Change<'a> {
    /// Name of the entry to replace.
    pub name: &'a [u8],
    /// Its new uncompressed bytes.
    pub contents: &'a [u8],
}

/// The new header fields of a rewritten entry.
struct Rewritten {
    flags: u16,
    crc: u32,
    compressed: u32,
    size: u32,
}

/// Writes `archive` again with new contents for the entries named in `changes`. Every other entry
/// is copied byte for byte; CRCs, sizes, local header and central directory offsets are recomputed.
pub(crate) fn rebuild(
    input: &[u8],
    archive: &Archive,
    changes: &[Change<'_>],
    codes: &ZipCodes,
) -> Result<Vec<u8>> {
    let added = changes.iter().fold(0_usize, |sum, change| {
        sum.saturating_add(change.contents.len() / 4)
    });
    let mut out = Vec::with_capacity(input.len().saturating_add(added));
    let mut offsets = Vec::with_capacity(archive.entries.len());
    let mut rewritten: Vec<Option<Rewritten>> = Vec::with_capacity(archive.entries.len());
    for entry in &archive.entries {
        offsets.push(fit_u32(out.len(), codes)?);
        let contents = changes
            .iter()
            .find(|change| entry.is(change.name))
            .map(|change| change.contents);
        let Some(contents) = contents else {
            out.extend_from_slice(input.get(entry.local..entry.span_end).unwrap_or_default());
            rewritten.push(None);
            continue;
        };
        let data = match entry.method {
            METHOD_DEFLATE => deflate(contents, codes)?,
            METHOD_STORED => contents.to_vec(),
            other => {
                return Err(Error::new(
                    codes.write,
                    format!("compression method {other} cannot be written"),
                ));
            }
        };
        let mut crc = flate2::Crc::new();
        crc.update(contents);
        let fields = Rewritten {
            flags: entry.flags & !FLAG_DESCRIPTOR,
            crc: crc.sum(),
            compressed: fit_u32(data.len(), codes)?,
            size: fit_u32(contents.len(), codes)?,
        };
        let mut header = input
            .get(entry.local..entry.data)
            .ok_or_else(|| Error::new(codes.write, "a local header is missing"))?
            .to_vec();
        put_u16(&mut header, 6, fields.flags);
        put_u32(&mut header, 14, fields.crc);
        put_u32(&mut header, 18, fields.compressed);
        put_u32(&mut header, 22, fields.size);
        out.extend_from_slice(&header);
        out.extend_from_slice(&data);
        rewritten.push(Some(fields));
    }
    let central_start = fit_u32(out.len(), codes)?;
    for ((entry, offset), fields) in archive.entries.iter().zip(&offsets).zip(&rewritten) {
        let mut record = input
            .get(entry.central.0..entry.central.1)
            .ok_or_else(|| Error::new(codes.write, "a central directory record is missing"))?
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
    let central_size = fit_u32(out.len(), codes)?.saturating_sub(central_start);
    let mut end = input.get(archive.end..).unwrap_or_default().to_vec();
    put_u32(&mut end, 12, central_size);
    put_u32(&mut end, 16, central_start);
    out.extend_from_slice(&end);
    Ok(out)
}
