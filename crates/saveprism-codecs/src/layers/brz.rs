//! `container.brz`: Brickadia `.brz` world archives (the BRDB container).
//!
//! # Format
//!
//! A `.brz` archive starts with the three ASCII bytes `BRZ`, a one-byte version (0), a one-byte
//! index method (0 = stored, 1 = Zstandard), the uncompressed and stored index sizes, and the
//! BLAKE3 hash of the uncompressed index. The index lists folders (parent id, name), files
//! (parent id, content id, name), and blobs (method, uncompressed size, stored size, BLAKE3 hash
//! of the uncompressed bytes). Blob data follows the index in order.
//!
//! The layout was verified against the MIT-licensed `Meshiest/brs-js` project
//! (<https://github.com/Meshiest/brs-js>, `src/brdb/brz.ts`) and the MIT `Brickadia/brs` sources
//! where they overlap. No code was copied; only the documented field order and constants are used.
//!
//! # What the container returns
//!
//! One file from the archive, chosen by the `entry` hint (its path) or the largest file whose
//! bytes another codec or layer recognises. Zstandard blobs are decompressed; the index is
//! verified against its BLAKE3 hash.
//!
//! # Exact writing and read-only cases
//!
//! An unchanged payload is returned from the saved original bytes. An edited stored blob is
//! written back only when another file does not share it and its length does not change; its
//! BLAKE3 hash in the index and the index hash in the header are recomputed. Zstandard blobs,
//! shared blobs and length changes are read-only with a note in the `note` parameter.

use std::sync::Arc;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    Confidence, DecodeCtx, Error, ErrorCode, Layer, Params, Registry, Result, SniffCtx, codes,
};

use super::{inner_is_known, keep_source, source_if_unchanged};

/// The archive is damaged or cut short.
pub const BRZ_DAMAGED: ErrorCode = ErrorCode::new("SP-BRZ-001");
/// No file holds something another codec recognises.
pub const BRZ_NO_PAYLOAD: ErrorCode = ErrorCode::new("SP-BRZ-002");
/// The edited file cannot be written back exactly.
pub const BRZ_WRITE: ErrorCode = ErrorCode::new("SP-BRZ-003");

/// Parameter: path of the chosen file.
pub const ENTRY_PARAM: &str = "entry";
/// Parameter: zero-based index of the chosen file.
pub const ENTRY_INDEX_PARAM: &str = "entry_index";
/// Parameter: why the chosen file is read-only, when it is.
pub const NOTE_PARAM: &str = "note";

const MAGIC: &[u8; 3] = b"BRZ";
const HEADER_LEN: usize = 45;
const HASH_LEN: usize = 32;
const METHOD_NONE: u8 = 0;
const METHOD_ZSTD: u8 = 1;

/// Registers the BRZ container layer.
pub fn register(reg: &mut Registry) {
    reg.add_layer(Arc::new(BrzLayer));
}

/// The `container.brz` layer.
#[derive(Clone, Copy, Debug, Default)]
pub struct BrzLayer;

#[derive(Clone, Debug)]
struct Folder {
    parent: i32,
    name: String,
}

#[derive(Clone, Debug)]
struct FileEntry {
    parent: i32,
    content: i32,
    name: String,
}

#[derive(Clone, Debug)]
struct Blob {
    method: u8,
    size: usize,
    stored: usize,
    hash_offset: usize,
    data_offset: usize,
}

#[derive(Clone, Debug)]
struct Archive {
    index_method: u8,
    index_size: usize,
    index_stored: usize,
    index_hash_offset: usize,
    index: Vec<u8>,
    folders: Vec<Folder>,
    files: Vec<FileEntry>,
    blobs: Vec<Blob>,
    blob_hash_base: usize,
}

fn damaged(message: impl Into<String>, offset: usize) -> Error {
    Error::new(BRZ_DAMAGED, message).with_offset(len_u64(offset))
}

fn write_error(message: impl Into<String>) -> Error {
    Error::new(BRZ_WRITE, message)
}

struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }

    fn take(&mut self, len: usize) -> Option<&'a [u8]> {
        let end = self.at.checked_add(len)?;
        let out = self.bytes.get(self.at..end)?;
        self.at = end;
        Some(out)
    }

    fn u16(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes(<[u8; 2]>::try_from(self.take(2)?).ok()?))
    }

    fn i32(&mut self) -> Option<i32> {
        Some(i32::from_le_bytes(<[u8; 4]>::try_from(self.take(4)?).ok()?))
    }

    fn usize_i32(&mut self) -> Option<usize> {
        usize::try_from(self.i32()?).ok()
    }
}

fn index_bytes(input: &[u8]) -> Result<(u8, usize, usize, usize, Vec<u8>)> {
    if input.get(..3) != Some(MAGIC) {
        return Err(damaged("the file has no BRZ magic", 0));
    }
    if input.get(3).copied() != Some(0) {
        return Err(damaged("the BRZ version is not supported", 3));
    }
    let Some(method) = input.get(4).copied() else {
        return Err(damaged("the BRZ index method is missing", 4));
    };
    if method != METHOD_NONE && method != METHOD_ZSTD {
        return Err(damaged(
            format!("the BRZ index method {method} is not supported"),
            4,
        ));
    }
    let size = i32::from_le_bytes(
        input
            .get(5..9)
            .and_then(|raw| <[u8; 4]>::try_from(raw).ok())
            .ok_or_else(|| damaged("the BRZ index size is missing", 5))?,
    );
    let stored = i32::from_le_bytes(
        input
            .get(9..13)
            .and_then(|raw| <[u8; 4]>::try_from(raw).ok())
            .ok_or_else(|| damaged("the BRZ stored index size is missing", 9))?,
    );
    let size = usize::try_from(size).map_err(|_| damaged("the BRZ index size is negative", 5))?;
    let stored =
        usize::try_from(stored).map_err(|_| damaged("the BRZ stored index size is negative", 9))?;
    let hash_end = HEADER_LEN;
    if input.len() < hash_end {
        return Err(damaged("the BRZ header is cut short", 0));
    }
    let data_end = hash_end
        .checked_add(stored)
        .ok_or_else(|| damaged("the BRZ index size overflows", 9))?;
    let raw = input
        .get(hash_end..data_end)
        .ok_or_else(|| damaged("the BRZ index lies beyond the file", hash_end))?;
    let index = if method == METHOD_NONE {
        if size != stored {
            return Err(damaged("a stored BRZ index has different size fields", 5));
        }
        raw.to_vec()
    } else {
        let limits = saveprism_core::Limits::default();
        let ctx = DecodeCtx::new(&limits);
        let decoder = zstd::stream::read::Decoder::new(raw).map_err(|source| {
            Error::new(BRZ_DAMAGED, "BRZ index decompression failed").with_source(source)
        })?;
        ctx.read_bounded(decoder, len_u64(input.len()), BRZ_DAMAGED)?
    };
    if index.len() != size {
        return Err(damaged(
            format!(
                "the BRZ index has {} bytes but its header says {size}",
                index.len()
            ),
            5,
        ));
    }
    let Some(expected) = input.get(13..13 + HASH_LEN) else {
        return Err(damaged("the BRZ index hash is missing", 13));
    };
    if blake3::hash(&index).as_bytes() != expected {
        return Err(damaged("the BRZ index hash does not match", 13));
    }
    Ok((method, size, stored, hash_end, index))
}

type IndexParts = (Vec<Folder>, Vec<FileEntry>, Vec<Blob>, usize);

