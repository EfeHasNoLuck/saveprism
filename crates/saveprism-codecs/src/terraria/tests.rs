//! Tests of the Terraria codecs and the player encryption layer.

use std::path::PathBuf;

use saveprism_core::detect::{DetectOptions, detect};
use saveprism_core::guard::{Verdict, check_round_trip};
use saveprism_core::{
    Codec, DecodeCtx, Decoded, Edit, Layer, Limits, NodeFlags, ParamValue, Params, Path, Registry,
    SaveDoc, Value, apply_edits,
};

use super::aes::{TerrariaPlayerLayer, player_password_unlocks_first_block};
use super::player::{P_ORIGINAL, TerrariaPlayerCodec};
use super::world::TerrariaWorldCodec;
use super::*;

const PLAYER: &str = "synthetic/terraria/player-decrypted.plr";
const WORLD: &str = "synthetic/terraria/world.wld";
/// A made-up key used only by these tests.
const TEST_KEY: &str = "synthKey";

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn fixture(relative: &str) -> Vec<u8> {
    std::fs::read(fixtures().join(relative)).unwrap()
}

fn decode(codec: &dyn Codec, bytes: &[u8]) -> Decoded {
    let limits = Limits::default();
    let mut ctx = DecodeCtx::new(&limits);
    codec.decode(bytes, &mut ctx).unwrap()
}

fn exact(codec: &dyn Codec, bytes: &[u8]) -> Decoded {
    let decoded = decode(codec, bytes);
    let encoded = codec.encode(&decoded.doc, &decoded.params).unwrap();
    assert_eq!(encoded, bytes, "re-encoded bytes differ");
    decoded
}

fn at<'a>(doc: &'a SaveDoc, path: &str) -> &'a saveprism_core::Node {
    let id = doc
        .resolve(&Path::parse(path).unwrap())
        .unwrap_or_else(|| panic!("no node at {path}"));
    doc.get(id).unwrap()
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

fn bool_at(doc: &SaveDoc, path: &str) -> bool {
    match at(doc, path).value {
        Value::Bool(value) => value,
        ref other => panic!("{path} is {other:?}"),
    }
}

fn bytes_at(doc: &SaveDoc, path: &str) -> Vec<u8> {
    match &at(doc, path).value {
        Value::Bytes(bytes) => bytes.clone(),
        other => panic!("{path} is {other:?}"),
    }
}

fn set(doc: &mut SaveDoc, path: &str, value: saveprism_core::OwnedValue) {
    apply_edits(
        doc,
        &[Edit::Set {
            path: Path::parse(path).unwrap(),
            value,
        }],
        &Limits::default(),
    )
    .unwrap();
}

fn registry() -> Registry {
    let mut reg = Registry::default();
    register(&mut reg);
    reg
}

// ----- player -----------------------------------------------------------------------------------

#[test]
fn the_player_fixture_round_trips_exactly() {
    let bytes = fixture(PLAYER);
    let decoded = exact(&TerrariaPlayerCodec, &bytes);
    let doc = &decoded.doc;
    assert_eq!(
        at(doc, "").type_hint.as_ref().map(|hint| hint.as_str()),
        Some("Terraria:player")
    );
    assert_eq!(int_at(doc, "/version"), 317);
    assert_eq!(text_at(doc, "/magic"), "relogic");
    assert_eq!(int_at(doc, "/file_type"), 3);
    assert_eq!(int_at(doc, "/revision"), 7);
    assert!(bool_at(doc, "/favorite"));
    assert_eq!(text_at(doc, "/name"), "Mira");
    assert_eq!(int_at(doc, "/difficulty"), 0);
    assert_eq!(int_at(doc, "/play_time_ticks"), 123_456_789);
    assert_eq!(int_at(doc, "/life"), 140);
    assert_eq!(int_at(doc, "/max_life"), 140);
    assert_eq!(int_at(doc, "/mana"), 60);
    assert_eq!(int_at(doc, "/max_mana"), 60);
    assert_eq!(int_at(doc, "/inventory/0/id"), 1);
    assert_eq!(int_at(doc, "/inventory/0/stack"), 1);
    assert_eq!(int_at(doc, "/inventory/0/prefix"), 0);
    assert_eq!(int_at(doc, "/inventory/2/id"), 73);
    assert_eq!(int_at(doc, "/inventory/2/stack"), 99);
    assert_eq!(int_at(doc, "/inventory/2/prefix"), 4);
    assert_eq!(int_at(doc, "/inventory/57/id"), 75);
    assert!(at(doc, "/tail").flags.contains(NodeFlags::READ_ONLY));
    assert!(
        decoded
            .params
            .get(P_ORIGINAL)
            .is_some_and(|value| matches!(value, ParamValue::Bytes(_)))
    );
}

