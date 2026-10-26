//! `container.mpq`: Blizzard MoPaQ archives (`.mpq`, `.SC2Save`, `.w3z`, `.sav`).
//!
//! # Format
//!
//! The header signature is `MPQ\x1A` (a `MPQ\x1B` user-data header may point to it), followed by
//! the header size, archive size, format version, sector-size shift, hash/block table offsets and
//! their entry counts. Version 2 adds the high table offsets; versions 3 and 4 extend the header.
//! The hash table (16-byte entries) and block table (16-byte entries) are encrypted with the
//! public Storm crypt table and the fixed keys for `(hash table)` and `(block table)`. File names
//! come from the archive's `(listfile)` entry; file keys are derived from their names (part of the
//! format, not a secret).
//!
//! The layout was verified against the MIT-licensed StormLib sources
//! (<https://github.com/ladislav-zezula/StormLib>, `src/StormLib.h`, `src/SBaseCommon.cpp`,
//! `src/SFileReadFile.cpp`) and the public MPQ format articles they document. No code was copied;
//! only the documented constants and the crypt schedule are used.
//!
//! # What the container returns
//!
//! One entry, chosen by the `entry` hint or the largest entry whose bytes another codec or layer
//! recognises. Zlib and bzip2 sectors are decompressed; PKWARE implode, LZMA, Huffman and sector
//! CRCs are not. Entries that cannot be decompressed are returned as their stored bytes with a
//! note in the `note` parameter.
//!
//! # Exact writing and read-only cases
//!
//! An unchanged payload is returned from the saved original bytes. An edited uncompressed,
//! unencrypted entry may shrink or keep its size; its block-table sizes are updated and the table
//! is encrypted again, while every other byte stays identical. Compressed or encrypted entries
//! are read-only: decompression or re-encryption would change bytes outside the chosen entry.

use std::sync::Arc;
use std::sync::OnceLock;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    Confidence, DecodeCtx, Error, ErrorCode, Layer, Params, Registry, Result, SniffCtx, codes,
};

use super::{inner_is_known, keep_source, source_if_unchanged};

/// The archive is damaged, cut short or its tables do not fit.
pub const MPQ_DAMAGED: ErrorCode = ErrorCode::new("SP-MPQ-001");
/// No entry holds something another codec recognises.
pub const MPQ_NO_PAYLOAD: ErrorCode = ErrorCode::new("SP-MPQ-002");
/// The edited entry cannot be written back into the archive.
pub const MPQ_WRITE: ErrorCode = ErrorCode::new("SP-MPQ-003");
/// An entry uses a compression SavePrism cannot read.
pub const MPQ_UNSUPPORTED: ErrorCode = ErrorCode::new("SP-MPQ-004");

/// Parameter: name of the chosen entry.
pub const ENTRY_PARAM: &str = "entry";
/// Parameter: zero-based index of the chosen entry in the block table.
pub const ENTRY_INDEX_PARAM: &str = "entry_index";
/// Parameter: why the chosen entry is read-only, when it is.
pub const NOTE_PARAM: &str = "note";

const MPQ_MAGIC: &[u8; 4] = b"MPQ\x1A";
const MPQ_USERDATA: &[u8; 4] = b"MPQ\x1B";
const HASH_ENTRY: usize = 16;
const BLOCK_ENTRY: usize = 16;
const HASH_TABLE_KEY: u32 = 0xC3AF_3770;
const BLOCK_TABLE_KEY: u32 = 0xEC83_B3A3;
const HASH_FILE_KEY: u32 = 0x300;

const MPQ_FILE_IMPLODE: u32 = 0x0000_0100;
const MPQ_FILE_ENCRYPTED: u32 = 0x0001_0000;
const MPQ_FILE_KEY_V2: u32 = 0x0002_0000;
const MPQ_FILE_PATCH_FILE: u32 = 0x0010_0000;
const MPQ_FILE_SINGLE_UNIT: u32 = 0x0100_0000;
const MPQ_FILE_DELETE_MARKER: u32 = 0x0200_0000;
const MPQ_FILE_EXISTS: u32 = 0x8000_0000;
const MPQ_FILE_COMPRESS_MASK: u32 = 0x0000_FF00;

