//! Tests of the read-only bundle reader.

use std::io::Cursor;
use std::path::PathBuf;

use proptest::prelude::*;

use super::bundle::{BundleToc, Compression, decode_payload, read_entry};
use super::cards::parse_cards;
use super::{W3_BUNDLE, W3_PARSE};

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/synthetic/witcher3-bundle")
}

fn toc_named(name: &str) -> BundleToc {
    let path = fixtures().join("standalone").join(format!("{name}.bundle"));
    let bytes = std::fs::read(path).expect("a synthetic bundle exists");
    let mut cursor = Cursor::new(bytes);
    BundleToc::read(&mut cursor).expect("the synthetic bundle parses")
}

fn entry_bytes(name: &str, suffix: &str) -> Vec<u8> {
    let path = fixtures().join("standalone").join(format!("{name}.bundle"));
    let bytes = std::fs::read(path).expect("a synthetic bundle exists");
    let mut cursor = Cursor::new(bytes);
    let toc = BundleToc::read(&mut cursor).expect("the synthetic bundle parses");
    let entry = toc.find_end(suffix).expect("the entry exists");
    read_entry(&mut cursor, entry, 1024 * 1024).expect("the entry decompresses")
}

#[test]
fn every_version_and_compression_reads_the_same_invented_cards() {
    for (name, version, compression) in [
        ("v3-none", 3, Compression::None),
        ("v5-none", 5, Compression::None),
        ("v3-zlib", 3, Compression::Zlib),
        ("v5-lz4", 5, Compression::Lz4),
        ("v5-snappy", 5, Compression::Snappy),
    ] {
        let toc = toc_named(name);
        assert_eq!(toc.version, version, "{name}");
        let regular = toc
            .find_end("gameplay/items/def_gwint_cards_final.xml")
            .expect("the regular card file exists");
        assert_eq!(regular.compression, compression, "{name}");
        let xml = entry_bytes(name, "gameplay/items/def_gwint_cards_final.xml");
        let cards = parse_cards(&xml, false).expect("the invented XML parses");
        assert_eq!(cards.len(), 3, "{name}");
        let first = cards.first().expect("a first card");
        assert_eq!(first.index, 1);
        assert_eq!(first.name, "Alpha Test");
        assert_eq!(first.faction, "F_NEUTRAL");
        assert!(!first.leader);

        let leaders = entry_bytes(name, "gameplay/items/def_gwint_battle_king_cards.xml");
        let leaders = parse_cards(&leaders, true).expect("the invented leader XML parses");
        assert_eq!(leaders.len(), 2, "{name}");
        let first = leaders.first().expect("a first leader");
        assert!(first.leader);
        assert_eq!(first.name, "Leader One");
    }
}

#[test]
fn legacy_offsets_are_absolute_and_names_keep_their_separators() {
    let toc = toc_named("v3-none");
    let entry = toc
        .find_end("def_gwint_battle_king_cards.xml")
        .expect("the leader entry exists");
    assert!(entry.name.contains('\\'));
    assert!(entry.offset >= 0x20);
    assert_eq!(entry.compression, Compression::None);
}

#[test]
fn a_doboz_payload_is_refused_with_a_clear_error() {
    let path = fixtures().join("standalone").join("v3-none.bundle");
    let mut bytes = std::fs::read(path).expect("a synthetic bundle exists");
    // The compression field of the first legacy entry is at 0x20 + 0x13c.
    let field = bytes
        .get_mut(0x20 + 0x13c..0x20 + 0x140)
        .expect("the compression field is in the buffer");
    field.copy_from_slice(&3_u32.to_le_bytes());
    let mut cursor = Cursor::new(bytes);
    let toc = BundleToc::read(&mut cursor).expect("a mutated bundle still parses");
    let entry = toc.entries.first().expect("a first entry");
    let error = read_entry(&mut cursor, entry, 1024).expect_err("Doboz is refused");
    assert_eq!(error.code(), W3_BUNDLE);
    assert!(error.message().contains("Doboz"));
}

#[test]
fn an_unsupported_version_is_refused() {
    let path = fixtures().join("standalone").join("v3-none.bundle");
    let mut bytes = std::fs::read(path).expect("a synthetic bundle exists");
    let field = bytes.get_mut(20..22).expect("the version field");
    field.copy_from_slice(&4_u16.to_le_bytes());
    let error = BundleToc::parse(&bytes).expect_err("version 4 is not supported");
    assert_eq!(error.code(), W3_BUNDLE);
    assert!(error.message().contains("version 4"));
}

#[test]
fn a_truncated_payload_range_is_refused() {
    let path = fixtures().join("standalone").join("v3-none.bundle");
    let mut bytes = std::fs::read(path).expect("a synthetic bundle exists");
    let end = u32::try_from(bytes.len().saturating_sub(1)).expect("a small fixture");
    let field = bytes
        .get_mut(0x20 + 0x11c..0x20 + 0x120)
        .expect("the offset field");
    field.copy_from_slice(&end.to_le_bytes());
    let error = BundleToc::parse(&bytes).expect_err("an out-of-file payload is refused");
    assert_eq!(error.code(), W3_PARSE);
}

#[test]
fn the_output_budget_is_honoured() {
    let path = fixtures().join("standalone").join("v5-none.bundle");
    let bytes = std::fs::read(path).expect("a synthetic bundle exists");
    let mut cursor = Cursor::new(bytes);
    let toc = BundleToc::read(&mut cursor).expect("the synthetic bundle parses");
    let entry = toc.entries.first().expect("a first entry");
    let error = read_entry(&mut cursor, entry, 1).expect_err("one byte is too small");
    assert_eq!(error.code(), W3_BUNDLE);
    assert!(error.message().contains("budget"));
}

#[test]
fn a_payload_that_lies_about_its_size_is_refused() {
    let path = fixtures().join("standalone").join("v5-none.bundle");
    let mut bytes = std::fs::read(path).expect("a synthetic bundle exists");
    let field = bytes
        .get_mut(0x20 + 0x118..0x20 + 0x11c)
        .expect("the size field");
    field.copy_from_slice(&7_u32.to_le_bytes());
    let mut cursor = Cursor::new(bytes);
    let toc = BundleToc::read(&mut cursor).expect("the mutated bundle parses");
    let entry = toc.entries.first().expect("a first entry");
    let error = read_entry(&mut cursor, entry, 1024).expect_err("a wrong size is refused");
    assert_eq!(error.code(), W3_PARSE);
}

#[test]
fn an_empty_payload_decodes_to_an_empty_vec() {
    // A tiny in-memory v5 bundle whose entry is empty and stored without compression.
    let entry = super::bundle::BundleEntry {
        name: "empty.xml".to_string(),
        offset: 0,
        size: 0,
        compressed_size: 0,
        crc: 0,
        compression: Compression::None,
    };
    let out = decode_payload(&[], &entry, 0).expect("an empty payload is valid");
    assert!(out.is_empty());
}

proptest! {
    #[test]
    fn a_random_bundle_never_panics(data in proptest::collection::vec(any::<u8>(), 0..4096)) {
        if let Ok(toc) = BundleToc::parse(&data) {
            for entry in &toc.entries {
                let start = usize::try_from(entry.offset).unwrap_or(usize::MAX);
                let end = start.saturating_add(entry.compressed_size as usize);
                if let Some(payload) = data.get(start..end) {
                    let _ = decode_payload(payload, entry, 64 * 1024);
                }
            }
        }
    }
}
