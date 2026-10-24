//! Tests of the Windows Registry (`.reg`) codec.

use std::path::PathBuf;

use proptest::prelude::*;
use saveprism_core::{
    Codec, DecodeCtx, Decoded, Edit, Limits, NodeFlags, OwnedValue, Path, SaveDoc, Value,
    apply_edits, codes,
};

use super::*;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn fixture(relative: &str) -> Vec<u8> {
    std::fs::read(fixtures().join(relative)).unwrap()
}

fn decode_with(bytes: &[u8], limits: &Limits) -> saveprism_core::Result<Decoded> {
    let mut ctx = DecodeCtx::new(limits);
    RegCodec.decode(bytes, &mut ctx)
}

fn decode(bytes: &[u8]) -> Decoded {
    decode_with(bytes, &Limits::default()).unwrap()
}

fn encode(decoded: &Decoded) -> Vec<u8> {
    RegCodec.encode(&decoded.doc, &decoded.params).unwrap()
}

/// Decodes and checks that writing gives the same bytes.
fn exact(bytes: &[u8]) -> Decoded {
    let decoded = decode(bytes);
    let written = encode(&decoded);
    assert_eq!(written, bytes, "re-encoded bytes differ");
    decoded
}

fn set(decoded: &mut Decoded, path: &str, value: OwnedValue) -> saveprism_core::Result<()> {
    apply_edits(
        &mut decoded.doc,
        &[Edit::Set {
            path: Path::parse(path).unwrap(),
            value,
        }],
        &Limits::default(),
    )
}

fn key(name: &str) -> String {
    format!("/HKEY_CURRENT_USER\\Software\\Northwind Labs\\Prism Runner{name}")
}

fn at<'a>(doc: &'a SaveDoc, path: &str) -> &'a saveprism_core::Node {
    let id = doc
        .resolve(&Path::parse(path).unwrap())
        .unwrap_or_else(|| panic!("no node at {path}"));
    doc.get(id).unwrap()
}

fn value_at(doc: &SaveDoc, path: &str) -> Value {
    at(doc, path).value.clone()
}

fn hint_at<'a>(doc: &'a SaveDoc, path: &str) -> &'a str {
    at(doc, path).type_hint.as_ref().unwrap().as_str()
}

#[test]
fn every_fixture_round_trips_exactly() {
    for name in [
        "unity-playerprefs.reg",
        "regedit4-ansi.reg",
        "mixed-types.reg",
    ] {
        let bytes = fixture(&format!("synthetic/reg/{name}"));
        let decoded = exact(&bytes);
        let again = decode(&encode(&decoded));
        assert!(
            decoded.doc.tree_eq(&again.doc),
            "{name} reads back differently"
        );
    }
}

#[test]
fn unity_prefs_have_the_expected_tree() {
    let decoded = exact(&fixture("synthetic/reg/unity-playerprefs.reg"));
    let doc = &decoded.doc;
    let score = key("/HighScore");
    assert_eq!(
        value_at(doc, &score),
        Value::Int {
            v: 300,
            width: saveprism_core::IntWidth::I32
        }
    );
    assert_eq!(hint_at(doc, &score), "REG:dword:HighScore_h64840323");
    assert_eq!(
        value_at(doc, &key("/Lives")),
        Value::Int {
            v: -1,
            width: saveprism_core::IntWidth::I32
        }
    );
    assert_eq!(
        value_at(doc, &key("/Volume")),
        Value::Float {
            v: 0.75,
            width: saveprism_core::FloatWidth::F64
        }
    );
    assert_eq!(
        value_at(doc, &key("/PlayerName")),
        Value::Str("Élodie".into())
    );
    assert_eq!(
        hint_at(doc, &key("/PlayerName")),
        "REG:unity-string:PlayerName_h775476977"
    );
    match value_at(doc, &key("/Motto")) {
        Value::Str(text) => assert!(text.starts_with("Grüße aus dem Prismenlauf!")),
        other => panic!("{other:?}"),
    }
    let unlocked = "/HKEY_CURRENT_USER\\Software\\Northwind Labs\\Prism Runner\\Levels/Unlocked";
    assert_eq!(
        value_at(doc, unlocked),
        Value::Int {
            v: 3,
            width: saveprism_core::IntWidth::I32
        }
    );
    // Containers are read-only.
    assert!(at(doc, "").flags.contains(NodeFlags::READ_ONLY));
    assert!(at(doc, &key("")).flags.contains(NodeFlags::READ_ONLY));
}

#[test]
fn other_value_kinds_are_read_only() {
    let decoded = exact(&fixture("synthetic/reg/mixed-types.reg"));
    let doc = &decoded.doc;
    let base = "/HKEY_CURRENT_USER\\Software\\Northwind Labs\\Prism Runner\\Settings";
    assert!(matches!(
        value_at(doc, &format!("{base}/DisplayName")),
        Value::Str(_)
    ));
    assert!(
        !at(doc, &format!("{base}/DisplayName"))
            .flags
            .contains(NodeFlags::READ_ONLY)
    );
    for name in ["InstallPath", "Profiles", "Seed", "OldCache"] {
        let path = format!("{base}/{name}");
        assert!(
            at(doc, &path).flags.contains(NodeFlags::READ_ONLY),
            "{name} must be read-only"
        );
    }
    assert_eq!(value_at(doc, &format!("{base}/OldCache")), Value::Null);
}

