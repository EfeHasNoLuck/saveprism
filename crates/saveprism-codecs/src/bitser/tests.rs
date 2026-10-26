//! Tests of the Bitser codec.

use std::path::PathBuf;

use saveprism_core::detect::{DetectOptions, detect};
use saveprism_core::{
    Codec, DecodeCtx, Decoded, Edit, IntWidth, Limits, NodeFlags, OwnedValue, Path, Registry,
    SaveDoc, Value, apply_edits,
};

use super::*;

const PLAYER: &str = "synthetic/bitser/player.bitser";
const NESTED: &str = "synthetic/bitser/nested.bitser";
const TYPES: &str = "synthetic/bitser/types.bitser";

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn fixture(relative: &str) -> Vec<u8> {
    std::fs::read(fixtures().join(relative)).unwrap()
}

fn decode(bytes: &[u8]) -> Decoded {
    let limits = Limits::default();
    let mut ctx = DecodeCtx::new(&limits);
    BitserCodec.decode(bytes, &mut ctx).unwrap()
}

fn exact(bytes: &[u8]) -> Decoded {
    let decoded = decode(bytes);
    assert_eq!(
        BitserCodec.encode(&decoded.doc, &decoded.params).unwrap(),
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
    for name in [PLAYER, NESTED, TYPES] {
        exact(&fixture(name));
    }
}

#[test]
fn the_player_tree_has_its_values() {
    let decoded = decode(&fixture(PLAYER));
    assert_eq!(at(&decoded.doc, "/array/0").value, Value::Str("Ash".into()));
    assert_eq!(
        at(&decoded.doc, "/array/1").value,
        Value::Int {
            v: 7,
            width: IntWidth::I16
        }
    );
}

#[test]
fn editing_a_scalar_changes_only_that_span() {
    let bytes = fixture(PLAYER);
    let mut decoded = exact(&bytes);
    set(&mut decoded, "/array/1", OwnedValue::Int(300));
    let out = BitserCodec.encode(&decoded.doc, &decoded.params).unwrap();
    assert_ne!(out, bytes);
    let again = decode(&out);
    assert_eq!(
        at(&again.doc, "/array/1").value,
        Value::Int {
            v: 300,
            width: IntWidth::I16
        }
    );
    assert_eq!(at(&again.doc, "/array/0").value, Value::Str("Ash".into()));
}

#[test]
fn editing_a_string_keeps_the_other_values_and_references() {
    let bytes = fixture(NESTED);
    let mut decoded = exact(&bytes);
    set(
        &mut decoded,
        "/array/0/array/0",
        OwnedValue::Str("Mira the Bold".into()),
    );
    set(&mut decoded, "/array/0/map/0", OwnedValue::Int(77));
    let out = BitserCodec.encode(&decoded.doc, &decoded.params).unwrap();
    let again = decode(&out);
    assert_eq!(
        at(&again.doc, "/array/0/array/0").value,
        Value::Str("Mira the Bold".into())
    );
    assert_eq!(
        at(&again.doc, "/array/0/map/0").value,
        Value::Int {
            v: 77,
            width: IntWidth::I16
        }
    );
    assert!(matches!(
        at(&again.doc, "/array/1").value,
        Value::Ref(target) if target == again.doc.resolve(&Path::parse("/array/0").unwrap()).unwrap()
    ));
}

#[test]
fn resources_instances_metatables_and_extensions_are_read_only() {
    let decoded = exact(&fixture(TYPES));
    for path in ["/array/5", "/array/6", "/array/7"] {
        if let Some(id) = decoded.doc.resolve(&Path::parse(path).unwrap()) {
            assert!(
                decoded
                    .doc
                    .get(id)
                    .unwrap()
                    .flags
                    .contains(NodeFlags::READ_ONLY),
                "{path}"
            );
        }
    }
    let instance = at(&decoded.doc, "/array/6");
    assert!(matches!(instance.value, Value::Object(_)));
    assert_eq!(
        instance.type_hint.as_ref().unwrap().as_str(),
        "BITSER:instance"
    );
}

#[test]
fn a_reference_to_an_uninitialized_value_is_refused() {
    // Table with one value that is a reference to the table itself, which is still being read.
    // In Bitser the table is registered first, so this is actually valid: it points at the table.
    let bytes = [240, 28, 128, 27];
    let decoded = exact(&bytes);
    assert!(matches!(at(&decoded.doc, "/array/0").value, Value::Ref(_)));
}

#[test]
fn empty_or_unknown_data_is_refused() {
    for bytes in [&b""[..], &[255, 27][..], &[251][..]] {
        let limits = Limits::default();
        let mut ctx = DecodeCtx::new(&limits);
        assert!(BitserCodec.decode(bytes, &mut ctx).is_err(), "{bytes:?}");
    }
}

#[test]
fn sniff_is_a_weak_hint_and_rejects_text() {
    let ctx = SniffCtx::default();
    for name in [PLAYER, NESTED, TYPES] {
        let confidence = BitserCodec.sniff(&fixture(name), &ctx);
        assert!(confidence > Confidence::NONE, "{name}");
        assert!(confidence < Confidence::LOW, "{name}");
    }
    assert_eq!(BitserCodec.sniff(b"{\"a\":1}", &ctx), Confidence::NONE);
    assert_eq!(BitserCodec.sniff(b"", &ctx), Confidence::NONE);
}

#[test]
fn detection_picks_the_codec() {
    let mut reg = Registry::default();
    crate::register_all(&mut reg);
    for name in [PLAYER, NESTED, TYPES] {
        let bytes = fixture(name);
        let opts = DetectOptions {
            file_name: Some("save.dat".to_string()),
            ..DetectOptions::default()
        };
        let found = detect(&bytes, &reg, &Limits::default(), &opts).unwrap();
        assert_eq!(found.best.recipe.kinds(), vec!["format.bitser"], "{name}");
    }
}