#[test]
fn a_player_edit_patches_only_its_field_and_keeps_the_padding() {
    let bytes = fixture(PLAYER);
    let mut decoded = decode(&TerrariaPlayerCodec, &bytes);
    set(
        &mut decoded.doc,
        "/name",
        saveprism_core::OwnedValue::Str("Mira the Builder".into()),
    );
    set(
        &mut decoded.doc,
        "/favorite",
        saveprism_core::OwnedValue::Bool(false),
    );
    set(
        &mut decoded.doc,
        "/life",
        saveprism_core::OwnedValue::Int(120),
    );
    set(
        &mut decoded.doc,
        "/inventory/2/stack",
        saveprism_core::OwnedValue::Int(42),
    );
    let edited = TerrariaPlayerCodec
        .encode(&decoded.doc, &decoded.params)
        .unwrap();
    assert_eq!(edited.len() % 16, 0);
    let again = decode(&TerrariaPlayerCodec, &edited);
    assert_eq!(text_at(&again.doc, "/name"), "Mira the Builder");
    assert!(!bool_at(&again.doc, "/favorite"));
    assert_eq!(int_at(&again.doc, "/life"), 120);
    assert_eq!(int_at(&again.doc, "/inventory/2/stack"), 42);
    assert!(
        bytes_at(&again.doc, "/tail").starts_with(&bytes_at(&decoded.doc, "/tail")),
        "the original tail stays and the new padding is only appended"
    );
    let rebuilt = TerrariaPlayerCodec
        .encode(&again.doc, &again.params)
        .unwrap();
    assert_eq!(rebuilt, edited);
}

#[test]
fn a_too_long_player_name_is_refused() {
    let bytes = fixture(PLAYER);
    let mut decoded = decode(&TerrariaPlayerCodec, &bytes);
    set(
        &mut decoded.doc,
        "/name",
        saveprism_core::OwnedValue::Str("x".repeat(21)),
    );
    let error = TerrariaPlayerCodec
        .encode(&decoded.doc, &decoded.params)
        .unwrap_err();
    assert_eq!(error.code(), TERRARIA_WRITE);
}

#[test]
fn the_plain_player_is_detected_by_content() {
    let reg = registry();
    let bytes = fixture(PLAYER);
    let limits = Limits::default();
    let opts = DetectOptions {
        file_name: Some("player.plr".to_string()),
        ..DetectOptions::default()
    };
    let detection = detect(&bytes, &reg, &limits, &opts).expect("detected");
    assert_eq!(detection.best.recipe.kinds(), ["format.terraria-player"]);
    let report = check_round_trip(&bytes, &detection.best.recipe, &reg, &limits).unwrap();
    assert_eq!(report.overall, Verdict::Exact);
}

#[test]
fn random_bytes_are_not_a_player() {
    let reg = registry();
    let limits = Limits::default();
    let input = vec![0xA5_u8; 512];
    let opts = DetectOptions {
        file_name: Some("player.plr".to_string()),
        ..DetectOptions::default()
    };
    if let Ok(detection) = detect(&input, &reg, &limits, &opts) {
        assert!(
            !detection
                .best
                .recipe
                .kinds()
                .contains(&"format.terraria-player")
        );
    }
}

// ----- encryption layer -------------------------------------------------------------------------

fn encrypted_player(plain: &[u8]) -> Vec<u8> {
    let mut params = Params::new();
    params.set("password", TEST_KEY);
    TerrariaPlayerLayer.encode(plain, &params).unwrap()
}

