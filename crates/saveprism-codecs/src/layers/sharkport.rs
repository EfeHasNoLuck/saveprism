//! `container.sharkport`: SharkPort / X-Port GBA save snapshots (`.sps`, `.xps`).
//!
//! # Format
//!
//! A little-endian, length-prefixed text header: a 4-byte length and `"SharkPortSave"`, the
//! platform code (`0x000f0000` for GBA), then title, date and comment as length-prefixed
//! strings, then the length of a second header plus the raw save. The 0x1C-byte second header
//! carries the ROM's internal name, checksum and maker byte; the raw GBA save follows, and a
//! 4-byte checksum closes the file. The checksum is the running sum
//! `sum += byte << (sum % 18)` over the second header and raw save (32-bit wrapping).
//!
//! Field order checked against public descriptions of the GameShark/SharkPort format; the
//! checksum rule is implemented from that description, not copied code.
//!
//! # What the container returns
//!
//! The raw GBA save bytes, which go to normal detection. Title, date, comment and the second
//! header stay part of the container.
//!
//! # Exact writing and safety
//!
//! An unchanged payload is returned from the saved original bytes. After an edit the raw save,
//! its length field and the trailing checksum are rewritten; every header byte before the raw
//! save is kept. Lengths are checked against the input before any slice is taken.

use std::sync::Arc;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    Confidence, DecodeCtx, Error, ErrorCode, Layer, Params, Registry, Result, SniffCtx, codes,
};

use super::{keep_source, source_if_unchanged};

/// The snapshot is damaged or cut short.
pub const SPS_DAMAGED: ErrorCode = ErrorCode::new("SP-SPS-001");
/// The snapshot is not a GBA SharkPort file.
pub const SPS_UNSUPPORTED: ErrorCode = ErrorCode::new("SP-SPS-002");
/// The snapshot cannot be written again.
pub const SPS_WRITE: ErrorCode = ErrorCode::new("SP-SPS-003");

/// Parameter: title text from the header.
pub const TITLE_PARAM: &str = "title";
/// Parameter: date text from the header.
pub const DATE_PARAM: &str = "date";
/// Parameter: comment text from the header.
pub const COMMENT_PARAM: &str = "comment";
/// Parameter: byte offset of the raw save inside the file.
pub const SAVE_OFFSET_PARAM: &str = "save_offset";
/// Parameter: byte offset of the raw-save length field.
pub const LENGTH_OFFSET_PARAM: &str = "length_offset";

const MAGIC: &[u8] = b"SharkPortSave";
const PLATFORM_GBA: u32 = 0x000f_0000;
const SECOND_HEADER: usize = 0x1C;
const MIN_SAVE: usize = 512;

/// Registers the SharkPort layer.
pub fn register(reg: &mut Registry) {
    reg.add_layer(Arc::new(SharkPortLayer));
}

/// The SharkPort / X-Port GBA save layer.
#[derive(Clone, Copy, Debug, Default)]
pub struct SharkPortLayer;

fn damaged(message: impl Into<String>, offset: usize) -> Error {
    Error::new(SPS_DAMAGED, message).with_offset(len_u64(offset))
}

fn write_error(message: impl Into<String>) -> Error {
    Error::new(SPS_WRITE, message)
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    let raw = bytes.get(at..at.checked_add(4)?)?;
    Some(u32::from_le_bytes(<[u8; 4]>::try_from(raw).ok()?))
}

fn text(bytes: &[u8], at: usize) -> Result<(String, usize)> {
    let length = u32_at(bytes, at)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| damaged("a header text has no length", at))?;
    let start = at.saturating_add(4);
    let end = start
        .checked_add(length)
        .ok_or_else(|| damaged("a header text is too large", at))?;
    let raw = bytes
        .get(start..end)
        .ok_or_else(|| damaged("a header text is cut short", start))?;
    Ok((String::from_utf8_lossy(raw).into_owned(), end))
}

fn checksum(bytes: &[u8]) -> u32 {
    let mut sum = 0_u32;
    for &byte in bytes {
        let shift = sum % 18;
        sum = sum.wrapping_add(u32::from(byte) << shift);
    }
    sum
}

#[derive(Clone, Debug)]
struct Snapshot {
    title: String,
    date: String,
    comment: String,
    length_offset: usize,
    save_offset: usize,
    save_len: usize,
    stored_checksum: u32,
    calculated_checksum: u32,
}

