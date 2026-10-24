//! Tests of the SQLite codec.

use std::path::PathBuf;

use proptest::prelude::*;
use rusqlite::{Connection, MAIN_DB};
use saveprism_core::detect::{DetectOptions, detect};
use saveprism_core::guard::{Verdict, check_round_trip};
use saveprism_core::{
    Codec, DecodeCtx, Decoded, Edit, Limits, NodeFlags, OwnedValue, Path, Recipe, Registry, Value,
    apply_edits, codes,
};

use super::*;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn fixture(relative: &str) -> Vec<u8> {
    std::fs::read(fixtures().join(relative)).unwrap()
}

fn decode_with(bytes: &[u8], limits: &Limits) -> saveprism_core::Result<Decoded> {
    let mut ctx = DecodeCtx::new(limits);
    SqliteCodec.decode(bytes, &mut ctx)
}

fn decode(bytes: &[u8]) -> Decoded {
    decode_with(bytes, &Limits::default()).unwrap()
}

fn encode(decoded: &Decoded) -> Vec<u8> {
    SqliteCodec.encode(&decoded.doc, &decoded.params).unwrap()
}

fn exact(bytes: &[u8]) -> Decoded {
    let decoded = decode(bytes);
    assert_eq!(encode(&decoded), bytes, "re-encoded bytes differ");
    decoded
}

fn at<'a>(doc: &'a saveprism_core::SaveDoc, path: &str) -> &'a saveprism_core::Node {
    let id = doc
        .resolve(&Path::parse(path).unwrap())
        .unwrap_or_else(|| panic!("no node at {path}"));
    doc.get(id).unwrap()
}

fn int_at(doc: &saveprism_core::SaveDoc, path: &str) -> i128 {
    match at(doc, path).value {
        Value::Int { v, .. } => v,
        ref other => panic!("{path} is {other:?}"),
    }
}

fn read_only(doc: &saveprism_core::SaveDoc, path: &str) -> bool {
    at(doc, path).flags.contains(NodeFlags::READ_ONLY)
}

fn hint_of<'a>(doc: &'a saveprism_core::SaveDoc, path: &str) -> &'a str {
    at(doc, path).type_hint.as_ref().unwrap().as_str()
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

fn integrity(bytes: &[u8]) -> String {
    let mut connection = Connection::open_in_memory().unwrap();
    connection
        .deserialize_read_exact(MAIN_DB, bytes, bytes.len(), true)
        .unwrap();
    connection
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .unwrap()
}

// ----- fixtures ---------------------------------------------------------------------------------

#[test]
fn every_fixture_round_trips_exactly() {
    let dir = fixtures().join("synthetic/sqlite");
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        panic!("missing {}", dir.display());
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| ["sqlite", "sqlite3", "db"].contains(&e))
        {
            found.push(path);
        }
    }
    assert!(!found.is_empty());
    for path in found {
        let bytes = std::fs::read(&path).unwrap();
        let decoded = decode_with(&bytes, &Limits::default())
            .unwrap_or_else(|error| panic!("{}: {}", path.display(), error.report()));
        assert_eq!(
            SqliteCodec.encode(&decoded.doc, &decoded.params).unwrap(),
            bytes,
            "{} does not round-trip",
            path.display()
        );
    }
}