const COMPRESSION_ZLIB: u8 = 0x02;
const COMPRESSION_BZIP2: u8 = 0x10;

/// Registers the MPQ container layer.
pub fn register(reg: &mut Registry) {
    reg.add_layer(Arc::new(MpqLayer));
}

/// The `container.mpq` layer.
#[derive(Clone, Copy, Debug, Default)]
pub struct MpqLayer;

#[derive(Clone, Copy, Debug)]
struct Block {
    pos: usize,
    csize: usize,
    fsize: usize,
    flags: u32,
}

#[derive(Clone, Debug)]
struct Archive {
    base: usize,
    sector_size: usize,
    block_pos: usize,
    block_count: usize,
    hash_count: usize,
    hash: Vec<u8>,
    blocks: Vec<u8>,
}

fn damaged(message: impl Into<String>, offset: usize) -> Error {
    Error::new(MPQ_DAMAGED, message).with_offset(len_u64(offset))
}

fn write_error(message: impl Into<String>) -> Error {
    Error::new(MPQ_WRITE, message)
}

fn unsupported(message: impl Into<String>) -> Error {
    Error::new(MPQ_UNSUPPORTED, message)
}

fn storm_table() -> &'static [u32; 0x500] {
    static TABLE: OnceLock<[u32; 0x500]> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = [0_u32; 0x500];
        let mut seed = 0x0010_0001_u32;
        for index1 in 0..0x100_usize {
            let mut index2 = index1;
            for _ in 0..5 {
                seed = seed.wrapping_mul(125).wrapping_add(3) % 0x2A_AAAB;
                let high = (seed & 0xFFFF) << 16;
                seed = seed.wrapping_mul(125).wrapping_add(3) % 0x2A_AAAB;
                let low = seed & 0xFFFF;
                if let Some(slot) = table.get_mut(index2) {
                    *slot = high | low;
                }
                index2 += 0x100;
            }
        }
        table
    })
}

fn upper(byte: u8) -> u8 {
    match byte {
        b'a'..=b'z' => byte - 32,
        b'/' => b'\\',
        other => other,
    }
}

fn hash_string(name: &[u8], hash_type: u32) -> u32 {
    let table = storm_table();
    let mut seed1 = 0x7FED_7FED_u32;
    let mut seed2 = 0xEEEE_EEEE_u32;
    for &byte in name {
        let ch = u32::from(upper(byte));
        let index = hash_type.wrapping_add(ch) as usize;
        let value = table.get(index).copied().unwrap_or(0);
        seed1 = value ^ seed1.wrapping_add(seed2);
        seed2 = ch
            .wrapping_add(seed1)
            .wrapping_add(seed2)
            .wrapping_add(seed2 << 5)
            .wrapping_add(3);
    }
    seed1
}

fn crypt(data: &mut [u8], key: u32) {
    let table = storm_table();
    let mut key1 = key;
    let mut key2 = 0xEEEE_EEEE_u32;
    for chunk in data.as_chunks_mut::<4>().0 {
        key2 = key2.wrapping_add(
            table
                .get(0x400 + (key1 & 0xFF) as usize)
                .copied()
                .unwrap_or(0),
        );
        let value = u32::from_le_bytes(*chunk) ^ key1.wrapping_add(key2);
        *chunk = value.to_le_bytes();
        key1 = ((!key1 << 0x15).wrapping_add(0x1111_1111)) | (key1 >> 0x0B);
        key2 = value
            .wrapping_add(key2)
            .wrapping_add(key2 << 5)
            .wrapping_add(3);
    }
}

