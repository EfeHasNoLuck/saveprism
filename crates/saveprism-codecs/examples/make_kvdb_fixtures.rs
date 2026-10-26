//! Writes the synthetic LevelDB, RocksDB and LiteDB fixtures under `fixtures/synthetic/`.
//!
//! `cargo run -p saveprism-codecs --example make_kvdb_fixtures`
//!
//! Everything is invented and built byte by byte from the public layouts, so the committed files are
//! an independent check of the readers.

use std::fs;
use std::io::Write;
use std::path::PathBuf;

const LEVELDB_MAGIC: u64 = 0xdb47_7524_8b80_fb57;
const ROCKSDB_MAGIC: u64 = 0x88e2_41b7_85f4_cff7;
const NONE: u32 = u32::MAX;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/synthetic")
}

fn put_u8(out: &mut [u8], at: usize, value: u8) {
    if let Some(byte) = out.get_mut(at) {
        *byte = value;
    }
}

fn put_u16(out: &mut [u8], at: usize, value: u16) {
    if let Some(target) = out.get_mut(at..at.saturating_add(2)) {
        target.copy_from_slice(&value.to_le_bytes());
    }
}

fn put_u32(out: &mut [u8], at: usize, value: u32) {
    if let Some(target) = out.get_mut(at..at.saturating_add(4)) {
        target.copy_from_slice(&value.to_le_bytes());
    }
}

fn put_bytes(out: &mut [u8], at: usize, bytes: &[u8]) {
    if let Some(target) = out.get_mut(at..at.saturating_add(bytes.len())) {
        target.copy_from_slice(bytes);
    }
}

fn varint(mut value: u64) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let mut byte = u8::try_from(value & 0x7f).unwrap_or(0);
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            return out;
        }
    }
}

fn crc32c(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffff_u32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0x82f6_3b78_u32
            } else {
                crc >> 1
            };
        }
    }
    crc ^ 0xffff_ffff
}

fn masked_crc(crc: u32) -> u32 {
    crc.rotate_right(15).wrapping_add(0xa282_ead8)
}

fn internal_key(user: &[u8], sequence: u64, kind: u8) -> Vec<u8> {
    let mut key = user.to_vec();
    key.extend_from_slice(&((sequence << 8) | u64::from(kind)).to_le_bytes());
    key
}

