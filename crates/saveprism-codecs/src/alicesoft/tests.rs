use std::path::PathBuf;

use saveprism_core::{
    Codec, DecodeCtx, Edit, Limits, NodeFlags, OwnedValue, Path, SniffCtx, Value, apply_edits,
};

use super::*;

fn fixture(root: &str, name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/synthetic")
        .join(root)
        .join(name);
    std::fs::read(path).expect("read the synthetic fixture")
}

fn decode_bytes(bytes: &[u8], name: &str) -> Decoded {
    let limits = Limits::default();
    let mut ctx = DecodeCtx::new(&limits);
    AliceSoftCodec
        .decode(bytes, &mut ctx)
        .unwrap_or_else(|error| panic!("{name} should decode: {error:?}"))
}

fn exact(bytes: &[u8], name: &str) -> Decoded {
    let decoded = decode_bytes(bytes, name);
    assert_eq!(
        AliceSoftCodec
            .encode(&decoded.doc, &decoded.params)
            .unwrap(),
        bytes,
        "{name} must write back byte for byte"
    );
    decoded
}

fn node_value(decoded: &Decoded, path: &str) -> Value {
    decoded
        .doc
        .resolve(&Path::parse(path).expect("test path"))
        .and_then(|id| decoded.doc.get(id))
        .map(|node| node.value.clone())
        .expect("the test path exists")
}

fn set(decoded: &mut Decoded, path: &str, value: OwnedValue) {
    apply_edits(
        &mut decoded.doc,
        &[Edit::Set {
            path: Path::parse(path).expect("test path"),
            value,
        }],
        &Limits::default(),
    )
    .expect("apply edit");
}

#[test]
fn sniff_recognises_both_system_3_signatures_and_system_4() {
    let system3 = fixture("alicesoft", "system38.asd");
    let system4 = fixture("alicesoft", "system4.asd");
    let ctx = SniffCtx::default();
    assert_eq!(AliceSoftCodec.sniff(&system3, &ctx), Confidence::CERTAIN);
    assert!(AliceSoftCodec.sniff(&system4, &ctx).value() > 0.9);
    assert_eq!(
        AliceSoftCodec
            .sniff(b"System3.8 SaveData(c)ALICE-SOFT", &ctx)
            .value(),
        0.0
    );
    assert_eq!(AliceSoftCodec.sniff(b"not a save", &ctx).value(), 0.0);
    // A VX Ace save or an RPG Maker 2000/2003 .lsd must not be claimed.
    assert_eq!(
        AliceSoftCodec
            .sniff(b"RPGMV\x00\x00\x00\x00\x00\x03\x00", &ctx)
            .value(),
        0.0
    );
}

#[test]
fn system3_round_trips_and_exposes_variables() {
    let bytes = fixture("alicesoft", "system38.asd");
    let decoded = exact(&bytes, "system38.asd");
    assert_eq!(
        node_value(&decoded, "/magic"),
        Value::Str("System3.8 SaveData(c)ALICE-SOFT".to_owned())
    );
    assert_eq!(
        node_value(&decoded, "/strings/1"),
        Value::Str("放課後の教室".to_owned())
    );
    assert_eq!(
        node_value(&decoded, "/variables/0/page"),
        Value::Int {
            v: 0,
            width: saveprism_core::IntWidth::U32
        }
    );
    assert_eq!(
        node_value(&decoded, "/variables/0/values/2"),
        Value::Int {
            v: 65535,
            width: saveprism_core::IntWidth::U16
        }
    );
    assert_eq!(
        node_value(&decoded, "/variables/1/values/2"),
        Value::Int {
            v: 42,
            width: saveprism_core::IntWidth::U16
        }
    );
}

#[test]
fn system3_numbers_and_same_length_strings_are_editable() {
    let bytes = fixture("alicesoft", "system38.asd");
    let mut decoded = exact(&bytes, "system38.asd");
    set(&mut decoded, "/variables/0/values/0", OwnedValue::Int(1234));
    set(
        &mut decoded,
        "/strings/0",
        OwnedValue::Str("Kira".to_owned()),
    );
    let written = AliceSoftCodec
        .encode(&decoded.doc, &decoded.params)
        .unwrap();
    assert_ne!(written, bytes);
    let again = decode_bytes(&written, "edited system38.asd");
    assert_eq!(
        node_value(&again, "/variables/0/values/0"),
        Value::Int {
            v: 1234,
            width: saveprism_core::IntWidth::U16
        }
    );
    assert_eq!(
        node_value(&again, "/strings/0"),
        Value::Str("Kira".to_owned())
    );
    assert_eq!(
        AliceSoftCodec.encode(&again.doc, &again.params).unwrap(),
        written
    );
}

#[test]
fn system3_refuses_a_number_above_uint16_and_a_longer_string() {
    let bytes = fixture("alicesoft", "system38.asd");
    let mut decoded = exact(&bytes, "system38.asd");
    let out_of_range = apply_edits(
        &mut decoded.doc,
        &[Edit::Set {
            path: Path::parse("/variables/0/values/0").expect("test path"),
            value: OwnedValue::Int(70_000),
        }],
        &Limits::default(),
    );
    assert!(out_of_range.is_err());

    let mut decoded = exact(&bytes, "system38.asd");
    set(
        &mut decoded,
        "/strings/0",
        OwnedValue::Str("a longer name".to_owned()),
    );
    assert!(
        AliceSoftCodec
            .encode(&decoded.doc, &decoded.params)
            .is_err()
    );
}

