//! Tests of the Ruby Marshal codec.

use std::path::PathBuf;

use proptest::prelude::*;
use saveprism_core::detect::{DetectOptions, detect};
use saveprism_core::guard::{Verdict, check_round_trip};
use saveprism_core::{
    Codec, DecodeCtx, Decoded, Edit, Limits, Node, NodeFlags, NodeId, OwnedValue, ParamValue, Path,
    Position, Recipe, Registry, SaveDoc, SniffCtx, Step, Value, apply_edits, codes,
};

use super::write::{float_text, int_bytes, push_long};
use super::*;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn fixture(relative: &str) -> Vec<u8> {
    std::fs::read(fixtures().join(relative)).unwrap()
}

fn decode_with(bytes: &[u8], limits: &Limits) -> saveprism_core::Result<Decoded> {
    let mut ctx = DecodeCtx::new(limits);
    RubyMarshalCodec.decode(bytes, &mut ctx)
}

fn decode(bytes: &[u8]) -> Decoded {
    decode_with(bytes, &Limits::default()).unwrap()
}

fn encode(decoded: &Decoded) -> Vec<u8> {
    RubyMarshalCodec
        .encode(&decoded.doc, &decoded.params)
        .unwrap()
}

/// Decodes, checks that writing gives the same bytes, returns the decoded document.
fn exact(bytes: &[u8]) -> Decoded {
    let decoded = decode(bytes);
    let written = encode(&decoded);
    assert_eq!(written, bytes, "re-encoded bytes differ");
    decoded
}

/// One dump: the version header and `body`.
fn dump(body: &[u8]) -> Vec<u8> {
    let mut out = vec![4, 8];
    out.extend_from_slice(body);
    out
}

fn id_at(doc: &SaveDoc, path: &str) -> NodeId {
    doc.resolve(&Path::parse(path).unwrap())
        .unwrap_or_else(|| panic!("no node at {path}"))
}

fn at<'a>(doc: &'a SaveDoc, path: &str) -> &'a Node {
    doc.get(id_at(doc, path)).unwrap()
}

fn hint_of<'a>(doc: &'a SaveDoc, path: &str) -> &'a str {
    at(doc, path).type_hint.as_ref().unwrap().as_str()
}

fn int_at(doc: &SaveDoc, path: &str) -> i128 {
    match at(doc, path).value {
        Value::Int { v, .. } => v,
        ref other => panic!("{path} is {other:?}"),
    }
}

fn read_only(doc: &SaveDoc, path: &str) -> bool {
    at(doc, path).flags.contains(NodeFlags::READ_ONLY)
}

fn set(decoded: &mut Decoded, path: &str, value: OwnedValue) {
    apply_edits(
        &mut decoded.doc,
        &[Edit::Set {
            path: Path::parse(path).unwrap(),
            value,
        }],
        &Limits::default(),
    )
    .unwrap();
}

fn long(value: i64) -> Vec<u8> {
    let mut out = Vec::new();
    push_long(&mut out, value).unwrap();
    out
}

/// `:name` (a new symbol).
fn sym(name: &str) -> Vec<u8> {
    let mut out = vec![b':'];
    out.extend(long(name.len() as i64));
    out.extend_from_slice(name.as_bytes());
    out
}

/// `"text` (a string without encoding).
fn plain_str(text: &[u8]) -> Vec<u8> {
    let mut out = vec![b'"'];
    out.extend(long(text.len() as i64));
    out.extend_from_slice(text);
    out
}

