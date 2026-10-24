//! Tests of the BSON codec.

use std::path::PathBuf;

use saveprism_core::detect::{DetectOptions, detect};
use saveprism_core::{
    Codec, DecodeCtx, Decoded, Edit, Limits, NodeFlags, OwnedValue, Path, Registry, SaveDoc, Value,
    apply_edits,
};

use super::*;

const PLAYER: &str = "synthetic/bson/player.bson";
const EMPTY: &str = "synthetic/bson/empty.bson";

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn fixture(relative: &str) -> Vec<u8> {
    std::fs::read(fixtures().join(relative)).unwrap()
}

fn decode(bytes: &[u8]) -> Decoded {
    let limits = Limits::default();
    let mut ctx = DecodeCtx::new(&limits);
    BsonCodec.decode(bytes, &mut ctx).unwrap()
}

fn exact(bytes: &[u8]) -> Decoded {
    let decoded = decode(bytes);
    assert_eq!(
        BsonCodec.encode(&decoded.doc, &decoded.params).unwrap(),
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
    let id = doc.resolve(&Path::parse(path).unwrap()).unwrap();
    doc.get(id).unwrap()
}

#[test]
fn every_fixture_round_trips_exactly() {
    for name in [PLAYER, EMPTY] {
        exact(&fixture(name));
    }
}

#[test]
fn the_tree_has_the_expected_values() {
    let decoded = decode(&fixture(PLAYER));
    let doc = &decoded.doc;
    assert_eq!(at(doc, "/name").value, Value::Str("Rin".into()));
    assert_eq!(
        at(doc, "/health").value,
        Value::Float {
            v: 87.5,
            width: saveprism_core::FloatWidth::F64
        }
    );
    assert_eq!(
        at(doc, "/level").value,
        Value::Int {
            v: 21,
            width: saveprism_core::IntWidth::I32
        }
    );
    assert!(at(doc, "/playerId").flags.contains(NodeFlags::READ_ONLY));
    assert!(!at(doc, "/name").flags.contains(NodeFlags::READ_ONLY));
}

#[test]
fn editing_values_recomputes_lengths() {
    let bytes = fixture(PLAYER);
    let mut decoded = exact(&bytes);
    set(
        &mut decoded,
        "/name",
        OwnedValue::Str("Rin the Long".into()),
    );
    set(&mut decoded, "/level", OwnedValue::Int(22));
    set(&mut decoded, "/alive", OwnedValue::Bool(false));
    let out = BsonCodec.encode(&decoded.doc, &decoded.params).unwrap();
    assert_ne!(out, bytes);
    let again = decode(&out);
    assert_eq!(
        at(&again.doc, "/name").value,
        Value::Str("Rin the Long".into())
    );
    assert_eq!(
        at(&again.doc, "/level").value,
        Value::Int {
            v: 22,
            width: saveprism_core::IntWidth::I32
        }
    );
    assert_eq!(at(&again.doc, "/alive").value, Value::Bool(false));
    let stored = again.doc.get(again.doc.root()).unwrap();
    let Value::Object(entries) = &stored.value else {
        panic!("root is not a document");
    };
    assert_eq!(
        i32::from_le_bytes(out.get(..4).and_then(|b| b.try_into().ok()).unwrap()),
        i32::try_from(out.len()).unwrap()
    );
    assert_eq!(entries.len(), 14);
}

#[test]
fn detection_picks_the_codec() {
    let mut reg = Registry::default();
    crate::register_all(&mut reg);
    for name in [PLAYER, EMPTY] {
        let bytes = fixture(name);
        let opts = DetectOptions {
            file_name: Some("save.bson".to_string()),
            ..DetectOptions::default()
        };
        let found = detect(&bytes, &reg, &Limits::default(), &opts).unwrap();
        assert_eq!(found.best.recipe.kinds(), vec!["format.bson"], "{name}");
    }
}
