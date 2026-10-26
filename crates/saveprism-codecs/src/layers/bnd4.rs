//! `container.bnd4`: FromSoftware BND4 binders (`.sl2`, `.bnd`).
//!
//! # Format
//!
//! A BND4 file starts with the ASCII magic `BND4`. The header is 0x40 bytes: a big-endian
//! switch, a file count, the 8-byte version text, the per-entry header size, the offset where
//! the headers end, the `unicode` flag, the `format` bit field and, for extended binders, the
//! offset of the name hash table. Entry headers follow at 0x40: flags, compressed size, an
//! optional uncompressed size, the data offset (32- or 64-bit), an optional id and name offset.
//! Names are UTF-16LE when `unicode` is set, otherwise Shift-JIS. Entry data is stored raw or
//! wrapped in a DCX container (`DCX\0` + zlib `DFLT`).
//!
//! The layout was verified against the MIT-licensed SoulsFormats project
//! (<https://github.com/JKAnderson/SoulsFormats>, `Binder/BND4/BND4.cs`,
//! `Binder/BinderFileHeader.cs`, `Binder/Binder.cs`, `Formats/DCX.cs`) and the public souls
//! modding notes it describes. No code was copied; only the field layout and the documented DCX
//! constants are used.
//!
//! # What the container returns
//!
//! One entry, chosen by the `entry` hint (a name) or the largest entry whose bytes another codec
//! or layer recognises. The entry name and its header offset are kept in the parameters.
//!
//! # Exact writing and read-only cases
//!
//! An unchanged payload is returned from the saved original bytes. An edited raw entry is written
//! back when it fits in the space the entry already occupies; only its size fields change and all
//! other bytes stay identical. An edited DCX/zlib entry is compressed again with the same DFLT
//! header; the result is a valid binder but not necessarily the same zlib bytes. Entries wrapped
//! in DCX `EDGE`, `KRAK` or `ZSTD`, and entries whose offsets or tables do not fit, are returned
//! as their stored bytes (read-only) with a note in the `note` parameter.

use std::sync::Arc;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    Confidence, DecodeCtx, Error, ErrorCode, Layer, Params, Registry, Result, SniffCtx, codes,
};

use super::{inner_is_known, keep_source, source_if_unchanged};

/// The binder is damaged or cut short.
pub const BND4_DAMAGED: ErrorCode = ErrorCode::new("SP-BND-001");
/// No entry holds something another codec recognises.
pub const BND4_NO_PAYLOAD: ErrorCode = ErrorCode::new("SP-BND-002");
/// The edited entry cannot be written back into the binder.
pub const BND4_WRITE: ErrorCode = ErrorCode::new("SP-BND-003");

/// Parameter: raw name of the chosen entry.
pub const ENTRY_PARAM: &str = "entry";
/// Parameter: header offset of the chosen entry.
pub const ENTRY_OFFSET_PARAM: &str = "entry_offset";
/// Parameter: why the chosen entry is read-only, when it is.
pub const NOTE_PARAM: &str = "note";

const HEADER_LEN: usize = 0x40;
const MAGIC: &[u8; 4] = b"BND4";
const DCX_MAGIC: &[u8; 4] = b"DCX\0";
const DCX_ZLIB_START: usize = 0x4C;

const FMT_IDS: u8 = 0x02;
const FMT_NAMES1: u8 = 0x04;
const FMT_NAMES2: u8 = 0x08;
const FMT_LONG_OFFSETS: u8 = 0x10;
const FMT_COMPRESSION: u8 = 0x20;

/// Registers the BND4 container layer.
pub fn register(reg: &mut Registry) {
    reg.add_layer(Arc::new(Bnd4Layer));
}

/// The `container.bnd4` layer.
#[derive(Clone, Copy, Debug, Default)]
pub struct Bnd4Layer;

#[derive(Clone, Copy, Debug)]
struct Reader<'a> {
    bytes: &'a [u8],
    big_endian: bool,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8], big_endian: bool) -> Self {
        Self { bytes, big_endian }
    }

    fn u8(&self, offset: usize) -> Option<u8> {
        self.bytes.get(offset).copied()
    }

    fn u32(&self, offset: usize) -> Option<u32> {
        let raw = self.bytes.get(offset..offset.checked_add(4)?)?;
        let four = <[u8; 4]>::try_from(raw).ok()?;
        Some(if self.big_endian {
            u32::from_be_bytes(four)
        } else {
            u32::from_le_bytes(four)
        })
    }

    fn i32(&self, offset: usize) -> Option<i32> {
        self.u32(offset).map(|value| value as i32)
    }

    fn i64(&self, offset: usize) -> Option<i64> {
        let raw = self.bytes.get(offset..offset.checked_add(8)?)?;
        let eight = <[u8; 8]>::try_from(raw).ok()?;
        Some(if self.big_endian {
            i64::from_be_bytes(eight)
        } else {
            i64::from_le_bytes(eight)
        })
    }

    fn field_u64(&self, offset: usize, long: bool) -> Option<u64> {
        if long {
            self.i64(offset).and_then(|value| u64::try_from(value).ok())
        } else {
            self.u32(offset).map(u64::from)
        }
    }
}

