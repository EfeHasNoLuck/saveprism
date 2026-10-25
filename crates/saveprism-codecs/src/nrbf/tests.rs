//! Tests of the .NET BinaryFormatter (MS-NRBF) codec.

use std::path::PathBuf;

use proptest::prelude::*;
use saveprism_core::detect::{DetectOptions, detect};
use saveprism_core::{
    Codec, DecodeCtx, Decoded, Edit, Limits, NodeFlags, NodeId, OwnedValue, Path, Value,
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
    NrbfCodec.decode(bytes, &mut ctx)
}

fn decode(bytes: &[u8]) -> Decoded {
    decode_with(bytes, &Limits::default()).unwrap()
}

fn encode(decoded: &Decoded) -> Vec<u8> {
    NrbfCodec.encode(&decoded.doc, &decoded.params).unwrap()
}

/// Decodes, checks that writing gives the same bytes, returns the decoded document.
fn exact(bytes: &[u8]) -> Decoded {
    let decoded = decode(bytes);
    let written = encode(&decoded);
    assert_eq!(written, bytes, "re-encoded bytes differ");
    decoded
}

fn id_at(doc: &saveprism_core::SaveDoc, path: &str) -> NodeId {
    doc.resolve(&Path::parse(path).unwrap())
        .unwrap_or_else(|| panic!("no node at {path}"))
}

fn at<'a>(doc: &'a saveprism_core::SaveDoc, path: &str) -> &'a saveprism_core::Node {
    doc.get(id_at(doc, path)).unwrap()
}

fn hint_of<'a>(doc: &'a saveprism_core::SaveDoc, path: &str) -> &'a str {
    at(doc, path).type_hint.as_ref().unwrap().as_str()
}

fn int_at(doc: &saveprism_core::SaveDoc, path: &str) -> i128 {
    match at(doc, path).value {
        Value::Int { v, .. } => v,
        ref other => panic!("{path} is {other:?}"),
    }
}

fn float_at(doc: &saveprism_core::SaveDoc, path: &str) -> f64 {
    match at(doc, path).value {
        Value::Float { v, .. } => v,
        ref other => panic!("{path} is {other:?}"),
    }
}

fn read_only(doc: &saveprism_core::SaveDoc, path: &str) -> bool {
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

fn try_set(decoded: &mut Decoded, path: &str, value: OwnedValue) -> saveprism_core::Result<()> {
    apply_edits(
        &mut decoded.doc,
        &[Edit::Set {
            path: Path::parse(path).unwrap(),
            value,
        }],
        &Limits::default(),
    )
}

fn all_fixtures() -> Vec<PathBuf> {
    let mut found = Vec::new();
    let dir = fixtures().join("synthetic/nrbf");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|e| e.to_str()) != Some("gz") {
            found.push(path);
        }
    }
    found.sort();
    found
}

fn i32b(value: i32) -> Vec<u8> {
    value.to_le_bytes().to_vec()
}

fn lp(text: &str) -> Vec<u8> {
    let mut out = Vec::new();
    put_string(&mut out, text).unwrap();
    out
}

fn header(root: i32) -> Vec<u8> {
    let mut out = vec![0];
    out.extend(i32b(root));
    out.extend(i32b(0));
    out.extend(i32b(1));
    out.extend(i32b(0));
    out
}

// ----- fixtures ---------------------------------------------------------------------------------

#[test]
fn every_fixture_round_trips_exactly() {
    let files = all_fixtures();
    assert!(files.len() >= 6, "{files:?}");
    for path in files {
        let bytes = std::fs::read(&path).unwrap();
        let decoded = decode_with(&bytes, &Limits::default())
            .unwrap_or_else(|error| panic!("{}: {}", path.display(), error.report()));
        let written = NrbfCodec.encode(&decoded.doc, &decoded.params).unwrap();
        assert!(written == bytes, "{} does not round-trip", path.display());
    }
}

