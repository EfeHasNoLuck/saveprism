//! Tests of the emulator-save codecs.

use std::path::PathBuf;

use saveprism_core::detect::{DetectOptions, detect};
use saveprism_core::{
    Codec, Confidence, DecodeCtx, Decoded, Edit, Limits, NodeFlags, OwnedValue, Path, Registry,
    SaveDoc, SniffCtx, Value, apply_edits,
};

use super::retroarch::RetroArchStateCodec;
use super::snes9x::Snes9xSnapshotCodec;
use super::sram::EmulatorSramCodec;

const SNAPSHOT: &str = "synthetic/emu/snapshot.000";
const STATE: &str = "synthetic/emu/game.state";
const SRAM: &str = "synthetic/emu/game.srm";

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn fixture(relative: &str) -> Vec<u8> {
    std::fs::read(fixtures().join(relative)).unwrap()
}

fn set(decoded: &mut Decoded, path: &str, value: OwnedValue) {
    apply_edits(
        &mut decoded.doc,
        &[Edit::Set {
            path: Path::parse(path).unwrap(),
            value,
        }],
        &Limits::default(),
    )
    .unwrap();
}

fn at<'a>(doc: &'a SaveDoc, path: &str) -> &'a saveprism_core::Node {
    let id = doc.resolve(&Path::parse(path).unwrap()).unwrap();
    doc.get(id).unwrap()
}

fn snes_decode(bytes: &[u8]) -> Decoded {
    let limits = Limits::default();
    let mut ctx = DecodeCtx::new(&limits);
    Snes9xSnapshotCodec.decode(bytes, &mut ctx).unwrap()
}

fn snes_exact(bytes: &[u8]) -> Decoded {
    let decoded = snes_decode(bytes);
    assert_eq!(
        Snes9xSnapshotCodec
            .encode(&decoded.doc, &decoded.params)
            .unwrap(),
        bytes
    );
    decoded
}

#[test]
fn snes9x_snapshot_round_trips_and_shows_blocks() {
    let bytes = fixture(SNAPSHOT);
    let decoded = snes_exact(&bytes);
    assert_eq!(
        at(&decoded.doc, "/version").value,
        Value::Int {
            v: 6,
            width: saveprism_core::IntWidth::U32
        }
    );
    assert_eq!(
        at(&decoded.doc, "/blocks/0/name").value,
        Value::Str("NAM".into())
    );
    let sra = at(&decoded.doc, "/blocks/2/data");
    assert!(!sra.flags.contains(NodeFlags::READ_ONLY));
    assert_eq!(sra.type_hint.as_ref().unwrap().as_str(), "S9X:sram");
    assert!(
        at(&decoded.doc, "/blocks/1/data")
            .flags
            .contains(NodeFlags::READ_ONLY)
    );
}

#[test]
fn snes9x_sram_edit_keeps_every_other_byte() {
    let bytes = fixture(SNAPSHOT);
    let mut decoded = snes_exact(&bytes);
    set(
        &mut decoded,
        "/blocks/2/data",
        OwnedValue::Bytes(vec![0xAB, 0xCD]),
    );
    // Replacing the whole block changes its length; the short value above must be refused.
    assert!(
        Snes9xSnapshotCodec
            .encode(&decoded.doc, &decoded.params)
            .is_err()
    );

    let mut decoded = snes_exact(&bytes);
    let current = match &at(&decoded.doc, "/blocks/2/data").value {
        Value::Bytes(bytes) => bytes.clone(),
        other => panic!("unexpected SRA value: {other:?}"),
    };
    let mut edited = current.clone();
    if let Some(byte) = edited.first_mut() {
        *byte ^= 0xFF;
    }
    set(
        &mut decoded,
        "/blocks/2/data",
        OwnedValue::Bytes(edited.clone()),
    );
    let out = Snes9xSnapshotCodec
        .encode(&decoded.doc, &decoded.params)
        .unwrap();
    let again = snes_decode(&out);
    assert_eq!(at(&again.doc, "/blocks/2/data").value, Value::Bytes(edited));
    assert_eq!(&out[..bytes.len().min(64)], &bytes[..bytes.len().min(64)]);
}

