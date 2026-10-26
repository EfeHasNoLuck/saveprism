//! Tests of the RocksDB block-based table codec.

use std::path::PathBuf;

use proptest::prelude::*;
use saveprism_core::detect::{DetectOptions, detect};
use saveprism_core::guard::{Verdict, check_round_trip};
use saveprism_core::{
    Codec, DecodeCtx, Decoded, Edit, Limits, NodeFlags, OwnedValue, Path, Registry, Value,
    apply_edits,
};

use super::*;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn fixture(relative: &str) -> Vec<u8> {
    std::fs::read(fixtures().join(relative)).unwrap()
}

fn decode_with(bytes: &[u8]) -> saveprism_core::Result<Decoded> {
    let limits = Limits::default();
    let mut ctx = DecodeCtx::new(&limits);
    RocksDbCodec.decode(bytes, &mut ctx)
}

fn exact(bytes: &[u8]) -> Decoded {
    let decoded = decode_with(bytes).unwrap();
    assert_eq!(
        RocksDbCodec.encode(&decoded.doc, &decoded.params).unwrap(),
        bytes
    );
    decoded
}

fn at<'a>(doc: &'a saveprism_core::SaveDoc, path: &str) -> &'a saveprism_core::Node {
    let id = doc
        .resolve(&Path::parse(path).unwrap())
        .unwrap_or_else(|| panic!("no node at {path}"));
    doc.get(id).unwrap()
}

#[test]
fn both_plain_and_deflate_compressed_tables_decode() {
    for path in [
        "synthetic/rocksdb/save.sst",
        "synthetic/rocksdb/compressed.sst",
    ] {
        let decoded = exact(&fixture(path));
        let doc = &decoded.doc;
        assert_eq!(
            at(doc, "").type_hint.as_ref().unwrap().as_str(),
            "ROCKSDB:entries"
        );
        assert_eq!(at(doc, "/player~1gold").value, Value::Str("1250".into()));
        assert_eq!(at(doc, "/player~1name").value, Value::Str("Aria".into()));
        assert_eq!(at(doc, "/removed~1flag").value, Value::Null);
        assert!(
            at(doc, "/player~1gold")
                .flags
                .contains(NodeFlags::READ_ONLY)
        );
    }
}

#[test]
fn edits_are_refused() {
    let mut decoded = decode_with(&fixture("synthetic/rocksdb/save.sst")).unwrap();
    let limits = Limits::default();
    let error = apply_edits(
        &mut decoded.doc,
        &[Edit::Set {
            path: Path::parse("/player~1gold").unwrap(),
            value: OwnedValue::Str("1".into()),
        }],
        &limits,
    )
    .unwrap_err();
    assert_eq!(error.code(), saveprism_core::codes::CORE_READ_ONLY);
    let id = decoded
        .doc
        .resolve(&Path::parse("/player~1gold").unwrap())
        .unwrap();
    decoded.doc.set_flags(id, NodeFlags::empty()).unwrap();
    apply_edits(
        &mut decoded.doc,
        &[Edit::Set {
            path: Path::parse("/player~1gold").unwrap(),
            value: OwnedValue::Str("1".into()),
        }],
        &limits,
    )
    .unwrap();
    let error = RocksDbCodec
        .encode(&decoded.doc, &decoded.params)
        .unwrap_err();
    assert_eq!(error.code(), ROCKSDB_WRITE);
}

#[test]
fn detection_and_round_trip_guard_pick_rocksdb() {
    let mut reg = Registry::default();
    crate::register_all(&mut reg);
    let bytes = fixture("synthetic/rocksdb/save.sst");
    let options = DetectOptions {
        file_name: Some("000005.sst".to_string()),
        ..DetectOptions::default()
    };
    let found = detect(&bytes, &reg, &Limits::default(), &options).unwrap();
    assert_eq!(found.best.recipe.kinds(), vec!["format.rocksdb"]);
    let report = check_round_trip(&bytes, &found.best.recipe, &reg, &Limits::default()).unwrap();
    assert_eq!(report.overall, Verdict::Exact);
}

#[test]
fn unsupported_versions_are_refused() {
    for version in [1_u32, 8, 20] {
        let mut bytes = fixture("synthetic/rocksdb/save.sst");
        let version_at = bytes.len() - 12;
        if let Some(target) = bytes.get_mut(version_at..version_at + 4) {
            target.copy_from_slice(&version.to_le_bytes());
        }
        assert_eq!(
            decode_with(&bytes).unwrap_err().code(),
            ROCKSDB_UNSUPPORTED,
            "version {version}"
        );
    }
}

#[test]
fn truncations_and_random_bytes_are_errors_not_panics() {
    let bytes = fixture("synthetic/rocksdb/save.sst");
    for end in 0..bytes.len() {
        assert!(
            decode_with(&bytes[..end]).is_err(),
            "prefix {end} was accepted"
        );
    }
    assert_eq!(decode_with(b"").unwrap_err().code(), ROCKSDB_PARSE);
}

#[test]
fn the_codec_is_registered() {
    let mut reg = Registry::default();
    crate::register_all(&mut reg);
    assert!(reg.is_codec("format.rocksdb"));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn random_input_never_panics(data in proptest::collection::vec(any::<u8>(), 0..512)) {
        let _ = decode_with(&data);
    }
}
