use std::path::PathBuf;

use saveprism_core::{
    Codec, DecodeCtx, Edit, Limits, NodeFlags, OwnedValue, Path, SniffCtx, Value, apply_edits,
};

use super::*;

fn fixture(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/synthetic/onscripter")
        .join(name);
    std::fs::read(path).expect("read the ONScripter fixture")
}

fn decode_named(bytes: &[u8], name: &str) -> Decoded {
    let limits = Limits::default();
    let mut ctx = DecodeCtx::new(&limits);
    OnscripterCodec
        .decode(bytes, &mut ctx)
        .unwrap_or_else(|error| panic!("{name} should decode: {error:?}"))
}

fn exact(bytes: &[u8], name: &str) -> Decoded {
    let decoded = decode_named(bytes, name);
    assert_eq!(
        OnscripterCodec
            .encode(&decoded.doc, &decoded.params)
            .unwrap(),
        bytes,
        "{name} must write back byte for byte"
    );
    decoded
}

fn node_value(decoded: &Decoded, path: &str) -> Value {
    let path = Path::parse(path).expect("test path");
    decoded
        .doc
        .resolve(&path)
        .and_then(|id| decoded.doc.get(id))
        .map(|node| node.value.clone())
        .expect("the test path exists")
}

#[test]
fn sniff_global_and_slot_files() {
    let global = fixture("global.sav");
    let slot = fixture("save1.dat");
    let ctx = SniffCtx::default();
    let named_global = SniffCtx {
        file_name: Some("global.sav"),
        ..SniffCtx::default()
    };
    assert!(OnscripterCodec.sniff(&global, &named_global).value() > 0.8);
    assert!(OnscripterCodec.sniff(&slot, &ctx).value() > 0.8);
    assert_eq!(OnscripterCodec.sniff(&global, &ctx).value(), 0.0);
    assert_eq!(OnscripterCodec.sniff(b"not a save", &ctx).value(), 0.0);
    // A plain `.sav` of another engine must not be claimed by name alone.
    let qsp = b"QSPSAVEDGAME\x006\x00.\x009\x00.\x006\x00\r\x00\n\x00";
    let named = SniffCtx {
        file_name: Some("global.sav"),
        ..SniffCtx::default()
    };
    assert_eq!(OnscripterCodec.sniff(qsp, &named).value(), 0.0);
}

#[test]
fn global_file_round_trips_and_exposes_variables() {
    let bytes = fixture("global.sav");
    let decoded = exact(&bytes, "global.sav");
    assert_eq!(
        node_value(&decoded, "/variables/0/index"),
        Value::Int {
            v: 200,
            width: saveprism_core::IntWidth::I32
        }
    );
    assert_eq!(
        node_value(&decoded, "/variables/0/number"),
        Value::Int {
            v: 1234,
            width: saveprism_core::IntWidth::I32
        }
    );
    assert_eq!(
        node_value(&decoded, "/variables/1/string"),
        Value::Str("放課後の教室".to_owned())
    );
}

#[test]
fn global_numbers_and_strings_are_editable() {
    let bytes = fixture("global.sav");
    let mut decoded = exact(&bytes, "global.sav");
    let limits = Limits::default();
    apply_edits(
        &mut decoded.doc,
        &[
            Edit::Set {
                path: Path::parse("/variables/0/number").unwrap(),
                value: OwnedValue::Int(-999_999),
            },
            Edit::Set {
                path: Path::parse("/variables/1/string").unwrap(),
                value: OwnedValue::Str("新しい教室".to_owned()),
            },
        ],
        &limits,
    )
    .unwrap();
    let written = OnscripterCodec
        .encode(&decoded.doc, &decoded.params)
        .unwrap();
    assert_ne!(written, bytes);
    let again = decode_named(&written, "edited global.sav");
    assert_eq!(
        node_value(&again, "/variables/0/number"),
        Value::Int {
            v: -999_999,
            width: saveprism_core::IntWidth::I32
        }
    );
    assert_eq!(
        node_value(&again, "/variables/1/string"),
        Value::Str("新しい教室".to_owned())
    );
}

#[test]
fn alternate_global_spelling_is_read() {
    exact(&fixture("gloval.sav"), "gloval.sav");
}

#[test]
fn slot_round_trips_and_description_is_editable() {
    let bytes = fixture("save1.dat");
    let mut decoded = exact(&bytes, "save1.dat");
    assert_eq!(
        node_value(&decoded, "/file_version"),
        Value::Int {
            v: 208,
            width: saveprism_core::IntWidth::I32
        }
    );
    let body = decoded
        .doc
        .resolve(&Path::parse("/body").unwrap())
        .and_then(|id| decoded.doc.get(id))
        .expect("body");
    assert!(
        body.flags.contains(NodeFlags::READ_ONLY),
        "the slot body is raw"
    );
    apply_edits(
        &mut decoded.doc,
        &[Edit::Set {
            path: Path::parse("/description").unwrap(),
            value: OwnedValue::Str("変更した説明".to_owned()),
        }],
        &Limits::default(),
    )
    .unwrap();
    let written = OnscripterCodec
        .encode(&decoded.doc, &decoded.params)
        .unwrap();
    let again = decode_named(&written, "edited save1.dat");
    assert_eq!(
        node_value(&again, "/description"),
        Value::Str("変更した説明".to_owned())
    );
    assert_eq!(
        OnscripterCodec.encode(&again.doc, &again.params).unwrap(),
        written
    );
}

#[test]
fn another_slot_round_trips() {
    exact(&fixture("save10.dat"), "save10.dat");
}

#[test]
fn unrepresentable_text_is_refused() {
    let bytes = fixture("global.sav");
    let mut decoded = exact(&bytes, "global.sav");
    apply_edits(
        &mut decoded.doc,
        &[Edit::Set {
            path: Path::parse("/variables/0/string").unwrap(),
            value: OwnedValue::Str("emoji 🎮".to_owned()),
        }],
        &Limits::default(),
    )
    .unwrap();
    let error = OnscripterCodec
        .encode(&decoded.doc, &decoded.params)
        .expect_err("Shift-JIS cannot hold an emoji");
    assert!(!error.to_string().is_empty());
}