#[test]
fn system3_stack_stays_read_only() {
    let bytes = fixture("alicesoft", "system38.asd");
    let decoded = exact(&bytes, "system38.asd");
    let id = decoded
        .doc
        .resolve(&Path::parse("/stack").unwrap())
        .expect("stack");
    let node = decoded.doc.get(id).expect("stack node");
    assert!(node.flags.contains(NodeFlags::READ_ONLY));
}

#[test]
fn system4_round_trips_and_exposes_globals() {
    let bytes = fixture("alicesoft", "system4.asd");
    let decoded = exact(&bytes, "system4.asd");
    assert_eq!(
        node_value(&decoded, "/kind"),
        Value::Str("global".to_owned())
    );
    assert_eq!(
        node_value(&decoded, "/globals/0/name"),
        Value::Str("gold".to_owned())
    );
    assert_eq!(
        node_value(&decoded, "/globals/0/value"),
        Value::Int {
            v: 1250,
            width: saveprism_core::IntWidth::I32
        }
    );
    assert_eq!(
        node_value(&decoded, "/globals/1/value"),
        Value::Str("Mira".to_owned())
    );
    assert_eq!(node_value(&decoded, "/globals/2/value"), Value::Bool(true));
    assert_eq!(
        node_value(&decoded, "/globals/3/value"),
        Value::Float {
            v: 1.5,
            width: saveprism_core::FloatWidth::F32
        }
    );
}

#[test]
fn system4_values_are_editable_and_written_back() {
    let bytes = fixture("alicesoft", "system4.asd");
    let mut decoded = exact(&bytes, "system4.asd");
    set(&mut decoded, "/globals/0/value", OwnedValue::Int(9999));
    set(
        &mut decoded,
        "/globals/1/value",
        OwnedValue::Str("Rune".to_owned()),
    );
    set(&mut decoded, "/globals/2/value", OwnedValue::Bool(false));
    set(&mut decoded, "/globals/3/value", OwnedValue::Float(2.25));
    let written = AliceSoftCodec
        .encode(&decoded.doc, &decoded.params)
        .unwrap();
    assert_ne!(written, bytes);
    assert!(written.starts_with(b"GD\x01\x01"));
    let again = decode_bytes(&written, "edited system4.asd");
    assert_eq!(
        node_value(&again, "/globals/0/value"),
        Value::Int {
            v: 9999,
            width: saveprism_core::IntWidth::I32
        }
    );
    assert_eq!(
        node_value(&again, "/globals/1/value"),
        Value::Str("Rune".to_owned())
    );
    assert_eq!(node_value(&again, "/globals/2/value"), Value::Bool(false));
    assert_eq!(
        node_value(&again, "/globals/3/value"),
        Value::Float {
            v: 2.25,
            width: saveprism_core::FloatWidth::F32
        }
    );
    assert_eq!(
        AliceSoftCodec.encode(&again.doc, &again.params).unwrap(),
        written
    );
}

#[test]
fn encrypted_system4_is_detected_and_read_only() {
    let mut bytes = b"GD\x01\x01".to_vec();
    bytes.extend_from_slice(&8_u32.to_le_bytes());
    bytes.push(SYS4_ENCRYPTED_MARK);
    bytes.extend_from_slice(&[0_u8; 7]);
    let decoded = decode_bytes(&bytes, "encrypted system4.asd");
    assert_eq!(node_value(&decoded, "/encrypted"), Value::Bool(true));
    assert_eq!(
        AliceSoftCodec
            .encode(&decoded.doc, &decoded.params)
            .unwrap(),
        bytes
    );
}

#[test]
fn artemis_envelope_is_read_only_but_round_trips() {
    use crate::artemis::ArtemisCodec;

    let bytes = fixture("artemis", "saveg.dat");
    let limits = Limits::default();
    let mut ctx = DecodeCtx::new(&limits);
    let decoded = ArtemisCodec.decode(&bytes, &mut ctx).expect("decode");
    assert_eq!(
        ArtemisCodec.encode(&decoded.doc, &decoded.params).unwrap(),
        bytes
    );
    let id = decoded
        .doc
        .resolve(&Path::parse("/body").unwrap())
        .expect("body");
    assert!(
        decoded
            .doc
            .get(id)
            .expect("body node")
            .flags
            .contains(NodeFlags::READ_ONLY)
    );
    assert!(ArtemisCodec.sniff(&bytes, &SniffCtx::default()).value() > 0.9);
    assert_eq!(
        ArtemisCodec
            .sniff(
                b"BOWG\x00\x00\x00\x00\x00\x00\x00\x00",
                &SniffCtx::default()
            )
            .value(),
        0.0
    );
}

#[test]
fn system4_backup_extension_decodes() {
    exact(&fixture("alicesoft", "system4.asdbak"), "system4.asdbak");
}