fn encrypt(data: &mut [u8], key: u32) {
    let table = storm_table();
    let mut key1 = key;
    let mut key2 = 0xEEEE_EEEE_u32;
    for chunk in data.as_chunks_mut::<4>().0 {
        key2 = key2.wrapping_add(
            table
                .get(0x400 + (key1 & 0xFF) as usize)
                .copied()
                .unwrap_or(0),
        );
        let plain = u32::from_le_bytes(*chunk);
        *chunk = (plain ^ key1.wrapping_add(key2)).to_le_bytes();
        key1 = ((!key1 << 0x15).wrapping_add(0x1111_1111)) | (key1 >> 0x0B);
        key2 = plain
            .wrapping_add(key2)
            .wrapping_add(key2 << 5)
            .wrapping_add(3);
    }
}

fn u32_at(input: &[u8], offset: usize) -> Option<u32> {
    let raw = input.get(offset..offset.checked_add(4)?)?;
    Some(u32::from_le_bytes(<[u8; 4]>::try_from(raw).ok()?))
}

fn u16_at(input: &[u8], offset: usize) -> Option<u16> {
    let raw = input.get(offset..offset.checked_add(2)?)?;
    Some(u16::from_le_bytes(<[u8; 2]>::try_from(raw).ok()?))
}

fn find_header(input: &[u8]) -> Option<usize> {
    if input.get(..4) == Some(MPQ_MAGIC) {
        return Some(0);
    }
    if input.get(..4) == Some(MPQ_USERDATA) {
        let offset = usize::try_from(u32_at(input, 8)?).ok()?;
        if input.get(offset..offset.checked_add(4)?) == Some(MPQ_MAGIC) {
            return Some(offset);
        }
        return None;
    }
    None
}

fn parse(input: &[u8]) -> Result<Archive> {
    let base = find_header(input).ok_or_else(|| damaged("the file has no MPQ signature", 0))?;
    let header_size = usize::try_from(
        u32_at(
            input,
            base.checked_add(4)
                .ok_or_else(|| damaged("the MPQ header offset overflows", 0))?,
        )
        .ok_or_else(|| damaged("the MPQ header is missing", base))?,
    )
    .map_err(|_| damaged("the MPQ header size is too large", base))?;
    if !(0x20..=0x1000).contains(&header_size) {
        return Err(damaged(
            format!("the MPQ header size 0x{header_size:X} is not supported"),
            base + 4,
        ));
    }
    let header_end = base
        .checked_add(header_size)
        .ok_or_else(|| damaged("the MPQ header overflows", base))?;
    if header_end > input.len() {
        return Err(damaged("the MPQ header lies beyond the file", base));
    }
    let sector_shift = u16_at(input, base + 0x0E).unwrap_or(0);
    if sector_shift > 24 {
        return Err(damaged("the MPQ sector size is too large", base + 0x0E));
    }
    let sector_size = 512usize
        .checked_shl(u32::from(sector_shift))
        .ok_or_else(|| damaged("the MPQ sector size overflows", base + 0x0E))?;
    let mut hash_pos = u64::from(u32_at(input, base + 0x10).unwrap_or(0));
    let mut block_pos = u64::from(u32_at(input, base + 0x14).unwrap_or(0));
    let hash_count = usize::try_from(u32_at(input, base + 0x18).unwrap_or(0))
        .map_err(|_| damaged("the hash table size is too large", base + 0x18))?;
    let block_count = usize::try_from(u32_at(input, base + 0x1C).unwrap_or(0))
        .map_err(|_| damaged("the block table size is too large", base + 0x1C))?;
    if header_size >= 0x2C {
        let hash_hi = u64::from(u16_at(input, base + 0x28).unwrap_or(0));
        let block_hi = u64::from(u16_at(input, base + 0x2A).unwrap_or(0));
        hash_pos |= hash_hi << 32;
        block_pos |= block_hi << 32;
    }
    if hash_count > (1 << 24) || block_count > (1 << 24) {
        return Err(damaged("the MPQ tables are too large", base + 0x18));
    }
    let hash_len = hash_count
        .checked_mul(HASH_ENTRY)
        .ok_or_else(|| damaged("the hash table overflows", base + 0x18))?;
    let block_len = block_count
        .checked_mul(BLOCK_ENTRY)
        .ok_or_else(|| damaged("the block table overflows", base + 0x1C))?;
    let hash_start = base
        .checked_add(
            usize::try_from(hash_pos)
                .map_err(|_| damaged("the hash table offset is too large", base + 0x10))?,
        )
        .ok_or_else(|| damaged("the hash table offset overflows", base))?;
    let block_start = base
        .checked_add(
            usize::try_from(block_pos)
                .map_err(|_| damaged("the block table offset is too large", base + 0x14))?,
        )
        .ok_or_else(|| damaged("the block table offset overflows", base))?;
    let hash_end = hash_start
        .checked_add(hash_len)
        .ok_or_else(|| damaged("the hash table overflows", hash_start))?;
    let block_end = block_start
        .checked_add(block_len)
        .ok_or_else(|| damaged("the block table overflows", block_start))?;
    if hash_end > input.len() || block_end > input.len() {
        return Err(damaged("an MPQ table lies beyond the file", base));
    }
    let mut hash = input
        .get(hash_start..hash_end)
        .ok_or_else(|| damaged("the hash table is cut short", hash_start))?
        .to_vec();
    crypt(&mut hash, HASH_TABLE_KEY);
    let mut blocks = input
        .get(block_start..block_end)
        .ok_or_else(|| damaged("the block table is cut short", block_start))?
        .to_vec();
    crypt(&mut blocks, BLOCK_TABLE_KEY);
    Ok(Archive {
        base,
        sector_size,
        block_pos: block_start,
        block_count,
        hash_count,
        hash,
        blocks,
    })
}

