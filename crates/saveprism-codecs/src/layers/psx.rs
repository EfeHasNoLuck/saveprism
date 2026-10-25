//! `container.psx-card` and `container.psx-gme`: PlayStation 1 memory card images.
//!
//! # Format
//!
//! A raw card is 128 KiB: 16 blocks of 8 KiB, each block 64 frames of 128 bytes. Block 0 is
//! the directory. Frame 0 is the header (`"MC"` and an XOR checksum in the last byte); frames
//! 1..15 are directory entries with the block state (in use `0x51` first, `0x52` middle,
//! `0x53` last; free `0xA0`..`0xA3` deleted), the allocated size, the next block number minus
//! one (`0xFFFF` at the end), the ASCII name and an XOR checksum. A file's blocks are chained
//! through those entries; the first block starts with a title frame (`"SC"`) and 1..3 icon
//! frames, the following blocks are plain data. Tables checked against the psx-spx
//! "Controllers and Memory Cards" chapter (public documentation).
//!
//! `.gme` files (InterAct DexDrive) are a 3904-byte (0xF40) header followed by the same 128 KiB
//! card. They are peeled by `container.psx-gme`, then the raw card by `container.psx-card`.
//!
//! # What the container returns
//!
//! The largest in-use save file, or the file named by the `entry` parameter hint. Its data
//! region (starting after the title and icon frames) is returned to the normal detection. If
//! the file is unknown binary it can still be inspected; only its bytes are exposed. The
//! directory and title/icon frames stay part of the container.
//!
//! # Exact writing and safety
//!
//! An unchanged payload is returned from the saved original bytes. After an edit the payload is
//! written back into the same blocks (a different length is refused, because a PS1 directory
//! cannot describe a partial block) and every header and directory XOR checksum is recomputed.
//! Block chains are followed with a visited set and bounded by the 15 directory frames, so a
//! damaged card cannot loop or read outside the image.

use std::sync::Arc;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    Confidence, DecodeCtx, Error, ErrorCode, Layer, Params, Registry, Result, SniffCtx, codes,
};

use super::{keep_source, source_if_unchanged};

/// The card is damaged, cut short or has no usable save file.
pub const PSX_DAMAGED: ErrorCode = ErrorCode::new("SP-PSX-001");
/// The edited save cannot be written back into the card.
pub const PSX_WRITE: ErrorCode = ErrorCode::new("SP-PSX-002");
/// No in-use save file was found.
pub const PSX_NO_PAYLOAD: ErrorCode = ErrorCode::new("SP-PSX-003");
/// The `.gme` file is damaged or has a bad header.
pub const GME_DAMAGED: ErrorCode = ErrorCode::new("SP-GME-001");
/// The `.gme` file cannot be written again.
pub const GME_WRITE: ErrorCode = ErrorCode::new("SP-GME-002");

/// Parameter: raw name of the chosen save file.
pub const ENTRY_PARAM: &str = "entry";
/// Parameter: directory frame index of the chosen file (1..15).
pub const ENTRY_FRAME_PARAM: &str = "entry_frame";

const FRAME: usize = 128;
const BLOCK: usize = 8192;
const CARD_LEN: usize = 128 * 1024;
const GME_HEADER: usize = 0xF40;
const GME_LEN: usize = GME_HEADER + CARD_LEN;
const DIR_FIRST_FRAME: usize = 1;
const DIR_LAST_FRAME: usize = 15;
const CHECKSUM_BYTE: usize = 0x7F;
const NAME_OFFSET: usize = 0x0A;
const NAME_LEN: usize = 0x14;
const NEXT_OFFSET: usize = 0x08;
const SIZE_OFFSET: usize = 0x04;
const ICON_FRAME: usize = 2;

const STATE_FIRST: u32 = 0x51;
const STATE_MIDDLE: u32 = 0x52;
const STATE_LAST: u32 = 0x53;

/// Registers both PS1 card layers.
pub fn register(reg: &mut Registry) {
    reg.add_layer(Arc::new(PsxGmeLayer));
    reg.add_layer(Arc::new(PsxCardLayer));
}

/// The raw 128 KiB card layer.
#[derive(Clone, Copy, Debug, Default)]
pub struct PsxCardLayer;

/// The DexDrive `.gme` envelope layer.
#[derive(Clone, Copy, Debug, Default)]
pub struct PsxGmeLayer;

fn damaged(message: impl Into<String>, offset: usize) -> Error {
    Error::new(PSX_DAMAGED, message).with_offset(len_u64(offset))
}

fn write_error(message: impl Into<String>) -> Error {
    Error::new(PSX_WRITE, message)
}

