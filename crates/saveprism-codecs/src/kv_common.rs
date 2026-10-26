//! Shared helpers for the LevelDB, RocksDB and LiteDB codecs.
//!
//! The LevelDB table/log layout and the RocksDB block trailer are public (LevelDB `doc/table_format.md`
//! and `doc/log_format.md`, BSD; RocksDB `table/format.cc`, Apache-2.0/BSD). This module keeps the
//! varint, CRC32C, block and text helpers in one place; the codecs decide what a block means.

use std::io::Read;

use saveprism_core::limits::len_u64;

/// Block trailer size of both LevelDB and RocksDB block-based tables: one compression byte and a
/// 32-bit checksum.
pub(crate) const BLOCK_TRAILER: usize = 5;

/// A parse failure with the byte offset that caused it.
#[derive(Clone, Debug)]
pub(crate) struct ParseError {
    /// What went wrong.
    pub message: String,
    /// Offset in the input.
    pub offset: usize,
}

impl ParseError {
    /// Creates an error.
    pub fn new(message: impl Into<String>, offset: usize) -> Self {
        Self {
            message: message.into(),
            offset,
        }
    }
}

/// A forward-only reader over untrusted bytes.
pub(crate) struct Reader<'a> {
    input: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    /// Starts at `pos`; `None` when that is past the input.
    pub fn new(input: &'a [u8], pos: usize) -> Option<Self> {
        if pos > input.len() {
            None
        } else {
            Some(Self { input, pos })
        }
    }

    /// Current offset.
    pub fn pos(&self) -> usize {
        self.pos
    }

    /// Takes `count` bytes.
    pub fn take(&mut self, count: usize) -> Result<&'a [u8], ParseError> {
        let start = self.pos;
        let end = start
            .checked_add(count)
            .ok_or_else(|| ParseError::new("the length overflows the input", start))?;
        let slice = self
            .input
            .get(start..end)
            .ok_or_else(|| ParseError::new("the data is cut short", start))?;
        self.pos = end;
        Ok(slice)
    }

    /// One byte.
    pub fn byte(&mut self) -> Result<u8, ParseError> {
        let start = self.pos;
        self.take(1)?
            .first()
            .copied()
            .ok_or_else(|| ParseError::new("the data is cut short", start))
    }

    /// A little-endian `u16`.
    pub fn u16_le(&mut self) -> Result<u16, ParseError> {
        let start = self.pos;
        let bytes: [u8; 2] = self
            .take(2)?
            .try_into()
            .map_err(|_| ParseError::new("the u16 is cut short", start))?;
        Ok(u16::from_le_bytes(bytes))
    }

    /// A little-endian `u32`.
    pub fn u32_le(&mut self) -> Result<u32, ParseError> {
        let start = self.pos;
        let bytes: [u8; 4] = self
            .take(4)?
            .try_into()
            .map_err(|_| ParseError::new("the u32 is cut short", start))?;
        Ok(u32::from_le_bytes(bytes))
    }

    /// A little-endian `u64`.
    pub fn u64_le(&mut self) -> Result<u64, ParseError> {
        let start = self.pos;
        let bytes: [u8; 8] = self
            .take(8)?
            .try_into()
            .map_err(|_| ParseError::new("the u64 is cut short", start))?;
        Ok(u64::from_le_bytes(bytes))
    }

    /// A base-128 varint32 (LevelDB/RocksDB compressing `util/coding.cc`).
    pub fn varint32(&mut self) -> Result<u32, ParseError> {
        let start = self.pos;
        let value = self.varint64()?;
        u32::try_from(value).map_err(|_| ParseError::new("the varint32 is too large", start))
    }

    /// A base-128 varint64; at most ten bytes and no overflow.
    pub fn varint64(&mut self) -> Result<u64, ParseError> {
        let start = self.pos;
        let mut value = 0_u64;
        let mut shift = 0_u32;
        for _ in 0..10 {
            let byte = self.byte()?;
            if shift == 63 && byte > 1 {
                return Err(ParseError::new("the varint64 overflows 64 bits", start));
            }
            value |= u64::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
            shift = shift.saturating_add(7);
        }
        Err(ParseError::new(
            "the varint64 is longer than ten bytes",
            start,
        ))
    }
}

