use std::path::PathBuf;

use proptest::prelude::*;

use saveprism_core::{
    Codec, DecodeCtx, Decoded, Edit, IntWidth, Limits, OwnedValue, Path, SniffCtx, Value,
    apply_edits, codes,
};

use super::*;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn fixture() -> Vec<u8> {
    std::fs::read(fixtures().join("synthetic/slime-rancher-2/invented_1.sav")).unwrap()
}

fn decode_with(bytes: &[u8], limits: &Limits) -> Result<Decoded> {
    let mut ctx = DecodeCtx::new(limits);
    SlimeRancher2Codec.decode(bytes, &mut ctx)
}

fn decode(bytes: &[u8]) -> Decoded {
    decode_with(bytes, &Limits::default()).unwrap()
}

fn encode(decoded: &Decoded) -> Vec<u8> {
    SlimeRancher2Codec
        .encode(&decoded.doc, &decoded.params)
        .unwrap()
}

fn value(decoded: &Decoded, path: &str) -> Option<Value> {
    decoded
        .doc
        .resolve(&Path::parse(path).unwrap())
        .and_then(|id| decoded.doc.get(id))
        .map(|node| node.value.clone())
}

fn int(v: i128) -> Option<Value> {
    Some(Value::Int {
        v,
        width: IntWidth::I32,
    })
}

fn set(decoded: &mut Decoded, path: &str, v: i128) -> saveprism_core::Result<()> {
    apply_edits(
        &mut decoded.doc,
        &[Edit::Set {
            path: Path::parse(path).unwrap(),
            value: OwnedValue::Int(v),
        }],
        &Limits::default(),
    )
}

/// Offsets where `a` and `b` (same length) differ.
fn differences(a: &[u8], b: &[u8]) -> Vec<usize> {
    assert_eq!(a.len(), b.len());
    a.iter()
        .zip(b)
        .enumerate()
        .filter(|(_, (x, y))| x != y)
        .map(|(at, _)| at)
        .collect()
}

#[test]
fn sniff_needs_the_root_record_and_name_tables() {
    let bytes = fixture();
    assert!(
        SlimeRancher2Codec
            .sniff(&bytes, &SniffCtx::default())
            .value()
            > 0.9
    );
    assert_eq!(
        SlimeRancher2Codec
            .sniff(b"{\"a\":1}", &SniffCtx::default())
            .value(),
        0.0
    );
    // The settings file has a tagged root record, not an empty tag.
    assert_eq!(
        SlimeRancher2Codec
            .sniff(
                b"\x0aSRSETTINGS\x01\x00\x00\x00\x00\x30",
                &SniffCtx::default()
            )
            .value(),
        0.0
    );
}

#[test]
fn the_invented_save_round_trips_and_shows_what_is_known() {
    let bytes = fixture();
    let decoded = decode(&bytes);
    assert_eq!(encode(&decoded), bytes);
    assert_eq!(value(&decoded, "/format_version"), int(11));
    assert_eq!(
        value(&decoded, "/save_name"),
        Some(Value::Str("20260101120000_1".to_owned()))
    );
    assert_eq!(
        value(&decoded, "/game_version"),
        Some(Value::Str("0.0.0 [invented]".to_owned()))
    );
    assert_eq!(value(&decoded, "/money"), int(1234));
    assert_eq!(value(&decoded, "/vacpack/Slot 1: Invented Plort"), int(12));
    assert_eq!(value(&decoded, "/vacpack/Slot 2: Invented Slime"), int(3));
    assert_eq!(value(&decoded, "/vacpack/Slot 3: empty"), int(0));
    assert_eq!(value(&decoded, "/vacpack/Slot 4: empty"), int(0));
    let root = decoded.doc.resolve(&Path::parse("/vacpack").unwrap());
    assert_eq!(decoded.doc.children(root.unwrap()).len(), 4);
    // The plot's group is storage; the run behind a wrong count is not shown at all.
    assert_eq!(
        value(&decoded, "/storage/Storage 1/Slot 1: Invented Veggie"),
        int(40)
    );
    assert_eq!(value(&decoded, "/storage/Storage 2"), None);
}

