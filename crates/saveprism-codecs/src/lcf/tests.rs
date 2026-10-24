//! Tests of the LCF codec (RPG Maker 2000 / 2003).

use std::path::PathBuf;

use proptest::prelude::*;
use saveprism_core::{
    Codec, DecodeCtx, Decoded, Edit, Limits, Node, NodeFlags, NodeId, OwnedValue, ParamValue, Path,
    SaveDoc, SniffCtx, Value, apply_edits,
};

use super::*;

const SAVE_2K: &str = "synthetic/rpgmaker-2k/Save01.lsd";
const SAVE_2K3: &str = "synthetic/rpgmaker-2k3/Save01.lsd";
const DB_2K: &str = "synthetic/rpgmaker-2k/RPG_RT.ldb";
const DB_2K3: &str = "synthetic/rpgmaker-2k3/RPG_RT.ldb";

fn fixture(relative: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(relative);
    std::fs::read(path).unwrap()
}

fn decode_result(bytes: &[u8]) -> saveprism_core::Result<Decoded> {
    let limits = Limits::default();
    let mut ctx = DecodeCtx::new(&limits);
    LcfCodec.decode(bytes, &mut ctx)
}

fn decode(bytes: &[u8]) -> Decoded {
    decode_result(bytes).unwrap()
}

