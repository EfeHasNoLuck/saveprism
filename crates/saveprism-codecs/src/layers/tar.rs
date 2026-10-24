//! `container.tar`: a save packed in an uncompressed tar archive.
//!
//! Format: the POSIX ustar header (GNU tar manual, "Basic Tar Format"): 512-byte blocks, the
//! `ustar` magic at offset 257, octal size and checksum fields, and two zero blocks at the end.
//!
//! Like the zip container, the layer picks one payload entry and returns its bytes; every other
//! entry stays byte-identical. The payload is the only regular file, or, when there are several,
//! the largest regular file whose contents another codec or layer recognises. The chosen raw name
//! is kept in the parameter `entry`.
//!
//! An unchanged archive is returned from the saved original bytes. After an edit only the payload
//! header (size and checksum) and its data blocks are rewritten; the trailing zero blocks and any
//! padding after them are copied unchanged.

use std::sync::Arc;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    Confidence, DecodeCtx, Error, ErrorCode, Layer, Params, Registry, Result, SniffCtx, codes,
};

use super::{inner_is_known, keep_source, source_if_unchanged};

/// The archive is damaged or cut short.
pub const TAR_DAMAGED: ErrorCode = ErrorCode::new("SP-TAR-001");
/// No regular file entry holds something another codec recognises.
pub const TAR_NO_PAYLOAD: ErrorCode = ErrorCode::new("SP-TAR-002");
/// The archive could not be written again.
pub const TAR_WRITE: ErrorCode = ErrorCode::new("SP-TAR-003");

/// Parameter: raw name of the chosen payload entry.
pub const ENTRY_PARAM: &str = "entry";
/// Parameter: header offset of the chosen payload entry (names may repeat in a tar archive).
pub const ENTRY_OFFSET_PARAM: &str = "entry_offset";

const BLOCK: usize = 512;
const NAME_LEN: usize = 100;
const SIZE_OFFSET: usize = 124;
const SIZE_LEN: usize = 12;
const CHECKSUM_OFFSET: usize = 148;
const CHECKSUM_LEN: usize = 8;
const MAGIC_OFFSET: usize = 257;
const MAGIC: &[u8; 5] = b"ustar";
/// Largest size representable in the 11 octal digits before the terminating NUL.
const MAX_SIZE: u64 = 0o77_777_777_777;

/// Registers the tar container layer.
pub fn register(reg: &mut Registry) {
    reg.add_layer(Arc::new(TarLayer));
}

/// The `container.tar` layer.
#[derive(Clone, Copy, Debug, Default)]
pub struct TarLayer;

fn bad(message: impl Into<String>, offset: usize) -> Error {
    Error::new(TAR_DAMAGED, message).with_offset(len_u64(offset))
}

fn no_payload() -> Error {
    Error::new(
        TAR_NO_PAYLOAD,
        "no regular file entry holds a format another codec recognises",
    )
}

/// Parses a trimmed octal field (digits, spaces and a trailing NUL).
fn octal(bytes: &[u8]) -> Option<u64> {
    let mut value = 0_u64;
    let mut seen = false;
    for &byte in bytes {
        match byte {
            b'0'..=b'7' => {
                seen = true;
                value = value.checked_mul(8)?.checked_add(u64::from(byte - b'0'))?;
            }
            b' ' | 0 => {}
            _ => return None,
        }
    }
    if seen { Some(value) } else { Some(0) }
}

fn checksum(block: &[u8]) -> u64 {
    block
        .iter()
        .enumerate()
        .map(|(index, &byte)| {
            if (CHECKSUM_OFFSET..CHECKSUM_OFFSET + CHECKSUM_LEN).contains(&index) {
                u64::from(b' ')
            } else {
                u64::from(byte)
            }
        })
        .sum()
}

fn name_of(block: &[u8]) -> Vec<u8> {
    block
        .get(..NAME_LEN)
        .unwrap_or_default()
        .iter()
        .copied()
        .take_while(|&byte| byte != 0)
        .collect()
}

fn is_regular(typeflag: u8) -> bool {
    matches!(typeflag, 0 | b'0' | b'7')
}

