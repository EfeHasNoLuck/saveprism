//! `container.erf`: BioWare Encapsulated Resource File (`.erf`, `.mod`, `.hak`, `.sav`).
//!
//! The layout is implemented from BioWare's public "Encapsulated Resource File Format" document,
//! distributed with the Neverwinter Nights toolset (mirrored at
//! <https://nwn.wiki/spaces/NWN1/pages/327727/File+Format+Specification+Bioware>, file
//! `Bioware_Aurora_ERF_Format.pdf`). The ResType-to-extension table is from the same package's
//! "Key and BIF" document (Table 1.3.1). No code was copied from a game or a GPL reader.
//!
//! Like the tar and zip layers, one resource is the payload: it is the only resource, or the
//! largest resource whose bytes another registered codec or layer recognises. The chosen ResRef
//! and its entry index are kept in the parameters `entry` and `entry_index`; decoding returns the
//! resource bytes and the rest of the pipeline detects the format inside (KOTOR's
//! `SAVEGAME.sav` is an ERF full of GFF files).
//!
//! An unchanged archive is returned from the original bytes. When the payload changes, the
//! resource list is rewritten (offset and size of every resource, recomputed contiguously in key
//! order) and only the chosen resource's data is replaced; all other resource bytes are copied
//! exactly. The header, localized description strings, key list and any prefix padding stay
//! byte-identical.

use std::sync::Arc;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    Confidence, DecodeCtx, Error, ErrorCode, Layer, Params, Registry, Result, SniffCtx, codes,
};

use super::{SOURCE_PARAM, inner_is_known, invalid_param, keep_source, source_if_unchanged};

/// The archive is damaged, cut short or its tables do not fit.
pub const ERF_DAMAGED: ErrorCode = ErrorCode::new("SP-ERF-001");
/// No resource holds something another codec recognises.
pub const ERF_NO_PAYLOAD: ErrorCode = ErrorCode::new("SP-ERF-002");
/// The archive could not be written again.
pub const ERF_WRITE: ErrorCode = ErrorCode::new("SP-ERF-003");

/// Parameter: ResRef of the chosen resource.
pub const ENTRY_PARAM: &str = "entry";
/// Parameter: zero-based index of the chosen resource in the key list.
pub const ENTRY_INDEX_PARAM: &str = "entry_index";
/// Parameter: human-readable file name of the chosen resource (`name.ext`).
pub const ENTRY_NAME_PARAM: &str = "entry_name";

const HEADER_LEN: usize = 160;
const KEY_LEN: usize = 24;
const RESOURCE_LEN: usize = 8;
const RESREF_LEN: usize = 16;
const V1_0: &[u8; 4] = b"V1.0";
const V1_1: &[u8; 4] = b"V1.1";
const TAGS: [&[u8; 4]; 5] = [b"ERF ", b"MOD ", b"SAV ", b"HAK ", b"NWM "];

/// Registers the ERF container layer.
pub fn register(reg: &mut Registry) {
    reg.add_layer(Arc::new(ErfLayer));
}

/// The `container.erf` layer.
#[derive(Clone, Copy, Debug, Default)]
pub struct ErfLayer;

#[derive(Clone, Debug)]
struct Header {
    entry_count: usize,
    resource_offset: usize,
}

#[derive(Clone, Debug)]
struct Entry {
    resref: Vec<u8>,
    res_type: u16,
    offset: usize,
    size: usize,
}

fn u32_at(input: &[u8], offset: usize) -> Option<u32> {
    let bytes = input.get(offset..offset.checked_add(4)?)?;
    Some(u32::from_le_bytes([
        *bytes.first()?,
        *bytes.get(1)?,
        *bytes.get(2)?,
        *bytes.get(3)?,
    ]))
}

fn bad(message: impl Into<String>, offset: usize) -> Error {
    Error::new(ERF_DAMAGED, message).with_offset(len_u64(offset))
}

fn write_error(message: impl Into<String>) -> Error {
    Error::new(ERF_WRITE, message)
}

fn no_payload() -> Error {
    Error::new(
        ERF_NO_PAYLOAD,
        "no resource holds a format another codec recognises",
    )
}

fn known_tag(tag: &[u8]) -> bool {
    TAGS.iter().any(|known| known.as_slice() == tag)
}

