//! Tests of the Witcher 3 codec.

use std::path::PathBuf;

use proptest::prelude::*;
use saveprism_core::{
    Codec, DecodeCtx, Decoded, Edit, Limits, Node, NodeFlags, NodeId, OwnedValue, ParamValue, Path,
    SaveDoc, Value, apply_edits, codes,
};

use super::*;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn fixture() -> Vec<u8> {
    std::fs::read(fixtures().join("synthetic/witcher3/ManualSave_invented.sav")).unwrap()
}

fn decode_with(bytes: &[u8], limits: &Limits) -> saveprism_core::Result<Decoded> {
    let mut ctx = DecodeCtx::new(limits);
    Witcher3Codec.decode(bytes, &mut ctx)
}

fn decode(bytes: &[u8]) -> Decoded {
    decode_with(bytes, &Limits::default()).unwrap()
}

fn encode(decoded: &Decoded) -> Vec<u8> {
    Witcher3Codec.encode(&decoded.doc, &decoded.params).unwrap()
}

fn pointer(text: &str) -> Path {
    Path::parse(&format!("/{}", text.replace('.', "/"))).unwrap()
}

fn id_at(doc: &SaveDoc, path: &str) -> NodeId {
    doc.resolve(&pointer(path))
        .unwrap_or_else(|| panic!("no node at {path}"))
}

fn at<'a>(doc: &'a SaveDoc, path: &str) -> &'a Node {
    doc.get(id_at(doc, path)).unwrap()
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

fn set(decoded: &mut Decoded, path: &str, value: OwnedValue) -> saveprism_core::Result<()> {
    apply_edits(
        &mut decoded.doc,
        &[Edit::Set {
            path: pointer(path),
            value,
        }],
        &Limits::default(),
    )
}

fn hinted(decoded: &Decoded, path: &str) -> String {
    at(&decoded.doc, path)
        .type_hint
        .as_ref()
        .map(|hint| hint.as_str().to_string())
        .unwrap_or_default()
}

