//! Tests for the Easy Save 3 encryption layer (`crypto.es3-aes`) on the keyed fixtures.
//! The password `synthetic-test-key` is invented for these files.

use std::path::Path as FsPath;

use saveprism_codecs::es3::{ES3_BAD_DATA, ES3_PASSWORD_REQUIRED, ES3_WRONG_PASSWORD};
use saveprism_core::pipeline;
use saveprism_core::{DecodeCtx, Limits, Params, Recipe, Registry, Step};

const PASSWORD: &str = "synthetic-test-key";

fn fixture(name: &str) -> std::io::Result<Vec<u8>> {
    let path = FsPath::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/keyed/es3-aes")
        .join(name);
    std::fs::read(&path)
}

fn registry() -> Registry {
    let mut reg = Registry::default();
    saveprism_codecs::register_all(&mut reg);
    reg
}

fn step_with_password(password: &str) -> Step {
    let mut params = Params::new();
    params.set("password", password);
    Step::new("crypto.es3-aes", params)
}

fn plain_recipe(password: &str) -> Recipe {
    Recipe::new(vec![step_with_password(password), Step::bare("format.es3")])
}

fn gzip_recipe(password: &str) -> Recipe {
    Recipe::new(vec![
        step_with_password(password),
        Step::bare("compression.gzip"),
        Step::bare("format.es3"),
    ])
}

/// Puts the password back into the first step, as the caller does before encoding.
/// Returns `None` when the recipe has no steps.
fn with_password(recipe: &Recipe, password: &str) -> Option<Recipe> {
    let mut recipe = recipe.clone();
    let step = recipe.steps.get_mut(0)?;
    step.params.set("password", password);
    Some(recipe)
}

#[test]
fn plain_file_round_trips_byte_for_byte() {
    let reg = registry();
    let input = fixture("settings.es3").unwrap();
    let out = pipeline::decode(&input, &plain_recipe(PASSWORD), &reg, &Limits::default()).unwrap();
    let text = String::from_utf8(out.frames.last().unwrap().clone()).unwrap();
    assert!(text.starts_with("{\"musicVolume\""), "{text}");

    let again = pipeline::encode(
        &out.doc,
        &with_password(&out.recipe, PASSWORD).unwrap(),
        &reg,
    )
    .unwrap();
    assert_eq!(again, input);
}

#[test]
fn gzip_file_opens_and_keeps_its_content() {
    let reg = registry();
    let input = fixture("progress-gzip.es3").unwrap();
    let out = pipeline::decode(&input, &gzip_recipe(PASSWORD), &reg, &Limits::default()).unwrap();
    let plain = out.frames.last().unwrap();
    let text = String::from_utf8(plain.clone()).unwrap();
    assert!(text.trim_start().starts_with('{'), "{text}");
    assert!(text.contains("\"level\""), "{text}");
    // The decrypted and unzipped text is the original plain ES3 text (as written by
    // fixtures/tools/make_es3_aes.ps1). Only the line ending depends on that script's checkout.
    let expected = "{\n\t\"level\" : {\n\t\t\"__type\" : \"int\",\n\t\t\"value\" : 12\n\t},\n\
        \t\"unlocked\" : {\n\t\t\"__type\" : \"System.Collections.Generic.List`1[[System.Int32, mscorlib, \
        Version=4.0.0.0, Culture=neutral, PublicKeyToken=b77a5c561934e089]],mscorlib\",\n\
        \t\t\"value\" : [1,2,3,5,8]\n\t}\n}";
    let expected_crlf = expected.replace('\n', "\r\n");
    assert!(
        text == expected || text == expected_crlf,
        "decrypted text differs from the original: {text:?}"
    );

    // The codec step: the re-encoded text may be laid out differently (ES3 mixes pretty and
    // compact arrays), but it must be editable, which means it decodes to the same document.
    let codec_step = out.recipe.steps.last().unwrap();
    let codec = reg.codec(&codec_step.kind).unwrap();
    let rebuilt = codec.encode(&out.doc, &codec_step.params).unwrap();
    let limits = Limits::default();
    let mut ctx =
        DecodeCtx::new(&limits).with_step(out.recipe.steps.len() - 1, codec_step.params.clone());
    let redecoded = codec.decode(&rebuilt, &mut ctx).unwrap();
    assert!(out.doc.tree_eq(&redecoded.doc));

    let recipe = with_password(&out.recipe, PASSWORD).unwrap();
    let again = pipeline::encode(&out.doc, &recipe, &reg).unwrap();
    // The GZip bytes may differ if the compressor is not reproducible; the document must not.
    let reopened =
        pipeline::decode(&again, &gzip_recipe(PASSWORD), &reg, &Limits::default()).unwrap();
    assert!(reopened.doc.tree_eq(&out.doc));
    // The encryption layer itself is exact: wrapping the same GZip bytes gives the same file.
    let gzip_bytes = out.frames.get(1).unwrap();
    let layer = reg.layer("crypto.es3-aes").unwrap();
    let step = recipe.steps.first().unwrap();
    assert_eq!(layer.encode(gzip_bytes, &step.params).unwrap(), input);
}

#[test]
fn wrong_password_is_an_error_not_a_panic() {
    let reg = registry();
    for name in ["settings.es3", "progress-gzip.es3"] {
        let input = fixture(name).unwrap();
        let error = pipeline::decode(
            &input,
            &plain_recipe("not-the-password"),
            &reg,
            &Limits::default(),
        )
        .unwrap_err();
        assert_eq!(error.code(), ES3_WRONG_PASSWORD, "{name}");
    }
}

#[test]
fn missing_password_is_reported() {
    let reg = registry();
    let recipe = Recipe::new(vec![Step::bare("crypto.es3-aes"), Step::bare("format.es3")]);
    let error = pipeline::decode(
        &fixture("settings.es3").unwrap(),
        &recipe,
        &reg,
        &Limits::default(),
    )
    .unwrap_err();
    assert_eq!(error.code(), ES3_PASSWORD_REQUIRED);
}

#[test]
fn truncated_and_empty_files_are_errors() {
    let reg = registry();
    let input = fixture("settings.es3").unwrap();
    for cut in [0_usize, 5, 16, 17, 31, 33] {
        let short = input.get(..cut).unwrap();
        let error =
            pipeline::decode(short, &plain_recipe(PASSWORD), &reg, &Limits::default()).unwrap_err();
        assert!(error.code().as_str().starts_with("SP-"), "{cut}");
        if cut == 17 {
            assert_eq!(error.code(), ES3_BAD_DATA);
        }
    }
}

#[test]
fn password_is_not_stored_in_the_recipe() {
    let reg = registry();
    let out = pipeline::decode(
        &fixture("settings.es3").unwrap(),
        &plain_recipe(PASSWORD),
        &reg,
        &Limits::default(),
    )
    .unwrap();
    let first = out.recipe.steps.first().unwrap();
    assert!(!first.params.contains("password"));
    assert!(first.params.contains("iv"));
    let toml = out.recipe.to_toml().unwrap();
    assert!(!toml.contains(PASSWORD), "{toml}");
}

#[test]
fn encode_without_password_is_an_error() {
    let reg = registry();
    let out = pipeline::decode(
        &fixture("settings.es3").unwrap(),
        &plain_recipe(PASSWORD),
        &reg,
        &Limits::default(),
    )
    .unwrap();
    assert!(pipeline::encode(&out.doc, &out.recipe, &reg).is_err());
}
