//! Tests of the LevelDB table and log codecs.

use std::path::PathBuf;

use proptest::prelude::*;
use saveprism_core::detect::{DetectOptions, detect};
use saveprism_core::guard::{Verdict, check_round_trip};
use saveprism_core::{
    Codec, DecodeCtx, Decoded, Edit, Limits, NodeFlags, OwnedValue, Path, Registry, Value,
    apply_edits,
};

use super::*;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn fixture(relative: &str) -> Vec<u8> {
    std::fs::read(fixtures().join(relative)).unwrap()
}

fn decode_with(codec: &dyn Codec, bytes: &[u8]) -> saveprism_core::Result<Decoded> {
    let limits = Limits::default();
    let mut ctx = DecodeCtx::new(&limits);
    codec.decode(bytes, &mut ctx)
}

fn exact(codec: &dyn Codec, bytes: &[u8]) -> Decoded {
    let decoded = decode_with(codec, bytes).unwrap();
    assert_eq!(
        codec.encode(&decoded.doc, &decoded.params).unwrap(),
        bytes,
        "re-encoded bytes differ"
    );
    decoded
}

fn at<'a>(doc: &'a saveprism_core::SaveDoc, path: &str) -> &'a saveprism_core::Node {
    let id = doc
        .resolve(&Path::parse(path).unwrap())
        .unwrap_or_else(|| panic!("no node at {path}"));
    doc.get(id).unwrap()
}

fn set(decoded: &mut Decoded, path: &str, value: OwnedValue) -> saveprism_core::Result<()> {
    apply_edits(
        &mut decoded.doc,
        &[Edit::Set {
            path: Path::parse(path).unwrap(),
            value,
        }],
        &Limits::default(),
    )
    .map(|_| ())
}

#[test]
fn the_table_fixture_reads_like_key_value_pairs() {
    let decoded = exact(&LevelDbSstCodec, &fixture("synthetic/leveldb/save.ldb"));
    let doc = &decoded.doc;
    assert_eq!(
        at(doc, "").type_hint.as_ref().unwrap().as_str(),
        "LEVELDB:entries"
    );
    assert_eq!(at(doc, "/player~1gold").value, Value::Str("1250".into()));
    assert_eq!(at(doc, "/player~1name").value, Value::Str("Aria".into()));
    assert_eq!(at(doc, "/removed~1flag").value, Value::Null);
    assert_eq!(
        at(doc, "/removed~1flag")
            .type_hint
            .as_ref()
            .unwrap()
            .as_str(),
        "LEVELDB:deleted"
    );
    assert!(
        at(doc, "/player~1gold")
            .flags
            .contains(NodeFlags::READ_ONLY)
    );
    assert!(at(doc, "").flags.contains(NodeFlags::READ_ONLY));
}

#[test]
fn the_log_fixture_reads_write_batch_entries_in_order() {
    let decoded = exact(&LevelDbLogCodec, &fixture("synthetic/leveldb/000003.log"));
    let doc = &decoded.doc;
    let keys: Vec<&str> = match &at(doc, "").value {
        Value::Object(entries) => entries.iter().map(|(key, _)| key.as_str()).collect(),
        other => panic!("root is {other:?}"),
    };
    assert_eq!(
        keys,
        vec![
            "player/gold",
            "player/name",
            "old/flag",
            "https://game.example / gold",
            "player/gold",
        ]
    );
    assert_eq!(at(doc, "/player~1gold").value, Value::Str("1250".into()));
    assert_eq!(at(doc, "/old~1flag").value, Value::Null);
    assert_eq!(
        at(doc, "/https:~1~1game.example ~1 gold").value,
        Value::Str("1250".into())
    );
    assert_eq!(
        at(doc, "/https:~1~1game.example ~1 gold")
            .type_hint
            .as_ref()
            .unwrap()
            .as_str(),
        "LEVELDB:chrome-local-storage"
    );
}

#[test]
fn edits_are_refused_because_the_file_is_read_only() {
    for (codec, path) in [
        (&LevelDbSstCodec as &dyn Codec, "synthetic/leveldb/save.ldb"),
        (&LevelDbLogCodec, "synthetic/leveldb/000003.log"),
    ] {
        let mut decoded = decode_with(codec, &fixture(path)).unwrap();
        let error = set(&mut decoded, "/player~1gold", OwnedValue::Str("1".into())).unwrap_err();
        assert_eq!(error.code(), saveprism_core::codes::CORE_READ_ONLY);
        let id = decoded
            .doc
            .resolve(&Path::parse("/player~1gold").unwrap())
            .unwrap();
        decoded.doc.set_flags(id, NodeFlags::empty()).unwrap();
        set(&mut decoded, "/player~1gold", OwnedValue::Str("1".into())).unwrap();
        let error = codec.encode(&decoded.doc, &decoded.params).unwrap_err();
        assert_eq!(error.code(), LEVELDB_WRITE);
    }
}

