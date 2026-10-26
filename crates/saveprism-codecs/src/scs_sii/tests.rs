//! Tests of the SCS `.sii` codecs.

use std::path::PathBuf;

use saveprism_core::detect::{DetectOptions, detect};
use saveprism_core::{
    Codec, Confidence, DecodeCtx, Decoded, Edit, IntWidth, Limits, NodeFlags, OwnedValue, Path,
    Registry, SaveDoc, SniffCtx, TypeHint, Value, apply_edits,
};

use super::*;

const PLAIN: &str = "synthetic/scs-sii/plain.sii";
const BINARY: &str = "synthetic/scs-sii/binary.bsii";

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn fixture(relative: &str) -> Vec<u8> {
    std::fs::read(fixtures().join(relative)).unwrap()
}

fn decode(bytes: &[u8]) -> Decoded {
    let limits = Limits::default();
    let mut ctx = DecodeCtx::new(&limits);
    ScsSiiCodec.decode(bytes, &mut ctx).unwrap()
}

fn exact(bytes: &[u8]) -> Decoded {
    let decoded = decode(bytes);
    assert_eq!(
        ScsSiiCodec.encode(&decoded.doc, &decoded.params).unwrap(),
        bytes,
        "re-encoded bytes differ"
    );
    decoded
}

fn set(decoded: &mut Decoded, path: &str, value: OwnedValue) {
    apply_edits(
        &mut decoded.doc,
        &[Edit::Set {
            path: Path::parse(path).unwrap(),
            value,
        }],
        &Limits::default(),
    )
    .unwrap();
}

fn at<'a>(doc: &'a SaveDoc, path: &str) -> &'a saveprism_core::Node {
    let id = doc.resolve(&Path::parse(path).unwrap()).unwrap();
    doc.get(id).unwrap()
}

#[test]
fn the_text_fixture_round_trips_exactly() {
    exact(&fixture(PLAIN));
}

#[test]
fn the_tree_has_units_attributes_and_values() {
    let decoded = decode(&fixture(PLAIN));
    assert_eq!(
        at(&decoded.doc, "/.data.player/class").value,
        Value::Str("player".into())
    );
    assert_eq!(
        at(&decoded.doc, "/.data.player/attributes/0/name").value,
        Value::Str("name".into())
    );
    assert_eq!(
        at(&decoded.doc, "/.data.player/attributes/0/value").value,
        Value::Str("Mira".into())
    );
    assert_eq!(
        at(&decoded.doc, "/.data.player/attributes/1/value").value,
        Value::Int {
            v: 1200,
            width: IntWidth::Unbounded
        }
    );
    assert_eq!(
        at(&decoded.doc, "/.data.player/attributes/3/value").value,
        Value::Bool(true)
    );
    assert!(
        at(&decoded.doc, "/.data.player/attributes/5/value")
            .flags
            .contains(NodeFlags::READ_ONLY)
    );
    assert_eq!(
        at(&decoded.doc, "/.data.other/attributes/0/value").value,
        Value::Str("Ash".into())
    );
}

#[test]
fn edits_patch_only_the_tokens_and_keep_comments_and_layout() {
    let bytes = fixture(PLAIN);
    let mut decoded = exact(&bytes);
    set(
        &mut decoded,
        "/.data.player/attributes/0/value",
        OwnedValue::Str("Mira the Bold".into()),
    );
    set(
        &mut decoded,
        "/.data.player/attributes/1/value",
        OwnedValue::Int(999_999),
    );
    let out = ScsSiiCodec.encode(&decoded.doc, &decoded.params).unwrap();
    let text = String::from_utf8(out).unwrap();
    assert!(text.contains("// SavePrism synthetic SII save"));
    assert!(text.contains("/* a block comment */"));
    assert!(text.contains("name: \"Mira the Bold\""));
    assert!(text.contains("money: 999999"));
    assert!(text.contains("speed: 12.5"));
    let again = decode(text.as_bytes());
    assert_eq!(
        at(&again.doc, "/.data.player/attributes/1/value").value,
        Value::Int {
            v: 999_999,
            width: IntWidth::Unbounded
        }
    );
}

#[test]
fn binary_sii_is_read_only_and_round_trips() {
    let bytes = fixture(BINARY);
    let limits = Limits::default();
    let mut ctx = DecodeCtx::new(&limits);
    let decoded = ScsSiiBinaryCodec.decode(&bytes, &mut ctx).unwrap();
    assert!(decoded.doc.get(decoded.doc.root()).is_some_and(|root| {
        root.flags.contains(NodeFlags::READ_ONLY)
            && root.type_hint.as_ref().map(TypeHint::as_str) == Some("SCS-SIIB:file")
    }));
    assert_eq!(
        ScsSiiBinaryCodec
            .encode(&decoded.doc, &decoded.params)
            .unwrap(),
        bytes
    );
    assert!(ScsSiiBinaryCodec.sniff(&bytes, &SniffCtx::default()) > Confidence::NONE);
}

#[test]
fn an_encrypted_save_opens_read_only_with_the_hint() {
    let bytes = b"ScsC\x00\x00\x00\x01encrypted".to_vec();
    let limits = Limits::default();
    let mut ctx = DecodeCtx::new(&limits);
    let decoded = ScsSiiEncryptedCodec.decode(&bytes, &mut ctx).unwrap();
    let hint = at(&decoded.doc, "/hint");
    assert!(hint.flags.contains(NodeFlags::READ_ONLY));
    assert!(matches!(&hint.value, Value::Str(text) if text.contains("g_save_format")));
    assert_eq!(
        ScsSiiEncryptedCodec
            .encode(&decoded.doc, &decoded.params)
            .unwrap(),
        bytes
    );
    assert!(ScsSiiEncryptedCodec.sniff(&bytes, &SniffCtx::default()) > Confidence::NONE);
}

#[test]
fn text_sniff_rejects_other_text() {
    let ctx = SniffCtx::default();
    assert!(ScsSiiCodec.sniff(&fixture(PLAIN), &ctx) >= Confidence::HIGH);
    assert_eq!(ScsSiiCodec.sniff(b"{\"a\":1}", &ctx), Confidence::NONE);
    assert_eq!(ScsSiiCodec.sniff(b"", &ctx), Confidence::NONE);
}

#[test]
fn detection_picks_the_text_and_binary_codecs() {
    let mut reg = Registry::default();
    crate::register_all(&mut reg);
    let plain = fixture(PLAIN);
    let opts = DetectOptions {
        file_name: Some("game.sii".to_string()),
        ..DetectOptions::default()
    };
    let found = detect(&plain, &reg, &Limits::default(), &opts).unwrap();
    assert_eq!(found.best.recipe.kinds(), vec!["format.scs-sii"]);

    let binary = fixture(BINARY);
    let found = detect(&binary, &reg, &Limits::default(), &opts).unwrap();
    assert_eq!(found.best.recipe.kinds(), vec!["format.scs-sii-binary"]);
}
