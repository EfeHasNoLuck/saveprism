//! `container.ps2-card`: PlayStation 2 memory card images (with or without the spare area).
//!
//! # Format
//!
//! A standard 8 MB card holds 1024 erase blocks of 16 pages; a page is 512 data bytes plus 16
//! spare bytes on a raw flash dump (528 bytes on disk) or 512 bytes in a data-only dump. The
//! superblock in page 0 names the page and cluster geometry, the indirect FAT list, the
//! allocation offset and the root directory. The FAT chains clusters; directory entries are 512
//! bytes (`DF_EXISTS`/`DF_FILE`/`DF_DIRECTORY` flags, byte length, first cluster, name). Each 128
//! data-byte chunk has a 20-bit Hamming ECC in three spare bytes (four chunks per page).
//!
//! Tables and field meanings checked against Ross Ridge's public "PlayStation 2 Memory Card File
//! System" document (public domain) and the ps2sdk memory-card sources (read for behavior only;
//! ps2sdk is AFL-2.0, its code is not copied). The ECC computation is our own implementation of
//! the described Hamming code.
//!
//! # What the container returns
//!
//! The largest regular file in the card's directory tree (normally the game's save data; icons
//! are small), or the file named by the `entry` hint (its path inside the save folder). The file
//! bytes go to normal detection; unknown binary data can still be inspected.
//!
//! # Exact writing and safety
//!
//! An unchanged payload is returned from the saved original bytes. After an edit the bytes are
//! written into the file's existing clusters (growing beyond them is refused), the directory
//! entry's length field is updated, and the ECC of every touched page is computed again. Other
//! pages, the spare bytes and the rest of the card stay byte-identical. FAT and directory chains
//! are followed with a visited set and a bounded length.

use std::sync::Arc;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    Confidence, DecodeCtx, Error, ErrorCode, Layer, Params, Registry, Result, SniffCtx, codes,
};

use super::{keep_source, source_if_unchanged};

/// The card is damaged, cut short or has no usable save file.
pub const PS2_DAMAGED: ErrorCode = ErrorCode::new("SP-PS2-001");
/// The edited save cannot be written back into the card.
pub const PS2_WRITE: ErrorCode = ErrorCode::new("SP-PS2-002");
/// No non-empty regular file was found in the card.
pub const PS2_NO_PAYLOAD: ErrorCode = ErrorCode::new("SP-PS2-003");

/// Parameter: path of the chosen file inside the card (`folder/file`).
pub const ENTRY_PARAM: &str = "entry";
/// Parameter: absolute page of the chosen directory entry.
pub const ENTRY_PAGE_PARAM: &str = "entry_page";
/// Parameter: byte offset of the chosen entry inside that page.
pub const ENTRY_OFFSET_PARAM: &str = "entry_offset";

const PAGE_DATA: usize = 512;
const PAGE_SPARE: usize = 16;
const PAGE_ECC: usize = 12;
const CLUSTER_PAGES_LIMIT: u32 = 16;
const MAX_PAGES: usize = 1 << 20;
const MAGIC: &[u8; 28] = b"Sony PS2 Memory Card Format ";
const DF_FILE: u16 = 0x0010;
const DF_DIRECTORY: u16 = 0x0020;
const DF_EXISTS: u16 = 0x8000;
const FAT_END: u32 = 0xFFFF_FFFF;
const FAT_ALLOCATED: u32 = 0x8000_0000;
const DIR_NAME_OFFSET: usize = 0x40;
const DIR_NAME_LEN: usize = 32;
const DIR_LENGTH_OFFSET: usize = 0x04;
const DIR_CLUSTER_OFFSET: usize = 0x10;
const MAX_DIR_DEPTH: usize = 8;
const MAX_DIR_ENTRIES: usize = 100_000;

/// Registers the PS2 memory card layer.
pub fn register(reg: &mut Registry) {
    reg.add_layer(Arc::new(Ps2CardLayer));
}

/// The PS2 memory card layer.
#[derive(Clone, Copy, Debug, Default)]
pub struct Ps2CardLayer;

fn damaged(message: impl Into<String>, offset: usize) -> Error {
    Error::new(PS2_DAMAGED, message).with_offset(len_u64(offset))
}

