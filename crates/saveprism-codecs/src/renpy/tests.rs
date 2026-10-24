//! Tests of the Ren'Py save layer.

use std::path::PathBuf;

use proptest::prelude::*;
use saveprism_core::{Confidence, DecodeCtx, Layer, Limits, Params, SniffCtx, codes};

use super::*;

fn fixture(relative: &str) -> Vec<u8> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");
    std::fs::read(root.join(relative)).unwrap()
}

const UNSIGNED: &str = "synthetic/renpy/renpy8-0/1-1-LT1.save";
const SIGNED: &str = "synthetic/renpy/renpy8-1-signed/1-1-LT1.save";

fn decode_with(input: &[u8], limits: &Limits) -> Result<(Vec<u8>, Params)> {
    let mut ctx = DecodeCtx::new(limits);
    RenpySaveLayer.decode(input, &mut ctx)
}

fn decode(input: &[u8]) -> (Vec<u8>, Params) {
    decode_with(input, &Limits::default()).unwrap()
}

/// Name and raw stored bytes (local header to the next entry) of every entry, plus the
/// uncompressed contents, read with this module's own parser.
fn entries(input: &[u8]) -> Vec<(String, Vec<u8>, Vec<u8>)> {
    let archive = parse(input).unwrap();
    let limits = Limits::default();
    let ctx = DecodeCtx::new(&limits);
    archive
        .entries
        .iter()
        .map(|entry| {
            (
                String::from_utf8(entry.name.clone()).unwrap(),
                input.get(entry.data..entry.span_end).unwrap().to_vec(),
                read_entry(input, entry, &ctx).unwrap(),
            )
        })
        .collect()
}

#[test]
fn decode_gives_the_log_and_whether_the_save_is_signed() {
    for (path, signed) in [(UNSIGNED, false), (SIGNED, true)] {
        let input = fixture(path);
        let (log, params) = decode(&input);
        assert!(log.starts_with(&[0x80, 0x02]));
        assert_eq!(log.last(), Some(&b'.'));
        assert_eq!(params.bool(SIGNED_PARAM).unwrap(), signed);
        assert_eq!(params.bytes(ARCHIVE_PARAM).unwrap(), input.as_slice());
        // The same log gives the same archive.
        assert_eq!(RenpySaveLayer.encode(&log, &params).unwrap(), input);
    }
}

#[test]
fn a_new_log_is_written_in_place_and_the_signatures_are_emptied() {
    let input = fixture(SIGNED);
    let (log, params) = decode(&input);
    // Push and pop a None before STOP: another valid pickle of the same state.
    let mut changed = log.clone();
    changed.insert(changed.len() - 1, b'N');
    changed.insert(changed.len() - 1, b'0');
    let written = RenpySaveLayer.encode(&changed, &params).unwrap();

    let before = entries(&input);
    let after = entries(&written);
    let names: Vec<&str> = after.iter().map(|(name, _, _)| name.as_str()).collect();
    assert_eq!(
        names,
        [
            "screenshot.png",
            "extra_info",
            "json",
            "renpy_version",
            "log",
            "signatures"
        ]
    );
    for ((name, stored_before, data_before), (_, stored_after, data_after)) in
        before.iter().zip(&after)
    {
        match name.as_str() {
            "log" => assert_eq!(data_after, &changed),
            "signatures" => {
                assert!(!data_before.is_empty());
                assert!(data_after.is_empty());
            }
            _ => {
                assert_eq!(stored_after, stored_before, "{name}");
                assert_eq!(data_after, data_before, "{name}");
            }
        }
    }
    let (again, params) = decode(&written);
    assert_eq!(again, changed);
    assert!(!params.bool(SIGNED_PARAM).unwrap());
    // Writing the edited archive again with its own log changes nothing.
    assert_eq!(RenpySaveLayer.encode(&again, &params).unwrap(), written);
}