/// CRC32C (Castagnoli), as used by LevelDB and by RocksDB checksums.
pub(crate) fn crc32c(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffff_u32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0x82f6_3b78_u32
            } else {
                crc >> 1
            };
        }
    }
    crc ^ 0xffff_ffff
}

/// LevelDB's checksum mask (`util/crc32c.h`).
pub(crate) fn mask_crc(crc: u32) -> u32 {
    crc.rotate_right(15).wrapping_add(0xa282_ead8)
}

/// Compression type byte in a block trailer (`rocksdb/compression_type.h`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Compression {
    /// `0x00`.
    None,
    /// `0x01`.
    Snappy,
    /// `0x02`.
    Zlib,
    /// `0x03`.
    Bzip2,
    /// `0x04`.
    Lz4,
    /// `0x05`.
    Lz4Hc,
    /// `0x06`.
    Xpress,
    /// `0x07`.
    Zstd,
    /// A custom or future type.
    Other(u8),
}

impl Compression {
    /// Maps a trailer byte.
    pub fn from_byte(byte: u8) -> Self {
        match byte {
            0x00 => Self::None,
            0x01 => Self::Snappy,
            0x02 => Self::Zlib,
            0x03 => Self::Bzip2,
            0x04 => Self::Lz4,
            0x05 => Self::Lz4Hc,
            0x06 => Self::Xpress,
            0x07 => Self::Zstd,
            other => Self::Other(other),
        }
    }

    /// Name for an error message.
    pub fn name(self) -> String {
        match self {
            Self::None => "none".to_string(),
            Self::Snappy => "Snappy".to_string(),
            Self::Zlib => "zlib".to_string(),
            Self::Bzip2 => "bzip2".to_string(),
            Self::Lz4 => "LZ4".to_string(),
            Self::Lz4Hc => "LZ4HC".to_string(),
            Self::Xpress => "XPRESS".to_string(),
            Self::Zstd => "Zstandard".to_string(),
            Self::Other(byte) => format!("type 0x{byte:02x}"),
        }
    }
}

fn limited(mut reader: impl Read, max: u64, what: &str) -> Result<Vec<u8>, ParseError> {
    let mut out = Vec::new();
    reader
        .by_ref()
        .take(max.saturating_add(1))
        .read_to_end(&mut out)
        .map_err(|error| ParseError::new(format!("{what} decompression failed: {error}"), 0))?;
    if len_u64(out.len()) > max {
        return Err(ParseError::new(
            format!("{what} decompressed data is larger than {max} bytes"),
            0,
        ));
    }
    Ok(out)
}

/// Decompresses a block payload; unsupported algorithms fail with a clear message.
pub(crate) fn decompress(
    compression: Compression,
    payload: &[u8],
    max: u64,
) -> Result<Vec<u8>, ParseError> {
    match compression {
        Compression::None => Ok(payload.to_vec()),
        Compression::Snappy => {
            let size = snap::raw::decompress_len(payload)
                .map_err(|_| ParseError::new("the Snappy block has no valid length header", 0))?;
            if len_u64(size) > max {
                return Err(ParseError::new(
                    format!("the Snappy block is larger than {max} bytes"),
                    0,
                ));
            }
            snap::raw::Decoder::new()
                .decompress_vec(payload)
                .map_err(|_| ParseError::new("the Snappy block is corrupt", 0))
        }
        Compression::Zlib => {
            let raw = flate2::read::DeflateDecoder::new(payload);
            match limited(raw, max, "zlib") {
                Ok(data) => Ok(data),
                Err(_) => {
                    let wrapped = flate2::read::ZlibDecoder::new(payload);
                    limited(wrapped, max, "zlib")
                }
            }
        }
        Compression::Bzip2 => {
            let reader = bzip2::read::BzDecoder::new(payload);
            limited(reader, max, "bzip2")
        }
        Compression::Zstd => {
            let reader = zstd::stream::read::Decoder::new(payload)
                .map_err(|_| ParseError::new("the Zstandard block is corrupt", 0))?;
            limited(reader, max, "Zstandard")
        }
        other => Err(ParseError::new(
            format!(
                "the block uses {} compression, which is not supported",
                other.name()
            ),
            0,
        )),
    }
}

