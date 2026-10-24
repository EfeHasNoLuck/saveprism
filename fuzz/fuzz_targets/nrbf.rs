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
    // Most inputs should look like an NRBF stream: prepend a 1.0 header when it is missing.
    let mut input = Vec::with_capacity(data.len() + 17);
    if !(data.first() == Some(&0)
        && data.len() >= 17
        && data.get(9..13) == Some(&[1, 0, 0, 0])
        && data.get(13..17) == Some(&[0, 0, 0, 0]))
    {
        input.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0]);
    }
    input.extend_from_slice(data);
    let recipe = Recipe::new(vec![Step::bare("format.nrbf")]);
    let Ok(out) = pipeline::decode(&input, &recipe, &reg, &limits) else {
        return;
    };
    // A decoded document must be writable, and what is written must read back as the same tree.
    let bytes = pipeline::encode(&out.doc, &out.recipe, &reg).expect("a decoded document encodes");
    let again = pipeline::decode(&bytes, &out.recipe, &reg, &limits).expect("written data decodes");
    assert!(out.doc.tree_eq(&again.doc), "the written data reads back differently");
});