fn write_error(message: impl Into<String>) -> Error {
    Error::new(PS2_WRITE, message)
}

fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    let raw = bytes.get(at..at.checked_add(2)?)?;
    Some(u16::from_le_bytes(<[u8; 2]>::try_from(raw).ok()?))
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    let raw = bytes.get(at..at.checked_add(4)?)?;
    Some(u32::from_le_bytes(<[u8; 4]>::try_from(raw).ok()?))
}

#[derive(Clone, Debug)]
struct Geometry {
    page_len: usize,
    pages_per_cluster: usize,
    clusters: u32,
    alloc_offset: u32,
    rootdir: u32,
    total_pages: usize,
    stride: usize,
    ecc: bool,
}

impl Geometry {
    fn cluster_size(&self) -> usize {
        self.page_len.saturating_mul(self.pages_per_cluster)
    }

    fn page_start(&self, page: usize) -> Option<usize> {
        page.checked_mul(self.stride)
    }

    fn page_data<'a>(&self, input: &'a [u8], page: usize) -> Result<&'a [u8]> {
        let start = self
            .page_start(page)
            .ok_or_else(|| damaged("the memory card is too large", page))?;
        input
            .get(start..start.saturating_add(self.page_len))
            .ok_or_else(|| damaged("the memory card ends inside a page", start))
    }

    fn check_page(&self, page: usize) -> Result<()> {
        if page >= self.total_pages {
            return Err(damaged(
                format!(
                    "page {page} is outside the card ({} pages)",
                    self.total_pages
                ),
                page,
            ));
        }
        Ok(())
    }
}

fn parse_superblock(input: &[u8]) -> Result<Geometry> {
    if input.len() < PAGE_DATA {
        return Err(damaged("the file is shorter than one memory card page", 0));
    }
    let superblock = input
        .get(..PAGE_DATA)
        .ok_or_else(|| damaged("the superblock is cut short", 0))?;
    if superblock.get(..MAGIC.len()) != Some(MAGIC) {
        return Err(damaged("the superblock has no PS2 card signature", 0));
    }
    let page_len = usize::from(
        u16_at(superblock, 0x28).ok_or_else(|| damaged("the page size is missing", 0x28))?,
    );
    let pages_per_cluster = usize::from(
        u16_at(superblock, 0x2A).ok_or_else(|| damaged("the cluster size is missing", 0x2A))?,
    );
    let pages_per_block = usize::from(
        u16_at(superblock, 0x2C).ok_or_else(|| damaged("the block size is missing", 0x2C))?,
    );
    let clusters =
        u32_at(superblock, 0x30).ok_or_else(|| damaged("the cluster count is missing", 0x30))?;
    let alloc_offset = u32_at(superblock, 0x34)
        .ok_or_else(|| damaged("the allocation offset is missing", 0x34))?;
    let rootdir = u32_at(superblock, 0x3C)
        .ok_or_else(|| damaged("the root directory cluster is missing", 0x3C))?;
    if superblock.get(0x150).copied() != Some(2) {
        return Err(damaged("the card type is not a PS2 memory card", 0x150));
    }
    if page_len != PAGE_DATA {
        return Err(damaged(
            format!("page size {page_len} is not supported (only 512-byte pages)"),
            0x28,
        ));
    }
    if pages_per_cluster == 0
        || pages_per_cluster > usize::try_from(CLUSTER_PAGES_LIMIT).unwrap_or(16)
    {
        return Err(damaged(
            format!("{pages_per_cluster} pages per cluster is not supported"),
            0x2A,
        ));
    }
    if pages_per_block == 0 || pages_per_block > 16 {
        return Err(damaged(
            format!("{pages_per_block} pages per block is not supported"),
            0x2C,
        ));
    }
    if clusters == 0 {
        return Err(damaged("the card has no clusters", 0x30));
    }
    if alloc_offset >= clusters {
        return Err(damaged("the allocation offset is outside the card", 0x34));
    }
    if rootdir >= clusters.saturating_sub(alloc_offset) {
        return Err(damaged(
            "the root directory cluster is outside the allocation area",
            0x3C,
        ));
    }
    let total_pages = usize::try_from(clusters)
        .ok()
        .and_then(|value| value.checked_mul(pages_per_cluster))
        .ok_or_else(|| damaged("the card geometry is too large", 0x30))?;
    if total_pages == 0 || total_pages > MAX_PAGES {
        return Err(damaged(
            format!("the card would have {total_pages} pages (limit {MAX_PAGES})"),
            0x30,
        ));
    }
    let data_len = total_pages
        .checked_mul(PAGE_DATA)
        .ok_or_else(|| damaged("the card geometry is too large", 0x30))?;
    let raw_len = total_pages
        .checked_mul(PAGE_DATA + PAGE_SPARE)
        .ok_or_else(|| damaged("the card geometry is too large", 0x30))?;
    let (stride, ecc) = if input.len() == raw_len {
        (PAGE_DATA + PAGE_SPARE, true)
    } else if input.len() == data_len {
        (PAGE_DATA, false)
    } else {
        return Err(damaged(
            format!(
                "the file is {} bytes, but the superblock describes {data_len} data bytes or {raw_len} raw bytes",
                input.len()
            ),
            0,
        ));
    };
    Ok(Geometry {
        page_len,
        pages_per_cluster,
        clusters,
        alloc_offset,
        rootdir,
        total_pages,
        stride,
        ecc,
    })
}