fn parse(input: &[u8]) -> Result<(Header, Vec<Entry>)> {
    if input.len() < HEADER_LEN {
        return Err(bad("the ERF is shorter than its header", 0));
    }
    let tag: [u8; 4] = input
        .get(..4)
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(|| bad("the ERF file tag is missing", 0))?;
    if !known_tag(&tag) {
        return Err(bad("the file is not an ERF, MOD, SAV, HAK or NWM", 0));
    }
    let version = input.get(4..8).unwrap_or_default();
    if version != V1_0 && version != V1_1 {
        return Err(bad("the ERF version is not V1.0 or V1.1", 4));
    }
    let get = |offset: usize| {
        u32_at(input, offset).ok_or_else(|| bad("the ERF header is cut short", offset))
    };
    let localized_size =
        usize::try_from(get(12)?).map_err(|_| bad("the localized string size is too large", 12))?;
    let entry_count =
        usize::try_from(get(16)?).map_err(|_| bad("the resource count is too large", 16))?;
    let localized_offset = usize::try_from(get(20)?)
        .map_err(|_| bad("the localized string offset is too large", 20))?;
    let key_offset =
        usize::try_from(get(24)?).map_err(|_| bad("the key list offset is too large", 24))?;
    let resource_offset =
        usize::try_from(get(28)?).map_err(|_| bad("the resource list offset is too large", 28))?;
    for (offset, size, name) in [
        (localized_offset, localized_size, "localized string"),
        (
            key_offset,
            entry_count
                .checked_mul(KEY_LEN)
                .ok_or_else(|| bad("the key list overflows", 24))?,
            "key",
        ),
        (
            resource_offset,
            entry_count
                .checked_mul(RESOURCE_LEN)
                .ok_or_else(|| bad("the resource list overflows", 28))?,
            "resource",
        ),
    ] {
        let end = offset
            .checked_add(size)
            .ok_or_else(|| bad(format!("the {name} table overflows"), offset))?;
        if end > input.len() {
            return Err(bad(
                format!("the {name} table lies beyond the file"),
                offset,
            ));
        }
    }
    let mut entries = Vec::with_capacity(entry_count);
    for index in 0..entry_count {
        let key_at = key_offset
            .checked_add(index.saturating_mul(KEY_LEN))
            .ok_or_else(|| bad("a key offset overflows", key_offset))?;
        let resref_bytes = input
            .get(key_at..key_at.saturating_add(RESREF_LEN))
            .ok_or_else(|| bad("a ResRef is cut short", key_at))?;
        let end = resref_bytes
            .iter()
            .position(|&byte| byte == 0)
            .unwrap_or(RESREF_LEN);
        let resref = resref_bytes.get(..end).unwrap_or_default().to_vec();
        if resref.is_empty() {
            return Err(bad("a key has an empty ResRef", key_at));
        }
        // Key layout: ResRef 16 bytes, ResID u32, ResType u16, Unused u16.
        let res_type = u32_at(input, key_at.saturating_add(20))
            .ok_or_else(|| bad("a key entry is cut short", key_at))? as u16;
        let resource_at = resource_offset
            .checked_add(index.saturating_mul(RESOURCE_LEN))
            .ok_or_else(|| bad("a resource offset overflows", resource_offset))?;
        let offset = usize::try_from(
            u32_at(input, resource_at)
                .ok_or_else(|| bad("a resource entry is cut short", resource_at))?,
        )
        .map_err(|_| bad("a resource offset is too large", resource_at))?;
        let size = usize::try_from(
            u32_at(input, resource_at.saturating_add(4))
                .ok_or_else(|| bad("a resource entry is cut short", resource_at))?,
        )
        .map_err(|_| bad("a resource size is too large", resource_at))?;
        let end = offset
            .checked_add(size)
            .ok_or_else(|| bad("a resource range overflows", resource_at))?;
        if end > input.len() {
            return Err(bad("a resource lies beyond the file", resource_at));
        }
        entries.push(Entry {
            resref,
            res_type,
            offset,
            size,
        });
    }
    Ok((
        Header {
            entry_count,
            resource_offset,
        },
        entries,
    ))
}

