//! Read-only reader for The Witcher 3 `POTATO70` content bundles.
//!
//! Everything here is plain data: a bundle is never executed and never written. The layout was
//! confirmed against the public notes and BSD-2-Clause reader of `Systemcluster/w3edit`
//! (`w3edit-formats.md`, `src/bundle.rs`); the code below is written from that description.
//!
//! # Layout
//!
//! | bytes | legacy version 3 | remastered version 5 |
//! | --- | --- | --- |
//! | `0..8` | `POTATO70` | `POTATO70` |
//! | `8..12` / `8..16` | bundle size (u32) | bundle size (u64) |
//! | `16..20` | TOC size (u32) | TOC size (u32) |
//! | `20..22` | version = 3 | version = 5 |
//! | `22..30` | mystery bytes | absolute TOC end (u64) |
//! | `30..32` | mystery bytes | reserved |
//!
//! The TOC starts at `0x20`. A legacy entry is `0x140` bytes (`name[0x100]`, hash u128, empty u32,
//! size u32, compressed size u32, absolute offset u32, date, time, zero u128, CRC32, compression);
//! a remastered entry is `0x130` bytes (`name[0x100]`, hash u128, absolute offset u64, size u32,
//! compressed size u32, CRC32, compression, reserved 8).
//!
//! Compression ids: `0` none, `1` zlib, `2` Snappy raw block, `3` Doboz, `4` LZ4 block, `5` LZ4-HC
//! block. Doboz has no permissively licensed decoder in this workspace, so it is refused with a
//! clear error instead of guessed. All decompression is bounded by the size in the TOC and by the
//! caller's `max_out`; a payload never grows past that.

use std::fmt;
use std::io::{Read, Seek, SeekFrom};

use saveprism_core::{Error, Result};

use super::{W3_BUNDLE, parse_error};

/// Magic at the start of every bundle.
pub const MAGIC: &[u8; 8] = b"POTATO70";
/// Header size in bytes.
const HEADER_SIZE: usize = 0x20;
/// Legacy TOC entry size.
const LEGACY_ENTRY_SIZE: usize = 0x140;
/// Remastered TOC entry size.
const REMASTERED_ENTRY_SIZE: usize = 0x130;
/// Longest name a TOC entry can hold.
const MAX_NAME_BYTES: usize = 0x100;
/// Largest TOC this reader accepts (the game writes far smaller ones).
pub const MAX_TOC_BYTES: u64 = 64 * 1024 * 1024;
/// Most entries a TOC may hold; the byte budget already bounds this, the explicit count only stops
/// an oversized multiplication before the allocation.
pub const MAX_ENTRIES: usize = 1_000_000;

/// The compression id of an entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Compression {
    /// Stored as-is.
    None,
    /// zlib (RFC 1950) stream.
    Zlib,
    /// Raw Snappy block.
    Snappy,
    /// Doboz; refused.
    Doboz,
    /// Raw LZ4 block.
    Lz4,
    /// Raw LZ4 block, high-compression flavour (same decoder as [`Compression::Lz4`]).
    Lz4Hc,
    /// An id this reader does not know.
    Unknown(u32),
}

impl Compression {
    /// Maps a TOC compression id.
    pub fn from_id(id: u32) -> Self {
        match id {
            0 => Self::None,
            1 => Self::Zlib,
            2 => Self::Snappy,
            3 => Self::Doboz,
            4 => Self::Lz4,
            5 => Self::Lz4Hc,
            other => Self::Unknown(other),
        }
    }

    /// The id stored in a TOC.
    pub fn id(self) -> u32 {
        match self {
            Self::None => 0,
            Self::Zlib => 1,
            Self::Snappy => 2,
            Self::Doboz => 3,
            Self::Lz4 => 4,
            Self::Lz4Hc => 5,
            Self::Unknown(id) => id,
        }
    }
}

impl fmt::Display for Compression {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::None => f.write_str("none"),
            Self::Zlib => f.write_str("zlib"),
            Self::Snappy => f.write_str("Snappy"),
            Self::Doboz => f.write_str("Doboz"),
            Self::Lz4 => f.write_str("LZ4"),
            Self::Lz4Hc => f.write_str("LZ4-HC"),
            Self::Unknown(id) => write!(f, "unknown ({id})"),
        }
    }
}

