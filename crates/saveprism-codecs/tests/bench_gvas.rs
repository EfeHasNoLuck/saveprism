//! Manual timing of `format.gvas` on a large (about 52 MB) GVAS save.
//!
//! Not part of the normal run (`#[ignore]`). Run it in release mode:
//! `cargo test -p saveprism-codecs --release --test bench_gvas -- --ignored --nocapture`
//!
//! The only assertion is the byte round trip (decode, then encode, gives the same bytes).
//! No time limit is enforced; the printed durations are the result.

use std::io::Cursor;
use std::time::Instant;

use saveprism_codecs::gvas;
use saveprism_core::{Codec, DecodeCtx, Limits, Params, Registry};
use uesave::{Save, Types};

const MIB: f64 = 1024.0 * 1024.0;
/// Size of the text property in the bench save (the whole file is a little larger).
const TEXT_BYTES: usize = 52 * 1024 * 1024;

fn put_string(out: &mut Vec<u8>, text: &str) {
    let len = i32::try_from(text.len() + 1).unwrap_or(i32::MAX);
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(text.as_bytes());
    out.push(0);
}

/// A GVAS file with the same header as `tests/gvas.rs`, one large `StrProperty` and `None`.
fn big_save(text_len: usize) -> Vec<u8> {
    let text = "a".repeat(text_len);
    let mut out = Vec::with_capacity(text_len + 256);
    out.extend_from_slice(b"GVAS");
    out.extend_from_slice(&2_u32.to_le_bytes()); // save game version
    out.extend_from_slice(&522_u32.to_le_bytes()); // UE4 package version
    out.extend_from_slice(&4_u16.to_le_bytes()); // engine major
    out.extend_from_slice(&27_u16.to_le_bytes()); // engine minor
    out.extend_from_slice(&0_u16.to_le_bytes()); // engine patch
    out.extend_from_slice(&0_u32.to_le_bytes()); // engine build
    put_string(&mut out, "++UE4+Release-4.27");
    out.extend_from_slice(&3_u32.to_le_bytes()); // custom version format
    out.extend_from_slice(&0_u32.to_le_bytes()); // custom version count
    put_string(&mut out, "/Script/Test.TestSave");
    put_string(&mut out, "Blob");
    put_string(&mut out, "StrProperty");
    // Value size: the length prefix (4) + the bytes + the NUL terminator.
    let value_size = u32::try_from(text_len + 5).unwrap_or(u32::MAX);
    out.extend_from_slice(&value_size.to_le_bytes()); // size
    out.extend_from_slice(&0_u32.to_le_bytes()); // index
    out.push(0); // no property guid
    put_string(&mut out, &text);
    put_string(&mut out, "None");
    out
}

#[test]
#[ignore]
fn bench_gvas() {
    let bytes = big_save(TEXT_BYTES);
    let limits = Limits::default();
    let input_len = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    println!(
        "input: {:.2} MB ({} bytes)",
        bytes.len() as f64 / MIB,
        bytes.len()
    );
    println!(
        "limits: default (max_input_bytes {:.0} MB, max_decoded_bytes {:.0} MB); input fits: {}",
        limits.max_input_bytes as f64 / MIB,
        limits.max_decoded_bytes as f64 / MIB,
        input_len <= limits.max_input_bytes
    );

    // 1. uesave read only.
    let start = Instant::now();
    let save = Save::read_with_types(&mut Cursor::new(bytes.as_slice()), Types::new())
        .expect("uesave reads the bench save");
    println!("uesave read: {:?}", start.elapsed());
    drop(save);

    // 2. format.gvas decode, up to the SaveDoc.
    let mut reg = Registry::default();
    gvas::register(&mut reg);
    let codec: &dyn Codec = reg.codec("format.gvas").expect("format.gvas is registered");
    let start = Instant::now();
    let mut ctx = DecodeCtx::new(&limits).with_step(0, Params::new());
    let decoded = codec
        .decode(&bytes, &mut ctx)
        .expect("format.gvas decodes the bench save");
    println!("format.gvas decode (to SaveDoc): {:?}", start.elapsed());

    // 3. format.gvas encode from the SaveDoc.
    let start = Instant::now();
    let again = codec
        .encode(&decoded.doc, &decoded.params)
        .expect("format.gvas encodes the bench save");
    println!("format.gvas encode: {:?}", start.elapsed());

    assert!(again == bytes, "decode -> encode changed the bytes");
    println!("round trip: byte identical");
}