/// Builds an invented save whose variable table count includes the two footer words as its last
/// entry, the layout real next-gen saves use (the table then ends 6 bytes before the file end).
fn footer_overlapping_variable_table() -> Vec<u8> {
    let header_size = 28_usize;
    let mut stream = Vec::new();
    stream.extend_from_slice(STREAM_MAGIC);
    stream.extend_from_slice(&66_u32.to_le_bytes());
    stream.extend_from_slice(&29_u32.to_le_bytes());
    stream.extend_from_slice(&0_u16.to_le_bytes());
    stream.extend_from_slice(b"NM");
    stream.extend_from_slice(b"MANU");
    stream.extend_from_slice(&1_u32.to_le_bytes());
    stream.extend_from_slice(&0_u32.to_le_bytes());
    stream.push(1);
    stream.push(b'P');
    stream.extend_from_slice(&0_u32.to_le_bytes());
    stream.extend_from_slice(b"ENOD");
    let names_absolute = (header_size + 14) as u32;
    stream.extend_from_slice(&names_absolute.to_le_bytes());
    stream.extend_from_slice(&0_u32.to_le_bytes());
    stream.extend_from_slice(b"SC");
    let variables_offset = stream.len();
    stream.extend_from_slice(&2_u32.to_le_bytes());
    stream.extend_from_slice(&0_u32.to_le_bytes());
    stream.extend_from_slice(&4_u32.to_le_bytes());
    stream.extend_from_slice(&names_absolute.to_le_bytes());
    stream.extend_from_slice(&0_u32.to_le_bytes());
    stream.extend_from_slice(&(header_size as u32 + variables_offset as u32).to_le_bytes());
    stream.extend_from_slice(FOOTER_MAGIC);
    assert_eq!(stream.len(), variables_offset + 4 + 2 * 8 + 6);

    let mut bytes = Vec::new();
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&1_u32.to_le_bytes());
    bytes.extend_from_slice(&(header_size as u32).to_le_bytes());
    bytes.extend_from_slice(&(stream.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&(stream.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    assert_eq!(bytes.len(), header_size);
    bytes.extend_from_slice(&stream);
    bytes
}

#[test]
fn a_variable_table_that_includes_the_footer_words_still_decodes() {
    let bytes = footer_overlapping_variable_table();
    assert_eq!(
        Witcher3Codec.sniff(&bytes, &saveprism_core::SniffCtx::default()),
        saveprism_core::Confidence::HIGH
    );
    let decoded = decode(&bytes);
    assert_eq!(encode(&decoded), bytes);
}

#[test]
fn unchanged_fixture_writes_the_same_bytes() {
    let bytes = fixture();
    let decoded = decode(&bytes);
    assert_eq!(encode(&decoded), bytes);
}

#[test]
fn tree_exposes_the_confirmed_scalars() {
    let bytes = fixture();
    let decoded = decode(&bytes);
    let doc = &decoded.doc;

    assert_eq!(int_at(doc, "save_version"), 66);
    assert_eq!(int_at(doc, "game_version"), 29);
    assert_eq!(int_at(doc, "Variables.W3Player.Orens"), 1250);
    assert_eq!(int_at(doc, "Variables.W3Player.level"), 12);
    assert_eq!(text_at(doc, "Variables.W3Player.name"), "Geralt");
    assert_eq!(text_at(doc, "Variables.W3Player.latinText"), "caf\u{e9}");
    assert_eq!(
        text_at(doc, "Variables.W3Player.CInventoryComponent.SItem.itemName"),
        "Orens"
    );
    assert_eq!(
        int_at(doc, "Variables.W3Player.CInventoryComponent.SItem.quantity"),
        7
    );
    assert_eq!(
        int_at(doc, "Variables.W3Player.CInventoryComponent.quantities.1"),
        2
    );
    assert_eq!(
        int_at(doc, "Variables.CR4GwintManager.SBCollectionCardSize.Size"),
        2
    );
    assert_eq!(
        int_at(
            doc,
            "Variables.CR4GwintManager.SBCollectionCardSize.SBCollectionCard.numCopies"
        ),
        4
    );
    assert_eq!(
        int_at(doc, "Variables.CR4GwintManager.GwintDecks.DeckCount"),
        2
    );
    assert_eq!(
        int_at(
            doc,
            "Variables.CR4GwintManager.GwintDecks.GwintDeck.LeaderIndex"
        ),
        4001
    );
    assert_eq!(
        int_at(doc, "Variables.InitialBuildIDBlock.seed"),
        18446744073709551615
    );
    assert_eq!(
        text_at(doc, "Variables.saveInfo.playthroughId"),
        "invented-playthrough"
    );
    assert!(matches!(
        at(doc, "Variables.facts.record").value,
        Value::Bytes(_)
    ));
    assert!(
        at(doc, "Variables.facts.record")
            .flags
            .contains(NodeFlags::READ_ONLY)
    );

    assert_eq!(hinted(&decoded, "Variables.W3Player.Orens"), "W3:Int32");
    assert_eq!(
        hinted(&decoded, "Variables.W3Player.name"),
        "W3:String/utf8"
    );
    assert_eq!(hinted(&decoded, "Variables.facts.record"), "W3:bytes");
    assert_eq!(
        hinted(&decoded, "Variables.CR4GwintManager"),
        "W3:gwent-manager"
    );
    assert_eq!(
        hinted(&decoded, "Variables.CR4GwintManager.SBCollectionCardSize"),
        "W3:gwent-collection"
    );
    assert_eq!(
        hinted(
            &decoded,
            "Variables.CR4GwintManager.SBCollectionCardSize.SBCollectionCard"
        ),
        "W3:gwent-card:3"
    );
    assert_eq!(
        hinted(&decoded, "Variables.CR4GwintManager.GwintDecks.GwintDeck"),
        "W3:gwent-deck"
    );
    let decks = children_named(doc, "Variables.CR4GwintManager.GwintDecks", "GwintDeck");
    let deck_cards = children_named_id(doc, decks[1], "GwintDeckCard");
    assert_eq!(hint_of(doc, deck_cards[0]), "W3:gwent-card:3");
    assert_eq!(
        hinted(&decoded, "Variables.CR4GwintManager.SBSelectedDeckIndex"),
        "W3:gwent-selected-deck"
    );
}

fn children_named(doc: &SaveDoc, path: &str, name: &str) -> Vec<NodeId> {
    children_named_id(doc, id_at(doc, path), name)
}

/// The children named `name`, including the numbered repeats (`name#2`, `name#3`, ...).
fn children_named_id(doc: &SaveDoc, id: NodeId, name: &str) -> Vec<NodeId> {
    let repeat = format!("{name}#");
    match doc.get(id).map(|node| &node.value) {
        Some(Value::Object(entries)) => entries
            .iter()
            .filter(|(key, _)| key == name || key.starts_with(&repeat))
            .map(|(_, id)| *id)
            .collect(),
        other => panic!("{id:?} is {other:?}"),
    }
}

#[test]
fn repeated_names_get_unique_numbered_keys() {
    let names = ["A", "A", "A#2", "B", "A"];
    let entries = names
        .iter()
        .enumerate()
        .map(|(index, name)| (name.to_string(), NodeId::from_index(index).unwrap()));
    let keys: Vec<String> = read::numbered(entries)
        .into_iter()
        .map(|(key, _)| key)
        .collect();
    assert_eq!(keys, ["A", "A#2", "A#2#2", "B", "A#3"]);

    let many = (0..20_000).map(|index| {
        (
            "SBCollectionCard".to_string(),
            NodeId::from_index(index).unwrap(),
        )
    });
    let keys = read::numbered(many);
    assert_eq!(
        keys.last().map(|(key, _)| key.as_str()),
        Some("SBCollectionCard#20000")
    );
}

#[test]
fn repeated_gwent_records_are_numbered_so_each_copy_count_has_a_path() {
    let bytes = fixture();
    let mut decoded = decode(&bytes);
    let collection = "Variables.CR4GwintManager.SBCollectionCardSize";
    assert_eq!(
        hinted(&decoded, &format!("{collection}.SBCollectionCard")),
        "W3:gwent-card:3"
    );
    assert_eq!(
        hinted(&decoded, &format!("{collection}.SBCollectionCard#2")),
        "W3:gwent-card:4"
    );
    assert_eq!(
        hinted(&decoded, "Variables.CR4GwintManager.GwintDecks.GwintDeck#2"),
        "W3:gwent-deck"
    );

    let second = format!("{collection}.SBCollectionCard#2.numCopies");
    set(&mut decoded, &second, OwnedValue::Int(3)).unwrap();
    let again = decode(&encode(&decoded));
    assert_eq!(int_at(&again.doc, &second), 3);
    let first = format!("{collection}.SBCollectionCard.numCopies");
    assert_eq!(
        int_at(&again.doc, &first),
        int_at(&decode(&bytes).doc, &first)
    );
}

fn hint_of(doc: &SaveDoc, id: NodeId) -> String {
    doc.get(id)
        .and_then(|node| node.type_hint.as_ref())
        .map(|hint| hint.as_str().to_string())
        .unwrap_or_default()
}

fn is_read_only(doc: &SaveDoc, id: NodeId) -> bool {
    doc.get(id)
        .is_some_and(|node| node.flags.contains(NodeFlags::READ_ONLY))
}

#[test]
fn gwent_cards_carry_their_index_and_keep_their_identity_read_only() {
    let decoded = decode(&fixture());
    let doc = &decoded.doc;
    let collection = "Variables.CR4GwintManager.SBCollectionCardSize";
    let cards = children_named(doc, collection, "SBCollectionCard");
    assert_eq!(cards.len(), 2);
    assert_eq!(hint_of(doc, cards[0]), "W3:gwent-card:3");
    assert_eq!(hint_of(doc, cards[1]), "W3:gwent-card:4");
    assert!(is_read_only(doc, cards[0]));

    let fields = match &doc.get(cards[0]).unwrap().value {
        Value::Object(entries) => entries.clone(),
        other => panic!("card is {other:?}"),
    };
    let card_index = fields
        .iter()
        .find(|(key, _)| key == "cardIndex")
        .map(|(_, id)| *id)
        .unwrap();
    let copies = fields
        .iter()
        .find(|(key, _)| key == "numCopies")
        .map(|(_, id)| *id)
        .unwrap();
    assert!(is_read_only(doc, card_index));
    assert!(!is_read_only(doc, copies));

    let decks = children_named(doc, "Variables.CR4GwintManager.GwintDecks", "GwintDeck");
    assert_eq!(decks.len(), 2);
    assert_eq!(hint_of(doc, decks[0]), "W3:gwent-deck");
    assert!(is_read_only(doc, decks[0]));
    let deck_fields = match &doc.get(decks[1]).unwrap().value {
        Value::Object(entries) => entries.clone(),
        other => panic!("deck is {other:?}"),
    };
    for (name, id) in &deck_fields {
        if ["DeckUnlocked", "LeaderIndex", "CardCount"].contains(&name.as_str()) {
            assert!(is_read_only(doc, *id), "{name} should be read-only");
        }
    }
    let deck_cards = children_named_id(doc, decks[1], "GwintDeckCard");
    assert_eq!(deck_cards.len(), 2);
    let deck_card_fields = children_named_id(doc, deck_cards[0], "cardIndex");
    assert_eq!(deck_card_fields.len(), 1);
    assert!(is_read_only(doc, deck_card_fields[0]));
}

#[test]
fn editing_a_collection_copy_count_round_trips() {
    let bytes = fixture();
    let mut decoded = decode(&bytes);
    let collection = "Variables.CR4GwintManager.SBCollectionCardSize";
    let cards = children_named(&decoded.doc, collection, "SBCollectionCard");
    let copies = match &decoded.doc.get(cards[1]).unwrap().value {
        Value::Object(entries) => entries
            .iter()
            .find(|(key, _)| key == "numCopies")
            .map(|(_, id)| *id)
            .unwrap(),
        other => panic!("card is {other:?}"),
    };
    decoded
        .doc
        .set_value(
            copies,
            Value::Int {
                v: 3,
                width: saveprism_core::IntWidth::I32,
            },
        )
        .unwrap();

    let written = encode(&decoded);
    assert_ne!(written, bytes);
    let again = decode(&written);
    let cards = children_named(&again.doc, collection, "SBCollectionCard");
    let copies = match &again.doc.get(cards[1]).unwrap().value {
        Value::Object(entries) => entries
            .iter()
            .find(|(key, _)| key == "numCopies")
            .map(|(_, id)| *id)
            .unwrap(),
        other => panic!("card is {other:?}"),
    };
    assert_eq!(
        again.doc.get(copies).unwrap().value,
        Value::Int {
            v: 3,
            width: saveprism_core::IntWidth::I32,
        }
    );
}

#[test]
fn gwent_identity_and_structure_edits_are_refused() {
    let mut decoded = decode(&fixture());
    for path in [
        "Variables.CR4GwintManager.SBCollectionCardSize.Size",
        "Variables.CR4GwintManager.SBCollectionCardSize.SBCollectionCard.cardIndex",
        "Variables.CR4GwintManager.GwintDecks.DeckCount",
        "Variables.CR4GwintManager.GwintDecks.GwintDeck.LeaderIndex",
        "Variables.CR4GwintManager.GwintDecks.GwintDeck.CardCount",
        "Variables.CR4GwintManager.GwintDecks.GwintDeck.DeckUnlocked",
    ] {
        let error = set(&mut decoded, path, OwnedValue::Int(1)).unwrap_err();
        assert_eq!(error.code(), codes::CORE_READ_ONLY, "{path}");
    }
}

#[test]
fn editing_orens_and_an_item_quantity_survives_a_round_trip() {
    let bytes = fixture();
    let mut decoded = decode(&bytes);
    set(
        &mut decoded,
        "Variables.W3Player.Orens",
        OwnedValue::Int(999_999),
    )
    .unwrap();
    set(
        &mut decoded,
        "Variables.W3Player.CInventoryComponent.SItem.quantity",
        OwnedValue::Int(42),
    )
    .unwrap();

    let written = encode(&decoded);
    assert_ne!(written, bytes);
    let again = decode(&written);
    assert_eq!(int_at(&again.doc, "Variables.W3Player.Orens"), 999_999);
    assert_eq!(
        int_at(
            &again.doc,
            "Variables.W3Player.CInventoryComponent.SItem.quantity"
        ),
        42
    );
}

#[test]
fn editing_other_scalar_kinds_survives_a_round_trip() {
    let bytes = fixture();
    let mut decoded = decode(&bytes);
    set(
        &mut decoded,
        "Variables.W3Player.name",
        OwnedValue::Str("Triss!".to_string()),
    )
    .unwrap();
    set(
        &mut decoded,
        "Variables.W3Player.weight",
        OwnedValue::Float(80.25),
    )
    .unwrap();
    set(
        &mut decoded,
        "Variables.W3Player.CInventoryComponent.isNewGamePlus",
        OwnedValue::Bool(false),
    )
    .unwrap();
    set(
        &mut decoded,
        "Variables.W3Player.CInventoryComponent.quantities.2",
        OwnedValue::Int(-9),
    )
    .unwrap();
    set(
        &mut decoded,
        "Variables.InitialBuildIDBlock.seed",
        OwnedValue::Int(7),
    )
    .unwrap();

    let again = decode(&encode(&decoded));
    assert_eq!(text_at(&again.doc, "Variables.W3Player.name"), "Triss!");
    assert!(matches!(
        at(&again.doc, "Variables.W3Player.weight").value,
        Value::Float { v, .. } if (v - 80.25).abs() < f64::EPSILON
    ));
    assert_eq!(
        at(
            &again.doc,
            "Variables.W3Player.CInventoryComponent.isNewGamePlus"
        )
        .value,
        Value::Bool(false)
    );
    assert_eq!(
        int_at(
            &again.doc,
            "Variables.W3Player.CInventoryComponent.quantities.2"
        ),
        -9
    );
    assert_eq!(int_at(&again.doc, "Variables.InitialBuildIDBlock.seed"), 7);
}

#[test]
fn changing_a_string_length_is_refused() {
    let bytes = fixture();
    let mut decoded = decode(&bytes);
    set(
        &mut decoded,
        "Variables.W3Player.name",
        OwnedValue::Str("Geralt of Rivia".to_string()),
    )
    .unwrap();
    let error = Witcher3Codec
        .encode(&decoded.doc, &decoded.params)
        .unwrap_err();
    assert_eq!(error.code(), W3_WRITE);
}

#[test]
fn bad_inputs_are_errors_not_panics() {
    assert_eq!(
        decode_with(b"", &Limits::default()).unwrap_err().code(),
        W3_NOT_SAVE
    );
    assert_eq!(
        decode_with(b"not a witcher save", &Limits::default())
            .unwrap_err()
            .code(),
        W3_NOT_SAVE
    );

    let bytes = fixture();
    assert_eq!(
        decode_with(&bytes[..bytes.len() / 2], &Limits::default())
            .unwrap_err()
            .code(),
        W3_PARSE
    );

    let mut corrupt = bytes.clone();
    corrupt[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(decode_with(&corrupt, &Limits::default()).is_err());
}

#[test]
fn a_declared_decompression_bomb_is_refused_before_allocating() {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&1_u32.to_le_bytes());
    bytes.extend_from_slice(&28_u32.to_le_bytes());
    let decoded_size = 32_u32 * 1024 * 1024;
    bytes.extend_from_slice(&1_u32.to_le_bytes());
    bytes.extend_from_slice(&decoded_size.to_le_bytes());
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.push(0);
    let error = decode_with(&bytes, &Limits::default()).unwrap_err();
    assert_eq!(error.code(), codes::CORE_LIMIT_DECODED);
}

#[test]
fn truncated_chunk_table_is_an_error() {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&2_u32.to_le_bytes());
    bytes.extend_from_slice(&40_u32.to_le_bytes());
    bytes.extend_from_slice(&[0_u8; 10]);
    let error = decode_with(&bytes, &Limits::default()).unwrap_err();
    assert_eq!(error.code(), W3_PARSE);
}

#[test]
fn a_bad_lz4_block_is_an_error() {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&1_u32.to_le_bytes());
    bytes.extend_from_slice(&28_u32.to_le_bytes());
    bytes.extend_from_slice(&5_u32.to_le_bytes());
    bytes.extend_from_slice(&100_u32.to_le_bytes());
    bytes.extend_from_slice(&33_u32.to_le_bytes());
    bytes.extend_from_slice(&[0xff; 5]);
    let error = decode_with(&bytes, &Limits::default()).unwrap_err();
    assert_eq!(error.code(), W3_PARSE);
}

