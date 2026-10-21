//! Detector tests with the real layers and the JSON codec.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::indexing_slicing)]

use saveprism_codecs::register_all;
use saveprism_core::detect::{DetectOptions, detect};
use saveprism_core::pipeline;
use saveprism_core::{Limits, Recipe, Registry, Step, codes};

const JSON: &str = r#"{"name":"hero","level":7,"items":["sword","shield"],"gold":120}"#;

fn registry() -> Registry {
    let mut reg = Registry::default();
    register_all(&mut reg);
    reg
}

/// The file bytes of `JSON` wrapped in `layers` (outermost first), made with the pipeline.
fn make_input(reg: &Registry, layers: &[&str]) -> Vec<u8> {
    let limits = Limits::default();
    let plain = pipeline::decode(
        JSON.as_bytes(),
        &Recipe::new(vec![Step::bare("format.json")]),
        reg,
        &limits,
    )
    .unwrap();
    let codec_params = plain.recipe.steps.last().unwrap().params.clone();
    let mut steps: Vec<Step> = layers.iter().map(|id| Step::bare(*id)).collect();
    steps.push(Step::new("format.json", codec_params));
    pipeline::encode(&plain.doc, &Recipe::new(steps), reg).unwrap()
}

/// Detects `input`, checks the recipe kinds and that decode then encode gives the input back.
fn check(reg: &Registry, input: &[u8], expected: &[&str]) {
    let limits = Limits::default();
    let detection = detect(input, reg, &limits, &DetectOptions::default()).unwrap();
    assert_eq!(detection.best.recipe.kinds(), expected);
    let decoded = pipeline::decode(input, &detection.best.recipe, reg, &limits).unwrap();
    let again = pipeline::encode(&decoded.doc, &decoded.recipe, reg).unwrap();
    assert_eq!(again, input);
}

#[test]
fn plain_json() {
    let reg = registry();
    let input = make_input(&reg, &[]);
    assert_eq!(input, JSON.as_bytes());
    check(&reg, &input, &["format.json"]);
}

#[test]
fn base64_json() {
    let reg = registry();
    let input = make_input(&reg, &["encoding.base64"]);
    check(&reg, &input, &["encoding.base64", "format.json"]);
}

#[test]
fn gzip_json() {
    let reg = registry();
    let input = make_input(&reg, &["compression.gzip"]);
    check(&reg, &input, &["compression.gzip", "format.json"]);
}

#[test]
fn zlib_json() {
    let reg = registry();
    let input = make_input(&reg, &["compression.zlib"]);
    check(&reg, &input, &["compression.zlib", "format.json"]);
}

#[test]
fn lz_string_json_is_not_taken_for_base64() {
    let reg = registry();
    let input = make_input(&reg, &["compression.lz-string"]);
    check(&reg, &input, &["compression.lz-string", "format.json"]);
}

#[test]
fn base64_of_gzip_json() {
    let reg = registry();
    let input = make_input(&reg, &["encoding.base64", "compression.gzip"]);
    check(
        &reg,
        &input,
        &["encoding.base64", "compression.gzip", "format.json"],
    );
}

#[test]
fn random_bytes_match_nothing() {
    let reg = registry();
    // Deterministic pseudo-random bytes; the first byte rules out every sniffer.
    let mut state: u32 = 0x1234_5678;
    let mut input = vec![0xFF_u8];
    for _ in 0..512 {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        input.push(state.to_le_bytes()[3]);
    }
    let error = detect(&input, &reg, &Limits::default(), &DetectOptions::default()).unwrap_err();
    assert_eq!(error.code(), codes::DET_NO_MATCH);
}