fn parse(input: &[u8]) -> Result<Snapshot> {
    let (magic, after_magic) = text(input, 0)?;
    if magic.as_bytes() != MAGIC {
        return Err(Error::new(
            SPS_UNSUPPORTED,
            "the file does not start with a SharkPortSave header",
        ));
    }
    let platform = u32_at(input, after_magic)
        .ok_or_else(|| damaged("the platform code is missing", after_magic))?;
    if platform != PLATFORM_GBA {
        return Err(Error::new(
            SPS_UNSUPPORTED,
            format!("platform {platform:#010x} is not a GBA save"),
        ));
    }
    let (title, after_title) = text(input, after_magic.saturating_add(4))?;
    let (date, after_date) = text(input, after_title)?;
    let (comment, length_offset) = text(input, after_date)?;
    let total = u32_at(input, length_offset)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| damaged("the raw-save length is missing", length_offset))?;
    if total < SECOND_HEADER {
        return Err(damaged(
            "the raw-save length does not include the second header",
            length_offset,
        ));
    }
    let save_offset = length_offset
        .checked_add(4)
        .and_then(|value| value.checked_add(SECOND_HEADER))
        .ok_or_else(|| damaged("the raw save is too large", length_offset))?;
    let save_len = total - SECOND_HEADER;
    let save_end = save_offset
        .checked_add(save_len)
        .ok_or_else(|| damaged("the raw save is too large", save_offset))?;
    let checksum_offset = save_end;
    let checksum_end = checksum_offset.saturating_add(4);
    if checksum_end != input.len() {
        return Err(damaged(
            format!(
                "the file is {} bytes but its header describes {checksum_end}",
                input.len()
            ),
            checksum_offset,
        ));
    }
    let stored_checksum = u32_at(input, checksum_offset)
        .ok_or_else(|| damaged("the checksum is cut short", checksum_offset))?;
    let second_header_start = length_offset.saturating_add(4);
    let covered = input
        .get(second_header_start..save_end)
        .ok_or_else(|| damaged("the second header is cut short", second_header_start))?;
    Ok(Snapshot {
        title,
        date,
        comment,
        length_offset,
        save_offset,
        save_len,
        stored_checksum,
        calculated_checksum: checksum(covered),
    })
}

