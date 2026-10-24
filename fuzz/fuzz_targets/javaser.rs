#![no_main]

use libfuzzer_sys::fuzz_target;
use saveprism_codecs::register_all;
use saveprism_core::{pipeline, Limits, Recipe, Registry, Step};

fuzz_target!(|data: &[u8]| {
    let mut registry = Registry::default();
    register_all(&mut registry);
    let limits = Limits { max_decoded_bytes: 16 * 1024 * 1024, ..Limits::default() };
    let mut input = Vec::with_capacity(data.len().saturating_add(4));
    if !data.starts_with(&[0xac, 0xed, 0, 5]) { input.extend_from_slice(&[0xac, 0xed, 0, 5]); }
    input.extend_from_slice(data);
    let recipe = Recipe::new(vec![Step::bare("format.java-serialization")]);
    let Ok(decoded) = pipeline::decode(&input, &recipe, &registry, &limits) else { return; };
    let Ok(bytes) = pipeline::encode(&decoded.doc, &decoded.recipe, &registry) else { return; };
    let Ok(again) = pipeline::decode(&bytes, &decoded.recipe, &registry, &limits) else { panic!("encoded stream did not decode") };
    assert!(decoded.doc.tree_eq(&again.doc));
});