impl Archive {
    fn hash_entry(&self, index: usize) -> Option<(u32, u32, u32)> {
        let at = index.checked_mul(HASH_ENTRY)?;
        let first = u32_at(&self.hash, at)?;
        let second = u32_at(&self.hash, at + 4)?;
        let block = u32_at(&self.hash, at + 12)?;
        Some((first, second, block))
    }

    fn block(&self, index: usize) -> Option<Block> {
        let at = index.checked_mul(BLOCK_ENTRY)?;
        Some(Block {
            pos: usize::try_from(u32_at(&self.blocks, at)?).ok()?,
            csize: usize::try_from(u32_at(&self.blocks, at + 4)?).ok()?,
            fsize: usize::try_from(u32_at(&self.blocks, at + 8)?).ok()?,
            flags: u32_at(&self.blocks, at + 12)?,
        })
    }

    fn find(&self, name: &[u8]) -> Option<usize> {
        if self.hash_count == 0 {
            return None;
        }
        let start = (hash_string(name, 0) as usize) % self.hash_count;
        let want_a = hash_string(name, 0x100);
        let want_b = hash_string(name, 0x200);
        for step in 0..self.hash_count {
            let index = (start + step) % self.hash_count;
            let (a, b, block) = self.hash_entry(index)?;
            match block {
                0xFFFF_FFFF => return None,
                0xFFFF_FFFE => continue,
                _ => {}
            }
            if a == want_a && b == want_b {
                let block_index = usize::try_from(block).ok()?;
                if block_index < self.block_count {
                    return Some(block_index);
                }
                return None;
            }
        }
        None
    }

    fn listfile_names(&self, input: &[u8]) -> Option<Vec<Vec<u8>>> {
        let index = self.find(b"(listfile)")?;
        let block = self.block(index)?;
        let bytes = self.read(input, b"(listfile)", &block).ok()?;
        let mut names = Vec::new();
        for raw in bytes.split(|&byte| byte == b'\n') {
            let name = raw.strip_suffix(b"\r").unwrap_or(raw);
            let name = name.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(name);
            if name.is_empty() || name.starts_with(b"(") {
                continue;
            }
            names.push(name.to_vec());
        }
        Some(names)
    }