#[test]
fn player_save_reads_like_game_data() {
    let decoded = exact(&fixture("synthetic/nrbf/player.sav"));
    let doc = &decoded.doc;
    assert_eq!(hint_of(doc, ""), "NRBF:class:PlayerData@Assembly-CSharp");
    assert_eq!(int_at(doc, "/gold"), 1250);
    assert_eq!(float_at(doc, "/health"), 87.5);
    assert_eq!(at(doc, "/alive").value, Value::Bool(true));
    assert_eq!(at(doc, "/name").value, Value::Str("Ada".into()));
    assert_eq!(hint_of(doc, "/gold"), "NRBF:primitive:8");
    assert_eq!(hint_of(doc, "/health"), "NRBF:primitive:11");

    assert_eq!(hint_of(doc, "/inventory"), "NRBF:array");
    assert_eq!(doc.children(id_at(doc, "/inventory")), {
        let ids: Vec<NodeId> = (0..4)
            .map(|index| id_at(doc, &format!("/inventory/{index}")))
            .collect();
        ids
    });
    assert_eq!(int_at(doc, "/inventory/2"), 8);

    // List`1 shows only the live part of _items.
    assert!(
        hint_of(doc, "/list").starts_with("NRBF:class:System.Collections.Generic.List`1"),
        "{}",
        hint_of(doc, "/list")
    );
    assert_eq!(int_at(doc, "/list/_size"), 2);
    assert_eq!(doc.children(id_at(doc, "/list/_items")).len(), 2);
    assert_eq!(at(doc, "/list/_items/0").value, Value::Str("alpha".into()));

    // Dictionary`2 keeps its key/value objects and every value stays editable.
    assert!(
        hint_of(doc, "/lookup").starts_with("NRBF:class:System.Collections.Generic.Dictionary`2"),
        "{}",
        hint_of(doc, "/lookup")
    );
    assert_eq!(
        at(doc, "/lookup/KeyValuePairs/0/key").value,
        Value::Str("arrows".into())
    );
    assert_eq!(int_at(doc, "/lookup/KeyValuePairs/0/value"), 42);

    assert_eq!(
        hint_of(doc, "/companion"),
        "NRBF:class:CompanionData@Assembly-CSharp"
    );
    assert_eq!(
        at(doc, "/companion/nickname").value,
        Value::Str("Pip".into())
    );

    // A string with one MemberReference stays editable; two references make it shared.
    let pet_name = id_at(doc, "/petName");
    assert!(matches!(at(doc, "/petAlias").value, Value::Ref(target) if target == pet_name));
    let title = id_at(doc, "/title");
    assert!(matches!(at(doc, "/titleAlias").value, Value::Ref(target) if target == title));
    assert!(matches!(at(doc, "/titleAlias2").value, Value::Ref(target) if target == title));
    assert!(!read_only(doc, "/petName"));
    assert!(read_only(doc, "/title"));

    // Nulls covered by one ObjectNullMultiple256 record are still three members.
    for member in ["emptyA", "emptyB", "emptyC"] {
        assert_eq!(at(doc, &format!("/{member}")).value, Value::Null);
        assert!(read_only(doc, &format!("/{member}")));
    }

    assert_eq!(at(doc, "/initial").value, Value::Str("A".into()));
    assert!(read_only(doc, "/created"));
    assert_eq!(hint_of(doc, "/created"), "NRBF:primitive:13");
}

#[test]
fn system_class_only_stream_has_no_library() {
    let decoded = exact(&fixture("synthetic/nrbf/system-class.nrbf"));
    let doc = &decoded.doc;
    assert_eq!(hint_of(doc, ""), "NRBF:class:System.Exception");
    assert_eq!(
        at(doc, "/_message").value,
        Value::Str("an invented system class".into())
    );
    assert!(read_only(doc, "/_data"));
}

#[test]
fn class_with_id_reuses_metadata_and_typed_primitives_work() {
    let decoded = exact(&fixture("synthetic/nrbf/class-with-id.nrbf"));
    let doc = &decoded.doc;
    assert_eq!(hint_of(doc, "/first"), "NRBF:class:Point@GameLogic");
    assert_eq!(hint_of(doc, "/second"), "NRBF:class:Point@GameLogic");
    assert_eq!(int_at(doc, "/first/x"), 4);
    assert_eq!(int_at(doc, "/second/x"), 1);
    assert_eq!(int_at(doc, "/bonus"), 42);
    assert_eq!(hint_of(doc, "/bonus"), "NRBF:primitive:8");
    assert_eq!(at(doc, "/label").value, Value::Str("vip".into()));
    assert_eq!(at(doc, "/active").value, Value::Bool(true));
}

#[test]
fn jagged_binary_array_reads_as_nested_arrays() {
    let decoded = exact(&fixture("synthetic/nrbf/jagged.nrbf"));
    let doc = &decoded.doc;
    assert_eq!(hint_of(doc, ""), "NRBF:array");
    assert_eq!(doc.children(doc.root()).len(), 2);
    assert_eq!(int_at(doc, "/0/0"), 1);
    assert_eq!(int_at(doc, "/1/1"), 5);
}

#[test]
fn forward_references_resolve_to_the_later_object() {
    let decoded = exact(&fixture("synthetic/nrbf/forward.nrbf"));
    let doc = &decoded.doc;
    let target = match at(doc, "/buddy").value {
        Value::Ref(target) => target,
        ref other => panic!("/buddy is {other:?}"),
    };
    let buddy = doc.get(target).unwrap();
    assert_eq!(
        buddy.type_hint.as_ref().unwrap().as_str(),
        "NRBF:class:BuddyData@Assembly-CSharp"
    );
}