#[test]
fn item_names_are_made_readable() {
    assert_eq!(read::item_label("SlimeDefinition.Tabby"), "Tabby Slime");
    assert_eq!(
        read::item_label("IdentifiableType.PhosphorPlort"),
        "Phosphor Plort"
    );
    assert_eq!(read::item_label("ToyDefinition.YarnBall"), "Yarn Ball");
    assert_eq!(read::item_label("NoPrefix"), "No Prefix");
}

#[test]
fn money_edit_writes_the_player_and_the_summary_copy() {
    let bytes = fixture();
    let mut decoded = decode(&bytes);
    set(&mut decoded, "/money", 99_999).unwrap();
    let written = encode(&decoded);
    assert_eq!(differences(&bytes, &written).len(), 6, "two 4-byte amounts");
    let again = decode(&written);
    assert_eq!(value(&again, "/money"), int(99_999));
    // Both copies changed: both currency records now hold the new amount.
    let needle = 99_999_i32.to_le_bytes();
    let copies = written.windows(4).filter(|w| *w == needle).count();
    assert_eq!(copies, 2);
    assert_eq!(encode(&again), written, "the edited save must be stable");
}

#[test]
fn the_summary_copy_is_left_alone_when_it_differed() {
    let mut bytes = fixture();
    let summary = bytes
        .windows(4)
        .position(|w| w == 1234_i32.to_le_bytes())
        .unwrap();
    bytes[summary..summary + 4].copy_from_slice(&1000_i32.to_le_bytes());
    let mut decoded = decode(&bytes);
    assert_eq!(value(&decoded, "/money"), int(1234));
    set(&mut decoded, "/money", 5).unwrap();
    let written = encode(&decoded);
    assert_eq!(&written[summary..summary + 4], &1000_i32.to_le_bytes());
}

#[test]
fn slot_counts_of_filled_slots_are_editable() {
    let bytes = fixture();
    let mut decoded = decode(&bytes);
    set(&mut decoded, "/vacpack/Slot 2: Invented Slime", 30).unwrap();
    set(
        &mut decoded,
        "/storage/Storage 1/Slot 1: Invented Veggie",
        0,
    )
    .unwrap();
    let written = encode(&decoded);
    let again = decode(&written);
    assert_eq!(value(&again, "/vacpack/Slot 2: Invented Slime"), int(30));
    assert_eq!(
        value(&again, "/storage/Storage 1/Slot 1: Invented Veggie"),
        int(0)
    );
    assert_eq!(value(&again, "/money"), int(1234));
}

#[test]
fn unknown_and_empty_values_are_read_only() {
    let mut decoded = decode(&fixture());
    for path in [
        "/format_version",
        "/vacpack/Slot 3: empty",
        "/game_version",
        "/save_name",
    ] {
        let error = apply_edits(
            &mut decoded.doc,
            &[Edit::Set {
                path: Path::parse(path).unwrap(),
                value: OwnedValue::Int(1),
            }],
            &Limits::default(),
        )
        .unwrap_err();
        assert_eq!(error.code(), codes::CORE_READ_ONLY, "{path}");
    }
    let error = apply_edits(
        &mut decoded.doc,
        &[Edit::Remove {
            path: Path::parse("/vacpack/Slot 1: Invented Plort").unwrap(),
        }],
        &Limits::default(),
    )
    .unwrap_err();
    assert_eq!(error.code(), codes::CORE_READ_ONLY);
}

#[test]
fn negative_amounts_are_refused() {
    let mut decoded = decode(&fixture());
    set(&mut decoded, "/money", -1).unwrap();
    let error = SlimeRancher2Codec
        .encode(&decoded.doc, &decoded.params)
        .unwrap_err();
    assert_eq!(error.code(), SR2_WRITE);
}

#[test]
fn a_second_player_currency_hides_the_money() {
    let mut bytes = fixture();
    let end = bytes.len() - 2;
    let extra = b"\x07SRCRNCY\x01\x00\x00\x00\x00\x30\x01\x00\x00\x00\x05\x00\x00\x00\xfa\x00\x00\x00\x00\x40";
    bytes.splice(end..end, extra.iter().copied());
    let decoded = decode(&bytes);
    assert_eq!(
        value(&decoded, "/money"),
        None,
        "which copy is the player's is unclear"
    );
    assert_eq!(encode(&decoded), bytes);
}

