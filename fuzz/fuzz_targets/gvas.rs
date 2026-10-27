#![no_main]

use libfuzzer_sys::fuzz_target;
use saveprism_codecs::register_all;
use saveprism_core::{Limits, Recipe, Registry, Step, pipeline};

fn put_string(out: &mut Vec<u8>, text: &str) {
    out.extend_from_slice(&(text.len() as u32 + 1).to_le_bytes());
    out.extend_from_slice(text.as_bytes());
    out.push(0);
}

/// A valid UE 4.27 header and class name, so fuzzed bytes reach the property
/// reader (where the hint search runs).
fn header() -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"GVAS");
    out.extend_from_slice(&2_u32.to_le_bytes());
    out.extend_from_slice(&522_u32.to_le_bytes());
    out.extend_from_slice(&4_u16.to_le_bytes());
    out.extend_from_slice(&27_u16.to_le_bytes());
    out.extend_from_slice(&0_u16.to_le_bytes());
    out.extend_from_slice(&0_u32.to_le_bytes());
    put_string(&mut out, "++UE4+Release-4.27");
    out.extend_from_slice(&3_u32.to_le_bytes());
    out.extend_from_slice(&0_u32.to_le_bytes());
    put_string(&mut out, "/Script/Test.TestSave");
    out
}

fuzz_target!(|data: &[u8]| {
    let mut reg = Registry::default();
    register_all(&mut reg);
    let limits = Limits {
        max_decoded_bytes: 16 * 1024 * 1024,
        ..Limits::default()
    };

    let mut input = Vec::with_capacity(data.len() + 128);
    if data.starts_with(b"GVAS") {
        input.extend_from_slice(data);
    } else {
        input.extend_from_slice(&header());
        input.extend_from_slice(data);
    }

    let recipe = Recipe::new(vec![Step::bare("format.gvas")]);
    let Ok(out) = pipeline::decode(&input, &recipe, &reg, &limits) else {
        return;
    };
    let bytes = pipeline::encode(&out.doc, &out.recipe, &reg).expect("a decoded document encodes");
    let again = pipeline::decode(&bytes, &out.recipe, &reg, &limits).expect("written data decodes");
    assert!(
        out.doc.tree_eq(&again.doc),
        "the written data reads back differently"
    );
});
