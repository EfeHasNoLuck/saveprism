//! Tests of the pickle codec.

use std::path::PathBuf;

use proptest::prelude::*;
use saveprism_core::detect::{DetectOptions, detect};
use saveprism_core::{
    Codec, DecodeCtx, Decoded, Edit, Layer, Limits, Node, NodeFlags, NodeId, OwnedValue, Path,
    Position, Registry, SaveDoc, Value, apply_edits, codes, pipeline,
};

use super::*;
use crate::layers::ZlibLayer;
use crate::renpy::RenpySaveLayer;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn fixture(relative: &str) -> Vec<u8> {
    std::fs::read(fixtures().join(relative)).unwrap()
}

const SAVES: [&str; 4] = [
    "synthetic/renpy/renpy8-0/1-1-LT1.save",
    "synthetic/renpy/renpy8-1-signed/1-1-LT1.save",
    "synthetic/renpy/renpy8-3/1-1-LT1.save",
    "synthetic/renpy/renpy7/1-1-LT1.save",
];
const PERSISTENTS: [&str; 2] = [
    "synthetic/renpy/persistent/persistent",
    "synthetic/renpy/persistent-signed/persistent",
];

/// The pickled `log` of a Ren'Py save fixture.
fn log_of(relative: &str) -> Vec<u8> {
    let limits = Limits::default();
    let mut ctx = DecodeCtx::new(&limits);
    RenpySaveLayer
        .decode(&fixture(relative), &mut ctx)
        .unwrap()
        .0
}

/// The pickle inside a `persistent` fixture.
fn persistent_of(relative: &str) -> Vec<u8> {
    let limits = Limits::default();
    let mut ctx = DecodeCtx::new(&limits);
    ZlibLayer.decode(&fixture(relative), &mut ctx).unwrap().0
}

fn decode_with(bytes: &[u8], limits: &Limits) -> saveprism_core::Result<Decoded> {
    let mut ctx = DecodeCtx::new(limits);
    PythonPickleCodec.decode(bytes, &mut ctx)
}

fn decode(bytes: &[u8]) -> Decoded {
    decode_with(bytes, &Limits::default()).unwrap()
}