/// Which compression numbering a block trailer uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Scheme {
    /// LevelDB: 0 none, 1 Snappy, 2 Zstandard (`include/leveldb/options.h`).
    LevelDb,
    /// RocksDB: `rocksdb/compression_type.h`.
    RocksDb,
}

/// Reads a serialized block at `offset` with `size` payload bytes, checks its CRC32C (when
/// `verify_crc`), and decompresses it.
pub(crate) fn read_block(
    input: &[u8],
    offset: u64,
    size: u64,
    verify_crc: bool,
    scheme: Scheme,
    max: u64,
) -> Result<Vec<u8>, ParseError> {
    let start = usize::try_from(offset)
        .map_err(|_| ParseError::new("the block offset does not fit in memory", 0))?;
    let size = usize::try_from(size)
        .map_err(|_| ParseError::new("the block size does not fit in memory", start))?;
    let end = start
        .checked_add(size)
        .ok_or_else(|| ParseError::new("the block size overflows the file", start))?;
    let trailer_end = end
        .checked_add(BLOCK_TRAILER)
        .ok_or_else(|| ParseError::new("the block trailer overflows the file", start))?;
    let payload = input
        .get(start..end)
        .ok_or_else(|| ParseError::new("the block is cut short", start))?;
    let trailer = input
        .get(end..trailer_end)
        .ok_or_else(|| ParseError::new("the block trailer is cut short", start))?;
    let type_byte = trailer
        .first()
        .copied()
        .ok_or_else(|| ParseError::new("the block trailer is empty", end))?;
    let crc_bytes: [u8; 4] = trailer
        .get(1..5)
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(|| ParseError::new("the block checksum is cut short", end))?;
    if verify_crc {
        let stored = u32::from_le_bytes(crc_bytes);
        let mut checked = payload.to_vec();
        checked.push(type_byte);
        if mask_crc(crc32c(&checked)) != stored {
            return Err(ParseError::new("the block checksum does not match", start));
        }
    }
    let compression = match scheme {
        Scheme::LevelDb => match type_byte {
            0 => Compression::None,
            1 => Compression::Snappy,
            2 => Compression::Zstd,
            other => {
                return Err(ParseError::new(
                    format!("the LevelDB block compression type 0x{other:02x} is not supported"),
                    end,
                ));
            }
        },
        Scheme::RocksDb => Compression::from_byte(type_byte),
    };
    let data = decompress(compression, payload, max)?;
    if len_u64(data.len()) > max {
        return Err(ParseError::new(
            format!("the block is larger than {max} bytes"),
            start,
        ));
    }
    Ok(data)
}

/// The key/value pairs of one serialized block.
pub(crate) type BlockEntries = Vec<(Vec<u8>, Vec<u8>)>;

/// Parses prefix-compressed block entries and their restart array (LevelDB `block_builder.cc`).
pub(crate) fn parse_block_entries(block: &[u8]) -> Result<BlockEntries, ParseError> {
    let trailer = block
        .get(block.len().saturating_sub(4)..)
        .ok_or_else(|| ParseError::new("the block has no restart count", 0))?;
    let restarts_bytes: [u8; 4] = trailer
        .try_into()
        .map_err(|_| ParseError::new("the block has no restart count", 0))?;
    let restarts = u32::from_le_bytes(restarts_bytes);
    let restarts = usize::try_from(restarts)
        .map_err(|_| ParseError::new("the restart count is too large", 0))?;
    let footer = restarts
        .checked_mul(4)
        .and_then(|size| size.checked_add(4))
        .ok_or_else(|| ParseError::new("the restart array overflows", 0))?;
    if footer > block.len() {
        return Err(ParseError::new(
            "the restart array is larger than the block",
            0,
        ));
    }
    let entries_end = block.len() - footer;
    let mut reader =
        Reader::new(block, 0).ok_or_else(|| ParseError::new("the block cannot be read", 0))?;
    let mut previous: Vec<u8> = Vec::new();
    let mut entries = Vec::new();
    while reader.pos() < entries_end {
        let start = reader.pos();
        let shared = usize::try_from(reader.varint32()?)
            .map_err(|_| ParseError::new("the shared key length is too large", start))?;
        let unshared = usize::try_from(reader.varint32()?)
            .map_err(|_| ParseError::new("the key length is too large", start))?;
        let value_len = usize::try_from(reader.varint32()?)
            .map_err(|_| ParseError::new("the value length is too large", start))?;
        if shared > previous.len() {
            return Err(ParseError::new(
                "a key shares more bytes than the previous key",
                start,
            ));
        }
        let unshared_bytes = reader.take(unshared)?;
        let value = reader.take(value_len)?.to_vec();
        if reader.pos() > entries_end {
            return Err(ParseError::new(
                "an entry extends into the restart array",
                start,
            ));
        }
        let mut key = previous
            .get(..shared)
            .ok_or_else(|| ParseError::new("a key shares too many bytes", start))?
            .to_vec();
        key.extend_from_slice(unshared_bytes);
        previous = key.clone();
        entries.push((key, value));
    }
    Ok(entries)
}