fn all_fixtures() -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![
        fixtures().join("synthetic/rpgmaker-vxace"),
        fixtures().join("synthetic/rpgmaker-vx"),
        fixtures().join("synthetic/rpgmaker-xp"),
        // Real saves kept locally only (git-ignored); missing folders are skipped.
        fixtures().join("external/rpgmaker-xp"),
        fixtures().join("external/rpgmaker-vx"),
        fixtures().join("external/rpgmaker-vxace"),
    ];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| ["rxdata", "rvdata", "rvdata2"].contains(&e))
            {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

// ----- fixtures ---------------------------------------------------------------------------------

#[test]
fn every_fixture_round_trips_exactly() {
    let files = all_fixtures();
    assert!(files.len() >= 3, "{files:?}");
    for path in files {
        let bytes = std::fs::read(&path).unwrap();
        let decoded = decode_with(&bytes, &Limits::default())
            .unwrap_or_else(|error| panic!("{}: {}", path.display(), error.report()));
        let written = RubyMarshalCodec
            .encode(&decoded.doc, &decoded.params)
            .unwrap();
        assert!(written == bytes, "{} does not round-trip", path.display());
    }
}

#[test]
fn vxace_save_reads_like_the_game_objects() {
    let decoded = exact(&fixture("synthetic/rpgmaker-vxace/Save01.rvdata2"));
    let doc = &decoded.doc;
    assert!(decoded.params.is_empty(), "{:?}", decoded.params);
    assert_eq!(hint_of(doc, ""), "RUBY:dumps");
    assert_eq!(doc.children(doc.root()).len(), 2);

    // Header: a hash of symbols reads as an object.
    assert_eq!(hint_of(doc, "/0"), "RUBY:hash");
    assert_eq!(
        at(doc, "/0/playtime_s").value,
        Value::Str("01:24:00".into())
    );
    assert_eq!(hint_of(doc, "/0/playtime_s"), "RUBY:string/utf8");

    // Contents: objects keep their class and their instance variables.
    assert_eq!(hint_of(doc, "/1/party"), "RUBY:object:Game_Party");
    assert_eq!(ruby_class(at(doc, "/1/party")), Some("Game_Party"));
    assert_eq!(int_at(doc, "/1/party/@gold"), 1250);
    assert_eq!(hint_of(doc, "/1/party/@gold"), "RUBY:int");
    assert!(!read_only(doc, "/1/party/@gold"));
    assert_eq!(int_at(doc, "/1/party/@steps"), 3456);
    assert_eq!(int_at(doc, "/1/party/@actors/1"), 2);

    // A hash with integer keys is a map: item id 1 -> 5.
    let items = at(doc, "/1/party/@items");
    let Value::Map(entries) = &items.value else {
        panic!("{:?}", items.value)
    };
    assert_eq!(entries.len(), 2);
    assert!(matches!(
        doc.get(entries[0].0).unwrap().value,
        Value::Int { v: 1, .. }
    ));
    assert_eq!(int_at(doc, "/1/party/@items/0"), 5);
    assert!(read_only(doc, "/1/party/@items/0.key"));

    // Variables, switches.
    assert_eq!(hint_of(doc, "/1/variables"), "RUBY:object:Game_Variables");
    assert_eq!(int_at(doc, "/1/variables/@data/1"), 10);
    assert_eq!(int_at(doc, "/1/variables/@data/4"), 1 << 40);
    assert_eq!(int_at(doc, "/1/variables/@data/5"), -(1 << 35));
    assert_eq!(
        at(doc, "/1/variables/@data/6").value,
        Value::Float {
            v: 1.5,
            width: saveprism_core::FloatWidth::F64
        }
    );
    assert_eq!(at(doc, "/1/switches/@data/1").value, Value::Bool(true));
    assert_eq!(at(doc, "/1/switches/@data/2").value, Value::Bool(false));
    assert_eq!(at(doc, "/1/switches/@data/3").value, Value::Null);
    assert!(read_only(doc, "/1/switches/@data/3"));

    // Self switches: array keys in a map.
    let keys = at(doc, "/1/self_switches/@data");
    assert!(matches!(keys.value, Value::Map(_)));
    assert_eq!(hint_of(doc, "/1/self_switches/@data/0.key"), "RUBY:array");

    // Strings, symbols, links.
    assert_eq!(
        at(doc, "/1/actors/@data/1/@name").value,
        Value::Str("Eric".into())
    );
    assert!(!read_only(doc, "/1/actors/@data/1/@name"));
    assert_eq!(
        at(doc, "/1/actors/@data/2/@nickname").value,
        Value::Str("Ünal's friend ✦ ナタリー".into())
    );
    assert_eq!(
        at(doc, "/1/screen/@weather_type").value,
        Value::Str("none".into())
    );
    assert_eq!(hint_of(doc, "/1/screen/@weather_type"), "RUBY:symbol");
    assert!(read_only(doc, "/1/screen/@weather_type"));
    let link = at(doc, "/1/player/@character_name");
    assert_eq!(
        link.value,
        Value::Ref(id_at(doc, "/1/actors/@data/1/@character_name"))
    );
    assert!(link.flags.contains(NodeFlags::READ_ONLY));

    // _dump data and class references.
    assert_eq!(hint_of(doc, "/1/screen/@tone"), "RUBY:userdef:Tone");
    assert!(matches!(&at(doc, "/1/screen/@tone").value, Value::Bytes(b) if b.len() == 32));
    assert!(read_only(doc, "/1/screen/@tone"));
    assert_eq!(hint_of(doc, "/1/screen/@flash_color"), "RUBY:userdef:Color");
    assert_eq!(hint_of(doc, "/1/map/@map/@data"), "RUBY:userdef:Table");
    assert_eq!(
        at(doc, "/1/actors/@data/1/@equips/0/@class").value,
        Value::Str("RPG::Weapon".into())
    );
    assert_eq!(
        hint_of(doc, "/1/actors/@data/1/@equips/0/@class"),
        "RUBY:class"
    );

    // Floats keep Ruby's text.
    assert_eq!(
        at(doc, "/1/map/@display_x").value,
        Value::Float {
            v: 3.5,
            width: saveprism_core::FloatWidth::F64
        }
    );
    assert!(at(doc, "/1/map/@display_x").lexeme.is_none());

    // Containers cannot change shape.
    assert!(read_only(doc, "/1/party"));
    assert!(read_only(doc, "/1/party/@actors"));
    assert!(read_only(doc, ""));
}

#[test]
fn xp_save_is_a_list_of_dumps_without_encodings() {
    let decoded = exact(&fixture("synthetic/rpgmaker-xp/Save1.rxdata"));
    let doc = &decoded.doc;
    assert_eq!(doc.children(doc.root()).len(), 12);
    assert_eq!(int_at(doc, "/1"), 123_456);
    assert_eq!(hint_of(doc, "/8"), "RUBY:object:Game_Party");
    assert_eq!(int_at(doc, "/8/@gold"), 5000);
    assert_eq!(hint_of(doc, "/7/@data/1/@name"), "RUBY:string");
    assert_eq!(
        at(doc, "/7/@data/1/@name").value,
        Value::Str("Aluxes".into())
    );
    assert!(!read_only(doc, "/7/@data/1/@name"));
    assert_eq!(int_at(doc, "/4/@data/4"), 1 << 33);
}

#[test]
fn script_data_covers_the_rare_types() {
    let decoded = exact(&fixture("synthetic/rpgmaker-vxace/Save02.rvdata2"));
    let doc = &decoded.doc;
    let base = "/1/script_data";
    assert_eq!(hint_of(doc, &format!("{base}/point")), "RUBY:struct:Point");
    assert_eq!(int_at(doc, &format!("{base}/point/y")), -2);
    assert_eq!(
        hint_of(doc, &format!("{base}/custom")),
        "RUBY:usermarshal:Custom_Data"
    );
    assert_eq!(int_at(doc, &format!("{base}/custom/(data)/0")), 1);
    assert_eq!(
        hint_of(doc, &format!("{base}/extended")),
        "RUBY:extended:Comparable"
    );
    assert_eq!(
        hint_of(doc, &format!("{base}/extended/(value)")),
        "RUBY:object:Object"
    );
    assert_eq!(
        hint_of(doc, &format!("{base}/my_list")),
        "RUBY:userclass:My_List"
    );
    assert_eq!(
        hint_of(doc, &format!("{base}/pattern")),
        "RUBY:regexp/utf8:1"
    );
    assert_eq!(hint_of(doc, &format!("{base}/module")), "RUBY:module");
    assert_eq!(hint_of(doc, &format!("{base}/counts")), "RUBY:hash+default");
    assert_eq!(int_at(doc, &format!("{base}/counts/slime")), 3);
    assert_eq!(int_at(doc, &format!("{base}/counts/(default)")), 0);
    assert_eq!(hint_of(doc, &format!("{base}/by_id")), "RUBY:hash+default");
    assert_eq!(hint_of(doc, &format!("{base}/by_id/1.key")), "RUBY:default");
    assert_eq!(hint_of(doc, &format!("{base}/sjis")), "RUBY:ivars");
    assert!(matches!(
        at(doc, &format!("{base}/sjis/(value)")).value,
        Value::Bytes(_)
    ));
    assert!(read_only(doc, &format!("{base}/sjis/encoding")));
    assert_eq!(hint_of(doc, &format!("{base}/ascii")), "RUBY:string/ascii");
    assert!(!read_only(doc, &format!("{base}/ascii")));
    assert!(read_only(doc, &format!("{base}/binary")));
    assert_eq!(
        at(doc, &format!("{base}/odd_float")).lexeme.as_deref(),
        Some("2.50")
    );
    assert_eq!(
        at(doc, &format!("{base}/名前")).value,
        Value::Str("勇者".into())
    );
    // The cyclic array holds a link to itself.
    let lp = id_at(doc, &format!("{base}/loop"));
    assert_eq!(at(doc, &format!("{base}/loop/0")).value, Value::Ref(lp));
    // A linked float and a linked Bignum.
    let first = id_at(doc, &format!("{base}/floats/0"));
    assert_eq!(
        at(doc, &format!("{base}/floats/1")).value,
        Value::Ref(first)
    );
    assert!(read_only(doc, &format!("{base}/bigs/0")));
    assert!(matches!(
        decoded.params.get("symbol_utf8"),
        Some(ParamValue::List(items)) if items.len() == 2
    ));
}

#[test]
fn detection_and_guard_pick_the_codec() {
    let mut reg = Registry::default();
    crate::register_all(&mut reg);
    let limits = Limits::default();
    for name in [
        "synthetic/rpgmaker-vxace/Save01.rvdata2",
        "synthetic/rpgmaker-xp/Save1.rxdata",
    ] {
        let bytes = fixture(name);
        let options = DetectOptions {
            file_name: Some(name.rsplit('/').next().unwrap().to_string()),
            ..DetectOptions::default()
        };
        let found = detect(&bytes, &reg, &limits, &options).unwrap();
        assert_eq!(found.best.recipe.kinds(), vec!["format.ruby-marshal"]);
        let report = check_round_trip(&bytes, &found.best.recipe, &reg, &limits).unwrap();
        assert_eq!(report.overall, Verdict::Exact);
        assert!(report.editable);
    }
}

// ----- edits ------------------------------------------------------------------------------------

#[test]
fn edited_gold_is_written_and_read_back() {
    let original = fixture("synthetic/rpgmaker-vxace/Save01.rvdata2");
    let mut decoded = decode(&original);
    set(&mut decoded, "/1/party/@gold", OwnedValue::Int(999_999));
    let written = encode(&decoded);
    assert_ne!(written, original);
    let again = exact(&written);
    assert_eq!(int_at(&again.doc, "/1/party/@gold"), 999_999);
    assert_eq!(int_at(&again.doc, "/1/party/@steps"), 3456);

    // Through the pipeline too (the path the desktop app takes).
    let mut reg = Registry::default();
    crate::register_all(&mut reg);
    let recipe = Recipe::new(vec![Step::bare("format.ruby-marshal")]);
    let patched = RubyMarshalCodec
        .patch(
            &original,
            &[Edit::Set {
                path: Path::parse("/1/party/@gold").unwrap(),
                value: OwnedValue::Int(7),
            }],
            &decoded.params,
            &Limits::default(),
        )
        .unwrap();
    let out =
        saveprism_core::pipeline::decode(&patched, &recipe, &reg, &Limits::default()).unwrap();
    assert_eq!(int_at(&out.doc, "/1/party/@gold"), 7);
}

#[test]
fn an_integer_can_grow_into_a_bignum_and_links_stay_right() {
    let mut decoded = decode(&fixture("synthetic/rpgmaker-vxace/Save01.rvdata2"));
    // @gold comes before the player's link to the actor's sprite name: the Bignum adds an object
    // in between, so every later link index moves by one.
    set(&mut decoded, "/1/party/@gold", OwnedValue::Int(1 << 31));
    let again = exact(&encode(&decoded));
    assert_eq!(int_at(&again.doc, "/1/party/@gold"), 1 << 31);
    let target = id_at(&again.doc, "/1/actors/@data/1/@character_name");
    assert_eq!(
        at(&again.doc, "/1/player/@character_name").value,
        Value::Ref(target)
    );

    // And back to a Fixnum.
    let mut back = again;
    set(&mut back, "/1/party/@gold", OwnedValue::Int(-5));
    let last = exact(&encode(&back));
    assert_eq!(int_at(&last.doc, "/1/party/@gold"), -5);
    assert_eq!(
        last.doc
            .get(id_at(&last.doc, "/1/player/@character_name"))
            .unwrap()
            .value,
        Value::Ref(id_at(&last.doc, "/1/actors/@data/1/@character_name"))
    );
}

#[test]
fn strings_floats_and_booleans_can_be_edited() {
    let mut decoded = decode(&fixture("synthetic/rpgmaker-vxace/Save01.rvdata2"));
    set(
        &mut decoded,
        "/1/actors/@data/1/@name",
        OwnedValue::Str("Ériç ⚔".into()),
    );
    set(&mut decoded, "/1/map/@display_x", OwnedValue::Float(4.25));
    set(
        &mut decoded,
        "/1/variables/@data/8",
        OwnedValue::Float(100.0),
    );
    set(&mut decoded, "/1/switches/@data/2", OwnedValue::Bool(true));
    let written = encode(&decoded);
    assert!(written.windows(5).any(|w| w == b"\x094.25"));
    assert!(written.windows(4).any(|w| w == b"\x081e2"));
    let again = exact(&written);
    assert_eq!(
        at(&again.doc, "/1/actors/@data/1/@name").value,
        Value::Str("Ériç ⚔".into())
    );
    assert_eq!(
        hint_of(&again.doc, "/1/actors/@data/1/@name"),
        "RUBY:string/utf8"
    );
    assert_eq!(
        at(&again.doc, "/1/switches/@data/2").value,
        Value::Bool(true)
    );
}

#[test]
fn read_only_nodes_and_shapes_cannot_change() {
    let mut decoded = decode(&fixture("synthetic/rpgmaker-vxace/Save02.rvdata2"));
    let limits = Limits::default();
    let refused = |doc: &mut SaveDoc, edit: Edit| {
        let error = apply_edits(doc, &[edit], &limits).unwrap_err();
        assert_eq!(error.code(), codes::CORE_READ_ONLY);
    };
    let path = |text: &str| Path::parse(text).unwrap();
    for target in [
        "/1/switches/@data/3",
        "/1/screen/@weather_type",
        "/1/player/@character_name",
        "/1/screen/@tone",
        "/1/script_data/binary",
        "/1/script_data/pattern",
        "/1/script_data/class",
        "/1/party/@items/0.key",
    ] {
        refused(
            &mut decoded.doc,
            Edit::Set {
                path: path(target),
                value: OwnedValue::Int(1),
            },
        );
    }
    refused(
        &mut decoded.doc,
        Edit::Insert {
            parent: path("/1/party/@actors"),
            at: Position::End,
            value: OwnedValue::Int(3),
        },
    );
    refused(
        &mut decoded.doc,
        Edit::Remove {
            path: path("/1/party/@gold"),
        },
    );
    refused(
        &mut decoded.doc,
        Edit::RenameKey {
            path: path("/1/party/@gold"),
            new_key: "@money".into(),
        },
    );

    // A value that changes its type cannot be written.
    set(
        &mut decoded,
        "/1/party/@gold",
        OwnedValue::Str("lots".into()),
    );
    let error = RubyMarshalCodec
        .encode(&decoded.doc, &decoded.params)
        .unwrap_err();
    assert_eq!(error.code(), RUBY_WRITE);
}

#[test]
fn ascii_strings_stay_ascii() {
    let mut decoded = decode(&fixture("synthetic/rpgmaker-vxace/Save02.rvdata2"));
    set(
        &mut decoded,
        "/1/script_data/ascii",
        OwnedValue::Str("still plain".into()),
    );
    exact(&encode(&decoded));
    set(
        &mut decoded,
        "/1/script_data/ascii",
        OwnedValue::Str("não".into()),
    );
    let error = RubyMarshalCodec
        .encode(&decoded.doc, &decoded.params)
        .unwrap_err();
    assert_eq!(error.code(), RUBY_WRITE);
}

// ----- single types -----------------------------------------------------------------------------

#[test]
fn long_encoding_matches_the_spec_examples() {
    let cases: [(i64, &[u8]); 12] = [
        (0, &[0x00]),
        (1, &[0x06]),
        (122, &[0x7f]),
        (123, &[0x01, 0x7b]),
        (255, &[0x01, 0xff]),
        (256, &[0x02, 0x00, 0x01]),
        (-1, &[0xfa]),
        (-123, &[0x80]),
        (-124, &[0xff, 0x84]),
        (-256, &[0xff, 0x00]),
        (-257, &[0xfe, 0xff, 0xfe]),
        ((1 << 30) - 1, &[0x04, 0xff, 0xff, 0xff, 0x3f]),
    ];
    for (value, bytes) in cases {
        assert_eq!(long(value), bytes, "{value}");
        let mut body = vec![b'i'];
        body.extend_from_slice(bytes);
        let decoded = exact(&dump(&body));
        assert_eq!(int_at(&decoded.doc, "/0"), i128::from(value));
    }
    assert!(push_long(&mut Vec::new(), 1 << 32).is_err());
    assert!(push_long(&mut Vec::new(), -(1 << 32)).is_ok());
}

#[test]
fn integers_use_fixnum_or_bignum_like_ruby() {
    for value in [
        0_i128,
        -(1 << 30),
        (1 << 30) - 1,
        1 << 30,
        -(1 << 30) - 1,
        1 << 62,
        -(1 << 64),
        i128::MAX,
        i128::MIN,
    ] {
        let bytes = dump(&int_bytes(value));
        let decoded = exact(&bytes);
        assert_eq!(int_at(&decoded.doc, "/0"), value);
        assert!(at(&decoded.doc, "/0").lexeme.is_none(), "{value}");
    }
    assert_eq!(int_bytes(1 << 30), b"l+\x07\x00\x00\x00\x40");
}

#[test]
fn non_canonical_integers_keep_their_bytes_until_edited() {
    for body in [
        &b"i\x01\x05"[..],              // 5 in two bytes
        &b"i\x04\x00\x00\x00\x80"[..],  // 2^31 as a Fixnum (Ruby writes a Bignum)
        &b"l+\x07\x05\x00\x00\x00"[..], // 5 as a Bignum with a zero short
        &b"l-\x00"[..],                 // Bignum minus zero
    ] {
        let mut decoded = exact(&dump(body));
        assert!(
            at(&decoded.doc, "/0")
                .lexeme
                .as_deref()
                .unwrap()
                .starts_with("raw:")
        );
        set(&mut decoded, "/0", OwnedValue::Int(5));
        assert_eq!(encode(&decoded), dump(b"i\x0a"));
    }
}

#[test]
fn huge_bignums_are_read_only_bytes() {
    let mut body = b"l+".to_vec();
    body.extend(long(10));
    body.extend((1..=20).map(|n| n as u8));
    let decoded = exact(&dump(&body));
    assert_eq!(hint_of(&decoded.doc, "/0"), "RUBY:bignum");
    assert!(read_only(&decoded.doc, "/0"));
}

#[test]
fn float_text_follows_ruby() {
    let cases = [
        (1.5, "1.5"),
        (12.0, "12"),
        (100.0, "1e2"),
        (0.1, "0.1"),
        (0.001, "0.001"),
        (0.0001, "0.0001"),
        (0.00001, "1e-5"),
        (1e20, "1e20"),
        (-2.75, "-2.75"),
        (1.0 / 3.0, "0.3333333333333333"),
        (0.0, "0"),
        (-0.0, "-0"),
        (f64::INFINITY, "inf"),
        (f64::NEG_INFINITY, "-inf"),
        (f64::NAN, "nan"),
    ];
    for (value, text) in cases {
        assert_eq!(float_text(value), text, "{value}");
        let mut body = vec![b'f'];
        body.extend(long(text.len() as i64));
        body.extend_from_slice(text.as_bytes());
        let decoded = exact(&dump(&body));
        assert!(at(&decoded.doc, "/0").lexeme.is_none(), "{text}");
    }
}

#[test]
fn float_text_and_mantissa_bytes_round_trip() {
    // Ruby 1.8 style: "%.16g" text, a NUL, extra mantissa bytes.
    let body = b"[\x07f\x0e0.1\x00\x12\x34\x56\x78\xaaf\x091.50";
    let mut decoded = exact(&dump(body));
    assert_eq!(at(&decoded.doc, "/0/0").lexeme.as_deref(), Some("0.1"));
    assert_eq!(at(&decoded.doc, "/0/1").lexeme.as_deref(), Some("1.50"));
    assert!(decoded.params.contains("float_tails"));
    // Editing the first float drops its text and its extra bytes; the second keeps its text.
    set(&mut decoded, "/0/0", OwnedValue::Float(0.25));
    assert_eq!(encode(&decoded), dump(b"[\x07f\x090.25f\x091.50"));
    // A float that is not a number is an error, not a panic.
    let error = decode_with(&dump(b"f\x08abc"), &Limits::default()).unwrap_err();
    assert_eq!(error.code(), RUBY_PARSE);
}

#[test]
fn strings_with_and_without_encodings() {
    // UTF-8, US-ASCII, binary, and an encoding name (generic instance variables).
    let body = b"[\x09I\"\x07\xc3\xa9\x06:\x06ETI\"\x06a\x06;\x00F\"\x06\xff\
I\"\x07\x83\x41\x06:\x0dencoding\"\x0eShift_JIS";
    let decoded = exact(&dump(body));
    let doc = &decoded.doc;
    assert_eq!(hint_of(doc, "/0/0"), "RUBY:string/utf8");
    assert_eq!(at(doc, "/0/0").value, Value::Str("é".into()));
    assert_eq!(hint_of(doc, "/0/1"), "RUBY:string/ascii");
    assert_eq!(hint_of(doc, "/0/2"), "RUBY:string");
    assert_eq!(at(doc, "/0/2").value, Value::Bytes(vec![0xff]));
    assert!(read_only(doc, "/0/2"));
    assert_eq!(hint_of(doc, "/0/3"), "RUBY:ivars");
    assert!(read_only(doc, "/0/3/(value)"));
    assert_eq!(
        at(doc, "/0/3/encoding").value,
        Value::Str("Shift_JIS".into())
    );
    // A string marked UTF-8 whose bytes are not UTF-8, and a US-ASCII one with other bytes:
    // read-only, written back as they were (found by the property test).
    let broken = exact(&dump(
        b"[\x07I\"\x06\x80\x06:\x06ETI\"\x07\xc3\xa9\x06;\x00F",
    ));
    assert!(read_only(&broken.doc, "/0/0"));
    assert!(read_only(&broken.doc, "/0/1"));
    // `:E` with a value that is not true/false is an ordinary instance variable.
    let odd = exact(&dump(b"I\"\x06x\x06:\x06Ei\x06"));
    assert_eq!(hint_of(&odd.doc, "/0"), "RUBY:ivars");
    assert_eq!(int_at(&odd.doc, "/0/E"), 1);
}

#[test]
fn symbols_links_and_their_odd_forms() {
    // [:a, :a, :b, ;0] canonical.
    let decoded = exact(&dump(b"[\x09:\x06a;\x00:\x06b;\x00"));
    assert!(decoded.params.is_empty());
    assert_eq!(at(&decoded.doc, "/0/3").value, Value::Str("a".into()));

    // [:a, :a (written again), ;1 (the second definition)]: kept through the parameters.
    let odd = exact(&dump(b"[\x08:\x06a:\x06a;\x06"));
    assert!(odd.params.contains("symbol_redefs"));
    assert!(odd.params.contains("symbol_links"));

    // A UTF-8 symbol with :E.
    let utf8 = exact(&dump(b"I:\x07\xc3\xa9\x06:\x06ET"));
    assert_eq!(at(&utf8.doc, "/0").value, Value::Str("é".into()));
    assert!(utf8.params.contains("symbol_utf8"));

    // A symbol link out of range, a symbol that is not UTF-8, an unknown symbol encoding.
    let limits = Limits::default();
    assert_eq!(
        decode_with(&dump(b";\x06"), &limits).unwrap_err().code(),
        RUBY_BAD_LINK
    );
    assert_eq!(
        decode_with(&dump(b":\x06\xff"), &limits)
            .unwrap_err()
            .code(),
        RUBY_UNSUPPORTED
    );
    assert_eq!(
        decode_with(&dump(b"I:\x06a\x06:\x0dencoding\"\x06x"), &limits)
            .unwrap_err()
            .code(),
        RUBY_UNSUPPORTED
    );
}

#[test]
fn object_links_resolve_and_cycles_stay_references() {
    // [s = "x", s] and a = [a].
    let decoded = exact(&dump(b"[\x07\"\x06x@\x06"));
    let first = id_at(&decoded.doc, "/0/0");
    assert_eq!(at(&decoded.doc, "/0/1").value, Value::Ref(first));
    let cyclic = exact(&dump(b"[\x06@\x00"));
    let outer = id_at(&cyclic.doc, "/0");
    assert_eq!(at(&cyclic.doc, "/0/0").value, Value::Ref(outer));
    assert!(cyclic.doc.tree_eq(&decode(&dump(b"[\x06@\x00")).doc));
    // A link to an object that does not exist yet.
    let error = decode_with(&dump(b"[\x06@\x06"), &Limits::default()).unwrap_err();
    assert_eq!(error.code(), RUBY_BAD_LINK);
    // Fixnums and symbols are not objects: [1, :a, @1] has no object 1.
    let error = decode_with(&dump(b"[\x08i\x06:\x06a@\x06"), &Limits::default()).unwrap_err();
    assert_eq!(error.code(), RUBY_BAD_LINK);
}

#[test]
fn dump_data_inside_instance_variables_is_numbered_after_them() {
    // [I u:Foo "abc" {@x => "s"}, @1 ("s"), @2 (the _dump object)]
    let mut body = b"[\x08Iu".to_vec();
    body.extend(sym("Foo"));
    body.extend(b"\x08abc\x06");
    body.extend(sym("@x"));
    body.extend(plain_str(b"s"));
    body.extend(b"@\x06@\x07");
    let decoded = exact(&dump(&body));
    let doc = &decoded.doc;
    assert_eq!(hint_of(doc, "/0/0"), "RUBY:ivars");
    assert_eq!(at(doc, "/0/1").value, Value::Ref(id_at(doc, "/0/0/@x")));
    assert_eq!(
        at(doc, "/0/2").value,
        Value::Ref(id_at(doc, "/0/0/(value)"))
    );
    assert_eq!(hint_of(doc, "/0/0/(value)"), "RUBY:userdef:Foo");
}

#[test]
fn every_other_type_round_trips() {
    let mut cases: Vec<Vec<u8>> = vec![
        b"0".to_vec(),
        b"T".to_vec(),
        b"F".to_vec(),
        b"{\x00".to_vec(),
        b"}\x00i\x06".to_vec(),
        b"c\x0aClass".to_vec(),
        b"m\x0bKernel".to_vec(),
        b"M\x0bKernel".to_vec(),
        b"I/\x08a.b\x00\x06:\x06EF".to_vec(),
        b"/\x08a.b\x05".to_vec(),
    ];
    let mut data = b"d".to_vec();
    data.extend(sym("Foo"));
    data.extend(b"[\x00");
    cases.push(data);
    let mut user = b"U".to_vec();
    user.extend(sym("Set"));
    user.extend(b"{\x00");
    cases.push(user);
    let mut extended = b"e".to_vec();
    extended.extend(sym("M"));
    extended.extend(b"e");
    extended.extend(sym("N"));
    extended.extend(b"o");
    extended.extend(sym("Object"));
    extended.extend(b"\x00");
    cases.push(extended);
    let mut uclass = b"IC".to_vec();
    uclass.extend(sym("MyString"));
    uclass.extend(b"\"\x06x\x06");
    uclass.extend(sym("E"));
    uclass.extend(b"T");
    cases.push(uclass);
    let mut ivar_array = b"I[\x06i\x06\x06".to_vec();
    ivar_array.extend(sym("@note"));
    ivar_array.extend(b"T");
    cases.push(ivar_array);
    let mut strukt = b"S".to_vec();
    strukt.extend(sym("Point"));
    strukt.extend(b"\x07");
    strukt.extend(sym("x"));
    strukt.extend(b"i\x06");
    strukt.extend(sym("y"));
    strukt.extend(b"f\x061");
    cases.push(strukt);
    for body in cases {
        let decoded = exact(&dump(&body));
        assert!(decoded.doc.unfinished_count() == 0);
    }
    // The innermost string of a user class inside unknown instance variables is read-only.
    let mut uclass = b"IC".to_vec();
    uclass.extend(sym("MyString"));
    uclass.extend(b"\"\x06x\x06");
    uclass.extend(sym("E"));
    uclass.extend(b"T");
    let decoded = exact(&dump(&uclass));
    assert!(read_only(&decoded.doc, "/0/(value)/(value)"));
}

#[test]
fn several_dumps_and_trailing_bytes() {
    let mut bytes = dump(b"i\x06");
    bytes.extend(dump(b"T"));
    bytes.extend_from_slice(b"\x00\x01junk");
    let decoded = exact(&bytes);
    assert_eq!(decoded.doc.children(decoded.doc.root()).len(), 3);
    assert_eq!(hint_of(&decoded.doc, "/2"), "RUBY:trailing");
    assert!(read_only(&decoded.doc, "/2"));
}

// ----- hostile input ----------------------------------------------------------------------------

fn code_of(bytes: &[u8]) -> saveprism_core::ErrorCode {
    decode_with(bytes, &Limits::default()).unwrap_err().code()
}

#[test]
fn bad_headers_are_errors() {
    assert_eq!(code_of(b""), RUBY_PARSE);
    assert_eq!(code_of(b"\x04\x07i\x06"), RUBY_UNSUPPORTED);
    assert_eq!(code_of(b"{\"a\":1}"), RUBY_PARSE);
    assert_eq!(code_of(b"\x04\x08"), RUBY_PARSE);
    assert_eq!(code_of(b"\x04\x08X"), RUBY_PARSE);
    assert_eq!(code_of(b"\x04\x08l*\x00"), RUBY_PARSE);
}

#[test]
fn every_truncation_of_a_save_is_an_error_not_a_panic() {
    let bytes = fixture("synthetic/rpgmaker-vxace/Save02.rvdata2");
    for end in 0..bytes.len() {
        let prefix = &bytes[..end];
        if let Ok(decoded) = decode_with(prefix, &Limits::default()) {
            // Only a cut right after the header dump (plus at most a lone byte kept as trailing
            // data) can still be read.
            assert!(decoded.doc.children(decoded.doc.root()).len() <= 2, "{end}");
        }
    }
}

#[test]
fn huge_lengths_are_refused_before_allocating() {
    for body in [
        &b"\"\x04\xff\xff\xff\xff"[..],
        &b"[\x04\xff\xff\xff\xff"[..],
        &b"{\x04\xff\xff\xff\xff"[..],
        &b"o:\x06A\x04\xff\xff\xff\xff"[..],
        &b"l+\x04\xff\xff\xff\xff"[..],
        &b"\"\xfa"[..],
        &b"[\xfc\x00\x00\x00\x80"[..],
    ] {
        assert_eq!(code_of(&dump(body)), RUBY_PARSE, "{body:?}");
    }
}

#[test]
fn deep_nesting_hits_the_depth_limit() {
    let mut body = Vec::new();
    for _ in 0..10_000 {
        body.extend_from_slice(b"[\x06");
    }
    body.push(b'0');
    assert_eq!(code_of(&dump(&body)), codes::CORE_LIMIT_DEPTH);
    let mut wrappers = Vec::new();
    for _ in 0..10_000 {
        wrappers.extend_from_slice(b"I");
    }
    assert_eq!(code_of(&dump(&wrappers)), codes::CORE_LIMIT_DEPTH);
    let mut symbols = Vec::new();
    for _ in 0..10_000 {
        symbols.extend_from_slice(b"I:\x06a\x06");
    }
    assert_eq!(code_of(&dump(&symbols)), codes::CORE_LIMIT_DEPTH);
}

#[test]
fn deep_but_allowed_nesting_reads_and_writes_without_overflowing() {
    // The reader and writer run on their own large stack, so a 4095-level document (above the
    // default limit of 512, allowed here) works even on a small test-thread stack.
    let limits = Limits {
        max_depth: MAX_NESTING,
        ..Limits::default()
    };
    let mut body = Vec::new();
    for _ in 0..(MAX_NESTING - 1) {
        body.extend_from_slice(b"[\x06");
    }
    body.push(b'0');
    let bytes = dump(&body);
    let decoded = decode_with(&bytes, &limits).unwrap();
    assert_eq!(encode(&decoded), bytes);
    // Above the reader's own cap the limit is refused whatever the limits say.
    let limits = Limits {
        max_depth: 1_000_000,
        ..Limits::default()
    };
    let mut body = Vec::new();
    for _ in 0..(MAX_NESTING + 10) {
        body.extend_from_slice(b"[\x06");
    }
    body.push(b'0');
    let error = decode_with(&dump(&body), &limits).unwrap_err();
    assert_eq!(error.code(), codes::CORE_LIMIT_DEPTH);
}

#[test]
fn the_node_limit_is_enforced() {
    let limits = Limits {
        max_nodes: 50,
        ..Limits::default()
    };
    let bytes = fixture("synthetic/rpgmaker-vxace/Save01.rvdata2");
    let error = decode_with(&bytes, &limits).unwrap_err();
    assert_eq!(error.code(), codes::CORE_LIMIT_NODES);
}

#[test]
fn sniff_needs_the_header() {
    let none = SniffCtx::default();
    let named = SniffCtx {
        file_name: Some("Save01.RVDATA2"),
        depth: 0,
    };
    assert_eq!(
        RubyMarshalCodec.sniff(b"\x04\x08{\x00", &none),
        saveprism_core::Confidence::HIGH
    );
    assert!(RubyMarshalCodec.sniff(b"\x04\x08{\x00", &named) > saveprism_core::Confidence::HIGH);
    assert_eq!(
        RubyMarshalCodec.sniff(b"\x04\x08", &named),
        saveprism_core::Confidence::NONE
    );
    assert_eq!(
        RubyMarshalCodec.sniff(b"\x04\x08X", &named),
        saveprism_core::Confidence::NONE
    );
    assert_eq!(
        RubyMarshalCodec.sniff(b"{}", &named),
        saveprism_core::Confidence::NONE
    );
}

#[test]
fn hints_parse_back() {
    for hint in [
        "RUBY:dumps",
        "RUBY:string/utf8",
        "RUBY:regexp/ascii:7",
        "RUBY:userdef:Table",
        "RUBY:object:RPG::Event",
        "RUBY:hash+default",
    ] {
        assert!(parse_hint(hint).is_some(), "{hint}");
    }
    for hint in [
        "RUBY:int/utf8",
        "RUBY:regexp:x",
        "RUBY:object",
        "JSON:int",
        "RUBY:what",
    ] {
        assert!(parse_hint(hint).is_none(), "{hint}");
    }
    assert_eq!(
        parse_hint("RUBY:object:RPG::Event"),
        Some(Kind::Object("RPG::Event"))
    );
}

/// What decodes must write, and what is written must read back as the same tree.
fn writes_back(bytes: &[u8]) {
    if let Ok(decoded) = decode_with(bytes, &Limits::default()) {
        let written = RubyMarshalCodec
            .encode(&decoded.doc, &decoded.params)
            .unwrap();
        let again = decode_with(&written, &Limits::default()).unwrap();
        assert!(decoded.doc.tree_eq(&again.doc));
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// Random bytes after the header never panic.
    #[test]
    fn random_input_never_panics(body in proptest::collection::vec(any::<u8>(), 0..256)) {
        writes_back(&dump(&body));
    }

    /// Random bytes made of Marshal type bytes and small numbers reach deeper into the reader.
    #[test]
    fn marshal_like_input_never_panics(
        body in proptest::collection::vec(
            prop_oneof![
                proptest::sample::select(b"0TFil\"f:;@[{}oSICeuU/cmMd+-E".to_vec()),
                0_u8..16,
                any::<u8>(),
            ],
            0..96,
        )
    ) {
        writes_back(&dump(&body));
    }

    /// Mutated saves never panic.
    #[test]
    fn mutated_saves_never_panic(position in 0_usize..3430, byte in any::<u8>()) {
        let mut bytes = fixture("synthetic/rpgmaker-vxace/Save02.rvdata2");
        if let Some(slot) = bytes.get_mut(position) {
            *slot = byte;
        }
        writes_back(&bytes);
    }
}

#[test]
fn everything_inside_a_composite_hash_key_is_read_only() {
    // `{ ["key"] => 1 }` with a UTF-8 key string: editing it would change the key (and two keys
    // that became equal would merge when Ruby loads the file).
    let mut body = vec![b'{'];
    body.extend(long(1));
    body.push(b'[');
    body.extend(long(1));
    body.push(b'I');
    body.extend(plain_str(b"key"));
    body.extend(long(1));
    body.extend(sym("E"));
    body.push(b'T');
    body.push(b'i');
    body.extend(long(1));
    let decoded = exact(&dump(&body));
    let doc = &decoded.doc;
    let Value::Map(entries) = &at(doc, "/0").value else {
        panic!("not a map")
    };
    let (key, _) = entries[0];
    let Value::Array(items) = &doc.get(key).unwrap().value else {
        panic!("the key is not an array")
    };
    let inner = doc.get(items[0]).unwrap();
    assert!(matches!(inner.value, Value::Str(_)));
    assert!(inner.flags.contains(NodeFlags::READ_ONLY));
}

// ----- growable hashes --------------------------------------------------------------------------

/// The path of the first dump of the Ruby class `class`.
fn dump_of(doc: &SaveDoc, class: &str) -> String {
    let Value::Array(dumps) = &doc.get(doc.root()).unwrap().value else {
        panic!("the root is not a list of dumps")
    };
    let index = dumps
        .iter()
        .position(|id| ruby_class(doc.get(*id).unwrap()) == Some(class))
        .unwrap_or_else(|| panic!("no {class} dump"));
    format!("/{index}")
}

fn map_len(doc: &SaveDoc, path: &str) -> usize {
    match &at(doc, path).value {
        Value::Map(entries) => entries.len(),
        other => panic!("{path} is {other:?}"),
    }
}

fn insert(
    decoded: &mut Decoded,
    path: &str,
    key: i128,
    value: i128,
) -> saveprism_core::Result<usize> {
    insert_int_entry(
        &mut decoded.doc,
        &Path::parse(path).unwrap(),
        key,
        value,
        &Limits::default(),
    )
}

/// Inserts `key => value` at `path`, writes, reads back: the new file round-trips exactly and reads
/// as the edited document (every other value, hint and link as before). Returns the new bytes.
fn insert_and_check(relative: &str, path: &str, key: i128, value: i128) -> Vec<u8> {
    let original = fixture(relative);
    let mut decoded = decode(&original);
    assert!(
        is_growable_hash(at(&decoded.doc, path)),
        "{relative} {path}"
    );
    let before = map_len(&decoded.doc, path);
    let index = insert(&mut decoded, path, key, value).unwrap();
    assert_eq!(index, before);
    let written = encode(&decoded);
    let again = exact(&written);
    assert!(
        again.doc.tree_eq(&decoded.doc),
        "{relative}: the file reads differently"
    );
    assert_eq!(map_len(&again.doc, path), before + 1);
    assert_eq!(int_at(&again.doc, &format!("{path}/{index}.key")), key);
    assert_eq!(int_at(&again.doc, &format!("{path}/{index}")), value);
    assert_eq!(
        hint_of(&again.doc, &format!("{path}/{index}.key")),
        "RUBY:int"
    );
    assert_eq!(hint_of(&again.doc, &format!("{path}/{index}")), "RUBY:int");
    // The original still opens and writes exactly as it was.
    exact(&original);
    written
}

#[test]
fn integer_keyed_hashes_take_new_integer_entries() {
    let original = fixture("synthetic/rpgmaker-vxace/Save02.rvdata2");
    let written = insert_and_check(
        "synthetic/rpgmaker-vxace/Save02.rvdata2",
        "/1/party/@items",
        7,
        3,
    );
    // `i 7`, `i 3`: four bytes more, the entry count still fits its byte.
    assert_eq!(written.len(), original.len() + 4);

    // An empty hash (XP party armors) takes entries too.
    let xp = decode(&fixture("synthetic/rpgmaker-xp/Save1.rxdata"));
    let armors = format!("{}/@armors", dump_of(&xp.doc, "Game_Party"));
    assert_eq!(map_len(&xp.doc, &armors), 0);
    insert_and_check("synthetic/rpgmaker-xp/Save1.rxdata", &armors, 2, 1);
}

#[test]
fn a_bignum_entry_keeps_every_link_right() {
    // The party comes before the player, whose sprite name is a link: the Bignum adds an object in
    // between, so the link index moves by one.
    let written = insert_and_check(
        "synthetic/rpgmaker-vxace/Save01.rvdata2",
        "/1/party/@items",
        99,
        1 << 40,
    );
    let again = decode(&written);
    let target = id_at(&again.doc, "/1/actors/@data/1/@character_name");
    assert_eq!(
        at(&again.doc, "/1/player/@character_name").value,
        Value::Ref(target)
    );
}

#[test]
fn only_integer_keyed_plain_hashes_grow() {
    let mut decoded = decode(&fixture("synthetic/rpgmaker-vxace/Save02.rvdata2"));
    let before = encode(&decoded);
    // The contents hash (symbol keys) and an array take no entries.
    for path in ["/1", "/1/party/@actors", "/1/party/@gold"] {
        let error = insert(&mut decoded, path, 1, 1).unwrap_err();
        assert_eq!(error.code(), codes::CORE_READ_ONLY, "{path}");
    }
    // A key the hash has is refused.
    let key = int_at(&decoded.doc, "/1/party/@items/0.key");
    let error = insert(&mut decoded, "/1/party/@items", key, 1).unwrap_err();
    assert_eq!(error.code(), codes::CORE_EDIT_FAILED);
    assert_eq!(encode(&decoded), before);

    // A hash with a default, and one with a text key.
    let mut with_default = vec![b'}'];
    with_default.extend(long(0));
    with_default.push(b'i');
    with_default.extend(long(5));
    let mut text_key = vec![b'{'];
    text_key.extend(long(1));
    text_key.extend(plain_str(b"a"));
    text_key.push(b'i');
    text_key.extend(long(1));
    for body in [with_default, text_key] {
        let mut decoded = exact(&dump(&body));
        assert!(!is_growable_hash(at(&decoded.doc, "/0")));
        let error = insert(&mut decoded, "/0", 1, 1).unwrap_err();
        assert_eq!(error.code(), codes::CORE_READ_ONLY);
        assert_eq!(encode(&decoded), dump(&body));
    }
}

#[test]
fn only_added_entries_come_out_again() {
    let original = fixture("synthetic/rpgmaker-vxace/Save02.rvdata2");
    let mut decoded = decode(&original);
    let limits = Limits::default();
    let remove = |doc: &mut SaveDoc, path: &str| {
        apply_edits(
            doc,
            &[Edit::Remove {
                path: Path::parse(path).unwrap(),
            }],
            &limits,
        )
    };
    let error = remove(&mut decoded.doc, "/1/party/@items/0").unwrap_err();
    assert_eq!(error.code(), codes::CORE_READ_ONLY);

    let index = insert(&mut decoded, "/1/party/@items", 7, 3).unwrap();
    // The new key is read-only like every hash key; the count can be edited.
    let key = format!("/1/party/@items/{index}.key");
    assert!(read_only(&decoded.doc, &key));
    set(
        &mut decoded,
        &format!("/1/party/@items/{index}"),
        OwnedValue::Int(9),
    );
    remove(&mut decoded.doc, &format!("/1/party/@items/{index}")).unwrap();
    assert_eq!(encode(&decoded), original);
}

#[test]
fn an_entry_without_ruby_types_is_not_written() {
    // Inserted with the core alone, the entry has no hints: the writer refuses it.
    let mut decoded = decode(&fixture("synthetic/rpgmaker-vxace/Save02.rvdata2"));
    apply_edits(
        &mut decoded.doc,
        &[Edit::InsertEntry {
            parent: Path::parse("/1/party/@items").unwrap(),
            key: OwnedValue::Str("x".into()),
            value: OwnedValue::Int(1),
        }],
        &Limits::default(),
    )
    .unwrap();
    let error = RubyMarshalCodec
        .encode(&decoded.doc, &decoded.params)
        .unwrap_err();
    assert_eq!(error.code(), RUBY_WRITE);
}
