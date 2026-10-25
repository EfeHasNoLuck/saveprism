#![no_main]

use libfuzzer_sys::fuzz_target;
use saveprism_codecs::register_all;
use saveprism_core::{pipeline, Limits, Recipe, Registry, Step};

const FORMATS: [&str; 7] = [
    "format.xml",
    "format.ini",
    "format.properties",
    "format.toml",
    "format.yaml",
    "format.csv",
    "format.plist",
];

fuzz_target!(|data: &[u8]| {
    let mut registry = Registry::default();
    register_all(&mut registry);
    let limits = Limits {
        max_input_bytes: 2 * 1024 * 1024,
        max_decoded_bytes: 4 * 1024 * 1024,
        ..Limits::default()
    };
    for format in FORMATS {
        let recipe = Recipe::new(vec![Step::bare(format)]);
        if let Ok(decoded) = pipeline::decode(data, &recipe, &registry, &limits) {
            let encoded = pipeline::encode(&decoded.doc, &decoded.recipe, &registry)
                .expect("decoded structured text re-encodes");
            let again = pipeline::decode(&encoded, &decoded.recipe, &registry, &limits)
                .expect("encoded structured text decodes");
            assert!(decoded.doc.tree_eq(&again.doc));
        }
    }
});