/// One byte's contribution to the 20-bit Hamming code: bit 7 is the parity flag, bits 4..6 the
/// XOR of the set bit positions, bits 0..3 the even/odd column parity.
fn byte_parity(byte: u8) -> u8 {
    let mut positions = 0_u8;
    let mut parity = 0_u8;
    for bit in 0_u8..8 {
        if byte & (1 << bit) != 0 {
            positions ^= bit;
            parity ^= 1;
        }
    }
    let low = (if parity == 1 { 7 } else { 0 }) ^ positions;
    (parity << 7) | (parity << 3) | (positions << 4) | (low & 0x07)
}

fn hamming_ecc(data: &[u8]) -> [u8; 3] {
    let mut column = 0_u8;
    let mut lines_odd = 0_u8;
    let mut lines_even = 0_u8;
    for (index, &byte) in data.iter().enumerate() {
        let value = byte_parity(byte);
        column ^= value;
        if value & 0x80 != 0 {
            let position = u8::try_from(index).unwrap_or(u8::MAX);
            lines_odd ^= !position;
            lines_even ^= position;
        }
    }
    [!column & 0x77, !lines_odd & 0x7F, !lines_even & 0x7F]
}

fn page_ecc(geometry: &Geometry, input: &[u8], page: usize) -> Result<[u8; PAGE_ECC]> {
    let data = geometry.page_data(input, page)?;
    let mut ecc = [0_u8; PAGE_ECC];
    for (chunk, slot) in data.chunks(PAGE_DATA / 4).zip(ecc.chunks_mut(3)) {
        let code = hamming_ecc(chunk);
        slot.copy_from_slice(&code);
    }
    Ok(ecc)
}

fn check_page_ecc(geometry: &Geometry, input: &[u8], page: usize) -> Result<()> {
    if !geometry.ecc {
        return Ok(());
    }
    let expected = page_ecc(geometry, input, page)?;
    let start = geometry
        .page_start(page)
        .and_then(|value| value.checked_add(PAGE_DATA))
        .ok_or_else(|| damaged("the memory card is too large", page))?;
    let stored = input
        .get(start..start.saturating_add(PAGE_ECC))
        .ok_or_else(|| damaged("the spare area is cut short", start))?;
    if stored != expected {
        return Err(damaged(
            format!("the ECC of page {page} does not match its data"),
            start,
        ));
    }
    Ok(())
}

fn read_cluster(geometry: &Geometry, input: &[u8], cluster: u32) -> Result<Vec<u8>> {
    let first = usize::try_from(cluster)
        .ok()
        .and_then(|value| value.checked_mul(geometry.pages_per_cluster))
        .ok_or_else(|| damaged("the cluster number is too large", 0))?;
    let mut out = Vec::with_capacity(geometry.cluster_size());
    for index in 0..geometry.pages_per_cluster {
        let page = first
            .checked_add(index)
            .ok_or_else(|| damaged("the cluster number is too large", 0))?;
        geometry.check_page(page)?;
        check_page_ecc(geometry, input, page)?;
        out.extend_from_slice(geometry.page_data(input, page)?);
    }
    Ok(out)
}