fn encode(decoded: &Decoded) -> Vec<u8> {
    LcfCodec.encode(&decoded.doc, &decoded.params).unwrap()
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

/// Encodes the edited document and decodes the bytes again.
fn reopen(decoded: &Decoded) -> Decoded {
    decode(&encode(decoded))
}

fn ber(value: u32) -> Vec<u8> {
    let mut out = Vec::new();
    push_ber(&mut out, value);
    out
}

fn chunk(id: u32, data: &[u8]) -> Vec<u8> {
    let mut out = ber(id);
    out.extend(ber(data.len() as u32));
    out.extend_from_slice(data);
    out
}

fn save(chunks: &[Vec<u8>]) -> Vec<u8> {
    let mut out = ber(SAVE_HEADER.len() as u32);
    out.extend_from_slice(SAVE_HEADER);
    for chunk in chunks {
        out.extend_from_slice(chunk);
    }
    out
}

/// A struct body: the chunks and the end mark.
fn body(chunks: &[Vec<u8>]) -> Vec<u8> {
    let mut out = chunks.concat();
    out.push(0);
    out
}

#[test]
fn ber_numbers() {
    for (value, bytes) in [
        (0_u32, vec![0x00]),
        (0x7F, vec![0x7F]),
        (0x80, vec![0x81, 0x00]),
        (5000, vec![0xA7, 0x08]),
        (u32::MAX, vec![0x8F, 0xFF, 0xFF, 0xFF, 0x7F]),
    ] {
        assert_eq!(ber(value), bytes, "{value}");
        assert_eq!(read_ber(&bytes), Some((value, bytes.len())));
    }
    // -1 as uint32.
    assert_eq!(ber(-1_i32 as u32), [0x8F, 0xFF, 0xFF, 0xFF, 0x7F]);
    // Leading zero groups read but are not canonical.
    assert_eq!(read_ber(&[0x80, 0x80, 0x05]), Some((5, 3)));
    assert!(!is_canonical(&[0x80, 0x05]));
    assert!(is_canonical(&[0x00]));
    // Six bytes, too large, truncated.
    assert_eq!(read_ber(&[0x80, 0x80, 0x80, 0x80, 0x80, 0x01]), None);
    assert_eq!(read_ber(&[0x90, 0x80, 0x80, 0x80, 0x00]), None);
    assert_eq!(read_ber(&[0x81]), None);
    assert_eq!(read_ber(&[]), None);
}

#[test]
fn fixtures_are_detected_and_written_back_exactly() {
    for relative in [SAVE_2K, SAVE_2K3, DB_2K, DB_2K3] {
        let bytes = fixture(relative);
        let ctx = SniffCtx {
            file_name: Some(relative),
            depth: 0,
        };
        assert!(LcfCodec.sniff(&bytes, &ctx).value() > 0.9, "{relative}");
        exact(&bytes);
    }
    let ctx = SniffCtx::default();
    assert_eq!(LcfCodec.sniff(b"\x0aLcfMapUnit", &ctx).value(), 0.0);
    assert_eq!(LcfCodec.sniff(b"\x0bLcfSaveDat", &ctx).value(), 0.0);
}

#[test]
fn the_2k_save_becomes_a_tree_of_named_fields() {
    let decoded = exact(&fixture(SAVE_2K));
    let doc = &decoded.doc;
    assert_eq!(hint_of(doc, ""), "LCF:struct:Save");
    assert!(read_only(doc, ""));
    assert_eq!(hint_of(doc, "/title"), "LCF:struct:SaveTitle");
    assert_eq!(at(doc, "/title/hero_name").value, Value::Str("Alex".into()));
    assert!(!read_only(doc, "/title/hero_name"));
    assert_eq!(
        at(doc, "/title/timestamp").value,
        Value::Float {
            v: 45123.75,
            width: saveprism_core::FloatWidth::F64
        }
    );

    // System: sizes are read-only counts, vectors hold editable elements.
    assert_eq!(int_at(doc, "/system/switches_size"), 5);
    assert_eq!(hint_of(doc, "/system/switches_size"), "LCF:count");
    assert!(read_only(doc, "/system/switches_size"));
    assert_eq!(hint_of(doc, "/system/switches"), "LCF:bool[]");
    assert_eq!(at(doc, "/system/switches/0").value, Value::Bool(true));
    assert!(!read_only(doc, "/system/switches/0"));
    assert_eq!(hint_of(doc, "/system/variables"), "LCF:int32[]");
    assert_eq!(int_at(doc, "/system/variables/1"), -7);
    assert_eq!(int_at(doc, "/system/variables/3"), 20_000_000);
    assert_eq!(int_at(doc, "/system/variables/5"), 2_147_483_647);
    assert_eq!(hint_of(doc, "/system/title_music"), "LCF:struct:Music");
    assert_eq!(
        at(doc, "/system/title_music/name").value,
        Value::Str("Opening1".into())
    );
    assert_eq!(hint_of(doc, "/system/transition_out"), "LCF:int8");
    assert_eq!(at(doc, "/system/save_allowed").value, Value::Bool(true));
    assert_eq!(at(doc, "/system/menu_allowed").value, Value::Bool(false));

    // A struct type with no field table keeps its chunks as read-only bytes.
    assert_eq!(hint_of(doc, "/screen"), "LCF:struct:SaveScreen");
    assert_eq!(at(doc, "/screen/0x01").value, Value::Bytes(vec![100]));
    assert_eq!(hint_of(doc, "/screen/0x01"), "LCF:bytes");
    assert!(read_only(doc, "/screen/0x01"));

    // Arrays: elements are structs whose first entry is the element id.
    assert_eq!(hint_of(doc, "/actors"), "LCF:array:SaveActor");
    let Value::Object(entries) = &at(doc, "/actors/1").value else {
        panic!("not an object")
    };
    assert_eq!(entries[0].0, ID_KEY);
    assert_eq!(int_at(doc, "/actors/1/(id)"), 2);
    assert!(read_only(doc, "/actors/1/(id)"));
    assert_eq!(at(doc, "/actors/1/name").value, Value::Str("Mira".into()));
    // "\x01" (use the database name) and a Windows-1252 name are read-only bytes.
    assert_eq!(at(doc, "/actors/0/name").value, Value::Bytes(vec![1]));
    assert_eq!(hint_of(doc, "/actors/0/name"), "LCF:string");
    assert!(read_only(doc, "/actors/0/name"));
    assert_eq!(
        at(doc, "/actors/2/name").value,
        Value::Bytes(b"Jos\xe9".to_vec())
    );
    assert_eq!(int_at(doc, "/actors/0/level"), 12);
    assert_eq!(int_at(doc, "/actors/0/equipped/1"), 5);
    assert_eq!(hint_of(doc, "/actors/0/equipped"), "LCF:int16[]");
    // An empty vector.
    assert_eq!(at(doc, "/actors/0/status").value, Value::Array(Vec::new()));

    // Inventory.
    assert_eq!(int_at(doc, "/inventory/gold"), 5000);
    assert_eq!(int_at(doc, "/inventory/party/1"), 2);
    assert_eq!(int_at(doc, "/inventory/item_ids/3"), 7);
    assert_eq!(int_at(doc, "/inventory/item_counts/3"), 99);
    assert_eq!(hint_of(doc, "/inventory/item_counts"), "LCF:uint8[]");
    assert_eq!(int_at(doc, "/targets/0/map_y"), 4);
    assert_eq!(at(doc, "/common_events").value, Value::Array(Vec::new()));
    assert!(decoded.params.is_empty(), "{:?}", decoded.params);
}

#[test]
fn the_2k3_save_and_the_databases_read() {
    let decoded = exact(&fixture(SAVE_2K3));
    let doc = &decoded.doc;
    assert_eq!(int_at(doc, "/system/atb_mode"), 1);
    assert_eq!(int_at(doc, "/system/variables/1"), 16_777_217);
    assert_eq!(int_at(doc, "/actors/0/battle_commands/3"), 4);
    assert_eq!(hint_of(doc, "/actors/0/battle_commands"), "LCF:int32[]");
    assert!(
        doc.resolve(&Path::parse("/inventory/gold").unwrap())
            .is_none()
    );

    let decoded = exact(&fixture(DB_2K));
    let doc = &decoded.doc;
    assert_eq!(hint_of(doc, ""), "LCF:struct:Database");
    assert_eq!(at(doc, "/actors/2/name").value, Value::Str("Jose".into()));
    assert_eq!(int_at(doc, "/items/1/type"), 1);
    assert_eq!(
        at(doc, "/items/4/name").value,
        Value::Bytes(b"\xc9lixir".to_vec())
    );
    assert_eq!(
        at(doc, "/variables/2/name").value,
        Value::Str("Zero Counter".into())
    );
    assert_eq!(hint_of(doc, "/skills/0/0x01"), "LCF:bytes");

    // The 2003 database ends with an end mark, kept as a parameter.
    let decoded = exact(&fixture(DB_2K3));
    assert_eq!(
        decoded.params.opt_bytes(P_END_MARK).unwrap(),
        Some(&[0_u8][..])
    );
    assert!(
        decoded
            .doc
            .resolve(&Path::parse("/classes").unwrap())
            .is_some()
    );
}

#[test]
fn edits_of_every_scalar_kind_survive_a_write() {
    let mut decoded = decode(&fixture(SAVE_2K));
    set(&mut decoded, "/inventory/gold", OwnedValue::Int(999_999));
    set(
        &mut decoded,
        "/inventory/item_counts/0",
        OwnedValue::Int(42),
    );
    set(
        &mut decoded,
        "/system/variables/0",
        OwnedValue::Int(-5_000_000),
    );
    set(&mut decoded, "/system/switches/1", OwnedValue::Bool(true));
    set(&mut decoded, "/actors/1/level", OwnedValue::Int(99));
    set(&mut decoded, "/system/transition_out", OwnedValue::Int(-3));
    set(&mut decoded, "/system/menu_allowed", OwnedValue::Bool(true));
    set(
        &mut decoded,
        "/title/hero_name",
        OwnedValue::Str("Alexander".into()),
    );
    set(
        &mut decoded,
        "/title/timestamp",
        OwnedValue::Float(46000.25),
    );
    set(&mut decoded, "/actors/0/skills/1", OwnedValue::Int(-2));
    let again = reopen(&decoded);
    let doc = &again.doc;
    assert_eq!(int_at(doc, "/inventory/gold"), 999_999);
    assert_eq!(int_at(doc, "/inventory/item_counts/0"), 42);
    assert_eq!(int_at(doc, "/system/variables/0"), -5_000_000);
    assert_eq!(at(doc, "/system/switches/1").value, Value::Bool(true));
    assert_eq!(int_at(doc, "/actors/1/level"), 99);
    assert_eq!(int_at(doc, "/system/transition_out"), -3);
    assert_eq!(at(doc, "/system/menu_allowed").value, Value::Bool(true));
    assert_eq!(
        at(doc, "/title/hero_name").value,
        Value::Str("Alexander".into())
    );
    assert_eq!(int_at(doc, "/actors/0/skills/1"), -2);
    // Everything else is unchanged.
    assert_eq!(int_at(doc, "/system/variables/5"), 2_147_483_647);
    assert_eq!(int_at(doc, "/actors/2/level"), 3);
    assert_eq!(int_at(doc, "/targets/0/map_x"), 3);
    assert!(decoded.doc.tree_eq(doc));
}

#[test]
fn a_growing_number_updates_every_enclosing_length() {
    let original = fixture(SAVE_2K);
    let mut decoded = decode(&original);
    // 3 takes one byte, 100000 three; 3400 takes two bytes, 1000000 three.
    set(&mut decoded, "/system/save_count", OwnedValue::Int(100_000));
    set(&mut decoded, "/actors/2/exp", OwnedValue::Int(1_000_000));
    set(&mut decoded, "/inventory/gold", OwnedValue::Int(-1));
    let written = encode(&decoded);
    assert_eq!(written.len(), original.len() + 2 + 1 + 3);
    let again = decode(&written);
    assert_eq!(int_at(&again.doc, "/system/save_count"), 100_000);
    assert_eq!(int_at(&again.doc, "/actors/2/exp"), 1_000_000);
    assert_eq!(int_at(&again.doc, "/inventory/gold"), -1);
    assert!(decoded.doc.tree_eq(&again.doc));
    assert_eq!(encode(&again), written);
}

#[test]
fn non_canonical_numbers_are_kept_until_their_value_changes() {
    // Header length, chunk id, chunk length, array count, element id, end mark and an integer
    // written with leading zero groups; then a top-level end mark and trailing bytes.
    let mut bytes = vec![0x80, 0x0B];
    bytes.extend_from_slice(SAVE_HEADER);
    let mut inventory = vec![0x80, 0x15, 0x03, 0x80, 0x80, 0x07];
    inventory.extend([0x80, 0x00]);
    bytes.extend([0x80, 0x6D, 0x80, inventory.len() as u8]);
    bytes.extend(&inventory);
    let mut actors = vec![0x80, 0x01, 0x80, 0x04];
    actors.extend(body(&[chunk(0x1F, &[0x80, 0x05])]));
    bytes.extend(chunk(0x6C, &actors));
    bytes.extend(chunk(0x99, &[1, 2, 3]));
    bytes.extend([0x00, 0xAB, 0xCD]);

    let mut decoded = exact(&bytes);
    let doc = &decoded.doc;
    assert_eq!(int_at(doc, "/inventory/gold"), 7);
    assert_eq!(
        at(doc, "/inventory/gold").lexeme.as_deref(),
        Some("raw:808007")
    );
    assert_eq!(int_at(doc, "/actors/0/(id)"), 4);
    assert_eq!(int_at(doc, "/actors/0/level"), 5);
    assert_eq!(at(doc, "/0x99").value, Value::Bytes(vec![1, 2, 3]));
    assert!(matches!(
        decoded.params.get(P_RAW_BER),
        Some(ParamValue::Table(_))
    ));
    assert_eq!(
        decoded.params.opt_bytes(P_TRAILING).unwrap(),
        Some(&[0xAB, 0xCD][..])
    );

    // A new gold value drops its raw bytes; the inventory length changes, so its recorded raw
    // length no longer applies, while the other recorded numbers stay as they were.
    set(&mut decoded, "/inventory/gold", OwnedValue::Int(300));
    let written = encode(&decoded);
    let mut expected = vec![0x80, 0x0B];
    expected.extend_from_slice(SAVE_HEADER);
    let mut inventory = vec![0x80, 0x15, 0x02, 0x82, 0x2C];
    inventory.extend([0x80, 0x00]);
    expected.extend([0x80, 0x6D, inventory.len() as u8]);
    expected.extend(&inventory);
    expected.extend(chunk(0x6C, &actors));
    expected.extend(chunk(0x99, &[1, 2, 3]));
    expected.extend([0x00, 0xAB, 0xCD]);
    assert_eq!(written, expected);
}

#[test]
fn data_that_does_not_fit_its_type_is_kept_as_bytes() {
    let bytes = save(&[
        // A struct missing its end mark, a vector of odd length, an empty and a two-byte boolean,
        // a too-long number, an empty chunk and an array whose count is too large.
        chunk(0x64, &chunk(0x0B, b"Al")),
        chunk(
            0x6D,
            &body(&[
                chunk(0x02, &[1, 0, 2]),
                chunk(0x18, &[]),
                chunk(0x19, &[0x81, 0x00]),
                chunk(0x15, &[0x05, 0x06]),
                chunk(0x20, &[]),
            ]),
        ),
        chunk(0x6C, &[0x02, 0x01, 0x00]),
        chunk(0x6E, &[]),
    ]);
    let decoded = exact(&bytes);
    let doc = &decoded.doc;
    for path in [
        "/title",
        "/inventory/party",
        "/inventory/timer1_active",
        "/inventory/gold",
        "/inventory/battles",
        "/actors",
        "/targets",
    ] {
        assert_eq!(hint_of(doc, path), "LCF:bytes", "{path}");
        assert!(read_only(doc, path), "{path}");
    }
    // A boolean that is a number other than 0 or 1 stays editable as a number.
    assert_eq!(int_at(doc, "/inventory/timer1_visible"), 128);
    assert_eq!(hint_of(doc, "/inventory/timer1_visible"), "LCF:bool");
}

#[test]
fn malformed_and_truncated_input_is_an_error() {
    let original = fixture(SAVE_2K);
    // Every cut inside the header or a top-level chunk fails; cuts between chunks are files.
    for cut in 0..original.len() {
        if let Ok(decoded) = decode_result(&original[..cut]) {
            assert_eq!(encode(&decoded), &original[..cut]);
        }
    }
    let error = decode_result(&original[..20]).unwrap_err();
    assert_eq!(error.code(), LCF_PARSE);
    assert_eq!(decode_result(&[]).unwrap_err().code(), LCF_PARSE);
    assert_eq!(decode_result(&[0x0B, b'L']).unwrap_err().code(), LCF_PARSE);

    // A chunk id of more than five bytes, and one too large for 32 bits.
    let mut long = save(&[]);
    long.extend([0x80, 0x80, 0x80, 0x80, 0x80, 0x01, 0x00]);
    assert_eq!(decode_result(&long).unwrap_err().code(), LCF_PARSE);
    let mut large = save(&[]);
    large.extend([0x90, 0x80, 0x80, 0x80, 0x00, 0x00]);
    assert_eq!(decode_result(&large).unwrap_err().code(), LCF_PARSE);
    // A chunk longer than the file.
    let mut over = save(&[]);
    over.extend([0x64, 0x05, 0x00]);
    assert_eq!(decode_result(&over).unwrap_err().code(), LCF_PARSE);

    // Other LCF files are not read.
    let mut map = vec![0x0A];
    map.extend_from_slice(b"LcfMapUnit");
    assert_eq!(decode_result(&map).unwrap_err().code(), LCF_UNSUPPORTED);
}

#[test]
fn values_that_cannot_be_written_are_refused() {
    let mut decoded = decode(&fixture(SAVE_2K));
    set(
        &mut decoded,
        "/title/hero_name",
        OwnedValue::Str("Jos\u{e9}".into()),
    );
    let error = LcfCodec.encode(&decoded.doc, &decoded.params).unwrap_err();
    assert_eq!(error.code(), LCF_WRITE);

    // Widths keep values in range: the core refuses them before the writer sees them.
    let mut decoded = decode(&fixture(SAVE_2K));
    for (path, value) in [
        ("/inventory/gold", i128::from(i32::MAX) + 1),
        ("/inventory/item_counts/0", 256),
        ("/actors/0/skills/0", 40_000),
        ("/system/transition_out", 128),
    ] {
        let result = apply_edits(
            &mut decoded.doc,
            &[Edit::Set {
                path: Path::parse(path).unwrap(),
                value: OwnedValue::Int(value),
            }],
            &Limits::default(),
        );
        assert!(result.is_err(), "{path}");
    }
    for path in ["/actors/0/(id)", "/system/switches_size", "/actors/0/name"] {
        let result = apply_edits(
            &mut decoded.doc,
            &[Edit::Set {
                path: Path::parse(path).unwrap(),
                value: OwnedValue::Int(1),
            }],
            &Limits::default(),
        );
        assert!(result.is_err(), "{path}");
    }
}

/// Decoding never panics; whatever decodes is written back exactly.
fn writes_back(bytes: &[u8]) {
    if let Ok(decoded) = decode_result(bytes) {
        assert_eq!(encode(&decoded), bytes);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// Random chunks after the header never panic.
    #[test]
    fn random_input_never_panics(body in proptest::collection::vec(any::<u8>(), 0..256)) {
        writes_back(&save(&[body]));
    }

    /// Small numbers reach deeper into the struct reader.
    #[test]
    fn lcf_like_input_never_panics(
        body in proptest::collection::vec(
            prop_oneof![0_u8..8, 0x64_u8..0x73, 0x80_u8..0x84, any::<u8>()],
            0..160,
        )
    ) {
        writes_back(&save(&[body]));
    }

    /// Mutated saves never panic.
    #[test]
    fn mutated_saves_never_panic(position in 0_usize..618, byte in any::<u8>()) {
        let mut bytes = fixture(SAVE_2K);
        if let Some(slot) = bytes.get_mut(position) {
            *slot = byte;
        }
        writes_back(&bytes);
    }
}