/// One file in a bundle, as the TOC describes it.
#[derive(Clone, Debug)]
pub struct BundleEntry {
    /// Path inside the bundle, with the separator the game stored (usually `\`).
    pub name: String,
    /// Absolute byte offset of the compressed payload in the bundle file.
    pub offset: u64,
    /// Uncompressed size.
    pub size: u32,
    /// Compressed size (the number of bytes stored at `offset`).
    pub compressed_size: u32,
    /// CRC32 of the uncompressed bytes from the TOC (not enforced by this reader).
    pub crc: u32,
    /// Compression of the payload.
    pub compression: Compression,
}

/// The header and table of one bundle.
#[derive(Clone, Debug)]
pub struct BundleToc {
    /// 3 for a legacy bundle, 5 for a remastered one.
    pub version: u16,
    /// TOC size in bytes.
    pub toc_size: u32,
    /// Every entry, in TOC order.
    pub entries: Vec<BundleEntry>,
}

impl BundleToc {
    /// Parses the header and TOC from a seekable file, reading only those bytes.
    ///
    /// `file_len` is taken from the reader itself. Every payload range is checked against it so a
    /// later [`read_entry`] cannot seek outside the file.
    pub fn read<R: Read + Seek>(reader: &mut R) -> Result<Self> {
        let file_len = reader
            .seek(SeekFrom::End(0))
            .map_err(|error| parse_error(format!("cannot measure a bundle: {error}")))?;
        reader
            .seek(SeekFrom::Start(0))
            .map_err(|error| parse_error(format!("cannot rewind a bundle: {error}")))?;
        let mut header = [0_u8; HEADER_SIZE];
        reader.read_exact(&mut header).map_err(|error| {
            parse_error(format!("a bundle header is shorter than 32 bytes: {error}"))
        })?;
        Self::parse_header(&header, file_len, |toc| {
            reader
                .read_exact(toc)
                .map_err(|error| parse_error(format!("a bundle TOC is truncated: {error}")))
        })
    }