#[test]
fn an_unsigned_save_gets_no_signatures_entry() {
    let input = fixture(UNSIGNED);
    let (mut log, params) = decode(&input);
    log.insert(log.len() - 1, b'N');
    log.insert(log.len() - 1, b'0');
    let written = RenpySaveLayer.encode(&log, &params).unwrap();
    let names: Vec<String> = entries(&written)
        .into_iter()
        .map(|(name, _, _)| name)
        .collect();
    assert!(!names.contains(&SIGNATURES_ENTRY.to_string()));
    assert_eq!(decode(&written).0, log);
}

#[test]
fn sniff_wants_a_zip_with_a_log() {
    let ctx = SniffCtx::default();
    let save = fixture(UNSIGNED);
    assert_eq!(RenpySaveLayer.sniff(&save, &ctx), Confidence::HIGH);
    let named = SniffCtx {
        file_name: Some("1-1-LT1.save"),
        depth: 0,
    };
    assert!(RenpySaveLayer.sniff(&save, &named) > Confidence::HIGH);
    assert_eq!(RenpySaveLayer.sniff(b"PK\x03\x04", &ctx), Confidence::NONE);
    assert_eq!(RenpySaveLayer.sniff(b"{}", &ctx), Confidence::NONE);
    // A zip whose entries do not include a log is somebody else's archive.
    let mut renamed = save.clone();
    for at in 0..renamed.len().saturating_sub(3) {
        if renamed.get(at..at + 3) == Some(b"log") {
            renamed.get_mut(at..at + 3).unwrap().copy_from_slice(b"lag");
        }
    }
    assert_eq!(RenpySaveLayer.sniff(&renamed, &ctx), Confidence::NONE);
    assert!(decode_with(&renamed, &Limits::default()).is_err());
}

#[test]
fn a_large_entry_is_stopped_by_the_budget() {
    let save = fixture(UNSIGNED);
    // The log is 1 316 bytes uncompressed.
    let limits = Limits {
        max_decoded_bytes: 1000,
        ..Limits::default()
    };
    let error = decode_with(&save, &limits).unwrap_err();
    assert_eq!(error.code(), codes::CORE_LIMIT_DECODED);
}

#[test]
fn damaged_archives_give_errors() {
    let save = fixture(UNSIGNED);
    for cut in 0..save.len() {
        assert!(decode_with(save.get(..cut).unwrap(), &Limits::default()).is_err());
    }
    // A wrong CRC.
    let archive = parse(&save).unwrap();
    let log = archive.find(LOG_ENTRY).unwrap();
    let mut bad = save.clone();
    *bad.get_mut(log.central.0 + 16).unwrap() ^= 0xFF;
    assert_eq!(
        decode_with(&bad, &Limits::default()).unwrap_err().code(),
        RENPY_ZIP
    );
    // An encrypted entry.
    let mut locked = save.clone();
    *locked.get_mut(log.central.0 + 8).unwrap() |= 1;
    assert_eq!(
        decode_with(&locked, &Limits::default()).unwrap_err().code(),
        RENPY_UNSUPPORTED
    );
}

#[test]
fn error_codes_are_stable() {
    assert_eq!(RENPY_ZIP.as_str(), "SP-RENPY-001");
    assert_eq!(RENPY_UNSUPPORTED.as_str(), "SP-RENPY-002");
    assert_eq!(RENPY_WRITE.as_str(), "SP-RENPY-003");
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Mutated saves never panic, and what decodes writes back.
    #[test]
    fn mutated_saves_never_panic(position in 0_usize..1600, byte in any::<u8>()) {
        let mut bytes = fixture(SIGNED);
        if let Some(slot) = bytes.get_mut(position) {
            *slot = byte;
        }
        if let Ok((log, params)) = decode_with(&bytes, &Limits::default()) {
            prop_assert_eq!(RenpySaveLayer.encode(&log, &params).unwrap(), bytes);
        }
    }
}
