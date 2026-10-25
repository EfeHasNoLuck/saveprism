//! Tests for the CSV codec.
#![allow(clippy::expect_used, clippy::indexing_slicing)]

use saveprism_core::{Codec, DecodeCtx, Edit, Limits, OwnedValue, Path};

use super::CsvCodec;

fn decode(input: &[u8]) -> saveprism_core::Decoded {
    CsvCodec
        .decode(input, &mut DecodeCtx::new(&Limits::default()))
        .expect("CSV parses")
}

fn encode(decoded: &saveprism_core::Decoded) -> Vec<u8> {
    CsvCodec
        .encode(&decoded.doc, &decoded.params)
        .expect("CSV writes")
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

const SAMPLE: &str =
    "name,class,note\r\nMira,\"Ranger, Scout\",\"said \"\"hi\"\"\"\r\n\"Multi\nLine\",Mage,\r\n";

#[test]
fn round_trips_and_edits_cells_with_quotes() {
    let mut decoded = decode(SAMPLE.as_bytes());
    assert_eq!(encode(&decoded), SAMPLE.as_bytes());
    set(&mut decoded, "/0/name", OwnedValue::Str("Rune".into()));
    set(
        &mut decoded,
        "/0/note",
        OwnedValue::Str("a \"b\", c".into()),
    );
    set(&mut decoded, "/1/class", OwnedValue::Str("Healer".into()));
    let output = encode(&decoded);
    let text = String::from_utf8(output).expect("utf8");
    assert!(text.starts_with("name,class,note\r\nRune,\"Ranger, Scout\",\"a \"\"b\"\", c\"\r\n"));
    assert!(text.contains("\"Multi\nLine\",Healer,"));
}

#[test]
fn headerless_rows_are_numbered() {
    let input = b"1,2,3\n4,5,6\n";
    let decoded = decode(input);
    let id = decoded
        .doc
        .resolve(&Path::parse("/0/1").expect("path"))
        .expect("cell");
    assert!(matches!(
        decoded.doc.get(id).map(|node| &node.value),
        Some(saveprism_core::Value::Str(text)) if text == "2"
    ));
}

#[test]
fn generated_csv_fixtures_round_trip() {
    for name in ["save.csv", "no-header.csv"] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/synthetic/csv")
            .join(name);
        let input = std::fs::read(path).expect("fixture exists");
        let decoded = decode(&input);
        assert_eq!(encode(&decoded), input, "{name}");
    }
}

fn encoding_fixture(name: &str) -> Vec<u8> {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/synthetic/encodings")
        .join(name);
    std::fs::read(path).expect("fixture")
}

#[test]
fn a_utf16be_csv_round_trips_and_edits() {
    let bytes = encoding_fixture("csv-utf16be.csv");
    let mut decoded = decode(&bytes);
    assert_eq!(encode(&decoded), bytes);
    set(
        &mut decoded,
        "/0/name",
        OwnedValue::Str("\u{30a2}\u{30ad}".into()),
    );
    let again = decode(&encode(&decoded));
    let id = again
        .doc
        .resolve(&Path::parse("/0/name").expect("path"))
        .expect("node");
    assert_eq!(
        again.doc.get(id).map(|node| node.value.clone()),
        Some(saveprism_core::Value::Str("アキ".into()))
    );
}
