#![allow(
    clippy::indexing_slicing,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::chunks_exact_to_as_chunks
)]

//! Writes the synthetic archive-container fixtures under `fixtures/synthetic/archives/`.
//!
//! `cargo run -p saveprism-codecs --example make_archive_fixtures`
//!
//! The data is invented. BND4, MPQ, BRZ and BRS are written from the public layouts documented in
//! the matching modules; the test files exercise raw and compressed entries, encrypted MPQ tables
//! and the Brickadia index/blob hashes.

use std::fs;
use std::io::Write;
use std::path::PathBuf;

const JSON: &[u8] = br#"{"game":"synthetic","tag":"archives","gold":1234,"name":"Test Player","items":["potion",1,2,3]}"#;

fn out_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/synthetic/archives")
}

fn reverse_bits(value: u8) -> u8 {
    let mut out = 0_u8;
    for bit in 0..8 {
        if value & (1 << bit) != 0 {
            out |= 1 << (7 - bit);
        }
    }
    out
}

// ---------------------------------------------------------------- BND4

fn bnd4() -> Vec<u8> {
    const FORMAT: u8 = 0x02 | 0x04 | 0x08 | 0x20; // IDs | Names1 | Names2 | Compression
    const FILE_HEADER: usize = 0x10 + 4 + 8 + 4 + 4;
    let files: [(&str, &[u8]); 2] = [
        ("user_data.json", JSON),
        ("readme.txt", b"invented BND4 entry\n"),
    ];
    let name_table = 0x40 + files.len() * FILE_HEADER;
    let mut names = Vec::new();
    let mut name_offsets = Vec::new();
    for (name, _) in &files {
        name_offsets.push(name_table + names.len());
        for unit in name.encode_utf16() {
            names.extend_from_slice(&unit.to_le_bytes());
        }
        names.extend_from_slice(&[0, 0]);
    }
    while !names.len().is_multiple_of(0x10) {
        names.push(0);
    }
    let headers_end = name_table + names.len();
    let mut offsets = Vec::new();
    let mut data = Vec::new();
    for (_, bytes) in &files {
        while !(headers_end + data.len()).is_multiple_of(0x10) {
            data.push(0);
        }
        offsets.push(headers_end + data.len());
        data.extend_from_slice(bytes);
    }

    let mut out = Vec::new();
    out.extend_from_slice(b"BND4");
    out.extend_from_slice(&[0, 0, 0, 0]);
    out.extend_from_slice(&[0, 0, 1, 0]);
    out.extend_from_slice(&(files.len() as i32).to_le_bytes());
    out.extend_from_slice(&0x40_i64.to_le_bytes());
    out.extend_from_slice(b"20240101");
    out.extend_from_slice(&(FILE_HEADER as i64).to_le_bytes());
    out.extend_from_slice(&(headers_end as i64).to_le_bytes());
    out.push(1); // unicode
    out.push(reverse_bits(FORMAT));
    out.push(0); // extended
    out.push(0);
    out.extend_from_slice(&0_i32.to_le_bytes());
    out.extend_from_slice(&0_i64.to_le_bytes());
    assert_eq!(out.len(), 0x40);
    for (index, (_, bytes)) in files.iter().enumerate() {
        out.push(reverse_bits(0)); // flags
        out.extend_from_slice(&[0, 0, 0]);
        out.extend_from_slice(&(-1_i32).to_le_bytes());
        out.extend_from_slice(&(bytes.len() as i64).to_le_bytes());
        out.extend_from_slice(&(bytes.len() as i64).to_le_bytes());
        out.extend_from_slice(&(offsets[index] as u32).to_le_bytes());
        out.extend_from_slice(&(index as i32).to_le_bytes());
        out.extend_from_slice(&(name_offsets[index] as u32).to_le_bytes());
    }
    out.extend_from_slice(&names);
    out.extend_from_slice(&data);
    out
}

// ---------------------------------------------------------------- MPQ

fn storm_table() -> [u32; 0x500] {
    let mut table = [0_u32; 0x500];
    let mut seed = 0x0010_0001_u32;
    for index1 in 0..0x100_usize {
        let mut index2 = index1;
        for _ in 0..5 {
            seed = seed.wrapping_mul(125).wrapping_add(3) % 0x2A_AAAB;
            let high = (seed & 0xFFFF) << 16;
            seed = seed.wrapping_mul(125).wrapping_add(3) % 0x2A_AAAB;
            table[index2] = high | (seed & 0xFFFF);
            index2 += 0x100;
        }
    }
    table
}

fn upper(byte: u8) -> u8 {
    match byte {
        b'a'..=b'z' => byte - 32,
        b'/' => b'\\',
        other => other,
    }
}

