#![no_main]

use libfuzzer_sys::fuzz_target;
use saveprism_codecs::register_all;
use saveprism_core::pipeline;
use saveprism_core::{Limits, Recipe, Registry, Step};

fuzz_target!(|data: &[u8]| {
    let mut reg = Registry::default();
    register_all(&mut reg);
    let limits = Limits { max_input_bytes: 0x20020, ..Limits::default() };
    let recipe = Recipe::new(vec![Step::bare("format.pokemon-gen3")]);
    let Ok(decoded) = pipeline::decode(data, &recipe, &reg, &limits) else { return; };
    let bytes = pipeline::encode(&decoded.doc, &decoded.recipe, &reg).expect("decoded Gen 3 tree encodes");
    let again = pipeline::decode(&bytes, &decoded.recipe, &reg, &limits).expect("written save decodes");
    assert!(decoded.doc.tree_eq(&again.doc));
});
