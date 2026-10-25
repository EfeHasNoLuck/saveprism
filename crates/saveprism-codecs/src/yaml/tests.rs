//! Tests for the YAML codec.
#![allow(clippy::expect_used, clippy::indexing_slicing)]

use saveprism_core::{Codec, DecodeCtx, Edit, Limits, OwnedValue, Path};

use super::YamlCodec;

fn decode(input: &[u8]) -> saveprism_core::Decoded {
    YamlCodec
        .decode(input, &mut DecodeCtx::new(&Limits::default()))
        .expect("YAML parses")
}

fn encode(decoded: &saveprism_core::Decoded) -> Vec<u8> {
    YamlCodec
        .encode(&decoded.doc, &decoded.params)
        .expect("YAML writes")
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

const SAMPLE: &str = "# save\nplayer:\n  name: Mira\n  level: 42\n  ratio: 1.5\n  active: true\n  title: \"hero\"\n  single: 'a b'\n  bio: |\n    line one\n    line two\nitems:\n  - sword\n  - shield\nanchored: &base {x: 1}\ncopy: *base\ntagged: !!str 5\nempty:\n";

#[test]
fn round_trips_and_edits_styles() {
    let mut decoded = decode(SAMPLE.as_bytes());
    assert_eq!(encode(&decoded), SAMPLE.as_bytes());
    set(
        &mut decoded,
        "/0/player/name",
        OwnedValue::Str("Rune: the bold".into()),
    );
    set(&mut decoded, "/0/player/level", OwnedValue::Int(7));
    set(
        &mut decoded,
        "/0/player/title",
        OwnedValue::Str("a \"b\"".into()),
    );
    set(
        &mut decoded,
        "/0/player/single",
        OwnedValue::Str("it's".into()),
    );
    set(
        &mut decoded,
        "/0/player/bio",
        OwnedValue::Str("new\nlines".into()),
    );
    let output = encode(&decoded);
    let text = String::from_utf8(output).expect("utf8");
    assert!(text.contains("name: \"Rune: the bold\""));
    assert!(text.contains("level: 7"));
    assert!(text.contains("title: \"a \\\"b\\\"\""));
    assert!(text.contains("single: 'it''s'"));
    assert!(text.contains("bio: |\n    new\n    lines"));
    assert!(text.contains("anchored: &base {x: 1}"));
    assert!(text.contains("copy: *base"));
    assert!(text.contains("tagged: !!str 5"));
}

#[test]
fn anchors_aliases_and_tags_are_read_only() {
    let mut decoded = decode(SAMPLE.as_bytes());
    for path in ["/0/copy", "/0/tagged", "/0/anchored"] {
        let result = saveprism_core::apply_edits(
            &mut decoded.doc,
            &[Edit::Set {
                path: Path::parse(path).expect("path"),
                value: OwnedValue::Str("x".into()),
            }],
            &Limits::default(),
        );
        assert!(result.is_err(), "{path} must stay read-only");
    }
}

#[test]
fn crlf_and_flow_style_round_trip() {
    let input = "---\r\nflow: {a: 1, b: [true, null]}\r\nlist: [1, 2.5, \"x\"]\r\n";
    let decoded = decode(input.as_bytes());
    assert_eq!(encode(&decoded), input.as_bytes());
}

#[test]
fn generated_yaml_fixture_round_trips() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/synthetic/yaml/save.yaml");
    let input = std::fs::read(path).expect("fixture exists");
    let decoded = decode(&input);
    assert_eq!(encode(&decoded), input);
}