#[test]
fn the_game_save_reads_like_tables() {
    let decoded = exact(&fixture("synthetic/sqlite/game.sqlite"));
    let doc = &decoded.doc;
    assert_eq!(hint_of(doc, ""), "SQLITE:tables");
    assert_eq!(
        doc.children(doc.root())
            .iter()
            .map(|id| match &doc.get(*id).unwrap().value {
                Value::Array(_) => "table",
                _ => "other",
            })
            .collect::<Vec<_>>(),
        vec!["table"; 5]
    );

    assert_eq!(hint_of(doc, "/player"), "SQLITE:table");
    assert_eq!(
        doc.children(doc.resolve(&Path::parse("/player").unwrap()).unwrap())
            .len(),
        2
    );
    assert_eq!(int_at(doc, "/player/0/(rowid)"), 1);
    assert!(read_only(doc, "/player/0/(rowid)"));
    assert_eq!(hint_of(doc, "/player/0/id"), "SQLITE:integer/rowid-alias");
    assert!(!read_only(doc, "/player/0/id"));
    assert_eq!(int_at(doc, "/player/0/id"), 1);
    assert_eq!(int_at(doc, "/player/0/gold"), 1250);
    assert!(!read_only(doc, "/player/0/gold"));
    assert_eq!(
        at(doc, "/player/0/hp").value,
        Value::Float {
            v: 87.5,
            width: saveprism_core::FloatWidth::F64
        }
    );
    assert!(!read_only(doc, "/player/0/hp"));
    assert_eq!(at(doc, "/player/0/name").value, Value::Str("Aria".into()));
    assert!(!read_only(doc, "/player/0/name"));
    assert!(matches!(&at(doc, "/player/0/flags").value, Value::Bytes(b) if b == &[0, 0xff, 0x10]));
    assert!(read_only(doc, "/player/0/flags"));
    assert_eq!(at(doc, "/player/0/nickname").value, Value::Null);
    assert!(read_only(doc, "/player/0/nickname"));
    assert_eq!(int_at(doc, "/player/1/gold"), 40);

    assert_eq!(int_at(doc, "/inventory/2/count"), 10);
    assert_eq!(
        at(doc, "/inventory/2/item").value,
        Value::Str("rope".into())
    );

    // A WITHOUT ROWID table: no rowid entry, every cell read-only.
    assert_eq!(hint_of(doc, "/variables"), "SQLITE:table/without-rowid");
    assert_eq!(int_at(doc, "/variables/0/value"), 4);
    assert!(read_only(doc, "/variables/0/key"));
    assert!(read_only(doc, "/variables/0/value"));

    // A generated column is visible and read-only.
    assert_eq!(hint_of(doc, "/generated"), "SQLITE:table");
    assert_eq!(int_at(doc, "/generated/0/amount"), 21);
    assert_eq!(int_at(doc, "/generated/0/doubled"), 42);
    assert!(read_only(doc, "/generated/0/doubled"));

    // A view is read-only.
    assert_eq!(hint_of(doc, "/player_summary"), "SQLITE:view");
    assert_eq!(
        at(doc, "/player_summary/0/name").value,
        Value::Str("Aria".into())
    );
    assert!(read_only(doc, "/player_summary/0/name"));
    assert!(decoded.params.contains(ORIGINAL));
}

#[test]
fn detection_and_guard_pick_the_codec() {
    let mut reg = Registry::default();
    crate::register_all(&mut reg);
    let bytes = fixture("synthetic/sqlite/game.sqlite");
    let options = DetectOptions {
        file_name: Some("game.sqlite".to_string()),
        ..DetectOptions::default()
    };
    let found = detect(&bytes, &reg, &Limits::default(), &options).unwrap();
    assert_eq!(found.best.recipe.kinds(), vec!["format.sqlite"]);
    let report = check_round_trip(&bytes, &found.best.recipe, &reg, &Limits::default()).unwrap();
    assert_eq!(report.overall, Verdict::Exact);
    assert!(report.editable);
}

// ----- edits ------------------------------------------------------------------------------------

#[test]
fn edited_scalars_are_written_and_read_back() {
    let original = fixture("synthetic/sqlite/game.sqlite");
    let mut decoded = decode(&original);
    set(&mut decoded, "/player/0/gold", OwnedValue::Int(999_999));
    set(&mut decoded, "/player/0/hp", OwnedValue::Float(90.25));
    set(
        &mut decoded,
        "/player/1/name",
        OwnedValue::Str("Bram the Bold".into()),
    );
    set(&mut decoded, "/inventory/0/count", OwnedValue::Int(12));
    let written = encode(&decoded);
    assert_ne!(written, original);
    assert_eq!(integrity(&written), "ok");

    let again = exact(&written);
    assert_eq!(int_at(&again.doc, "/player/0/gold"), 999_999);
    assert_eq!(
        at(&again.doc, "/player/0/hp").value,
        Value::Float {
            v: 90.25,
            width: saveprism_core::FloatWidth::F64
        }
    );
    assert_eq!(
        at(&again.doc, "/player/1/name").value,
        Value::Str("Bram the Bold".into())
    );
    assert_eq!(int_at(&again.doc, "/inventory/0/count"), 12);

    // Every other cell is unchanged.
    assert_eq!(int_at(&again.doc, "/player/0/id"), 1);
    assert_eq!(int_at(&again.doc, "/player/1/gold"), 40);
    assert_eq!(int_at(&again.doc, "/player/1/(rowid)"), 2);
    assert_eq!(
        at(&again.doc, "/player/0/name").value,
        Value::Str("Aria".into())
    );
    assert_eq!(
        at(&again.doc, "/player/0/flags").value,
        Value::Bytes(vec![0, 0xff, 0x10])
    );
    assert_eq!(int_at(&again.doc, "/inventory/2/count"), 10);
    assert_eq!(int_at(&again.doc, "/variables/0/value"), 4);
    assert_eq!(int_at(&again.doc, "/generated/0/doubled"), 42);
}

