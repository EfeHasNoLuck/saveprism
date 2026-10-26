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
    for id in ["format.snes9x-snapshot", "format.retroarch-state"] {
        let recipe = Recipe::new(vec![Step::bare(id)]);
        let Ok(out) = pipeline::decode(data, &recipe, &reg, &limits) else {
            continue;
        };
        let bytes =
            pipeline::encode(&out.doc, &out.recipe, &reg).expect("a decoded document encodes");
        assert_eq!(bytes, data, "the written data differs from the input");
    }
});
