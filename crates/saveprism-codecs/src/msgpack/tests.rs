//! Tests of the MessagePack codec.

use std::path::PathBuf;

use saveprism_core::detect::{DetectOptions, detect};
use saveprism_core::{
    Codec, DecodeCtx, Decoded, Edit, Limits, NodeFlags, OwnedValue, Path, Registry, SaveDoc, Value,
    apply_edits,
};

use super::*;

const PLAYER: &str = "synthetic/msgpack/player.msgpack";
const WIDTHS: &str = "synthetic/msgpack/widths.msgpack";
const EMPTY: &str = "synthetic/msgpack/empty-array.msgpack";

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn fixture(relative: &str) -> Vec<u8> {
    std::fs::read(fixtures().join(relative)).unwrap()
}

fn decode(bytes: &[u8]) -> Decoded {
    let limits = Limits::default();
    let mut ctx = DecodeCtx::new(&limits);
    MessagePackCodec.decode(bytes, &mut ctx).unwrap()
}

fn exact(bytes: &[u8]) -> Decoded {
    let decoded = decode(bytes);
    assert_eq!(
        MessagePackCodec
            .encode(&decoded.doc, &decoded.params)
            .unwrap(),
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
    for name in [PLAYER, WIDTHS, EMPTY] {
        exact(&fixture(name));
    }
}

#[test]
fn the_tree_has_the_expected_values() {
    let decoded = decode(&fixture(PLAYER));
    let doc = &decoded.doc;
    assert_eq!(at(doc, "/0").value, Value::Str("Ash".into()));
    assert_eq!(
        at(doc, "/1").value,
        Value::Int {
            v: 7,
            width: saveprism_core::IntWidth::U8
        }
    );
    assert!(!at(doc, "/1").flags.contains(NodeFlags::READ_ONLY));
    assert!(
        at(doc, "/4").flags.contains(NodeFlags::READ_ONLY),
        "nil is read-only"
    );
}

#[test]
fn editing_a_scalar_changes_only_that_span() {
    let bytes = fixture(PLAYER);
    let mut decoded = exact(&bytes);
    set(&mut decoded, "/1", OwnedValue::Int(99));
    let out = MessagePackCodec
        .encode(&decoded.doc, &decoded.params)
        .unwrap();
    assert_ne!(out, bytes);
    let again = decode(&out);
    assert_eq!(
        at(&again.doc, "/1").value,
        Value::Int {
            v: 99,
            width: saveprism_core::IntWidth::U8
        }
    );
    assert_eq!(at(&again.doc, "/0").value, Value::Str("Ash".into()));
}

#[test]
fn a_wide_header_stays_wide_when_another_value_is_edited() {
    let bytes = fixture(WIDTHS);
    let mut decoded = exact(&bytes);
    set(&mut decoded, "/0", OwnedValue::Int(2));
    let out = MessagePackCodec
        .encode(&decoded.doc, &decoded.params)
        .unwrap();
    assert!(
        out.windows(5)
            .any(|window| window == [0xce, 0x00, 0x00, 0x00, 0x01]),
        "the untouched uint32 header was rewritten"
    );
}

#[test]
fn an_extension_is_read_only() {
    let bytes = fixture(WIDTHS);
    let decoded = exact(&bytes);
    let id = decoded.doc.resolve(&Path::parse("/4").unwrap()).unwrap();
    let node = decoded.doc.get(id).unwrap();
    assert!(
        node.type_hint
            .as_ref()
            .unwrap()
            .as_str()
            .starts_with("MP:ext:")
    );
    assert!(node.flags.contains(NodeFlags::READ_ONLY));
}

#[test]
fn sniff_accepts_our_fixtures_and_rejects_text() {
    for name in [PLAYER, WIDTHS, EMPTY] {
        let bytes = fixture(name);
        assert!(
            MessagePackCodec.sniff(&bytes, &SniffCtx::default()).value() > 0.0,
            "{name}"
        );
    }
    assert_eq!(
        MessagePackCodec.sniff(b"{\"a\":1}", &SniffCtx::default()),
        Confidence::NONE
    );
}

#[test]
fn detection_picks_the_codec() {
    let mut reg = Registry::default();
    crate::register_all(&mut reg);
    for name in [PLAYER, WIDTHS, EMPTY] {
        let bytes = fixture(name);
        let opts = DetectOptions {
            file_name: Some("save.msgpack".to_string()),
            ..DetectOptions::default()
        };
        let found = detect(&bytes, &reg, &Limits::default(), &opts).unwrap();
        assert_eq!(found.best.recipe.kinds(), vec!["format.msgpack"], "{name}");
    }
}
