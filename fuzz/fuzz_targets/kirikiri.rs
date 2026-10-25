#![no_main]

use libfuzzer_sys::fuzz_target;
use saveprism_codecs::register_all;
use saveprism_core::{DecodeCtx, Limits, Registry};

fuzz_target!(|data: &[u8]| {
    let mut reg = Registry::default();
    register_all(&mut reg);
    let Some(layer) = reg.layer("obfuscation.kirikiri") else {
        return;
    };
    let limits = Limits {
        max_decoded_bytes: 16 * 1024 * 1024,
        ..Limits::default()
    };
    let mut ctx = DecodeCtx::new(&limits);
    if let Ok((inner, params)) = layer.decode(data, &mut ctx) {
        let _ = layer.encode(&inner, &params);
    }
});
