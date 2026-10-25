//! Tests of the QSP codec.

use std::path::PathBuf;

use proptest::prelude::*;
use saveprism_core::detect::{DetectOptions, detect};
use saveprism_core::guard::{Verdict, check_round_trip};
use saveprism_core::{
    Codec, DecodeCtx, Decoded, Edit, Limits, Node, NodeFlags, NodeId, OwnedValue, Path, Registry,
    SaveDoc, Value, apply_edits,
};

use super::*;

const FULL: &str = "synthetic/qsp/chapter-one.sav";
const MINIMAL: &str = "synthetic/qsp/minimal.sav";
const ANSI: &str = "synthetic/qsp/legacy-ansi.sav";
const KOI8: &str = "synthetic/qsp/legacy-koi8r.sav";
const PADDED: &str = "synthetic/qsp/padded-numbers.sav";

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn fixture(relative: &str) -> Vec<u8> {
    std::fs::read(fixtures().join(relative)).unwrap()
}

fn decode_with(bytes: &[u8], limits: &Limits) -> saveprism_core::Result<Decoded> {
    let mut ctx = DecodeCtx::new(limits);
    QspCodec.decode(bytes, &mut ctx)
}

fn decode(bytes: &[u8]) -> Decoded {
    decode_with(bytes, &Limits::default()).unwrap()
}

fn encode(decoded: &Decoded) -> Vec<u8> {
    QspCodec.encode(&decoded.doc, &decoded.params).unwrap()
}

