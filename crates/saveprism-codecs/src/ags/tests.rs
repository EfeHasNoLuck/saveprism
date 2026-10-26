use std::path::PathBuf;

use saveprism_core::{
    Codec, DecodeCtx, Edit, Limits, NodeFlags, OwnedValue, Path, SniffCtx, Value, apply_edits,
};

use super::*;

fn fixture(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/synthetic/ags")
        .join(name);
    std::fs::read(path).expect("read the AGS fixture")
}

fn decode(bytes: &[u8]) -> Decoded {
    let limits = Limits::default();
    let mut ctx = DecodeCtx::new(&limits);
    AgsCodec
        .decode(bytes, &mut ctx)
        .expect("a valid synthetic AGS save")
}

fn exact(bytes: &[u8]) -> Decoded {
    let decoded = decode(bytes);
    assert_eq!(
        AgsCodec.encode(&decoded.doc, &decoded.params).unwrap(),
        bytes,
        "an unchanged AGS save must write back byte for byte"
    );
    decoded
}

#[test]
fn sniff_needs_the_ags_signature() {
    let bytes = fixture("agssave.000");
    assert!(AgsCodec.sniff(&bytes, &SniffCtx::default()).value() > 0.98);
    assert_eq!(
        AgsCodec.sniff(b"{\"a\":1}", &SniffCtx::default()).value(),
        0.0
    );
    assert_eq!(
        AgsCodec
            .sniff(b"Adventure Game Studio saved game", &SniffCtx::default())
            .value(),
        0.9
    );
}

#[test]
fn legacy_saves_are_refused_with_a_clear_error() {
    let limits = Limits::default();
    let mut ctx = DecodeCtx::new(&limits);
    let error = AgsCodec
        .decode(b"Adventure Game Studio saved game", &mut ctx)
        .expect_err("the legacy signature is not supported");
    assert_eq!(error.code(), AGS_UNSUPPORTED);
}

#[test]
fn current_save_round_trips_exactly() {
    exact(&fixture("agssave.000"));
}

#[test]
fn pre_363_save_round_trips_exactly() {
    let decoded = exact(&fixture("agssave.999"));
    let root = decoded.doc.root();
    let version = decoded
        .doc
        .resolve(&Path::parse("/version").unwrap())
        .and_then(|id| decoded.doc.get(id))
        .map(|node| node.value.clone());
    assert!(matches!(version, Some(Value::Int { v: 13, .. })));
    assert!(
        decoded
            .doc
            .resolve(&Path::parse("/format").unwrap())
            .is_none(),
        "a 3.5.1 save has no file-format table"
    );
    assert!(decoded.doc.get(root).is_some());
}

#[test]
fn description_is_editable_and_refits_the_offsets() {
    let bytes = fixture("agssave.000");
    let mut decoded = exact(&bytes);
    let path = Path::parse("/description/user_text").unwrap();
    apply_edits(
        &mut decoded.doc,
        &[Edit::Set {
            path,
            value: OwnedValue::Str("A much longer invented chapter title".to_owned()),
        }],
        &Limits::default(),
    )
    .unwrap();
    let written = AgsCodec.encode(&decoded.doc, &decoded.params).unwrap();
    assert_ne!(written, bytes);
    let again = decode(&written);
    let value = again
        .doc
        .resolve(&Path::parse("/description/user_text").unwrap())
        .and_then(|id| again.doc.get(id))
        .map(|node| node.value.clone());
    assert_eq!(
        value,
        Some(Value::Str(
            "A much longer invented chapter title".to_owned()
        ))
    );
    assert_eq!(
        AgsCodec.encode(&again.doc, &again.params).unwrap(),
        written,
        "the edited save must be stable"
    );
}

#[test]
fn engine_fields_are_editable_but_script_globals_are_not() {
    let mut decoded = exact(&fixture("agssave.000"));
    let limits = Limits::default();
    apply_edits(
        &mut decoded.doc,
        &[Edit::Set {
            path: Path::parse("/environment/game_title").unwrap(),
            value: OwnedValue::Str("Renamed Game".to_owned()),
        }],
        &limits,
    )
    .unwrap();
    let error = apply_edits(
        &mut decoded.doc,
        &[Edit::Set {
            path: Path::parse("/script_globals/2/value").unwrap(),
            value: OwnedValue::Int(1),
        }],
        &limits,
    )
    .unwrap_err();
    assert_eq!(
        error.code(),
        saveprism_core::codes::CORE_READ_ONLY,
        "a script global is read-only"
    );
    let written = AgsCodec.encode(&decoded.doc, &decoded.params).unwrap();
    let again = decode(&written);
    let title = again
        .doc
        .resolve(&Path::parse("/environment/game_title").unwrap())
        .and_then(|id| again.doc.get(id))
        .map(|node| node.value.clone());
    assert_eq!(title, Some(Value::Str("Renamed Game".to_owned())));
}

#[test]
fn game_data_and_components_stay_read_only() {
    let decoded = exact(&fixture("agssave.000"));
    let game = decoded
        .doc
        .resolve(&Path::parse("/game_data").unwrap())
        .and_then(|id| decoded.doc.get(id))
        .expect("game data");
    assert!(game.flags.contains(NodeFlags::READ_ONLY));
    let components = decoded
        .doc
        .resolve(&Path::parse("/components").unwrap())
        .and_then(|id| decoded.doc.get(id))
        .expect("components");
    assert!(components.flags.contains(NodeFlags::READ_ONLY));
}