/// A prefix-compressed block with restart points (LevelDB `block_builder.cc`).
fn block(entries: &[(Vec<u8>, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut restarts = vec![0_u32];
    let mut previous: Vec<u8> = Vec::new();
    for (index, (key, value)) in entries.iter().enumerate() {
        if index > 0 && index % 16 == 0 {
            restarts.push(u32::try_from(out.len()).unwrap_or(0));
        }
        let shared = if index % 16 == 0 {
            0
        } else {
            previous
                .iter()
                .zip(key.iter())
                .take_while(|(left, right)| left == right)
                .count()
        };
        let unshared = key.get(shared..).unwrap_or_default();
        out.extend_from_slice(&varint(u64::try_from(shared).unwrap_or(0)));
        out.extend_from_slice(&varint(u64::try_from(unshared.len()).unwrap_or(0)));
        out.extend_from_slice(&varint(u64::try_from(value.len()).unwrap_or(0)));
        out.extend_from_slice(unshared);
        out.extend_from_slice(value);
        previous = key.clone();
    }
    for restart in &restarts {
        out.extend_from_slice(&restart.to_le_bytes());
    }
    out.extend_from_slice(&u32::try_from(restarts.len()).unwrap_or(0).to_le_bytes());
    out
}

fn serialized(block: &[u8], compression: u8) -> Vec<u8> {
    let mut checked = block.to_vec();
    checked.push(compression);
    let crc = masked_crc(crc32c(&checked));
    let mut out = checked;
    out.extend_from_slice(&crc.to_le_bytes());
    out
}

fn leveldb_table(entries: &[(Vec<u8>, Vec<u8>)]) -> Vec<u8> {
    let data_block = block(entries);
    let data = serialized(&data_block, 0);
    let last = entries
        .last()
        .map(|(key, _)| key.clone())
        .unwrap_or_default();
    let index_entries = vec![(last, {
        let mut value = varint(0);
        value.extend_from_slice(&varint(u64::try_from(data_block.len()).unwrap_or(0)));
        value
    })];
    let index_block = block(&index_entries);
    let index = serialized(&index_block, 0);
    let index_offset = u64::try_from(data.len()).unwrap_or(0);
    let meta_offset = u64::try_from(data.len() + index.len()).unwrap_or(0);
    let empty_block = block(&[]);
    let empty = serialized(&empty_block, 0);
    let mut out = Vec::new();
    out.extend_from_slice(&data);
    out.extend_from_slice(&index);
    out.extend_from_slice(&empty);
    let mut footer = Vec::new();
    footer.extend_from_slice(&varint(meta_offset));
    footer.extend_from_slice(&varint(u64::try_from(empty_block.len()).unwrap_or(0)));
    footer.extend_from_slice(&varint(index_offset));
    footer.extend_from_slice(&varint(u64::try_from(index_block.len()).unwrap_or(0)));
    footer.resize(40, 0);
    footer.extend_from_slice(&LEVELDB_MAGIC.to_le_bytes());
    out.extend_from_slice(&footer);
    out
}

fn rocksdb_table(entries: &[(Vec<u8>, Vec<u8>)], compress: bool) -> Vec<u8> {
    let raw = block(entries);
    let (stored, stored_len) = if compress {
        let mut encoder =
            flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
        let payload = match encoder.write_all(&raw) {
            Ok(()) => encoder.finish().ok(),
            Err(_) => None,
        };
        match payload {
            Some(payload) => {
                let len = payload.len();
                (serialized(&payload, 2), len)
            }
            None => (serialized(&raw, 0), raw.len()),
        }
    } else {
        (serialized(&raw, 0), raw.len())
    };
    let data = stored;
    let last = entries
        .last()
        .map(|(key, _)| key.clone())
        .unwrap_or_default();
    let mut handle = varint(0);
    handle.extend_from_slice(&varint(u64::try_from(stored_len).unwrap_or(0)));
    let index_block = block(&[(last, handle)]);
    let index = serialized(&index_block, 0);
    let index_offset = u64::try_from(data.len()).unwrap_or(0);
    let properties_value = 0_u32.to_le_bytes();
    let properties_block = block(&[(
        b"rocksdb.block.based.table.index.type".to_vec(),
        properties_value.to_vec(),
    )]);
    let properties = serialized(&properties_block, 0);
    let properties_offset = u64::try_from(data.len() + index.len()).unwrap_or(0);
    let mut properties_handle = varint(properties_offset);
    properties_handle
        .extend_from_slice(&varint(u64::try_from(properties_block.len()).unwrap_or(0)));
    let metaindex_block = block(&[(b"rocksdb.properties".to_vec(), properties_handle)]);
    let metaindex = serialized(&metaindex_block, 0);
    let metaindex_offset = u64::try_from(data.len() + index.len() + properties.len()).unwrap_or(0);
    let mut out = Vec::new();
    out.extend_from_slice(&data);
    out.extend_from_slice(&index);
    out.extend_from_slice(&properties);
    out.extend_from_slice(&metaindex);
    let mut footer = Vec::new();
    footer.push(1);
    footer.extend_from_slice(&varint(metaindex_offset));
    footer.extend_from_slice(&varint(u64::try_from(metaindex_block.len()).unwrap_or(0)));
    footer.extend_from_slice(&varint(index_offset));
    footer.extend_from_slice(&varint(u64::try_from(index_block.len()).unwrap_or(0)));
    footer.resize(41, 0);
    footer.extend_from_slice(&2_u32.to_le_bytes());
    footer.extend_from_slice(&ROCKSDB_MAGIC.to_le_bytes());
    out.extend_from_slice(&footer);
    out
}

fn write_batch(entries: &[(u8, &[u8], &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&1_u64.to_le_bytes());
    out.extend_from_slice(&u32::try_from(entries.len()).unwrap_or(0).to_le_bytes());
    for (tag, key, value) in entries {
        out.push(*tag);
        out.extend_from_slice(&varint(u64::try_from(key.len()).unwrap_or(0)));
        out.extend_from_slice(key);
        if *tag == 1 {
            out.extend_from_slice(&varint(u64::try_from(value.len()).unwrap_or(0)));
            out.extend_from_slice(value);
        }
    }
    out
}

fn log_record(payload: &[u8]) -> Vec<u8> {
    let mut checked = vec![1_u8];
    checked.extend_from_slice(payload);
    let crc = masked_crc(crc32c(&checked));
    let mut out = Vec::new();
    out.extend_from_slice(&crc.to_le_bytes());
    out.extend_from_slice(&u16::try_from(payload.len()).unwrap_or(0).to_le_bytes());
    out.push(1);
    out.extend_from_slice(payload);
    out
}

fn leveldb_log() -> Vec<u8> {
    let chrome_key = {
        let mut key = b"_https://game.example\0".to_vec();
        key.push(1);
        key.extend_from_slice(b"gold");
        key
    };
    let chrome_value = {
        let mut value = vec![1_u8];
        value.extend_from_slice(b"1250");
        value
    };
    let first = write_batch(&[
        (1, b"player/gold", b"1250"),
        (1, b"player/name", b"Aria"),
        (0, b"old/flag", b""),
        (1, chrome_key.as_slice(), chrome_value.as_slice()),
    ]);
    let second = write_batch(&[(1, b"player/gold", b"1300")]);
    let mut out = log_record(&first);
    out.extend_from_slice(&log_record(&second));
    out
}

enum Bson {
    Int(i32),
    Double(f64),
    Text(&'static str),
    Bool(bool),
}

fn bson_element(name: &str, value: &Bson) -> Vec<u8> {
    let mut out = Vec::new();
    match value {
        Bson::Int(v) => {
            out.push(0x10);
            out.extend_from_slice(name.as_bytes());
            out.push(0);
            out.extend_from_slice(&v.to_le_bytes());
        }
        Bson::Double(v) => {
            out.push(0x01);
            out.extend_from_slice(name.as_bytes());
            out.push(0);
            out.extend_from_slice(&v.to_le_bytes());
        }
        Bson::Bool(v) => {
            out.push(0x08);
            out.extend_from_slice(name.as_bytes());
            out.push(0);
            out.push(u8::from(*v));
        }
        Bson::Text(v) => {
            out.push(0x02);
            out.extend_from_slice(name.as_bytes());
            out.push(0);
            out.extend_from_slice(&i32::try_from(v.len() + 1).unwrap_or(0).to_le_bytes());
            out.extend_from_slice(v.as_bytes());
            out.push(0);
        }
    }
    out
}

fn bson(entries: &[(&str, Bson)]) -> Vec<u8> {
    let mut body = Vec::new();
    for (name, value) in entries {
        body.extend_from_slice(&bson_element(name, value));
    }
    let mut out = Vec::new();
    out.extend_from_slice(&i32::try_from(body.len() + 5).unwrap_or(0).to_le_bytes());
    out.extend_from_slice(&body);
    out.push(0);
    out
}

fn player_bson(
    id: i32,
    name: &'static str,
    gold: i32,
    level: i32,
    hp: f64,
    alive: bool,
) -> Vec<u8> {
    bson(&[
        ("_id", Bson::Int(id)),
        ("name", Bson::Text(name)),
        ("gold", Bson::Int(gold)),
        ("level", Bson::Int(level)),
        ("hp", Bson::Double(hp)),
        ("alive", Bson::Bool(alive)),
    ])
}

/// Writes one v5 `CollectionIndex` (`Engine/Structures/CollectionIndex.cs`) and returns the cursor.
fn v5_index(
    page: &mut [u8],
    mut cursor: usize,
    slot: u8,
    name: &str,
    expression: &str,
    unique: bool,
) -> usize {
    put_u8(page, cursor, slot);
    put_u8(page, cursor + 1, 0);
    cursor += 2;
    put_bytes(page, cursor, format!("{name}\0").as_bytes());
    cursor += name.len() + 1;
    put_bytes(page, cursor, format!("{expression}\0").as_bytes());
    cursor += expression.len() + 1;
    put_u8(page, cursor, u8::from(unique));
    cursor += 1;
    put_u32(page, cursor, NONE);
    put_u8(page, cursor + 4, 255);
    cursor += 5;
    put_u32(page, cursor, NONE);
    put_u8(page, cursor + 4, 255);
    cursor += 5;
    put_u8(page, cursor, 1);
    cursor += 1;
    put_u32(page, cursor, NONE);
    cursor + 4
}

fn header_v5_base(page: &mut [u8], id: u32, kind: u8, col: u32) {
    put_u32(page, 0, id);
    if let Some(byte) = page.get_mut(4) {
        *byte = kind;
    }
    put_u32(page, 5, NONE);
    put_u32(page, 9, NONE);
    if let Some(byte) = page.get_mut(13) {
        *byte = 255;
    }
    put_u32(page, 14, NONE);
    put_u32(page, 19, col);
    put_u16(page, 28, 32);
    if let Some(byte) = page.get_mut(30) {
        *byte = 255;
    }
}

fn litedb_v5() -> Vec<u8> {
    let mut out = vec![0_u8; 8192 * 3];
    // Header page.
    if let Some(page) = out.get_mut(..8192) {
        header_v5_base(page, 0, 1, NONE);
        put_bytes(page, 32, b"** This is a LiteDB file **");
        if let Some(byte) = page.get_mut(59) {
            *byte = 8;
        }
        put_u32(page, 60, NONE);
        put_u32(page, 64, 2);
        let collections = bson(&[("player", Bson::Int(1))]);
        put_bytes(page, 192, &collections);
    }
    // Collection page.
    if let Some(page) = out.get_mut(8192..16384) {
        header_v5_base(page, 1, 2, NONE);
        for slot in 0..5 {
            put_u32(page, 32 + slot * 4, NONE);
        }
        put_u8(page, 96, 2);
        let mut cursor = v5_index(page, 97, 0, "_id", "$._id", true);
        cursor = v5_index(page, cursor, 1, "gold", "$.gold", false);
        let _ = cursor;
    }
    // Data page.
    if let Some(page) = out.get_mut(16384..24576) {
        header_v5_base(page, 2, 4, 1);
        let first = player_bson(1, "Aria", 1250, 12, 87.5, true);
        let second = player_bson(2, "Bram", 40, 7, 50.0, false);
        let segments = [&first, &second];
        let mut position = 32_usize;
        let mut used = 0_u16;
        for (index, document) in segments.iter().enumerate() {
            let length = 6 + document.len();
            let mut segment = vec![0_u8; length];
            put_u32(&mut segment, 1, NONE);
            if let Some(byte) = segment.get_mut(5) {
                *byte = 255;
            }
            put_bytes(&mut segment, 6, document);
            put_bytes(page, position, &segment);
            let slot = 8192 - (index + 1) * 4;
            put_u16(page, slot + 2, u16::try_from(position).unwrap_or(0));
            put_u16(page, slot, u16::try_from(length).unwrap_or(0));
            position += length;
            used = used.saturating_add(u16::try_from(length).unwrap_or(0));
        }
        if let Some(byte) = page.get_mut(23) {
            *byte = 2;
        }
        put_u16(page, 24, used);
        put_u16(page, 28, u16::try_from(position).unwrap_or(0));
        if let Some(byte) = page.get_mut(30) {
            *byte = 1;
        }
    }
    out
}

fn header_v4(page: &mut [u8], id: u32, kind: u8, items: u16) {
    put_u32(page, 0, id);
    if let Some(byte) = page.get_mut(4) {
        *byte = kind;
    }
    put_u32(page, 5, NONE);
    put_u32(page, 9, NONE);
    put_u16(page, 13, items);
}

fn litedb_v4() -> Vec<u8> {
    let mut out = vec![0_u8; 4096 * 5];
    // Header page 0.
    if let Some(page) = out.get_mut(..4096) {
        header_v4(page, 0, 1, 0);
        put_bytes(page, 25, b"** This is a LiteDB file **");
        if let Some(byte) = page.get_mut(52) {
            *byte = 7;
        }
        put_u16(page, 65, 0);
        if let Some(byte) = page.get_mut(103) {
            *byte = 1;
        }
        put_bytes(page, 104, b"player\0");
        put_u32(page, 111, 1);
    }
    // Collection page 1.
    if let Some(page) = out.get_mut(4096..8192) {
        header_v4(page, 1, 2, 0);
        let mut cursor = 25;
        put_bytes(page, cursor, b"player\0");
        cursor += 7 + 12;
        // 16 index slots; the first two are used (primary key and a secondary index).
        put_bytes(page, cursor, b"_id=$._id\0");
        cursor += 10;
        put_u8(page, cursor, 1);
        cursor += 1;
        put_u32(page, cursor, 2);
        cursor += 4;
        cursor += 12; // head node (2), tail node (6), free index page list (4)
        put_bytes(page, cursor, b"gold=$.gold\0");
        cursor += 12;
        put_u8(page, cursor, 0);
        cursor += 1;
        put_u32(page, cursor, NONE);
        // Remaining 14 slots stay empty.
    }
    // Index page 2.
    if let Some(page) = out.get_mut(8192..12288) {
        header_v4(page, 2, 3, 2);
        let mut cursor = 25;
        for index in 0..2_u16 {
            put_u16(page, cursor, index);
            if let Some(byte) = page.get_mut(cursor + 2) {
                *byte = 1; // levels
            }
            if let Some(byte) = page.get_mut(cursor + 3) {
                *byte = 0; // slot
            }
            put_u32(page, cursor + 4, NONE);
            put_u16(page, cursor + 8, 255);
            put_u32(page, cursor + 10, NONE);
            put_u16(page, cursor + 14, 255);
            put_u16(page, cursor + 16, 0); // key length
            if let Some(byte) = page.get_mut(cursor + 18) {
                *byte = 0; // key type
            }
            put_u32(page, cursor + 19, 3);
            put_u16(page, cursor + 23, index);
            put_u32(page, cursor + 25, NONE);
            put_u16(page, cursor + 29, 255);
            put_u32(page, cursor + 31, NONE);
            put_u16(page, cursor + 35, 255);
            cursor += 37;
        }
    }
    let first = player_bson(1, "Aria", 1250, 12, 87.5, true);
    let second = player_bson(2, "Bram", 40, 7, 50.0, false);
    let split = 20_usize;
    let (head, tail) = second.split_at(split.min(second.len()));
    // Data page 3.
    if let Some(page) = out.get_mut(12288..16384) {
        header_v4(page, 3, 4, 2);
        let mut cursor = 25;
        put_u16(page, cursor, 0);
        put_u32(page, cursor + 2, NONE);
        put_u16(page, cursor + 6, u16::try_from(first.len()).unwrap_or(0));
        put_bytes(page, cursor + 8, &first);
        cursor += 8 + first.len();
        put_u16(page, cursor, 1);
        put_u32(page, cursor + 2, 4);
        put_u16(page, cursor + 6, u16::try_from(head.len()).unwrap_or(0));
        put_bytes(page, cursor + 8, head);
    }
    // Extend page 4.
    if let Some(page) = out.get_mut(16384..20480) {
        header_v4(page, 4, 5, u16::try_from(tail.len()).unwrap_or(0));
        put_u32(page, 9, NONE);
        put_bytes(page, 25, tail);
    }
    out
}

fn main() -> std::io::Result<()> {
    let base = fixtures();
    let leveldb = base.join("leveldb");
    let rocksdb = base.join("rocksdb");
    let litedb = base.join("litedb");
    fs::create_dir_all(&leveldb)?;
    fs::create_dir_all(&rocksdb)?;
    fs::create_dir_all(&litedb)?;

    let table_entries = vec![
        (internal_key(b"player/gold", 4, 1), b"1250".to_vec()),
        (internal_key(b"player/name", 4, 1), b"Aria".to_vec()),
        (internal_key(b"removed/flag", 3, 0), Vec::new()),
    ];
    fs::write(leveldb.join("save.ldb"), leveldb_table(&table_entries))?;
    fs::write(leveldb.join("000003.log"), leveldb_log())?;
    fs::write(
        rocksdb.join("save.sst"),
        rocksdb_table(&table_entries, false),
    )?;
    fs::write(
        rocksdb.join("compressed.sst"),
        rocksdb_table(&table_entries, true),
    )?;
    fs::write(litedb.join("game.db"), litedb_v5())?;
    fs::write(litedb.join("legacy-v4.db"), litedb_v4())?;
    println!("wrote {} fixtures", 6);
    Ok(())
}
