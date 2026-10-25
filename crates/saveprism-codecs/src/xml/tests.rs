//! Tests for the XML codec.
#![allow(clippy::expect_used, clippy::indexing_slicing)]

use saveprism_core::{DecodeCtx, Edit, Limits, OwnedValue, Path};

use super::XmlCodec;
use saveprism_core::Codec;

fn decode(input: &[u8]) -> saveprism_core::Decoded {
    XmlCodec
        .decode(input, &mut DecodeCtx::new(&Limits::default()))
        .expect("XML parses")
}

fn encode(decoded: &saveprism_core::Decoded) -> Vec<u8> {
    XmlCodec
        .encode(&decoded.doc, &decoded.params)
        .expect("XML writes")
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

const SAMPLE: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!-- top -->\n<save version=\"2\">\n  <player name='Mira &amp; Rune' hp=\"42\">\n    <item id=\"1\">Sword</item>\n    <item id=\"2\"><![CDATA[Raw <text>]]></item>\n    <note>line</note>\n  </player>\n  <empty/>\n</save>\n";

#[test]
fn round_trips_and_keeps_every_byte() {
    let decoded = decode(SAMPLE.as_bytes());
    assert_eq!(encode(&decoded), SAMPLE.as_bytes());
}

#[test]
fn edits_attribute_and_text_only() {
    let mut decoded = decode(SAMPLE.as_bytes());
    set(
        &mut decoded,
        "/save/player/@hp",
        OwnedValue::Str("99".into()),
    );
    set(
        &mut decoded,
        "/save/player/note/#text",
        OwnedValue::Str("a < b & c".into()),
    );
    let output = encode(&decoded);
    let text = String::from_utf8(output).expect("utf8");
    assert!(text.contains("hp=\"99\""));
    assert!(text.contains("<note>a &lt; b &amp; c</note>"));
    assert!(text.contains("<!-- top -->"));
    assert!(text.contains("name='Mira &amp; Rune'"));
    assert!(text.contains("<![CDATA[Raw <text>]]>"));
    assert!(text.contains("<empty/>"));
}

#[test]
fn repeated_elements_become_numbered_children() {
    let decoded = decode(SAMPLE.as_bytes());
    let id = decoded
        .doc
        .resolve(&Path::parse("/save/player/item#2").expect("path"))
        .expect("second item");
    assert!(matches!(
        decoded.doc.get(id).map(|node| &node.value),
        Some(saveprism_core::Value::Object(_))
    ));
}

#[test]
fn comments_and_cdata_are_read_only() {
    let mut decoded = decode(SAMPLE.as_bytes());
    let result = saveprism_core::apply_edits(
        &mut decoded.doc,
        &[Edit::Set {
            path: Path::parse("/save/#comment").expect("path"),
            value: OwnedValue::Str("changed".into()),
        }],
        &Limits::default(),
    );
    assert!(result.is_err());
}

#[test]
fn crlf_unicode_and_empty_text_round_trip() {
    let input = "\u{feff}<?xml version=\"1.0\"?>\r\n<root>\r\n  <name>Birim \u{0130}</name>\r\n  <blank></blank>\r\n</root>\r\n";
    let decoded = decode(input.as_bytes());
    assert_eq!(encode(&decoded), input.as_bytes());
}

#[test]
fn unknown_entities_stay_read_only() {
    let input = b"<root><x>&unknown;</x></root>";
    let mut decoded = decode(input);
    assert_eq!(encode(&decoded), input);
    let result = saveprism_core::apply_edits(
        &mut decoded.doc,
        &[Edit::Set {
            path: Path::parse("/root/x/#text").expect("path"),
            value: OwnedValue::Str("new".into()),
        }],
        &Limits::default(),
    );
    assert!(result.is_err());
}

#[test]
fn generated_xml_fixture_round_trips() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/synthetic/xml/save.xml");
    let input = std::fs::read(path).expect("fixture exists");
    let decoded = decode(&input);
    assert_eq!(encode(&decoded), input);
}
