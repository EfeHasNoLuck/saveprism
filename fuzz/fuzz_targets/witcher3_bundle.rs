#![no_main]

use libfuzzer_sys::fuzz_target;
use saveprism_codecs::witcher3::bundle::{BundleToc, decode_payload};
use saveprism_codecs::witcher3::cards::parse_cards;

fuzz_target!(|data: &[u8]| {
    let Ok(toc) = BundleToc::parse(data) else {
        return;
    };
    for entry in &toc.entries {
        let start = usize::try_from(entry.offset).unwrap_or(usize::MAX);
        let end = start.saturating_add(entry.compressed_size as usize);
        if let Some(payload) = data.get(start..end)
            && let Ok(bytes) = decode_payload(payload, entry, 8 * 1024 * 1024)
        {
            let _ = parse_cards(&bytes, false);
            let _ = parse_cards(&bytes, true);
        }
    }
});