fn write_page(geometry: &Geometry, out: &mut [u8], page: usize, data: &[u8]) -> Result<()> {
    if data.len() != geometry.page_len {
        return Err(write_error("a memory card page has the wrong length"));
    }
    let start = geometry
        .page_start(page)
        .ok_or_else(|| write_error("the memory card is too large"))?;
    let target = out
        .get_mut(start..start.saturating_add(geometry.page_len))
        .ok_or_else(|| write_error("the memory card ends inside a page"))?;
    target.copy_from_slice(data);
    if geometry.ecc {
        let mut ecc = [0_u8; PAGE_ECC];
        for (chunk, slot) in data.chunks(PAGE_DATA / 4).zip(ecc.chunks_mut(3)) {
            slot.copy_from_slice(&hamming_ecc(chunk));
        }
        let ecc_start = start.saturating_add(geometry.page_len);
        let spare = out
            .get_mut(ecc_start..ecc_start.saturating_add(PAGE_ECC))
            .ok_or_else(|| write_error("the spare area is cut short"))?;
        spare.copy_from_slice(&ecc);
    }
    Ok(())
}

struct Fat {
    entries: Vec<u32>,
    alloc_offset: u32,
    clusters: u32,
}

impl Fat {
    fn next(&self, relative: u32) -> Result<u32> {
        let index =
            usize::try_from(relative).map_err(|_| damaged("the FAT index is too large", 0))?;
        let value = self
            .entries
            .get(index)
            .copied()
            .ok_or_else(|| damaged("a FAT index is outside the table", index))?;
        if value == FAT_END {
            return Ok(FAT_END);
        }
        if value & FAT_ALLOCATED == 0 {
            return Err(damaged(
                format!("cluster {relative} is free but part of a chain"),
                index,
            ));
        }
        let next = value & !FAT_ALLOCATED;
        if next >= self.clusters {
            return Err(damaged("a FAT chain points outside the card", index));
        }
        Ok(next)
    }

    fn chain(&self, first: u32) -> Result<Vec<u32>> {
        let mut chain = Vec::new();
        let mut relative = first;
        while relative != FAT_END {
            if chain.len() >= usize::try_from(self.clusters).unwrap_or(0) {
                return Err(damaged("a FAT chain is longer than the card", 0));
            }
            let absolute = self
                .alloc_offset
                .checked_add(relative)
                .ok_or_else(|| damaged("a FAT chain overflows", 0))?;
            chain.push(absolute);
            relative = self.next(relative)?;
        }
        if chain.is_empty() {
            return Err(damaged("a chain starts at the end marker", 0));
        }
        Ok(chain)
    }
}

fn parse_fat(geometry: &Geometry, input: &[u8]) -> Result<Fat> {
    let superblock = geometry.page_data(input, 0)?;
    let mut fat = vec![0_u32; usize::try_from(geometry.clusters).unwrap_or(0)];
    for table_index in 0..32_usize {
        let Some(ifc) = u32_at(
            superblock,
            0x50_usize.saturating_add(table_index.saturating_mul(4)),
        ) else {
            break;
        };
        if ifc == 0 || ifc == FAT_END {
            continue;
        }
        if ifc >= geometry.clusters {
            return Err(damaged(
                "an indirect FAT cluster is outside the card",
                ifc as usize,
            ));
        }
        let indirect = read_cluster(geometry, input, ifc)?;
        for entry_index in 0..(indirect.len() / 4) {
            let Some(fat_cluster) = u32_at(&indirect, entry_index.saturating_mul(4)) else {
                continue;
            };
            if fat_cluster == 0 || fat_cluster == FAT_END {
                continue;
            }
            if fat_cluster >= geometry.clusters {
                return Err(damaged(
                    "a FAT cluster is outside the card",
                    fat_cluster as usize,
                ));
            }
            let block = read_cluster(geometry, input, fat_cluster)?;
            let base = table_index
                .saturating_mul(indirect.len() / 4)
                .saturating_add(entry_index)
                .saturating_mul(block.len() / 4);
            for index in 0..(block.len() / 4) {
                let Some(value) = u32_at(&block, index.saturating_mul(4)) else {
                    continue;
                };
                if let Some(slot) = fat.get_mut(base.saturating_add(index)) {
                    *slot = value;
                }
            }
        }
    }
    Ok(Fat {
        entries: fat,
        alloc_offset: geometry.alloc_offset,
        clusters: geometry.clusters,
    })
}

