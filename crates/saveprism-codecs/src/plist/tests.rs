//! Tests for the property list codecs.
#![allow(clippy::expect_used, clippy::indexing_slicing)]

use saveprism_core::{Codec, DecodeCtx, Edit, Limits, NodeFlags, OwnedValue, Path, Value};

use super::PlistCodec;

fn decode(input: &[u8]) -> saveprism_core::Decoded {
    PlistCodec
        .decode(input, &mut DecodeCtx::new(&Limits::default()))
        .expect("plist parses")
}

fn encode(decoded: &saveprism_core::Decoded) -> Vec<u8> {
    PlistCodec
        .encode(&decoded.doc, &decoded.params)
        .expect("plist writes")
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

const XML_SAMPLE: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\">\n<dict>\n  <key>name</key><string>Mira &amp; Rune</string>\n  <key>level</key><integer>42</integer>\n  <key>ratio</key><real>1.5</real>\n  <key>active</key><true/>\n  <key>items</key><array><string>a</string><string>b</string></array>\n  <key>born</key><date>2020-01-02T03:04:05Z</date>\n  <key>blob</key><data>AAEC</data>\n</dict>\n</plist>\n";

#[test]
fn xml_round_trips_and_edits_typed_values() {
    let mut decoded = decode(XML_SAMPLE.as_bytes());
    assert_eq!(encode(&decoded), XML_SAMPLE.as_bytes());
    set(&mut decoded, "/name", OwnedValue::Str("a < b".into()));
    set(&mut decoded, "/level", OwnedValue::Int(7));
    set(&mut decoded, "/ratio", OwnedValue::Float(2.0));
    set(&mut decoded, "/active", OwnedValue::Bool(false));
    let output = encode(&decoded);
    let text = String::from_utf8(output).expect("utf8");
    assert!(text.contains("<string>a &lt; b</string>"));
    assert!(text.contains("<integer>7</integer>"));
    assert!(text.contains("<real>2.0</real>"));
    assert!(text.contains("<key>active</key><false/>"));
    assert!(text.contains("<date>2020-01-02T03:04:05Z</date>"));
    assert!(text.contains("<data>AAEC</data>"));
}

#[test]
fn xml_true_start_tag_is_replaced_as_a_whole() {
    let input = b"<plist><dict><key>a</key><true></true></dict></plist>";
    let mut decoded = decode(input);
    set(&mut decoded, "/a", OwnedValue::Bool(false));
    assert_eq!(
        encode(&decoded),
        b"<plist><dict><key>a</key><false/></dict></plist>"
    );
}

#[test]
fn binary_plist_is_read_only_but_exact() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/synthetic/plist/binary.plist");
    let input = std::fs::read(path).expect("fixture exists");
    let decoded = decode(&input);
    assert_eq!(encode(&decoded), input);
    assert!(
        decoded
            .doc
            .get(decoded.doc.root())
            .is_some_and(|node| { node.flags.contains(NodeFlags::READ_ONLY) })
    );
    let child = decoded
        .doc
        .children(decoded.doc.root())
        .into_iter()
        .find(|id| {
            decoded
                .doc
                .get(*id)
                .is_some_and(|node| matches!(node.value, Value::Str(ref text) if text == "Mira"))
        });
    assert!(child.is_some(), "binary strings are decoded as data");
}

#[test]
fn binary_plist_from_a_real_writer_is_read_as_data() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/synthetic/plist/binary-real.plist");
    let input = std::fs::read(path).expect("fixture exists");
    let decoded = decode(&input);
    assert_eq!(encode(&decoded), input);
    let mut stack = vec![decoded.doc.root()];
    let mut strings = Vec::new();
    while let Some(id) = stack.pop() {
        if let Some(node) = decoded.doc.get(id)
            && let Value::Str(ref text) = node.value
        {
            strings.push(text.clone());
        }
        stack.extend(decoded.doc.children(id));
    }
    assert!(strings.iter().any(|text| text == "Mira"));
    assert!(strings.iter().any(|text| text == "\u{0130}stanbul"));
    assert!(
        decoded
            .doc
            .get(decoded.doc.root())
            .is_some_and(|node| { node.flags.contains(NodeFlags::READ_ONLY) })
    );
}

#[test]
fn generated_plist_fixtures_round_trip() {
    for name in [
        "save.plist",
        "unicode.plist",
        "binary.plist",
        "binary-real.plist",
    ] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/synthetic/plist")
            .join(name);
        let input = std::fs::read(path).expect("fixture exists");
        let decoded = decode(&input);
        assert_eq!(encode(&decoded), input, "{name}");
    }
}