#[derive(Clone, Debug)]
struct Entry {
    name: Vec<u8>,
    header: usize,
    data_start: usize,
    size: usize,
    end: usize,
}

#[derive(Clone, Debug)]
struct Archive {
    entries: Vec<Entry>,
    /// Offset of the first zero block (or the end of the data when the marker is missing).
    tail: usize,
}

fn parse(input: &[u8]) -> Result<Archive> {
    if input.len() < BLOCK {
        return Err(bad("the tar archive is shorter than one header block", 0));
    }
    let mut entries = Vec::new();
    let mut pos = 0_usize;
    let tail = loop {
        let header_end = pos
            .checked_add(BLOCK)
            .ok_or_else(|| bad("the tar archive is too large", pos))?;
        let Some(block) = input.get(pos..header_end) else {
            if pos == input.len() {
                break pos;
            }
            return Err(bad("the tar archive ends inside a header block", pos));
        };
        if block.iter().all(|&byte| byte == 0) {
            break pos;
        }
        if block.get(MAGIC_OFFSET..MAGIC_OFFSET + MAGIC.len()) != Some(MAGIC) {
            return Err(bad("a tar header has no ustar magic", pos + MAGIC_OFFSET));
        }
        let stored = octal(
            block
                .get(CHECKSUM_OFFSET..CHECKSUM_OFFSET + CHECKSUM_LEN)
                .unwrap_or_default(),
        )
        .ok_or_else(|| bad("a tar header checksum is not octal", pos + CHECKSUM_OFFSET))?;
        if stored != checksum(block) {
            return Err(bad(
                "a tar header checksum does not match",
                pos + CHECKSUM_OFFSET,
            ));
        }
        let size = octal(
            block
                .get(SIZE_OFFSET..SIZE_OFFSET + SIZE_LEN)
                .unwrap_or_default(),
        )
        .ok_or_else(|| bad("a tar header size is not octal", pos + SIZE_OFFSET))?;
        let size = usize::try_from(size).map_err(|_| {
            bad(
                "a tar entry is too large for this platform",
                pos + SIZE_OFFSET,
            )
        })?;
        let data_start = header_end;
        let data_end = data_start
            .checked_add(size)
            .ok_or_else(|| bad("a tar entry is too large", pos + SIZE_OFFSET))?;
        let end = data_end
            .checked_add((BLOCK - (size % BLOCK)) % BLOCK)
            .ok_or_else(|| bad("a tar entry is too large", pos + SIZE_OFFSET))?;
        if end > input.len() {
            return Err(bad(
                "the tar archive ends inside the contents of an entry",
                data_start,
            ));
        }
        let typeflag = block.get(156).copied().unwrap_or(0);
        entries.push(Entry {
            name: name_of(block),
            header: pos,
            data_start,
            size,
            end,
        });
        if !is_regular(typeflag) {
            // Metadata and directory entries are kept as they are but never chosen as payload.
            let last = entries.len().saturating_sub(1);
            if let Some(entry) = entries.get_mut(last) {
                entry.name.clear();
            }
        }
        pos = end;
    };
    Ok(Archive { entries, tail })
}

fn regular_entries(archive: &Archive) -> Vec<&Entry> {
    let mut files: Vec<&Entry> = archive
        .entries
        .iter()
        .filter(|entry| !entry.name.is_empty() && !entry.name.ends_with(b"/"))
        .collect();
    files.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.header.cmp(&b.header)));
    files
}

fn read_entry<'a>(input: &'a [u8], entry: &Entry) -> &'a [u8] {
    input
        .get(entry.data_start..entry.data_start.saturating_add(entry.size))
        .unwrap_or_default()
}

fn payload<'a>(archive: &'a Archive, input: &[u8], ctx: &DecodeCtx<'_>) -> Result<&'a Entry> {
    let budget = ctx.output_budget(len_u64(input.len()));
    for entry in &archive.entries {
        if let Ok(size) = u64::try_from(entry.size)
            && size > budget
        {
            return Err(Error::new(
                codes::CORE_LIMIT_DECODED,
                format!(
                    "an entry would be {size} bytes, the budget is {budget} bytes (possible decompression bomb)"
                ),
            )
            .with_offset(len_u64(entry.header)));
        }
    }
    let files = regular_entries(archive);
    match files.as_slice() {
        [] => Err(no_payload()),
        [only] => Ok(only),
        _ => {
            for entry in files {
                if inner_is_known(read_entry(input, entry)) {
                    return Ok(entry);
                }
            }
            Err(no_payload())
        }
    }
}

