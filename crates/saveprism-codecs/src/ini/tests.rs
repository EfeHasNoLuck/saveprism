//! Tests for the INI codec.
#![allow(clippy::expect_used, clippy::indexing_slicing)]

use saveprism_core::{Codec, DecodeCtx, Edit, Limits, OwnedValue, Path};

use super::IniCodec;

fn decode(input: &[u8]) -> saveprism_core::Decoded {
    IniCodec
        .decode(input, &mut DecodeCtx::new(&Limits::default()))
        .expect("INI parses")
}

fn encode(decoded: &saveprism_core::Decoded) -> Vec<u8> {
    IniCodec
        .encode(&decoded.doc, &decoded.params)
        .expect("INI writes")
}

fn set(decoded: &mut saveprism_core::Decoded, path: &str, value: OwnedValue) {
    saveprism_core::apply_edits(
        &mut decoded.doc,
        &[Edit::Set {
            path: Path::parse(path).expect("path"),
            value,
        }],
        &Limits::default(),
    )
    .expect("edit applies");
}

const SAMPLE: &str = "; game settings\r\n[Player]\r\n  name = Mira  ; the hero\r\n  level=42\r\n# comment\r\n[Window]\r\nwidth = 1280\r\nheight=720\r\n[Player]\r\nlevel = 43\r\n";

#[test]
fn round_trips_and_edits_one_value() {
    let mut decoded = decode(SAMPLE.as_bytes());
    assert_eq!(encode(&decoded), SAMPLE.as_bytes());
    set(
        &mut decoded,
        "/Player#2/level",
        OwnedValue::Str("99".into()),
    );
    let output = encode(&decoded);
    let text = String::from_utf8(output).expect("utf8");
    assert!(text.contains("level = 99"));
    assert!(text.contains("  name = Mira  ; the hero"));
    assert!(text.contains("; game settings\r\n"));
    assert!(text.contains("[Window]\r\nwidth = 1280"));
}

#[test]
fn repeated_sections_are_numbered_and_unnamed_keys_work() {
    let input = b"top = 1\n[A]\nx = 2\n[A]\nx = 3\n";
    let decoded = decode(input);
    assert!(
        decoded
            .doc
            .resolve(&Path::parse("/A#2/x").expect("path"))
            .is_some()
    );
    assert!(
        decoded
            .doc
            .resolve(&Path::parse("/A/x").expect("path"))
            .is_some()
    );
    assert!(
        decoded
            .doc
            .resolve(&Path::parse("//top").expect("path"))
            .is_some()
    );
}

#[test]
fn value_with_equals_and_hash_stays_intact() {
    let input = b"[A]\nurl = https://example.test/a?b=1#frag\n";
    let decoded = decode(input);
    assert_eq!(encode(&decoded), input);
}

#[test]
fn generated_ini_fixture_round_trips() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/synthetic/ini/settings.ini");
    let input = std::fs::read(path).expect("fixture exists");
    let decoded = decode(&input);
    assert_eq!(encode(&decoded), input);
}

fn encoding_fixture(name: &str) -> Vec<u8> {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/synthetic/encodings")
        .join(name);
    std::fs::read(path).expect("fixture")
}

#[test]
fn a_cp949_ini_round_trips_and_edits() {
    let bytes = encoding_fixture("ini-cp949.ini");
    let mut decoded = decode(&bytes);
    assert_eq!(encode(&decoded), bytes);
    set(
        &mut decoded,
        "/Player/name",
        OwnedValue::Str("\u{bbf8}\u{bbf8}".into()),
    );
    let again = decode(&encode(&decoded));
    let id = again
        .doc
        .resolve(&Path::parse("/Player/name").expect("path"))
        .expect("node");
    assert_eq!(
        again.doc.get(id).map(|node| node.value.clone()),
        Some(saveprism_core::Value::Str("미미".into()))
    );
}
