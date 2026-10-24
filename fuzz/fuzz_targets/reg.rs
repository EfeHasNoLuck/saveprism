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
    // Most inputs should look like a .reg export: prepend a header when it is missing.
    let mut input = Vec::with_capacity(data.len() + 36);
    if !data.starts_with(b"Windows Registry Editor Version 5.00") && !data.starts_with(b"REGEDIT4")
    {
        input.extend_from_slice(b"Windows Registry Editor Version 5.00\r\n\r\n");
    }
    input.extend_from_slice(data);
    let recipe = Recipe::new(vec![Step::bare("format.reg")]);
    let Ok(out) = pipeline::decode(&input, &recipe, &reg, &limits) else {
        return;
    };
    // A decoded document must be writable, and what is written must read back as the same tree.
    let bytes = pipeline::encode(&out.doc, &out.recipe, &reg).expect("a decoded document encodes");
    let again = pipeline::decode(&bytes, &out.recipe, &reg, &limits).expect("written data decodes");
    assert!(
        out.doc.tree_eq(&again.doc),
        "the written data reads back differently"
    );
});
