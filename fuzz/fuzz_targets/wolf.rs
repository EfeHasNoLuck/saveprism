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
    // Build a plausible WOLF header so the fuzzer reaches the parts and the variable database.
    let mut input = vec![0_u8; 0x14];
    input[6] = 0x55;
    input.push(0x19);
    input.extend_from_slice(&4_u16.to_le_bytes());
    input.extend_from_slice(b"test");
    input.extend_from_slice(&0x8E_u16.to_le_bytes());
    input.extend_from_slice(data);
    let recipe = Recipe::new(vec![Step::bare("format.wolf-rpg")]);
    let Ok(out) = pipeline::decode(&input, &recipe, &reg, &limits) else {
        return;
    };
    // A decoded document must be writable, and what is written must read back as the same tree.
    let bytes = pipeline::encode(&out.doc, &out.recipe, &reg).expect("a decoded document encodes");
    let again = pipeline::decode(&bytes, &out.recipe, &reg, &limits).expect("written data decodes");
    assert!(out.doc.tree_eq(&again.doc), "the written data reads back differently");
});