#[derive(Clone, Debug)]
struct Entry {
    name: Vec<u8>,
    header: usize,
    compressed_size_field: usize,
    uncompressed_size_field: Option<usize>,
    compressed: bool,
    compressed_size: usize,
    data_offset: usize,
    capacity: usize,
}

#[derive(Clone, Debug)]
struct Binder {
    entries: Vec<Entry>,
}

fn damaged(message: impl Into<String>, offset: usize) -> Error {
    Error::new(BND4_DAMAGED, message).with_offset(len_u64(offset))
}

fn write_error(message: impl Into<String>) -> Error {
    Error::new(BND4_WRITE, message)
}

fn reverse_bits(value: u8) -> u8 {
    let mut out = 0_u8;
    for bit in 0..8 {
        if value & (1 << bit) != 0 {
            out |= 1 << (7 - bit);
        }
    }
    out
}

fn read_format(raw: u8, bit_big_endian: bool) -> u8 {
    let reverse = bit_big_endian || ((raw & 1) != 0 && (raw & 0x80) == 0);
    if reverse { raw } else { reverse_bits(raw) }
}

fn read_flags(raw: u8, bit_big_endian: bool) -> u8 {
    if bit_big_endian {
        raw
    } else {
        reverse_bits(raw)
    }
}

fn utf16_name(input: &[u8], offset: usize) -> Option<Vec<u8>> {
    let mut units = Vec::new();
    let mut at = offset;
    loop {
        let raw = input.get(at..at.checked_add(2)?)?;
        let unit = u16::from_le_bytes(<[u8; 2]>::try_from(raw).ok()?);
        if unit == 0 {
            break;
        }
        units.push(unit);
        at = at.checked_add(2)?;
        if units.len() > 32 * 1024 {
            return None;
        }
    }
    String::from_utf16(&units).ok().map(String::into_bytes)
}

fn shift_jis_name(input: &[u8], offset: usize) -> Option<Vec<u8>> {
    let end = input
        .get(offset..)?
        .iter()
        .position(|&byte| byte == 0)?
        .checked_add(offset)?;
    let raw = input.get(offset..end)?;
    let (text, _, _) = encoding_rs::SHIFT_JIS.decode(raw);
    Some(text.into_owned().into_bytes())
}

