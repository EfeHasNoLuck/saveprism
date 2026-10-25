//! Tests for the Java properties codec.
#![allow(clippy::expect_used, clippy::indexing_slicing)]

use saveprism_core::{Codec, DecodeCtx, Edit, Limits, OwnedValue, Path};

use super::PropertiesCodec;

fn decode(input: &[u8]) -> saveprism_core::Decoded {
    PropertiesCodec
        .decode(input, &mut DecodeCtx::new(&Limits::default()))
        .expect("properties parse")
}

fn encode(decoded: &saveprism_core::Decoded) -> Vec<u8> {
    PropertiesCodec
        .encode(&decoded.doc, &decoded.params)
        .expect("properties write")
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

const SAMPLE: &str = "# a comment\n! another\nname = Mira\\ Rune\nlevel:42\ngreeting = hello\\\n    world\nunicode = caf\\u00e9\nempty=\n";

#[test]
fn round_trips_and_decodes_escapes() {
    let decoded = decode(SAMPLE.as_bytes());
    assert_eq!(encode(&decoded), SAMPLE.as_bytes());
    let id = decoded
        .doc
        .resolve(&Path::parse("/greeting").expect("path"))
        .expect("greeting");
    assert!(matches!(
        decoded.doc.get(id).map(|node| &node.value),
        Some(saveprism_core::Value::Str(text)) if text == "helloworld"
    ));
}

#[test]
fn edits_escape_newlines_and_keep_comments() {
    let mut decoded = decode(SAMPLE.as_bytes());
    set(
        &mut decoded,
        "/greeting",
        OwnedValue::Str("line1\nline2".into()),
    );
    set(&mut decoded, "/name", OwnedValue::Str("Mira Rune".into()));
    let output = encode(&decoded);
    let text = String::from_utf8(output).expect("utf8");
    assert!(text.contains("name = Mira Rune"));
    assert!(text.contains("greeting = line1\\nline2"));
    assert!(text.contains("# a comment\n! another\n"));
}

#[test]
fn continuation_span_is_replaced_whole() {
    let input = b"key = one \\\n      two \\\n      three\n";
    let mut decoded = decode(input);
    set(&mut decoded, "/key", OwnedValue::Str("new".into()));
    assert_eq!(encode(&decoded), b"key = new\n");
}

#[test]
fn generated_properties_fixture_round_trips() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/synthetic/properties/settings.properties");
    let input = std::fs::read(path).expect("fixture exists");
    let decoded = decode(&input);
    assert_eq!(encode(&decoded), input);
}
