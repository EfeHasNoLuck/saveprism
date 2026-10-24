//! Tests of the Witcher 3 codec.

use std::path::PathBuf;

use proptest::prelude::*;
use saveprism_core::{
    Codec, DecodeCtx, Decoded, Edit, Limits, Node, NodeFlags, NodeId, OwnedValue, ParamValue, Path,
    SaveDoc, Value, apply_edits, codes,
};

use super::*;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn fixture() -> Vec<u8> {
    std::fs::read(fixtures().join("synthetic/witcher3/ManualSave_invented.sav")).unwrap()
}

fn decode_with(bytes: &[u8], limits: &Limits) -> saveprism_core::Result<Decoded> {
    let mut ctx = DecodeCtx::new(limits);
    Witcher3Codec.decode(bytes, &mut ctx)
}

fn decode(bytes: &[u8]) -> Decoded {
    decode_with(bytes, &Limits::default()).unwrap()
}

fn encode(decoded: &Decoded) -> Vec<u8> {
    Witcher3Codec.encode(&decoded.doc, &decoded.params).unwrap()
}

fn pointer(text: &str) -> Path {
    Path::parse(&format!("/{}", text.replace('.', "/"))).unwrap()
}

fn id_at(doc: &SaveDoc, path: &str) -> NodeId {
    doc.resolve(&pointer(path))
        .unwrap_or_else(|| panic!("no node at {path}"))
}

fn at<'a>(doc: &'a SaveDoc, path: &str) -> &'a Node {
    doc.get(id_at(doc, path)).unwrap()
}

fn int_at(doc: &SaveDoc, path: &str) -> i128 {
    match at(doc, path).value {
        Value::Int { v, .. } => v,
        ref other => panic!("{path} is {other:?}"),
    }
}

fn text_at(doc: &SaveDoc, path: &str) -> String {
    match &at(doc, path).value {
        Value::Str(text) => text.clone(),
        other => panic!("{path} is {other:?}"),
    }
}

fn set(decoded: &mut Decoded, path: &str, value: OwnedValue) -> saveprism_core::Result<()> {
    apply_edits(
        &mut decoded.doc,
        &[Edit::Set {
            path: pointer(path),
            value,
        }],
        &Limits::default(),
    )
}

fn hinted(decoded: &Decoded, path: &str) -> String {
    at(&decoded.doc, path)
        .type_hint
        .as_ref()
        .map(|hint| hint.as_str().to_string())
        .unwrap_or_default()
}