fn parse(input: &[u8]) -> Result<Binder> {
    if input.len() < HEADER_LEN {
        return Err(damaged("the BND4 is shorter than its header", 0));
    }
    if input.get(..4) != Some(MAGIC) {
        return Err(damaged("the file has no BND4 magic", 0));
    }
    let big_endian = input.get(0x09).copied().unwrap_or(0) != 0;
    let bit_big_endian = input.get(0x0A).copied().unwrap_or(1) == 0;
    let reader = Reader::new(input, big_endian);
    let file_count = reader
        .i32(0x0C)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| damaged("the file count is missing", 0x0C))?;
    let file_header_size = reader
        .i64(0x20)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| damaged("the file header size is missing", 0x20))?;
    let _headers_end = reader.i64(0x28);
    let unicode = input.get(0x30).copied().unwrap_or(0) != 0;
    let format = read_format(input.get(0x31).copied().unwrap_or(0), bit_big_endian);
    if file_header_size < 0x10 {
        return Err(damaged(
            format!("the file header size 0x{file_header_size:X} is too small"),
            0x20,
        ));
    }
    let has_ids = format & FMT_IDS != 0;
    let has_names = format & (FMT_NAMES1 | FMT_NAMES2) != 0;
    let has_compression = format & FMT_COMPRESSION != 0;
    let long_offsets = format & FMT_LONG_OFFSETS != 0;
    if file_count > 1_000_000 {
        return Err(damaged("the binder has too many entries", 0x0C));
    }
    let table_end = HEADER_LEN
        .checked_add(file_count.saturating_mul(file_header_size))
        .ok_or_else(|| damaged("the entry header table overflows", 0x0C))?;
    if table_end > input.len() {
        return Err(damaged("the entry header table lies beyond the file", 0x0C));
    }

    let mut entries = Vec::with_capacity(file_count);
    for index in 0..file_count {
        let header = HEADER_LEN
            .checked_add(index.saturating_mul(file_header_size))
            .ok_or_else(|| damaged("an entry header offset overflows", HEADER_LEN))?;
        let mut at = header;
        let raw_flags = reader
            .u8(at)
            .ok_or_else(|| damaged("an entry header is cut short", at))?;
        let flags = read_flags(raw_flags, bit_big_endian);
        at = at
            .checked_add(0x08)
            .ok_or_else(|| damaged("an entry header offset overflows", at))?;
        let compressed_size_field = at;
        let compressed_size = reader
            .i64(at)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| damaged("an entry has no compressed size", at))?;
        at = at
            .checked_add(8)
            .ok_or_else(|| damaged("an entry header offset overflows", at))?;
        let uncompressed_size_field = if has_compression {
            let field = at;
            at = at
                .checked_add(8)
                .ok_or_else(|| damaged("an entry header offset overflows", at))?;
            Some(field)
        } else {
            None
        };
        let data_offset = reader
            .field_u64(at, long_offsets)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| damaged("an entry has no data offset", at))?;
        at = at
            .checked_add(if long_offsets { 8 } else { 4 })
            .ok_or_else(|| damaged("an entry header offset overflows", at))?;
        if has_ids {
            at = at
                .checked_add(4)
                .ok_or_else(|| damaged("an entry header offset overflows", at))?;
        }
        let mut name = Vec::new();
        if has_names {
            let name_offset = reader
                .u32(at)
                .and_then(|value| usize::try_from(value).ok())
                .ok_or_else(|| damaged("an entry has no name offset", at))?;
            at = at
                .checked_add(4)
                .ok_or_else(|| damaged("an entry header offset overflows", at))?;
            if name_offset != 0 {
                name = if unicode {
                    utf16_name(input, name_offset)
                } else {
                    shift_jis_name(input, name_offset)
                }
                .ok_or_else(|| damaged("an entry name is cut short or invalid", name_offset))?;
            }
        }
        if format == FMT_NAMES1 {
            at = at
                .checked_add(8)
                .ok_or_else(|| damaged("an entry header offset overflows", at))?;
        }
        let _ = at;
        let data_end = data_offset
            .checked_add(compressed_size)
            .ok_or_else(|| damaged("an entry data range overflows", data_offset))?;
        if data_end > input.len() {
            return Err(damaged("an entry's data lies beyond the file", data_offset));
        }
        entries.push(Entry {
            name,
            header,
            compressed_size_field,
            uncompressed_size_field,
            compressed: flags & 1 != 0,
            compressed_size,
            data_offset,
            capacity: compressed_size,
        });
    }

    let offsets: Vec<usize> = entries.iter().map(|entry| entry.data_offset).collect();
    for (index, offset) in offsets.iter().enumerate() {
        let next = offsets.get(index + 1).copied().unwrap_or(input.len());
        if next > *offset
            && let Some(slot) = entries.get_mut(index)
        {
            slot.capacity = next.saturating_sub(*offset);
        }
    }

    Ok(Binder { entries })
}

fn zlib_decompress(input: &[u8], ctx: &DecodeCtx<'_>) -> Result<Vec<u8>> {
    let decoder = flate2::read::ZlibDecoder::new(input);
    ctx.read_bounded(decoder, len_u64(input.len()), BND4_DAMAGED)
}

fn zlib_compress(input: &[u8]) -> Result<Vec<u8>> {
    use std::io::Write as _;
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
    encoder
        .write_all(input)
        .map_err(|source| write_error("zlib compression failed").with_source(source))?;
    encoder
        .finish()
        .map_err(|source| write_error("zlib compression failed").with_source(source))
}

fn decompress_entry(entry: &Entry, raw: &[u8], ctx: &DecodeCtx<'_>) -> Result<Option<Vec<u8>>> {
    if !entry.compressed {
        return Ok(Some(raw.to_vec()));
    }
    if raw.starts_with(DCX_MAGIC) {
        if raw.get(0x28..0x2C) != Some(b"DFLT") || raw.len() < DCX_ZLIB_START {
            return Ok(None);
        }
        return zlib_decompress(raw.get(DCX_ZLIB_START..).unwrap_or_default(), ctx).map(Some);
    }
    // Some binders store a bare zlib stream with the compressed flag set.
    if raw.first().copied() == Some(0x78) {
        return zlib_decompress(raw, ctx).map(Some);
    }
    Ok(None)
}

