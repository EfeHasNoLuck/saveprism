//! Tests of the Odin Serializer binary codec.

use std::path::PathBuf;

use saveprism_core::detect::{DetectOptions, detect};
use saveprism_core::guard::{Verdict, check_round_trip};
use saveprism_core::{
    Codec, DecodeCtx, Decoded, Edit, FloatWidth, IntWidth, Limits, NodeFlags, OwnedValue, Path,
    Registry, SaveDoc, Value, apply_edits, pipeline,
};

use super::*;

const PLAYER: &str = "synthetic/odin/player.odin";
const TYPES: &str = "synthetic/odin/types.odin";
const ODIN_JSON: &str = "synthetic/odin/odin-json.json";

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn fixture(relative: &str) -> Vec<u8> {
    std::fs::read(fixtures().join(relative)).unwrap()
}

fn decode(bytes: &[u8]) -> Decoded {
    let limits = Limits::default();
    let mut ctx = DecodeCtx::new(&limits);
    OdinCodec.decode(bytes, &mut ctx).unwrap()
}

fn exact(bytes: &[u8]) -> Decoded {
    let decoded = decode(bytes);
    assert_eq!(
        OdinCodec.encode(&decoded.doc, &decoded.params).unwrap(),
        bytes,
        "re-encoded bytes differ"
    );
    decoded
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
    let id = doc
        .resolve(&Path::parse(path).unwrap())
        .unwrap_or_else(|| panic!("path {path} is missing"));
    doc.get(id)
        .unwrap_or_else(|| panic!("path {path} has a stale id"))
}

#[test]
fn every_fixture_round_trips_exactly() {
    for name in [PLAYER, TYPES] {
        exact(&fixture(name));
    }
}

#[test]
fn the_player_tree_has_the_expected_values() {
    let decoded = decode(&fixture(PLAYER));
    let doc = &decoded.doc;
    assert_eq!(
        at(doc, "/player/gold").value,
        Value::Int {
            v: 120,
            width: IntWidth::I32
        }
    );
    assert_eq!(at(doc, "/player/name").value, Value::Str("Rin".into()));
    assert_eq!(at(doc, "/player/title").value, Value::Str("Ranger".into()));
    assert_eq!(
        at(doc, "/player/hp").value,
        Value::Float {
            v: 87.5,
            width: FloatWidth::F32
        }
    );
    assert_eq!(
        at(doc, "/player/xp").value,
        Value::Float {
            v: 1543.25,
            width: FloatWidth::F64
        }
    );
    assert_eq!(at(doc, "/player/alive").value, Value::Bool(true));
    assert_eq!(
        at(doc, "/player/morale").value,
        Value::Int {
            v: -3,
            width: IntWidth::I8
        }
    );
    assert_eq!(
        at(doc, "/player/level").value,
        Value::Int {
            v: 21,
            width: IntWidth::U16
        }
    );
    assert_eq!(
        at(doc, "/player/playtime").value,
        Value::Int {
            v: 30_000_000_000,
            width: IntWidth::I64
        }
    );
    assert_eq!(
        at(doc, "/player/initial").value,
        Value::Int {
            v: i128::from(b'R'),
            width: IntWidth::U16
        }
    );
    assert_eq!(at(doc, "/player/pet").value, Value::Null);
    assert_eq!(
        at(doc, "/player/inventory/#0/0").value,
        Value::Str("sword".into())
    );
    assert_eq!(
        at(doc, "/player/#14/count").value,
        Value::Int {
            v: 2,
            width: IntWidth::I32
        }
    );
    assert_eq!(
        at(doc, "/player/$type").value,
        Value::Str("Game.SaveData, Assembly-CSharp".into())
    );
    assert_eq!(
        at(doc, "/player/inventory/$type").value,
        Value::Str("Game.SaveData, Assembly-CSharp".into())
    );
    assert!(
        at(doc, "/player/$type")
            .flags
            .contains(NodeFlags::READ_ONLY)
    );
    assert!(!at(doc, "/player/gold").flags.contains(NodeFlags::READ_ONLY));
}

