#![no_main]

use libfuzzer_sys::fuzz_target;
use saveprism_codecs::register_all;
use saveprism_core::pipeline;
use saveprism_core::{Limits, Recipe, Registry, Step};

const SEED: &[u8] = include_bytes!("../../fixtures/synthetic/bioware/character.bic");

fuzz_target!(|data: &[u8]| {
    let mut reg = Registry::default();
    register_all(&mut reg);
    let limits = Limits {
        max_decoded_bytes: 16 * 1024 * 1024,
        ..Limits::default()
    };
    // Mutate a real GFF header and bodies in place so most inputs reach the field reader.
    let mut input = SEED.to_vec();
    let offset = data
        .get(..4)
        .map(|bytes| u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize)
        .unwrap_or(0)
        % input.len();
    let payload = data.get(4..).unwrap_or_default();
    let room = input.len().saturating_sub(offset).min(payload.len());
    if let (Some(target), Some(source)) = (
        input.get_mut(offset..offset.saturating_add(room)),
        payload.get(..room),
    ) {
        target.copy_from_slice(source);
    }
    let recipe = Recipe::new(vec![Step::bare("format.gff")]);
    let Ok(out) = pipeline::decode(&input, &recipe, &reg, &limits) else {
        return;
    };
    let bytes = pipeline::encode(&out.doc, &out.recipe, &reg).expect("a decoded document encodes");
    let again = pipeline::decode(&bytes, &out.recipe, &reg, &limits).expect("written data decodes");
    assert!(out.doc.tree_eq(&again.doc), "the written data reads back differently");
});
