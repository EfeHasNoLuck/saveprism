#![no_main]

use libfuzzer_sys::fuzz_target;
use saveprism_codecs::register_all;
use saveprism_core::{pipeline, Limits, Recipe, Registry, Step};

const SAVE: &[u8] = b"\x0bLcfSaveData";
const DATABASE: &[u8] = b"\x0bLcfDataBase";

fuzz_target!(|data: &[u8]| {
    let mut registry = Registry::default();
    register_all(&mut registry);
    let limits = Limits { max_decoded_bytes: 16 * 1024 * 1024, ..Limits::default() };
    let mut input = Vec::with_capacity(data.len().saturating_add(SAVE.len()));
    if !data.starts_with(SAVE) && !data.starts_with(DATABASE) {
        input.extend_from_slice(SAVE);
    }
    input.extend_from_slice(data);
    let recipe = Recipe::new(vec![Step::bare("format.lcf")]);
    let Ok(decoded) = pipeline::decode(&input, &recipe, &registry, &limits) else { return; };
    let Ok(bytes) = pipeline::encode(&decoded.doc, &decoded.recipe, &registry) else {
        panic!("a decoded file did not encode")
    };
    assert_eq!(bytes, input, "LCF data is written back exactly");
});
