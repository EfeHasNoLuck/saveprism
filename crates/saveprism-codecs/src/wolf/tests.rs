//! Tests for the WOLF RPG Editor codec.

use std::path::PathBuf;

use saveprism_core::{
    Codec, DecodeCtx, Edit, IntWidth, Limits, OwnedValue, Path, SaveDoc, SniffCtx, Value,
};

use super::{WOLF_ENCRYPTED, WOLF_PARSE, WolfRpgCodec};

fn fixture(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/synthetic/wolf")
        .join(name);
    std::fs::read(path).expect("synthetic WOLF fixture is missing")
}

fn decode(bytes: &[u8]) -> (SaveDoc, saveprism_core::Params) {
    let limits = Limits::default();
    let mut ctx = DecodeCtx::new(&limits);
    let decoded = WolfRpgCodec.decode(bytes, &mut ctx).expect("decode failed");
    (decoded.doc, decoded.params)
}

fn resolve(doc: &SaveDoc, path: &str) -> saveprism_core::NodeId {
    doc.resolve(&Path::parse(path).unwrap())
        .unwrap_or_else(|| panic!("missing path {path}"))
}

#[test]
fn every_fixture_round_trips_byte_for_byte() {
    for name in ["SaveData01.sav", "SaveData02.sav"] {
        let bytes = fixture(name);
        let (doc, params) = decode(&bytes);
        let encoded = WolfRpgCodec.encode(&doc, &params).expect("encode failed");
        assert_eq!(encoded, bytes, "{name} did not round-trip");
    }
}

#[test]
fn variables_switches_and_variable_db_are_exposed() {
    let bytes = fixture("SaveData01.sav");
    let (doc, _) = decode(&bytes);
    assert_eq!(
        doc.get(resolve(&doc, "/Switches/0")).unwrap().value,
        Value::Int {
            v: 1,
            width: IntWidth::U32
        }
    );
    assert_eq!(
        doc.get(resolve(&doc, "/Variables/1")).unwrap().value,
        Value::Int {
            v: -5,
            width: IntWidth::I32
        }
    );
    assert_eq!(
        doc.get(resolve(&doc, "/Variable DB/Type 1/Record 0/Field 1"))
            .unwrap()
            .value,
        Value::Str("Potion".into())
    );
}

#[test]
fn editing_a_variable_and_a_string_variable_writes_them_back() {
    let bytes = fixture("SaveData01.sav");
    let (_, params) = decode(&bytes);
    let limits = Limits::default();
    let edits = [
        Edit::Set {
            path: Path::parse("/Variables/3").unwrap(),
            value: OwnedValue::Int(999),
        },
        Edit::Set {
            path: Path::parse("/Variable DB/Type 1/Record 0/Field 1").unwrap(),
            value: OwnedValue::Str("Super Potion".into()),
        },
    ];
    let patched = WolfRpgCodec
        .patch(&bytes, &edits, &params, &limits)
        .expect("patch failed");
    let (doc, _) = decode(&patched);
    assert_eq!(
        doc.get(resolve(&doc, "/Variables/3")).unwrap().value,
        Value::Int {
            v: 999,
            width: IntWidth::I32
        }
    );
    assert_eq!(
        doc.get(resolve(&doc, "/Variable DB/Type 1/Record 0/Field 1"))
            .unwrap()
            .value,
        Value::Str("Super Potion".into())
    );
}

#[test]
fn encrypted_saves_report_a_clear_error() {
    let mut bytes = vec![0_u8; 0x40];
    bytes[1] = b'P';
    bytes[0x14] = 0x00;
    let limits = Limits::default();
    let mut ctx = DecodeCtx::new(&limits);
    let error = WolfRpgCodec.decode(&bytes, &mut ctx).unwrap_err();
    assert_eq!(error.code(), WOLF_ENCRYPTED);
    assert!(error.message().contains("encrypted Wolf RPG save"));
}

#[test]
fn truncated_input_fails_without_panicking() {
    let bytes = fixture("SaveData01.sav");
    for length in [0, 3, 0x14, 0x15, 0x20, 40, 200, bytes.len() - 1] {
        let limits = Limits::default();
        let mut ctx = DecodeCtx::new(&limits);
        let error = WolfRpgCodec.decode(&bytes[..length], &mut ctx).unwrap_err();
        assert_eq!(error.code(), WOLF_PARSE, "length {length}");
    }
}

#[test]
fn sniff_accepts_wolf_names_and_rejects_other_sav_magics() {
    let bytes = fixture("SaveData01.sav");
    let ctx = SniffCtx {
        file_name: Some("SaveData01.sav"),
        depth: 0,
    };
    assert!(WolfRpgCodec.sniff(&bytes, &ctx) >= saveprism_core::Confidence::HIGH);
    let gvas =
        b"GVAS\x02\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00";
    assert_eq!(
        WolfRpgCodec.sniff(gvas, &ctx),
        saveprism_core::Confidence::NONE
    );
}

use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn random_body_never_panics(body in proptest::collection::vec(any::<u8>(), 0..512)) {
        let mut input = vec![0_u8; 0x14];
        input[6] = 0x55;
        input.push(0x19);
        input.extend_from_slice(&4_u16.to_le_bytes());
        input.extend_from_slice(b"test");
        input.extend_from_slice(&0x8E_u16.to_le_bytes());
        input.extend_from_slice(&body);
        let limits = Limits::default();
        let mut ctx = DecodeCtx::new(&limits);
        let _ = WolfRpgCodec.decode(&input, &mut ctx);
    }
}
