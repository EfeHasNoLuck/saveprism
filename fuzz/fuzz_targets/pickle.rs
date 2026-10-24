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
        max_nodes: 1_000_000,
        ..Limits::default()
    };
    // Most inputs should look like a pickle: a protocol header in front, STOP at the end.
    let mut input = Vec::with_capacity(data.len() + 3);
    if !data.starts_with(&[0x80]) {
        input.extend_from_slice(&[0x80, 0x04]);
    }
    input.extend_from_slice(data);
    if !input.ends_with(b".") {
        input.push(b'.');
    }
    let recipe = Recipe::new(vec![Step::bare("format.python-pickle")]);
    let Ok(out) = pipeline::decode(&input, &recipe, &reg, &limits) else {
        return;
    };
    // An unchanged document writes back byte for byte.
    let bytes = pipeline::encode(&out.doc, &out.recipe, &reg).expect("a decoded document encodes");
    assert_eq!(bytes, input, "the written data differs from the input");
});