#[test]
fn rags_extension_and_label_are_recognised() {
    let bytes = fixture("synthetic/nrbf/rags.rsv");
    let decoded = exact(&bytes);
    assert_eq!(hint_of(&decoded.doc, ""), "NRBF:class:RagsSaveData@RAGS");
    let codec = NrbfCodec;
    let ctx = saveprism_core::SniffCtx {
        file_name: Some("slot.rsv"),
        depth: 0,
    };
    assert!(codec.sniff(&bytes, &ctx) > saveprism_core::Confidence::HIGH);
}

#[test]
fn gzip_wrapped_save_is_detected_behind_the_layer() {
    let bytes = fixture("synthetic/nrbf/player.sav.gz");
    let mut reg = saveprism_core::Registry::default();
    crate::register_all(&mut reg);
    let detection = detect(
        &bytes,
        &reg,
        &Limits::default(),
        &DetectOptions {
            file_name: Some("player.sav.gz".into()),
            ..DetectOptions::default()
        },
    )
    .unwrap();
    let kinds = detection.best.recipe.kinds();
    assert_eq!(kinds, ["compression.gzip", "format.nrbf"]);
}

// ----- edits ------------------------------------------------------------------------------------

#[test]
fn scalar_edits_round_trip_with_their_exact_type() {
    let mut decoded = exact(&fixture("synthetic/nrbf/player.sav"));
    set(&mut decoded, "/gold", OwnedValue::Int(500));
    set(&mut decoded, "/health", OwnedValue::Float(12.5));
    set(&mut decoded, "/alive", OwnedValue::Bool(false));
    set(&mut decoded, "/name", OwnedValue::Str("Grace".into()));
    set(&mut decoded, "/petName", OwnedValue::Str("Mochi II".into()));
    set(
        &mut decoded,
        "/lookup/KeyValuePairs/0/value",
        OwnedValue::Int(-1),
    );
    let written = encode(&decoded);
    assert!(written.len() != fixture("synthetic/nrbf/player.sav").len());
    let again = decode(&written);
    assert_eq!(int_at(&again.doc, "/gold"), 500);
    assert_eq!(float_at(&again.doc, "/health"), 12.5);
    assert_eq!(at(&again.doc, "/alive").value, Value::Bool(false));
    assert_eq!(at(&again.doc, "/name").value, Value::Str("Grace".into()));
    assert_eq!(
        at(&again.doc, "/petName").value,
        Value::Str("Mochi II".into())
    );
    assert_eq!(int_at(&again.doc, "/lookup/KeyValuePairs/0/value"), -1);
    assert_eq!(hint_of(&again.doc, "/gold"), "NRBF:primitive:8");
}

#[test]
fn int_edits_are_range_checked() {
    let mut decoded = exact(&fixture("synthetic/nrbf/player.sav"));
    let error = try_set(
        &mut decoded,
        "/gold",
        OwnedValue::Int(i128::from(i32::MAX) + 1),
    )
    .unwrap_err();
    assert_eq!(error.code(), codes::CORE_EDIT_FAILED);
    // The document is unchanged after a failed edit.
    assert_eq!(int_at(&decoded.doc, "/gold"), 1250);
}

#[test]
fn shared_strings_are_read_only_but_single_references_are_editable() {
    let mut decoded = exact(&fixture("synthetic/nrbf/player.sav"));
    let error = try_set(&mut decoded, "/title", OwnedValue::Str("Villain".into())).unwrap_err();
    assert_eq!(error.code(), codes::CORE_READ_ONLY);
    // A MemberReference node itself is also read-only.
    let error = try_set(&mut decoded, "/titleAlias", OwnedValue::Null).unwrap_err();
    assert_eq!(error.code(), codes::CORE_READ_ONLY);

    set(&mut decoded, "/petName", OwnedValue::Str("Mochi".into()));
    let again = decode(&encode(&decoded));
    assert_eq!(at(&again.doc, "/petName").value, Value::Str("Mochi".into()));
}

// ----- malformed input --------------------------------------------------------------------------

