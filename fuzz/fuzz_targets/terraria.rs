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
    for kind in ["format.terraria-player", "format.terraria-world"] {
        let recipe = Recipe::new(vec![Step::bare(kind)]);
        let Ok(out) = pipeline::decode(data, &recipe, &reg, &limits) else {
            continue;
        };
        // Nothing is edited here. The writer may still refuse a decoded file (a player name over
        // the game's 20 characters, a section pointer that no longer fits): that is an answer, not
        // a crash. Bytes are not compared either: a decodable file can be non-canonical (a flag
        // byte other than 0 or 1, a padded length prefix) and is then written in canonical form.
        let Ok(bytes) = pipeline::encode(&out.doc, &out.recipe, &reg) else {
            return;
        };
        // What is written must read back as the same tree.
        let again =
            pipeline::decode(&bytes, &out.recipe, &reg, &limits).expect("written data decodes");
        assert!(
            out.doc.tree_eq(&again.doc),
            "the written data reads back differently"
        );
        return;
    }
});