#[test]
fn detection_picks_each_file_kind_and_not_the_other_formats() {
    let mut reg = Registry::default();
    crate::register_all(&mut reg);
    let limits = Limits::default();
    for (file, path, kind) in [
        (
            "save.ldb",
            "synthetic/leveldb/save.ldb",
            "format.leveldb-sst",
        ),
        (
            "000003.log",
            "synthetic/leveldb/000003.log",
            "format.leveldb-log",
        ),
    ] {
        let bytes = fixture(path);
        let options = DetectOptions {
            file_name: Some(file.to_string()),
            ..DetectOptions::default()
        };
        let found = detect(&bytes, &reg, &limits, &options).unwrap();
        assert_eq!(found.best.recipe.kinds(), vec![kind]);
        let report = check_round_trip(&bytes, &found.best.recipe, &reg, &limits).unwrap();
        assert_eq!(report.overall, Verdict::Exact);
    }
    let sqlite = fixture("synthetic/sqlite/game.sqlite");
    let options = DetectOptions {
        file_name: Some("game.db".to_string()),
        ..DetectOptions::default()
    };
    let found = detect(&sqlite, &reg, &limits, &options).unwrap();
    assert_eq!(found.best.recipe.kinds(), vec!["format.sqlite"]);
}

#[test]
fn a_log_file_is_not_mistaken_for_a_table_and_the_reverse() {
    let log = fixture("synthetic/leveldb/000003.log");
    let table = fixture("synthetic/leveldb/save.ldb");
    let ctx = saveprism_core::SniffCtx {
        file_name: Some("000003.log"),
        depth: 0,
    };
    assert_eq!(
        LevelDbSstCodec.sniff(&log, &ctx),
        saveprism_core::Confidence::NONE
    );
    let ctx = saveprism_core::SniffCtx {
        file_name: Some("save.ldb"),
        depth: 0,
    };
    assert_eq!(
        LevelDbLogCodec.sniff(&table, &ctx),
        saveprism_core::Confidence::NONE
    );
}

fn code_of(codec: &dyn Codec, bytes: &[u8]) -> saveprism_core::ErrorCode {
    decode_with(codec, bytes).unwrap_err().code()
}

#[test]
fn bad_headers_and_truncations_are_errors_not_panics() {
    assert_eq!(code_of(&LevelDbSstCodec, b""), LEVELDB_PARSE);
    assert_eq!(code_of(&LevelDbSstCodec, b"not a table"), LEVELDB_PARSE);
    let table = fixture("synthetic/leveldb/save.ldb");
    for end in 0..table.len() {
        assert!(
            decode_with(&LevelDbSstCodec, &table[..end]).is_err(),
            "table prefix {end} was accepted"
        );
    }
    let log = fixture("synthetic/leveldb/000003.log");
    // The first record ends at byte 100: that prefix is a complete, valid log. Every other cut is
    // inside a record.
    for end in 0..log.len() {
        if end == 100 {
            assert!(decode_with(&LevelDbLogCodec, &log[..end]).is_ok());
        } else {
            assert!(
                decode_with(&LevelDbLogCodec, &log[..end]).is_err(),
                "log prefix {end} was accepted"
            );
        }
    }
    let mut corrupt = table;
    if let Some(byte) = corrupt.get_mut(0) {
        *byte ^= 0xff;
    }
    assert!(decode_with(&LevelDbSstCodec, &corrupt).is_err());
    let mut corrupt = fixture("synthetic/leveldb/000003.log");
    if let Some(byte) = corrupt.get_mut(7) {
        *byte ^= 0xff;
    }
    assert!(decode_with(&LevelDbLogCodec, &corrupt).is_err());
}

#[test]
fn the_codecs_are_registered() {
    let mut reg = Registry::default();
    crate::register_all(&mut reg);
    assert!(reg.is_codec("format.leveldb-sst"));
    assert!(reg.is_codec("format.leveldb-log"));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn random_input_never_panics(data in proptest::collection::vec(any::<u8>(), 0..512)) {
        let _ = decode_with(&LevelDbSstCodec, &data);
        let _ = decode_with(&LevelDbLogCodec, &data);
    }
}
