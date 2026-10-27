//! Tests of the LiteDB codec.

use std::path::PathBuf;

use proptest::prelude::*;
use saveprism_core::detect::{DetectOptions, detect};
use saveprism_core::guard::{Verdict, check_round_trip};
use saveprism_core::{
    Codec, DecodeCtx, Decoded, Edit, FloatWidth, Limits, NodeFlags, OwnedValue, Path, Registry,
    Value, apply_edits,
};

use super::*;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn fixture(relative: &str) -> Vec<u8> {
    std::fs::read(fixtures().join(relative)).unwrap()
}

fn decode_with(bytes: &[u8]) -> saveprism_core::Result<Decoded> {
    let limits = Limits::default();
    let mut ctx = DecodeCtx::new(&limits);
    LiteDbCodec.decode(bytes, &mut ctx)
}

fn exact(bytes: &[u8]) -> Decoded {
    let decoded = decode_with(bytes).unwrap();
    assert_eq!(
        LiteDbCodec.encode(&decoded.doc, &decoded.params).unwrap(),
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
fn the_v5_fixture_reads_as_collections_and_documents() {
    let decoded = exact(&fixture("synthetic/litedb/game.db"));
    let doc = &decoded.doc;
    assert_eq!(
        at(doc, "").type_hint.as_ref().unwrap().as_str(),
        "LITEDB:collections"
    );
    assert_eq!(
        at(doc, "/player").type_hint.as_ref().unwrap().as_str(),
        "LITEDB:collection"
    );
    assert_eq!(
        at(doc, "/player/0").type_hint.as_ref().unwrap().as_str(),
        "LITEDB:document"
    );
    assert_eq!(
        at(doc, "/player/0/gold")
            .type_hint
            .as_ref()
            .unwrap()
            .as_str(),
        "BSON:int32"
    );
    match at(doc, "/player/0/gold").value {
        Value::Int { v, .. } => assert_eq!(v, 1250),
        ref other => panic!("gold is {other:?}"),
    }
    assert_eq!(at(doc, "/player/0/name").value, Value::Str("Aria".into()));
    match at(doc, "/player/0/hp").value {
        Value::Float {
            v,
            width: FloatWidth::F64,
        } => assert_eq!(v, 87.5),
        ref other => panic!("hp is {other:?}"),
    }
    assert_eq!(at(doc, "/player/0/alive").value, Value::Bool(true));
    match at(doc, "/player/1/gold").value {
        Value::Int { v, .. } => assert_eq!(v, 40),
        ref other => panic!("gold is {other:?}"),
    }
    assert!(at(doc, "").flags.contains(NodeFlags::READ_ONLY));
    // `_id` and the indexed `gold` field are read-only; `level` is not indexed.
    assert!(
        at(doc, "/player/0/_id")
            .flags
            .contains(NodeFlags::READ_ONLY)
    );
    assert!(
        at(doc, "/player/0/gold")
            .flags
            .contains(NodeFlags::READ_ONLY)
    );
    assert!(
        !at(doc, "/player/0/level")
            .flags
            .contains(NodeFlags::READ_ONLY)
    );
    assert!(
        at(doc, "/player/0/name")
            .flags
            .contains(NodeFlags::READ_ONLY)
    );
    // The index is the reason only for `_id` and `gold`; text is read-only for its length.
    for (path, indexed) in [
        ("/player/0/_id", true),
        ("/player/0/gold", true),
        ("/player/0/level", false),
        ("/player/0/name", false),
    ] {
        assert_eq!(
            at(doc, path).flags.contains(NodeFlags::INDEXED),
            indexed,
            "{path}"
        );
    }
}

#[test]
fn fixed_size_values_are_patched_in_place() {
    let original = fixture("synthetic/litedb/game.db");
    let mut decoded = decode_with(&original).unwrap();
    set(&mut decoded, "/player/0/level", OwnedValue::Int(42)).unwrap();
    set(&mut decoded, "/player/0/hp", OwnedValue::Float(12.25)).unwrap();
    set(&mut decoded, "/player/1/alive", OwnedValue::Bool(true)).unwrap();
    set(&mut decoded, "/player/1/level", OwnedValue::Int(-5)).unwrap();
    let written = LiteDbCodec.encode(&decoded.doc, &decoded.params).unwrap();
    assert_eq!(written.len(), original.len());
    assert_ne!(written, original);
    let again = exact(&written);
    match at(&again.doc, "/player/0/level").value {
        Value::Int { v, .. } => assert_eq!(v, 42),
        ref other => panic!("level is {other:?}"),
    }
    match at(&again.doc, "/player/0/hp").value {
        Value::Float { v, .. } => assert_eq!(v, 12.25),
        ref other => panic!("hp is {other:?}"),
    }
    assert_eq!(at(&again.doc, "/player/1/alive").value, Value::Bool(true));
    match at(&again.doc, "/player/1/level").value {
        Value::Int { v, .. } => assert_eq!(v, -5),
        ref other => panic!("level is {other:?}"),
    }
    // The indexed value is untouched.
    match at(&again.doc, "/player/0/gold").value {
        Value::Int { v, .. } => assert_eq!(v, 1250),
        ref other => panic!("gold is {other:?}"),
    }
    // The untouched document is still intact.
    assert_eq!(
        at(&again.doc, "/player/0/name").value,
        Value::Str("Aria".into())
    );
}

#[test]
fn length_changing_and_out_of_range_edits_are_refused() {
    let mut decoded = decode_with(&fixture("synthetic/litedb/game.db")).unwrap();
    let error = set(
        &mut decoded,
        "/player/0/name",
        OwnedValue::Str("A longer name".into()),
    )
    .unwrap_err();
    assert_eq!(error.code(), saveprism_core::codes::CORE_READ_ONLY);
    set(
        &mut decoded,
        "/player/0/level",
        OwnedValue::Str("lots".into()),
    )
    .unwrap();
    let error = LiteDbCodec
        .encode(&decoded.doc, &decoded.params)
        .unwrap_err();
    assert_eq!(error.code(), LITEDB_WRITE);
    let mut decoded = decode_with(&fixture("synthetic/litedb/game.db")).unwrap();
    assert!(
        set(
            &mut decoded,
            "/player/0/level",
            OwnedValue::Int(i128::from(i32::MAX) + 1),
        )
        .is_err(),
        "an out-of-range integer edit must be refused"
    );
}

#[test]
fn the_v4_legacy_fixture_reads_extended_documents() {
    let decoded = exact(&fixture("synthetic/litedb/legacy-v4.db"));
    let doc = &decoded.doc;
    assert_eq!(at(doc, "/player/0/name").value, Value::Str("Aria".into()));
    assert_eq!(at(doc, "/player/1/name").value, Value::Str("Bram".into()));
    match at(doc, "/player/0/gold").value {
        Value::Int { v, .. } => assert_eq!(v, 1250),
        ref other => panic!("gold is {other:?}"),
    }
    // `_id` and the indexed `gold` field are read-only; `level` is not indexed.
    assert!(
        at(doc, "/player/0/_id")
            .flags
            .contains(NodeFlags::READ_ONLY)
    );
    assert!(
        at(doc, "/player/0/gold")
            .flags
            .contains(NodeFlags::READ_ONLY)
    );
    assert!(
        !at(doc, "/player/0/level")
            .flags
            .contains(NodeFlags::READ_ONLY)
    );
    // The extended document has no contiguous span, so its values are read-only.
    assert!(
        at(doc, "/player/1/level")
            .flags
            .contains(NodeFlags::READ_ONLY)
    );
    let mut decoded = decoded;
    set(&mut decoded, "/player/0/level", OwnedValue::Int(7)).unwrap();
    let written = LiteDbCodec.encode(&decoded.doc, &decoded.params).unwrap();
    let again = exact(&written);
    match at(&again.doc, "/player/0/level").value {
        Value::Int { v, .. } => assert_eq!(v, 7),
        ref other => panic!("level is {other:?}"),
    }
    assert_eq!(
        at(&again.doc, "/player/1/name").value,
        Value::Str("Bram".into())
    );
}

#[test]
fn detection_keeps_sqlite_and_litedb_apart() {
    let mut reg = Registry::default();
    crate::register_all(&mut reg);
    let limits = Limits::default();
    for (file, path) in [
        ("game.db", "synthetic/litedb/game.db"),
        ("legacy-v4.db", "synthetic/litedb/legacy-v4.db"),
    ] {
        let bytes = fixture(path);
        let options = DetectOptions {
            file_name: Some(file.to_string()),
            ..DetectOptions::default()
        };
        let found = detect(&bytes, &reg, &limits, &options).unwrap();
        assert_eq!(found.best.recipe.kinds(), vec!["format.litedb"]);
        let report = check_round_trip(&bytes, &found.best.recipe, &reg, &limits).unwrap();
        assert_eq!(report.overall, Verdict::Exact);
        assert!(report.editable);
    }
    let sqlite = fixture("synthetic/sqlite/game.sqlite");
    let options = DetectOptions {
        file_name: Some("game.db".to_string()),
        ..DetectOptions::default()
    };
    let found = detect(&sqlite, &reg, &limits, &options).unwrap();
    assert_eq!(found.best.recipe.kinds(), vec!["format.sqlite"]);
    let ctx = saveprism_core::SniffCtx {
        file_name: Some("game.db"),
        depth: 0,
    };
    assert_eq!(
        LiteDbCodec.sniff(&sqlite, &ctx),
        saveprism_core::Confidence::NONE
    );
}

#[test]
fn truncations_and_corruption_are_errors_not_panics() {
    for path in ["synthetic/litedb/game.db", "synthetic/litedb/legacy-v4.db"] {
        let bytes = fixture(path);
        let mut end = 0;
        while end < bytes.len() {
            assert!(
                decode_with(&bytes[..end]).is_err(),
                "{path} prefix {end} was accepted"
            );
            end += 89;
        }
        let mut corrupt = bytes;
        if let Some(byte) = corrupt.get_mut(32) {
            *byte = b'X';
        }
        assert!(decode_with(&corrupt).is_err());
    }
    assert_eq!(decode_with(b"").unwrap_err().code(), LITEDB_PARSE);
}

#[test]
fn the_writer_refuses_an_indexed_field_even_with_the_flag_cleared() {
    for path in ["synthetic/litedb/game.db", "synthetic/litedb/legacy-v4.db"] {
        let mut decoded = decode_with(&fixture(path)).unwrap();
        for field in ["_id", "gold"] {
            let node_path = format!("/player/0/{field}");
            let id = decoded
                .doc
                .resolve(&Path::parse(&node_path).unwrap())
                .unwrap();
            assert!(
                at(&decoded.doc, &node_path)
                    .flags
                    .contains(NodeFlags::READ_ONLY),
                "{path}: {field} must be read-only"
            );
            decoded.doc.set_flags(id, NodeFlags::empty()).unwrap();
            set(&mut decoded, &node_path, OwnedValue::Int(77)).unwrap();
            let error = LiteDbCodec
                .encode(&decoded.doc, &decoded.params)
                .unwrap_err();
            assert_eq!(error.code(), LITEDB_WRITE, "{path}: {field}");
            assert!(
                error.message().contains("index"),
                "{path}: {field}: {}",
                error.message()
            );
        }
    }
}

#[test]
fn a_complex_index_expression_makes_the_whole_collection_read_only() {
    for (path, needle) in [
        ("synthetic/litedb/game.db", b"$.gold".as_slice()),
        ("synthetic/litedb/legacy-v4.db", b"gold=$.gold".as_slice()),
    ] {
        let mut bytes = fixture(path);
        let expression_at = bytes
            .windows(needle.len())
            .position(|window| window == needle)
            .unwrap_or_else(|| panic!("{path}: index expression not found"));
        let dollar = expression_at + needle.len() - b"$.gold".len();
        // Turn `$.gold` into an equally long, non-simple `$.g-ld` so no offset moves.
        let byte = bytes
            .get_mut(dollar + 3)
            .unwrap_or_else(|| panic!("{path}: expression is cut short"));
        assert_eq!(*byte, b'o', "{path}: expression offset");
        *byte = b'-';
        let decoded = decode_with(&bytes).unwrap();
        for field in ["level", "hp", "name", "gold"] {
            assert!(
                at(&decoded.doc, &format!("/player/0/{field}"))
                    .flags
                    .contains(NodeFlags::READ_ONLY),
                "{path}: {field} must be read-only"
            );
        }
    }
}

#[test]
fn only_simple_index_paths_are_accepted() {
    assert_eq!(read::simple_first("$._id"), Some("_id".to_string()));
    assert_eq!(
        read::simple_first("$.Address.City"),
        Some("Address".to_string())
    );
    assert_eq!(read::simple_first("LOWER($.Name)"), None);
    assert_eq!(read::simple_first("$.Items[*].Id"), None);
    assert_eq!(read::simple_first("$"), None);
    assert_eq!(read::simple_first("$.Name + 1"), None);
}

#[test]
fn an_unreadable_collection_page_makes_the_collection_read_only() {
    let mut bytes = fixture("synthetic/litedb/game.db");
    // A bogus index count makes the v5 collection page unreadable as an index list.
    if let Some(byte) = bytes.get_mut(8192 + 96) {
        *byte = 200;
    }
    let decoded = decode_with(&bytes).unwrap();
    assert!(
        at(&decoded.doc, "/player/0/level")
            .flags
            .contains(NodeFlags::READ_ONLY)
    );
    assert!(
        at(&decoded.doc, "/player/0/hp")
            .flags
            .contains(NodeFlags::READ_ONLY)
    );
}

#[test]
fn the_codec_is_registered() {
    let mut reg = Registry::default();
    crate::register_all(&mut reg);
    assert!(reg.is_codec("format.litedb"));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn random_input_never_panics(data in proptest::collection::vec(any::<u8>(), 0..512)) {
        let _ = decode_with(&data);
    }
}
