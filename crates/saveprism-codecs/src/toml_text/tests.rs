//! Tests for the TOML codec.
#![allow(clippy::expect_used, clippy::indexing_slicing)]

use saveprism_core::{Codec, DecodeCtx, Edit, Limits, OwnedValue, Path};

use super::TomlCodec;

fn decode(input: &[u8]) -> saveprism_core::Decoded {
    TomlCodec
        .decode(input, &mut DecodeCtx::new(&Limits::default()))
        .expect("TOML parses")
}

fn encode(decoded: &saveprism_core::Decoded) -> Vec<u8> {
    TomlCodec
        .encode(&decoded.doc, &decoded.params)
        .expect("TOML writes")
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

const SAMPLE: &str = "# comment\n[player]\nname = \"Mira\"   # inline\nlevel = 0x2a\nratio = 1e3\nactive = true\nitems = [\"sword\", \"shield\"]\nborn = 1999-01-02T03:04:05Z\n\n[[quest]]\ntitle = 'First'\ndone = false\n";

#[test]
fn round_trips_and_edits_keep_layout() {
    let mut decoded = decode(SAMPLE.as_bytes());
    assert_eq!(encode(&decoded), SAMPLE.as_bytes());
    set(&mut decoded, "/player/level", OwnedValue::Int(7));
    set(
        &mut decoded,
        "/player/name",
        OwnedValue::Str("A \"hero\"".into()),
    );
    set(&mut decoded, "/quest/0/done", OwnedValue::Bool(true));
    let output = encode(&decoded);
    let text = String::from_utf8(output).expect("utf8");
    assert!(text.contains("name = \"A \\\"hero\\\"\"   # inline"));
    assert!(text.contains("level = 7\n"));
    assert!(text.contains("done = true"));
    assert!(text.contains("# comment"));
    assert!(text.contains("born = 1999-01-02T03:04:05Z"));
}

#[test]
fn datetime_is_read_only() {
    let mut decoded = decode(SAMPLE.as_bytes());
    let result = saveprism_core::apply_edits(
        &mut decoded.doc,
        &[Edit::Set {
            path: Path::parse("/player/born").expect("path"),
            value: OwnedValue::Str("now".into()),
        }],
        &Limits::default(),
    );
    assert!(result.is_err());
}

#[test]
fn generated_toml_fixture_round_trips() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/synthetic/toml/settings.toml");
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
fn a_shift_jis_toml_round_trips_and_edits() {
    let bytes = encoding_fixture("toml-shiftjis.toml");
    let mut decoded = decode(&bytes);
    assert_eq!(encode(&decoded), bytes);
    set(
        &mut decoded,
        "/player/name",
        OwnedValue::Str("\u{30a2}\u{30ad}".into()),
    );
    let again = decode(&encode(&decoded));
    let id = again
        .doc
        .resolve(&Path::parse("/player/name").expect("path"))
        .expect("node");
    assert_eq!(
        again.doc.get(id).map(|node| node.value.clone()),
        Some(saveprism_core::Value::Str("アキ".into()))
    );
}
