//! TyranoScript fixtures: the URL-escape layer followed by `format.json`.
#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

use std::path::PathBuf;

use saveprism_codecs::layers::{LAY_URL_ESCAPE_INVALID, UrlEscapeLayer};
use saveprism_codecs::register_all;
use saveprism_core::detect::{DetectOptions, detect};
use saveprism_core::guard::{Verdict, check_round_trip};
use saveprism_core::{
    Edit, Layer, Limits, OwnedValue, Params, Path, Recipe, Registry, Step, Value, apply_edits,
    pipeline,
};

const FIXTURES: [&str; 3] = [
    "synthetic/tyrano/sample_tyrano_data.sav",
    "synthetic/tyrano/sample_tyrano_data_uri.sav",
    "synthetic/tyrano/sample_tyrano_sf.sav",
];

fn registry() -> Registry {
    let mut reg = Registry::default();
    register_all(&mut reg);
    reg
}

fn fixture(relative: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(relative);
    std::fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

fn detect_recipe(bytes: &[u8], reg: &Registry) -> Recipe {
    let limits = Limits::default();
    detect(bytes, reg, &limits, &DetectOptions::default())
        .unwrap()
        .best
        .recipe
}

fn value_at(doc: &saveprism_core::SaveDoc, path: &str) -> Value {
    let id = doc
        .resolve(&Path::parse(path).unwrap())
        .unwrap_or_else(|| panic!("no node at {path}"));
    doc.get(id).unwrap().value.clone()
}

#[test]
fn every_fixture_round_trips_exactly() {
    let reg = registry();
    let limits = Limits::default();
    for relative in FIXTURES {
        let bytes = fixture(relative);
        let recipe = detect_recipe(&bytes, &reg);
        assert_eq!(
            recipe.kinds(),
            ["encoding.url-escape", "format.json"],
            "{relative}"
        );
        let report = check_round_trip(&bytes, &recipe, &reg, &limits).unwrap();
        assert_eq!(report.overall, Verdict::Exact, "{relative}");
        assert!(report.editable, "{relative}");

        let decoded = pipeline::decode(&bytes, &recipe, &reg, &limits).unwrap();
        let written = pipeline::encode(&decoded.doc, &decoded.recipe, &reg).unwrap();
        assert_eq!(written, bytes, "{relative} does not round-trip");
    }
}

#[test]
fn the_two_styles_record_their_own_escapes() {
    let reg = registry();
    let limits = Limits::default();
    let escape = detect_recipe(&fixture(FIXTURES[0]), &reg);
    let uri = detect_recipe(&fixture(FIXTURES[1]), &reg);
    let escape_params = &escape.steps[0].params;
    let uri_params = &uri.steps[0].params;
    assert_eq!(escape_params.str("style").unwrap(), "escape");
    assert_eq!(escape_params.str("hex").unwrap(), "upper");
    assert_eq!(uri_params.str("style").unwrap(), "uri");
    assert_eq!(uri_params.str("hex").unwrap(), "upper");

    // The layer alone must reproduce both files byte for byte.
    for relative in [FIXTURES[0], FIXTURES[1]] {
        let bytes = fixture(relative);
        let (inner, params) = UrlEscapeLayer
            .decode(&bytes, &mut saveprism_core::DecodeCtx::new(&limits))
            .unwrap();
        assert_eq!(UrlEscapeLayer.encode(&inner, &params).unwrap(), bytes);
    }
}

#[test]
fn editing_scalars_in_slot_one_survives_the_layer() {
    let reg = registry();
    let limits = Limits::default();
    let bytes = fixture(FIXTURES[0]);
    let recipe = detect_recipe(&bytes, &reg);
    let mut decoded = pipeline::decode(&bytes, &recipe, &reg, &limits).unwrap();

    assert_eq!(
        value_at(&decoded.doc, "/data/0/stat/f/money"),
        Value::Int {
            v: 1250,
            width: saveprism_core::IntWidth::Unbounded
        }
    );
    assert_eq!(
        value_at(&decoded.doc, "/data/0/stat/f/name"),
        Value::Str("旅人 アリス".into())
    );
    assert_eq!(
        value_at(&decoded.doc, "/data/0/stat/f/hasKey"),
        Value::Bool(true)
    );

    apply_edits(
        &mut decoded.doc,
        &[
            Edit::Set {
                path: Path::parse("/data/0/stat/f/money").unwrap(),
                value: OwnedValue::Int(7777),
            },
            Edit::Set {
                path: Path::parse("/data/0/stat/f/name").unwrap(),
                value: OwnedValue::Str("新しい 旅人".into()),
            },
            Edit::Set {
                path: Path::parse("/data/0/stat/f/hasKey").unwrap(),
                value: OwnedValue::Bool(false),
            },
        ],
        &limits,
    )
    .unwrap();

    let written = pipeline::encode(&decoded.doc, &decoded.recipe, &reg).unwrap();
    assert!(
        written.starts_with(b"%7B"),
        "the edited file keeps the escape style"
    );
    let again = pipeline::decode(&written, &decoded.recipe, &reg, &limits).unwrap();
    assert!(decoded.doc.tree_eq(&again.doc));

    assert_eq!(
        value_at(&again.doc, "/data/0/stat/f/money"),
        Value::Int {
            v: 7777,
            width: saveprism_core::IntWidth::Unbounded
        }
    );
    assert_eq!(
        value_at(&again.doc, "/data/0/stat/f/name"),
        Value::Str("新しい 旅人".into())
    );
    assert_eq!(
        value_at(&again.doc, "/data/0/stat/f/hasKey"),
        Value::Bool(false)
    );
    // Another slot and another kind of value are unchanged.
    assert_eq!(
        value_at(&again.doc, "/data/1/stat/f/name"),
        Value::Str("Boris the merchant".into())
    );
    assert_eq!(
        value_at(&again.doc, "/data/0/stat/f/inventory/potion"),
        Value::Int {
            v: 3,
            width: saveprism_core::IntWidth::Unbounded
        }
    );
}

#[test]
fn malformed_and_truncated_escapes_are_errors() {
    let reg = registry();
    let limits = Limits::default();
    let recipe = Recipe::new(vec![
        Step::new("encoding.url-escape", Params::new()),
        Step::bare("format.json"),
    ]);
    for input in [
        &b"%7B%22a%22%3A%22bad%ZZ%22%7D"[..],
        b"%7B%22a%22%3A%22bad%",
        b"%7B%",
        b"%uD800",
    ] {
        let error = pipeline::decode(input, &recipe, &reg, &limits).unwrap_err();
        assert_eq!(error.code(), LAY_URL_ESCAPE_INVALID, "{input:?}");
    }
}
