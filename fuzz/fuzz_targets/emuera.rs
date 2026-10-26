#![no_main]

use libfuzzer_sys::fuzz_target;
use saveprism_codecs::register_all;
use saveprism_core::pipeline;
use saveprism_core::{Limits, Recipe, Registry, Step};

const HEADER: &[u8] = &[0x89, 0x45, 0x52, 0x41, 0x0D, 0x0A, 0x1A, 0x0A];
const VERSION: &[u8] = &1808_u32.to_le_bytes();
const RESERVED: &[u8] = &0_u32.to_le_bytes();

fuzz_target!(|data: &[u8]| {
    let mut reg = Registry::default();
    register_all(&mut reg);
    let limits = Limits {
        max_decoded_bytes: 16 * 1024 * 1024,
        ..Limits::default()
    };
    // Give the fuzzer a valid header and version so it reaches the variable records.
    let mut input = Vec::with_capacity(data.len() + HEADER.len() + 8);
    if !data.starts_with(HEADER) {
        input.extend_from_slice(HEADER);
        input.extend_from_slice(VERSION);
        input.extend_from_slice(RESERVED);
    }
    input.extend_from_slice(data);
    let recipe = Recipe::new(vec![Step::bare("format.emuera")]);
    let Ok(out) = pipeline::decode(&input, &recipe, &reg, &limits) else {
        return;
    };
    let bytes = pipeline::encode(&out.doc, &out.recipe, &reg).expect("a decoded document encodes");
    let again = pipeline::decode(&bytes, &out.recipe, &reg, &limits).expect("written data decodes");
    assert!(out.doc.tree_eq(&again.doc), "the written data reads back differently");
});
