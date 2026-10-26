#![no_main]

use libfuzzer_sys::fuzz_target;
use saveprism_codecs::register_all;
use saveprism_core::pipeline;
use saveprism_core::{Limits, Recipe, Registry, Step};

const SEED: &[u8] = include_bytes!("../../fixtures/synthetic/bioware/dao-save.das");

fuzz_target!(|data: &[u8]| {
    let mut reg = Registry::default();
    register_all(&mut reg);
    let limits = Limits {
        max_decoded_bytes: 16 * 1024 * 1024,
        ..Limits::default()
    };
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
    let recipe = Recipe::new(vec![Step::bare("format.gff4")]);
    let Ok(out) = pipeline::decode(&input, &recipe, &reg, &limits) else {
        return;
    };
    // The GFF4 writer is read-only and must return the decoded bytes exactly.
    let bytes = pipeline::encode(&out.doc, &out.recipe, &reg).expect("read-only write succeeds");
    assert_eq!(bytes, input, "the read-only writer changed the file");
});