#[test]
fn the_layer_decrypts_and_re_encrypts_the_player() {
    let plain = fixture(PLAYER);
    let encrypted = encrypted_player(&plain);
    assert_eq!(encrypted.len(), plain.len());
    assert!(player_password_unlocks_first_block(&encrypted, TEST_KEY));
    assert!(!player_password_unlocks_first_block(&encrypted, "wrongKey"));

    let mut hints = Params::new();
    hints.set("password", TEST_KEY);
    let limits = Limits::default();
    let mut ctx = DecodeCtx::new(&limits).with_step(0, hints);
    let (decrypted, _) = TerrariaPlayerLayer.decode(&encrypted, &mut ctx).unwrap();
    assert_eq!(decrypted, plain);

    let mut params = Params::new();
    params.set("password", TEST_KEY);
    assert_eq!(
        TerrariaPlayerLayer.encode(&plain, &params).unwrap(),
        encrypted
    );
}

#[test]
fn the_layer_needs_its_key() {
    let plain = fixture(PLAYER);
    let encrypted = encrypted_player(&plain);
    let limits = Limits::default();
    let mut ctx = DecodeCtx::new(&limits);
    let error = TerrariaPlayerLayer
        .decode(&encrypted, &mut ctx)
        .unwrap_err();
    assert_eq!(error.code(), TERRARIA_PASSWORD_REQUIRED);
}

#[test]
fn the_decrypted_layer_chain_round_trips_through_the_pipeline() {
    let reg = registry();
    let limits = Limits::default();
    let plain = fixture(PLAYER);
    let encrypted = encrypted_player(&plain);
    let mut params = Params::new();
    params.set("password", TEST_KEY);
    let recipe = saveprism_core::Recipe::new(vec![
        saveprism_core::Step::new("crypto.terraria-plr", params.clone()),
        saveprism_core::Step::bare("format.terraria-player"),
    ]);
    let out = saveprism_core::pipeline::decode(&encrypted, &recipe, &reg, &limits).unwrap();
    let mut encode_recipe = out.recipe.clone();
    if let Some(step) = encode_recipe.steps.first_mut() {
        step.params.set("password", TEST_KEY);
    }
    let rebuilt = saveprism_core::pipeline::encode(&out.doc, &encode_recipe, &reg).unwrap();
    assert_eq!(rebuilt, encrypted);
}

// ----- world ------------------------------------------------------------------------------------

#[test]
fn the_world_fixture_round_trips_exactly() {
    let bytes = fixture(WORLD);
    let decoded = exact(&TerrariaWorldCodec, &bytes);
    let doc = &decoded.doc;
    assert_eq!(
        at(doc, "").type_hint.as_ref().map(|hint| hint.as_str()),
        Some("Terraria:world")
    );
    assert_eq!(int_at(doc, "/version"), 326);
    assert_eq!(text_at(doc, "/magic"), "relogic");
    assert_eq!(int_at(doc, "/file_type"), 2);
    assert!(bool_at(doc, "/favorite"));
    assert_eq!(int_at(doc, "/section_count"), 11);
    assert_eq!(int_at(doc, "/section_pointers/0"), 73);
    assert_eq!(text_at(doc, "/header/name"), "Prism Hollow");
    assert_eq!(text_at(doc, "/header/seed"), "4829103");
    assert_eq!(int_at(doc, "/header/id"), 917_263);
    assert_eq!(int_at(doc, "/header/width"), 8400);
    assert_eq!(int_at(doc, "/header/height"), 1200);
    assert_eq!(int_at(doc, "/header/game_mode"), 2);
    assert!(bool_at(doc, "/header/zenith_world"));
    assert!(!bool_at(doc, "/header/skyblock_world"));
    assert!(bool_at(doc, "/header/crimson"));
    assert_eq!(int_at(doc, "/header/shadow_orb_count"), 3);
    assert_eq!(int_at(doc, "/header/altar_count"), 2);
    assert!(bool_at(doc, "/header/hard_mode"));
    assert_eq!(int_at(doc, "/header/spawn_x"), 4200);
    assert!(!bytes_at(doc, "/sections").is_empty());
    assert!(at(doc, "/sections").flags.contains(NodeFlags::READ_ONLY));
}