fn parse_index(index: &[u8], data_start: usize) -> Result<IndexParts> {
    let mut cursor = Cursor::new(index);
    let folder_count = cursor
        .usize_i32()
        .ok_or_else(|| damaged("the BRZ folder count is missing", 0))?;
    let file_count = cursor
        .usize_i32()
        .ok_or_else(|| damaged("the BRZ file count is missing", 4))?;
    let blob_count = cursor
        .usize_i32()
        .ok_or_else(|| damaged("the BRZ blob count is missing", 8))?;
    if folder_count > 1_000_000 || file_count > 10_000_000 || blob_count > 10_000_000 {
        return Err(damaged("the BRZ index is too large", 0));
    }
    let mut folder_parent = Vec::with_capacity(folder_count);
    for _ in 0..folder_count {
        folder_parent.push(
            cursor
                .i32()
                .ok_or_else(|| damaged("a BRZ folder parent is missing", cursor.at))?,
        );
    }
    let mut folder_lens = Vec::with_capacity(folder_count);
    for _ in 0..folder_count {
        folder_lens.push(usize::from(cursor.u16().ok_or_else(|| {
            damaged("a BRZ folder name length is missing", cursor.at)
        })?));
    }
    let mut folders = Vec::with_capacity(folder_count);
    for (index, len) in folder_lens.into_iter().enumerate() {
        let raw = cursor
            .take(len)
            .ok_or_else(|| damaged("a BRZ folder name is cut short", cursor.at))?;
        let name = String::from_utf8(raw.to_vec())
            .map_err(|_| damaged("a BRZ folder name is not UTF-8", cursor.at))?;
        folders.push(Folder {
            parent: folder_parent.get(index).copied().unwrap_or(-1),
            name,
        });
    }
    let mut file_parent = Vec::with_capacity(file_count);
    for _ in 0..file_count {
        file_parent.push(
            cursor
                .i32()
                .ok_or_else(|| damaged("a BRZ file parent is missing", cursor.at))?,
        );
    }
    let mut file_content = Vec::with_capacity(file_count);
    for _ in 0..file_count {
        file_content.push(
            cursor
                .i32()
                .ok_or_else(|| damaged("a BRZ file content id is missing", cursor.at))?,
        );
    }
    let mut file_lens = Vec::with_capacity(file_count);
    for _ in 0..file_count {
        file_lens.push(usize::from(cursor.u16().ok_or_else(|| {
            damaged("a BRZ file name length is missing", cursor.at)
        })?));
    }
    let mut files = Vec::with_capacity(file_count);
    for (index, len) in file_lens.into_iter().enumerate() {
        let raw = cursor
            .take(len)
            .ok_or_else(|| damaged("a BRZ file name is cut short", cursor.at))?;
        let name = String::from_utf8(raw.to_vec())
            .map_err(|_| damaged("a BRZ file name is not UTF-8", cursor.at))?;
        files.push(FileEntry {
            parent: file_parent.get(index).copied().unwrap_or(-1),
            content: file_content.get(index).copied().unwrap_or(-1),
            name,
        });
    }
    let methods = cursor
        .take(blob_count)
        .ok_or_else(|| damaged("the BRZ blob methods are cut short", cursor.at))?
        .to_vec();
    let mut sizes = Vec::with_capacity(blob_count);
    for _ in 0..blob_count {
        sizes.push(
            cursor
                .usize_i32()
                .ok_or_else(|| damaged("a BRZ blob size is missing", cursor.at))?,
        );
    }
    let mut stored = Vec::with_capacity(blob_count);
    for _ in 0..blob_count {
        stored.push(
            cursor
                .usize_i32()
                .ok_or_else(|| damaged("a BRZ stored blob size is missing", cursor.at))?,
        );
    }
    let hash_base = cursor.at;
    let hashes = cursor
        .take(
            blob_count
                .checked_mul(HASH_LEN)
                .ok_or_else(|| damaged("the BRZ blob hashes overflow", cursor.at))?,
        )
        .ok_or_else(|| damaged("the BRZ blob hashes are cut short", cursor.at))?;
    if cursor.at != index.len() {
        return Err(damaged(
            "the BRZ index has trailing bytes after the blob hashes",
            cursor.at,
        ));
    }
    let mut blobs = Vec::with_capacity(blob_count);
    let mut offset = data_start;
    for index in 0..blob_count {
        let method = methods.get(index).copied().unwrap_or(255);
        if method != METHOD_NONE && method != METHOD_ZSTD {
            return Err(damaged(
                format!("the BRZ blob method {method} is not supported"),
                hash_base,
            ));
        }
        let size = sizes.get(index).copied().unwrap_or(0);
        let size_stored = stored.get(index).copied().unwrap_or(0);
        let end = offset
            .checked_add(size_stored)
            .ok_or_else(|| damaged("a BRZ blob range overflows", offset))?;
        blobs.push(Blob {
            method,
            size,
            stored: size_stored,
            hash_offset: hash_base
                .checked_add(index.saturating_mul(HASH_LEN))
                .ok_or_else(|| damaged("a BRZ blob hash offset overflows", hash_base))?,
            data_offset: offset,
        });
        offset = end;
    }
    let _ = hashes;
    Ok((folders, files, blobs, offset))
}