/// Decodes, checks that writing gives the same bytes, returns the decoded document.
fn exact(bytes: &[u8]) -> Decoded {
    let decoded = decode(bytes);
    assert_eq!(encode(&decoded), bytes, "re-encoded bytes differ");
    decoded
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

fn text_at(doc: &SaveDoc, path: &str) -> String {
    match &at(doc, path).value {
        Value::Str(text) => text.clone(),
        other => panic!("{path} is {other:?}"),
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

// ----- fixtures ---------------------------------------------------------------------------------

#[test]
fn every_fixture_round_trips_exactly() {
    for name in [FULL, MINIMAL, ANSI, KOI8, PADDED] {
        let bytes = fixture(name);
        let decoded = exact(&bytes);
        assert_eq!(hint_of(&decoded.doc, ""), "QSP:save", "{name} root hint");
        assert_eq!(text_at(&decoded.doc, "/header"), "QSPSAVEDGAME");
    }
}

#[test]
fn the_tree_has_the_confirmed_fields() {
    let decoded = decode(&fixture(FULL));
    let doc = &decoded.doc;
    assert_eq!(text_at(doc, "/version"), "5.9.6");
    assert_eq!(int_at(doc, "/checksum"), -1_234_567);
    assert_eq!(int_at(doc, "/playtime_ms"), 3_723_000);
    assert_eq!(text_at(doc, "/location"), "Workshop");
    assert!(read_only(doc, "/header"));
    assert!(read_only(doc, "/version"));
    assert!(read_only(doc, "/variables"));

    assert_eq!(text_at(doc, "/playlist/0"), "audio/theme.ogg");
    assert_eq!(text_at(doc, "/includes/0"), "epilogue.qsp");
    assert_eq!(text_at(doc, "/actions/0/description"), "Inspect the bench");
    assert_eq!(int_at(doc, "/actions/0/lines/1/line_number"), 11);
    assert_eq!(text_at(doc, "/objects/1/name"), "Wrench");
    assert_eq!(int_at(doc, "/object_groups/0/objects_count"), 2);

    assert_eq!(text_at(doc, "/variables/0/name"), "MONEY");
    assert_eq!(int_at(doc, "/variables/0/values/0"), 120);
    assert_eq!(int_at(doc, "/variables/0/values/1"), -5);
    assert_eq!(text_at(doc, "/variables/1/values/0"), "Robin");
    assert_eq!(hint_of(doc, "/variables/2/values/0"), "QSP:bool");
    assert_eq!(hint_of(doc, "/variables/2/values/2"), "QSP:undef");
    assert!(read_only(doc, "/variables/2/values/2"));
    assert_eq!(hint_of(doc, "/variables/3/values/2"), "QSP:tuple");
    assert_eq!(int_at(doc, "/variables/3/values/2/0"), 1);
    assert_eq!(text_at(doc, "/variables/3/values/2/1"), "spare key");
    assert_eq!(int_at(doc, "/variables/3/keys/0/index"), 0);
    assert_eq!(int_at(doc, "/variables/3/keys/0/key"), 1);
    assert!(read_only(doc, "/variables/3/keys/0/index"));
    assert!(read_only(doc, "/variables/0/name"));
}

#[test]
fn an_ansi_save_is_read_and_written_in_its_code_page() {
    let decoded = exact(&fixture(ANSI));
    assert_eq!(text_at(&decoded.doc, "/location"), "Мастерская");
    assert_eq!(text_at(&decoded.doc, "/description"), "Старая мастерская");
    assert_eq!(text_at(&decoded.doc, "/variables/1/values/0"), "Робин");
    assert_eq!(
        decoded.params.get(P_ENCODING),
        Some(&saveprism_core::ParamValue::Str("ansi".into()))
    );
}

#[test]
fn koi8_r_is_told_apart_from_windows_1251() {
    let ansi = exact(&fixture(ANSI));
    assert_eq!(ansi.params.str(P_ENCODING).unwrap(), "ansi");
    let koi8 = exact(&fixture(KOI8));
    assert_eq!(koi8.params.str(P_ENCODING).unwrap(), "ansi-koi8r");
    assert_eq!(text_at(&koi8.doc, "/location"), "Мастерская");
    assert_eq!(text_at(&koi8.doc, "/variables/1/name"), "ИМЯ");
}

#[test]
fn non_canonical_numbers_keep_their_text() {
    let decoded = exact(&fixture(PADDED));
    assert_eq!(int_at(&decoded.doc, "/checksum"), 7);
    assert_eq!(
        at(&decoded.doc, "/checksum").lexeme.as_deref(),
        Some("+007")
    );
    assert_eq!(int_at(&decoded.doc, "/variables/0/values/0"), -3);
    let params = &decoded.params;
    let table = match params.get(P_VARIANT_TYPES) {
        Some(saveprism_core::ParamValue::Table(table)) => table,
        other => panic!("variant types are {other:?}"),
    };
    assert_eq!(
        table.get("0"),
        Some(&saveprism_core::ParamValue::Str("+2".into()))
    );
}

// ----- edits ------------------------------------------------------------------------------------

#[test]
fn a_number_and_a_string_variable_can_be_edited() {
    let original = fixture(FULL);
    let mut decoded = decode(&original);
    set(&mut decoded, "/variables/0/values/0", OwnedValue::Int(999));
    set(
        &mut decoded,
        "/variables/1/values/0",
        OwnedValue::Str("Ada".into()),
    );
    set(&mut decoded, "/variables/3/values/2/0", OwnedValue::Int(7));
    set(&mut decoded, "/variables/2/values/0", OwnedValue::Int(0));
    let written = encode(&decoded);
    assert_ne!(written, original);
    let again = exact(&written);
    assert_eq!(int_at(&again.doc, "/variables/0/values/0"), 999);
    assert_eq!(text_at(&again.doc, "/variables/1/values/0"), "Ada");
    assert_eq!(int_at(&again.doc, "/variables/3/values/2/0"), 7);
    assert_eq!(int_at(&again.doc, "/variables/2/values/0"), 0);
    assert_eq!(hint_of(&again.doc, "/variables/2/values/0"), "QSP:bool");
    // Untouched neighbours keep their values.
    assert_eq!(int_at(&again.doc, "/variables/0/values/1"), -5);
    assert_eq!(text_at(&again.doc, "/variables/3/values/2/1"), "spare key");
}

#[test]
fn a_non_canonical_number_loses_its_old_text_when_edited() {
    let mut decoded = decode(&fixture(PADDED));
    set(&mut decoded, "/checksum", OwnedValue::Int(8));
    let again = exact(&encode(&decoded));
    assert_eq!(int_at(&again.doc, "/checksum"), 8);
    assert_eq!(at(&again.doc, "/checksum").lexeme, None);
}

#[test]
fn an_ansi_string_can_be_edited() {
    let mut decoded = decode(&fixture(ANSI));
    set(
        &mut decoded,
        "/variables/1/values/0",
        OwnedValue::Str("Ада".into()),
    );
    let again = exact(&encode(&decoded));
    assert_eq!(text_at(&again.doc, "/variables/1/values/0"), "Ада");
}

#[test]
fn a_character_the_code_page_lacks_is_a_write_error() {
    let mut decoded = decode(&fixture(ANSI));
    set(
        &mut decoded,
        "/variables/1/values/0",
        OwnedValue::Str("Ada ⚔".into()),
    );
    let error = QspCodec.encode(&decoded.doc, &decoded.params).unwrap_err();
    assert_eq!(error.code(), QSP_WRITE);
}

#[test]
fn read_only_fields_cannot_be_edited() {
    let mut decoded = decode(&fixture(FULL));
    let error = apply_edits(
        &mut decoded.doc,
        &[Edit::Set {
            path: Path::parse("/header").unwrap(),
            value: OwnedValue::Str("other".into()),
        }],
        &Limits::default(),
    )
    .unwrap_err();
    assert_eq!(error.code(), saveprism_core::codes::CORE_READ_ONLY);
}

#[test]
fn detection_and_guard_pick_the_codec() {
    let mut reg = Registry::default();
    crate::register_all(&mut reg);
    let limits = Limits::default();
    for name in [FULL, MINIMAL, ANSI, KOI8, PADDED] {
        let bytes = fixture(name);
        let options = DetectOptions {
            file_name: Some(name.rsplit('/').next().unwrap().to_string()),
            ..DetectOptions::default()
        };
        let found = detect(&bytes, &reg, &limits, &options).unwrap();
        assert_eq!(found.best.recipe.kinds(), vec!["format.qsp"], "{name}");
        let report = check_round_trip(&bytes, &found.best.recipe, &reg, &limits).unwrap();
        assert_eq!(report.overall, Verdict::Exact, "{name}");
        assert!(report.editable, "{name}");
    }
}

// ----- hostile input ----------------------------------------------------------------------------

fn code_of(bytes: &[u8]) -> saveprism_core::ErrorCode {
    decode_with(bytes, &Limits::default()).unwrap_err().code()
}

/// Splits a UTF-16LE save into its lines (without the CRLF).
fn wide_lines(bytes: &[u8]) -> Vec<Vec<u16>> {
    let (pairs, _rest) = bytes.as_chunks::<2>();
    let mut units = Vec::with_capacity(pairs.len());
    for pair in pairs {
        units.push(u16::from_le_bytes(*pair));
    }
    let mut lines = Vec::new();
    let mut current = Vec::new();
    let mut index = 0;
    while let Some(unit) = units.get(index).copied() {
        if unit == 0x000D && units.get(index + 1).copied() == Some(0x000A) {
            lines.push(std::mem::take(&mut current));
            index += 2;
        } else {
            current.push(unit);
            index += 1;
        }
    }
    lines
}

fn join_wide(lines: &[Vec<u16>]) -> Vec<u8> {
    let mut out = Vec::new();
    for line in lines {
        for unit in line {
            out.extend_from_slice(&unit.to_le_bytes());
        }
        out.extend_from_slice(&[0x0D, 0x00, 0x0A, 0x00]);
    }
    out
}

/// Encodes text the way QSP writes an encoded line.
fn encoded_line(text: &str) -> Vec<u16> {
    text.encode_utf16()
        .map(|unit| {
            if unit == SHIFT {
                unit.wrapping_neg()
            } else if unit == 0 {
                0
            } else {
                unit.wrapping_sub(SHIFT)
            }
        })
        .collect()
}

#[test]
fn bad_headers_are_errors() {
    assert_eq!(code_of(b""), QSP_PARSE);
    assert_eq!(code_of(b"QSPSAVEDGAME\r\n"), QSP_PARSE);
    assert_eq!(code_of(b"{\"a\":1}"), QSP_PARSE);
    let mut wrong = fixture(MINIMAL);
    wrong[0] = b'X';
    assert_eq!(code_of(&wrong), QSP_PARSE);
}

#[test]
fn every_truncation_of_a_save_is_an_error_not_a_panic() {
    let bytes = fixture(FULL);
    for end in 0..bytes.len() {
        let prefix = &bytes[..end];
        if let Ok(decoded) = decode_with(prefix, &Limits::default()) {
            // A prefix that still parses must be writable.
            QspCodec.encode(&decoded.doc, &decoded.params).unwrap();
        }
    }
}

#[test]
fn a_wrong_count_is_an_error() {
    let bytes = fixture(FULL);
    let mut lines = wide_lines(&bytes);
    // Playlist count is line 13 (header, version, eight scalars, timer interval).
    let slot = lines.get_mut(13).unwrap();
    *slot = encoded_line("99");
    assert_eq!(code_of(&join_wide(&lines)), QSP_PARSE);
}

#[test]
fn a_missing_final_separator_is_an_error() {
    let mut bytes = fixture(MINIMAL);
    bytes.truncate(bytes.len() - 4);
    assert_eq!(code_of(&bytes), QSP_PARSE);
}

#[test]
fn invalid_utf16_is_an_error() {
    let bytes = fixture(MINIMAL);
    let mut lines = wide_lines(&bytes);
    // The location line is line 10.
    let slot = lines.get_mut(10).unwrap();
    *slot = vec![0xD800];
    assert_eq!(code_of(&join_wide(&lines)), QSP_PARSE);
}

#[test]
fn an_unknown_value_type_is_unsupported() {
    let bytes = fixture(PADDED);
    let mut lines = wide_lines(&bytes);
    // The last lines are: variable name, value count, type, value, key count.
    let index = lines.len() - 3;
    let slot = lines.get_mut(index).unwrap();
    *slot = encoded_line("9");
    assert_eq!(code_of(&join_wide(&lines)), QSP_UNSUPPORTED);
}

#[test]
fn deep_nesting_is_refused() {
    let mut lines = wide_lines(&fixture(MINIMAL));
    // Replace "variable count = 0" (line 19) with one deeply nested value.
    lines.truncate(19);
    lines.push(encoded_line("1"));
    lines.push(encoded_line("A"));
    lines.push(encoded_line("1"));
    for _ in 0..(MAX_NESTING + 2) {
        lines.push(encoded_line("1"));
        lines.push(encoded_line("1"));
    }
    lines.push(encoded_line("0"));
    lines.push(encoded_line("0"));
    assert_eq!(code_of(&join_wide(&lines)), QSP_PARSE);
}

// ----- properties -------------------------------------------------------------------------------

/// A decoded document must be writable, and what is written must read back as the same tree.
fn writes_back(bytes: &[u8]) {
    let Ok(decoded) = decode_with(bytes, &Limits::default()) else {
        return;
    };
    let written = QspCodec.encode(&decoded.doc, &decoded.params).unwrap();
    let again = decode_with(&written, &Limits::default()).unwrap();
    assert!(decoded.doc.tree_eq(&again.doc));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn random_input_never_panics(body in proptest::collection::vec(any::<u8>(), 0..256)) {
        writes_back(&body);
    }

    #[test]
    fn mutated_saves_never_panic(position in 0_usize..1060, byte in any::<u8>()) {
        let mut bytes = fixture(FULL);
        if let Some(slot) = bytes.get_mut(position) {
            *slot = byte;
        }
        writes_back(&bytes);
    }
}