#[test]
fn editing_the_rowid_alias_with_other_cells_works() {
    let mut decoded = decode(&fixture("synthetic/sqlite/game.sqlite"));
    set(&mut decoded, "/player/0/id", OwnedValue::Int(77));
    set(&mut decoded, "/player/0/gold", OwnedValue::Int(1));
    let written = encode(&decoded);
    assert_eq!(integrity(&written), "ok");
    let again = exact(&written);
    // The edited row moved behind the untouched row: rows are read in rowid order.
    assert_eq!(int_at(&again.doc, "/player/0/(rowid)"), 2);
    assert_eq!(int_at(&again.doc, "/player/1/(rowid)"), 77);
    assert_eq!(int_at(&again.doc, "/player/1/id"), 77);
    assert_eq!(int_at(&again.doc, "/player/1/gold"), 1);
}

#[test]
fn unchanged_cells_keep_the_original_bytes() {
    let original = fixture("synthetic/sqlite/game.sqlite");
    let mut decoded = decode(&original);
    set(&mut decoded, "/player/0/gold", OwnedValue::Int(1250));
    assert_eq!(encode(&decoded), original);
}

#[test]
fn read_only_cells_and_tables_are_refused() {
    let mut decoded = decode(&fixture("synthetic/sqlite/game.sqlite"));
    let limits = Limits::default();
    let refused = |doc: &mut saveprism_core::SaveDoc, path: &str, value: OwnedValue| {
        let error = apply_edits(
            doc,
            &[Edit::Set {
                path: Path::parse(path).unwrap(),
                value,
            }],
            &limits,
        )
        .unwrap_err();
        assert_eq!(error.code(), codes::CORE_READ_ONLY, "{path}");
    };
    for path in [
        "/player/0/(rowid)",
        "/player/0/flags",
        "/player/0/nickname",
        "/variables/0/value",
        "/player_summary/0/gold",
        "/generated/0/doubled",
    ] {
        refused(&mut decoded.doc, path, OwnedValue::Int(1));
    }
}

#[test]
fn a_value_cannot_change_its_type() {
    let mut decoded = decode(&fixture("synthetic/sqlite/game.sqlite"));
    set(
        &mut decoded,
        "/player/0/gold",
        OwnedValue::Str("lots".into()),
    );
    let error = SqliteCodec
        .encode(&decoded.doc, &decoded.params)
        .unwrap_err();
    assert_eq!(error.code(), SQLITE_WRITE);
}

// ----- hostile input ----------------------------------------------------------------------------

fn code_of(bytes: &[u8]) -> saveprism_core::ErrorCode {
    decode_with(bytes, &Limits::default()).unwrap_err().code()
}

#[test]
fn bad_headers_are_errors() {
    assert_eq!(code_of(b""), SQLITE_PARSE);
    assert_eq!(code_of(b"SQLite format 3"), SQLITE_PARSE);
    assert_eq!(code_of(b"not a database at all"), SQLITE_PARSE);
    let mut wal = fixture("synthetic/sqlite/game.sqlite");
    wal[18] = 2;
    wal[19] = 2;
    assert_eq!(code_of(&wal), SQLITE_WAL);
}

#[test]
fn every_truncation_of_a_save_is_an_error_not_a_panic() {
    let bytes = fixture("synthetic/sqlite/game.sqlite");
    for end in 0..bytes.len() {
        let prefix = &bytes[..end];
        assert!(
            decode_with(prefix, &Limits::default()).is_err(),
            "prefix of {end} bytes was accepted"
        );
    }
}

#[test]
fn corrupt_pages_are_errors() {
    let mut bytes = fixture("synthetic/sqlite/game.sqlite");
    // Page size 0 is invalid; SQLite refuses the database.
    bytes[16] = 0;
    bytes[17] = 0;
    assert!(decode_with(&bytes, &Limits::default()).is_err());
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn random_input_never_panics(data in proptest::collection::vec(any::<u8>(), 0..512)) {
        let _ = decode_with(&data, &Limits::default());
    }
}

#[test]
fn the_registered_codec_is_findable() {
    let mut reg = Registry::default();
    crate::register_all(&mut reg);
    assert!(reg.is_codec("format.sqlite"));
    let recipe = Recipe::new(vec![saveprism_core::Step::bare("format.sqlite")]);
    assert_eq!(recipe.kinds(), vec!["format.sqlite"]);
}

#[test]
fn tables_with_triggers_stay_read_only() {
    // An UPDATE would run the trigger, which is SQL from the save file.
    let connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch(
            "CREATE TABLE player (gold INTEGER); INSERT INTO player VALUES (10);
             CREATE TABLE log (note TEXT); INSERT INTO log VALUES ('start');
             CREATE TRIGGER audit AFTER UPDATE ON player BEGIN INSERT INTO log VALUES ('x'); END;",
        )
        .unwrap();
    let bytes = connection.serialize(MAIN_DB).unwrap().to_vec();
    let decoded = exact(&bytes);
    assert!(read_only(&decoded.doc, "/player/0/gold"));
    assert!(!read_only(&decoded.doc, "/log/0/note"));
}