fn gme_damaged(message: impl Into<String>, offset: usize) -> Error {
    Error::new(GME_DAMAGED, message).with_offset(len_u64(offset))
}

fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    let raw = bytes.get(at..at.checked_add(2)?)?;
    Some(u16::from_le_bytes(<[u8; 2]>::try_from(raw).ok()?))
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    let raw = bytes.get(at..at.checked_add(4)?)?;
    Some(u32::from_le_bytes(<[u8; 4]>::try_from(raw).ok()?))
}

fn xor_checksum(bytes: &[u8]) -> u8 {
    bytes.iter().fold(0_u8, |sum, &byte| sum ^ byte)
}

fn frame(card: &[u8], index: usize) -> Result<&[u8]> {
    let start = index
        .checked_mul(FRAME)
        .ok_or_else(|| damaged("the memory card is too large", index))?;
    card.get(start..start.saturating_add(FRAME))
        .ok_or_else(|| damaged("the memory card ends inside a frame", start))
}

fn frame_mut(card: &mut [u8], index: usize) -> Result<&mut [u8]> {
    let start = index
        .checked_mul(FRAME)
        .ok_or_else(|| write_error("the memory card is too large"))?;
    card.get_mut(start..start.saturating_add(FRAME))
        .ok_or_else(|| write_error("the memory card ends inside a frame"))
}

fn name_of(entry: &[u8]) -> Vec<u8> {
    entry
        .get(NAME_OFFSET..NAME_OFFSET.saturating_add(NAME_LEN))
        .unwrap_or_default()
        .iter()
        .copied()
        .take_while(|&byte| byte != 0)
        .collect()
}

fn checksum_ok(entry: &[u8]) -> bool {
    entry.get(CHECKSUM_BYTE).is_some_and(|&stored| {
        stored == xor_checksum(entry.get(..CHECKSUM_BYTE).unwrap_or_default())
    })
}

#[derive(Clone, Debug)]
struct DirEntry {
    frame_index: usize,
    state: u32,
    size: usize,
    next: u16,
    name: Vec<u8>,
}

#[derive(Clone, Debug)]
struct Card {
    entries: Vec<DirEntry>,
}

fn parser(card: &[u8]) -> Result<Card> {
    if card.len() != CARD_LEN {
        return Err(damaged(
            format!(
                "a PS1 memory card is {CARD_LEN} bytes, this file is {} bytes",
                card.len()
            ),
            0,
        ));
    }
    let header = frame(card, 0)?;
    if header.get(..2) != Some(b"MC") {
        return Err(damaged("the header frame has no \"MC\" signature", 0));
    }
    if !checksum_ok(header) {
        return Err(damaged("the header frame checksum does not match", 0));
    }
    let mut entries = Vec::new();
    let mut valid_frames = 0_usize;
    for frame_index in DIR_FIRST_FRAME..=DIR_LAST_FRAME {
        let entry = frame(card, frame_index)?;
        if !checksum_ok(entry) {
            continue;
        }
        valid_frames = valid_frames.saturating_add(1);
        let Some(state) = u32_at(entry, 0) else {
            continue;
        };
        if !matches!(state, STATE_FIRST | STATE_MIDDLE | STATE_LAST) {
            continue;
        }
        let Some(size) = u32_at(entry, SIZE_OFFSET).and_then(|v| usize::try_from(v).ok()) else {
            continue;
        };
        let Some(next) = u16_at(entry, NEXT_OFFSET) else {
            continue;
        };
        entries.push(DirEntry {
            frame_index,
            state,
            size,
            next,
            name: name_of(entry),
        });
    }
    if valid_frames == 0 {
        return Err(damaged(
            "no directory frame has a valid XOR checksum",
            FRAME,
        ));
    }
    Ok(Card { entries })
}

#[derive(Clone, Debug)]
struct SaveFile {
    name: Vec<u8>,
    frame_index: usize,
    blocks: Vec<usize>,
    size: usize,
}