/// Builds an invented save whose variable table count includes the two footer words as its last
/// entry, the layout real next-gen saves use (the table then ends 6 bytes before the file end).
fn footer_overlapping_variable_table() -> Vec<u8> {
    let header_size = 28_usize;
    let mut stream = Vec::new();
    stream.extend_from_slice(STREAM_MAGIC);
    stream.extend_from_slice(&66_u32.to_le_bytes());
    stream.extend_from_slice(&29_u32.to_le_bytes());
    stream.extend_from_slice(&0_u16.to_le_bytes());
    stream.extend_from_slice(b"NM");
    stream.extend_from_slice(b"MANU");
    stream.extend_from_slice(&1_u32.to_le_bytes());
    stream.extend_from_slice(&0_u32.to_le_bytes());
    stream.push(1);
    stream.push(b'P');
    stream.extend_from_slice(&0_u32.to_le_bytes());
    stream.extend_from_slice(b"ENOD");
    let names_absolute = (header_size + 14) as u32;
    stream.extend_from_slice(&names_absolute.to_le_bytes());
    stream.extend_from_slice(&0_u32.to_le_bytes());
    stream.extend_from_slice(b"SC");
    let variables_offset = stream.len();
    stream.extend_from_slice(&2_u32.to_le_bytes());
    stream.extend_from_slice(&0_u32.to_le_bytes());
    stream.extend_from_slice(&4_u32.to_le_bytes());
    stream.extend_from_slice(&names_absolute.to_le_bytes());
    stream.extend_from_slice(&0_u32.to_le_bytes());
    stream.extend_from_slice(&(header_size as u32 + variables_offset as u32).to_le_bytes());
    stream.extend_from_slice(FOOTER_MAGIC);
    assert_eq!(stream.len(), variables_offset + 4 + 2 * 8 + 6);

    let mut bytes = Vec::new();
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&1_u32.to_le_bytes());
    bytes.extend_from_slice(&(header_size as u32).to_le_bytes());
    bytes.extend_from_slice(&(stream.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&(stream.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    assert_eq!(bytes.len(), header_size);
    bytes.extend_from_slice(&stream);
    bytes
}

#[test]
fn a_variable_table_that_includes_the_footer_words_still_decodes() {
    let bytes = footer_overlapping_variable_table();
    assert_eq!(
        Witcher3Codec.sniff(&bytes, &saveprism_core::SniffCtx::default()),
        saveprism_core::Confidence::HIGH
    );
    let decoded = decode(&bytes);
    assert_eq!(encode(&decoded), bytes);
}

#[test]
fn unchanged_fixture_writes_the_same_bytes() {
    let bytes = fixture();
    let decoded = decode(&bytes);
    assert_eq!(encode(&decoded), bytes);
}

#[test]
fn tree_exposes_the_confirmed_scalars() {
    let bytes = fixture();
    let decoded = decode(&bytes);
    let doc = &decoded.doc;

    assert_eq!(int_at(doc, "save_version"), 66);
    assert_eq!(int_at(doc, "game_version"), 29);
    assert_eq!(int_at(doc, "Variables.W3Player.Orens"), 1250);
    assert_eq!(int_at(doc, "Variables.W3Player.level"), 12);
    assert_eq!(text_at(doc, "Variables.W3Player.name"), "Geralt");
    assert_eq!(text_at(doc, "Variables.W3Player.latinText"), "caf\u{e9}");
    assert_eq!(
        text_at(doc, "Variables.W3Player.CInventoryComponent.SItem.itemName"),
        "Orens"
    );
    assert_eq!(
        int_at(doc, "Variables.W3Player.CInventoryComponent.SItem.quantity"),
        7
    );
    assert_eq!(
        int_at(doc, "Variables.W3Player.CInventoryComponent.quantities.1"),
        2
    );
    assert_eq!(int_at(doc, "Variables.GwintManager.cardCount"), 2);
    assert_eq!(int_at(doc, "Variables.GwintManager.collection.3"), 4);
    assert_eq!(
        int_at(doc, "Variables.InitialBuildIDBlock.seed"),
        18446744073709551615
    );
    assert_eq!(
        text_at(doc, "Variables.saveInfo.playthroughId"),
        "invented-playthrough"
    );
    assert!(matches!(
        at(doc, "Variables.facts.record").value,
        Value::Bytes(_)
    ));
    assert!(
        at(doc, "Variables.facts.record")
            .flags
            .contains(NodeFlags::READ_ONLY)
    );

    assert_eq!(hinted(&decoded, "Variables.W3Player.Orens"), "W3:Int32");
    assert_eq!(
        hinted(&decoded, "Variables.W3Player.name"),
        "W3:String/utf8"
    );
    assert_eq!(hinted(&decoded, "Variables.facts.record"), "W3:bytes");
}

#[test]
fn editing_orens_and_an_item_quantity_survives_a_round_trip() {
    let bytes = fixture();
    let mut decoded = decode(&bytes);
    set(
        &mut decoded,
        "Variables.W3Player.Orens",
        OwnedValue::Int(999_999),
    )
    .unwrap();
    set(
        &mut decoded,
        "Variables.W3Player.CInventoryComponent.SItem.quantity",
        OwnedValue::Int(42),
    )
    .unwrap();

    let written = encode(&decoded);
    assert_ne!(written, bytes);
    let again = decode(&written);
    assert_eq!(int_at(&again.doc, "Variables.W3Player.Orens"), 999_999);
    assert_eq!(
        int_at(
            &again.doc,
            "Variables.W3Player.CInventoryComponent.SItem.quantity"
        ),
        42
    );
}

#[test]
fn editing_other_scalar_kinds_survives_a_round_trip() {
    let bytes = fixture();
    let mut decoded = decode(&bytes);
    set(
        &mut decoded,
        "Variables.W3Player.name",
        OwnedValue::Str("Triss!".to_string()),
    )
    .unwrap();
    set(
        &mut decoded,
        "Variables.W3Player.weight",
        OwnedValue::Float(80.25),
    )
    .unwrap();
    set(
        &mut decoded,
        "Variables.W3Player.CInventoryComponent.isNewGamePlus",
        OwnedValue::Bool(false),
    )
    .unwrap();
    set(
        &mut decoded,
        "Variables.W3Player.CInventoryComponent.quantities.2",
        OwnedValue::Int(-9),
    )
    .unwrap();
    set(
        &mut decoded,
        "Variables.InitialBuildIDBlock.seed",
        OwnedValue::Int(7),
    )
    .unwrap();

    let again = decode(&encode(&decoded));
    assert_eq!(text_at(&again.doc, "Variables.W3Player.name"), "Triss!");
    assert!(matches!(
        at(&again.doc, "Variables.W3Player.weight").value,
        Value::Float { v, .. } if (v - 80.25).abs() < f64::EPSILON
    ));
    assert_eq!(
        at(
            &again.doc,
            "Variables.W3Player.CInventoryComponent.isNewGamePlus"
        )
        .value,
        Value::Bool(false)
    );
    assert_eq!(
        int_at(
            &again.doc,
            "Variables.W3Player.CInventoryComponent.quantities.2"
        ),
        -9
    );
    assert_eq!(int_at(&again.doc, "Variables.InitialBuildIDBlock.seed"), 7);
}

#[test]
fn changing_a_string_length_is_refused() {
    let bytes = fixture();
    let mut decoded = decode(&bytes);
    set(
        &mut decoded,
        "Variables.W3Player.name",
        OwnedValue::Str("Geralt of Rivia".to_string()),
    )
    .unwrap();
    let error = Witcher3Codec
        .encode(&decoded.doc, &decoded.params)
        .unwrap_err();
    assert_eq!(error.code(), W3_WRITE);
}

#[test]
fn bad_inputs_are_errors_not_panics() {
    assert_eq!(
        decode_with(b"", &Limits::default()).unwrap_err().code(),
        W3_NOT_SAVE
    );
    assert_eq!(
        decode_with(b"not a witcher save", &Limits::default())
            .unwrap_err()
            .code(),
        W3_NOT_SAVE
    );

    let bytes = fixture();
    assert_eq!(
        decode_with(&bytes[..bytes.len() / 2], &Limits::default())
            .unwrap_err()
            .code(),
        W3_PARSE
    );

    let mut corrupt = bytes.clone();
    corrupt[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(decode_with(&corrupt, &Limits::default()).is_err());
}

#[test]
fn a_declared_decompression_bomb_is_refused_before_allocating() {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&1_u32.to_le_bytes());
    bytes.extend_from_slice(&28_u32.to_le_bytes());
    let decoded_size = 32_u32 * 1024 * 1024;
    bytes.extend_from_slice(&1_u32.to_le_bytes());
    bytes.extend_from_slice(&decoded_size.to_le_bytes());
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.push(0);
    let error = decode_with(&bytes, &Limits::default()).unwrap_err();
    assert_eq!(error.code(), codes::CORE_LIMIT_DECODED);
}

#[test]
fn truncated_chunk_table_is_an_error() {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&2_u32.to_le_bytes());
    bytes.extend_from_slice(&40_u32.to_le_bytes());
    bytes.extend_from_slice(&[0_u8; 10]);
    let error = decode_with(&bytes, &Limits::default()).unwrap_err();
    assert_eq!(error.code(), W3_PARSE);
}

#[test]
fn a_bad_lz4_block_is_an_error() {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&1_u32.to_le_bytes());
    bytes.extend_from_slice(&28_u32.to_le_bytes());
    bytes.extend_from_slice(&5_u32.to_le_bytes());
    bytes.extend_from_slice(&100_u32.to_le_bytes());
    bytes.extend_from_slice(&33_u32.to_le_bytes());
    bytes.extend_from_slice(&[0xff; 5]);
    let error = decode_with(&bytes, &Limits::default()).unwrap_err();
    assert_eq!(error.code(), W3_PARSE);
}