fn parse(input: &[u8]) -> Result<Archive> {
    let (index_method, index_size, index_stored, index_hash_offset, index) = index_bytes(input)?;
    let data_start = HEADER_LEN
        .checked_add(index_stored)
        .ok_or_else(|| damaged("the BRZ data offset overflows", HEADER_LEN))?;
    let (folders, files, blobs, end) = parse_index(&index, data_start)?;
    if end > input.len() {
        return Err(damaged("a BRZ blob lies beyond the file", end));
    }
    let blob_hash_base = index
        .len()
        .checked_sub(blobs.len().saturating_mul(HASH_LEN))
        .ok_or_else(|| damaged("the BRZ blob hash table overflows", index.len()))?;
    Ok(Archive {
        index_method,
        index_size,
        index_stored,
        index_hash_offset,
        index,
        folders,
        files,
        blobs,
        blob_hash_base,
    })
}

impl Archive {
    fn folder_path(&self, id: i32, depth: usize) -> String {
        if id < 0 || depth > 32 {
            return String::new();
        }
        let Some(folder) = self.folders.get(usize::try_from(id).unwrap_or(usize::MAX)) else {
            return String::new();
        };
        let parent = self.folder_path(folder.parent, depth + 1);
        if parent.is_empty() {
            folder.name.clone()
        } else {
            format!("{parent}/{}", folder.name)
        }
    }

    fn file_path(&self, index: usize) -> String {
        let Some(file) = self.files.get(index) else {
            return String::new();
        };
        let parent = self.folder_path(file.parent, 0);
        if parent.is_empty() {
            file.name.clone()
        } else {
            format!("{parent}/{}", file.name)
        }
    }

    fn blob(&self, file: &FileEntry) -> Option<&Blob> {
        usize::try_from(file.content)
            .ok()
            .and_then(|index| self.blobs.get(index))
    }

    fn blob_references(&self, blob_index: usize) -> usize {
        self.files
            .iter()
            .filter(|file| usize::try_from(file.content).ok() == Some(blob_index))
            .count()
    }
}

fn content(input: &[u8], archive: &Archive, blob: &Blob, ctx: &DecodeCtx<'_>) -> Result<Vec<u8>> {
    let end = blob
        .data_offset
        .checked_add(blob.stored)
        .ok_or_else(|| damaged("a BRZ blob range overflows", blob.data_offset))?;
    let raw = input
        .get(blob.data_offset..end)
        .ok_or_else(|| damaged("a BRZ blob lies beyond the file", blob.data_offset))?;
    let data = if blob.method == METHOD_NONE {
        raw.to_vec()
    } else {
        let decoder = zstd::stream::read::Decoder::new(raw).map_err(|source| {
            Error::new(BRZ_DAMAGED, "BRZ blob decompression failed").with_source(source)
        })?;
        ctx.read_bounded(decoder, len_u64(input.len()), BRZ_DAMAGED)?
    };
    if data.len() != blob.size {
        return Err(damaged(
            format!(
                "a BRZ blob has {} bytes but its index says {}",
                data.len(),
                blob.size
            ),
            blob.data_offset,
        ));
    }
    let expected = archive
        .index
        .get(blob.hash_offset..blob.hash_offset.saturating_add(HASH_LEN))
        .ok_or_else(|| damaged("a BRZ blob hash is missing", blob.hash_offset))?;
    if blake3::hash(&data).as_bytes() != expected {
        return Err(damaged("a BRZ blob hash does not match", blob.data_offset));
    }
    Ok(data)
}

