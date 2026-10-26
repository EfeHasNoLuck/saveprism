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
    let recipe = Recipe::new(vec![Step::bare("format.es3")]);
    let Ok(out) = pipeline::decode(data, &recipe, &reg, &limits) else {
        return;
    };
    // A decoded document must be writable again and the written bytes must decode too.
    if let Ok(bytes) = pipeline::encode(&out.doc, &out.recipe, &reg) {
        let _ = pipeline::decode(&bytes, &out.recipe, &reg, &limits);
    }
});