    fn file_key(&self, name: &[u8], block: &Block) -> u32 {
        let plain = plain_name(name);
        let mut key = hash_string(plain, HASH_FILE_KEY);
        if block.flags & MPQ_FILE_KEY_V2 != 0 {
            key = key.wrapping_add(u32::try_from(block.pos).unwrap_or(0))
                ^ u32::try_from(block.fsize).unwrap_or(0);
        }
        key
    }

    fn read(&self, input: &[u8], name: &[u8], block: &Block) -> Result<Vec<u8>> {
        if block.flags & MPQ_FILE_DELETE_MARKER != 0 || block.flags & MPQ_FILE_EXISTS == 0 {
            return Err(unsupported("the entry is deleted"));
        }
        if block.flags & MPQ_FILE_PATCH_FILE != 0 {
            return Err(unsupported("patch files are read-only"));
        }
        let start = self
            .base
            .checked_add(block.pos)
            .ok_or_else(|| damaged("an entry offset overflows", self.base))?;
        let end = start
            .checked_add(block.csize)
            .ok_or_else(|| damaged("an entry range overflows", start))?;
        let raw = input
            .get(start..end)
            .ok_or_else(|| damaged("an entry lies beyond the file", start))?;
        let key = (block.flags & MPQ_FILE_ENCRYPTED != 0).then(|| self.file_key(name, block));
        let compressed = block.flags & MPQ_FILE_COMPRESS_MASK != 0;
        if block.flags & MPQ_FILE_IMPLODE != 0 {
            return Err(unsupported("PKWARE implode is not supported"));
        }
        if compressed && block.flags & MPQ_FILE_SINGLE_UNIT != 0 {
            let mut data = raw.to_vec();
            if let Some(key) = key {
                crypt(&mut data, key);
            }
            let Some(method) = data.first().copied() else {
                return Err(damaged("a compressed entry is empty", start));
            };
            let body = data.get(1..).unwrap_or_default();
            return decompress(method, body, block.fsize);
        }
        if compressed {
            let sectors = block.fsize.div_ceil(self.sector_size);
            let table_len = sectors
                .checked_add(1)
                .and_then(|count| count.checked_mul(4))
                .ok_or_else(|| damaged("the sector table overflows", start))?;
            let mut table = raw
                .get(..table_len)
                .ok_or_else(|| damaged("the sector table is cut short", start))?
                .to_vec();
            if let Some(key) = key {
                crypt(&mut table, key.wrapping_sub(1));
            }
            let mut offsets = Vec::with_capacity(sectors + 1);
            for index in 0..=sectors {
                let value = u32_at(&table, index.saturating_mul(4))
                    .and_then(|value| usize::try_from(value).ok())
                    .ok_or_else(|| damaged("the sector table is cut short", start))?;
                offsets.push(value);
            }
            let mut out = Vec::with_capacity(block.fsize);
            for index in 0..sectors {
                let from = offsets.get(index).copied().unwrap_or(0);
                let to = offsets.get(index + 1).copied().unwrap_or(from);
                if to < from || to > raw.len() {
                    return Err(damaged("a sector range is invalid", start + from));
                }
                let mut sector = raw
                    .get(from..to)
                    .ok_or_else(|| damaged("a sector is cut short", start + from))?
                    .to_vec();
                if let Some(key) = key {
                    crypt(
                        &mut sector,
                        key.wrapping_add(u32::try_from(index).unwrap_or(0)),
                    );
                }
                if sector.is_empty() {
                    continue;
                }
                let method = sector.first().copied().unwrap_or(0);
                let body = sector.get(1..).unwrap_or_default();
                let part = if method == 0 {
                    body.to_vec()
                } else {
                    decompress(method, body, self.sector_size)?
                };
                out.extend_from_slice(&part);
            }
            out.truncate(block.fsize);
            return Ok(out);
        }
        let mut data = raw.to_vec();
        if let Some(key) = key {
            if block.flags & MPQ_FILE_SINGLE_UNIT != 0 {
                crypt(&mut data, key);
            } else {
                for (index, chunk) in data.chunks_mut(self.sector_size).enumerate() {
                    crypt(chunk, key.wrapping_add(u32::try_from(index).unwrap_or(0)));
                }
            }
        }
        data.truncate(block.fsize);
        Ok(data)
    }
}

