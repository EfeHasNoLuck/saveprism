//! `format.gvas` tests: a small hand-built UE 4.27 save (one `IntProperty`) through the codec.

use saveprism_codecs::gvas::{self, GVAS_PARSE};
use saveprism_core::{
    Codec, DecodeCtx, Limits, ParamValue, Params, Registry, SniffCtx, codes, doc::Value,
};

fn put_string(out: &mut Vec<u8>, text: &str) {
    let len = i32::try_from(text.len() + 1).unwrap_or(i32::MAX);
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(text.as_bytes());
    out.push(0);
}

/// A minimal GVAS file: UE 4.27 header, class name, `Gold: IntProperty = 42`, `None`.
fn sample_save() -> Vec<u8> {
    let mut out = Vec::new();
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
    put_string(&mut out, "Gold");
    put_string(&mut out, "IntProperty");
    out.extend_from_slice(&4_u32.to_le_bytes()); // size
    out.extend_from_slice(&0_u32.to_le_bytes()); // index
    out.push(0); // no property guid
    out.extend_from_slice(&42_i32.to_le_bytes());
    put_string(&mut out, "None");
    out
}

fn registry() -> Registry {
    let mut reg = Registry::default();
    gvas::register(&mut reg);
    reg
}

fn decode_with(
    codec: &dyn Codec,
    bytes: &[u8],
    hints: Params,
) -> saveprism_core::Result<saveprism_core::Decoded> {
    let limits = Limits::default();
    let mut ctx = DecodeCtx::new(&limits).with_step(0, hints);
    codec.decode(bytes, &mut ctx)
}

#[test]
fn sniff_needs_the_gvas_magic() {
    let reg = registry();
    let codec = reg.codec("format.gvas").unwrap();
    let ctx = SniffCtx::default();
    assert!(codec.sniff(&sample_save(), &ctx).value() >= 0.9);
    assert_eq!(codec.sniff(b"{\"a\":1}", &ctx).value(), 0.0);
    assert_eq!(codec.sniff(b"GVA", &ctx).value(), 0.0);
    assert_eq!(codec.sniff(b"", &ctx).value(), 0.0);
}

#[test]
fn decode_encode_is_byte_identical() {
    let reg = registry();
    let codec = reg.codec("format.gvas").unwrap();
    let bytes = sample_save();
    let decoded = decode_with(codec, &bytes, Params::new()).unwrap();
    assert!(matches!(
        decoded.doc.get(decoded.doc.root()).map(|node| &node.value),
        Some(Value::Object(_))
    ));
    let again = codec.encode(&decoded.doc, &decoded.params).unwrap();
    assert_eq!(again, bytes);
}

#[test]
fn types_parameter_survives_decode_and_encode() {
    let reg = registry();
    let codec = reg.codec("format.gvas").unwrap();
    let bytes = sample_save();

    let mut table = Params::new();
    table.set(".worldSaveData.CharacterSaveParameterMap.Key", "Guid");
    let mut hints = Params::new();
    hints.set("types", table.clone());

    let decoded = decode_with(codec, &bytes, hints).unwrap();
    assert_eq!(
        decoded.params.get("types"),
        Some(&ParamValue::Table(table.clone()))
    );
    let again = codec.encode(&decoded.doc, &decoded.params).unwrap();
    assert_eq!(again, bytes);

    // Without hints an empty table is stored, so the key is always present.
    let plain = decode_with(codec, &bytes, Params::new()).unwrap();
    assert_eq!(
        plain.params.get("types"),
        Some(&ParamValue::Table(Params::new()))
    );
}

#[test]
fn bad_types_parameter_is_rejected() {
    let reg = registry();
    let codec = reg.codec("format.gvas").unwrap();
    let mut table = Params::new();
    table.set(".a.b", 5_i64);
    let mut hints = Params::new();
    hints.set("types", table);
    let error = decode_with(codec, &sample_save(), hints).unwrap_err();
    assert_eq!(error.code(), codes::CORE_INVALID_PARAM);

    let mut hints = Params::new();
    hints.set("types", "Guid");
    let error = decode_with(codec, &sample_save(), hints).unwrap_err();
    assert_eq!(error.code(), codes::CORE_INVALID_PARAM);
}

#[test]
fn every_truncation_is_an_error_not_a_panic() {
    let reg = registry();
    let codec = reg.codec("format.gvas").unwrap();
    let bytes = sample_save();
    for len in 0..bytes.len() {
        let result = decode_with(codec, &bytes[..len], Params::new());
        let error = result.expect_err("a truncated save must not decode");
        assert_eq!(error.code(), GVAS_PARSE, "length {len}");
    }
}

#[test]
fn garbage_after_the_magic_is_an_error() {
    let reg = registry();
    let codec = reg.codec("format.gvas").unwrap();
    let mut bytes = b"GVAS".to_vec();
    bytes.extend(std::iter::repeat_n(0xFF_u8, 64));
    let error = decode_with(codec, &bytes, Params::new()).unwrap_err();
    assert_eq!(error.code(), GVAS_PARSE);
}

#[test]
fn input_over_the_limit_is_refused() {
    let reg = registry();
    let codec = reg.codec("format.gvas").unwrap();
    let limits = Limits {
        max_input_bytes: 16,
        ..Limits::default()
    };
    let mut ctx = DecodeCtx::new(&limits);
    let error = codec.decode(&sample_save(), &mut ctx).unwrap_err();
    assert_eq!(error.code(), codes::CORE_LIMIT_INPUT);
}
