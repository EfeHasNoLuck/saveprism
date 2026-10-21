#![no_main]

use libfuzzer_sys::fuzz_target;
use saveprism_codecs::register_all;
use saveprism_core::detect::{DetectOptions, detect};
use saveprism_core::{Limits, Registry};

fuzz_target!(|data: &[u8]| {
    let mut reg = Registry::default();
    register_all(&mut reg);
    let limits = Limits {
        max_decoded_bytes: 16 * 1024 * 1024,
        ..Limits::default()
    };
    // Ok or Err are both fine; the target only looks for panics, hangs and memory blow-ups.
    let _ = detect(data, &reg, &limits, &DetectOptions::default());
});
