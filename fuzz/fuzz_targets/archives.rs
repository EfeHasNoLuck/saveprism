#![no_main]

use libfuzzer_sys::fuzz_target;
use saveprism_codecs::register_all;
use saveprism_core::{DecodeCtx, Limits, Registry};

const SEEDS: [(&str, &[u8]); 5] = [
    (
        "container.bnd4",
        include_bytes!("../../fixtures/synthetic/archives/bnd4/save.sl2"),
    ),
    (
        "container.mpq",
        include_bytes!("../../fixtures/synthetic/archives/mpq/save.SC2Save"),
    ),
    (
        "container.brz",
        include_bytes!("../../fixtures/synthetic/archives/brz/world.brz"),
    ),
    (
        "format.brickadia-brs",
        include_bytes!("../../fixtures/synthetic/archives/brs/save.brs"),
    ),
    (
        "container.erf",
        include_bytes!("../../fixtures/synthetic/bioware/kotor-save.sav"),
    ),
];

fuzz_target!(|data: &[u8]| {
    // The first byte selects an archive, the next four the offset and the rest the replacement
    // bytes. A real seed is mutated in place so the parsers see valid headers most of the time.
    let Some((first, rest)) = data.split_first() else {
        return;
    };
    let (id, seed) = SEEDS[usize::from(*first) % SEEDS.len()];
    let mut input = seed.to_vec();
    if rest.len() < 4 {
        return;
    }
    let offset = u32::from_le_bytes([rest[0], rest[1], rest[2], rest[3]]) as usize % input.len();
    let payload = rest.get(4..).unwrap_or_default();
    let room = input.len().saturating_sub(offset).min(payload.len());
    if let (Some(target), Some(source)) = (
        input.get_mut(offset..offset.saturating_add(room)),
        payload.get(..room),
    ) {
        target.copy_from_slice(source);
    }
    let mut reg = Registry::default();
    register_all(&mut reg);
    let limits = Limits {
        max_decoded_bytes: 16 * 1024 * 1024,
        ..Limits::default()
    };
    if let Some(layer) = reg.layer(id) {
        let mut ctx = DecodeCtx::new(&limits);
        if let Ok((inner, params)) = layer.decode(&input, &mut ctx) {
            let _ = layer.encode(&inner, &params);
        }
    } else if let Some(codec) = reg.codec(id) {
        let mut ctx = DecodeCtx::new(&limits);
        if let Ok(decoded) = codec.decode(&input, &mut ctx) {
            let _ = codec.encode(&decoded.doc, &decoded.params);
        }
    }
});