impl Card {
    fn files(&self) -> Vec<SaveFile> {
        let mut files = Vec::new();
        for entry in &self.entries {
            if entry.state != STATE_FIRST {
                continue;
            }
            let mut blocks = Vec::new();
            let mut current = entry;
            let mut visited = [false; DIR_LAST_FRAME + 1];
            loop {
                let index = current.frame_index;
                if index > DIR_LAST_FRAME {
                    break;
                }
                let Some(seen) = visited.get_mut(index) else {
                    break;
                };
                if std::mem::replace(seen, true) {
                    break;
                }
                blocks.push(index);
                if current.next == 0xFFFF {
                    break;
                }
                let next_frame = usize::from(current.next).saturating_add(1);
                if next_frame > DIR_LAST_FRAME {
                    break;
                }
                let Some(next) = self
                    .entries
                    .iter()
                    .find(|candidate| candidate.frame_index == next_frame)
                else {
                    break;
                };
                if next.state != STATE_MIDDLE && next.state != STATE_LAST {
                    break;
                }
                current = next;
            }
            files.push(SaveFile {
                name: entry.name.clone(),
                frame_index: entry.frame_index,
                blocks,
                size: entry.size,
            });
        }
        files
    }
}

fn choose<'a>(files: &'a [SaveFile], hint: Option<&[u8]>) -> Option<&'a SaveFile> {
    if let Some(name) = hint
        && let Some(found) = files.iter().find(|file| file.name == name)
    {
        return Some(found);
    }
    files.iter().max_by(|a, b| {
        a.size
            .cmp(&b.size)
            .then_with(|| b.frame_index.cmp(&a.frame_index))
    })
}

/// Absolute offset of the first payload byte of `file` inside the card.
fn payload_start(card: &[u8], file: &SaveFile) -> Result<usize> {
    let block = file
        .blocks
        .first()
        .ok_or_else(|| damaged("the save file has no block", 0))?;
    let block_start = block
        .checked_mul(BLOCK)
        .ok_or_else(|| damaged("the memory card is too large", 0))?;
    let title = card
        .get(block_start..block_start.saturating_add(FRAME))
        .ok_or_else(|| damaged("a save block is cut short", block_start))?;
    let icons: usize = if title.get(..2) == Some(b"SC") {
        match title.get(ICON_FRAME) {
            Some(0x11) => 1,
            Some(0x12) => 2,
            Some(0x13) => 3,
            _ => 1,
        }
    } else {
        0
    };
    block_start
        .checked_add((1 + icons).saturating_mul(FRAME))
        .ok_or_else(|| damaged("the memory card is too large", block_start))
}

fn payload(card: &[u8], file: &SaveFile) -> Result<Vec<u8>> {
    let start = payload_start(card, file)?;
    let mut out = Vec::new();
    for (index, &block) in file.blocks.iter().enumerate() {
        let block_start = block
            .checked_mul(BLOCK)
            .ok_or_else(|| damaged("the memory card is too large", 0))?;
        let (from, to) = if index == 0 {
            (start, block_start.saturating_add(BLOCK))
        } else {
            (block_start, block_start.saturating_add(BLOCK))
        };
        let bytes = card
            .get(from..to)
            .ok_or_else(|| damaged("a save block is cut short", from))?;
        out.extend_from_slice(bytes);
    }
    Ok(out)
}

fn payload_len(card: &[u8], file: &SaveFile) -> Result<usize> {
    payload(card, file).map(|bytes| bytes.len())
}

fn write_payload(card: &mut [u8], file: &SaveFile, inner: &[u8]) -> Result<()> {
    let start = payload_start(card, file)?;
    let mut cursor = 0_usize;
    for (index, &block) in file.blocks.iter().enumerate() {
        let block_start = block
            .checked_mul(BLOCK)
            .ok_or_else(|| write_error("the memory card is too large"))?;
        let (from, to) = if index == 0 {
            (start, block_start.saturating_add(BLOCK))
        } else {
            (block_start, block_start.saturating_add(BLOCK))
        };
        let len = to.saturating_sub(from);
        let chunk = inner
            .get(cursor..cursor.saturating_add(len))
            .ok_or_else(|| write_error("the new save file is larger than the card"))?;
        let target = card
            .get_mut(from..to)
            .ok_or_else(|| write_error("a save block is cut short"))?;
        target.copy_from_slice(chunk);
        cursor = cursor.saturating_add(len);
    }
    Ok(())
}

fn rewrite_checksums(card: &mut [u8]) -> Result<()> {
    for index in 0..=DIR_LAST_FRAME {
        let entry = frame_mut(card, index)?;
        let sum = xor_checksum(entry.get(..CHECKSUM_BYTE).unwrap_or_default());
        if let Some(slot) = entry.get_mut(CHECKSUM_BYTE) {
            *slot = sum;
        }
    }
    Ok(())
}

