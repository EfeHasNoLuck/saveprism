//! Tests of the zip container layer.

use std::path::PathBuf;

use proptest::prelude::*;
use saveprism_core::detect::{DetectOptions, detect};
use saveprism_core::guard::{Verdict, check_round_trip};
use saveprism_core::{
    Confidence, DecodeCtx, Edit, Layer, Limits, OwnedValue, Path, Registry, SniffCtx, codes,
    pipeline,
};

use super::*;
use crate::register_all;

fn fixture(relative: &str) -> Vec<u8> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");
    std::fs::read(root.join(relative)).unwrap()
}

const FIXTURES: [&str; 3] = [
    "synthetic/zip/single-json.zip",
    "synthetic/zip/stored-es3.zip",
    "synthetic/zip/marshal-payload.zip",
];

const RENPY: [&str; 2] = [
    "synthetic/renpy/renpy8-0/1-1-LT1.save",
    "synthetic/renpy/renpy8-1-signed/1-1-LT1.save",
];

fn registry() -> Registry {
    let mut reg = Registry::default();
    register_all(&mut reg);
    reg
}

fn decode_with(input: &[u8], limits: &Limits) -> Result<(Vec<u8>, Params)> {
    let mut ctx = DecodeCtx::new(limits);
    ZipLayer.decode(input, &mut ctx)
}

fn decode(input: &[u8]) -> (Vec<u8>, Params) {
    decode_with(input, &Limits::default()).unwrap()
}

fn recipe_for(bytes: &[u8], reg: &Registry) -> saveprism_core::Recipe {
    detect(bytes, reg, &Limits::default(), &DetectOptions::default())
        .unwrap()
        .best
        .recipe
}

/// Name and raw stored bytes (local header to the next entry) of every entry, read with the shared
/// parser.
fn stored_entries(input: &[u8]) -> Vec<(Vec<u8>, Vec<u8>)> {
    let archive = parse(input, &CODES).unwrap();
    archive
        .entries
        .iter()
        .map(|entry| {
            (
                entry.name.clone(),
                input.get(entry.local..entry.span_end).unwrap().to_vec(),
            )
        })
        .collect()
}

#[test]
fn every_container_fixture_round_trips_exactly() {
    let reg = registry();
    let limits = Limits::default();
    for relative in FIXTURES {
        let bytes = fixture(relative);
        let recipe = recipe_for(&bytes, &reg);
        assert_eq!(recipe.kinds()[0], "container.zip", "{relative}");
        let report = check_round_trip(&bytes, &recipe, &reg, &limits).unwrap();
        assert_eq!(report.overall, Verdict::Exact, "{relative}");

        let decoded = pipeline::decode(&bytes, &recipe, &reg, &limits).unwrap();
        let written = pipeline::encode(&decoded.doc, &decoded.recipe, &reg).unwrap();
        assert_eq!(written, bytes, "{relative} does not round-trip");

        // The layer alone must also reproduce the file.
        let (inner, params) = decode(&bytes);
        assert_eq!(ZipLayer.encode(&inner, &params).unwrap(), bytes);
    }
}

#[test]
fn the_payload_entry_is_the_only_file_or_the_largest_recognised_one() {
    for (relative, name) in [
        (FIXTURES[0], b"save.json".as_slice()),
        (FIXTURES[1], b"save.es3".as_slice()),
        (FIXTURES[2], b"save.rvdata2".as_slice()),
    ] {
        let (_, params) = decode(&fixture(relative));
        assert_eq!(params.bytes(ENTRY_PARAM).unwrap(), name, "{relative}");
    }
}

#[test]
fn editing_the_inner_json_keeps_the_other_entries_byte_identical() {
    let reg = registry();
    let limits = Limits::default();
    let bytes = fixture(FIXTURES[1]);
    let recipe = recipe_for(&bytes, &reg);
    assert_eq!(
        recipe.kinds(),
        ["container.zip", "format.es3"],
        "the ES3-like payload should be read as ES3"
    );
    let mut decoded = pipeline::decode(&bytes, &recipe, &reg, &limits).unwrap();
    let edits = [Edit::Set {
        path: Path::parse("/gold/value").unwrap(),
        value: OwnedValue::Int(999),
    }];
    saveprism_core::apply_edits(&mut decoded.doc, &edits, &limits).unwrap();
    let written = pipeline::encode(&decoded.doc, &decoded.recipe, &reg).unwrap();
    assert_ne!(written, bytes);

    let again = pipeline::decode(&written, &recipe, &reg, &limits).unwrap();
    let id = again
        .doc
        .resolve(&Path::parse("/gold/value").unwrap())
        .unwrap();
    assert!(matches!(
        again.doc.get(id).unwrap().value,
        saveprism_core::Value::Int { v: 999, .. }
    ));

    // Only the payload entry changed; meta.txt is byte for byte the same.
    let before = stored_entries(&bytes);
    let after = stored_entries(&written);
    assert_eq!(before.len(), after.len());
    for ((name_before, stored_before), (name_after, stored_after)) in before.iter().zip(&after) {
        assert_eq!(name_before, name_after);
        if name_before.as_slice() == b"save.es3" {
            assert_ne!(stored_before, stored_after);
        } else {
            assert_eq!(stored_before, stored_after, "{name_before:?}");
        }
    }
}

