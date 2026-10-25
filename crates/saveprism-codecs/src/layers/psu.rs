//! `container.psu`: PS2 save export used by EMS adapters and uLaunchELF.
//!
//! # Format
//!
//! A `.psu` is a flat sequence of entries. Each entry is a 512-byte header (little-endian type
//! `0x8427` directory / `0x8497` file, byte size, creation and modification times, name padded
//! to 448 bytes), followed by the file content and padding to a 1024-byte boundary of the
//! content. The first entry names the save folder; the next entries are `.`, `..` and the
//! folder's files. Layout and field meanings checked against the public MIT `pypsu` project and
//! the PS2 Save Tools `.psu` note.
//!
//! # What the container returns
//!
//! The largest file entry, or the file named by the `entry` hint. Its contents go to normal
//! detection, so a JSON/text game save opens as usual and an unknown binary can still be
//! inspected.
//!
//! # Exact writing and safety
//!
//! An unchanged payload is returned from the saved original bytes. After an edit the chosen
//! entry's header size and content are rewritten; its padding is kept when the size did not
//! change and zero-filled otherwise. Every other entry is copied byte for byte.

use std::sync::Arc;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    Confidence, DecodeCtx, Error, ErrorCode, Layer, Params, Registry, Result, SniffCtx, codes,
};

use super::{keep_source, source_if_unchanged};

/// The `.psu` file is damaged or cut short.
pub const PSU_DAMAGED: ErrorCode = ErrorCode::new("SP-PSU-001");
/// No file entry holds a non-empty file.
pub const PSU_NO_PAYLOAD: ErrorCode = ErrorCode::new("SP-PSU-002");
/// The edited save cannot be written back into the `.psu`.
pub const PSU_WRITE: ErrorCode = ErrorCode::new("SP-PSU-003");

/// Parameter: name of the chosen file entry.
pub const ENTRY_PARAM: &str = "entry";
/// Parameter: byte offset of the chosen entry header.
pub const ENTRY_OFFSET_PARAM: &str = "entry_offset";

const HEADER: usize = 512;
const NAME_OFFSET: usize = 64;
const NAME_LEN: usize = 448;
const DIR_TYPE: u16 = 0x8427;
const FILE_TYPE: u16 = 0x8497;
const ALIGN: usize = 1024;

/// Registers the `.psu` layer.
pub fn register(reg: &mut Registry) {
    reg.add_layer(Arc::new(PsuLayer));
}

/// The `.psu` container layer.
#[derive(Clone, Copy, Debug, Default)]
pub struct PsuLayer;

fn damaged(message: impl Into<String>, offset: usize) -> Error {
    Error::new(PSU_DAMAGED, message).with_offset(len_u64(offset))
}

fn write_error(message: impl Into<String>) -> Error {
    Error::new(PSU_WRITE, message)
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
struct Entry {
    offset: usize,
    kind: u16,
    name: Vec<u8>,
    size: usize,
    content_start: usize,
    content_end: usize,
    next: usize,
}

fn name_of(header: &[u8]) -> Vec<u8> {
    header
        .get(NAME_OFFSET..NAME_OFFSET.saturating_add(NAME_LEN))
        .unwrap_or_default()
        .iter()
        .copied()
        .take_while(|&byte| byte != 0)
        .collect()
}

fn padding_for(size: usize) -> usize {
    let remainder = size % ALIGN;
    if remainder == 0 { 0 } else { ALIGN - remainder }
}

fn parse(input: &[u8]) -> Result<Vec<Entry>> {
    let mut entries = Vec::new();
    let mut offset = 0_usize;
    while offset < input.len() {
        let header = input
            .get(offset..offset.saturating_add(HEADER))
            .ok_or_else(|| damaged("the .psu ends inside an entry header", offset))?;
        let Some(kind) = u16_at(header, 0) else {
            return Err(damaged("an entry has no type", offset));
        };
        if kind != DIR_TYPE && kind != FILE_TYPE {
            return Err(damaged(
                format!("entry type {kind:#06x} is not supported"),
                offset,
            ));
        }
        let size = u32_at(header, 4)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| damaged("an entry has no usable size", offset))?;
        let content_start = offset.saturating_add(HEADER);
        if kind == DIR_TYPE {
            entries.push(Entry {
                offset,
                kind,
                name: name_of(header),
                size,
                content_start,
                content_end: content_start,
                next: content_start,
            });
            offset = content_start;
            continue;
        }
        let content_end = content_start
            .checked_add(size)
            .ok_or_else(|| damaged("an entry is too large", offset))?;
        if content_end > input.len() {
            return Err(damaged("a file entry is cut short", content_start));
        }
        let next = content_end
            .checked_add(padding_for(size))
            .ok_or_else(|| damaged("an entry is too large", offset))?;
        if next > input.len() {
            return Err(damaged("a file entry's padding is cut short", content_end));
        }
        entries.push(Entry {
            offset,
            kind,
            name: name_of(header),
            size,
            content_start,
            content_end,
            next,
        });
        offset = next;
    }
    if entries.first().is_none_or(|entry| entry.kind != DIR_TYPE) {
        return Err(damaged("the first .psu entry is not the save folder", 0));
    }
    Ok(entries)
}

fn name_of_entry(entry: &Entry) -> &[u8] {
    entry.name.as_slice()
}

