use std::path::PathBuf;

use saveprism_core::{
    Codec, DecodeCtx, Edit, Limits, NodeFlags, OwnedValue, Path, SniffCtx, Value, apply_edits,
};

use super::*;

fn fixture(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/synthetic/emuera")
        .join(name);
    std::fs::read(path).expect("read the Emuera fixture")
}

fn decode_named(bytes: &[u8], name: &str) -> Decoded {
    let limits = Limits::default();
    let mut ctx = DecodeCtx::new(&limits);
    EmueraCodec
        .decode(bytes, &mut ctx)
        .unwrap_or_else(|error| panic!("{name} should decode: {error:?}"))
}

fn exact(bytes: &[u8], name: &str) -> Decoded {
    let decoded = decode_named(bytes, name);
    assert_eq!(
        EmueraCodec.encode(&decoded.doc, &decoded.params).unwrap(),
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
fn sniff_needs_the_emuera_header() {
    let bytes = fixture("save00.sav");
    assert!(EmueraCodec.sniff(&bytes, &SniffCtx::default()).value() > 0.98);
    assert_eq!(
        EmueraCodec
            .sniff(b"save00.sav", &SniffCtx::default())
            .value(),
        0.0
    );
    assert_eq!(EmueraCodec.sniff(b"", &SniffCtx::default()).value(), 0.0);
    // A plain `.sav` of another engine is not claimed, even when it is called global.sav.
    let named = SniffCtx {
        file_name: Some("global.sav"),
        ..SniffCtx::default()
    };
    assert_eq!(EmueraCodec.sniff(b"QSPSAVEDGAME", &named).value(), 0.0);
}

#[test]
fn normal_save_round_trips_exactly() {
    exact(&fixture("save00.sav"), "save00.sav");
}

#[test]
fn every_fixture_round_trips_exactly() {
    for name in [
        "save00.sav",
        "save01.sav",
        "global.sav",
        "var_01.dat",
        "chara_00.dat",
    ] {
        exact(&fixture(name), name);
    }
}

#[test]
fn named_variables_and_character_data_are_visible() {
    let decoded = exact(&fixture("save00.sav"), "save00.sav");
    assert_eq!(
        node_value(&decoded, "/file_type"),
        Value::Str("normal".into())
    );
    assert_eq!(
        node_value(&decoded, "/variables/MONEY"),
        Value::Int {
            v: 123456,
            width: saveprism_core::IntWidth::I64
        }
    );
    assert_eq!(
        node_value(&decoded, "/variables/STR/3"),
        Value::Str("放課後".to_owned())
    );
    assert_eq!(
        node_value(&decoded, "/characters/1/values/NAME"),
        Value::Str("ヒロイン".to_owned())
    );
    assert_eq!(
        node_value(&decoded, "/characters/0/values/CFLAG/1"),
        Value::Int {
            v: 100,
            width: saveprism_core::IntWidth::I64
        }
    );
    assert_eq!(
        node_value(&decoded, "/characters/0/user_defined/MYVAR/2"),
        Value::Int {
            v: 9,
            width: saveprism_core::IntWidth::I64
        }
    );
}

#[test]
fn array_shapes_are_kept() {
    let decoded = exact(&fixture("save00.sav"), "save00.sav");
    let da = decoded
        .doc
        .resolve(&Path::parse("/variables/DA").unwrap())
        .and_then(|id| decoded.doc.get(id))
        .expect("DA");
    assert!(matches!(&da.value, Value::Array(rows) if rows.len() == 3));
    let ta = decoded
        .doc
        .resolve(&Path::parse("/variables/TA").unwrap())
        .and_then(|id| decoded.doc.get(id))
        .expect("TA");
    assert!(matches!(&ta.value, Value::Array(planes) if planes.len() == 3));
    assert_eq!(
        node_value(&decoded, "/variables/TA/2/2/2"),
        Value::Int {
            v: 9,
            width: saveprism_core::IntWidth::I64
        }
    );
    assert_eq!(
        node_value(&decoded, "/variables/SB/0/2/2"),
        Value::Str("z".to_owned())
    );
}

#[test]
fn integers_strings_and_array_elements_are_editable() {
    let bytes = fixture("save00.sav");
    let mut decoded = exact(&bytes, "save00.sav");
    apply_edits(
        &mut decoded.doc,
        &[
            Edit::Set {
                path: Path::parse("/variables/MONEY").unwrap(),
                value: OwnedValue::Int(2_000_000),
            },
            Edit::Set {
                path: Path::parse("/variables/FLAG/5").unwrap(),
                value: OwnedValue::Int(77),
            },
            Edit::Set {
                path: Path::parse("/variables/STR/1").unwrap(),
                value: OwnedValue::Str("変更".to_owned()),
            },
            Edit::Set {
                path: Path::parse("/characters/0/values/BASE/0").unwrap(),
                value: OwnedValue::Int(120),
            },
            Edit::Set {
                path: Path::parse("/description").unwrap(),
                value: OwnedValue::Str("edited chapter".to_owned()),
            },
        ],
        &Limits::default(),
    )
    .unwrap();
    let written = EmueraCodec.encode(&decoded.doc, &decoded.params).unwrap();
    assert_ne!(written, bytes);
    let again = decode_named(&written, "edited save00.sav");
    assert_eq!(
        node_value(&again, "/variables/MONEY"),
        Value::Int {
            v: 2_000_000,
            width: saveprism_core::IntWidth::I64
        }
    );
    assert_eq!(
        node_value(&again, "/variables/FLAG/5"),
        Value::Int {
            v: 77,
            width: saveprism_core::IntWidth::I64
        }
    );
    assert_eq!(
        node_value(&again, "/variables/STR/1"),
        Value::Str("変更".to_owned())
    );
    assert_eq!(
        node_value(&again, "/characters/0/values/BASE/0"),
        Value::Int {
            v: 120,
            width: saveprism_core::IntWidth::I64
        }
    );
    assert_eq!(
        EmueraCodec.encode(&again.doc, &again.params).unwrap(),
        written
    );
}

#[test]
fn global_file_exposes_global_arrays() {
    let mut decoded = exact(&fixture("global.sav"), "global.sav");
    assert_eq!(
        node_value(&decoded, "/file_type"),
        Value::Str("global".into())
    );
    assert_eq!(
        node_value(&decoded, "/variables/GLOBALS/1"),
        Value::Str("invented".to_owned())
    );
    apply_edits(
        &mut decoded.doc,
        &[Edit::Set {
            path: Path::parse("/variables/GLOBAL/4").unwrap(),
            value: OwnedValue::Int(50),
        }],
        &Limits::default(),
    )
    .unwrap();
    let written = EmueraCodec.encode(&decoded.doc, &decoded.params).unwrap();
    let again = decode_named(&written, "edited global.sav");
    assert_eq!(
        node_value(&again, "/variables/GLOBAL/4"),
        Value::Int {
            v: 50,
            width: saveprism_core::IntWidth::I64
        }
    );
}

#[test]
fn character_and_variable_files_are_read() {
    let chars = exact(&fixture("chara_00.dat"), "chara_00.dat");
    assert_eq!(
        node_value(&chars, "/characters/0/values/NAME"),
        Value::Str("ヒロイン".to_owned())
    );
    let vars = exact(&fixture("var_01.dat"), "var_01.dat");
    assert_eq!(
        node_value(&vars, "/file_type"),
        Value::Str("variables".into())
    );
    let description = vars
        .doc
        .resolve(&Path::parse("/description").unwrap())
        .and_then(|id| vars.doc.get(id))
        .expect("description");
    assert!(!description.flags.contains(NodeFlags::READ_ONLY));
}

#[test]
fn a_wrong_header_is_rejected() {
    let limits = Limits::default();
    let mut ctx = DecodeCtx::new(&limits);
    let mut wrong = HEADER.to_vec();
    wrong.extend_from_slice(&1700_u32.to_le_bytes());
    let error = EmueraCodec
        .decode(&wrong, &mut ctx)
        .expect_err("only version 1808 is supported");
    assert_eq!(error.code(), EMUERA_UNSUPPORTED);

    let mut ctx = DecodeCtx::new(&limits);
    let error = EmueraCodec
        .decode(b"\x89ERA\x0d\x0a\x1a\x0a\x10\x07\x00\x00", &mut ctx)
        .expect_err("a short magic is not an Emuera save");
    assert_eq!(error.code(), EMUERA_PARSE);
}