fn chosen<'a>(
    archive: &'a Archive,
    input: &[u8],
    ctx: &DecodeCtx<'_>,
    hint: Option<&[u8]>,
) -> Option<(&'a FileEntry, usize, Vec<u8>)> {
    if let Some(path) = hint
        && let Some(index) = archive
            .files
            .iter()
            .enumerate()
            .find(|(index, _)| archive.file_path(*index).as_bytes() == path)
            .map(|(index, _)| index)
        && let Some(file) = archive.files.get(index)
        && let Some(blob) = archive.blob(file)
        && let Ok(bytes) = content(input, archive, blob, ctx)
    {
        return Some((file, index, bytes));
    }
    let mut order: Vec<usize> = (0..archive.files.len()).collect();
    order.sort_by_key(|&index| {
        std::cmp::Reverse(
            archive
                .files
                .get(index)
                .and_then(|file| archive.blob(file))
                .map_or(0, |blob| blob.size),
        )
    });
    let mut fallback = None;
    for index in order {
        let Some(file) = archive.files.get(index) else {
            continue;
        };
        let Some(blob) = archive.blob(file) else {
            continue;
        };
        let Ok(bytes) = content(input, archive, blob, ctx) else {
            continue;
        };
        if fallback.is_none() {
            fallback = Some((file, index, bytes.clone()));
        }
        if inner_is_known(&bytes) {
            return Some((file, index, bytes));
        }
    }
    fallback
}

