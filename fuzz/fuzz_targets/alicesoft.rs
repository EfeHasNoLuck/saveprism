#![no_main]

use libfuzzer_sys::fuzz_target;
use saveprism_codecs::register_all;
use saveprism_core::pipeline;
use saveprism_core::{Limits, Recipe, Registry, Step};

const SYS3_MAGIC: &[u8] = b"System3.8 SaveData(c)ALICE-SOFT";
const SYS4_MAGIC: &[u8] = b"GD\x01\x01";

fuzz_target!(|data: &[u8]| {
    let mut reg = Registry::default();
    register_all(&mut reg);
    let limits = Limits {
        max_decoded_bytes: 16 * 1024 * 1024,
        ..Limits::default()
    };
    // Try the raw input, a System 3.x header prefix and a System 4 envelope prefix.
    let mut system3 = SYS3_MAGIC.to_vec();
    system3.extend_from_slice(data);
    let mut system4 = SYS4_MAGIC.to_vec();
    system4.extend_from_slice(data);
    for input in [data.to_vec(), system3, system4] {
        let recipe = Recipe::new(vec![Step::bare("format.alicesoft")]);
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
