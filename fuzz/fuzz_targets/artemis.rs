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
    for magic in [b"BOWS", b"BOWG"] {
        let mut input = magic.to_vec();
        input.extend_from_slice(data);
        let recipe = Recipe::new(vec![Step::bare("format.artemis")]);
        let Ok(out) = pipeline::decode(&input, &recipe, &reg, &limits) else {
            continue;
        };
        let bytes =
            pipeline::encode(&out.doc, &out.recipe, &reg).expect("a decoded document encodes");
        assert_eq!(bytes, input, "the read-only envelope writes back exactly");
    }
});