#[test]
fn retroarch_state_round_trips_and_shows_blocks() {
    let bytes = fixture(STATE);
    let limits = Limits::default();
    let mut ctx = DecodeCtx::new(&limits);
    let decoded = RetroArchStateCodec.decode(&bytes, &mut ctx).unwrap();
    assert_eq!(
        RetroArchStateCodec
            .encode(&decoded.doc, &decoded.params)
            .unwrap(),
        bytes
    );
    assert_eq!(
        at(&decoded.doc, "/version").value,
        Value::Int {
            v: 1,
            width: saveprism_core::IntWidth::U8
        }
    );
    assert_eq!(
        at(&decoded.doc, "/blocks/0/tag").value,
        Value::Str("MEM ".into())
    );
    assert!(
        at(&decoded.doc, "/blocks/0/data")
            .flags
            .contains(NodeFlags::READ_ONLY)
    );
}

#[test]
fn an_rzip_state_opens_read_only_with_a_hint() {
    let bytes = b"#RZIPv1#\x00\x00\x00\x01".to_vec();
    let limits = Limits::default();
    let mut ctx = DecodeCtx::new(&limits);
    let decoded = RetroArchStateCodec.decode(&bytes, &mut ctx).unwrap();
    let hint = at(&decoded.doc, "/hint");
    assert!(hint.flags.contains(NodeFlags::READ_ONLY));
    assert!(
        matches!(&hint.value, Value::Str(text) if text.contains("Save State File Compression"))
    );
    assert_eq!(
        RetroArchStateCodec
            .encode(&decoded.doc, &decoded.params)
            .unwrap(),
        bytes
    );
}

#[test]
fn srm_is_raw_bytes_and_round_trips() {
    let bytes = fixture(SRAM);
    let limits = Limits::default();
    let mut ctx = DecodeCtx::new(&limits);
    let decoded = EmulatorSramCodec.decode(&bytes, &mut ctx).unwrap();
    assert_eq!(
        EmulatorSramCodec
            .encode(&decoded.doc, &decoded.params)
            .unwrap(),
        bytes
    );
    assert_eq!(at(&decoded.doc, "/data").value, Value::Bytes(bytes.clone()));
    let mut edited = bytes.clone();
    if let Some(byte) = edited.first_mut() {
        *byte = 0x5A;
    }
    let mut decoded = decoded;
    set(&mut decoded, "/data", OwnedValue::Bytes(edited.clone()));
    assert_eq!(
        EmulatorSramCodec
            .encode(&decoded.doc, &decoded.params)
            .unwrap(),
        edited
    );
}

#[test]
fn sniff_reports_the_right_magic_and_size_hint() {
    let ctx = SniffCtx::default();
    assert!(Snes9xSnapshotCodec.sniff(&fixture(SNAPSHOT), &ctx) > Confidence::HIGH);
    assert!(RetroArchStateCodec.sniff(&fixture(STATE), &ctx) > Confidence::HIGH);
    assert_eq!(Snes9xSnapshotCodec.sniff(b"text", &ctx), Confidence::NONE);
    assert_eq!(RetroArchStateCodec.sniff(b"text", &ctx), Confidence::NONE);
    let srm = fixture(SRAM);
    let named = SniffCtx {
        file_name: Some("game.srm"),
        depth: 0,
    };
    assert!(EmulatorSramCodec.sniff(&srm, &named) > Confidence::NONE);
    assert_eq!(
        EmulatorSramCodec.sniff(&srm, &SniffCtx::default()),
        Confidence::NONE
    );
}

#[test]
fn detection_picks_the_emulator_codecs() {
    let mut reg = Registry::default();
    crate::register_all(&mut reg);
    let snapshot = fixture(SNAPSHOT);
    let found = detect(
        &snapshot,
        &reg,
        &Limits::default(),
        &DetectOptions {
            file_name: Some("snapshot.000".to_string()),
            ..DetectOptions::default()
        },
    )
    .unwrap();
    assert_eq!(found.best.recipe.kinds(), vec!["format.snes9x-snapshot"]);

    let state = fixture(STATE);
    let found = detect(
        &state,
        &reg,
        &Limits::default(),
        &DetectOptions {
            file_name: Some("game.state".to_string()),
            ..DetectOptions::default()
        },
    )
    .unwrap();
    assert_eq!(found.best.recipe.kinds(), vec!["format.retroarch-state"]);

    let srm = fixture(SRAM);
    let found = detect(
        &srm,
        &reg,
        &Limits::default(),
        &DetectOptions {
            file_name: Some("game.srm".to_string()),
            ..DetectOptions::default()
        },
    )
    .unwrap();
    assert_eq!(found.best.recipe.kinds(), vec!["format.emu-sram"]);
}
