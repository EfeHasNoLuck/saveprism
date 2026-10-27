#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

//! `format.gvas` struct type hint inference: synthetic UE 4.27 saves whose maps
//! and sets contain struct keys or values that carry no type in the file.

use std::path::{Path, PathBuf};

use saveprism_codecs::gvas::{self, GVAS_PARSE};
use saveprism_core::{
    DecodeCtx, Decoded, Edit, Limits, OwnedValue, ParamValue, Params, Path as DocPath, Registry,
    Value, apply_edits,
};

const MAP_STRUCT_KEYS: &str = "synthetic/gvas-hints/map-struct-keys.sav";
const SET_STRUCTS: &str = "synthetic/gvas-hints/set-structs.sav";
const MAP_GUID_VALUES: &str = "synthetic/gvas-hints/map-guid-values.sav";
const MAP_UNRESOLVABLE: &str = "synthetic/gvas-hints/map-unresolvable.sav";

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(fixtures().join(name)).unwrap()
}

fn codec() -> Registry {
    let mut registry = Registry::default();
    gvas::register(&mut registry);
    registry
}

fn decode(registry: &Registry, bytes: &[u8], hints: Params) -> saveprism_core::Result<Decoded> {
    let limits = Limits::default();
    let mut ctx = DecodeCtx::new(&limits).with_step(0, hints);
    let codec = registry.codec("format.gvas").unwrap();
    codec.decode(bytes, &mut ctx)
}

fn types_of(decoded: &Decoded) -> Params {
    match decoded.params.get("types") {
        Some(ParamValue::Table(table)) => table.clone(),
        other => panic!("types parameter is {other:?}"),
    }
}

fn exact(registry: &Registry, bytes: &[u8]) -> Decoded {
    let decoded = decode(registry, bytes, Params::new()).expect("the save decodes");
    let again = registry
        .codec("format.gvas")
        .unwrap()
        .encode(&decoded.doc, &decoded.params)
        .expect("the save encodes");
    assert_eq!(again, bytes, "re-encoded bytes differ");
    decoded
}

fn int_at(doc: &saveprism_core::SaveDoc, path: &str) -> i128 {
    let id = doc
        .resolve(&DocPath::parse(path).unwrap())
        .unwrap_or_else(|| panic!("no node at {path}"));
    match doc.get(id).map(|node| &node.value) {
        Some(Value::Int { v, .. }) => *v,
        other => panic!("{path} is {other:?}"),
    }
}

#[test]
fn infers_a_struct_map_key() {
    let registry = codec();
    let bytes = fixture(MAP_STRUCT_KEYS);
    let decoded = exact(&registry, &bytes);
    assert_eq!(
        types_of(&decoded).get("Scores.Key"),
        Some(&ParamValue::Str("Struct".to_string()))
    );
    assert_eq!(
        int_at(&decoded.doc, "/root/properties/Scores_0/0/value/Points_0"),
        11
    );
}

#[test]
fn infers_struct_set_elements() {
    let registry = codec();
    let bytes = fixture(SET_STRUCTS);
    let decoded = exact(&registry, &bytes);
    assert_eq!(
        types_of(&decoded).get("Tags"),
        Some(&ParamValue::Str("Struct".to_string()))
    );
}

#[test]
fn infers_a_guid_map_value() {
    let registry = codec();
    let bytes = fixture(MAP_GUID_VALUES);
    let decoded = exact(&registry, &bytes);
    assert_eq!(
        types_of(&decoded).get("Ids.Value"),
        Some(&ParamValue::Str("Guid".to_string()))
    );
}

#[test]
fn found_hints_are_reused_on_the_next_decode() {
    let registry = codec();
    let bytes = fixture(MAP_STRUCT_KEYS);
    let first = exact(&registry, &bytes);

    let mut hints = Params::new();
    hints.set("types", types_of(&first));
    let second = decode(&registry, &bytes, hints).expect("the stored hints decode");
    assert_eq!(types_of(&second), types_of(&first));
    assert_eq!(
        registry
            .codec("format.gvas")
            .unwrap()
            .encode(&second.doc, &second.params)
            .unwrap(),
        bytes
    );
}

#[test]
fn a_map_value_can_be_edited_without_losing_the_hints() {
    let registry = codec();
    let bytes = fixture(MAP_STRUCT_KEYS);
    let mut decoded = exact(&registry, &bytes);
    let value_path = "/root/properties/Scores_0/0/value/Points_0";
    apply_edits(
        &mut decoded.doc,
        &[Edit::Set {
            path: DocPath::parse(value_path).unwrap(),
            value: OwnedValue::Int(99),
        }],
        &Limits::default(),
    )
    .expect("the value is editable");
    let edited = registry
        .codec("format.gvas")
        .unwrap()
        .encode(&decoded.doc, &decoded.params)
        .expect("the edited save encodes");
    assert_ne!(edited, bytes);

    let after = decode(&registry, &edited, Params::new()).expect("the edited save decodes");
    assert_eq!(
        int_at(&after.doc, "/root/properties/Scores_0/0/value/Points_0"),
        99
    );
    assert_eq!(
        types_of(&after).get("Scores.Key"),
        Some(&ParamValue::Str("Struct".to_string()))
    );
    let again = registry
        .codec("format.gvas")
        .unwrap()
        .encode(&after.doc, &after.params)
        .unwrap();
    assert_eq!(again, edited);
}

#[test]
fn a_file_that_no_hint_can_resolve_fails_with_one_sentence() {
    let registry = codec();
    let error = decode(&registry, &fixture(MAP_UNRESOLVABLE), Params::new())
        .expect_err("no struct type can consume the broken key");
    assert_eq!(error.code(), GVAS_PARSE);
    assert!(
        error
            .to_string()
            .contains("could not infer the struct type hints"),
        "message was: {error}"
    );
}

/// Every real GVAS save under `fixtures/external/unreal-gvas` (local only) must
/// decode and write back exactly. `sample2.sav` is a known `uesave` 0.7.1 write
/// limitation (an ArrayProperty of Text is 56 bytes shorter on write, even when
/// calling `uesave` directly without this codec), so it must stay the only
/// exception.
#[test]
#[ignore = "reads the local, git-ignored fixtures/external/unreal-gvas saves"]
fn every_local_unreal_save_decodes_with_an_exact_round_trip() {
    let root = fixtures().join("external/unreal-gvas");
    if !root.is_dir() {
        return;
    }

    let mut files = Vec::new();
    collect_saves(&root, &mut files);
    assert!(!files.is_empty(), "no .sav files under {}", root.display());

    let registry = codec();
    let codec = registry.codec("format.gvas").unwrap();
    let limits = Limits::default();
    for path in files {
        let bytes = std::fs::read(&path).unwrap();
        let name = path.file_name().unwrap().to_string_lossy();
        let mut ctx = DecodeCtx::new(&limits);
        let decoded = codec
            .decode(&bytes, &mut ctx)
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        let again = codec
            .encode(&decoded.doc, &decoded.params)
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        if name == "sample2.sav" {
            assert_ne!(again, bytes, "{name} now round-trips; drop the exception");
        } else {
            assert_eq!(again, bytes, "{name} does not round-trip");
        }
    }
}

fn collect_saves(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_saves(&path, files);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("sav") {
            files.push(path);
        }
    }
}