#[derive(Clone, Debug)]
struct FileEntry {
    path: Vec<u8>,
    cluster: u32,
    length: usize,
    capacity: usize,
    entry_page: usize,
    entry_offset: usize,
}

fn name_of(entry: &[u8]) -> Vec<u8> {
    entry
        .get(DIR_NAME_OFFSET..DIR_NAME_OFFSET.saturating_add(DIR_NAME_LEN))
        .unwrap_or_default()
        .iter()
        .copied()
        .take_while(|&byte| byte != 0)
        .collect()
}

/// Reads a directory chain and returns its bytes together with the absolute page of every
/// cluster (so a directory entry can be located after an edit).
fn directory_bytes(
    geometry: &Geometry,
    input: &[u8],
    fat: &Fat,
    first_relative: u32,
) -> Result<(Vec<u8>, Vec<u32>)> {
    let chain = fat.chain(first_relative)?;
    let mut bytes = Vec::with_capacity(chain.len().saturating_mul(geometry.cluster_size()));
    for &absolute in &chain {
        bytes.extend_from_slice(&read_cluster(geometry, input, absolute)?);
    }
    let mut pages = Vec::new();
    for &absolute in &chain {
        let first = usize::try_from(absolute)
            .ok()
            .and_then(|value| value.checked_mul(geometry.pages_per_cluster))
            .ok_or_else(|| damaged("a directory cluster is too large", 0))?;
        pages.push(u32::try_from(first).unwrap_or(u32::MAX));
    }
    Ok((bytes, pages))
}

fn locate(geometry: &Geometry, pages: &[u32], byte_index: usize) -> Result<(usize, usize)> {
    let per_cluster = geometry.cluster_size();
    let cluster = byte_index / per_cluster;
    let within = byte_index % per_cluster;
    let page_in_cluster = within / geometry.page_len;
    let offset = within % geometry.page_len;
    let cluster_page = pages
        .get(cluster)
        .copied()
        .ok_or_else(|| damaged("a directory entry is outside its chain", byte_index))?;
    let page = usize::try_from(cluster_page)
        .ok()
        .and_then(|value| value.checked_add(page_in_cluster))
        .ok_or_else(|| damaged("a directory page is too large", byte_index))?;
    Ok((page, offset))
}

struct Walker<'a> {
    geometry: &'a Geometry,
    input: &'a [u8],
    fat: &'a Fat,
    files: Vec<FileEntry>,
    visited: std::collections::BTreeSet<u32>,
}

impl<'a> Walker<'a> {
    fn new(geometry: &'a Geometry, input: &'a [u8], fat: &'a Fat) -> Self {
        Self {
            geometry,
            input,
            fat,
            files: Vec::new(),
            visited: std::collections::BTreeSet::new(),
        }
    }