#[test]
fn editing_primitives_keeps_their_widths() {
    let bytes = fixture(PLAYER);
    let mut decoded = exact(&bytes);
    set(&mut decoded, "/player/gold", OwnedValue::Int(9999));
    set(&mut decoded, "/player/morale", OwnedValue::Int(-8));
    set(&mut decoded, "/player/level", OwnedValue::Int(65_000));
    set(&mut decoded, "/player/alive", OwnedValue::Bool(false));
    set(&mut decoded, "/player/hp", OwnedValue::Float(12.5));
    set(&mut decoded, "/player/initial", OwnedValue::Int(65));
    let out = OdinCodec.encode(&decoded.doc, &decoded.params).unwrap();
    assert_ne!(out, bytes);
    let again = decode(&out);
    assert_eq!(
        at(&again.doc, "/player/gold").value,
        Value::Int {
            v: 9999,
            width: IntWidth::I32
        }
    );
    assert_eq!(
        at(&again.doc, "/player/level").value,
        Value::Int {
            v: 65_000,
            width: IntWidth::U16
        }
    );
    assert_eq!(at(&again.doc, "/player/alive").value, Value::Bool(false));
    assert_eq!(
        at(&again.doc, "/player/hp").value,
        Value::Float {
            v: 12.5,
            width: FloatWidth::F32
        }
    );
    assert_eq!(
        at(&again.doc, "/player/initial").value,
        Value::Int {
            v: 65,
            width: IntWidth::U16
        }
    );
}

#[test]
fn editing_strings_recomputes_their_payload() {
    let bytes = fixture(PLAYER);
    let mut decoded = exact(&bytes);
    set(
        &mut decoded,
        "/player/name",
        OwnedValue::Str("Rin the Long".into()),
    );
    set(
        &mut decoded,
        "/player/title",
        OwnedValue::Str("Scout".into()),
    );
    set(
        &mut decoded,
        "/player/inventory/#0/2",
        OwnedValue::Str("elixir".into()),
    );
    let out = OdinCodec.encode(&decoded.doc, &decoded.params).unwrap();
    assert_ne!(out, bytes);
    let again = decode(&out);
    assert_eq!(
        at(&again.doc, "/player/name").value,
        Value::Str("Rin the Long".into())
    );
    assert_eq!(
        at(&again.doc, "/player/title").value,
        Value::Str("Scout".into())
    );
    assert_eq!(
        at(&again.doc, "/player/inventory/#0/2").value,
        Value::Str("elixir".into())
    );
}

#[test]
fn a_wide_string_may_gain_non_ascii_text() {
    let bytes = fixture(PLAYER);
    let mut decoded = exact(&bytes);
    set(
        &mut decoded,
        "/player/name",
        OwnedValue::Str("Rin \u{4e16}\u{754c}".into()),
    );
    let out = OdinCodec.encode(&decoded.doc, &decoded.params).unwrap();
    assert_ne!(out, bytes);
    let again = decode(&out);
    assert_eq!(
        at(&again.doc, "/player/name").value,
        Value::Str("Rin \u{4e16}\u{754c}".into())
    );
}

#[test]
fn an_eight_bit_string_is_widened_when_needed() {
    let bytes = fixture(PLAYER);
    let mut decoded = exact(&bytes);
    set(
        &mut decoded,
        "/player/title",
        OwnedValue::Str("Ranger \u{4e16}".into()),
    );
    let out = OdinCodec.encode(&decoded.doc, &decoded.params).unwrap();
    assert_ne!(out, bytes);
    let again = decode(&out);
    assert_eq!(
        at(&again.doc, "/player/title").value,
        Value::Str("Ranger \u{4e16}".into())
    );
    // The rewritten payload now uses the 16-bit flag.
    let span = again
        .doc
        .resolve(&Path::parse("/player/title").unwrap())
        .and_then(|id| again.doc.get(id))
        .and_then(|node| node.span)
        .unwrap();
    assert_eq!(out.get(span.start as usize), Some(&1));
}

#[test]
fn a_value_at_the_width_edge_round_trips() {
    let bytes = fixture(PLAYER);
    let mut decoded = exact(&bytes);
    set(&mut decoded, "/player/morale", OwnedValue::Int(-128));
    let out = OdinCodec.encode(&decoded.doc, &decoded.params).unwrap();
    let again = decode(&out);
    assert_eq!(
        at(&again.doc, "/player/morale").value,
        Value::Int {
            v: -128,
            width: IntWidth::I8
        }
    );
}

