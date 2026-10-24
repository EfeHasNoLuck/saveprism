//! Writes the synthetic layer fixtures under `fixtures/synthetic/layers/`.
//!
//! `cargo run -p saveprism-codecs --example make_layer_fixtures`
//!
//! The data is invented. The compressed variants are made with the same crates the layers use, so
//! the committed bytes are an independent check of the header parsing and of the round trip.

use std::fs;
use std::io::Write;
use std::path::PathBuf;

const JSON: &[u8] = b"{\"player\":{\"name\":\"Ada\",\"level\":12,\"gold\":340},\"day\":3}";

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join("synthetic/layers")
}

fn pad_to_block(out: &mut Vec<u8>) {
    while !out.len().is_multiple_of(512) {
        out.push(0);
    }
}

fn tar_header(name: &str, size: usize) -> [u8; 512] {
    let mut header = [0_u8; 512];
    header
        .get_mut(..name.len())
        .unwrap_or_default()
        .copy_from_slice(name.as_bytes());
    header
        .get_mut(100..108)
        .unwrap_or_default()
        .copy_from_slice(b"0000644\0");
    header
        .get_mut(108..116)
        .unwrap_or_default()
        .copy_from_slice(b"0000000\0");
    header
        .get_mut(116..124)
        .unwrap_or_default()
        .copy_from_slice(b"0000000\0");
    let size_field = format!("{size:011o}\0");
    header
        .get_mut(124..136)
        .unwrap_or_default()
        .copy_from_slice(size_field.as_bytes());
    header
        .get_mut(136..148)
        .unwrap_or_default()
        .copy_from_slice(b"00000000000\0");
    header
        .get_mut(148..156)
        .unwrap_or_default()
        .copy_from_slice(b"        ");
    header
        .get_mut(156..157)
        .unwrap_or_default()
        .copy_from_slice(b"0");
    header
        .get_mut(257..263)
        .unwrap_or_default()
        .copy_from_slice(b"ustar\0");
    header
        .get_mut(263..265)
        .unwrap_or_default()
        .copy_from_slice(b"00");
    header
        .get_mut(265..269)
        .unwrap_or_default()
        .copy_from_slice(b"root");
    header
        .get_mut(297..301)
        .unwrap_or_default()
        .copy_from_slice(b"root");
    let sum: u64 = header.iter().map(|&byte| u64::from(byte)).sum();
    let checksum = format!("{sum:06o}\0 ");
    header
        .get_mut(148..156)
        .unwrap_or_default()
        .copy_from_slice(checksum.as_bytes());
    header
}

fn tar_entry(out: &mut Vec<u8>, name: &str, data: &[u8]) {
    out.extend_from_slice(&tar_header(name, data.len()));
    out.extend_from_slice(data);
    pad_to_block(out);
}

fn hex_upper(data: &[u8]) -> Vec<u8> {
    const TABLE: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = Vec::with_capacity(data.len() * 2);
    for &byte in data {
        out.push(TABLE.get(usize::from(byte >> 4)).copied().unwrap_or(b'0'));
        out.push(TABLE.get(usize::from(byte & 0x0F)).copied().unwrap_or(b'0'));
    }
    out
}

fn percent_upper(data: &[u8]) -> Vec<u8> {
    const TABLE: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = Vec::with_capacity(data.len() * 3);
    for &byte in data {
        out.push(b'%');
        out.push(TABLE.get(usize::from(byte >> 4)).copied().unwrap_or(b'0'));
        out.push(TABLE.get(usize::from(byte & 0x0F)).copied().unwrap_or(b'0'));
    }
    out
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = fixtures();
    fs::create_dir_all(&out_dir)?;
    fs::write(out_dir.join("save.json"), JSON)?;

    let mut encoder = ::bzip2::write::BzEncoder::new(Vec::new(), ::bzip2::Compression::best());
    encoder.write_all(JSON)?;
    fs::write(out_dir.join("save.json.bz2"), encoder.finish()?)?;

    fs::write(
        out_dir.join("save.json.zst"),
        ::zstd::stream::encode_all(std::io::Cursor::new(JSON), 3)?,
    )?;

    let mut xz = Vec::new();
    ::lzma_rs::xz_compress(
        &mut std::io::BufReader::new(std::io::Cursor::new(JSON)),
        &mut xz,
    )?;
    fs::write(out_dir.join("save.json.xz"), xz)?;

    let mut lzma = Vec::new();
    ::lzma_rs::lzma_compress(
        &mut std::io::BufReader::new(std::io::Cursor::new(JSON)),
        &mut lzma,
    )?;
    fs::write(out_dir.join("save.json.lzma"), lzma)?;

    let mut lz4 = ::lz4_flex::frame::FrameEncoder::new(Vec::new());
    lz4.write_all(JSON)?;
    fs::write(out_dir.join("save.json.lz4"), lz4.finish()?)?;

    let mut brotli = Vec::new();
    let params = ::brotli::enc::BrotliEncoderParams {
        quality: 6,
        lgwin: 22,
        ..Default::default()
    };
    ::brotli::BrotliCompress(&mut std::io::Cursor::new(JSON), &mut brotli, &params)?;
    fs::write(out_dir.join("save.json.br"), brotli)?;

    let mut tar = Vec::new();
    tar_entry(&mut tar, "save.json", JSON);
    tar_entry(&mut tar, "readme.txt", b"invented container fixture\n");
    tar.extend_from_slice(&[0_u8; 1024]);
    fs::write(out_dir.join("save.tar"), tar)?;

    fs::write(out_dir.join("save.json.hex"), hex_upper(JSON))?;

    let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
    gzip.write_all(JSON)?;
    fs::write(
        out_dir.join("save.json.pct"),
        percent_upper(&gzip.finish()?),
    )?;

    println!("wrote layer fixtures to {}", out_dir.display());
    Ok(())
}