    fn walk(&mut self, first_relative: u32, prefix: &[u8], depth: usize) -> Result<()> {
        if depth > MAX_DIR_DEPTH {
            return Err(damaged("the directory tree is too deep", 0));
        }
        if !self.visited.insert(first_relative) {
            return Err(damaged("a directory chain loops", 0));
        }
        let (bytes, pages) = directory_bytes(self.geometry, self.input, self.fat, first_relative)?;
        let count = bytes.len() / PAGE_DATA;
        for index in 0..count {
            if self.files.len() >= MAX_DIR_ENTRIES {
                return Err(damaged("the card has too many directory entries", 0));
            }
            let Some(entry) = bytes.get(
                index.saturating_mul(PAGE_DATA)..index.saturating_add(1).saturating_mul(PAGE_DATA),
            ) else {
                break;
            };
            if depth == 0 && index == 0 {
                continue;
            }
            let Some(mode) = u16_at(entry, 0) else {
                continue;
            };
            if mode & DF_EXISTS == 0 {
                continue;
            }
            let name = name_of(entry);
            if name.is_empty() {
                continue;
            }
            if mode & DF_DIRECTORY != 0 {
                if name == b"." || name == b".." {
                    continue;
                }
                let Some(cluster) = u32_at(entry, DIR_CLUSTER_OFFSET) else {
                    continue;
                };
                let mut path = prefix.to_vec();
                if !path.is_empty() {
                    path.push(b'/');
                }
                path.extend_from_slice(&name);
                self.walk(cluster, &path, depth + 1)?;
                continue;
            }
            if mode & DF_FILE == 0 {
                continue;
            }
            let Some(length) =
                u32_at(entry, DIR_LENGTH_OFFSET).and_then(|value| usize::try_from(value).ok())
            else {
                continue;
            };
            let Some(cluster) = u32_at(entry, DIR_CLUSTER_OFFSET) else {
                continue;
            };
            if length == 0 || cluster == FAT_END {
                continue;
            }
            let chain = self.fat.chain(cluster)?;
            let capacity = chain.len().saturating_mul(self.geometry.cluster_size());
            if length > capacity {
                return Err(damaged(
                    format!("a file is {length} bytes but its clusters hold only {capacity} bytes"),
                    0,
                ));
            }
            let (entry_page, entry_offset) =
                locate(self.geometry, &pages, index.saturating_mul(PAGE_DATA))?;
            let mut path = prefix.to_vec();
            if !path.is_empty() {
                path.push(b'/');
            }
            path.extend_from_slice(&name);
            self.files.push(FileEntry {
                path,
                cluster,
                length,
                capacity,
                entry_page,
                entry_offset,
            });
        }
        Ok(())
    }
}

fn files_of(geometry: &Geometry, input: &[u8], fat: &Fat) -> Result<Vec<FileEntry>> {
    let Some(cluster) = fat.alloc_offset.checked_add(geometry.rootdir) else {
        return Err(damaged("the root directory cluster overflows", 0));
    };
    let relative = cluster
        .checked_sub(fat.alloc_offset)
        .ok_or_else(|| damaged("the root directory cluster is invalid", 0))?;
    let mut walker = Walker::new(geometry, input, fat);
    walker.walk(relative, b"", 0)?;
    Ok(walker.files)
}

fn choose<'a>(files: &'a [FileEntry], hint: Option<&[u8]>) -> Option<&'a FileEntry> {
    if let Some(name) = hint
        && let Some(found) = files.iter().find(|file| {
            file.path == name || file.path.rsplit(|&byte| byte == b'/').next() == Some(name)
        })
    {
        return Some(found);
    }
    files.iter().max_by(|a, b| a.length.cmp(&b.length))
}

fn file_bytes(geometry: &Geometry, input: &[u8], fat: &Fat, file: &FileEntry) -> Result<Vec<u8>> {
    let chain = fat.chain(file.cluster)?;
    let mut bytes = Vec::with_capacity(chain.len().saturating_mul(geometry.cluster_size()));
    for &absolute in &chain {
        bytes.extend_from_slice(&read_cluster(geometry, input, absolute)?);
    }
    bytes.truncate(file.length);
    Ok(bytes)
}