    /// Parses a complete header + TOC image (tests and callers that already hold the bytes).
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let header = bytes
            .get(..HEADER_SIZE)
            .ok_or_else(|| parse_error("a bundle header is shorter than 32 bytes"))?;
        Self::parse_header(header, bytes.len() as u64, |toc| {
            let wanted = toc.len();
            let slice = bytes
                .get(HEADER_SIZE..HEADER_SIZE.saturating_add(wanted))
                .ok_or_else(|| parse_error("a bundle TOC is truncated"))?;
            toc.copy_from_slice(slice);
            Ok(())
        })
    }

    fn parse_header(
        header: &[u8],
        file_len: u64,
        read_toc: impl FnOnce(&mut [u8]) -> Result<()>,
    ) -> Result<Self> {
        if header.get(..MAGIC.len()) != Some(MAGIC.as_slice()) {
            return Err(parse_error(
                "a bundle does not start with the POTATO70 magic",
            ));
        }
        let version =
            read_u16(header, 20).ok_or_else(|| parse_error("a bundle header has no version"))?;
        let entry_size = match version {
            3 => LEGACY_ENTRY_SIZE,
            5 => REMASTERED_ENTRY_SIZE,
            other => {
                return Err(Error::new(
                    W3_BUNDLE,
                    format!("bundle version {other} is not supported (only 3 and 5 are)"),
                ));
            }
        };
        let toc_size_u64 = u64::from(
            read_u32(header, 16).ok_or_else(|| parse_error("a bundle header has no TOC size"))?,
        );
        if toc_size_u64 > MAX_TOC_BYTES {
            return Err(Error::new(
                W3_BUNDLE,
                format!(
                    "the bundle TOC is {} bytes, larger than the {MAX_TOC_BYTES} byte budget",
                    toc_size_u64
                ),
            ));
        }
        let toc_size = usize::try_from(toc_size_u64)
            .map_err(|_| parse_error("the bundle TOC size does not fit this platform"))?;
        if !toc_size.is_multiple_of(entry_size) {
            return Err(parse_error(format!(
                "the bundle TOC size {toc_size} is not a multiple of its {entry_size} byte entries"
            )));
        }
        let entries_count = toc_size / entry_size;
        if entries_count > MAX_ENTRIES {
            return Err(parse_error("the bundle TOC holds too many entries"));
        }
        let toc_end = HEADER_SIZE
            .checked_add(toc_size)
            .ok_or_else(|| parse_error("the bundle TOC size overflows"))?;
        if toc_end as u64 > file_len {
            return Err(parse_error("the bundle TOC ends past the end of the file"));
        }
        let mut toc = vec![0_u8; toc_size];
        read_toc(&mut toc)?;

        let mut entries = Vec::with_capacity(entries_count);
        for index in 0..entries_count {
            let start = index
                .checked_mul(entry_size)
                .ok_or_else(|| parse_error("a bundle TOC entry offset overflows"))?;
            let end = start
                .checked_add(entry_size)
                .ok_or_else(|| parse_error("a bundle TOC entry offset overflows"))?;
            let entry = toc
                .get(start..end)
                .ok_or_else(|| parse_error("a bundle TOC entry is truncated"))?;
            let parsed = parse_entry(entry, version)?;
            let payload_end = parsed
                .offset
                .checked_add(u64::from(parsed.compressed_size))
                .ok_or_else(|| parse_error("a bundle payload range overflows"))?;
            if payload_end > file_len {
                return Err(parse_error(format!(
                    "\"{}\" ends at byte {payload_end}, past the file's {file_len} bytes",
                    parsed.name
                )));
            }
            entries.push(parsed);
        }
        Ok(Self {
            version,
            toc_size: u32::try_from(toc_size).unwrap_or(u32::MAX),
            entries,
        })
    }

    /// The first entry whose name ends with `suffix`, comparing `/` and `\` the same way and
    /// ignoring ASCII case.
    pub fn find_end(&self, suffix: &str) -> Option<&BundleEntry> {
        let wanted = normalize(suffix);
        self.entries
            .iter()
            .find(|entry| normalize(&entry.name).ends_with(&wanted))
    }
}

/// Reads and decompresses one entry, at most `max_out` bytes.
pub fn read_entry<R: Read + Seek>(
    reader: &mut R,
    entry: &BundleEntry,
    max_out: usize,
) -> Result<Vec<u8>> {
    let size = entry.size as usize;
    if size > max_out {
        return Err(Error::new(
            W3_BUNDLE,
            format!(
                "\"{}\" is {size} bytes uncompressed, larger than the {max_out} byte budget",
                entry.name
            ),
        ));
    }
    reader
        .seek(SeekFrom::Start(entry.offset))
        .map_err(|error| parse_error(format!("cannot seek to \"{}\": {error}", entry.name)))?;
    let mut payload = vec![0_u8; entry.compressed_size as usize];
    reader
        .read_exact(&mut payload)
        .map_err(|error| parse_error(format!("cannot read \"{}\": {error}", entry.name)))?;
    decode_payload(&payload, entry, max_out)
}