fn compress_entry(original: &[u8], inner: &[u8]) -> Result<Vec<u8>> {
    let compressed = zlib_compress(inner)?;
    if original.starts_with(DCX_MAGIC) && original.len() >= DCX_ZLIB_START {
        let mut header = original.get(..DCX_ZLIB_START).unwrap_or_default().to_vec();
        if let Some(field) = header.get_mut(0x1C..0x20) {
            field.copy_from_slice(
                &u32::try_from(inner.len())
                    .map_err(|_| write_error("the edited entry is too large"))?
                    .to_le_bytes(),
            );
        }
        if let Some(field) = header.get_mut(0x20..0x24) {
            field.copy_from_slice(
                &u32::try_from(compressed.len())
                    .map_err(|_| write_error("the compressed entry is too large"))?
                    .to_le_bytes(),
            );
        }
        let mut out = header;
        out.extend_from_slice(&compressed);
        Ok(out)
    } else {
        Ok(compressed)
    }
}

fn read_entry<'a>(input: &'a [u8], entry: &Entry) -> &'a [u8] {
    input
        .get(entry.data_offset..entry.data_offset.saturating_add(entry.compressed_size))
        .unwrap_or_default()
}

fn chosen<'a>(binder: &'a Binder, input: &[u8], hint: Option<&[u8]>) -> Option<&'a Entry> {
    if let Some(name) = hint
        && let Some(found) = binder.entries.iter().find(|entry| entry.name == name)
    {
        return Some(found);
    }
    let mut candidates: Vec<&Entry> = binder
        .entries
        .iter()
        .filter(|entry| entry.compressed_size > 0)
        .collect();
    candidates.sort_by_key(|entry| std::cmp::Reverse(entry.compressed_size));
    for entry in &candidates {
        if !entry.compressed && inner_is_known(read_entry(input, entry)) {
            return Some(entry);
        }
    }
    candidates.first().copied()
}

impl Layer for Bnd4Layer {
    fn id(&self) -> &'static str {
        "container.bnd4"
    }

    fn sniff(&self, input: &[u8], ctx: &SniffCtx<'_>) -> Confidence {
        if input.get(..4) != Some(MAGIC) {
            return Confidence::NONE;
        }
        let Ok(binder) = parse(input) else {
            return Confidence::NONE;
        };
        if binder.entries.is_empty() {
            return Confidence::NONE;
        }
        let named = ctx.file_name.is_some_and(|name| {
            let lower = name.to_ascii_lowercase();
            [".sl2", ".bnd", ".bhd", ".dcx"]
                .iter()
                .any(|end| lower.ends_with(end))
        });
        Confidence::new(if named { 1.0 } else { 0.95 })
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<(Vec<u8>, Params)> {
        ctx.limits().check_input(len_u64(input.len()))?;
        let binder = parse(input)?;
        let hint = ctx.hints().opt_bytes(ENTRY_PARAM)?;
        let entry = chosen(&binder, input, hint)
            .ok_or_else(|| Error::new(BND4_NO_PAYLOAD, "the binder holds no non-empty entry"))?;
        let raw = read_entry(input, entry);
        let mut params = Params::new();
        params.set(ENTRY_PARAM, entry.name.clone());
        params.set(
            ENTRY_OFFSET_PARAM,
            i64::try_from(entry.header).unwrap_or(i64::MAX),
        );
        match decompress_entry(entry, raw, ctx)? {
            Some(inner) => {
                ctx.limits()
                    .check_output(len_u64(input.len()), len_u64(inner.len()))?;
                keep_source(&mut params, input, &inner);
                Ok((inner, params))
            }
            None => {
                params.set(
                    NOTE_PARAM,
                    "the entry uses an unsupported compression (EDGE, KRAK or ZSTD); it is read-only",
                );
                keep_source(&mut params, input, raw);
                Ok((raw.to_vec(), params))
            }
        }
    }

    fn encode(&self, inner: &[u8], params: &Params) -> Result<Vec<u8>> {
        if let Some(source) = source_if_unchanged(inner, params)? {
            return Ok(source);
        }
        let source = params.bytes(super::SOURCE_PARAM)?;
        let name = params.bytes(ENTRY_PARAM)?;
        let offset = params
            .opt_int(ENTRY_OFFSET_PARAM)?
            .and_then(|value| usize::try_from(value).ok());
        let binder = parse(source)?;
        let entry = binder
            .entries
            .iter()
            .find(|entry| Some(entry.header) == offset)
            .or_else(|| binder.entries.iter().find(|entry| entry.name == name))
            .ok_or_else(|| {
                Error::new(
                    codes::CORE_INVALID_PARAM,
                    "the parameter \"entry\" names no entry of the binder",
                )
            })?;
        if params.opt_str(NOTE_PARAM)?.is_some() {
            return Err(write_error(
                "this entry uses a compression SavePrism cannot reproduce; it is read-only",
            ));
        }
        let original = read_entry(source, entry);
        let written = if entry.compressed {
            compress_entry(original, inner)?
        } else {
            inner.to_vec()
        };
        if written.len() > entry.capacity {
            return Err(write_error(format!(
                "the entry has {} bytes of room; the edited data needs {} bytes",
                entry.capacity,
                written.len()
            )));
        }
        let mut out = source.to_vec();
        let start = entry.data_offset;
        let end = start
            .checked_add(written.len())
            .ok_or_else(|| write_error("the entry data range overflows"))?;
        out.get_mut(start..end)
            .ok_or_else(|| write_error("the binder is cut short"))?
            .copy_from_slice(&written);
        write_i64(source, &mut out, entry.compressed_size_field, written.len())?;
        if entry.compressed
            && let Some(field) = entry.uncompressed_size_field
        {
            write_i64(source, &mut out, field, inner.len())?;
        }
        Ok(out)
    }
}