#[test]
fn a_longer_world_name_moves_the_section_pointers() {
    let bytes = fixture(WORLD);
    let mut decoded = decode(&TerrariaWorldCodec, &bytes);
    let old_first = int_at(&decoded.doc, "/section_pointers/0");
    let old_second = int_at(&decoded.doc, "/section_pointers/1");
    let sections = bytes_at(&decoded.doc, "/sections");
    set(
        &mut decoded.doc,
        "/header/name",
        saveprism_core::OwnedValue::Str("Prism Hollow Reborn".into()),
    );
    set(
        &mut decoded.doc,
        "/header/seed",
        saveprism_core::OwnedValue::Str("777".into()),
    );
    let edited = TerrariaWorldCodec
        .encode(&decoded.doc, &decoded.params)
        .unwrap();
    let again = decode(&TerrariaWorldCodec, &edited);
    assert_eq!(text_at(&again.doc, "/header/name"), "Prism Hollow Reborn");
    assert_eq!(text_at(&again.doc, "/header/seed"), "777");
    assert_eq!(int_at(&again.doc, "/section_pointers/0"), old_first);
    assert!(int_at(&again.doc, "/section_pointers/1") > old_second);
    assert_eq!(bytes_at(&again.doc, "/sections"), sections);
    let rebuilt = TerrariaWorldCodec
        .encode(&again.doc, &again.params)
        .unwrap();
    assert_eq!(rebuilt, edited);
}

#[test]
fn a_world_flag_edit_round_trips_without_moving_pointers() {
    let bytes = fixture(WORLD);
    let mut decoded = decode(&TerrariaWorldCodec, &bytes);
    let pointers: Vec<i128> = (0..11)
        .map(|index| int_at(&decoded.doc, &format!("/section_pointers/{index}")))
        .collect();
    set(
        &mut decoded.doc,
        "/header/skyblock_world",
        saveprism_core::OwnedValue::Bool(true),
    );
    set(
        &mut decoded.doc,
        "/header/blood_moon",
        saveprism_core::OwnedValue::Bool(true),
    );
    let edited = TerrariaWorldCodec
        .encode(&decoded.doc, &decoded.params)
        .unwrap();
    let again = decode(&TerrariaWorldCodec, &edited);
    assert!(bool_at(&again.doc, "/header/skyblock_world"));
    assert!(bool_at(&again.doc, "/header/blood_moon"));
    let again_pointers: Vec<i128> = (0..11)
        .map(|index| int_at(&again.doc, &format!("/section_pointers/{index}")))
        .collect();
    assert_eq!(again_pointers, pointers);
}

#[test]
fn the_world_is_detected_by_content_and_round_trips() {
    let reg = registry();
    let bytes = fixture(WORLD);
    let limits = Limits::default();
    let opts = DetectOptions {
        file_name: Some("world.wld".to_string()),
        ..DetectOptions::default()
    };
    let detection = detect(&bytes, &reg, &limits, &opts).expect("detected");
    assert_eq!(detection.best.recipe.kinds(), ["format.terraria-world"]);
    let report = check_round_trip(&bytes, &detection.best.recipe, &reg, &limits).unwrap();
    assert_eq!(report.overall, Verdict::Exact);
}

#[test]
fn a_world_does_not_claim_other_bytes() {
    let reg = registry();
    let limits = Limits::default();
    let input = vec![0_u8; 512];
    let opts = DetectOptions {
        file_name: Some("world.wld".to_string()),
        ..DetectOptions::default()
    };
    if let Ok(detection) = detect(&input, &reg, &limits, &opts) {
        assert!(
            !detection
                .best
                .recipe
                .kinds()
                .contains(&"format.terraria-world")
        );
    }
}

#[test]
fn an_older_player_version_is_refused_cleanly() {
    let mut bytes = fixture(PLAYER);
    if let Some(slot) = bytes.get_mut(..4) {
        slot.copy_from_slice(&120_i32.to_le_bytes());
    }
    let limits = Limits::default();
    let mut ctx = DecodeCtx::new(&limits);
    let error = TerrariaPlayerCodec.decode(&bytes, &mut ctx).unwrap_err();
    assert_eq!(error.code(), TERRARIA_UNSUPPORTED);
}