#[test]
fn renpy_saves_still_belong_to_the_renpy_layer() {
    let reg = registry();
    for relative in RENPY {
        let bytes = fixture(relative);
        let recipe = recipe_for(&bytes, &reg);
        assert_eq!(recipe.kinds()[0], "container.renpy-save", "{relative}");
        assert!(!recipe.kinds().contains(&"container.zip"), "{relative}");
        assert_eq!(
            ZipLayer.sniff(&bytes, &SniffCtx::default()),
            Confidence::NONE
        );
    }
}

#[test]
fn ordinary_archives_are_sniffed_and_foreign_bytes_are_not() {
    let bytes = fixture(FIXTURES[0]);
    assert!(ZipLayer.sniff(&bytes, &SniffCtx::default()) > Confidence::MEDIUM);
    let named = SniffCtx {
        file_name: Some("slot.SAV"),
        depth: 0,
    };
    assert!(ZipLayer.sniff(&bytes, &named) > ZipLayer.sniff(&bytes, &SniffCtx::default()));
    assert_eq!(
        ZipLayer.sniff(b"PK\x03\x04", &SniffCtx::default()),
        Confidence::NONE
    );
    assert_eq!(
        ZipLayer.sniff(b"{}", &SniffCtx::default()),
        Confidence::NONE
    );
}

#[test]
fn truncated_and_corrupt_archives_give_errors() {
    let bytes = fixture(FIXTURES[0]);
    for cut in 0..bytes.len() {
        assert!(
            decode_with(bytes.get(..cut).unwrap(), &Limits::default()).is_err(),
            "cut at {cut}"
        );
    }

    let archive = parse(&bytes, &CODES).unwrap();
    let entry = archive.entries.first().unwrap();
    let mut bad_crc = bytes.clone();
    *bad_crc.get_mut(entry.central.0 + 16).unwrap() ^= 0xFF;
    assert_eq!(
        decode_with(&bad_crc, &Limits::default())
            .unwrap_err()
            .code(),
        ZIP_DAMAGED
    );

    // Declared size above the output budget: stopped before anything is decompressed.
    let mut bomb = bytes.clone();
    put_u32(&mut bomb, entry.central.0 + 24, u32::MAX - 1);
    assert_eq!(
        decode_with(&bomb, &Limits::default()).unwrap_err().code(),
        codes::CORE_LIMIT_DECODED
    );

    // An encrypted entry and a zip64 size are refused.
    let mut encrypted = bytes.clone();
    *encrypted.get_mut(entry.central.0 + 8).unwrap() |= 1;
    assert_eq!(
        decode_with(&encrypted, &Limits::default())
            .unwrap_err()
            .code(),
        ZIP_UNSUPPORTED
    );
    let mut zip64 = bytes.clone();
    put_u32(&mut zip64, entry.central.0 + 20, u32::MAX);
    assert_eq!(
        decode_with(&zip64, &Limits::default()).unwrap_err().code(),
        ZIP_UNSUPPORTED
    );
}

#[test]
fn an_archive_without_a_recognised_payload_is_an_error() {
    let bytes = fixture(FIXTURES[1]);
    let archive = parse(&bytes, &CODES).unwrap();
    let entry = archive.find(b"save.es3").unwrap();
    let mut unknown = bytes.clone();
    // Same length, stored, so only the CRC has to be fixed; the bytes are nothing a codec knows.
    if let Some(slot) = unknown.get_mut(entry.data..entry.data + entry.size) {
        slot.fill(0xFF);
    }
    for at in 0..unknown.len().saturating_sub(8) {
        if unknown.get(at..at + 8) == Some(b"save.es3") {
            unknown
                .get_mut(at..at + 8)
                .unwrap()
                .copy_from_slice(b"data.bin");
        }
    }
    let mut crc = flate2::Crc::new();
    crc.update(unknown.get(entry.data..entry.data + entry.size).unwrap());
    put_u32(&mut unknown, entry.local + 14, crc.sum());
    put_u32(&mut unknown, entry.central.0 + 16, crc.sum());
    assert_eq!(
        decode_with(&unknown, &Limits::default())
            .unwrap_err()
            .code(),
        ZIP_NO_PAYLOAD
    );
}

#[test]
fn error_codes_are_stable() {
    assert_eq!(ZIP_DAMAGED.as_str(), "SP-ZIP-001");
    assert_eq!(ZIP_UNSUPPORTED.as_str(), "SP-ZIP-002");
    assert_eq!(ZIP_WRITE.as_str(), "SP-ZIP-003");
    assert_eq!(ZIP_NO_PAYLOAD.as_str(), "SP-ZIP-004");
}

/// Writes a little-endian `u32` when the slot exists.
fn put_u32(bytes: &mut [u8], at: usize, value: u32) {
    if let Some(slot) = bytes.get_mut(at..at + 4) {
        slot.copy_from_slice(&value.to_le_bytes());
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Mutated archives never panic, and what decodes writes back.
    #[test]
    fn mutated_archives_never_panic(position in 0_usize..600, byte in any::<u8>()) {
        let mut bytes = fixture(FIXTURES[0]);
        if let Some(slot) = bytes.get_mut(position) {
            *slot = byte;
        }
        if let Ok((inner, params)) = decode_with(&bytes, &Limits::default()) {
            prop_assert_eq!(ZipLayer.encode(&inner, &params).unwrap(), bytes);
        }
    }
}
