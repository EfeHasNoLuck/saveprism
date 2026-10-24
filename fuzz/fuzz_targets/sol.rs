#![no_main]

use libfuzzer_sys::fuzz_target;
use saveprism_codecs::register_all;
use saveprism_core::{Codec, Limits, Registry};

fuzz_target!(|data: &[u8]| {
    let mut registry = Registry::default();
    register_all(&mut registry);
    let limits = Limits { max_input_bytes: 16 * 1024 * 1024, max_decoded_bytes: 16 * 1024 * 1024, ..Limits::default() };
    let _ = saveprism_codecs::sol::FlashSolCodec;
    let mut ctx = saveprism_core::DecodeCtx::new(&limits);
    if let Ok(decoded) = saveprism_codecs::sol::FlashSolCodec.decode(data, &mut ctx) {
        let output = saveprism_codecs::sol::FlashSolCodec.encode(&decoded.doc, &decoded.params).expect("decoded LSO re-encodes");
        let mut ctx = saveprism_core::DecodeCtx::new(&limits);
        let again = saveprism_codecs::sol::FlashSolCodec.decode(&output, &mut ctx).expect("written LSO decodes");
        assert!(decoded.doc.tree_eq(&again.doc));
    }
});
