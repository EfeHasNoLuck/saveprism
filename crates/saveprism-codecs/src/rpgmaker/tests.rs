use std::io::Write as _;
use std::path::PathBuf;

use flate2::Compression;
use flate2::write::ZlibEncoder;
use saveprism_core::detect::{DetectOptions, detect};
use saveprism_core::guard::{Verdict, check_round_trip};
use saveprism_core::{
    Codec, Confidence, DecodeCtx, Layer, Limits, Params, Path, Registry, SniffCtx, codes,
};

use super::*;
use crate::layers::{LAY_ZLIB_INVALID, ZlibLayer};

fn limits() -> Limits {
    Limits::default()
}

/// A JSON text with enough variety that deflate output holds bytes above 0x7F.
fn sample_json() -> String {
    let mut state = 7_u32;
    let mut items = Vec::new();
    for i in 0..300 {
        state = state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        items.push(format!("{{\"id\":{i},\"n\":{}}}", (state >> 16) % 9973));
    }
    format!(
        "{{\"@\":\"Game_System\",\"party\":{{\"@c\":5,\"gold\":1250}},\"items\":[{}]}}",
        items.join(",")
    )
}

fn zlib_level_1(data: &[u8]) -> Vec<u8> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::new(1));
    encoder.write_all(data).unwrap();
    encoder.finish().unwrap()
}

fn mz_file() -> Vec<u8> {
    BinaryStringLayer
        .encode(&zlib_level_1(sample_json().as_bytes()), &Params::new())
        .unwrap()
}

#[test]
fn binary_string_round_trips_every_byte() {
    let all: Vec<u8> = (0..=255).collect();
    let text = BinaryStringLayer.encode(&all, &Params::new()).unwrap();
    // 128 ASCII bytes plus 128 two-byte characters.
    assert_eq!(text.len(), 128 + 2 * 128);
    assert!(std::str::from_utf8(&text).is_ok());

    let limits = limits();
    let mut ctx = DecodeCtx::new(&limits);
    let (bytes, params) = BinaryStringLayer.decode(&text, &mut ctx).unwrap();
    assert_eq!(bytes, all);
    assert_eq!(BinaryStringLayer.encode(&bytes, &params).unwrap(), text);
}

#[test]
fn binary_string_refuses_other_text() {
    let limits = limits();
    let mut ctx = DecodeCtx::new(&limits);
    // U+0101 is above 0xFF.
    let error = BinaryStringLayer
        .decode("a\u{101}".as_bytes(), &mut ctx)
        .unwrap_err();
    assert_eq!(error.code(), RPG_BINSTR_RANGE);
    let error = BinaryStringLayer
        .decode(&[0xFF, 0xFE], &mut ctx)
        .unwrap_err();
    assert_eq!(error.code(), RPG_BINSTR_UTF8);
}

#[test]
fn binary_string_sniff_wants_a_zlib_header() {
    let ctx = SniffCtx::default();
    assert_eq!(BinaryStringLayer.sniff(&mz_file(), &ctx), Confidence::HIGH);
    assert_eq!(
        BinaryStringLayer.sniff(b"{\"a\":1}", &ctx),
        Confidence::NONE
    );
    assert_eq!(BinaryStringLayer.sniff(b"", &ctx), Confidence::NONE);
    // Valid header, but a character above 0xFF.
    assert_eq!(
        BinaryStringLayer.sniff("x\u{1}\u{101}".as_bytes(), &ctx),
        Confidence::NONE
    );
    assert_eq!(
        BinaryStringLayer.sniff(&[0x78, 0x01, 0xFF], &ctx),
        Confidence::NONE
    );
}

