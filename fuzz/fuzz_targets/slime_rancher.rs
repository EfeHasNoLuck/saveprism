#![no_main]

use libfuzzer_sys::fuzz_target;
use saveprism_codecs::register_all;
use saveprism_core::pipeline;
use saveprism_core::{Limits, Recipe, Registry, Step};

/// Empty-tag root record, version 11, begin mark, then the first name-table tag.
const HEADER: &[u8] = b"\x00\x0b\x00\x00\x00\x00\x30\x0bINDXTBLLKUP";

fuzz_target!(|data: &[u8]| {
    let mut reg = Registry::default();
    register_all(&mut reg);
    let limits = Limits::default();
    // Give the fuzzer the header so it reaches the name tables, summary and slot scans.
    let mut input = Vec::with_capacity(data.len() + HEADER.len());
    if !data.starts_with(HEADER) {
        input.extend_from_slice(HEADER);
    }
    input.extend_from_slice(data);
    let recipe = Recipe::new(vec![Step::bare("format.slime-rancher-2")]);
    let Ok(out) = pipeline::decode(&input, &recipe, &reg, &limits) else {
        return;
    };
    let bytes = pipeline::encode(&out.doc, &out.recipe, &reg).expect("a decoded document encodes");
    assert_eq!(bytes, input, "an unchanged document writes the input back");
});