#[test]
fn editing_an_int_survives_and_checks_the_range() {
    let mut decoded = decode(&fixture("synthetic/reg/unity-playerprefs.reg"));
    let path = key("/HighScore");
    set(&mut decoded, &path, OwnedValue::Int(400)).unwrap();
    let written = encode(&decoded);
    let again = decode(&written);
    assert_eq!(
        value_at(&again.doc, &path),
        Value::Int {
            v: 400,
            width: saveprism_core::IntWidth::I32
        }
    );
    let error = set(&mut decoded, &path, OwnedValue::Int(1 << 31)).unwrap_err();
    assert_eq!(error.code(), codes::CORE_EDIT_FAILED);
}

#[test]
fn editing_a_float_survives() {
    let mut decoded = decode(&fixture("synthetic/reg/unity-playerprefs.reg"));
    let path = key("/Volume");
    set(&mut decoded, &path, OwnedValue::Float(0.5)).unwrap();
    let written = encode(&decoded);
    let again = decode(&written);
    assert_eq!(
        value_at(&again.doc, &path),
        Value::Float {
            v: 0.5,
            width: saveprism_core::FloatWidth::F64
        }
    );
}

#[test]
fn editing_a_unity_string_rewraps_it() {
    let mut decoded = decode(&fixture("synthetic/reg/unity-playerprefs.reg"));
    let path = key("/PlayerName");
    let long = "Ægir the Prism Runner of the Northern Reach".repeat(4);
    set(&mut decoded, &path, OwnedValue::Str(long.clone())).unwrap();
    let written = encode(&decoded);
    let text = String::from_utf16(
        &written
            .strip_prefix(&[0xFF, 0xFE])
            .unwrap()
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_le_bytes(*pair))
            .collect::<Vec<_>>(),
    )
    .unwrap();
    assert!(text.contains("\\\r\n  "), "the long value is wrapped");
    let again = decode(&written);
    assert_eq!(value_at(&again.doc, &path), Value::Str(long));
    // The trailing NUL is part of the binary value again.
    assert!(text.contains(",00"));
}

#[test]
fn editing_a_reg_sz_keeps_the_ansi_encoding() {
    let mut decoded = decode(&fixture("synthetic/reg/regedit4-ansi.reg"));
    let path = "/HKEY_CURRENT_USER\\Software\\Legacy Tools\\AnsiGame/Greeting";
    set(&mut decoded, path, OwnedValue::Str("Thé".into())).unwrap();
    let written = encode(&decoded);
    assert_eq!(written.first().copied(), Some(b'R'));
    assert!(written.windows(10).any(|w| w == b"\"Greeting\""));
    assert!(written.windows(4).any(|w| w == b"\x22Th\xe9"));
    let again = decode(&written);
    assert_eq!(value_at(&again.doc, path), Value::Str("Thé".into()));
}

#[test]
fn malformed_input_gives_errors_not_panics() {
    let header = |body: &str| format!("Windows Registry Editor Version 5.00\r\n\r\n{body}");
    let cases = [
        String::new(),
        "not a registry file".to_string(),
        header("value=1\r\n"),
        header("[HKEY_CURRENT_USER\\X]\r\n\"a\"=dword:123\r\n"),
        header("[HKEY_CURRENT_USER\\X]\r\n\"a\"=hex:0g\r\n"),
        header("[HKEY_CURRENT_USER\\X]\r\n\"a\"=hex(2\r\n"),
        header("[HKEY_CURRENT_USER\\X]\r\n\"a\"=\"unterminated\r\n"),
        header("[HKEY_CURRENT_USER\\X]\r\n\"a\"=hex:01,\\\r\n"),
        header("[]\r\n"),
    ];
    for body in cases {
        let bytes = encode_text(&body, TextEncoding::Ansi, false).unwrap();
        assert!(decode_with(&bytes, &Limits::default()).is_err(), "{body:?}");
    }
    let odd = [0xFF_u8, 0xFE, 0x41];
    assert_eq!(
        decode_with(&odd, &Limits::default()).unwrap_err().code(),
        REG_ENCODING
    );
}

#[test]
fn truncated_fixture_gives_an_error() {
    let bytes = fixture("synthetic/reg/unity-playerprefs.reg");
    let truncated = bytes.get(..bytes.len().saturating_sub(3)).unwrap_or(&[]);
    assert!(decode_with(truncated, &Limits::default()).is_err());
}

fn writes_back(bytes: &[u8]) {
    if let Ok(decoded) = decode_with(bytes, &Limits::default()) {
        let written = match RegCodec.encode(&decoded.doc, &decoded.params) {
            Ok(written) => written,
            Err(error) => panic!("a decoded document failed to encode: {error:?}"),
        };
        assert!(written == bytes, "re-encoded bytes differ");
        let again = decode_with(&written, &Limits::default()).unwrap();
        assert!(decoded.doc.tree_eq(&again.doc));
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn random_input_never_panics(body in proptest::collection::vec(any::<u8>(), 0..256)) {
        let mut bytes = b"Windows Registry Editor Version 5.00\r\n\r\n".to_vec();
        bytes.extend_from_slice(&body);
        writes_back(&bytes);
    }

    #[test]
    fn reg_like_input_never_panics(
        body in proptest::collection::vec(
            prop_oneof![
                proptest::sample::select(b"[]=@\"\\,:;-\r\n0123456789abcdefABCDEFhx(2b7)".to_vec()),
                0_u8..32,
                any::<u8>(),
            ],
            0..192,
        )
    ) {
        let mut bytes = b"REGEDIT4\r\n".to_vec();
        bytes.extend_from_slice(&body);
        writes_back(&bytes);
    }

    #[test]
    fn mutated_saves_never_panic(position in 0_usize..1738, byte in any::<u8>()) {
        let mut bytes = fixture("synthetic/reg/unity-playerprefs.reg");
        if let Some(slot) = bytes.get_mut(position) {
            *slot = byte;
        }
        writes_back(&bytes);
    }
}