fn hash_string(name: &[u8], kind: u32) -> u32 {
    let table = storm_table();
    let mut seed1 = 0x7FED_7FED_u32;
    let mut seed2 = 0xEEEE_EEEE_u32;
    for &byte in name {
        let ch = u32::from(upper(byte));
        seed1 = table[(kind + ch) as usize] ^ seed1.wrapping_add(seed2);
        seed2 = ch
            .wrapping_add(seed1)
            .wrapping_add(seed2)
            .wrapping_add(seed2 << 5)
            .wrapping_add(3);
    }
    seed1
}

fn encrypt(data: &mut [u8], key: u32) {
    let table = storm_table();
    let mut key1 = key;
    let mut key2 = 0xEEEE_EEEE_u32;
    for chunk in data.chunks_exact_mut(4) {
        key2 = key2.wrapping_add(table[0x400 + (key1 & 0xFF) as usize]);
        let raw = <[u8; 4]>::try_from(&*chunk).expect("four bytes");
        let plain = u32::from_le_bytes(raw);
        chunk.copy_from_slice(&(plain ^ key1.wrapping_add(key2)).to_le_bytes());
        key1 = ((!key1 << 0x15).wrapping_add(0x1111_1111)) | (key1 >> 0x0B);
        key2 = plain
            .wrapping_add(key2)
            .wrapping_add(key2 << 5)
            .wrapping_add(3);
    }
}

fn zlib(data: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
    encoder.write_all(data).expect("zlib");
    encoder.finish().expect("zlib")
}

fn mpq() -> Vec<u8> {
    const HASH_COUNT: usize = 16;
    let listfile = b"save.json\r\nreadme.txt\r\n";
    let compressed = zlib(listfile);
    let files: [(&str, Vec<u8>, u32, usize); 3] = [
        (
            "(listfile)",
            [vec![0x02], compressed.clone()].concat(),
            0x8100_0200,
            listfile.len(),
        ),
        ("save.json", JSON.to_vec(), 0x8000_0000, JSON.len()),
        (
            "readme.txt",
            b"invented MPQ entry\n".to_vec(),
            0x8000_0000,
            22,
        ),
    ];
    let header_len = 0x20usize;
    let mut data = Vec::new();
    let mut blocks = Vec::new();
    for (_, bytes, _, _) in &files {
        blocks.push((header_len + data.len(), bytes.len()));
        data.extend_from_slice(bytes);
    }
    let hash_start = header_len + data.len();
    let block_start = hash_start + HASH_COUNT * 16;
    let total = block_start + files.len() * 16;

    let mut hash = vec![0xFF_u8; HASH_COUNT * 16];
    for (index, (name, _, flags, size)) in files.iter().enumerate() {
        let mut slot = (hash_string(name.as_bytes(), 0) as usize) % HASH_COUNT;
        while hash.get(slot * 16 + 12) != Some(&0xFF) {
            slot = (slot + 1) % HASH_COUNT;
        }
        let at = slot * 16;
        hash[at..at + 4].copy_from_slice(&hash_string(name.as_bytes(), 0x100).to_le_bytes());
        hash[at + 4..at + 8].copy_from_slice(&hash_string(name.as_bytes(), 0x200).to_le_bytes());
        hash[at + 8..at + 10].copy_from_slice(&0_u16.to_le_bytes());
        hash[at + 10] = 0;
        hash[at + 11] = 0;
        hash[at + 12..at + 16].copy_from_slice(&(index as u32).to_le_bytes());
        let _ = (flags, size);
    }
    let mut block_table = Vec::new();
    for (index, (_, bytes, flags, size)) in files.iter().enumerate() {
        let (pos, _) = blocks[index];
        block_table.extend_from_slice(&(pos as u32).to_le_bytes());
        block_table.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        block_table.extend_from_slice(&(*size as u32).to_le_bytes());
        block_table.extend_from_slice(&flags.to_le_bytes());
    }
    encrypt(&mut hash, 0xC3AF_3770);
    encrypt(&mut block_table, 0xEC83_B3A3);

    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(b"MPQ\x1A");
    out.extend_from_slice(&0x20_u32.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&0_u16.to_le_bytes());
    out.extend_from_slice(&3_u16.to_le_bytes());
    out.extend_from_slice(&(hash_start as u32).to_le_bytes());
    out.extend_from_slice(&(block_start as u32).to_le_bytes());
    out.extend_from_slice(&(HASH_COUNT as u32).to_le_bytes());
    out.extend_from_slice(&(files.len() as u32).to_le_bytes());
    assert_eq!(out.len(), header_len);
    out.extend_from_slice(&data);
    out.extend_from_slice(&hash);
    out.extend_from_slice(&block_table);
    out
}

// ---------------------------------------------------------------- BRZ

