#![no_main]

use libfuzzer_sys::fuzz_target;
use saveprism_codecs::register_all;
use saveprism_core::pipeline;
use saveprism_core::{Limits, Recipe, Registry, Step};

fuzz_target!(|data: &[u8]| {
    let mut reg = Registry::default();
    register_all(&mut reg);
    let limits = Limits {
        max_decoded_bytes: 16 * 1024 * 1024,
        ..Limits::default()
    };
    let recipe = Recipe::new(vec![Step::bare("format.pixel-crushers")]);
    let Ok(out) = pipeline::decode(data, &recipe, &reg, &limits) else {
        return;
    };
    let bytes = pipeline::encode(&out.doc, &out.recipe, &reg).expect("a decoded document encodes");
    let again = pipeline::decode(&bytes, &out.recipe, &reg, &limits).expect("written data decodes");
    assert!(out.doc.tree_eq(&again.doc), "the written data reads back differently");
});
