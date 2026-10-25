//! Tests for the GameMaker `ds_map_write` codec.
#![allow(clippy::expect_used, clippy::indexing_slicing)]

use saveprism_core::{Codec, DecodeCtx, Edit, Limits, OwnedValue, Path, Value};

use super::GameMakerDsMapCodec;

fn real(value: f64) -> Vec<u8> {
    let mut out = 0_u32.to_le_bytes().to_vec();
    out.extend_from_slice(&value.to_bits().to_le_bytes());
    out
}

fn text(value: &str) -> Vec<u8> {
    let mut out = 1_u32.to_le_bytes().to_vec();
    out.extend_from_slice(&(value.len() as u32).to_le_bytes());
    out.extend_from_slice(value.as_bytes());
    out
}

fn map_bytes() -> Vec<u8> {
    let mut out = 402_u32.to_le_bytes().to_vec();
    out.extend_from_slice(&3_u32.to_le_bytes());
    out.extend_from_slice(&text("gold"));
    out.extend_from_slice(&real(120.0));
    out.extend_from_slice(&text("name"));
    out.extend_from_slice(&text("Mira"));
    out.extend_from_slice(&real(2.5));
    out.extend_from_slice(&text("pi"));
    out
}

fn hex(bytes: &[u8], upper: bool) -> Vec<u8> {
    bytes
        .iter()
        .flat_map(|byte| {
            let text = if upper {
                format!("{byte:02X}")
            } else {
                format!("{byte:02x}")
            };
            text.into_bytes()
        })
        .collect()
}

fn decode(input: &[u8]) -> saveprism_core::Decoded {
    GameMakerDsMapCodec
        .decode(input, &mut DecodeCtx::new(&Limits::default()))
        .expect("ds_map parses")
}

fn encode(decoded: &saveprism_core::Decoded) -> Vec<u8> {
    GameMakerDsMapCodec
        .encode(&decoded.doc, &decoded.params)
        .expect("ds_map writes")
}

fn value(decoded: &saveprism_core::Decoded, path: &str) -> Value {
    let id = decoded
        .doc
        .resolve(&Path::parse(path).expect("path"))
        .expect("node");
    decoded
        .doc
        .get(id)
        .map(|node| node.value.clone())
        .expect("value")
}

#[test]
fn a_map_round_trips_and_edits_keep_its_shape() {
    let input = hex(&map_bytes(), true);
    let mut decoded = decode(&input);
    assert_eq!(encode(&decoded), input);
    assert_eq!(value(&decoded, "/1"), Value::Str("Mira".into()));
    assert_eq!(
        value(&decoded, "/2.key"),
        Value::Float {
            v: 2.5,
            width: saveprism_core::FloatWidth::F64
        }
    );
    saveprism_core::apply_edits(
        &mut decoded.doc,
        &[Edit::Set {
            path: Path::parse("/0").expect("path"),
            value: OwnedValue::Int(999),
        }],
        &Limits::default(),
    )
    .expect("edit applies");
    let output = encode(&decoded);
    let again = decode(&output);
    assert_eq!(
        value(&again, "/0"),
        Value::Float {
            v: 999.0,
            width: saveprism_core::FloatWidth::F64
        }
    );
    assert_eq!(value(&again, "/1"), Value::Str("Mira".into()));
}

#[test]
fn a_lowercase_dump_is_written_back_unchanged() {
    let input = hex(&map_bytes(), false);
    let decoded = decode(&input);
    assert_eq!(encode(&decoded), input);
}

#[test]
fn a_trailing_newline_is_kept() {
    let mut input = hex(&map_bytes(), true);
    input.extend_from_slice(b"\r\n");
    let decoded = decode(&input);
    assert_eq!(encode(&decoded), input);
}

#[test]
fn unknown_value_types_are_refused() {
    let mut bytes = 402_u32.to_le_bytes().to_vec();
    bytes.extend_from_slice(&1_u32.to_le_bytes());
    bytes.extend_from_slice(&text("gold"));
    bytes.extend_from_slice(&2_u32.to_le_bytes());
    let input = hex(&bytes, true);
    assert!(
        GameMakerDsMapCodec
            .decode(&input, &mut DecodeCtx::new(&Limits::default()))
            .is_err()
    );
}

#[test]
fn generated_fixtures_round_trip() {
    let base =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/synthetic/gamemaker");
    for name in ["ds-map.txt", "ds-map-lower.txt"] {
        let input = std::fs::read(base.join(name)).expect("fixture exists");
        let decoded = decode(&input);
        assert_eq!(encode(&decoded), input, "{name}");
    }
}