fn plain_name(name: &[u8]) -> &[u8] {
    name.rsplit(|&byte| byte == b'/' || byte == b'\\')
        .next()
        .unwrap_or(name)
}

fn decompress(method: u8, body: &[u8], limit: usize) -> Result<Vec<u8>> {
    match method {
        COMPRESSION_ZLIB => {
            let mut decoder = flate2::read::ZlibDecoder::new(body);
            let mut out = Vec::new();
            std::io::Read::read_to_end(&mut decoder, &mut out).map_err(|source| {
                Error::new(MPQ_DAMAGED, "zlib decompression failed").with_source(source)
            })?;
            out.truncate(limit);
            Ok(out)
        }
        COMPRESSION_BZIP2 => {
            let mut decoder = bzip2::read::BzDecoder::new(body);
            let mut out = Vec::new();
            std::io::Read::read_to_end(&mut decoder, &mut out).map_err(|source| {
                Error::new(MPQ_DAMAGED, "bzip2 decompression failed").with_source(source)
            })?;
            out.truncate(limit);
            Ok(out)
        }
        other => Err(unsupported(format!(
            "compression method 0x{other:02X} is not supported"
        ))),
    }
}

struct NamedEntry {
    name: Vec<u8>,
    index: usize,
    block: Block,
}

fn entries(input: &[u8], archive: &Archive) -> Vec<NamedEntry> {
    let mut out = Vec::new();
    if let Some(names) = archive.listfile_names(input) {
        for name in names {
            if let Some(index) = archive.find(&name)
                && let Some(block) = archive.block(index)
            {
                out.push(NamedEntry { name, index, block });
            }
        }
    }
    if out.is_empty() {
        for index in 0..archive.block_count {
            if let Some(block) = archive.block(index)
                && block.flags & MPQ_FILE_EXISTS != 0
            {
                out.push(NamedEntry {
                    name: format!("File{index:08X}.bin").into_bytes(),
                    index,
                    block,
                });
            }
        }
    }
    out
}

fn chosen<'a>(
    entries: &'a [NamedEntry],
    input: &[u8],
    archive: &Archive,
    hint: Option<&[u8]>,
) -> Option<&'a NamedEntry> {
    if let Some(name) = hint
        && let Some(found) = entries.iter().find(|entry| entry.name == name)
    {
        return Some(found);
    }
    let mut sorted: Vec<&NamedEntry> = entries.iter().collect();
    sorted.sort_by_key(|entry| std::cmp::Reverse(entry.block.fsize));
    for entry in &sorted {
        if let Ok(bytes) = archive.read(input, &entry.name, &entry.block)
            && inner_is_known(&bytes)
        {
            return Some(entry);
        }
    }
    sorted.first().copied()
}

