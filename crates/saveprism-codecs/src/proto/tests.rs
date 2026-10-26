//! Tests of the schema-less Protocol Buffers codec.

use std::path::PathBuf;

use saveprism_core::detect::{DetectOptions, detect};
use saveprism_core::{
    Codec, DecodeCtx, Decoded, Edit, Limits, NodeFlags, OwnedValue, Path, Registry, SaveDoc, Value,
    apply_edits,
};

use super::*;

const PLAYER: &str = "synthetic/proto/player.pb";
const NESTED: &str = "synthetic/proto/nested.pb";
const WIDTHS: &str = "synthetic/proto/widths.pb";
const GROUPS: &str = "synthetic/proto/groups.pb";

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn fixture(relative: &str) -> Vec<u8> {
    std::fs::read(fixtures().join(relative)).unwrap()
}

fn decode(bytes: &[u8]) -> Decoded {
    let limits = Limits::default();
    let mut ctx = DecodeCtx::new(&limits);
    ProtobufCodec.decode(bytes, &mut ctx).unwrap()
}

fn exact(bytes: &[u8]) -> Decoded {
    let decoded = decode(bytes);
    assert_eq!(
        ProtobufCodec.encode(&decoded.doc, &decoded.params).unwrap(),
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
    for name in [PLAYER, NESTED, WIDTHS, GROUPS] {
        exact(&fixture(name));
    }
}

#[test]
fn the_player_tree_has_its_values() {
    let decoded = decode(&fixture(PLAYER));
    assert_eq!(at(&decoded.doc, "/0/value").value, Value::Str("Ash".into()));
    assert!(matches!(
        at(&decoded.doc, "/1/value").value,
        Value::Int { v: 7, .. }
    ));
    assert!(
        !at(&decoded.doc, "/1/value")
            .flags
            .contains(NodeFlags::READ_ONLY)
    );
}

#[test]
fn a_nested_edit_fixes_every_enclosing_length() {
    let bytes = fixture(NESTED);
    let mut decoded = exact(&bytes);
    // /0 is the outer message field, /0/value/0/value the inner varint.
    set(&mut decoded, "/0/value/0/value", OwnedValue::Int(300));
    let out = ProtobufCodec.encode(&decoded.doc, &decoded.params).unwrap();
    assert!(out.len() > bytes.len());
    let again = decode(&out);
    assert_eq!(
        at(&again.doc, "/0/value/0/value").value,
        Value::Int {
            v: 300,
            width: saveprism_core::IntWidth::U64
        }
    );
    assert_eq!(
        at(&again.doc, "/0/value/1/value").value,
        Value::Str("Ash".into())
    );
}

#[test]
fn a_string_edit_recomputes_its_length_and_the_parent_length() {
    let bytes = fixture(NESTED);
    let mut decoded = exact(&bytes);
    set(
        &mut decoded,
        "/0/value/1/value",
        OwnedValue::Str("Ashe".into()),
    );
    let out = ProtobufCodec.encode(&decoded.doc, &decoded.params).unwrap();
    let again = decode(&out);
    assert_eq!(
        at(&again.doc, "/0/value/1/value").value,
        Value::Str("Ashe".into())
    );
    assert_eq!(
        at(&again.doc, "/0/value/0/value").value,
        Value::Int {
            v: 7,
            width: saveprism_core::IntWidth::U64
        }
    );
}

#[test]
fn fixed_width_values_round_trip_and_edit() {
    let bytes = fixture(WIDTHS);
    let mut decoded = exact(&bytes);
    set(&mut decoded, "/0/value", OwnedValue::Int(0x0102_0304));
    let out = ProtobufCodec.encode(&decoded.doc, &decoded.params).unwrap();
    let again = decode(&out);
    assert_eq!(
        at(&again.doc, "/0/value").value,
        Value::Int {
            v: 0x0102_0304,
            width: saveprism_core::IntWidth::U32
        }
    );
    assert!(matches!(
        at(&decoded.doc, "/1/value").value,
        Value::Int { .. }
    ));
}

#[test]
fn groups_are_opaque_read_only_bytes() {
    let bytes = fixture(GROUPS);
    let decoded = exact(&bytes);
    let value = at(&decoded.doc, "/0/value");
    assert!(value.flags.contains(NodeFlags::READ_ONLY));
    assert!(matches!(value.value, Value::Bytes(_)));
    assert_eq!(value.type_hint.as_ref().unwrap().as_str(), "PB:group");
}

#[test]
fn a_non_minimal_tag_is_kept_when_another_field_changes() {
    let bytes = fixture(WIDTHS);
    let mut decoded = exact(&bytes);
    set(&mut decoded, "/2/value", OwnedValue::Int(9));
    let out = ProtobufCodec.encode(&decoded.doc, &decoded.params).unwrap();
    assert_eq!(
        out.get(..5),
        Some([0x0D, 0x04, 0x03, 0x02, 0x01].as_slice()),
        "the first field changed"
    );
    assert!(
        out.windows(2).any(|window| window == [0x90, 0x00]),
        "the non-minimal tag of the second field was rewritten"
    );
}

#[test]
fn invalid_wire_data_is_refused() {
    for bytes in [
        &[0x08][..],
        &[0x00, 0x01][..],
        &[0x0A, 0x05, 0x01][..],
        &[0x0E][..],
        &[0x0C][..],
    ] {
        let limits = Limits::default();
        let mut ctx = DecodeCtx::new(&limits);
        assert!(ProtobufCodec.decode(bytes, &mut ctx).is_err(), "{bytes:?}");
    }
}

#[test]
fn sniff_is_a_weak_hint_and_rejects_text() {
    let ctx = SniffCtx::default();
    for name in [PLAYER, NESTED, WIDTHS, GROUPS] {
        let bytes = fixture(name);
        let confidence = ProtobufCodec.sniff(&bytes, &ctx);
        assert!(confidence > Confidence::NONE, "{name}");
        assert!(confidence < Confidence::LOW, "{name}");
    }
    assert_eq!(ProtobufCodec.sniff(b"{\"a\": 1}", &ctx), Confidence::NONE);
    assert_eq!(ProtobufCodec.sniff(b"", &ctx), Confidence::NONE);
}

#[test]
fn detection_picks_the_codec() {
    let mut reg = Registry::default();
    crate::register_all(&mut reg);
    for name in [PLAYER, NESTED, WIDTHS, GROUPS] {
        let bytes = fixture(name);
        let opts = DetectOptions {
            file_name: Some("save.pb".to_string()),
            ..DetectOptions::default()
        };
        let found = detect(&bytes, &reg, &Limits::default(), &opts).unwrap();
        assert_eq!(found.best.recipe.kinds(), vec!["format.protobuf"], "{name}");
    }
}