impl Layer for SharkPortLayer {
    fn id(&self) -> &'static str {
        "container.sharkport"
    }

    fn sniff(&self, input: &[u8], ctx: &SniffCtx<'_>) -> Confidence {
        let Ok(snapshot) = parse(input) else {
            return Confidence::NONE;
        };
        if snapshot.save_len < MIN_SAVE {
            return Confidence::NONE;
        }
        let named = ctx.file_name.is_some_and(|name| {
            let lower = name.to_ascii_lowercase();
            [".sps", ".xps"]
                .iter()
                .any(|extension| lower.ends_with(extension))
        });
        if snapshot.stored_checksum == snapshot.calculated_checksum {
            Confidence::new(if named { 1.0 } else { 0.97 })
        } else {
            Confidence::new(if named { 0.9 } else { 0.85 })
        }
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<(Vec<u8>, Params)> {
        ctx.limits().check_input(len_u64(input.len()))?;
        let snapshot = parse(input)?;
        let data = input
            .get(snapshot.save_offset..snapshot.save_offset.saturating_add(snapshot.save_len))
            .ok_or_else(|| damaged("the raw save is cut short", snapshot.save_offset))?
            .to_vec();
        ctx.limits()
            .check_output(len_u64(input.len()), len_u64(data.len()))?;
        let mut params = Params::new();
        params.set(TITLE_PARAM, snapshot.title);
        params.set(DATE_PARAM, snapshot.date);
        params.set(COMMENT_PARAM, snapshot.comment);
        params.set(
            SAVE_OFFSET_PARAM,
            i64::try_from(snapshot.save_offset).unwrap_or(i64::MAX),
        );
        params.set(
            LENGTH_OFFSET_PARAM,
            i64::try_from(snapshot.length_offset).unwrap_or(i64::MAX),
        );
        keep_source(&mut params, input, &data);
        Ok((data, params))
    }

    fn encode(&self, inner: &[u8], params: &Params) -> Result<Vec<u8>> {
        if let Some(source) = source_if_unchanged(inner, params)? {
            return Ok(source);
        }
        let source = params.bytes(super::SOURCE_PARAM)?;
        let save_offset = params
            .opt_int(SAVE_OFFSET_PARAM)?
            .and_then(|value| usize::try_from(value).ok());
        let length_offset = params
            .opt_int(LENGTH_OFFSET_PARAM)?
            .and_then(|value| usize::try_from(value).ok());
        let snapshot = parse(source).map_err(|error| {
            if error.code() == SPS_UNSUPPORTED {
                Error::new(
                    codes::CORE_INVALID_PARAM,
                    "the saved SharkPort header is invalid",
                )
            } else {
                error
            }
        })?;
        let save_start = save_offset.unwrap_or(snapshot.save_offset);
        if save_start != snapshot.save_offset {
            return Err(Error::new(
                codes::CORE_INVALID_PARAM,
                "the saved raw-save offset does not match the file",
            ));
        }
        let length_field = length_offset.unwrap_or(snapshot.length_offset);
        if length_field != snapshot.length_offset {
            return Err(Error::new(
                codes::CORE_INVALID_PARAM,
                "the saved length field does not match the file",
            ));
        }
        let total = inner
            .len()
            .checked_add(SECOND_HEADER)
            .and_then(|value| u32::try_from(value).ok())
            .ok_or_else(|| write_error("the edited save is too large"))?;
        let second_header_start = snapshot.length_offset.saturating_add(4);
        let mut out = Vec::with_capacity(save_start.saturating_add(inner.len()).saturating_add(4));
        out.extend_from_slice(source.get(..snapshot.length_offset).unwrap_or_default());
        out.extend_from_slice(&total.to_le_bytes());
        out.extend_from_slice(
            source
                .get(second_header_start..snapshot.save_offset)
                .ok_or_else(|| write_error("the second header is cut short"))?,
        );
        out.extend_from_slice(inner);
        let code = checksum(
            out.get(second_header_start..)
                .ok_or_else(|| write_error("the rebuilt file is too short"))?,
        );
        out.extend_from_slice(&code.to_le_bytes());
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
            .join("../../fixtures/synthetic/console/gba")
            .join(relative);
        std::fs::read(path).unwrap()
    }

    fn decode(input: &[u8]) -> (Vec<u8>, Params) {
        let limits = Limits::default();
        let mut ctx = DecodeCtx::new(&limits);
        SharkPortLayer.decode(input, &mut ctx).unwrap()
    }

    #[test]
    fn fixture_round_trips_with_its_checksum() {
        let bytes = fixture("save.sps");
        let (inner, params) = decode(&bytes);
        assert!(inner.starts_with(b"{"));
        assert_eq!(params.str(TITLE_PARAM).unwrap(), "Test Save");
        assert_eq!(SharkPortLayer.encode(&inner, &params).unwrap(), bytes);
    }

    #[test]
    fn editing_recomputes_the_checksum() {
        let bytes = fixture("save.sps");
        let (mut inner, params) = decode(&bytes);
        let position = inner
            .iter()
            .position(|&byte| byte == b'1')
            .expect("a digit in the JSON");
        inner[position] = b'5';
        let written = SharkPortLayer.encode(&inner, &params).unwrap();
        let parsed = parse(&written).unwrap();
        assert_eq!(parsed.stored_checksum, parsed.calculated_checksum);
        let (read_back, _) = decode(&written);
        assert_eq!(read_back, inner);
    }

    #[test]
    fn xps_uses_the_same_container() {
        let bytes = fixture("save.xps");
        let (inner, params) = decode(&bytes);
        assert_eq!(SharkPortLayer.encode(&inner, &params).unwrap(), bytes);
    }

    #[test]
    fn ps2_platform_is_not_claimed() {
        let mut bytes = fixture("save.sps");
        let magic_len = u32_at(&bytes, 0).unwrap_or(0) as usize;
        let start = 4 + magic_len;
        if let Some(field) = bytes.get_mut(start..start + 4) {
            field.copy_from_slice(&0x0000_000fu32.to_le_bytes());
        }
        assert_eq!(
            SharkPortLayer.sniff(&bytes, &SniffCtx::default()),
            Confidence::NONE
        );
    }

    #[test]
    fn detection_reaches_the_json_payload() {
        let bytes = fixture("save.sps");
        let mut reg = Registry::default();
        register_all(&mut reg);
        let detection =
            detect(&bytes, &reg, &Limits::default(), &DetectOptions::default()).unwrap();
        assert_eq!(
            detection.best.recipe.kinds(),
            ["container.sharkport", "format.json"]
        );
        let report =
            check_round_trip(&bytes, &detection.best.recipe, &reg, &Limits::default()).unwrap();
        assert_eq!(report.overall, Verdict::Exact);
        let decoded =
            pipeline::decode(&bytes, &detection.best.recipe, &reg, &Limits::default()).unwrap();
        let encoded = pipeline::encode(&decoded.doc, &decoded.recipe, &reg).unwrap();
        assert_eq!(encoded, bytes);
    }
}
