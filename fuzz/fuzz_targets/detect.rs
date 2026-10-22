#![no_main]

use libfuzzer_sys::fuzz_target;
use saveprism_codecs::register_all;
use saveprism_core::detect::{DetectOptions, detect};
use saveprism_core::{Limits, Registry};

fuzz_target!(|data: &[u8]| {
    // Inputs that reach the GVAS codec (magic "GVAS", or "PlZ"/"PlM" at byte 8 for the
    // compressed container) are skipped: the fuzz target is built with panic=abort, so a panic
    // that uesave would catch in a release build (catch_unwind) cannot be observed here.
    if data.starts_with(b"GVAS") || data.get(8..11).is_some_and(|m| m == b"PlZ" || m == b"PlM") {
        return;
    }
    let mut reg = Registry::default();
    register_all(&mut reg);
    let limits = Limits {
        max_decoded_bytes: 16 * 1024 * 1024,
        ..Limits::default()
    };
    // Ok or Err are both fine; the target only looks for panics, hangs and memory blow-ups.
    let _ = detect(data, &reg, &limits, &DetectOptions::default());
});