fn brz() -> Vec<u8> {
    let files: [(&str, Vec<u8>, u8); 3] = [
        ("world.json", JSON.to_vec(), 0),
        ("readme.txt", b"invented BRZ file\n".to_vec(), 0),
        (
            "packed.bin",
            zstd::stream::encode_all(std::io::Cursor::new(b"packed data".as_slice()), 3)
                .expect("zstd"),
            1,
        ),
    ];
    let raw_sizes = [
        JSON.len(),
        b"invented BRZ file\n".len(),
        b"packed data".len(),
    ];
    let mut index = Vec::new();
    index.extend_from_slice(&0_i32.to_le_bytes());
    index.extend_from_slice(&(files.len() as i32).to_le_bytes());
    index.extend_from_slice(&(files.len() as i32).to_le_bytes());
    for _ in 0..files.len() {
        index.extend_from_slice(&(-1_i32).to_le_bytes());
    }
    for index_in_files in 0..files.len() {
        index.extend_from_slice(&(index_in_files as i32).to_le_bytes());
    }
    for (name, _, _) in &files {
        index.extend_from_slice(&(name.len() as u16).to_le_bytes());
    }
    for (name, _, _) in &files {
        index.extend_from_slice(name.as_bytes());
    }
    for (_, _, method) in &files {
        index.push(*method);
    }
    for size in raw_sizes {
        index.extend_from_slice(&(size as i32).to_le_bytes());
    }
    for (_, bytes, _) in &files {
        index.extend_from_slice(&(bytes.len() as i32).to_le_bytes());
    }
    for (name, _, method) in &files {
        let hash = if *method == 0 {
            blake3::hash(name.as_bytes())
        } else {
            blake3::hash(b"packed data")
        };
        index.extend_from_slice(hash.as_bytes());
    }
    // The first two blobs are raw; replace their hashes with the content hashes.
    let hash_base = index.len() - files.len() * 32;
    for (index_in_files, (_, bytes, method)) in files.iter().enumerate() {
        if *method != 0 {
            continue;
        }
        let at = hash_base + index_in_files * 32;
        index[at..at + 32].copy_from_slice(blake3::hash(bytes).as_bytes());
    }

    let mut out = Vec::new();
    out.extend_from_slice(b"BRZ");
    out.push(0);
    out.push(0);
    out.extend_from_slice(&(index.len() as i32).to_le_bytes());
    out.extend_from_slice(&(index.len() as i32).to_le_bytes());
    out.extend_from_slice(blake3::hash(&index).as_bytes());
    out.extend_from_slice(&index);
    for (_, bytes, _) in &files {
        out.extend_from_slice(bytes);
    }
    out
}

// ---------------------------------------------------------------- BRS

fn brs_section(inner: &[u8]) -> Vec<u8> {
    let compressed = zlib(inner);
    let mut out = Vec::new();
    out.extend_from_slice(&(inner.len() as i32).to_le_bytes());
    if compressed.len() < inner.len() {
        out.extend_from_slice(&(compressed.len() as i32).to_le_bytes());
        out.extend_from_slice(&compressed);
    } else {
        out.extend_from_slice(&0_i32.to_le_bytes());
        out.extend_from_slice(inner);
    }
    out
}

fn write_string(out: &mut Vec<u8>, text: &str) {
    out.extend_from_slice(&(text.len() as i32).to_le_bytes());
    out.extend_from_slice(text.as_bytes());
}

fn brs() -> Vec<u8> {
    let mut header1 = Vec::new();
    write_string(&mut header1, "Synthetic Park");
    write_string(&mut header1, "Test Builder");
    write_string(&mut header1, "An invented Brickadia save.");
    header1.extend(0..16_u8);
    header1.extend_from_slice(&123_456_789_i64.to_le_bytes());
    header1.extend_from_slice(&0_i32.to_le_bytes());

    let mut header2 = Vec::new();
    header2.extend_from_slice(&0_i32.to_le_bytes()); // mods
    header2.extend_from_slice(&1_i32.to_le_bytes()); // brick assets
    write_string(&mut header2, "PB_DefaultBrick");
    header2.extend_from_slice(&1_i32.to_le_bytes()); // colors
    header2.extend_from_slice(&0xFFFF_00FF_u32.to_le_bytes());
    header2.extend_from_slice(&1_i32.to_le_bytes()); // materials
    write_string(&mut header2, "BMC_Plastic");
    header2.extend_from_slice(&0_i32.to_le_bytes()); // owners

    let mut out = Vec::new();
    out.extend_from_slice(b"BRS");
    out.extend_from_slice(&4_u16.to_le_bytes());
    out.extend_from_slice(&brs_section(&header1));
    out.extend_from_slice(&brs_section(&header2));
    out.extend_from_slice(&brs_section(&[]));
    out
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = out_dir();
    for (relative, bytes) in [
        ("bnd4/save.sl2", bnd4()),
        ("mpq/save.SC2Save", mpq()),
        ("brz/world.brz", brz()),
        ("brs/save.brs", brs()),
    ] {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, bytes)?;
        println!("wrote {}", path.display());
    }
    Ok(())
}