impl Layer for BrzLayer {
    fn id(&self) -> &'static str {
        "container.brz"
    }

    fn sniff(&self, input: &[u8], ctx: &SniffCtx<'_>) -> Confidence {
        if input.get(..3) != Some(MAGIC) {
            return Confidence::NONE;
        }
        let Ok(_archive) = parse(input) else {
            return Confidence::NONE;
        };
        let named = ctx.file_name.is_some_and(|name| {
            let lower = name.to_ascii_lowercase();
            [".brz", ".brs"].iter().any(|end| lower.ends_with(end))
        });
        Confidence::new(if named { 1.0 } else { 0.95 })
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<(Vec<u8>, Params)> {
        ctx.limits().check_input(len_u64(input.len()))?;
        let archive = parse(input)?;
        let hint = ctx.hints().opt_bytes(ENTRY_PARAM)?;
        let (file, index, inner) = chosen(&archive, input, ctx, hint)
            .ok_or_else(|| Error::new(BRZ_NO_PAYLOAD, "the archive holds no usable file"))?;
        ctx.limits()
            .check_output(len_u64(input.len()), len_u64(inner.len()))?;
        let mut params = Params::new();
        params.set(ENTRY_PARAM, archive.file_path(index).into_bytes());
        params.set(ENTRY_INDEX_PARAM, i64::try_from(index).unwrap_or(i64::MAX));
        let blob_index = usize::try_from(file.content).ok();
        if let Some(blob_index) = blob_index
            && let Some(blob) = archive.blobs.get(blob_index)
        {
            if blob.method != METHOD_NONE {
                params.set(
                    NOTE_PARAM,
                    "the file is Zstandard-compressed; it is read-only",
                );
            } else if archive.blob_references(blob_index) > 1 {
                params.set(
                    NOTE_PARAM,
                    "the file's blob is shared by several files; it is read-only",
                );
            }
        }
        keep_source(&mut params, input, &inner);
        Ok((inner, params))
    }

    fn encode(&self, inner: &[u8], params: &Params) -> Result<Vec<u8>> {
        if let Some(source) = source_if_unchanged(inner, params)? {
            return Ok(source);
        }
        let source = params.bytes(super::SOURCE_PARAM)?;
        let index = params
            .opt_int(ENTRY_INDEX_PARAM)?
            .and_then(|value| usize::try_from(value).ok());
        let archive = parse(source)?;
        let file_index = index
            .filter(|value| *value < archive.files.len())
            .or_else(|| {
                let name = params.bytes(ENTRY_PARAM).ok()?;
                archive
                    .files
                    .iter()
                    .enumerate()
                    .find(|(index, _)| archive.file_path(*index).as_bytes() == name)
                    .map(|(index, _)| index)
            })
            .ok_or_else(|| {
                Error::new(
                    codes::CORE_INVALID_PARAM,
                    "the parameter \"entry\" names no file of the archive",
                )
            })?;
        let file = archive
            .files
            .get(file_index)
            .ok_or_else(|| write_error("the file entry is missing"))?;
        let blob_index = usize::try_from(file.content)
            .map_err(|_| write_error("the file has no content blob"))?;
        let blob = archive
            .blobs
            .get(blob_index)
            .ok_or_else(|| write_error("the content blob is missing"))?;
        if params.opt_str(NOTE_PARAM)?.is_some() {
            return Err(write_error("this file is read-only"));
        }
        if blob.method != METHOD_NONE {
            return Err(write_error(
                "Zstandard-compressed files cannot be written back exactly",
            ));
        }
        if archive.blob_references(blob_index) > 1 {
            return Err(write_error(
                "the file's blob is shared by several files; it is read-only",
            ));
        }
        if inner.len() != blob.size {
            return Err(write_error(
                "the edited file has a different length; the blob offsets would change",
            ));
        }
        if archive.index_method != METHOD_NONE {
            return Err(write_error("the BRZ index is compressed; it is read-only"));
        }
        let mut out = source.to_vec();
        let end = blob
            .data_offset
            .checked_add(inner.len())
            .ok_or_else(|| write_error("the blob range overflows"))?;
        out.get_mut(blob.data_offset..end)
            .ok_or_else(|| write_error("the archive is cut short"))?
            .copy_from_slice(inner);
        let mut index = archive.index.clone();
        let hash = blake3::hash(inner);
        index
            .get_mut(blob.hash_offset..blob.hash_offset.saturating_add(HASH_LEN))
            .ok_or_else(|| write_error("the blob hash is missing"))?
            .copy_from_slice(hash.as_bytes());
        let index_hash = blake3::hash(&index);
        out.get_mut(13..13 + HASH_LEN)
            .ok_or_else(|| write_error("the index hash is missing"))?
            .copy_from_slice(index_hash.as_bytes());
        let index_start = HEADER_LEN;
        let index_end = index_start
            .checked_add(index.len())
            .ok_or_else(|| write_error("the index range overflows"))?;
        out.get_mut(index_start..index_end)
            .ok_or_else(|| write_error("the archive is cut short"))?
            .copy_from_slice(&index);
        let _ = (
            archive.index_size,
            archive.index_stored,
            archive.index_hash_offset,
            archive.blob_hash_base,
        );
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
            .join("../../fixtures/synthetic/archives/brz/world.brz");
        std::fs::read(path).unwrap()
    }

    fn decode(input: &[u8]) -> (Vec<u8>, Params) {
        let limits = Limits::default();
        let mut ctx = DecodeCtx::new(&limits);
        BrzLayer.decode(input, &mut ctx).unwrap()
    }

    #[test]
    fn fixture_round_trips_and_picks_world_json() {
        let bytes = fixture();
        let (inner, params) = decode(&bytes);
        assert_eq!(params.bytes(ENTRY_PARAM).unwrap(), b"world.json");
        assert!(inner.starts_with(b"{"));
        assert_eq!(BrzLayer.encode(&inner, &params).unwrap(), bytes);
    }

    #[test]
    fn editing_a_stored_blob_updates_its_hash_and_the_index_hash() {
        let bytes = fixture();
        let (mut inner, params) = decode(&bytes);
        let position = inner.iter().position(|&byte| byte == b'1').unwrap();
        inner[position] = b'9';
        let written = BrzLayer.encode(&inner, &params).unwrap();
        assert_eq!(written.len(), bytes.len());
        let (again, _) = decode(&written);
        assert_eq!(again, inner);
        assert_ne!(written.get(13..45), bytes.get(13..45));
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
            ["container.brz", "format.json"]
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
            BrzLayer.decode(b"BRZ", &mut ctx).unwrap_err().code(),
            BRZ_DAMAGED
        );
        let bytes = fixture();
        let mut broken = bytes;
        if let Some(byte) = broken.get_mut(13) {
            *byte ^= 0xFF;
        }
        let mut ctx = DecodeCtx::new(&limits);
        assert_eq!(
            BrzLayer.decode(&broken, &mut ctx).unwrap_err().code(),
            BRZ_DAMAGED
        );
    }

    #[test]
    fn random_bytes_are_not_an_archive() {
        assert_eq!(
            BrzLayer.sniff(&vec![0_u8; 512], &SniffCtx::default()),
            Confidence::NONE
        );
    }
}