/// The public BioWare resource type table (Neverwinter Nights "Key and BIF" document).
fn extension(res_type: u16) -> Option<&'static str> {
    Some(match res_type {
        1 => "bmp",
        3 => "tga",
        4 => "wav",
        6 => "plt",
        7 => "ini",
        10 => "txt",
        2002 => "mdl",
        2009 => "nss",
        2010 => "ncs",
        2012 => "are",
        2013 => "set",
        2014 => "ifo",
        2015 => "bic",
        2016 => "wok",
        2017 => "2da",
        2022 => "txi",
        2023 => "git",
        2025 => "uti",
        2027 => "utc",
        2029 => "dlg",
        2030 => "itp",
        2032 => "utt",
        2033 => "dds",
        2035 => "uts",
        2036 => "ltr",
        2037 => "gff",
        2038 => "fac",
        2040 => "ute",
        2042 => "utd",
        2044 => "utp",
        2045 => "dft",
        2046 => "gic",
        2047 => "gui",
        2051 => "utm",
        2052 => "dwk",
        2053 => "pwk",
        2056 => "jrl",
        2058 => "utw",
        2060 => "ssf",
        2064 => "ndb",
        2065 => "ptm",
        2066 => "ptt",
        _ => return None,
    })
}

/// File name shown for a resource: `name.ext`, or `name.type<id>` for an unknown type.
pub fn resource_name(resref: &[u8], res_type: u16) -> Vec<u8> {
    let mut name = resref.to_vec();
    name.push(b'.');
    match extension(res_type) {
        Some(extension) => name.extend_from_slice(extension.as_bytes()),
        None => name.extend_from_slice(format!("type{res_type}").as_bytes()),
    }
    name
}

fn read_entry<'a>(input: &'a [u8], entry: &Entry) -> &'a [u8] {
    input
        .get(entry.offset..entry.offset.saturating_add(entry.size))
        .unwrap_or_default()
}

fn payload<'a>(entries: &'a [Entry], input: &[u8], ctx: &DecodeCtx<'_>) -> Result<&'a Entry> {
    let budget = ctx.output_budget(len_u64(input.len()));
    for entry in entries {
        if len_u64(entry.size) > budget {
            return Err(Error::new(
                codes::CORE_LIMIT_DECODED,
                format!(
                    "a resource would be {} bytes, the budget is {budget} bytes (possible decompression bomb)",
                    entry.size
                ),
            )
            .with_offset(len_u64(entry.offset)));
        }
    }
    match entries {
        [] => Err(no_payload()),
        [only] => Ok(only),
        _ => {
            let mut ordered: Vec<&Entry> = entries.iter().collect();
            ordered.sort_by(|left, right| {
                right
                    .size
                    .cmp(&left.size)
                    .then_with(|| left.offset.cmp(&right.offset))
            });
            for entry in ordered {
                if inner_is_known(read_entry(input, entry)) {
                    return Ok(entry);
                }
            }
            Err(no_payload())
        }
    }
}

fn put_u32(out: &mut [u8], offset: usize, value: usize) -> Result<()> {
    let value =
        u32::try_from(value).map_err(|_| write_error("the ERF grew larger than four gigabytes"))?;
    let target = out
        .get_mut(offset..offset.saturating_add(4))
        .ok_or_else(|| write_error("the resource list is cut short"))?;
    target.copy_from_slice(&value.to_le_bytes());
    Ok(())
}