impl Layer for PsuLayer {
    fn id(&self) -> &'static str {
        "container.psu"
    }

    fn sniff(&self, input: &[u8], ctx: &SniffCtx<'_>) -> Confidence {
        if input.len() < HEADER {
            return Confidence::NONE;
        }
        if u16_at(input, 0) != Some(DIR_TYPE) {
            return Confidence::NONE;
        }
        let Ok(entries) = parse(input) else {
            return Confidence::NONE;
        };
        let named = ctx
            .file_name
            .is_some_and(|name| name.to_ascii_lowercase().ends_with(".psu"));
        let has_file = entries.iter().any(|entry| entry.kind == FILE_TYPE);
        if has_file {
            Confidence::new(if named { 1.0 } else { 0.9 })
        } else {
            Confidence::new(0.6)
        }
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<(Vec<u8>, Params)> {
        ctx.limits().check_input(len_u64(input.len()))?;
        let entries = parse(input)?;
        let hint = ctx.hints().opt_bytes(ENTRY_PARAM)?;
        let chosen = entries
            .iter()
            .filter(|entry| entry.kind == FILE_TYPE)
            .find(|entry| hint.is_some_and(|name| name_of_entry(entry) == name))
            .or_else(|| {
                entries
                    .iter()
                    .filter(|entry| entry.kind == FILE_TYPE)
                    .max_by_key(|entry| entry.size)
            })
            .ok_or_else(|| Error::new(PSU_NO_PAYLOAD, "the .psu holds no file entry"))?;
        let data = input
            .get(chosen.content_start..chosen.content_end)
            .ok_or_else(|| damaged("a file entry is cut short", chosen.content_start))?
            .to_vec();
        ctx.limits()
            .check_output(len_u64(input.len()), len_u64(data.len()))?;
        let mut params = Params::new();
        params.set(ENTRY_PARAM, chosen.name.clone());
        params.set(
            ENTRY_OFFSET_PARAM,
            i64::try_from(chosen.offset).unwrap_or(i64::MAX),
        );
        keep_source(&mut params, input, &data);
        Ok((data, params))
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
        let entries = parse(source)?;
        let chosen = entries
            .iter()
            .find(|entry| Some(entry.offset) == offset && entry.kind == FILE_TYPE)
            .or_else(|| {
                entries
                    .iter()
                    .find(|entry| entry.kind == FILE_TYPE && name_of_entry(entry) == name)
            })
            .ok_or_else(|| {
                Error::new(
                    codes::CORE_INVALID_PARAM,
                    "the parameter \"entry\" names no file entry of the .psu",
                )
            })?;
        let mut header = source
            .get(chosen.offset..chosen.content_start)
            .ok_or_else(|| write_error("the entry header is cut short"))?
            .to_vec();
        let size =
            u32::try_from(inner.len()).map_err(|_| write_error("the edited save is too large"))?;
        let field = header
            .get_mut(4..8)
            .ok_or_else(|| write_error("the entry header is cut short"))?;
        field.copy_from_slice(&size.to_le_bytes());
        let mut out = Vec::with_capacity(source.len().saturating_add(inner.len()));
        out.extend_from_slice(source.get(..chosen.offset).unwrap_or_default());
        out.extend_from_slice(&header);
        out.extend_from_slice(inner);
        let padding = padding_for(inner.len());
        if inner.len() == chosen.size {
            out.extend_from_slice(
                source
                    .get(chosen.content_end..chosen.next)
                    .unwrap_or_default(),
            );
        } else {
            out.resize(out.len().saturating_add(padding), 0);
        }
        out.extend_from_slice(source.get(chosen.next..).unwrap_or_default());
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

    fn fixture() -> Vec<u8> {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/synthetic/console/ps2/save.psu");
        std::fs::read(path).unwrap()
    }

    fn decode(input: &[u8]) -> (Vec<u8>, Params) {
        let limits = Limits::default();
        let mut ctx = DecodeCtx::new(&limits);
        PsuLayer.decode(input, &mut ctx).unwrap()
    }

    #[test]
    fn fixture_round_trips_and_picks_the_largest_file() {
        let bytes = fixture();
        let (inner, params) = decode(&bytes);
        assert_eq!(params.bytes(ENTRY_PARAM).unwrap(), b"save.json");
        assert!(inner.starts_with(b"{"));
        assert_eq!(PsuLayer.encode(&inner, &params).unwrap(), bytes);
    }

    #[test]
    fn editing_keeps_the_other_entries_identical() {
        let bytes = fixture();
        let (mut inner, params) = decode(&bytes);
        let position = inner
            .iter()
            .position(|&byte| byte == b'1')
            .expect("a digit in the JSON");
        inner[position] = b'3';
        let written = PsuLayer.encode(&inner, &params).unwrap();
        let before = parse(&bytes).unwrap();
        let after = parse(&written).unwrap();
        assert_eq!(before.len(), after.len());
        assert_eq!(before[0].name, after[0].name);
        for (old, new) in before.iter().zip(after.iter()).skip(1) {
            assert_eq!(old.name, new.name);
            assert_eq!(old.kind, new.kind);
        }
        let (read_back, _) = decode(&written);
        assert_eq!(read_back, inner);
    }

    #[test]
    fn growing_a_file_repads_the_entry() {
        let bytes = fixture();
        let (mut inner, params) = decode(&bytes);
        inner.extend_from_slice(b"\n");
        let written = PsuLayer.encode(&inner, &params).unwrap();
        assert_eq!(written.len(), bytes.len());
        let (read_back, _) = decode(&written);
        assert_eq!(read_back, inner);
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
            ["container.psu", "format.json"]
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
    fn a_truncated_entry_is_refused() {
        let bytes = fixture();
        let cut = bytes.get(..bytes.len() / 2).unwrap_or_default();
        assert_eq!(parse(cut).unwrap_err().code(), PSU_DAMAGED);
    }
}
