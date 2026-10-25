#![no_main]

use libfuzzer_sys::fuzz_target;
use saveprism_codecs::register_all;
use saveprism_core::{DecodeCtx, Limits, Registry};

const SEEDS: [(&str, &[u8]); 3] = [
    (
        "container.psx-card",
        include_bytes!("../../fixtures/synthetic/console/ps1/card.mcr"),
    ),
    (
        "container.ps2-card",
        include_bytes!("../../fixtures/synthetic/console/ps2/card-raw.ps2"),
    ),
    (
        "container.psu",
        include_bytes!("../../fixtures/synthetic/console/ps2/save.psu"),
    ),
];

fuzz_target!(|data: &[u8]| {
    // The first byte selects a container, the next four the offset and the rest the replacement
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
    let Some(layer) = reg.layer(id) else {
        return;
    };
    let limits = Limits {
        max_decoded_bytes: 16 * 1024 * 1024,
        ..Limits::default()
    };
    let mut ctx = DecodeCtx::new(&limits);
    if let Ok((inner, params)) = layer.decode(&input, &mut ctx) {
        let _ = layer.encode(&inner, &params);
    }
});