fn write_file(
    geometry: &Geometry,
    source: &[u8],
    out: &mut [u8],
    fat: &Fat,
    file: &FileEntry,
    inner: &[u8],
) -> Result<()> {
    if inner.len() > file.capacity {
        return Err(write_error(format!(
            "the file has {} allocated bytes; the edited save is {} bytes",
            file.capacity,
            inner.len()
        )));
    }
    let chain = fat.chain(file.cluster)?;
    let cluster_size = geometry.cluster_size();
    let mut data = vec![0_u8; file.capacity];
    data.get_mut(..inner.len())
        .ok_or_else(|| write_error("the edited save is too large"))?
        .copy_from_slice(inner);
    for (cluster_index, &absolute) in chain.iter().enumerate() {
        let first = usize::try_from(absolute)
            .ok()
            .and_then(|value| value.checked_mul(geometry.pages_per_cluster))
            .ok_or_else(|| write_error("a file cluster is too large"))?;
        for page_in_cluster in 0..geometry.pages_per_cluster {
            let page = first
                .checked_add(page_in_cluster)
                .ok_or_else(|| write_error("a file page is too large"))?;
            let from = cluster_index
                .saturating_mul(cluster_size)
                .saturating_add(page_in_cluster.saturating_mul(geometry.page_len));
            let chunk = data
                .get(from..from.saturating_add(geometry.page_len))
                .ok_or_else(|| write_error("a file page is outside the data"))?;
            write_page(geometry, out, page, chunk)?;
        }
    }
    let start = geometry
        .page_start(file.entry_page)
        .ok_or_else(|| write_error("the directory page is too large"))?;
    let mut entry = source
        .get(start..start.saturating_add(geometry.page_len))
        .ok_or_else(|| write_error("the directory page is cut short"))?
        .to_vec();
    let text = u32::try_from(inner.len())
        .map_err(|_| write_error("the edited save is too large"))?
        .to_le_bytes();
    let field = entry
        .get_mut(
            file.entry_offset.saturating_add(DIR_LENGTH_OFFSET)
                ..file
                    .entry_offset
                    .saturating_add(DIR_LENGTH_OFFSET)
                    .saturating_add(4),
        )
        .ok_or_else(|| write_error("the directory entry is cut short"))?;
    field.copy_from_slice(&text);
    write_page(geometry, out, file.entry_page, &entry)?;
    Ok(())
}

