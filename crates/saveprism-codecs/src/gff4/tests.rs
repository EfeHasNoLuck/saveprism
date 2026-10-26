//! Tests of the GFF V4 read-only codec.

use std::path::PathBuf;

use saveprism_core::{
    Codec, Confidence, DecodeCtx, Decoded, IntWidth, Limits, NodeFlags, Path, SniffCtx, Value,
};

use super::{GFF4_PARSE, Gff4Codec};

fn fixture(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/synthetic/bioware")
        .join(name);
    std::fs::read(path).expect("fixture is readable")
}

fn decode(bytes: &[u8]) -> Decoded {
    let limits = Limits::default();
    let mut ctx = DecodeCtx::new(&limits);
    Gff4Codec
        .decode(bytes, &mut ctx)
        .expect("valid synthetic GFF4")
}

fn node<'a>(decoded: &'a Decoded, path: &str) -> &'a saveprism_core::Node {
    let id = decoded
        .doc
        .resolve(&Path::parse(path).expect("valid path"))
        .expect("path exists");
    decoded.doc.get(id).expect("node exists")
}

#[test]
fn dragon_age_file_is_read_and_written_unchanged() {
    let bytes = fixture("dao-save.das");
    let decoded = decode(&bytes);
    let root = decoded.doc.get(decoded.doc.root()).expect("root");
    assert_eq!(
        root.type_hint.as_ref().map(|hint| hint.as_str()),
        Some("GFF4:root:SAV:V4.0")
    );
    assert_eq!(
        node(&decoded, "/0x00000001").value,
        Value::Int {
            v: 4242,
            width: IntWidth::U32
        }
    );
    assert_eq!(
        node(&decoded, "/0x00000003").value,
        Value::Str("Dragon Age".to_owned())
    );
    assert_eq!(
        node(&decoded, "/0x00000005/2").value,
        Value::Int {
            v: 13,
            width: IntWidth::U32
        }
    );
    assert!(
        node(&decoded, "/0x00000001")
            .flags
            .contains(NodeFlags::READ_ONLY)
    );
    assert_eq!(
        Gff4Codec
            .encode(&decoded.doc, &decoded.params)
            .expect("read-only writer returns the original"),
        bytes
    );
}

#[test]
fn sniff_requires_the_magic_and_version() {
    let bytes = fixture("dao-save.das");
    let ctx = SniffCtx {
        file_name: Some("save.das"),
        ..SniffCtx::default()
    };
    assert!(Gff4Codec.sniff(&bytes, &ctx).value() > 0.9);
    assert_eq!(Gff4Codec.sniff(b"GFF V3.2", &ctx), Confidence::NONE);
    let gff3 = fixture("character.bic");
    assert_eq!(Gff4Codec.sniff(&gff3, &ctx), Confidence::NONE);
    assert_eq!(Gff4Codec.sniff(b"", &ctx), Confidence::NONE);
}

#[test]
fn a_truncated_file_gives_the_parse_code() {
    let bytes = fixture("dao-save.das");
    let limits = Limits::default();
    let mut ctx = DecodeCtx::new(&limits);
    assert_eq!(
        Gff4Codec
            .decode(bytes.get(..12).unwrap_or_default(), &mut ctx)
            .expect_err("truncated header")
            .code(),
        GFF4_PARSE
    );
    assert_eq!(
        Gff4Codec
            .decode(bytes.get(..80).unwrap_or_default(), &mut ctx)
            .expect_err("cut data block")
            .code(),
        GFF4_PARSE
    );
}