impl Layer for MpqLayer {
    fn id(&self) -> &'static str {
        "container.mpq"
    }

    fn sniff(&self, input: &[u8], ctx: &SniffCtx<'_>) -> Confidence {
        if find_header(input).is_none() {
            return Confidence::NONE;
        }
        let Ok(archive) = parse(input) else {
            return Confidence::NONE;
        };
        if archive.hash_count == 0 || archive.block_count == 0 {
            return Confidence::NONE;
        }
        let valid = (0..archive.hash_count.min(256)).any(|index| {
            archive.hash_entry(index).is_some_and(|(_, _, block)| {
                block == 0xFFFF_FFFF
                    || block == 0xFFFF_FFFE
                    || (block as usize) < archive.block_count
            })
        });
        if !valid {
            return Confidence::NONE;
        }
        let named = ctx.file_name.is_some_and(|name| {
            let lower = name.to_ascii_lowercase();
            [".mpq", ".sc2save", ".w3z", ".w3m", ".sav"]
                .iter()
                .any(|end| lower.ends_with(end))
        });
        Confidence::new(if named { 1.0 } else { 0.9 })
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<(Vec<u8>, Params)> {
        ctx.limits().check_input(len_u64(input.len()))?;
        let archive = parse(input)?;
        let all = entries(input, &archive);
        let hint = ctx.hints().opt_bytes(ENTRY_PARAM)?;
        let entry = chosen(&all, input, &archive, hint)
            .ok_or_else(|| Error::new(MPQ_NO_PAYLOAD, "the archive holds no usable entry"))?;
        let mut params = Params::new();
        params.set(ENTRY_PARAM, entry.name.clone());
        params.set(
            ENTRY_INDEX_PARAM,
            i64::try_from(entry.index).unwrap_or(i64::MAX),
        );
        match archive.read(input, &entry.name, &entry.block) {
            Ok(inner) => {
                ctx.limits()
                    .check_output(len_u64(input.len()), len_u64(inner.len()))?;
                keep_source(&mut params, input, &inner);
                Ok((inner, params))
            }
            Err(error) if error.code() == MPQ_UNSUPPORTED => {
                let start = archive.base.saturating_add(entry.block.pos);
                let end = start.saturating_add(entry.block.csize);
                let raw = input.get(start..end).unwrap_or_default().to_vec();
                params.set(NOTE_PARAM, error.message().to_string());
                keep_source(&mut params, input, &raw);
                Ok((raw, params))
            }
            Err(error) => Err(error),
        }
    }

    fn encode(&self, inner: &[u8], params: &Params) -> Result<Vec<u8>> {
        if let Some(source) = source_if_unchanged(inner, params)? {
            return Ok(source);
        }
        let source = params.bytes(super::SOURCE_PARAM)?;
        let name = params.bytes(ENTRY_PARAM)?;
        let index = params
            .opt_int(ENTRY_INDEX_PARAM)?
            .and_then(|value| usize::try_from(value).ok());
        let archive = parse(source)?;
        let block_index = index
            .filter(|value| *value < archive.block_count)
            .or_else(|| archive.find(name))
            .ok_or_else(|| {
                Error::new(
                    codes::CORE_INVALID_PARAM,
                    "the parameter \"entry\" names no entry of the archive",
                )
            })?;
        let block = archive
            .block(block_index)
            .ok_or_else(|| write_error("the block table entry is missing"))?;
        if params.opt_str(NOTE_PARAM)?.is_some() {
            return Err(write_error(
                "this entry is read-only (unsupported compression)",
            ));
        }
        if block.flags & (MPQ_FILE_ENCRYPTED | MPQ_FILE_IMPLODE) != 0
            || block.flags & MPQ_FILE_COMPRESS_MASK != 0
            || block.flags & MPQ_FILE_PATCH_FILE != 0
        {
            return Err(write_error(
                "compressed or encrypted MPQ entries are read-only",
            ));
        }
        if inner.len() > block.csize {
            return Err(write_error(format!(
                "the entry has {} bytes of room; the edited data needs {} bytes",
                block.csize,
                inner.len()
            )));
        }
        let start = archive
            .base
            .checked_add(block.pos)
            .ok_or_else(|| write_error("the entry offset overflows"))?;
        let end = start
            .checked_add(inner.len())
            .ok_or_else(|| write_error("the entry range overflows"))?;
        let mut out = source.to_vec();
        out.get_mut(start..end)
            .ok_or_else(|| write_error("the archive is cut short"))?
            .copy_from_slice(inner);
        let at = block_index
            .checked_mul(BLOCK_ENTRY)
            .ok_or_else(|| write_error("the block table index overflows"))?;
        let mut blocks = archive.blocks.clone();
        let size =
            u32::try_from(inner.len()).map_err(|_| write_error("the edited entry is too large"))?;
        if let Some(field) = blocks.get_mut(at + 4..at + 8) {
            field.copy_from_slice(&size.to_le_bytes());
        }
        if let Some(field) = blocks.get_mut(at + 8..at + 12) {
            field.copy_from_slice(&size.to_le_bytes());
        }
        encrypt(&mut blocks, BLOCK_TABLE_KEY);
        let table_end = archive
            .block_pos
            .checked_add(blocks.len())
            .ok_or_else(|| write_error("the block table overflows"))?;
        out.get_mut(archive.block_pos..table_end)
            .ok_or_else(|| write_error("the archive is cut short"))?
            .copy_from_slice(&blocks);
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use saveprism_core::detect::{DetectOptions, detect};
    use saveprism_core::guard::{Verdict, check_round_trip};
    use saveprism_core::{Limits, pipeline};

    use super::*;
    use crate::register_all;

    fn fixture() -> Vec<u8> {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/synthetic/archives/mpq/save.SC2Save");
        std::fs::read(path).unwrap()
    }

    fn decode(input: &[u8]) -> (Vec<u8>, Params) {
        let limits = Limits::default();
        let mut ctx = DecodeCtx::new(&limits);
        MpqLayer.decode(input, &mut ctx).unwrap()
    }

    #[test]
    fn fixture_round_trips_and_picks_save_json() {
        let bytes = fixture();
        let (inner, params) = decode(&bytes);
        assert_eq!(params.bytes(ENTRY_PARAM).unwrap(), b"save.json");
        assert!(inner.starts_with(b"{"));
        assert_eq!(MpqLayer.encode(&inner, &params).unwrap(), bytes);
    }

    #[test]
    fn editing_an_uncompressed_entry_keeps_every_other_byte() {
        let bytes = fixture();
        let (mut inner, params) = decode(&bytes);
        let position = inner.iter().position(|&byte| byte == b'1').unwrap();
        inner[position] = b'9';
        let written = MpqLayer.encode(&inner, &params).unwrap();
        assert_eq!(written.len(), bytes.len());
        let (again, _) = decode(&written);
        assert_eq!(again, inner);
        assert_eq!(
            bytes
                .iter()
                .zip(written.iter())
                .filter(|(a, b)| a != b)
                .count(),
            1
        );
    }

    #[test]
    fn detection_reaches_the_json_payload() {
        let bytes = fixture();
        let mut reg = Registry::default();
        register_all(&mut reg);
        let detection =
            detect(&bytes, &reg, &Limits::default(), &DetectOptions::default()).unwrap();
        assert_eq!(
            detection.best.recipe.kinds(),
            ["container.mpq", "format.json"]
        );
        let report =
            check_round_trip(&bytes, &detection.best.recipe, &reg, &Limits::default()).unwrap();
        assert_eq!(report.overall, Verdict::Exact);
        let decoded =
            pipeline::decode(&bytes, &detection.best.recipe, &reg, &Limits::default()).unwrap();
        let encoded = pipeline::encode(&decoded.doc, &decoded.recipe, &reg).unwrap();
        assert_eq!(encoded, bytes);
    }

    #[test]
    fn damaged_archives_give_the_code() {
        let limits = Limits::default();
        let mut ctx = DecodeCtx::new(&limits);
        assert_eq!(
            MpqLayer.decode(b"MPQ\x1A", &mut ctx).unwrap_err().code(),
            MPQ_DAMAGED
        );
        let bytes = fixture();
        let mut cut = bytes;
        cut.truncate(0x18);
        let mut ctx = DecodeCtx::new(&limits);
        assert_eq!(
            MpqLayer.decode(&cut, &mut ctx).unwrap_err().code(),
            MPQ_DAMAGED
        );
    }

    #[test]
    fn random_bytes_are_not_an_archive() {
        assert_eq!(
            MpqLayer.sniff(&vec![0_u8; 512], &SniffCtx::default()),
            Confidence::NONE
        );
    }
}
