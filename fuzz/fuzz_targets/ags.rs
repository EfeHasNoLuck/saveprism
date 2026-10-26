#![no_main]

use libfuzzer_sys::fuzz_target;
use saveprism_codecs::register_all;
use saveprism_core::pipeline;
use saveprism_core::{Limits, Recipe, Registry, Step};

const SIGNATURE: &[u8] = b"Adventure Game Studio saved game v2";
const VERSION: &[u8] = &3060300_i32.to_le_bytes();

fuzz_target!(|data: &[u8]| {
    let mut reg = Registry::default();
    register_all(&mut reg);
    let limits = Limits {
        max_decoded_bytes: 16 * 1024 * 1024,
        ..Limits::default()
    };
    // Give the fuzzer a modern AGS header so it reaches the description and game-state offsets.
    let mut input = Vec::with_capacity(data.len() + SIGNATURE.len() + 4);
    if !data.starts_with(SIGNATURE) {
        input.extend_from_slice(SIGNATURE);
        input.extend_from_slice(VERSION);
    }
    input.extend_from_slice(data);
    let recipe = Recipe::new(vec![Step::bare("format.ags")]);
    let Ok(out) = pipeline::decode(&input, &recipe, &reg, &limits) else {
        return;
    };
    let bytes = pipeline::encode(&out.doc, &out.recipe, &reg).expect("a decoded document encodes");
    let again = pipeline::decode(&bytes, &out.recipe, &reg, &limits).expect("written data decodes");
    assert!(out.doc.tree_eq(&again.doc), "the written data reads back differently");
});
