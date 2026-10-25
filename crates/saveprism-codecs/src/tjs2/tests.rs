//! Tests for the TJS2 codec.
#![allow(clippy::expect_used, clippy::indexing_slicing)]

use saveprism_core::{
    Codec, DecodeCtx, Edit, Limits, OwnedValue, Path, Recipe, Registry, Step, Value, pipeline,
};

use super::Tjs2Codec;

const SAMPLE: &str = concat!(
    "(const) %[\n",
    " \"name\" => \"Mira\",\n",
    " \"level\" => 42,\n",
    " \"rate\" => 0x1.8000000000000p+1 /* 3 */,\n",
    " \"items\" => (const) [\n",
    "  \"sword\",\n",
    "  7\n",
    " ],\n",
    " \"empty\" => void,\n",
    " \"nick\" => null,\n",
    " \"blob\" => <% DE AD BE EF %>\n",
    "]",
);

fn decode(input: &[u8]) -> saveprism_core::Decoded {
    Tjs2Codec
        .decode(input, &mut DecodeCtx::new(&Limits::default()))
        .expect("TJS2 parses")
}

fn encode(decoded: &saveprism_core::Decoded) -> Vec<u8> {
    Tjs2Codec
        .encode(&decoded.doc, &decoded.params)
        .expect("TJS2 writes")
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

fn value(decoded: &saveprism_core::Decoded, path: &str) -> Value {
    let id = decoded
        .doc
        .resolve(&Path::parse(path).expect("path"))
        .expect("node");
    decoded
        .doc
        .get(id)
        .map(|node| node.value.clone())
        .expect("value")
}

#[test]
fn round_trips_exactly() {
    let decoded = decode(SAMPLE.as_bytes());
    assert_eq!(encode(&decoded), SAMPLE.as_bytes());
    assert_eq!(
        value(&decoded, "/items/1"),
        Value::Int {
            v: 7,
            width: saveprism_core::IntWidth::I64
        }
    );
    assert_eq!(
        value(&decoded, "/blob"),
        Value::Bytes(vec![0xDE, 0xAD, 0xBE, 0xEF])
    );
    assert_eq!(value(&decoded, "/empty"), Value::Null);
}

#[test]
fn utf16le_text_with_bom_round_trips_and_edits() {
    let mut bytes = vec![0xFF, 0xFE];
    bytes.extend(SAMPLE.encode_utf16().flat_map(u16::to_le_bytes));
    let mut decoded = decode(&bytes);
    assert_eq!(encode(&decoded), bytes);
    set(&mut decoded, "/name", OwnedValue::Str("Rune".into()));
    let output = encode(&decoded);
    let units: Vec<u16> = output
        .get(2..)
        .expect("body")
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_le_bytes(*pair))
        .collect();
    let text = String::from_utf16(&units).expect("utf16");
    assert!(text.contains("\"Rune\""));
    assert!(text.contains("0x1.8000000000000p+1 /* 3 */"));
}

#[test]
fn edits_ints_reals_and_strings() {
    let mut decoded = decode(SAMPLE.as_bytes());
    set(&mut decoded, "/level", OwnedValue::Int(99));
    set(&mut decoded, "/rate", OwnedValue::Float(0.25));
    set(&mut decoded, "/items/0", OwnedValue::Str("axe".into()));
    let output = encode(&decoded);
    let text = String::from_utf8(output).expect("utf8");
    assert!(text.contains("\"level\" => 99"));
    assert!(text.contains("\"rate\" => 0.25"));
    assert!(text.contains("\"axe\""));
    let again = decode(text.as_bytes());
    assert_eq!(
        value(&again, "/level"),
        Value::Int {
            v: 99,
            width: saveprism_core::IntWidth::I64
        }
    );
    assert_eq!(value(&again, "/items/0"), Value::Str("axe".into()));
}

#[test]
fn escapes_are_read_and_written() {
    let input = b"(const) %[ \"text\" => \"a\\n\\\"b\\\"\\\\c\\x41\\x32\" ]";
    let decoded = decode(input);
    assert_eq!(value(&decoded, "/text"), Value::Str("a\n\"b\"\\cA2".into()));
    assert_eq!(encode(&decoded), input);
}

#[test]
fn array_root_round_trips() {
    let input = b"(const) [\n 1,\n (const) %[ \"x\" => \"y\" ]\n]";
    let decoded = decode(input);
    assert_eq!(encode(&decoded), input);
}

/// The generator-made files go through the whole pipeline, so the layer wrappers are exercised.
#[test]
fn generated_fixtures_round_trip() {
    let mut reg = Registry::default();
    crate::register_all(&mut reg);
    let base =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/synthetic/kirikiri");
    let cases: [(&str, &[&str]); 5] = [
        ("save01.ksd", &["format.tjs2"]),
        ("system.kdt", &["format.tjs2"]),
        ("scrambled.ksd", &["obfuscation.kirikiri", "format.tjs2"]),
        (
            "scrambled-mode0.ksd",
            &["obfuscation.kirikiri", "format.tjs2"],
        ),
        ("compressed.ksd", &["obfuscation.kirikiri", "format.tjs2"]),
    ];
    for (name, kinds) in cases {
        let input = std::fs::read(base.join(name)).expect("fixture exists");
        let recipe = Recipe::new(kinds.iter().map(|kind| Step::bare(*kind)).collect());
        let decoded = pipeline::decode(&input, &recipe, &reg, &Limits::default())
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        let output = pipeline::encode(&decoded.doc, &decoded.recipe, &reg)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert_eq!(output, input, "{name}");
    }
}