fn encode(decoded: &Decoded) -> saveprism_core::Result<Vec<u8>> {
    PythonPickleCodec.encode(&decoded.doc, &decoded.params)
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

fn int(v: i128) -> Value {
    Value::Int {
        v,
        width: saveprism_core::IntWidth::Unbounded,
    }
}

/// Decodes `bytes`, writes it back and checks nothing changed.
fn exact(bytes: &[u8]) -> Decoded {
    let decoded = decode(bytes);
    assert_eq!(encode(&decoded).unwrap(), bytes, "re-encoded bytes differ");
    decoded
}

/// Decoding never panics; whatever decodes writes back to the same tree.
fn writes_back(bytes: &[u8]) {
    let limits = Limits {
        max_depth: 64,
        ..Limits::default()
    };
    let Ok(decoded) = decode_with(bytes, &limits) else {
        return;
    };
    let written = encode(&decoded).unwrap();
    assert_eq!(written, bytes);
}

#[test]
fn every_fixture_round_trips_exactly() {
    for path in SAVES {
        exact(&log_of(path));
    }
    for path in PERSISTENTS {
        exact(&persistent_of(path));
    }
}

#[test]
fn fixtures_are_detected_as_renpy_saves_and_pickles() {
    let mut reg = Registry::default();
    crate::register_all(&mut reg);
    let limits = Limits::default();
    for path in SAVES {
        let opts = DetectOptions {
            file_name: Some("1-1-LT1.save".to_string()),
            ..DetectOptions::default()
        };
        let found = detect(&fixture(path), &reg, &limits, &opts).unwrap();
        assert_eq!(
            found.best.recipe.kinds(),
            vec!["container.renpy-save", "format.python-pickle"],
            "{path}"
        );
    }
    for path in PERSISTENTS {
        let opts = DetectOptions {
            file_name: Some("persistent".to_string()),
            ..DetectOptions::default()
        };
        let found = detect(&fixture(path), &reg, &limits, &opts).unwrap();
        assert_eq!(
            found.best.recipe.kinds(),
            vec!["compression.zlib", "format.python-pickle"],
            "{path}"
        );
    }
}

#[test]
fn ren_py_8_store_variables_have_the_expected_shape() {
    for path in SAVES.iter().take(3) {
        let decoded = decode(&log_of(path));
        let doc = &decoded.doc;
        assert_eq!(hint_of(doc, ""), "PY:tuple");
        assert_eq!(hint_of(doc, "/0"), "PY:dict");
        assert_eq!(at(doc, "/0/store.money").value, int(120));
        assert!(!read_only(doc, "/0/store.money"));
        assert_eq!(
            at(doc, "/0/store.hero_name").value,
            Value::Str("Sylvie".into())
        );
        assert_eq!(at(doc, "/0/store.met_eileen").value, Value::Bool(true));
        assert_eq!(at(doc, "/0/store.door_open").value, Value::Bool(false));
        assert_eq!(at(doc, "/0/store.last_choice").value, Value::Null);
        assert_eq!(at(doc, "/0/store.karma").value, int(-7));
        assert_eq!(at(doc, "/0/store.steps").value, int(123_456));
        assert_eq!(at(doc, "/0/store.bank_balance").value, int(1 << 70));
        assert!(!read_only(doc, "/0/store.bank_balance"));
        assert_eq!(hint_of(doc, "/0/store.huge_number"), "PY:bigint");
        assert!(read_only(doc, "/0/store.huge_number"));
        let Value::Str(huge) = &at(doc, "/0/store.huge_number").value else {
            panic!("big integer is not text");
        };
        assert!(huge.starts_with("265613988875874769338781322035779626829233452653394495974574961739092490901302182994384699044001"));
        assert_eq!(
            at(doc, "/0/store.motto").value,
            Value::Str("\u{c7}ok g\u{fc}zel \u{2014} \u{65e5}\u{672c}\u{8a9e} \u{1F600}".into())
        );
        assert_eq!(
            hint_of(doc, "/0/store.stats"),
            "PY:dict:renpy.revertable.RevertableDict"
        );
        assert_eq!(
            at(doc, "/0/store.stats/luck").value,
            Value::Float {
                v: 0.75,
                width: saveprism_core::FloatWidth::F64
            }
        );
        assert_eq!(
            hint_of(doc, "/0/store.visited"),
            "PY:list:renpy.revertable.RevertableList"
        );
        assert_eq!(
            at(doc, "/0/store.visited/1").value,
            Value::Str("cafe".into())
        );
        assert_eq!(
            hint_of(doc, "/0/store.flags_seen"),
            "PY:set:renpy.revertable.RevertableSet"
        );
        assert_eq!(
            at(doc, "/0/store.flags_seen/0").value,
            Value::Str("date1".into())
        );
        assert!(read_only(doc, "/0/store.flags_seen/0"));
        assert_eq!(hint_of(doc, "/0/store.position"), "PY:tuple");
        assert_eq!(
            hint_of(doc, "/0/store.inventory"),
            "PY:object:store.Inventory"
        );
        assert_eq!(at(doc, "/0/store.inventory/gold").value, int(12));
        assert_eq!(hint_of(doc, "/0/store.backpack"), "PY:ref");
        assert_eq!(hint_of(doc, "/0/store.mystery.clue_count"), "PY:int");
        // The rollback log follows the roots; its copy of the inventory is a reference.
        assert_eq!(hint_of(doc, "/1"), "PY:object:renpy.rollback.RollbackLog");
        assert_eq!(hint_of(doc, "/1/current"), "PY:ref");
        assert_eq!(hint_of(doc, "/1/log/0/inventory_snapshot"), "PY:ref");
    }
}

#[test]
fn shared_values_are_read_only() {
    let mut decoded = decode(&log_of(SAVES[0]));
    let doc = &decoded.doc;
    // One string object stored under three names (two variables and an attribute).
    assert!(read_only(doc, "/0/store.player_name"));
    assert_eq!(hint_of(doc, "/0/store.companion_name"), "PY:ref");
    assert!(read_only(doc, "/0/store.companion_name"));
    assert_eq!(hint_of(doc, "/0/store.inventory/owner"), "PY:ref");
    let error = apply_edits(
        &mut decoded.doc,
        &[Edit::Set {
            path: Path::parse("/0/store.player_name").unwrap(),
            value: OwnedValue::Str("Lucy".into()),
        }],
        &Limits::default(),
    )
    .unwrap_err();
    assert_eq!(error.code(), codes::CORE_READ_ONLY);
    // Values inside a shared object are still editable: the object is the same for all names.
    assert!(!read_only(&decoded.doc, "/0/store.inventory/gold"));
}

/// Edits money, a name, a flag and a float in a `RevertableDict`, writes, reads back.
fn edit_and_reopen(log: &[u8], long_name: bool) -> Vec<u8> {
    let mut decoded = decode(log);
    let name = if long_name {
        "N".repeat(300)
    } else {
        "Sylvie the Brave".to_string()
    };
    set(&mut decoded, "/0/store.money", OwnedValue::Int(99_999));
    set(
        &mut decoded,
        "/0/store.hero_name",
        OwnedValue::Str(name.clone()),
    );
    set(&mut decoded, "/0/store.met_eileen", OwnedValue::Bool(false));
    set(&mut decoded, "/0/store.stats/luck", OwnedValue::Float(0.25));
    set(&mut decoded, "/0/store.bank_balance", OwnedValue::Int(-5));
    let written = encode(&decoded).unwrap();
    assert_ne!(written, log);
    let again = decode(&written);
    let doc = &again.doc;
    assert_eq!(at(doc, "/0/store.money").value, int(99_999));
    assert_eq!(at(doc, "/0/store.hero_name").value, Value::Str(name));
    assert_eq!(at(doc, "/0/store.met_eileen").value, Value::Bool(false));
    assert_eq!(
        at(doc, "/0/store.stats/luck").value,
        Value::Float {
            v: 0.25,
            width: saveprism_core::FloatWidth::F64
        }
    );
    assert_eq!(at(doc, "/0/store.bank_balance").value, int(-5));
    // Everything else is the same tree.
    assert!(decoded.doc.tree_eq(&again.doc));
    // Writing the edited document again gives the same bytes.
    assert_eq!(encode(&again).unwrap(), written);
    written
}

#[test]
fn edits_survive_writing_in_protocol_2() {
    let written = edit_and_reopen(&log_of(SAVES[0]), false);
    // 99 999 does not fit BININT2: the edit wrote a BININT.
    assert!(
        written
            .windows(5)
            .any(|w| w == [ops::BININT, 0x9F, 0x86, 0x01, 0x00])
    );
    edit_and_reopen(&log_of(SAVES[0]), true);
}

#[test]
fn edits_survive_writing_in_protocol_5_with_frames() {
    let log = log_of(SAVES[2]);
    assert!(log.contains(&ops::FRAME));
    let written = edit_and_reopen(&log, true);
    // The 300 character name no longer fits SHORT_BINUNICODE, so it became BINUNICODE and the
    // frame that holds it grew.
    let mut long = vec![ops::BINUNICODE, 0x2C, 0x01, 0, 0];
    long.extend_from_slice(&[b'N'; 300]);
    assert!(written.windows(long.len()).any(|w| w == long.as_slice()));
    // Name +297 bytes (8 to 305), money +3 (BININT1 to BININT), bank balance -8 (LONG1 9 to 1).
    assert_eq!(written.len(), log.len() + 297 + 3 - 8);
}

#[test]
fn ren_py_7_python_2_save_reads_and_edits() {
    let log = log_of(SAVES[3]);
    let mut decoded = exact(&log);
    let doc = &decoded.doc;
    assert_eq!(hint_of(doc, "/0"), "PY:dict:renpy.python.RevertableDict");
    assert_eq!(at(doc, "/0/store.money").value, int(250));
    assert_eq!(hint_of(doc, "/0/store.nickname"), "PY:bytestr");
    assert_eq!(at(doc, "/0/store.big_steps").value, int(1 << 40));
    assert_eq!(at(doc, "/0/store.bigger").value, int(1 << 80));
    assert!(read_only(doc, "/0/store.pet_a"));
    assert_eq!(hint_of(doc, "/0/store.pet_b"), "PY:ref");
    assert_eq!(
        hint_of(doc, "/0/store.inventory"),
        "PY:object:store.Inventory"
    );
    assert_eq!(
        hint_of(doc, "/0/store.inventory/items"),
        "PY:list:renpy.python.RevertableList"
    );
    assert_eq!(
        hint_of(doc, "/0/store.note"),
        "PY:object:store.OldStyleNote"
    );
    assert_eq!(
        at(doc, "/0/store.note/text").value,
        Value::Str("plain ascii".into())
    );
    assert_eq!(hint_of(doc, "/1/current_inventory"), "PY:ref");

    set(&mut decoded, "/0/store.big_steps", OwnedValue::Int(7));
    set(
        &mut decoded,
        "/0/store.nickname",
        OwnedValue::Str("Lulu".into()),
    );
    set(&mut decoded, "/0/store.bigger", OwnedValue::Int(1));
    set(&mut decoded, "/0/store.stats/speed", OwnedValue::Float(3.5));
    let written = encode(&decoded).unwrap();
    // A Python 2 int stays an INT line, a long stays LONG1, a byte string stays a byte string.
    assert!(written.windows(3).any(|w| w == b"I7\n"));
    assert!(written.windows(3).any(|w| w == [ops::LONG1, 1, 1]));
    assert!(written.windows(6).any(|w| w == b"U\x04Lulu"));
    let again = decode(&written);
    assert_eq!(at(&again.doc, "/0/store.big_steps").value, int(7));
    assert_eq!(
        at(&again.doc, "/0/store.nickname").value,
        Value::Str("Lulu".into())
    );
}

#[test]
fn persistent_reads_as_an_object() {
    let decoded = exact(&persistent_of(PERSISTENTS[0]));
    let doc = &decoded.doc;
    assert_eq!(hint_of(doc, ""), "PY:object:renpy.persistent.Persistent");
    assert_eq!(at(doc, "/play_count").value, int(4));
    assert_eq!(at(doc, "/ending_good").value, Value::Bool(true));
    assert_eq!(
        hint_of(doc, "/_preferences"),
        "PY:object:renpy.preferences.Preferences"
    );
    assert_eq!(
        python_class(at(doc, "")),
        Some("renpy.persistent.Persistent")
    );
}

#[test]
fn protocol_0_text_pickles_round_trip_and_edit() {
    // [1, True, 1.5, u"hé", "x", 2**70 (LONG), None, {"k": -3}] in protocol 0.
    let bytes: &[u8] =
        b"(lp0\nI1\naI01\naF1.5\naVh\\u00e9\np1\naS'x'\np2\naL1180591620717411303424L\naNa(dp3\nVk\np4\nI-3\nsa.";
    let mut decoded = exact(bytes);
    let doc = &decoded.doc;
    assert_eq!(at(doc, "/0").value, int(1));
    assert_eq!(at(doc, "/1").value, Value::Bool(true));
    assert_eq!(at(doc, "/3").value, Value::Str("h\u{e9}".into()));
    assert!(read_only(doc, "/4"));
    assert_eq!(at(doc, "/5").value, int(1 << 70));
    assert_eq!(at(doc, "/7/k").value, int(-3));
    set(&mut decoded, "/0", OwnedValue::Int(-12));
    set(&mut decoded, "/1", OwnedValue::Bool(false));
    set(&mut decoded, "/2", OwnedValue::Float(2.5));
    set(&mut decoded, "/3", OwnedValue::Str("a\\b\n\u{20ac}".into()));
    set(&mut decoded, "/5", OwnedValue::Int(5));
    let written = encode(&decoded).unwrap();
    assert_eq!(
        written,
        b"(lp0\nI-12\naI00\naF2.5\naVa\\u005cb\\u000a\\u20ac\np1\naS'x'\np2\naL5L\naNa(dp3\nVk\np4\nI-3\nsa."
    );
    let again = decode(&written);
    assert_eq!(
        at(&again.doc, "/3").value,
        Value::Str("a\\b\n\u{20ac}".into())
    );
}

#[test]
fn shape_changes_are_refused() {
    let log = log_of(SAVES[0]);
    let mut decoded = decode(&log);
    apply_edits(
        &mut decoded.doc,
        &[Edit::Insert {
            parent: Path::parse("/0/store.visited").unwrap(),
            at: Position::End,
            value: OwnedValue::Str("beach".into()),
        }],
        &Limits::default(),
    )
    .unwrap();
    assert_eq!(encode(&decoded).unwrap_err().code(), PICKLE_WRITE);

    let mut decoded = decode(&log);
    apply_edits(
        &mut decoded.doc,
        &[Edit::Set {
            path: Path::parse("/0/store.money").unwrap(),
            value: OwnedValue::Str("lots".into()),
        }],
        &Limits::default(),
    )
    .unwrap();
    assert_eq!(encode(&decoded).unwrap_err().code(), PICKLE_WRITE);
}

#[test]
fn malformed_pickles_give_errors() {
    let cases: [(&[u8], ErrorCode); 9] = [
        (b"", PICKLE_PARSE),
        (b"\x80\x02", PICKLE_PARSE),
        (b"\x80\x06N.", PICKLE_UNSUPPORTED),
        (b"\x80\x05\x97.", PICKLE_UNSUPPORTED),
        (b"\x80\x02h\x05.", PICKLE_BAD_MEMO),
        (b"\x80\x02.", PICKLE_PARSE),
        (b"\x80\x02(\x85.", PICKLE_PARSE),
        (b"\x80\x02}K\x01s.", PICKLE_PARSE),
        (b"\x80\x02Na.", PICKLE_PARSE),
    ];
    for (bytes, code) in cases {
        let error = decode_with(bytes, &Limits::default()).unwrap_err();
        assert_eq!(error.code(), code, "{bytes:?}");
    }
}

#[test]
fn truncated_fixtures_give_errors() {
    for path in [SAVES[0], SAVES[2], SAVES[3]] {
        let log = log_of(path);
        let step = (log.len() / 1500).max(1);
        for cut in (0..log.len()).step_by(step) {
            assert!(
                decode_with(log.get(..cut).unwrap(), &Limits::default()).is_err(),
                "{path} cut at {cut}"
            );
        }
    }
}

#[test]
fn deep_nesting_hits_the_depth_limit() {
    let depth = 100_000;
    let mut bytes = b"\x80\x02".to_vec();
    bytes.extend(std::iter::repeat_n(ops::EMPTY_LIST, depth));
    bytes.extend(std::iter::repeat_n(ops::APPEND, depth - 1));
    bytes.push(ops::STOP);
    let error = decode_with(&bytes, &Limits::default()).unwrap_err();
    assert_eq!(error.code(), codes::CORE_LIMIT_DEPTH);
    // Deep MARK nesting is refused too.
    let mut marks = b"\x80\x02".to_vec();
    marks.extend(std::iter::repeat_n(ops::MARK, 10_000));
    marks.extend_from_slice(b"N.");
    let error = decode_with(&marks, &Limits::default()).unwrap_err();
    assert_eq!(error.code(), codes::CORE_LIMIT_DEPTH);
}

#[test]
fn cycles_become_references() {
    // l = []; l.append(l) in protocol 2.
    let bytes = b"\x80\x02]q\x00h\x00a.";
    let decoded = exact(bytes);
    assert_eq!(hint_of(&decoded.doc, "/0"), "PY:ref");
    let Value::Ref(target) = at(&decoded.doc, "/0").value else {
        panic!("not a reference");
    };
    assert_eq!(target, decoded.doc.root());
}

#[test]
fn objects_of_every_kind_have_a_shape() {
    // REDUCE of a callable with arguments, NEWOBJ_EX, a set, a frozenset, bytes, a global value.
    let bytes = b"\x80\x04\
\x8c\x08builtins\x8c\x07complex\x93K\x01K\x02\x86R\
\x8c\x03mod\x8c\x03Cls\x93)}\x8c\x01k\x8c\x01vs\x92\
\x8f(K\x03\x8c\x01a\x90\
\x87\
(K\x05\x91\
C\x02ab\
\x8c\x03mod\x8c\x01f\x93\
\x87\
\x88\x8a\x02\x01\x02\x86\
\x87.";
    let decoded = exact(bytes);
    let doc = &decoded.doc;
    assert_eq!(hint_of(doc, "/0/0"), "PY:instance:builtins.complex");
    assert!(read_only(doc, "/0/0/(args)/0"));
    assert_eq!(hint_of(doc, "/0/1"), "PY:instance:mod.Cls");
    assert_eq!(at(doc, "/0/1/(kwargs)/k").value, Value::Str("v".into()));
    assert_eq!(hint_of(doc, "/0/2"), "PY:set");
    assert!(read_only(doc, "/0/2/1"));
    assert_eq!(hint_of(doc, "/1/0"), "PY:frozenset");
    assert_eq!(at(doc, "/1/1").value, Value::Bytes(b"ab".to_vec()));
    assert_eq!(at(doc, "/1/2").value, Value::Str("mod.f".into()));
    assert_eq!(at(doc, "/2/0").value, Value::Bool(true));
    assert_eq!(at(doc, "/2/1").value, int(0x0201));
}

#[test]
fn sniff_needs_proto_and_stop() {
    let ctx = saveprism_core::SniffCtx::default();
    assert_eq!(
        PythonPickleCodec.sniff(b"\x80\x02N.", &ctx),
        saveprism_core::Confidence::HIGH
    );
    assert_eq!(
        PythonPickleCodec.sniff(b"(lp0\n.", &ctx),
        saveprism_core::Confidence::LOW
    );
    assert_eq!(
        PythonPickleCodec.sniff(b"\x80\x02N", &ctx),
        saveprism_core::Confidence::NONE
    );
    assert_eq!(
        PythonPickleCodec.sniff(b"{\"a\": 1}", &ctx),
        saveprism_core::Confidence::NONE
    );
}

#[test]
fn error_codes_are_stable() {
    assert_eq!(PICKLE_PARSE.as_str(), "SP-PICKLE-001");
    assert_eq!(PICKLE_BAD_MEMO.as_str(), "SP-PICKLE-002");
    assert_eq!(PICKLE_UNSUPPORTED.as_str(), "SP-PICKLE-003");
    assert_eq!(PICKLE_WRITE.as_str(), "SP-PICKLE-004");
}

#[test]
fn the_full_pipeline_edits_a_save_and_empties_its_signatures() {
    let mut reg = Registry::default();
    crate::register_all(&mut reg);
    let limits = Limits::default();
    let bytes = fixture(SAVES[1]);
    let found = detect(&bytes, &reg, &limits, &DetectOptions::default()).unwrap();
    let mut out = pipeline::decode(&bytes, &found.best.recipe, &reg, &limits).unwrap();
    apply_edits(
        &mut out.doc,
        &[Edit::Set {
            path: Path::parse("/0/store.money").unwrap(),
            value: OwnedValue::Int(5000),
        }],
        &limits,
    )
    .unwrap();
    let written = pipeline::encode(&out.doc, &out.recipe, &reg).unwrap();
    let again = pipeline::decode(&written, &out.recipe, &reg, &limits).unwrap();
    assert_eq!(at(&again.doc, "/0/store.money").value, int(5000));
    let mut ctx = DecodeCtx::new(&limits);
    let (_, params) = RenpySaveLayer.decode(&written, &mut ctx).unwrap();
    assert!(!params.bool(crate::renpy::SIGNED_PARAM).unwrap());
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// Random bytes after a protocol header never panic.
    #[test]
    fn random_input_never_panics(body in proptest::collection::vec(any::<u8>(), 0..256)) {
        let mut bytes = b"\x80\x02".to_vec();
        bytes.extend_from_slice(&body);
        bytes.push(b'.');
        writes_back(&bytes);
    }

    /// Random opcode soups reach deeper into the machine.
    #[test]
    fn opcode_soup_never_panics(
        body in proptest::collection::vec(
            prop_oneof![
                proptest::sample::select(
                    b"()]}0123NKMJGaebdsutlhqXUC\x85\x86\x87\x88\x89\x8a\x8c\x8f\x90\x91\x92\x93\x94\x81R".to_vec()
                ),
                0_u8..4,
                any::<u8>(),
            ],
            0..96,
        )
    ) {
        let mut bytes = b"\x80\x04".to_vec();
        bytes.extend_from_slice(&body);
        bytes.push(b'.');
        writes_back(&bytes);
    }

    /// Mutated saves never panic.
    #[test]
    fn mutated_logs_never_panic(position in 0_usize..2048, byte in any::<u8>()) {
        let mut bytes = log_of(SAVES[0]);
        if let Some(slot) = bytes.get_mut(position) {
            *slot = byte;
        }
        writes_back(&bytes);
    }
}

#[test]
fn a_standalone_pickle_is_detected_without_renpy() {
    // Protocol 2, an empty dict with one memo entry, STOP: a complete standalone pickle.
    let bytes = b"\x80\x02}q\x00.";
    let mut reg = Registry::default();
    crate::register_all(&mut reg);
    let opts = DetectOptions {
        file_name: Some("save.pkl".to_string()),
        ..DetectOptions::default()
    };
    let found = detect(bytes, &reg, &Limits::default(), &opts).unwrap();
    assert_eq!(found.best.recipe.kinds(), vec!["format.python-pickle"]);
    exact(bytes);
}
