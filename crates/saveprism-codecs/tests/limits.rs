//! Input limits: hostile input must end in a typed error, never a panic, a hang or unbounded memory.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::indexing_slicing)]

use proptest::prelude::*;
use saveprism_codecs::register_all;
use saveprism_core::codes;
use saveprism_core::detect::{DetectOptions, detect};
use saveprism_core::pipeline;
use saveprism_core::{ErrorCode, Limits, Params, Recipe, Registry, SniffCtx, Step};

const MIB: usize = 1024 * 1024;

fn registry() -> Registry {
    let mut reg = Registry::default();
    register_all(&mut reg);
    reg
}

/// A recipe made of the given layer ids followed by the JSON codec.
fn recipe(layers: &[&str]) -> Recipe {
    let mut steps: Vec<Step> = layers.iter().map(|id| Step::bare(*id)).collect();
    steps.push(Step::bare("format.json"));
    Recipe::new(steps)
}

/// Wraps `inner` with the layer `id` using default parameters.
fn wrap(reg: &Registry, id: &str, inner: &[u8]) -> Vec<u8> {
    reg.layer(id)
        .expect("layer is registered")
        .encode(inner, &Params::new())
        .expect("layer encodes")
}

fn decode_code(input: &[u8], recipe: &Recipe, limits: &Limits) -> ErrorCode {
    pipeline::decode(input, recipe, &registry(), limits)
        .expect_err("decoding must fail")
        .code()
}

#[test]
fn gzip_bomb_is_stopped() {
    let reg = registry();
    let bomb = wrap(&reg, "compression.gzip", &vec![0_u8; 10 * MIB]);
    let limits = Limits {
        max_decoded_bytes: 1024 * 1024,
        ..Limits::default()
    };
    let code = decode_code(&bomb, &recipe(&["compression.gzip"]), &limits);
    assert_eq!(code, codes::CORE_LIMIT_DECODED);
}

#[test]
fn zlib_bomb_is_stopped() {
    let reg = registry();
    let bomb = wrap(&reg, "compression.zlib", &vec![0_u8; 10 * MIB]);
    let limits = Limits {
        max_decoded_bytes: 1024 * 1024,
        ..Limits::default()
    };
    let code = decode_code(&bomb, &recipe(&["compression.zlib"]), &limits);
    assert_eq!(code, codes::CORE_LIMIT_DECODED);
}

#[test]
fn lz_string_quadratic_bomb_is_stopped() {
    let reg = registry();
    // 256 KiB of one repeated character packs into a tiny LZ-String text.
    let packed = wrap(&reg, "compression.lz-string", &vec![b'a'; 256 * 1024]);
    assert!(packed.len() < 16 * 1024, "the bomb must be small");
    let limits = Limits {
        max_decoded_bytes: 64 * 1024,
        ..Limits::default()
    };
    let code = decode_code(&packed, &recipe(&["compression.lz-string"]), &limits);
    assert_eq!(code, codes::CORE_LIMIT_DECODED);
}

#[test]
fn very_deep_json_hits_the_depth_limit() {
    let input = vec![b'['; 100_000];
    let code = decode_code(&input, &recipe(&[]), &Limits::default());
    assert_eq!(code, codes::CORE_LIMIT_DEPTH);

    let objects: Vec<u8> = b"{\"a\":".repeat(100_000);
    let code = decode_code(&objects, &recipe(&[]), &Limits::default());
    assert_eq!(code, codes::CORE_LIMIT_DEPTH);
}

#[test]
fn input_limit_applies_to_decode_and_detect() {
    let reg = registry();
    let limits = Limits {
        max_input_bytes: 8,
        ..Limits::default()
    };
    let input = b"[1,2,3,4,5,6]";
    assert_eq!(
        decode_code(input, &recipe(&[]), &limits),
        codes::CORE_LIMIT_INPUT
    );

    let error = detect(input, &reg, &limits, &DetectOptions::default())
        .expect_err("detect must fail on an oversized input");
    assert_eq!(error.code(), codes::CORE_LIMIT_INPUT);
}

#[test]
fn node_limit_applies_to_wide_json() {
    let mut input = String::from("[");
    for index in 0..1000 {
        if index > 0 {
            input.push(',');
        }
        input.push('0');
    }
    input.push(']');
    let limits = Limits {
        max_nodes: 100,
        ..Limits::default()
    };
    let code = decode_code(input.as_bytes(), &recipe(&[]), &limits);
    assert_eq!(code, codes::CORE_LIMIT_NODES);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// No layer, codec or detection panics on arbitrary bytes (Ok or Err are both fine).
    #[test]
    fn arbitrary_bytes_never_panic(data in proptest::collection::vec(any::<u8>(), 0..=4096)) {
        let reg = registry();
        let limits = Limits::default();

        for layer in reg.layers() {
            let _ = layer.sniff(&data, &SniffCtx::default());
            let mut ctx = saveprism_core::DecodeCtx::new(&limits);
            if let Ok((inner, params)) = layer.decode(&data, &mut ctx) {
                let _ = layer.encode(&inner, &params);
            }
        }

        for codec in reg.codecs() {
            let _ = codec.sniff(&data, &SniffCtx::default());
            let mut ctx = saveprism_core::DecodeCtx::new(&limits);
            if let Ok(decoded) = codec.decode(&data, &mut ctx) {
                let _ = codec.encode(&decoded.doc, &decoded.params);
            }
        }

        let _ = detect(&data, &reg, &limits, &DetectOptions::default());
    }
}