#[test]
fn bad_and_truncated_input_errors_without_panics() {
    assert!(decode_with(&[], &Limits::default()).is_err());
    assert!(decode_with(&header(10), &Limits::default()).is_err());

    let bytes = fixture("synthetic/nrbf/player.sav");
    for cut in [0, 1, 8, 16, 17, 20, 30, 60] {
        assert!(
            decode_with(&bytes[..cut], &Limits::default()).is_err(),
            "a {cut} byte prefix decoded"
        );
    }

    let mut bad_version = bytes.clone();
    bad_version[9] = 2;
    assert!(decode_with(&bad_version, &Limits::default()).is_err());

    // Every prefix is either valid or an error, but never a panic.
    for cut in 0..bytes.len() {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            decode_with(&bytes[..cut], &Limits::default())
        }));
        assert!(result.is_ok(), "decoding a {cut} byte prefix panicked");
    }
}

#[test]
fn unknown_references_and_metadata_cycles_error() {
    // MemberReference to an object that is never defined.
    let mut dangling = header(10);
    dangling.extend([9]);
    dangling.extend(i32b(99));
    dangling.push(11);
    assert!(decode_with(&dangling, &Limits::default()).is_err());

    // ClassWithId whose metadata is itself: metadata must exist before it is reused.
    let mut cycle = header(10);
    cycle.push(1);
    cycle.extend(i32b(10));
    cycle.extend(i32b(10));
    cycle.push(11);
    assert!(decode_with(&cycle, &Limits::default()).is_err());

    // A MemberReference cycle between real objects is a valid graph.
    let mut self_ref = header(10);
    self_ref.push(5);
    self_ref.extend(i32b(10));
    self_ref.extend(lp("Node"));
    self_ref.extend(i32b(1));
    self_ref.extend(lp("next"));
    self_ref.push(2); // Object member
    self_ref.extend(i32b(1)); // LibraryId
    self_ref.push(9);
    self_ref.extend(i32b(10));
    self_ref.push(11);
    let decoded = decode(&self_ref);
    assert!(matches!(
        decoded.doc.get(id_at(&decoded.doc, "/next")).unwrap().value,
        Value::Ref(target) if target == decoded.doc.root()
    ));
}

#[test]
fn a_truncated_string_length_is_an_error() {
    let mut bytes = header(10);
    bytes.push(6); // BinaryObjectString
    bytes.extend(i32b(10));
    bytes.push(0x7F); // 127 byte string, but nothing follows
    bytes.push(11);
    assert!(decode_with(&bytes, &Limits::default()).is_err());
}

// ----- properties -------------------------------------------------------------------------------

/// When random input happens to decode, writing it must succeed and read back as the same tree.
fn writes_back(bytes: &[u8]) {
    let Ok(decoded) = decode_with(bytes, &Limits::default()) else {
        return;
    };
    let written = NrbfCodec
        .encode(&decoded.doc, &decoded.params)
        .expect("a decoded document encodes");
    let again = decode_with(&written, &Limits::default()).expect("written data decodes");
    assert!(
        decoded.doc.tree_eq(&again.doc),
        "the written data reads back differently"
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn arbitrary_bytes_never_panic(body in proptest::collection::vec(any::<u8>(), 0..512)) {
        let mut data = header(10);
        data.extend(body);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            writes_back(&data);
        }));
        prop_assert!(result.is_ok());
    }

    #[test]
    fn mutated_saves_never_panic(position in 0_usize..900, byte in any::<u8>()) {
        let mut bytes = fixture("synthetic/nrbf/player.sav");
        if let Some(slot) = bytes.get_mut(position) {
            *slot = byte;
        }
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            writes_back(&bytes);
        }));
        prop_assert!(result.is_ok());
    }

    #[test]
    fn edited_int32_round_trips(value in any::<i32>()) {
        let mut decoded = exact(&fixture("synthetic/nrbf/player.sav"));
        set(&mut decoded, "/gold", OwnedValue::Int(i128::from(value)));
        let again = decode(&encode(&decoded));
        prop_assert_eq!(int_at(&again.doc, "/gold"), i128::from(value));
        prop_assert_eq!(hint_of(&again.doc, "/gold"), "NRBF:primitive:8");
    }

    #[test]
    fn edited_string_round_trips(value in "[ -~]{0,40}") {
        let mut decoded = exact(&fixture("synthetic/nrbf/player.sav"));
        set(&mut decoded, "/name", OwnedValue::Str(value.clone()));
        let again = decode(&encode(&decoded));
        prop_assert_eq!(&at(&again.doc, "/name").value, &Value::Str(value));
    }
}

#[test]
fn a_huge_run_of_nulls_in_a_tiny_file_is_refused() {
    // Found by fuzzing: a 1.0 header and one ObjectNullMultiple record of 9,474,609 nulls.
    let mut bytes = vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0];
    bytes.extend_from_slice(&[0x0e, 0x31, 0x92, 0x90, 0x00, 0x06]);
    let error = decode_with(&bytes, &Limits::default()).unwrap_err();
    assert_eq!(error.code(), codes::CORE_LIMIT_NODES);
}
