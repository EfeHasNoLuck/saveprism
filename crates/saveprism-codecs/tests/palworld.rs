//! `container.palworld-plz` tests: a small hand-built GVAS wrapped as `PlZ` 0x31 and 0x32.

use std::io::Write;

use flate2::Compression;
use flate2::write::ZlibEncoder;
use saveprism_codecs::gvas;
use saveprism_codecs::palworld::{self, PAL_INVALID, PAL_OODLE, PalworldPlzLayer};
use saveprism_core::{Confidence, DecodeCtx, Layer, Limits, Registry, SniffCtx};

fn put_string(out: &mut Vec<u8>, text: &str) {
    let len = i32::try_from(text.len() + 1).unwrap_or(i32::MAX);
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(text.as_bytes());
    out.push(0);
}

/// A minimal GVAS file: UE 4.27 header, class name, `Gold: IntProperty = 42`, `None`.
fn sample_gvas() -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"GVAS");
    out.extend_from_slice(&2_u32.to_le_bytes());
    out.extend_from_slice(&522_u32.to_le_bytes());
    out.extend_from_slice(&4_u16.to_le_bytes());
    out.extend_from_slice(&27_u16.to_le_bytes());
    out.extend_from_slice(&0_u16.to_le_bytes());
    out.extend_from_slice(&0_u32.to_le_bytes());
    put_string(&mut out, "++UE4+Release-4.27");
    out.extend_from_slice(&3_u32.to_le_bytes());
    out.extend_from_slice(&0_u32.to_le_bytes());
    put_string(&mut out, "/Script/Test.TestSave");
    put_string(&mut out, "Gold");
    put_string(&mut out, "IntProperty");
    out.extend_from_slice(&4_u32.to_le_bytes());
    out.extend_from_slice(&0_u32.to_le_bytes());
    out.push(0);
    out.extend_from_slice(&42_i32.to_le_bytes());
    put_string(&mut out, "None");
    out
}

fn zlib(data: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::new(6));
    encoder.write_all(data)?;
    Ok(encoder.finish()?)
}

/// Builds a PlZ file the way the game does.
fn make_plz(gvas: &[u8], kind: u8) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let first = zlib(gvas)?;
    let first_len = u32::try_from(first.len())?;
    let payload = if kind == 0x32 { zlib(&first)? } else { first };
    let mut out = Vec::new();
    out.extend_from_slice(&u32::try_from(gvas.len())?.to_le_bytes());
    out.extend_from_slice(&first_len.to_le_bytes());
    out.extend_from_slice(b"PlZ");
    out.push(kind);
    out.extend_from_slice(&payload);
    Ok(out)
}

fn decode(bytes: &[u8]) -> saveprism_core::Result<(Vec<u8>, saveprism_core::Params)> {
    let limits = Limits::default();
    let mut ctx = DecodeCtx::new(&limits);
    PalworldPlzLayer.decode(bytes, &mut ctx)
}

#[test]
fn both_kinds_open_and_wrap_back() {
    let gvas_bytes = sample_gvas();
    for kind in [0x31_u8, 0x32] {
        let file = make_plz(&gvas_bytes, kind).unwrap();
        let (inner, params) = decode(&file).unwrap();
        assert_eq!(inner, gvas_bytes);
        // The zlib levels are found by the zlib layer; if one were not reproduced exactly the
        // result would be semantic only, but a flate2 level-6 stream is reproduced byte for byte.
        let again = PalworldPlzLayer.encode(&inner, &params).unwrap();
        assert_eq!(again, file, "kind 0x{kind:02X}");
        let (inner2, _) = decode(&again).unwrap();
        assert_eq!(inner2, gvas_bytes);
    }
}

#[test]
fn inner_bytes_are_read_by_the_gvas_codec() {
    let mut reg = Registry::default();
    gvas::register(&mut reg);
    let codec = reg.codec("format.gvas").unwrap();
    let limits = Limits::default();
    let (inner, _) = decode(&make_plz(&sample_gvas(), 0x32).unwrap()).unwrap();
    let mut ctx = DecodeCtx::new(&limits);
    let decoded = codec.decode(&inner, &mut ctx).unwrap();
    assert_eq!(
        codec.encode(&decoded.doc, &decoded.params).unwrap(),
        sample_gvas()
    );
}

#[test]
fn oodle_is_reported() {
    let mut file = make_plz(&sample_gvas(), 0x31).unwrap();
    file[8..11].copy_from_slice(b"PlM");
    assert_eq!(decode(&file).unwrap_err().code(), PAL_OODLE);
    assert_eq!(
        PalworldPlzLayer.sniff(&file, &SniffCtx::default()),
        Confidence::NONE
    );
}

#[test]
fn sniff_needs_the_marker() {
    let ctx = SniffCtx::default();
    let file = make_plz(&sample_gvas(), 0x31).unwrap();
    assert!(PalworldPlzLayer.sniff(&file, &ctx) >= Confidence::HIGH);
    assert_eq!(PalworldPlzLayer.sniff(b"GVAS", &ctx), Confidence::NONE);
    assert_eq!(PalworldPlzLayer.sniff(b"", &ctx), Confidence::NONE);
}

#[test]
fn damaged_headers_give_sp_errors() {
    let good = make_plz(&sample_gvas(), 0x31).unwrap();
    let good2 = make_plz(&sample_gvas(), 0x32).unwrap();

    // Wrong uncompressed size.
    let mut file = good.clone();
    file[0] ^= 0x01;
    assert_eq!(decode(&file).unwrap_err().code(), PAL_INVALID);

    // Wrong compressed size, both kinds.
    for source in [&good, &good2] {
        let mut file = source.clone();
        file[4] ^= 0x01;
        assert_eq!(decode(&file).unwrap_err().code(), PAL_INVALID);
    }

    // Unknown kind.
    let mut file = good.clone();
    file[11] = 0x33;
    assert_eq!(decode(&file).unwrap_err().code(), PAL_INVALID);

    // Too short, wrong marker.
    assert_eq!(decode(b"").unwrap_err().code(), PAL_INVALID);
    assert_eq!(decode(&good[..5]).unwrap_err().code(), PAL_INVALID);
    let mut file = good.clone();
    file[8] = b'X';
    assert_eq!(decode(&file).unwrap_err().code(), PAL_INVALID);

    // Header only: the zlib data is missing (zlib error, not a panic).
    assert!(decode(&good[..12]).is_err());
    // Cut in the middle of the zlib data.
    assert!(decode(&good[..good.len() - 6]).is_err());
}

#[test]
fn register_adds_the_layer() {
    let mut reg = Registry::default();
    palworld::register(&mut reg);
    assert!(reg.is_layer("container.palworld-plz"));
}