fn write_size(header: &mut [u8], size: usize) -> Result<()> {
    let size = u64::try_from(size)
        .map_err(|_| Error::new(TAR_WRITE, "the new payload is too large for a tar header"))?;
    if size > MAX_SIZE {
        return Err(Error::new(
            TAR_WRITE,
            "the new payload is larger than a ustar header can describe",
        ));
    }
    let text = format!("{size:011o}\0");
    let field = header
        .get_mut(SIZE_OFFSET..SIZE_OFFSET + SIZE_LEN)
        .ok_or_else(|| Error::new(TAR_WRITE, "the tar header is too short"))?;
    field.copy_from_slice(text.as_bytes());
    Ok(())
}

fn write_checksum(header: &mut [u8]) -> Result<()> {
    for value in header
        .get_mut(CHECKSUM_OFFSET..CHECKSUM_OFFSET + CHECKSUM_LEN)
        .ok_or_else(|| Error::new(TAR_WRITE, "the tar header is too short"))?
    {
        *value = b' ';
    }
    let sum = checksum(header);
    let text = format!("{sum:06o}\0 ");
    let field = header
        .get_mut(CHECKSUM_OFFSET..CHECKSUM_OFFSET + CHECKSUM_LEN)
        .ok_or_else(|| Error::new(TAR_WRITE, "the tar header is too short"))?;
    field.copy_from_slice(text.as_bytes());
    Ok(())
}