#[test]
fn sniff_uses_the_magic_and_does_not_claim_other_save_files() {
    let bytes = fixture();
    let ctx = saveprism_core::SniffCtx::default();
    assert_eq!(
        Witcher3Codec.sniff(&bytes, &ctx),
        saveprism_core::Confidence::HIGH
    );
    assert_eq!(
        Witcher3Codec.sniff(b"not a save", &ctx),
        saveprism_core::Confidence::NONE
    );
}

#[test]
fn encode_needs_the_original_parameters() {
    let decoded = decode(&fixture());
    let empty = saveprism_core::Params::new();
    let error = Witcher3Codec.encode(&decoded.doc, &empty).unwrap_err();
    assert_eq!(error.code(), W3_WRITE);
}

#[test]
fn params_keep_the_container_facts() {
    let bytes = fixture();
    let decoded = decode(&bytes);
    assert_eq!(
        decoded.params.get(P_HEADER_SIZE),
        Some(&ParamValue::Int(40))
    );
    assert!(matches!(
        decoded.params.get(P_CHUNK_SIZES),
        Some(ParamValue::List(items)) if items.len() == 2
    ));
    assert_eq!(
        decoded.params.get(P_ORIGINAL),
        Some(&ParamValue::Bytes(bytes))
    );
}