#[test]
fn references_and_type_tables_are_read_only() {
    let decoded = decode(&fixture(TYPES));
    let doc = &decoded.doc;
    assert_eq!(
        at(doc, "/first/$id").value,
        Value::Int {
            v: 100,
            width: IntWidth::I32
        }
    );
    assert_eq!(
        at(doc, "/#1/$typeId").value,
        Value::Int {
            v: 10,
            width: IntWidth::I32
        }
    );
    assert_eq!(
        at(doc, "/#1/$type").value,
        Value::Str("Game.First, Assembly-CSharp".into())
    );
    assert_eq!(at(doc, "/#1/self").value, Value::Null);
    assert!(at(doc, "/#1/self").flags.contains(NodeFlags::READ_ONLY));
    assert_eq!(
        at(doc, "/externalIndex").value,
        Value::Int {
            v: 5,
            width: IntWidth::I32
        }
    );
    assert_eq!(
        at(doc, "/#4").value,
        Value::Int {
            v: -2,
            width: IntWidth::I32
        }
    );
    assert_eq!(
        at(doc, "/externalString").value,
        Value::Str("scene.unity".into())
    );
    assert!(
        at(doc, "/externalString")
            .flags
            .contains(NodeFlags::READ_ONLY)
    );
    assert!(at(doc, "/#7").flags.contains(NodeFlags::READ_ONLY));
    assert_eq!(at(doc, "/#9/2").value, Value::Str("three".into()));
}

#[test]
fn editing_inside_a_reused_type_node_works() {
    let bytes = fixture(TYPES);
    let mut decoded = exact(&bytes);
    set(&mut decoded, "/#1/value", OwnedValue::Int(42));
    set(&mut decoded, "/#2/value", OwnedValue::Int(43));
    let out = OdinCodec.encode(&decoded.doc, &decoded.params).unwrap();
    assert_ne!(out, bytes);
    let again = decode(&out);
    assert_eq!(
        at(&again.doc, "/#1/value").value,
        Value::Int {
            v: 42,
            width: IntWidth::I32
        }
    );
    assert_eq!(
        at(&again.doc, "/#2/value").value,
        Value::Int {
            v: 43,
            width: IntWidth::I32
        }
    );
}

#[test]
fn detection_recognises_the_fixtures() {
    let mut reg = Registry::default();
    crate::register_all(&mut reg);
    let limits = Limits::default();
    for (name, file) in [("save", PLAYER), ("save", TYPES)] {
        let bytes = fixture(file);
        let opts = DetectOptions {
            file_name: Some(name.to_string()),
            ..DetectOptions::default()
        };
        let detection = detect(&bytes, &reg, &limits, &opts).unwrap();
        assert_eq!(detection.best.recipe.kinds(), ["format.odin"], "{file}");
        let report = check_round_trip(&bytes, &detection.best.recipe, &reg, &limits).unwrap();
        assert_eq!(report.overall, Verdict::Exact, "{file}");
    }
}

#[test]
fn odin_json_stays_with_the_json_codec() {
    let mut reg = Registry::default();
    crate::register_all(&mut reg);
    let bytes = fixture(ODIN_JSON);
    let limits = Limits::default();
    let opts = DetectOptions {
        file_name: Some("odin-json.json".to_string()),
        ..DetectOptions::default()
    };
    let detection = detect(&bytes, &reg, &limits, &opts).unwrap();
    assert_eq!(detection.best.recipe.kinds(), ["format.json"]);
    let report = check_round_trip(&bytes, &detection.best.recipe, &reg, &limits).unwrap();
    assert_eq!(report.overall, Verdict::Exact);
    let decoded = pipeline::decode(&bytes, &detection.best.recipe, &reg, &limits).unwrap();
    let again = pipeline::encode(&decoded.doc, &decoded.recipe, &reg).unwrap();
    assert_eq!(again, bytes);
}

#[test]
fn a_truncated_stream_is_not_plausible() {
    let bytes = fixture(PLAYER);
    for cut in [1, 4, bytes.len() / 2, bytes.len() - 1] {
        assert!(!super::read::plausible(&bytes[..cut]), "cut at {cut}");
    }
    assert!(super::read::plausible(&bytes));
}

#[test]
fn a_random_blob_is_not_plausible() {
    let mut state: u32 = 0x9e37_79b9;
    let mut input = vec![0x01_u8];
    for _ in 0..256 {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        input.push(state.to_le_bytes()[2]);
    }
    assert!(!super::read::plausible(&input));
}
