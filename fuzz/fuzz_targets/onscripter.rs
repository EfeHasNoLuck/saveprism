#![no_main]

use libfuzzer_sys::fuzz_target;
use saveprism_codecs::register_all;
use saveprism_core::pipeline;
use saveprism_core::{Limits, Recipe, Registry, Step};

const SAVE_MAGIC: &[u8] = b"ONS";

fuzz_target!(|data: &[u8]| {
    let mut reg = Registry::default();
    register_all(&mut reg);
    let limits = Limits {
        max_decoded_bytes: 16 * 1024 * 1024,
        ..Limits::default()
    };
    // Half the inputs look like a save slot (magic + version), the rest like a global file.
    let mut slot = SAVE_MAGIC.to_vec();
    slot.extend_from_slice(&[2, 8]);
    slot.extend_from_slice(data);
    for input in [slot, data.to_vec()] {
        let recipe = Recipe::new(vec![Step::bare("format.onscripter")]);
        let Ok(out) = pipeline::decode(&input, &recipe, &reg, &limits) else {
            continue;
        };
        let bytes =
            pipeline::encode(&out.doc, &out.recipe, &reg).expect("a decoded document encodes");
        let again =
            pipeline::decode(&bytes, &out.recipe, &reg, &limits).expect("written data decodes");
        assert!(
            out.doc.tree_eq(&again.doc),
            "the written data reads back differently"
        );
    }
});