/// True when `bytes` is UTF-8 without control characters (tab, newline and carriage return are
/// allowed), so it can be shown as text.
fn is_plain_text(text: &str) -> bool {
    text.chars()
        .all(|c| !c.is_control() || matches!(c, '\t' | '\n' | '\r'))
}

/// Shows bytes as text when they are plain UTF-8, else as lower-case hex with a `0x` prefix.
pub(crate) fn text_or_hex(bytes: &[u8]) -> String {
    if let Ok(text) = std::str::from_utf8(bytes)
        && !text.is_empty()
        && is_plain_text(text)
    {
        return text.to_string();
    }
    let mut out = String::with_capacity(2 + bytes.len() * 2);
    out.push_str("0x");
    for byte in bytes {
        out.push(hex_digit(byte >> 4));
        out.push(hex_digit(byte & 0x0f));
    }
    out
}

fn hex_digit(nibble: u8) -> char {
    match nibble {
        0..=9 => char::from(b'0' + nibble),
        _ => char::from(b'a' + (nibble - 10)),
    }
}

/// Decodes one Chromium Local Storage typed string: `0x00` UTF-16LE, `0x01` Latin-1.
fn chrome_string(bytes: &[u8]) -> Option<String> {
    let (kind, rest) = bytes.split_first()?;
    match kind {
        0 => {
            if !rest.len().is_multiple_of(2) {
                return None;
            }
            encoding_rs::UTF_16LE
                .decode_without_bom_handling_and_without_replacement(rest)
                .map(|text| text.into_owned())
        }
        1 => Some(rest.iter().map(|&byte| char::from(byte)).collect()),
        _ => None,
    }
}

/// The parts of a Chromium Local Storage record: the origin, the script key and the value.
pub(crate) struct ChromeEntry {
    /// The serialized origin.
    pub origin: String,
    /// The script-visible key.
    pub key: String,
    /// The script-visible value.
    pub value: String,
}

/// Decodes Chromium's Local Storage key/value encoding.
///
/// The LevelDB key is `_` + `<serialized storage key>` + `\0` + `<typed script key>`; the value is a
/// typed string. Older profiles omit the type prefix on the script key, which is handled by falling
/// back to plain UTF-8.
pub(crate) fn chrome_entry(key: &[u8], value: &[u8]) -> Option<ChromeEntry> {
    let rest = key.strip_prefix(b"_")?;
    let separator = rest.iter().position(|byte| *byte == 0)?;
    let origin = rest.get(..separator)?;
    let script = rest.get(separator + 1..)?;
    if script.is_empty() {
        return None;
    }
    let key = chrome_string(script).or_else(|| {
        std::str::from_utf8(script)
            .ok()
            .filter(|text| is_plain_text(text))
            .map(str::to_string)
    })?;
    let value = chrome_string(value)?;
    if !is_plain_text(&value) {
        return None;
    }
    Some(ChromeEntry {
        origin: text_or_hex(origin),
        key,
        value,
    })
}

/// Splits a LevelDB/RocksDB internal key into the user key and the value-type byte.
pub(crate) fn internal_key(key: &[u8]) -> (&[u8], Option<u8>) {
    if key.len() < 8 {
        return (key, None);
    }
    let user = key.get(..key.len() - 8).unwrap_or_default();
    let kind = key.get(key.len() - 8).copied();
    (user, kind)
}