impl Layer for PsxCardLayer {
    fn id(&self) -> &'static str {
        "container.psx-card"
    }

    fn sniff(&self, input: &[u8], ctx: &SniffCtx<'_>) -> Confidence {
        if input.len() != CARD_LEN {
            return Confidence::NONE;
        }
        let Ok(card) = parser(input) else {
            return Confidence::NONE;
        };
        if card.entries.iter().any(|entry| entry.state == STATE_FIRST) {
            let named = ctx.file_name.is_some_and(|name| {
                let lower = name.to_ascii_lowercase();
                [".mcr", ".mcd", ".mem", ".card"]
                    .iter()
                    .any(|extension| lower.ends_with(extension))
            });
            Confidence::new(if named { 1.0 } else { 0.95 })
        } else {
            Confidence::new(0.7)
        }
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<(Vec<u8>, Params)> {
        ctx.limits().check_input(len_u64(input.len()))?;
        let card = parser(input)?;
        let files = card.files();
        let hint = ctx.hints().opt_bytes(ENTRY_PARAM)?;
        let file = choose(&files, hint).ok_or_else(|| {
            Error::new(PSX_NO_PAYLOAD, "the memory card holds no in-use save file")
        })?;
        let data = payload(input, file)?;
        ctx.limits()
            .check_output(len_u64(input.len()), len_u64(data.len()))?;
        let mut params = Params::new();
        params.set(ENTRY_PARAM, file.name.clone());
        params.set(
            ENTRY_FRAME_PARAM,
            i64::try_from(file.frame_index).unwrap_or(i64::MAX),
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
        let frame_index = params
            .opt_int(ENTRY_FRAME_PARAM)?
            .and_then(|value| usize::try_from(value).ok());
        let card = parser(source)?;
        let files = card.files();
        let file = files
            .iter()
            .find(|candidate| Some(candidate.frame_index) == frame_index)
            .or_else(|| files.iter().find(|candidate| candidate.name == name))
            .ok_or_else(|| {
                Error::new(
                    codes::CORE_INVALID_PARAM,
                    "the parameter \"entry\" names no save file of the card",
                )
            })?;
        let expected = payload_len(source, file)?;
        if inner.len() != expected {
            return Err(write_error(format!(
                "a PS1 save file keeps its block layout, so its length must stay {expected} bytes (the edit is {} bytes)",
                inner.len()
            )));
        }
        let mut out = source.to_vec();
        write_payload(&mut out, file, inner)?;
        rewrite_checksums(&mut out)?;
        Ok(out)
    }
}

impl Layer for PsxGmeLayer {
    fn id(&self) -> &'static str {
        "container.psx-gme"
    }

    fn sniff(&self, input: &[u8], ctx: &SniffCtx<'_>) -> Confidence {
        if input.len() != GME_LEN {
            return Confidence::NONE;
        }
        if input.get(..12) != Some(b"123-456-STD\0") {
            return Confidence::NONE;
        }
        let Some(card) = input.get(GME_HEADER..) else {
            return Confidence::NONE;
        };
        if parser(card).is_err() {
            return Confidence::NONE;
        }
        let named = ctx
            .file_name
            .is_some_and(|name| name.to_ascii_lowercase().ends_with(".gme"));
        Confidence::new(if named { 1.0 } else { 0.95 })
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<(Vec<u8>, Params)> {
        ctx.limits().check_input(len_u64(input.len()))?;
        if input.len() != GME_LEN {
            return Err(gme_damaged(
                format!(
                    ".gme files are {GME_LEN} bytes, this file is {} bytes",
                    input.len()
                ),
                0,
            ));
        }
        if input.get(..12) != Some(b"123-456-STD\0") {
            return Err(gme_damaged("the .gme header has no DexDrive signature", 0));
        }
        let card = input
            .get(GME_HEADER..)
            .ok_or_else(|| gme_damaged("the .gme file has no card data", GME_HEADER))?;
        parser(card)?;
        let mut params = Params::new();
        params.set(
            "header",
            input.get(..GME_HEADER).unwrap_or_default().to_vec(),
        );
        keep_source(&mut params, input, card);
        Ok((card.to_vec(), params))
    }

    fn encode(&self, inner: &[u8], params: &Params) -> Result<Vec<u8>> {
        if let Some(source) = source_if_unchanged(inner, params)? {
            return Ok(source);
        }
        if inner.len() != CARD_LEN {
            return Err(Error::new(
                GME_WRITE,
                format!(
                    "the card data must stay {CARD_LEN} bytes, this is {} bytes",
                    inner.len()
                ),
            ));
        }
        let header = params.bytes("header")?;
        if header.len() != GME_HEADER {
            return Err(Error::new(
                GME_WRITE,
                "the saved .gme header has the wrong length",
            ));
        }
        let mut out = Vec::with_capacity(GME_LEN);
        out.extend_from_slice(header);
        out.extend_from_slice(inner);
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use saveprism_core::detect::{DetectOptions, detect};
    use saveprism_core::guard::{Verdict, check_round_trip};
    use saveprism_core::pipeline;

    use super::*;
    use crate::register_all;

    fn fixture(relative: &str) -> Vec<u8> {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/synthetic/console")
            .join(relative);
        std::fs::read(path).unwrap()
    }

    fn decode(input: &[u8]) -> (Vec<u8>, Params) {
        let limits = saveprism_core::Limits::default();
        let mut ctx = DecodeCtx::new(&limits);
        PsxCardLayer.decode(input, &mut ctx).unwrap()
    }

    #[test]
    fn card_fixture_round_trips_and_picks_the_save() {
        let bytes = fixture("ps1/card.mcr");
        let (inner, params) = decode(&bytes);
        assert_eq!(params.bytes(ENTRY_PARAM).unwrap(), b"BASLUS-00000SAVE");
        assert!(inner.starts_with(b"{"));
        assert_eq!(PsxCardLayer.encode(&inner, &params).unwrap(), bytes);
    }

    #[test]
    fn editing_the_save_keeps_the_directory_and_recomputes_checksums() {
        let bytes = fixture("ps1/card.mcr");
        let (mut inner, params) = decode(&bytes);
        let position = inner
            .iter()
            .position(|&byte| byte == b'1')
            .expect("a digit in the JSON");
        inner[position] = b'9';
        let written = PsxCardLayer.encode(&inner, &params).unwrap();
        assert_eq!(written.len(), bytes.len());
        let before = parser(&bytes).unwrap();
        let after = parser(&written).unwrap();
        assert_eq!(before.entries.len(), after.entries.len());
        for (old, new) in before.entries.iter().zip(after.entries.iter()) {
            assert_eq!(old.name, new.name);
            assert_eq!(old.size, new.size);
        }
        let (read_back, _) = decode(&written);
        assert_eq!(read_back, inner);
    }

    #[test]
    fn gme_fixture_is_peeled_then_the_card() {
        let bytes = fixture("ps1/card.gme");
        let reg = {
            let mut reg = Registry::default();
            register_all(&mut reg);
            reg
        };
        let detection = detect(
            &bytes,
            &reg,
            &saveprism_core::Limits::default(),
            &DetectOptions::default(),
        )
        .unwrap();
        assert_eq!(
            detection.best.recipe.kinds(),
            ["container.psx-gme", "container.psx-card", "format.json"]
        );
        let report = check_round_trip(
            &bytes,
            &detection.best.recipe,
            &reg,
            &saveprism_core::Limits::default(),
        )
        .unwrap();
        assert_eq!(report.overall, Verdict::Exact);
        let decoded = pipeline::decode(
            &bytes,
            &detection.best.recipe,
            &reg,
            &saveprism_core::Limits::default(),
        )
        .unwrap();
        let encoded = pipeline::encode(&decoded.doc, &decoded.recipe, &reg).unwrap();
        assert_eq!(encoded, bytes);
    }

    #[test]
    fn random_128k_is_not_a_card() {
        let fake = vec![0_u8; CARD_LEN];
        let ctx = SniffCtx::default();
        assert_eq!(PsxCardLayer.sniff(&fake, &ctx), Confidence::NONE);
    }

    #[test]
    fn an_unknown_game_save_opens_as_a_read_only_raw_leaf() {
        let mut bytes = fixture("ps1/card.mcr");
        let (inner, params) = decode(&bytes);
        let start = bytes
            .windows(inner.len())
            .position(|window| window == inner)
            .expect("the payload inside the card");
        for (offset, byte) in bytes
            .get_mut(start..start.saturating_add(inner.len()))
            .expect("the payload inside the card")
            .iter_mut()
            .enumerate()
        {
            *byte = u8::try_from(offset % 251).unwrap_or(0) ^ 0xA5;
        }
        let mut reg = Registry::default();
        register_all(&mut reg);
        let detection = detect(
            &bytes,
            &reg,
            &saveprism_core::Limits::default(),
            &DetectOptions::default(),
        )
        .unwrap();
        assert_eq!(
            detection.best.recipe.kinds(),
            ["container.psx-card", "format.raw"]
        );
        let _ = params;
    }

    #[test]
    fn a_changed_length_is_refused() {
        let bytes = fixture("ps1/card.mcr");
        let (mut inner, params) = decode(&bytes);
        inner.push(b' ');
        assert_eq!(
            PsxCardLayer.encode(&inner, &params).unwrap_err().code(),
            PSX_WRITE
        );
    }
}