#[test]
fn truncated_or_damaged_saves_fail_without_panicking() {
    let bytes = fixture();
    // A prefix that happens to end on an end mark is still a readable (shorter) file.
    for len in 0..bytes.len() {
        match decode_with(&bytes[..len], &Limits::default()) {
            Ok(decoded) => assert_eq!(encode(&decoded), &bytes[..len]),
            Err(error) => assert_eq!(error.code(), SR2_PARSE, "prefix of {len} bytes"),
        }
    }
    // Flipping any single byte must never panic; it may still decode.
    for at in 0..bytes.len() {
        let mut damaged = bytes.clone();
        damaged[at] ^= 0xff;
        if let Ok(decoded) = decode_with(&damaged, &Limits::default()) {
            assert_eq!(encode(&decoded), damaged);
        }
    }
}

#[test]
fn the_node_limit_is_enforced() {
    let limits = Limits {
        max_nodes: 5,
        ..Limits::default()
    };
    let error = decode_with(&fixture(), &limits).unwrap_err();
    assert_eq!(error.code(), codes::CORE_LIMIT_NODES);
}

#[test]
fn encode_needs_the_original_bytes() {
    let decoded = decode(&fixture());
    let error = SlimeRancher2Codec
        .encode(&decoded.doc, &Params::new())
        .unwrap_err();
    assert_eq!(error.code(), SR2_WRITE);
}

#[test]
#[ignore = "reads the local, git-ignored fixtures/external/slime-rancher-2 saves"]
fn real_saves_round_trip_and_edit_the_money() {
    let root = fixtures().join("external/slime-rancher-2");
    let Ok(entries) = std::fs::read_dir(&root) else {
        eprintln!("no real Slime Rancher 2 saves under {}", root.display());
        return;
    };
    let mut saves: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "sav"))
        .collect();
    saves.sort();
    assert!(!saves.is_empty());
    for path in saves {
        let bytes = std::fs::read(&path).unwrap();
        assert!(
            SlimeRancher2Codec
                .sniff(&bytes, &SniffCtx::default())
                .value()
                > 0.9
        );
        let mut decoded = decode(&bytes);
        assert_eq!(encode(&decoded), bytes, "{}", path.display());
        let Some(Value::Int { v: money, .. }) = value(&decoded, "/money") else {
            panic!("{} has no money", path.display());
        };
        let vacpack = decoded
            .doc
            .resolve(&Path::parse("/vacpack").unwrap())
            .and_then(|id| decoded.doc.get(id))
            .map(|node| node.value.clone());
        let Some(Value::Object(slots)) = vacpack else {
            panic!("{} has no vacpack", path.display());
        };
        set(&mut decoded, "/money", money + 1000).unwrap();
        let written = encode(&decoded);
        let changed = differences(&bytes, &written);
        assert!(
            (1..=8).contains(&changed.len()),
            "{}: {} bytes changed",
            path.display(),
            changed.len()
        );
        assert_eq!(value(&decode(&written), "/money"), int(money + 1000));
        let rows: Vec<String> = slots
            .iter()
            .map(
                |(key, id)| match decoded.doc.get(*id).map(|node| &node.value) {
                    Some(Value::Int { v, .. }) => format!("{key} = {v}"),
                    _ => key.clone(),
                },
            )
            .collect();
        eprintln!("{}: money {money}, vacpack {rows:?}", path.display());
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Random bodies behind the header, and random splices into the invented save, never panic;
    /// whatever decodes writes back unchanged.
    #[test]
    fn random_input_never_panics(
        body in proptest::collection::vec(any::<u8>(), 0..512),
        at in any::<usize>(),
    ) {
        let mut input = b"\x00\x0b\x00\x00\x00\x00\x30\x0bINDXTBLLKUP".to_vec();
        input.extend_from_slice(&body);
        input.extend_from_slice(&END);
        let mut spliced = fixture();
        let at = at % spliced.len();
        spliced.splice(at..at, body.iter().copied());
        for bytes in [input, spliced] {
            if let Ok(decoded) = decode_with(&bytes, &Limits::default()) {
                prop_assert_eq!(encode(&decoded), bytes);
            }
        }
    }
}