impl Layer for ErfLayer {
    fn id(&self) -> &'static str {
        "container.erf"
    }

    /// A known four-byte tag, `V1.0`/`V1.1` and tables that fit the input.
    fn sniff(&self, input: &[u8], ctx: &SniffCtx<'_>) -> Confidence {
        let Some(tag) = input.get(..4) else {
            return Confidence::NONE;
        };
        if !known_tag(tag) {
            return Confidence::NONE;
        }
        if input.get(4..8) != Some(V1_0) && input.get(4..8) != Some(V1_1) {
            return Confidence::NONE;
        }
        if parse(input).is_err() {
            return Confidence::NONE;
        }
        let named = ctx.file_name.is_some_and(|name| {
            let lower = name.to_ascii_lowercase();
            [".erf", ".mod", ".hak", ".nwm", ".sav"]
                .iter()
                .any(|extension| lower.ends_with(extension))
        });
        Confidence::new(if named { 0.97 } else { 0.9 })
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<(Vec<u8>, Params)> {
        ctx.limits().check_input(len_u64(input.len()))?;
        let (_header, entries) = parse(input)?;
        let entry = payload(&entries, input, ctx)?;
        let index = entries
            .iter()
            .position(|candidate| candidate.offset == entry.offset && candidate.size == entry.size)
            .ok_or_else(no_payload)?;
        let data = read_entry(input, entry).to_vec();
        let mut params = Params::new();
        params.set(ENTRY_PARAM, entry.resref.clone());
        params.set(
            ENTRY_NAME_PARAM,
            resource_name(&entry.resref, entry.res_type),
        );
        params.set(ENTRY_INDEX_PARAM, i64::try_from(index).unwrap_or(i64::MAX));
        keep_source(&mut params, input, &data);
        Ok((data, params))
    }

    fn encode(&self, inner: &[u8], params: &Params) -> Result<Vec<u8>> {
        if let Some(source) = source_if_unchanged(inner, params)? {
            return Ok(source);
        }
        let source = params.bytes(SOURCE_PARAM)?;
        let name = params.bytes(ENTRY_PARAM)?;
        let index = params
            .opt_int(ENTRY_INDEX_PARAM)?
            .and_then(|value| usize::try_from(value).ok());
        let (header, entries) = parse(source)?;
        let chosen = index
            .and_then(|index| entries.get(index).map(|entry| (index, entry)))
            .filter(|(_, entry)| entry.resref == name)
            .or_else(|| {
                entries
                    .iter()
                    .position(|entry| entry.resref == name)
                    .and_then(|index| entries.get(index).map(|entry| (index, entry)))
            })
            .ok_or_else(|| invalid_param("the parameter \"entry\" names no resource".to_owned()))?;
        let (chosen_index, _chosen_entry) = chosen;
        if header.entry_count != entries.len() {
            return Err(write_error("the ERF key and resource lists disagree"));
        }
        let data_start = entries
            .iter()
            .map(|entry| entry.offset)
            .min()
            .ok_or_else(|| write_error("the ERF has no resource data"))?;
        let list_end = header
            .resource_offset
            .saturating_add(entries.len().saturating_mul(RESOURCE_LEN));
        if list_end > data_start || list_end > source.len() {
            return Err(write_error(
                "the ERF resource list overlaps the resource data",
            ));
        }
        let mut out = source.get(..data_start).unwrap_or_default().to_vec();
        for (index, entry) in entries.iter().enumerate() {
            let offset = out.len();
            if index == chosen_index {
                out.extend_from_slice(inner);
            } else {
                out.extend_from_slice(read_entry(source, entry));
            }
            let at = header
                .resource_offset
                .saturating_add(index.saturating_mul(RESOURCE_LEN));
            let size = out.len().saturating_sub(offset);
            put_u32(&mut out, at, offset)?;
            put_u32(&mut out, at.saturating_add(4), size)?;
        }
        let tail = entries
            .iter()
            .map(|entry| entry.offset.saturating_add(entry.size))
            .max()
            .unwrap_or_default();
        if tail < source.len() {
            out.extend_from_slice(source.get(tail..).unwrap_or_default());
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use saveprism_core::{Layer, Limits};

    use super::*;

    fn fixture(name: &str) -> Vec<u8> {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/synthetic/bioware")
            .join(name);
        std::fs::read(path).expect("fixture is readable")
    }

    fn decode(input: &[u8]) -> Result<(Vec<u8>, Params)> {
        let limits = Limits::default();
        let mut ctx = DecodeCtx::new(&limits);
        ErfLayer.decode(input, &mut ctx)
    }

    #[test]
    fn kotor_save_lists_its_resources_and_round_trips() {
        let bytes = fixture("kotor-save.sav");
        let (inner, params) = decode(&bytes).expect("valid ERF");
        assert!(inner.starts_with(b"BIC "));
        assert_eq!(params.bytes(ENTRY_PARAM).expect("entry"), b"character");
        assert_eq!(
            params.bytes(ENTRY_NAME_PARAM).expect("entry name"),
            b"character.bic"
        );
        assert_eq!(params.opt_int(ENTRY_INDEX_PARAM).expect("index"), Some(0));
        assert_eq!(
            ErfLayer.encode(&inner, &params).expect("unchanged archive"),
            bytes
        );
    }

    #[test]
    fn editing_the_payload_keeps_the_other_resource_identical() {
        let bytes = fixture("kotor-save.sav");
        let (inner, params) = decode(&bytes).expect("valid ERF");
        let (header, entries) = parse(&bytes).expect("valid ERF");
        let module = entries
            .iter()
            .find(|entry| entry.resref == b"module")
            .expect("module resource");
        let module_before = read_entry(&bytes, module).to_vec();
        let mut changed = inner.clone();
        changed.extend_from_slice(b"EXTRA");
        let written = ErfLayer
            .encode(&changed, &params)
            .expect("edited archive is written");
        let (_, after) = parse(&written).expect("written archive parses");
        let module_after = after
            .iter()
            .find(|entry| entry.resref == b"module")
            .expect("module survives");
        assert_eq!(read_entry(&written, module_after), module_before);
        let (again, _) = decode(&written).expect("written archive decodes");
        assert_eq!(again, changed);
        assert_eq!(after.len(), header.entry_count);
    }

    #[test]
    fn a_larger_payload_moves_the_later_resource() {
        let bytes = fixture("kotor-save.sav");
        let (inner, params) = decode(&bytes).expect("valid ERF");
        let mut bigger = inner.clone();
        bigger.extend(std::iter::repeat_n(b' ', 500));
        let written = ErfLayer
            .encode(&bigger, &params)
            .expect("edited archive is written");
        let (_, before) = parse(&bytes).expect("original parses");
        let (_, after) = parse(&written).expect("written archive parses");
        let module_before = before
            .iter()
            .find(|e| e.resref == b"module")
            .expect("before");
        let module_after = after.iter().find(|e| e.resref == b"module").expect("after");
        assert!(module_after.offset > module_before.offset);
        assert_eq!(
            read_entry(&bytes, module_before),
            read_entry(&written, module_after)
        );
        let (again, _) = decode(&written).expect("written archive decodes");
        assert_eq!(again, bigger);
    }

    #[test]
    fn resource_types_map_to_the_public_extension_table() {
        assert_eq!(resource_name(b"character", 2015), b"character.bic");
        assert_eq!(resource_name(b"module", 2014), b"module.ifo");
        assert_eq!(resource_name(b"area001", 2012), b"area001.are");
        assert_eq!(resource_name(b"odd", 4242), b"odd.type4242");
        assert_eq!(resource_name(b"readme", 10), b"readme.txt");
    }

    #[test]
    fn damaged_archives_give_the_codes() {
        assert_eq!(decode(b"").err().map(|e| e.code()), Some(ERF_DAMAGED));
        let bytes = fixture("kotor-save.sav");
        let mut bad_tag = bytes.clone();
        if let Some(byte) = bad_tag.get_mut(0) {
            *byte = b'X';
        }
        assert_eq!(decode(&bad_tag).err().map(|e| e.code()), Some(ERF_DAMAGED));
        let mut bad_version = bytes.clone();
        if let Some(byte) = bad_version.get_mut(7) {
            *byte = b'9';
        }
        assert_eq!(
            decode(&bad_version).err().map(|e| e.code()),
            Some(ERF_DAMAGED)
        );
        let mut cut = bytes;
        cut.truncate(200);
        assert_eq!(decode(&cut).err().map(|e| e.code()), Some(ERF_DAMAGED));
    }

    #[test]
    fn sniff_checks_the_tag_and_tables() {
        let bytes = fixture("kotor-save.sav");
        let ctx = SniffCtx {
            file_name: Some("SAVEGAME.sav"),
            ..SniffCtx::default()
        };
        assert!(ErfLayer.sniff(&bytes, &ctx) > Confidence::MEDIUM);
        assert_eq!(ErfLayer.sniff(b"not an erf", &ctx), Confidence::NONE);
        let gff = fixture("character.bic");
        assert_eq!(ErfLayer.sniff(&gff, &ctx), Confidence::NONE);
    }

    #[test]
    fn an_entry_explosion_is_refused_by_the_budget() {
        let bytes = fixture("kotor-save.sav");
        let limits = Limits {
            max_decoded_bytes: 8,
            ..Limits::default()
        };
        let mut ctx = DecodeCtx::new(&limits);
        assert_eq!(
            ErfLayer
                .decode(&bytes, &mut ctx)
                .err()
                .map(|error| error.code()),
            Some(saveprism_core::codes::CORE_LIMIT_DECODED)
        );
    }
}