impl Layer for TarLayer {
    fn id(&self) -> &'static str {
        "container.tar"
    }

    fn sniff(&self, input: &[u8], _ctx: &SniffCtx<'_>) -> Confidence {
        let Some(block) = input.get(..BLOCK) else {
            return Confidence::NONE;
        };
        if block.get(MAGIC_OFFSET..MAGIC_OFFSET + MAGIC.len()) != Some(MAGIC) {
            return Confidence::NONE;
        }
        let Some(stored) = octal(
            block
                .get(CHECKSUM_OFFSET..CHECKSUM_OFFSET + CHECKSUM_LEN)
                .unwrap_or_default(),
        ) else {
            return Confidence::NONE;
        };
        if stored != checksum(block) {
            return Confidence::NONE;
        }
        if parse(input).is_err() {
            return Confidence::NONE;
        }
        Confidence::new(0.85)
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<(Vec<u8>, Params)> {
        ctx.limits().check_input(len_u64(input.len()))?;
        let archive = parse(input)?;
        let entry = payload(&archive, input, ctx)?;
        let data = read_entry(input, entry).to_vec();
        let mut params = Params::new();
        params.set(ENTRY_PARAM, entry.name.clone());
        params.set(
            ENTRY_OFFSET_PARAM,
            i64::try_from(entry.header).unwrap_or(i64::MAX),
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
        let archive = parse(source)?;
        let chosen = archive
            .entries
            .iter()
            .find(|entry| Some(entry.header) == offset)
            .or_else(|| archive.entries.iter().find(|entry| entry.name == name))
            .ok_or_else(|| {
                Error::new(
                    codes::CORE_INVALID_PARAM,
                    "the parameter \"entry\" names no entry of the archive",
                )
            })?;
        let mut out = Vec::with_capacity(source.len().saturating_add(inner.len()));
        for entry in &archive.entries {
            if entry.header == chosen.header {
                let mut header = source
                    .get(entry.header..entry.header.saturating_add(BLOCK))
                    .ok_or_else(|| Error::new(TAR_WRITE, "the tar header is cut short"))?
                    .to_vec();
                write_size(&mut header, inner.len())?;
                write_checksum(&mut header)?;
                out.extend_from_slice(&header);
                out.extend_from_slice(inner);
                let padding = (BLOCK - (inner.len() % BLOCK)) % BLOCK;
                out.resize(out.len().saturating_add(padding), 0);
            } else {
                out.extend_from_slice(
                    source
                        .get(entry.header..entry.end)
                        .ok_or_else(|| Error::new(TAR_WRITE, "the tar archive is cut short"))?,
                );
            }
        }
        out.extend_from_slice(source.get(archive.tail..).unwrap_or_default());
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use saveprism_core::{Limits, codes};

    use super::*;

    fn fixture() -> Vec<u8> {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/synthetic/layers/save.tar");
        std::fs::read(path).unwrap()
    }

    fn decode(input: &[u8]) -> Result<(Vec<u8>, Params)> {
        let limits = Limits::default();
        let mut ctx = DecodeCtx::new(&limits);
        TarLayer.decode(input, &mut ctx)
    }

    #[test]
    fn fixture_round_trips_exactly() {
        let bytes = fixture();
        let (inner, params) = decode(&bytes).unwrap();
        assert!(inner.starts_with(b"{"));
        assert_eq!(params.bytes(ENTRY_PARAM).unwrap(), b"save.json");
        assert_eq!(TarLayer.encode(&inner, &params).unwrap(), bytes);
    }

    #[test]
    fn editing_the_payload_keeps_the_other_entries_identical() {
        let bytes = fixture();
        let (mut inner, params) = decode(&bytes).unwrap();
        inner.extend_from_slice(b"\n");
        let written = TarLayer.encode(&inner, &params).unwrap();
        let before = parse(&bytes).unwrap();
        let after = parse(&written).unwrap();
        let readme_before = before
            .entries
            .iter()
            .find(|entry| entry.name == b"readme.txt")
            .unwrap();
        let readme_after = after
            .entries
            .iter()
            .find(|entry| entry.name == b"readme.txt")
            .unwrap();
        assert_eq!(
            read_entry(&bytes, readme_before),
            read_entry(&written, readme_after)
        );
        let (again, _) = decode(&written).unwrap();
        assert_eq!(again, inner);
    }

    #[test]
    fn a_larger_payload_moves_the_following_entry_but_keeps_it() {
        let bytes = fixture();
        let (inner, params) = decode(&bytes).unwrap();
        let mut bigger = inner.clone();
        bigger.extend(std::iter::repeat_n(b' ', BLOCK * 2));
        let written = TarLayer.encode(&bigger, &params).unwrap();
        let (again, _) = decode(&written).unwrap();
        assert_eq!(again, bigger);
        let after = parse(&written).unwrap();
        assert!(
            after
                .entries
                .iter()
                .any(|entry| entry.name == b"readme.txt")
        );
    }

    #[test]
    fn damaged_archives_give_the_code() {
        assert_eq!(decode(b"").unwrap_err().code(), TAR_DAMAGED);
        let bytes = fixture();
        let mut bad_sum = bytes.clone();
        let at = CHECKSUM_OFFSET;
        *bad_sum.get_mut(at).unwrap() ^= b'1';
        assert_eq!(decode(&bad_sum).unwrap_err().code(), TAR_DAMAGED);
        let mut cut = bytes;
        cut.truncate(700);
        assert_eq!(decode(&cut).unwrap_err().code(), TAR_DAMAGED);
    }

    #[test]
    fn sniff_checks_the_ustar_magic_and_checksum() {
        let ctx = SniffCtx::default();
        let bytes = fixture();
        assert!(TarLayer.sniff(&bytes, &ctx) > Confidence::MEDIUM);
        assert_eq!(TarLayer.sniff(b"short", &ctx), Confidence::NONE);
        let mut broken = bytes;
        let at = CHECKSUM_OFFSET;
        *broken.get_mut(at).unwrap() ^= b'1';
        assert_eq!(TarLayer.sniff(&broken, &ctx), Confidence::NONE);
    }

    #[test]
    fn a_bomb_declared_by_a_header_is_refused() {
        let bytes = fixture();
        let limits = Limits {
            max_decoded_bytes: 8,
            ..Limits::default()
        };
        let mut ctx = DecodeCtx::new(&limits);
        assert_eq!(
            TarLayer.decode(&bytes, &mut ctx).unwrap_err().code(),
            codes::CORE_LIMIT_DECODED
        );
    }
}
