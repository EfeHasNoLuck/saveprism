//! Tests of the Terraria codecs and the player encryption layer.

use std::path::PathBuf;

use saveprism_core::detect::{DetectOptions, detect};
use saveprism_core::guard::{Verdict, check_round_trip};
use saveprism_core::{
    Codec, Confidence, DecodeCtx, Decoded, Edit, IntWidth, Layer, Limits, NodeFlags, OwnedValue,
    ParamValue, Params, Path, Registry, SaveDoc, SniffCtx, Value, apply_edits, codes,
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
fn an_unpadded_player_reproduces_its_bytes() {
    // A plaintext that is not a whole number of AES blocks is never padded when nothing changed.
    let mut bytes = fixture(PLAYER);
    bytes.extend_from_slice(&[1, 2, 3]);
    assert_ne!(bytes.len() % 16, 0);
    exact(&TerrariaPlayerCodec, &bytes);
}

#[test]
fn only_a_difficulty_the_game_knows_is_written() {
    let bytes = fixture(PLAYER);
    let mut decoded = decode(&TerrariaPlayerCodec, &bytes);
    set(&mut decoded.doc, "/difficulty", OwnedValue::Int(4));
    let error = TerrariaPlayerCodec
        .encode(&decoded.doc, &decoded.params)
        .unwrap_err();
    assert_eq!(error.code(), TERRARIA_WRITE);
    assert!(
        error.message().contains("difficulty"),
        "{}",
        error.message()
    );

    set(&mut decoded.doc, "/difficulty", OwnedValue::Int(3));
    let edited = TerrariaPlayerCodec
        .encode(&decoded.doc, &decoded.params)
        .unwrap();
    let again = decode(&TerrariaPlayerCodec, &edited);
    assert_eq!(int_at(&again.doc, "/difficulty"), 3);
}

#[test]
fn only_an_item_id_the_game_can_load_is_written() {
    let bytes = fixture(PLAYER);
    for bad in [99_999, -49, i128::from(i32::MAX)] {
        let mut decoded = decode(&TerrariaPlayerCodec, &bytes);
        set(&mut decoded.doc, "/inventory/0/id", OwnedValue::Int(bad));
        let error = TerrariaPlayerCodec
            .encode(&decoded.doc, &decoded.params)
            .unwrap_err();
        assert_eq!(error.code(), TERRARIA_WRITE, "id {bad}");
        assert!(error.message().contains("item id"), "{}", error.message());
    }
    // An empty slot (0), a legacy negative id and a normal id are fine.
    for good in [0, -5, 4_000] {
        let mut decoded = decode(&TerrariaPlayerCodec, &bytes);
        set(&mut decoded.doc, "/inventory/0/id", OwnedValue::Int(good));
        let edited = TerrariaPlayerCodec
            .encode(&decoded.doc, &decoded.params)
            .unwrap();
        let again = decode(&TerrariaPlayerCodec, &edited);
        assert_eq!(int_at(&again.doc, "/inventory/0/id"), good);
    }
}

#[test]
fn a_player_value_the_file_already_holds_still_saves() {
    // The range checks apply to what was changed; an odd value that was already there is kept.
    let mut bytes = fixture(PLAYER);
    let difficulty = super::player::difficulty_offset(&bytes).unwrap();
    bytes[difficulty] = 9;
    let decoded = exact(&TerrariaPlayerCodec, &bytes);
    assert_eq!(int_at(&decoded.doc, "/difficulty"), 9);
}

#[test]
fn a_player_name_the_file_already_holds_still_saves() {
    // A name over the game's 20 characters (a mod, a newer game) is kept when it is not edited.
    let bytes = fixture(PLAYER);
    let difficulty = super::player::difficulty_offset(&bytes).unwrap();
    let name_start = difficulty - "Mira".len() - 1;
    let long = "x".repeat(25);
    let mut patched = bytes[..name_start].to_vec();
    patched.push(25);
    patched.extend_from_slice(long.as_bytes());
    patched.extend_from_slice(&bytes[difficulty..]);
    let mut decoded = exact(&TerrariaPlayerCodec, &patched);
    assert_eq!(text_at(&decoded.doc, "/name"), long);
    set(&mut decoded.doc, "/life", OwnedValue::Int(150));
    let edited = TerrariaPlayerCodec
        .encode(&decoded.doc, &decoded.params)
        .unwrap();
    assert_eq!(
        int_at(&decode(&TerrariaPlayerCodec, &edited).doc, "/life"),
        150
    );
}

#[test]
fn an_implausible_layout_is_flagged() {
    let bytes = fixture(PLAYER);
    let decoded = decode(&TerrariaPlayerCodec, &bytes);
    assert!(!layout_looks_wrong(&decoded.doc));
    for (path, value) in [
        ("/max_life", 0),
        ("/max_life", 5_000),
        ("/max_mana", -1),
        ("/max_mana", 2_000),
    ] {
        let mut decoded = decode(&TerrariaPlayerCodec, &bytes);
        set(&mut decoded.doc, path, OwnedValue::Int(value));
        assert!(layout_looks_wrong(&decoded.doc), "{path} = {value}");
    }
    // No mana at all is a real state, not a layout problem.
    let mut decoded = decode(&TerrariaPlayerCodec, &bytes);
    set(&mut decoded.doc, "/max_mana", OwnedValue::Int(0));
    assert!(!layout_looks_wrong(&decoded.doc));
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
fn the_games_backup_name_is_a_player_file_too() {
    assert!(is_player_file_name("Mira.plr"));
    assert!(is_player_file_name("Mira.PLR"));
    assert!(is_player_file_name("Mira.plr.bak"));
    assert!(is_player_file_name("Mira.Plr.BAK"));
    assert!(!is_player_file_name("Mira.bak"));
    assert!(!is_player_file_name("Mira.wld.bak"));

    let encrypted = encrypted_player(&fixture(PLAYER));
    let sniff = |name: &str| {
        TerrariaPlayerLayer.sniff(
            &encrypted,
            &SniffCtx {
                file_name: Some(name),
                depth: 0,
            },
        )
    };
    assert!(sniff("Mira.plr") > Confidence::NONE);
    assert!(sniff("Mira.plr.bak") > Confidence::NONE);
    assert_eq!(sniff("Mira.bak"), Confidence::NONE);
}

#[test]
fn only_a_key_of_exactly_eight_characters_is_accepted() {
    let plain = fixture(PLAYER);
    let encrypted = encrypted_player(&plain);
    let limits = Limits::default();
    for key in [
        "",
        "short",
        "sevenCh",
        "nineChars",
        "twelveChars1",
        "sixteenCharsKey!",
    ] {
        assert!(
            !player_password_unlocks_first_block(&encrypted, key),
            "{key}"
        );
        let mut hints = Params::new();
        hints.set("password", key);
        let mut ctx = DecodeCtx::new(&limits).with_step(0, hints);
        let error = TerrariaPlayerLayer
            .decode(&encrypted, &mut ctx)
            .unwrap_err();
        assert_eq!(error.code(), TERRARIA_WRONG_PASSWORD, "{key:?}");

        let mut params = Params::new();
        params.set("password", key);
        let error = TerrariaPlayerLayer.encode(&plain, &params).unwrap_err();
        assert_eq!(error.code(), TERRARIA_WRONG_PASSWORD, "{key:?}");
    }
    // The key is checked before the file, so a wrong length is never reported as damaged data.
    let mut hints = Params::new();
    hints.set("password", "short");
    let mut ctx = DecodeCtx::new(&limits).with_step(0, hints);
    let error = TerrariaPlayerLayer
        .decode(&encrypted[..20], &mut ctx)
        .unwrap_err();
    assert_eq!(error.code(), TERRARIA_WRONG_PASSWORD);
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
fn a_longer_seed_moves_the_section_pointers() {
    let bytes = fixture(WORLD);
    let mut decoded = decode(&TerrariaWorldCodec, &bytes);
    let old_first = int_at(&decoded.doc, "/section_pointers/0");
    let old_second = int_at(&decoded.doc, "/section_pointers/1");
    let sections = bytes_at(&decoded.doc, "/sections");
    set(
        &mut decoded.doc,
        "/header/seed",
        OwnedValue::Str("482910377777".into()),
    );
    let edited = TerrariaWorldCodec
        .encode(&decoded.doc, &decoded.params)
        .unwrap();
    let again = decode(&TerrariaWorldCodec, &edited);
    assert_eq!(text_at(&again.doc, "/header/seed"), "482910377777");
    assert_eq!(int_at(&again.doc, "/section_pointers/0"), old_first);
    assert!(int_at(&again.doc, "/section_pointers/1") > old_second);
    assert_eq!(bytes_at(&again.doc, "/sections"), sections);
    let rebuilt = TerrariaWorldCodec
        .encode(&again.doc, &again.params)
        .unwrap();
    assert_eq!(rebuilt, edited);
}

#[test]
fn the_world_name_id_and_size_are_read_only() {
    // The footer repeats the name and id, and the tile section's length depends on the size.
    let bytes = fixture(WORLD);
    for (path, value) in [
        (
            "/header/name",
            OwnedValue::Str("Prism Hollow Reborn".into()),
        ),
        ("/header/id", OwnedValue::Int(1)),
        ("/header/width", OwnedValue::Int(4200)),
        ("/header/height", OwnedValue::Int(900)),
        ("/header/left", OwnedValue::Int(0)),
        ("/header/right", OwnedValue::Int(0)),
        ("/header/top", OwnedValue::Int(0)),
        ("/header/bottom", OwnedValue::Int(0)),
    ] {
        let mut decoded = decode(&TerrariaWorldCodec, &bytes);
        assert!(
            at(&decoded.doc, path).flags.contains(NodeFlags::READ_ONLY),
            "{path}"
        );
        let error = apply_edits(
            &mut decoded.doc,
            &[Edit::Set {
                path: Path::parse(path).unwrap(),
                value,
            }],
            &Limits::default(),
        )
        .unwrap_err();
        assert_eq!(error.code(), codes::CORE_READ_ONLY, "{path}");
    }
}

#[test]
fn the_other_bits_of_the_world_flags_word_survive_a_save() {
    // Bit 0 is the favorite flag; bits 8 and 63 stand for flags a newer game may set.
    let mut bytes = fixture(WORLD);
    let word: u64 = 0x8000_0000_0000_0101;
    bytes[16..24].copy_from_slice(&word.to_le_bytes());
    let mut decoded = exact(&TerrariaWorldCodec, &bytes);
    assert!(bool_at(&decoded.doc, "/favorite"));
    set(&mut decoded.doc, "/favorite", OwnedValue::Bool(false));
    let edited = TerrariaWorldCodec
        .encode(&decoded.doc, &decoded.params)
        .unwrap();
    assert_eq!(edited.len(), bytes.len());
    assert_eq!(edited[16..24], (word & !1).to_le_bytes());
    assert_eq!(edited[..16], bytes[..16]);
    assert_eq!(edited[24..], bytes[24..]);
    let again = decode(&TerrariaWorldCodec, &edited);
    assert!(!bool_at(&again.doc, "/favorite"));
}

#[test]
fn an_implausible_world_size_is_flagged() {
    let bytes = fixture(WORLD);
    let decoded = decode(&TerrariaWorldCodec, &bytes);
    assert!(!layout_looks_wrong(&decoded.doc));
    for (key, value) in [
        ("width", 0),
        ("width", -8400),
        ("width", 10_001),
        ("height", 0),
        ("height", 123_456),
    ] {
        let mut decoded = decode(&TerrariaWorldCodec, &bytes);
        let path = format!("/header/{key}");
        let id = decoded.doc.resolve(&Path::parse(&path).unwrap()).unwrap();
        // The size is read-only for users; the check is about what a wrong layout would produce.
        decoded.doc.set_flags(id, NodeFlags::empty()).unwrap();
        let value = Value::Int {
            v: value,
            width: IntWidth::I32,
        };
        decoded.doc.set_value(id, value).unwrap();
        assert!(layout_looks_wrong(&decoded.doc), "{key}");
    }
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