#[test]
#[ignore = "reads the three local real saves when they are present"]
fn real_saves_round_trip_and_expose_a_gwent_collection() {
    fn collect(dir: &std::path::Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect(&path, out);
            } else if path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("sav"))
            {
                out.push(path);
            }
        }
    }

    let root = fixtures().join("external/witcher3");
    let mut saves = Vec::new();
    collect(&root, &mut saves);
    if saves.is_empty() {
        eprintln!("no real Witcher 3 saves under {}", root.display());
        return;
    }

    for path in saves {
        let bytes = std::fs::read(&path).unwrap();
        let decoded = decode(&bytes);
        assert_eq!(
            encode(&decoded),
            bytes,
            "{} does not round-trip",
            path.display()
        );

        let doc = &decoded.doc;
        let mut collections = 0usize;
        let mut cards = 0usize;
        for index in 0..doc.len() {
            let Some(node) = doc.get_index(index) else {
                continue;
            };
            let hint = node.type_hint.as_ref().map(|hint| hint.as_str());
            if hint == Some("W3:gwent-collection") {
                collections += 1;
                if let Value::Object(entries) = &node.value {
                    cards += entries
                        .iter()
                        .filter(|(_, child)| {
                            doc.get(*child)
                                .and_then(|node| node.type_hint.as_ref())
                                .is_some_and(|hint| hint.as_str().starts_with("W3:gwent-card"))
                        })
                        .count();
                }
            }
        }
        assert!(
            collections >= 1 && cards > 0,
            "{} has no Gwent collection",
            path.display()
        );
    }
}

proptest! {
    #[test]
    fn any_i32_orens_value_round_trips(value in prop::num::i32::ANY) {
        let bytes = fixture();
        let mut decoded = decode(&bytes);
        set(&mut decoded, "Variables.W3Player.Orens", OwnedValue::Int(i128::from(value))).unwrap();
        let again = decode(&encode(&decoded));
        prop_assert_eq!(int_at(&again.doc, "Variables.W3Player.Orens"), i128::from(value));
    }

    #[test]
    fn any_u64_seed_value_round_trips(value in prop::num::u64::ANY) {
        let bytes = fixture();
        let mut decoded = decode(&bytes);
        set(&mut decoded, "Variables.InitialBuildIDBlock.seed", OwnedValue::Int(i128::from(value))).unwrap();
        let again = decode(&encode(&decoded));
        prop_assert_eq!(int_at(&again.doc, "Variables.InitialBuildIDBlock.seed"), i128::from(value));
    }
}
