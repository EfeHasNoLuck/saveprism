#![no_main]

use libfuzzer_sys::fuzz_target;
use saveprism_codecs::register_all;
use saveprism_core::pipeline;
use saveprism_core::{Limits, Recipe, Registry, Step};

const MAGIC: &[u8] = b"SQLite format 3\0";

/// Puts a syntactically plausible header in front of the fuzz bytes, so the SQLite reader is
/// exercised instead of rejecting every input at the magic check.
fn with_header(data: &[u8]) -> Vec<u8> {
    let total = data.len() + 100;
    let pages = u32::try_from(total.div_ceil(4096)).unwrap_or(u32::MAX);
    let mut input = vec![0_u8; 100];
    input[..MAGIC.len()].copy_from_slice(MAGIC);
    input[16..18].copy_from_slice(&4096_u16.to_be_bytes());
    input[18] = 1;
    input[19] = 1;
    input[28..32].copy_from_slice(&pages.to_be_bytes());
    input[56] = 1;
    input[92..96].copy_from_slice(&1_u32.to_be_bytes());
    input[96..100].copy_from_slice(&3_045_000_u32.to_be_bytes());
    input.extend_from_slice(data);
    input
}

fuzz_target!(|data: &[u8]| {
    let mut reg = Registry::default();
    register_all(&mut reg);
    let limits = Limits {
        max_decoded_bytes: 16 * 1024 * 1024,
        ..Limits::default()
    };
    let input = if data.starts_with(MAGIC) {
        data.to_vec()
    } else {
        with_header(data)
    };
    let recipe = Recipe::new(vec![Step::bare("format.sqlite")]);
    let Ok(out) = pipeline::decode(&input, &recipe, &reg, &limits) else {
        return;
    };
    let bytes = pipeline::encode(&out.doc, &out.recipe, &reg).expect("a decoded document encodes");
    let again = pipeline::decode(&bytes, &out.recipe, &reg, &limits).expect("written data decodes");
    assert!(out.doc.tree_eq(&again.doc), "the written data reads back differently");
});