impl Layer for Ps2CardLayer {
    fn id(&self) -> &'static str {
        "container.ps2-card"
    }

    fn sniff(&self, input: &[u8], ctx: &SniffCtx<'_>) -> Confidence {
        let Ok(geometry) = parse_superblock(input) else {
            return Confidence::NONE;
        };
        if geometry.ecc && check_page_ecc(&geometry, input, 0).is_err() {
            return Confidence::NONE;
        }
        let Ok(fat) = parse_fat(&geometry, input) else {
            return Confidence::NONE;
        };
        if files_of(&geometry, input, &fat).is_err() {
            return Confidence::NONE;
        }
        let named = ctx.file_name.is_some_and(|name| {
            let lower = name.to_ascii_lowercase();
            [".ps2", ".mcd", ".psm", ".vm2", ".bin"]
                .iter()
                .any(|extension| lower.ends_with(extension))
        });
        Confidence::new(if named { 1.0 } else { 0.95 })
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<(Vec<u8>, Params)> {
        ctx.limits().check_input(len_u64(input.len()))?;
        let geometry = parse_superblock(input)?;
        let fat = parse_fat(&geometry, input)?;
        let files = files_of(&geometry, input, &fat)?;
        let hint = ctx.hints().opt_bytes(ENTRY_PARAM)?;
        let file = choose(&files, hint).ok_or_else(|| {
            Error::new(
                PS2_NO_PAYLOAD,
                "the memory card holds no non-empty save file",
            )
        })?;
        let data = file_bytes(&geometry, input, &fat, file)?;
        ctx.limits()
            .check_output(len_u64(input.len()), len_u64(data.len()))?;
        let mut params = Params::new();
        params.set(ENTRY_PARAM, file.path.clone());
        params.set(
            ENTRY_PAGE_PARAM,
            i64::try_from(file.entry_page).unwrap_or(i64::MAX),
        );
        params.set(
            ENTRY_OFFSET_PARAM,
            i64::try_from(file.entry_offset).unwrap_or(i64::MAX),
        );
        keep_source(&mut params, input, &data);
        Ok((data, params))
    }

    fn encode(&self, inner: &[u8], params: &Params) -> Result<Vec<u8>> {
        if let Some(source) = source_if_unchanged(inner, params)? {
            return Ok(source);
        }
        let source = params.bytes(super::SOURCE_PARAM)?;
        let path = params.bytes(ENTRY_PARAM)?;
        let entry_page = params
            .opt_int(ENTRY_PAGE_PARAM)?
            .and_then(|value| usize::try_from(value).ok());
        let entry_offset = params
            .opt_int(ENTRY_OFFSET_PARAM)?
            .and_then(|value| usize::try_from(value).ok());
        let geometry = parse_superblock(source)?;
        let fat = parse_fat(&geometry, source)?;
        let files = files_of(&geometry, source, &fat)?;
        let file = files
            .iter()
            .find(|candidate| {
                Some(candidate.entry_page) == entry_page
                    && Some(candidate.entry_offset) == entry_offset
            })
            .or_else(|| files.iter().find(|candidate| candidate.path == path))
            .ok_or_else(|| {
                Error::new(
                    codes::CORE_INVALID_PARAM,
                    "the parameter \"entry\" names no save file of the card",
                )
            })?;
        let mut out = source.to_vec();
        write_file(&geometry, source, &mut out, &fat, file, inner)?;
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use saveprism_core::detect::{DetectOptions, detect};
    use saveprism_core::guard::{Verdict, check_round_trip};
    use saveprism_core::{Limits, pipeline};

    use super::*;
    use crate::register_all;

    fn fixture(relative: &str) -> Vec<u8> {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/synthetic/console")
            .join(relative);
        std::fs::read(path).unwrap()
    }

    fn decode(input: &[u8]) -> (Vec<u8>, Params) {
        let limits = Limits::default();
        let mut ctx = DecodeCtx::new(&limits);
        Ps2CardLayer.decode(input, &mut ctx).unwrap()
    }

    #[test]
    fn ecc_values_match_the_documented_hamming_example() {
        // Hamming code of an all-zero chunk is all ones after the final inversion, except the
        // unused bits that are masked off.
        assert_eq!(hamming_ecc(&[0_u8; 128]), [0x77, 0x7F, 0x7F]);
        let mut data = [0_u8; 128];
        if let Some(first) = data.first_mut() {
            *first = 1;
        }
        let code = hamming_ecc(&data);
        assert_ne!(code, [0x77, 0x7F, 0x7F]);
    }

    #[test]
    fn card_fixture_round_trips_and_picks_the_save() {
        let bytes = fixture("ps2/card.ps2");
        let (inner, params) = decode(&bytes);
        assert_eq!(
            params.bytes(ENTRY_PARAM).unwrap(),
            b"BASLUS-00001/save.json"
        );
        assert!(inner.starts_with(b"{"));
        assert_eq!(Ps2CardLayer.encode(&inner, &params).unwrap(), bytes);
    }

    #[test]
    fn data_only_fixture_round_trips() {
        let bytes = fixture("ps2/card-raw.ps2");
        let (inner, params) = decode(&bytes);
        assert!(inner.starts_with(b"{"));
        assert_eq!(Ps2CardLayer.encode(&inner, &params).unwrap(), bytes);
    }

    #[test]
    fn editing_a_save_rewrites_only_its_pages_and_their_ecc() {
        let bytes = fixture("ps2/card.ps2");
        let (mut inner, params) = decode(&bytes);
        let position = inner
            .iter()
            .position(|&byte| byte == b'1')
            .expect("a digit in the JSON");
        inner[position] = b'7';
        let written = Ps2CardLayer.encode(&inner, &params).unwrap();
        assert_eq!(written.len(), bytes.len());
        let (read_back, _) = decode(&written);
        assert_eq!(read_back, inner);
        let changed = bytes
            .iter()
            .zip(written.iter())
            .filter(|(a, b)| a != b)
            .count();
        assert!(changed > 0);
        assert!(changed < bytes.len() / 2);
        let geometry = parse_superblock(&written).unwrap();
        for page in 0..geometry.total_pages {
            check_page_ecc(&geometry, &written, page).unwrap();
        }
    }

    #[test]
    fn detection_reaches_the_json_payload() {
        let bytes = fixture("ps2/card.ps2");
        let mut reg = Registry::default();
        register_all(&mut reg);
        let detection =
            detect(&bytes, &reg, &Limits::default(), &DetectOptions::default()).unwrap();
        assert_eq!(
            detection.best.recipe.kinds(),
            ["container.ps2-card", "format.json"]
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
    fn random_bytes_are_not_a_card() {
        let fake = vec![0_u8; 528 * 64];
        assert_eq!(
            Ps2CardLayer.sniff(&fake, &SniffCtx::default()),
            Confidence::NONE
        );
    }
}