fn write_i64(source: &[u8], out: &mut [u8], offset: usize, value: usize) -> Result<()> {
    let big_endian = source.get(0x09).copied().unwrap_or(0) != 0;
    let value = i64::try_from(value).map_err(|_| write_error("the value is too large"))?;
    let bytes = if big_endian {
        value.to_be_bytes()
    } else {
        value.to_le_bytes()
    };
    out.get_mut(offset..offset.saturating_add(8))
        .ok_or_else(|| write_error("the binder header is cut short"))?
        .copy_from_slice(&bytes);
    Ok(())
}

#[cfg(test)]
mod tests {
    use saveprism_core::detect::{DetectOptions, detect};
    use saveprism_core::guard::{Verdict, check_round_trip};
    use saveprism_core::{Limits, pipeline};

    use super::*;
    use crate::register_all;

    fn fixture(relative: &str) -> Vec<u8> {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/synthetic/archives")
            .join(relative);
        std::fs::read(path).unwrap()
    }

    fn decode(input: &[u8]) -> (Vec<u8>, Params) {
        let limits = Limits::default();
        let mut ctx = DecodeCtx::new(&limits);
        Bnd4Layer.decode(input, &mut ctx).unwrap()
    }

    #[test]
    fn fixture_round_trips_and_picks_the_json_entry() {
        let bytes = fixture("bnd4/save.sl2");
        let (inner, params) = decode(&bytes);
        assert_eq!(params.bytes(ENTRY_PARAM).unwrap(), b"user_data.json");
        assert!(inner.starts_with(b"{"));
        assert_eq!(Bnd4Layer.encode(&inner, &params).unwrap(), bytes);
    }

    #[test]
    fn editing_a_raw_entry_updates_only_its_size_and_bytes() {
        let bytes = fixture("bnd4/save.sl2");
        let (mut inner, params) = decode(&bytes);
        let position = inner.iter().position(|&byte| byte == b'1').unwrap();
        inner[position] = b'9';
        let written = Bnd4Layer.encode(&inner, &params).unwrap();
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
        let bytes = fixture("bnd4/save.sl2");
        let mut reg = Registry::default();
        register_all(&mut reg);
        let detection =
            detect(&bytes, &reg, &Limits::default(), &DetectOptions::default()).unwrap();
        assert_eq!(
            detection.best.recipe.kinds(),
            ["container.bnd4", "format.json"]
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
    fn damaged_binders_give_the_code() {
        let limits = Limits::default();
        let mut ctx = DecodeCtx::new(&limits);
        assert_eq!(
            Bnd4Layer.decode(b"BND4", &mut ctx).unwrap_err().code(),
            BND4_DAMAGED
        );
        let bytes = fixture("bnd4/save.sl2");
        let mut cut = bytes;
        cut.truncate(0x60);
        let mut ctx = DecodeCtx::new(&limits);
        assert_eq!(
            Bnd4Layer.decode(&cut, &mut ctx).unwrap_err().code(),
            BND4_DAMAGED
        );
    }

    #[test]
    fn random_bytes_are_not_a_binder() {
        assert_eq!(
            Bnd4Layer.sniff(&vec![0_u8; 512], &SniffCtx::default()),
            Confidence::NONE
        );
    }
}
