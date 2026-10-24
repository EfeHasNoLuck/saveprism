//! Tests of the CBOR codec.

use std::path::PathBuf;

use saveprism_core::detect::{DetectOptions, detect};
use saveprism_core::{
    Codec, DecodeCtx, Decoded, Edit, IntWidth, Limits, OwnedValue, Path, Registry, SaveDoc, Value,
    apply_edits,
};

use super::*;

const PLAYER: &str = "synthetic/cbor/player.cbor";
const INDEFINITE: &str = "synthetic/cbor/indefinite.cbor";
const SEQUENCE: &str = "synthetic/cbor/sequence.cbor";

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn fixture(relative: &str) -> Vec<u8> {
    std::fs::read(fixtures().join(relative)).unwrap()
}

fn decode(bytes: &[u8]) -> Decoded {
    let limits = Limits::default();
    let mut ctx = DecodeCtx::new(&limits);
    CborCodec.decode(bytes, &mut ctx).unwrap()
}

fn exact(bytes: &[u8]) -> Decoded {
    let decoded = decode(bytes);
    assert_eq!(
        CborCodec.encode(&decoded.doc, &decoded.params).unwrap(),
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
    for name in [PLAYER, INDEFINITE, SEQUENCE] {
        exact(&fixture(name));
    }
}

#[test]
fn the_tree_has_the_expected_values() {
    let decoded = decode(&fixture(PLAYER));
    let doc = &decoded.doc;
    assert_eq!(at(doc, "/0").value, Value::Str("Mira".into()));
    assert_eq!(
        at(doc, "/1").value,
        Value::Int {
            v: 12,
            width: IntWidth::U8
        }
    );
    assert_eq!(
        at(doc, "/11").type_hint.as_ref().unwrap().as_str(),
        "CBOR:tag:1"
    );
}

#[test]
fn editing_a_scalar_rewrites_only_that_span() {
    let bytes = fixture(PLAYER);
    let mut decoded = exact(&bytes);
    set(&mut decoded, "/1", OwnedValue::Int(20));
    let out = CborCodec.encode(&decoded.doc, &decoded.params).unwrap();
    assert_ne!(out, bytes);
    let again = decode(&out);
    assert_eq!(
        at(&again.doc, "/1").value,
        Value::Int {
            v: 20,
            width: IntWidth::U8
        }
    );
}

#[test]
fn indefinite_items_keep_their_form_after_another_edit() {
    let bytes = fixture(INDEFINITE);
    let mut decoded = exact(&bytes);
    set(&mut decoded, "/1/0", OwnedValue::Int(9));
    let out = CborCodec.encode(&decoded.doc, &decoded.params).unwrap();
    assert!(
        out.starts_with(&[0xbf, 0x65, b't', b'i', b't', b'l', b'e', 0x7f]),
        "the indefinite title string was rewritten"
    );
    let again = decode(&out);
    assert_eq!(
        at(&again.doc, "/1/0").value,
        Value::Int {
            v: 9,
            width: IntWidth::U8
        }
    );
}

#[test]
fn a_float32_keeps_its_width() {
    let decoded = decode(&fixture(PLAYER));
    let node = at(&decoded.doc, "/4");
    assert_eq!(
        node.value,
        Value::Float {
            v: -2.5,
            width: saveprism_core::FloatWidth::F32
        }
    );
}

#[test]
fn detection_picks_the_codec() {
    let mut reg = Registry::default();
    crate::register_all(&mut reg);
    for name in [PLAYER, INDEFINITE, SEQUENCE] {
        let bytes = fixture(name);
        let opts = DetectOptions {
            file_name: Some("save.cbor".to_string()),
            ..DetectOptions::default()
        };
        let found = detect(&bytes, &reg, &Limits::default(), &opts).unwrap();
        assert_eq!(found.best.recipe.kinds(), vec!["format.cbor"], "{name}");
    }
}
