#![no_main]

use libfuzzer_sys::fuzz_target;
use saveprism_codecs::register_all;
use saveprism_core::{DecodeCtx, Limits, Registry};

const LAYERS: [&str; 4] = [
    "encoding.base64",
    "compression.gzip",
    "compression.zlib",
    "compression.lz-string",
];

fuzz_target!(|data: &[u8]| {
    // The first byte picks the layer, the rest is the layer input.
    let Some((first, rest)) = data.split_first() else {
        return;
    };
    let mut reg = Registry::default();
    register_all(&mut reg);
    let id = LAYERS[usize::from(*first) % LAYERS.len()];
    let Some(layer) = reg.layer(id) else {
        return;
    };
    let limits = Limits {
        max_decoded_bytes: 16 * 1024 * 1024,
        ..Limits::default()
    };
    let mut ctx = DecodeCtx::new(&limits);
    if let Ok((inner, params)) = layer.decode(rest, &mut ctx) {
        let _ = layer.encode(&inner, &params);
    }
});
