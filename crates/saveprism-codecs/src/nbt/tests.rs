//! Tests of the NBT codec.

use std::path::PathBuf;

use saveprism_core::detect::{DetectOptions, detect};
use saveprism_core::{
    Codec, DecodeCtx, Decoded, Edit, Limits, OwnedValue, Path, Registry, SaveDoc, Value,
    apply_edits,
};

use super::*;

const JAVA: &str = "synthetic/nbt/save.nbt";
const JAVA_GZ: &str = "synthetic/nbt/save.nbt.gz";
const BEDROCK: &str = "synthetic/nbt/bedrock.dat";
const LEVEL: &str = "synthetic/nbt/level.dat";
const EMPTY: &str = "synthetic/nbt/empty.nbt";

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn fixture(relative: &str) -> Vec<u8> {
    std::fs::read(fixtures().join(relative)).unwrap()
}

fn decode(bytes: &[u8]) -> Decoded {
    let limits = Limits::default();
    let mut ctx = DecodeCtx::new(&limits);
    NbtCodec.decode(bytes, &mut ctx).unwrap()
}

fn exact(bytes: &[u8]) -> Decoded {
    let decoded = decode(bytes);
    assert_eq!(
        NbtCodec.encode(&decoded.doc, &decoded.params).unwrap(),
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
fn every_fixture_round_trips_exactly() {
    for name in [JAVA, BEDROCK, LEVEL, EMPTY] {
        exact(&fixture(name));
    }
}

#[test]
fn the_tree_has_the_expected_values() {
    for name in [JAVA, BEDROCK] {
        let decoded = decode(&fixture(name));
        let doc = &decoded.doc;
        assert_eq!(at(doc, "/name").value, Value::Str("Aster".into()));
        assert_eq!(
            at(doc, "/level").value,
            Value::Int {
                v: 42,
                width: saveprism_core::IntWidth::I32
            }
        );
        assert_eq!(at(doc, "/friends/1/name").value, Value::Str("Cleo".into()));
        assert_eq!(
            at(doc, "/scores/2").value,
            Value::Int {
                v: 300_000,
                width: saveprism_core::IntWidth::I32
            }
        );
        assert_eq!(
            decoded.params.str(super::ENDIAN).unwrap(),
            if name == JAVA { "big" } else { "little" }
        );
    }
}

#[test]
fn the_bedrock_header_is_kept() {
    let decoded = exact(&fixture(LEVEL));
    assert!(decoded.params.bool(super::HEADER).unwrap());
    assert_eq!(decoded.params.int(super::VERSION).unwrap(), 9);
    assert_eq!(decoded.params.str(super::ROOT_NAME).unwrap(), "");
}

#[test]
fn editing_scalars_and_array_elements_works() {
    for name in [JAVA, BEDROCK, LEVEL] {
        let bytes = fixture(name);
        let mut decoded = exact(&bytes);
        set(&mut decoded, "/level", OwnedValue::Int(77));
        set(&mut decoded, "/scores/0", OwnedValue::Int(-5));
        set(&mut decoded, "/name", OwnedValue::Str("Nova".into()));
        let out = NbtCodec.encode(&decoded.doc, &decoded.params).unwrap();
        assert_ne!(out, bytes, "{name}");
        let again = decode(&out);
        assert_eq!(
            at(&again.doc, "/level").value,
            Value::Int {
                v: 77,
                width: saveprism_core::IntWidth::I32
            }
        );
        assert_eq!(
            at(&again.doc, "/scores/0").value,
            Value::Int {
                v: -5,
                width: saveprism_core::IntWidth::I32
            }
        );
        assert_eq!(at(&again.doc, "/name").value, Value::Str("Nova".into()));
    }
}

#[test]
fn a_byte_value_out_of_range_is_rejected() {
    let bytes = fixture(JAVA);
    let mut decoded = exact(&bytes);
    let result = apply_edits(
        &mut decoded.doc,
        &[Edit::Set {
            path: Path::parse("/slot").unwrap(),
            value: OwnedValue::Int(300),
        }],
        &Limits::default(),
    );
    assert!(result.is_err(), "a byte above 127 must not be accepted");
}

#[test]
fn detection_picks_nbt_directly_and_through_gzip() {
    let mut reg = Registry::default();
    crate::register_all(&mut reg);
    for name in [JAVA, BEDROCK, LEVEL, EMPTY] {
        let bytes = fixture(name);
        let opts = DetectOptions {
            file_name: Some("save.nbt".to_string()),
            ..DetectOptions::default()
        };
        let found = detect(&bytes, &reg, &Limits::default(), &opts).unwrap();
        assert_eq!(found.best.recipe.kinds(), vec!["format.nbt"], "{name}");
    }
    let gz = fixture(JAVA_GZ);
    let opts = DetectOptions {
        file_name: Some("save.nbt.gz".to_string()),
        ..DetectOptions::default()
    };
    let found = detect(&gz, &reg, &Limits::default(), &opts).unwrap();
    assert_eq!(
        found.best.recipe.kinds(),
        vec!["compression.gzip", "format.nbt"]
    );
}