#[test]
fn mz_chain_decodes_and_encodes_back_to_an_equal_document() {
    let limits = limits();
    let file = mz_file();

    let mut ctx = DecodeCtx::new(&limits);
    let (deflated, binstr_params) = BinaryStringLayer.decode(&file, &mut ctx).unwrap();
    let (json, zlib_params) = ZlibLayer.decode(&deflated, &mut ctx).unwrap();
    assert_eq!(json, sample_json().as_bytes());
    let first = JsonCodec.decode(&json, &mut ctx).unwrap();

    // Encode every step again, then decode the result.
    let json_again = JsonCodec.encode(&first.doc, &first.params).unwrap();
    let deflated_again = ZlibLayer.encode(&json_again, &zlib_params).unwrap();
    let file_again = BinaryStringLayer
        .encode(&deflated_again, &binstr_params)
        .unwrap();

    let mut ctx = DecodeCtx::new(&limits);
    let (deflated, _) = BinaryStringLayer.decode(&file_again, &mut ctx).unwrap();
    let (json, _) = ZlibLayer.decode(&deflated, &mut ctx).unwrap();
    let second = JsonCodec.decode(&json, &mut ctx).unwrap();
    assert!(first.doc.tree_eq(&second.doc));
}

#[test]
fn mz_file_is_detected_and_the_guard_does_not_say_different() {
    let mut reg = Registry::default();
    crate::register_all(&mut reg);
    let file = mz_file();
    let opts = DetectOptions {
        file_name: Some("file1.rmmzsave".to_string()),
        ..DetectOptions::default()
    };
    let detection = detect(&file, &reg, &limits(), &opts).unwrap();
    let recipe = &detection.best.recipe;
    assert_eq!(
        recipe.kinds(),
        [
            "encoding.js-binary-string",
            "compression.zlib",
            "format.rpgmaker-json"
        ]
    );
    // pako and miniz_oxide can differ, so "semantic" is acceptable; "different" is not.
    let report = check_round_trip(&file, recipe, &reg, &limits()).unwrap();
    assert!(matches!(report.overall, Verdict::Exact | Verdict::Semantic));
}

#[test]
fn damaged_mz_data_gives_a_layer_error() {
    let limits = limits();
    let mut ctx = DecodeCtx::new(&limits);
    let mut deflated = zlib_level_1(sample_json().as_bytes());
    let cut = deflated.len() / 2;
    deflated.truncate(cut);
    let error = ZlibLayer.decode(&deflated, &mut ctx).unwrap_err();
    assert_eq!(error.code(), LAY_ZLIB_INVALID);
}

fn meta_hint(doc: &SaveDoc, path: &str) -> Option<String> {
    let id = doc.resolve(&Path::parse(path).unwrap())?;
    doc.get(id)?
        .type_hint
        .as_ref()
        .map(|hint| hint.as_str().to_string())
}

#[test]
fn jsonex_meta_keys_get_the_hint_and_the_bytes_stay() {
    let text = br#"{"@":"Game_System","@c":5,"@a":[1,2],"x":{"@r":3,"hp":7,"mp":1.5}}"#;
    let limits = limits();
    let mut ctx = DecodeCtx::new(&limits);
    let decoded = RpgMakerJsonCodec.decode(text, &mut ctx).unwrap();
    let doc = &decoded.doc;

    for path in ["/@", "/@c", "/@a", "/x/@r"] {
        assert_eq!(meta_hint(doc, path).as_deref(), Some(JSONEX_META), "{path}");
    }
    assert_eq!(meta_hint(doc, "/x/hp").as_deref(), Some("JSON:int"));
    assert_eq!(meta_hint(doc, "/x/mp").as_deref(), Some("JSON:float"));
    assert_eq!(meta_hint(doc, "/x").as_deref(), None);

    assert_eq!(
        RpgMakerJsonCodec.encode(doc, &decoded.params).unwrap(),
        text
    );

    // The plain JSON codec gives an equal tree except for the hints.
    let mut ctx = DecodeCtx::new(&limits);
    let plain = JsonCodec.decode(text, &mut ctx).unwrap();
    assert_eq!(meta_hint(&plain.doc, "/@c").as_deref(), Some("JSON:int"));
}

