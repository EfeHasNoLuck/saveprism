#![no_main]

use libfuzzer_sys::fuzz_target;
use saveprism_codecs::register_all;
use saveprism_core::pipeline;
use saveprism_core::{Limits, Recipe, Registry, Step};

const UTF16_MAGIC: &[u8] = &[
    0x51, 0x00, 0x53, 0x00, 0x50, 0x00, 0x53, 0x00, 0x41, 0x00, 0x56, 0x00, 0x45, 0x00, 0x44,
    0x00, 0x47, 0x00, 0x41, 0x00, 0x4D, 0x00, 0x45, 0x00, 0x0D, 0x00, 0x0A, 0x00,
];
const ANSI_MAGIC: &[u8] = b"QSPSAVEDGAME\r\n";
const VERSION: &[u8] = &[
    0x35, 0x00, 0x2E, 0x00, 0x39, 0x00, 0x2E, 0x00, 0x36, 0x00, 0x0D, 0x00, 0x0A, 0x00,
];

fuzz_target!(|data: &[u8]| {
    let mut reg = Registry::default();
    register_all(&mut reg);
    let limits = Limits {
        max_decoded_bytes: 16 * 1024 * 1024,
        ..Limits::default()
    };
    // Most inputs should reach the field reader: give them a save header and version when missing.
    let mut input = Vec::with_capacity(data.len() + 64);
    if !(data.starts_with(UTF16_MAGIC) || data.starts_with(ANSI_MAGIC)) {
        input.extend_from_slice(UTF16_MAGIC);
        input.extend_from_slice(VERSION);
    }
    input.extend_from_slice(data);
    let recipe = Recipe::new(vec![Step::bare("format.qsp")]);
    let Ok(out) = pipeline::decode(&input, &recipe, &reg, &limits) else {
        return;
    };
    let bytes = pipeline::encode(&out.doc, &out.recipe, &reg).expect("a decoded document encodes");
    let again = pipeline::decode(&bytes, &out.recipe, &reg, &limits).expect("written data decodes");
    assert!(out.doc.tree_eq(&again.doc), "the written data reads back differently");
});