/// Decompresses an entry payload that is already in memory.
pub fn decode_payload(payload: &[u8], entry: &BundleEntry, max_out: usize) -> Result<Vec<u8>> {
    let size = entry.size as usize;
    if size > max_out {
        return Err(Error::new(
            W3_BUNDLE,
            format!(
                "\"{}\" is {size} bytes uncompressed, larger than the {max_out} byte budget",
                entry.name
            ),
        ));
    }
    let out = match entry.compression {
        Compression::None => {
            if payload.len() != size {
                return Err(parse_error(format!(
                    "\"{}\" is stored without compression but its TOC size and payload disagree",
                    entry.name
                )));
            }
            payload.to_vec()
        }
        Compression::Zlib => {
            let mut decoder = flate2::read::ZlibDecoder::new(payload);
            let mut out = Vec::with_capacity(size);
            decoder
                .by_ref()
                .take(size as u64 + 1)
                .read_to_end(&mut out)
                .map_err(|error| {
                    parse_error(format!("cannot inflate \"{}\": {error}", entry.name))
                })?;
            out
        }
        Compression::Snappy => {
            let expected = snap::raw::decompress_len(payload).map_err(|error| {
                parse_error(format!("cannot read Snappy \"{}\": {error}", entry.name))
            })?;
            if expected != size {
                return Err(parse_error(format!(
                    "Snappy \"{}\" says {expected} bytes but the TOC says {size}",
                    entry.name
                )));
            }
            snap::raw::Decoder::new()
                .decompress_vec(payload)
                .map_err(|error| {
                    parse_error(format!(
                        "cannot decompress Snappy \"{}\": {error}",
                        entry.name
                    ))
                })?
        }
        Compression::Lz4 | Compression::Lz4Hc => lz4_flex::block::decompress(payload, size)
            .map_err(|error| {
                parse_error(format!("cannot decompress LZ4 \"{}\": {error}", entry.name))
            })?,
        Compression::Doboz => {
            return Err(Error::new(
                W3_BUNDLE,
                format!(
                    "\"{}\" uses Doboz compression, which SavePrism does not have a decoder for",
                    entry.name
                ),
            ));
        }
        Compression::Unknown(id) => {
            return Err(Error::new(
                W3_BUNDLE,
                format!("\"{}\" uses unknown compression id {id}", entry.name),
            ));
        }
    };
    if out.len() != size {
        return Err(parse_error(format!(
            "\"{}\" decompressed to {} bytes but the TOC says {size}",
            entry.name,
            out.len()
        )));
    }
    Ok(out)
}

fn parse_entry(entry: &[u8], version: u16) -> Result<BundleEntry> {
    let name_bytes = entry
        .get(..MAX_NAME_BYTES)
        .ok_or_else(|| parse_error("a bundle TOC entry has no name field"))?;
    let name_len = name_bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(MAX_NAME_BYTES);
    let name: String = name_bytes
        .get(..name_len)
        .unwrap_or_default()
        .iter()
        .map(|byte| char::from(*byte))
        .collect();
    let (offset, size, compressed_size, crc, compression) = if version == 3 {
        (
            u64::from(
                read_u32(entry, 0x11c)
                    .ok_or_else(|| parse_error("a legacy entry has no offset"))?,
            ),
            read_u32(entry, 0x114).ok_or_else(|| parse_error("a legacy entry has no size"))?,
            read_u32(entry, 0x118)
                .ok_or_else(|| parse_error("a legacy entry has no compressed size"))?,
            read_u32(entry, 0x138).ok_or_else(|| parse_error("a legacy entry has no CRC"))?,
            read_u32(entry, 0x13c)
                .ok_or_else(|| parse_error("a legacy entry has no compression"))?,
        )
    } else {
        (
            read_u64(entry, 0x110)
                .ok_or_else(|| parse_error("a remastered entry has no offset"))?,
            read_u32(entry, 0x118).ok_or_else(|| parse_error("a remastered entry has no size"))?,
            read_u32(entry, 0x11c)
                .ok_or_else(|| parse_error("a remastered entry has no compressed size"))?,
            read_u32(entry, 0x120).ok_or_else(|| parse_error("a remastered entry has no CRC"))?,
            read_u32(entry, 0x124)
                .ok_or_else(|| parse_error("a remastered entry has no compression"))?,
        )
    };
    Ok(BundleEntry {
        name,
        offset,
        size,
        compressed_size,
        crc,
        compression: Compression::from_id(compression),
    })
}

fn normalize(name: &str) -> String {
    name.replace('\\', "/").to_ascii_lowercase()
}

fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    let end = offset.checked_add(2)?;
    Some(u16::from_le_bytes(bytes.get(offset..end)?.try_into().ok()?))
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    let end = offset.checked_add(4)?;
    Some(u32::from_le_bytes(bytes.get(offset..end)?.try_into().ok()?))
}

fn read_u64(bytes: &[u8], offset: usize) -> Option<u64> {
    let end = offset.checked_add(8)?;
    Some(u64::from_le_bytes(bytes.get(offset..end)?.try_into().ok()?))
}