#[test]
fn rpgmaker_json_codec_sniffs_only_jsonex_markers() {
    let mut reg = Registry::default();
    register(&mut reg);
    assert!(reg.is_layer("encoding.js-binary-string"));
    assert!(reg.is_codec("format.rpgmaker-json"));
    let ctx = SniffCtx::default();
    let json_best = Confidence::new(0.9).value();
    for input in [
        &b"{\"@c\":1}"[..],
        b"  {\"@\":\"Game_System\"}",
        b"[null,{\"a\":1,\"@\":\"Object\",\"@a\":[1]}]",
    ] {
        let score = RpgMakerJsonCodec.sniff(input, &ctx).value();
        assert!(score > json_best, "{score}");
    }
    for input in [&b"{\"a\":1}"[..], b"[1,2]", b"@c:", b"", b"GVAS\"@c\":"] {
        assert_eq!(RpgMakerJsonCodec.sniff(input, &ctx), Confidence::NONE);
    }
}

#[test]
fn detector_picks_rpgmaker_json_only_with_markers() {
    let mut reg = Registry::default();
    crate::register_all(&mut reg);
    let opts = DetectOptions::default();
    let marked = detect(sample_json().as_bytes(), &reg, &limits(), &opts).unwrap();
    assert_eq!(
        marked.best.recipe.kinds().last().copied(),
        Some("format.rpgmaker-json")
    );
    let plain = detect(br#"{"a":1,"b":[1,2]}"#, &reg, &limits(), &opts).unwrap();
    assert_eq!(
        plain.best.recipe.kinds().last().copied(),
        Some("format.json")
    );
}

#[test]
fn error_codes_are_stable() {
    assert_eq!(RPG_BINSTR_UTF8.as_str(), "SP-RPG-001");
    assert_eq!(RPG_BINSTR_RANGE.as_str(), "SP-RPG-002");
    assert_eq!(RPG_DATA_INVALID.as_str(), "SP-RPG-003");
    assert_eq!(RPG_META_FAILED.as_str(), "SP-RPG-004");
}

/// A fresh empty folder under the system temp folder.
fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("saveprism-rpg-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn name_dictionary_reads_the_data_files() {
    let dir = temp_dir("names");
    std::fs::write(
        dir.join("System.json"),
        r#"{"gameTitle":"T","switches":["","Boss dead","Door open"],"variables":["","Gold spent"]}"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("Actors.json"),
        r#"[null,{"id":1,"name":"Eric"},{"id":2,"name":"Lena"}]"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("Items.json"),
        r#"[null,{"id":1,"name":"Potion"},{"id":5,"name":""}]"#,
    )
    .unwrap();

    let names = NameDictionary::load(&dir, &limits()).unwrap();
    assert_eq!(names.switch(1), Some("Boss dead"));
    assert_eq!(names.switch(2), Some("Door open"));
    assert_eq!(names.switch(0), None);
    assert_eq!(names.variable(1), Some("Gold spent"));
    assert_eq!(names.actor(2), Some("Lena"));
    assert_eq!(names.item(1), Some("Potion"));
    assert_eq!(names.item(5), None);
    assert!(!names.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn name_dictionary_without_files_is_empty() {
    let dir = temp_dir("empty");
    let names = NameDictionary::load(&dir, &limits()).unwrap();
    assert!(names.is_empty());
    let missing = dir.join("does-not-exist");
    assert!(
        NameDictionary::load(&missing, &limits())
            .unwrap()
            .is_empty()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn name_dictionary_reports_broken_and_oversized_files() {
    let dir = temp_dir("broken");
    std::fs::write(dir.join("Actors.json"), "[{").unwrap();
    let error = NameDictionary::load(&dir, &limits()).unwrap_err();
    assert_eq!(error.code(), RPG_DATA_INVALID);

    std::fs::write(dir.join("Actors.json"), "[null]").unwrap();
    let tiny = Limits {
        max_input_bytes: 2,
        ..Limits::default()
    };
    let error = NameDictionary::load(&dir, &tiny).unwrap_err();
    assert_eq!(error.code(), codes::CORE_LIMIT_INPUT);
    let _ = std::fs::remove_dir_all(&dir);
}