#[test]
fn sniff_uses_the_magic_and_does_not_claim_other_save_files() {
    let bytes = fixture();
    let ctx = saveprism_core::SniffCtx::default();
    assert_eq!(
        Witcher3Codec.sniff(&bytes, &ctx),
        saveprism_core::Confidence::HIGH
    );
    assert_eq!(
        Witcher3Codec.sniff(b"not a save", &ctx),
        saveprism_core::Confidence::NONE
    );
}

#[test]
fn encode_needs_the_original_parameters() {
    let decoded = decode(&fixture());
    let empty = saveprism_core::Params::new();
    let error = Witcher3Codec.encode(&decoded.doc, &empty).unwrap_err();
    assert_eq!(error.code(), W3_WRITE);
}

#[test]
fn params_keep_the_container_facts() {
    let bytes = fixture();
    let decoded = decode(&bytes);
    assert_eq!(
        decoded.params.get(P_HEADER_SIZE),
        Some(&ParamValue::Int(40))
    );
    assert!(matches!(
        decoded.params.get(P_CHUNK_SIZES),
        Some(ParamValue::List(items)) if items.len() == 2
    ));
    assert_eq!(
        decoded.params.get(P_ORIGINAL),
        Some(&ParamValue::Bytes(bytes))
    );
}

proptest! {
    #[test]
    fn any_i32_orens_value_round_trips(value in prop::num::i32::ANY) {
        let bytes = fixture();
        let mut decoded = decode(&bytes);
        set(&mut decoded, "Variables.W3Player.Orens", OwnedValue::Int(i128::from(value))).unwrap();
        let again = decode(&encode(&decoded));
        prop_assert_eq!(int_at(&again.doc, "Variables.W3Player.Orens"), i128::from(value));
    }

    #[test]
    fn any_u64_seed_value_round_trips(value in prop::num::u64::ANY) {
        let bytes = fixture();
        let mut decoded = decode(&bytes);
        set(&mut decoded, "Variables.InitialBuildIDBlock.seed", OwnedValue::Int(i128::from(value))).unwrap();
        let again = decode(&encode(&decoded));
        prop_assert_eq!(int_at(&again.doc, "Variables.InitialBuildIDBlock.seed"), i128::from(value));
    }
}
