use saveprism_core::{
    Codec, DecodeCtx, Edit, Limits, NodeFlags, Path, SniffCtx, Value, apply_edits,
};

use super::*;

fn checksum(bytes: &[u8]) -> u16 {
    super::checksum(bytes).expect("fixture words align")
}

fn block(index: u32, key: u32, emerald: bool) -> Vec<u8> {
    let mut out = vec![0u8; BLOCK];
    for id in 0..14usize {
        let at = id * SECTION;
        let size = match id {
            0 => 3884,
            4 => 3848,
            13 => 2000,
            _ => 3968,
        };
        out[at + 0xff4..at + 0xff6].copy_from_slice(&(id as u16).to_le_bytes());
        out[at + 0xff8..at + 0xffc].copy_from_slice(&MAGIC.to_le_bytes());
        out[at + 0xffc..at + 0x1000].copy_from_slice(&index.to_le_bytes());
        if id == 0 {
            out[at + 8] = 1;
            out[at + 0x0a..at + 0x0c].copy_from_slice(&321u16.to_le_bytes());
            out[at + 0x0c..at + 0x0e].copy_from_slice(&654u16.to_le_bytes());
            if emerald {
                out[at + 0xac..at + 0xb0].copy_from_slice(&key.to_le_bytes());
                out[at + 0x1f4..at + 0x1f8].copy_from_slice(&key.to_le_bytes());
            }
        }
        if id == 1 {
            let team = if emerald { 0x234 } else { 0x34 };
            let money = if emerald { 0x490 } else { 0x290 };
            let pockets = if emerald {
                [
                    (0x560, 30),
                    (0x5d8, 30),
                    (0x650, 16),
                    (0x690, 64),
                    (0x790, 46),
                ]
            } else {
                [
                    (0x560, 20),
                    (0x5b0, 20),
                    (0x600, 16),
                    (0x640, 64),
                    (0x740, 46),
                ]
            };
            for (offset, slots) in pockets {
                for slot in 0..slots {
                    out[at + offset + slot * 4 + 2..at + offset + slot * 4 + 4]
                        .copy_from_slice(&(key as u16).to_le_bytes());
                }
            }
            out[at + team..at + team + 4].copy_from_slice(&1u32.to_le_bytes());
            out[at + money..at + money + 4].copy_from_slice(&(50_000u32 ^ key).to_le_bytes());
            out[at + money + 4..at + money + 6]
                .copy_from_slice(&(400u16 ^ key as u16).to_le_bytes());
            out[at + 0x560..at + 0x562].copy_from_slice(&13u16.to_le_bytes());
            out[at + 0x562..at + 0x564].copy_from_slice(&(4u16 ^ key as u16).to_le_bytes());
            let start = at + team + 4;
            let record = &mut out[start..start + 100];
            let pv = 5u32;
            let ot = 77u32;
            record[..4].copy_from_slice(&pv.to_le_bytes());
            record[4..8].copy_from_slice(&ot.to_le_bytes());
            record[0x54] = 12;
            record[0x56..0x58].copy_from_slice(&25u16.to_le_bytes());
            let mut subs = [[0u8; 12]; 4];
            subs[0][..2].copy_from_slice(&25u16.to_le_bytes());
            subs[0][4..8].copy_from_slice(&777u32.to_le_bytes());
            subs[0][9] = 80;
            subs[3][4..8].copy_from_slice(&((10u32) | (12u32 << 5) | (7u32 << 10)).to_le_bytes());
            let order = ORDERS[(pv % 24) as usize];
            let mut plain = [0u8; 48];
            for (part, tag) in order.iter().enumerate() {
                let sub = match tag {
                    b'G' => 0,
                    b'A' => 1,
                    b'E' => 2,
                    _ => 3,
                };
                plain[part * 12..part * 12 + 12].copy_from_slice(&subs[sub]);
            }
            let pkey = pv ^ ot;
            for (i, chunk) in plain.as_chunks::<4>().0.iter().enumerate() {
                let word = u32::from_le_bytes(*chunk) ^ pkey;
                record[0x20 + i * 4..0x24 + i * 4].copy_from_slice(&word.to_le_bytes());
            }
            record[0x1c..0x1e].copy_from_slice(&checksum(&plain).to_le_bytes());
        }
        let sum = checksum(&out[at..at + size]);
        out[at + 0xff6..at + 0xff8].copy_from_slice(&sum.to_le_bytes());
    }
    out
}

fn substruct_index(tag: u8) -> usize {
    match tag {
        b'G' => 0,
        b'A' => 1,
        b'E' => 2,
        _ => 3,
    }
}

fn set_experience(save: &mut [u8], start: usize, experience: u32) {
    let pv = u32::from_le_bytes(save[start..start + 4].try_into().expect("personality"));
    let ot = u32::from_le_bytes(save[start + 4..start + 8].try_into().expect("ot id"));
    let key = pv ^ ot;
    let order = ORDERS[(pv % 24) as usize];
    let mut plain = [0u8; 48];
    for i in 0..12 {
        let word = u32::from_le_bytes(
            save[start + 0x20 + i * 4..start + 0x24 + i * 4]
                .try_into()
                .expect("encrypted word"),
        ) ^ key;
        plain[i * 4..i * 4 + 4].copy_from_slice(&word.to_le_bytes());
    }
    let mut subs = [[0u8; 12]; 4];
    for (part, tag) in order.iter().enumerate() {
        subs[substruct_index(*tag)].copy_from_slice(&plain[part * 12..part * 12 + 12]);
    }
    subs[0][4..8].copy_from_slice(&experience.to_le_bytes());
    for (part, tag) in order.iter().enumerate() {
        plain[part * 12..part * 12 + 12].copy_from_slice(&subs[substruct_index(*tag)]);
    }
    let sum = super::pokemon_checksum(&plain).expect("Pokémon checksum length");
    save[start + 0x1c..start + 0x1e].copy_from_slice(&sum.to_le_bytes());
    for i in 0..12 {
        let word =
            u32::from_le_bytes(plain[i * 4..i * 4 + 4].try_into().expect("plaintext word")) ^ key;
        save[start + 0x20 + i * 4..start + 0x24 + i * 4].copy_from_slice(&word.to_le_bytes());
    }
}

fn reseal(save: &mut [u8], section: usize) {
    let size = match u16::from_le_bytes(
        save[section + 0xff4..section + 0xff6]
            .try_into()
            .expect("section id"),
    ) {
        0 => 3884,
        4 => 3848,
        13 => 2000,
        _ => 3968,
    };
    let sum = checksum(&save[section..section + size]);
    save[section + 0xff6..section + 0xff8].copy_from_slice(&sum.to_le_bytes());
}

fn synthetic() -> Vec<u8> {
    let mut bytes = block(2, 0xA123_4321, true);
    bytes.extend(block(3, 0xB123_4321, true));
    bytes.resize(0x20000, 0x5a);
    bytes
}

fn decode(bytes: &[u8]) -> Decoded {
    let limits = Limits::default();
    let mut ctx = DecodeCtx::new(&limits);
    Gen3Codec
        .decode(bytes, &mut ctx)
        .expect("synthetic save decodes")
}

fn field(doc: &SaveDoc, path: &str) -> u32 {
    let id = doc
        .resolve(&Path::parse(path).expect("valid test path"))
        .expect("path exists");
    match &doc.get(id).expect("node exists").value {
        Value::Int { v, .. } => *v as u32,
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn unchanged_bytes_round_trip_and_newer_valid_block_is_selected() {
    let bytes = synthetic();
    let decoded = decode(&bytes);
    assert_eq!(field(&decoded.doc, "/money/balance"), 50_000);
    assert_eq!(field(&decoded.doc, "/party/0/experience"), 777);
    assert_eq!(field(&decoded.doc, "/inventory/bag/items/0/quantity"), 4);
    assert_eq!(
        Gen3Codec
            .encode(&decoded.doc, &decoded.params)
            .expect("encode"),
        bytes
    );
}

#[test]
fn edited_money_coins_experience_iv_and_stat_keep_checksums_valid() {
    let bytes = synthetic();
    let mut decoded = decode(&bytes);
    let edits = [
        ("/money/balance", 123_456u32),
        ("/money/coins", 987u32),
        ("/inventory/bag/items/0/quantity", 7u32),
        ("/party/0/experience", 88_888u32),
        ("/party/0/hp_iv", 31u32),
        ("/party/0/attack", 345u32),
    ];
    let changes = edits
        .iter()
        .map(|(path, value)| Edit::Set {
            path: Path::parse(path).expect("path"),
            value: saveprism_core::OwnedValue::Int(i128::from(*value)),
        })
        .collect::<Vec<_>>();
    apply_edits(&mut decoded.doc, &changes, &Limits::default()).expect("apply edits");
    let written = Gen3Codec
        .encode(&decoded.doc, &decoded.params)
        .expect("encode edits");
    let checked = decode(&written);
    for (path, value) in edits {
        assert_eq!(field(&checked.doc, path), value);
    }
    assert_eq!(
        &written[..BLOCK],
        &bytes[..BLOCK],
        "inactive block remains byte-identical"
    );
    assert_eq!(
        &written[BLOCK * 2..],
        &bytes[BLOCK * 2..],
        "trailing data remains byte-identical"
    );
    assert!(checked_block(&written, 0).is_some());
    let checksum_valid = checked
        .doc
        .resolve(&Path::parse("/party/0/checksum_valid").expect("path"))
        .expect("checksum node");
    assert_eq!(
        checked.doc.get(checksum_valid).expect("checksum").value,
        Value::Bool(true)
    );
}

#[test]
fn masked_balance_is_written_with_key_and_bad_input_is_rejected() {
    let bytes = synthetic();
    assert_eq!(
        Gen3Codec.sniff(&bytes, &SniffCtx::default()),
        Confidence::HIGH
    );
    let mut decoded = decode(&bytes);
    apply_edits(
        &mut decoded.doc,
        &[Edit::Set {
            path: Path::parse("/money/balance").expect("path"),
            value: saveprism_core::OwnedValue::Int(1_000_000),
        }],
        &Limits::default(),
    )
    .expect("edit applies to tree");
    assert!(Gen3Codec.encode(&decoded.doc, &decoded.params).is_err());
    let mut damaged = bytes.clone();
    damaged[0xff8] = 0;
    damaged[BLOCK + 0xff8] = 0;
    let limits = Limits::default();
    let mut ctx = DecodeCtx::new(&limits);
    assert!(Gen3Codec.decode(&damaged, &mut ctx).is_err());
    assert!(Gen3Codec.sniff(&[0u8; 123], &SniffCtx::default()) == Confidence::NONE);
    let mut short_dump = block(9, 0x7654_3210, true);
    short_dump.resize(0x10000, 0xA5);
    let short_decoded = decode(&short_dump);
    assert_eq!(
        Gen3Codec
            .encode(&short_decoded.doc, &short_decoded.params)
            .expect("64 KiB round trip"),
        short_dump
    );
    let mut wrong_size_ctx = DecodeCtx::new(&limits);
    assert!(
        Gen3Codec
            .decode(&[0u8; SECTION], &mut wrong_size_ctx)
            .is_err()
    );
}

#[test]
fn generated_fixtures_cover_variants_party_orders_and_footer_exactly() {
    let root =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/synthetic/gen3");
    for (name, variant) in [
        ("emerald.sav", "Emerald"),
        ("firered.sav", "FireRed/LeafGreen"),
        ("ruby.sav", "Ruby/Sapphire-like (variant uncertain)"),
        ("emerald-footer.srm", "Emerald"),
    ] {
        let bytes = std::fs::read(root.join(name)).expect("fixture is present");
        let decoded = decode(&bytes);
        let game = decoded
            .doc
            .resolve(&Path::parse("/game").expect("path"))
            .expect("game node");
        assert_eq!(
            decoded.doc.get(game).expect("game").value,
            Value::Str(variant.into())
        );
        let party = decoded
            .doc
            .resolve(&Path::parse("/party").expect("path"))
            .expect("party node");
        let Value::Array(members) = &decoded.doc.get(party).expect("party").value else {
            panic!("party must be an array")
        };
        assert_eq!(members.len(), 6);
        if name == "ruby.sav" {
            for path in ["/money/balance", "/inventory/bag/items/0/quantity"] {
                let id = decoded
                    .doc
                    .resolve(&Path::parse(path).expect("path"))
                    .expect("value path");
                assert!(
                    decoded
                        .doc
                        .get(id)
                        .expect("value")
                        .flags
                        .contains(NodeFlags::READ_ONLY)
                );
            }
        }
        assert_eq!(
            Gen3Codec
                .encode(&decoded.doc, &decoded.params)
                .expect("encode"),
            bytes
        );
    }
}

#[test]
fn high_word_pokemon_checksum_is_preserved_unless_the_record_changes() {
    let mut bytes = block(2, 0x1122_3344, true);
    let start = SECTION + 0x234 + 4;
    set_experience(&mut bytes, start, 100_000);
    reseal(&mut bytes, SECTION);
    bytes.resize(0x10000, 0x5a);

    let mut decoded = decode(&bytes);
    assert_eq!(field(&decoded.doc, "/party/0/experience"), 100_000);
    let valid = decoded
        .doc
        .resolve(&Path::parse("/party/0/checksum_valid").expect("path"))
        .expect("checksum node");
    assert_eq!(
        decoded.doc.get(valid).expect("checksum").value,
        Value::Bool(true)
    );
    assert_eq!(
        Gen3Codec
            .encode(&decoded.doc, &decoded.params)
            .expect("unchanged encode"),
        bytes,
        "a large experience value makes the game's 16-bit checksum differ from a folded one"
    );

    apply_edits(
        &mut decoded.doc,
        &[Edit::Set {
            path: Path::parse("/party/0/experience").expect("path"),
            value: saveprism_core::OwnedValue::Int(131_072),
        }],
        &Limits::default(),
    )
    .expect("edit applies");
    let written = Gen3Codec
        .encode(&decoded.doc, &decoded.params)
        .expect("edited encode");
    let checked = decode(&written);
    assert_eq!(field(&checked.doc, "/party/0/experience"), 131_072);
    let valid = checked
        .doc
        .resolve(&Path::parse("/party/0/checksum_valid").expect("path"))
        .expect("checksum node");
    assert_eq!(
        checked.doc.get(valid).expect("checksum").value,
        Value::Bool(true)
    );
    assert_eq!(
        Gen3Codec
            .encode(&checked.doc, &checked.params)
            .expect("second encode"),
        written
    );
}

#[test]
fn frlg_bag_stack_of_999_is_valid_and_round_trips() {
    let key = 0xA1B2_C3D4u32;
    let mut bytes = block(2, key, false);
    let s1 = SECTION;
    bytes[s1..s1 + SECTION].fill(0);
    bytes[s1 + 0xff4..s1 + 0xff6].copy_from_slice(&1u16.to_le_bytes());
    bytes[s1 + 0xff8..s1 + 0xffc].copy_from_slice(&MAGIC.to_le_bytes());
    bytes[s1 + 0xffc..s1 + 0x1000].copy_from_slice(&2u32.to_le_bytes());
    bytes[s1 + 0x290..s1 + 0x294].copy_from_slice(&key.to_le_bytes());
    bytes[s1 + 0x294..s1 + 0x296].copy_from_slice(&(key as u16).to_le_bytes());
    bytes[s1 + 0x310..s1 + 0x312].copy_from_slice(&4u16.to_le_bytes());
    bytes[s1 + 0x312..s1 + 0x314].copy_from_slice(&(999u16 ^ key as u16).to_le_bytes());
    let s0 = 0;
    bytes[s0 + 0xac..s0 + 0xb0].copy_from_slice(&1u32.to_le_bytes());
    bytes[s0 + 0xaf8..s0 + 0xafc].copy_from_slice(&key.to_le_bytes());
    bytes[s0 + 0xf20..s0 + 0xf24].copy_from_slice(&key.to_le_bytes());
    reseal(&mut bytes, s0);
    reseal(&mut bytes, s1);
    bytes.resize(0x10000, 0x5a);

    let mut decoded = decode(&bytes);
    assert_eq!(field(&decoded.doc, "/inventory/bag/items/0/quantity"), 999);
    assert_eq!(
        Gen3Codec
            .encode(&decoded.doc, &decoded.params)
            .expect("unchanged encode"),
        bytes
    );

    apply_edits(
        &mut decoded.doc,
        &[Edit::Set {
            path: Path::parse("/inventory/bag/items/0/quantity").expect("path"),
            value: saveprism_core::OwnedValue::Int(998),
        }],
        &Limits::default(),
    )
    .expect("edit applies");
    let written = Gen3Codec
        .encode(&decoded.doc, &decoded.params)
        .expect("edited encode");
    let checked = decode(&written);
    assert_eq!(field(&checked.doc, "/inventory/bag/items/0/quantity"), 998);
    assert_eq!(
        Gen3Codec
            .encode(&checked.doc, &checked.params)
            .expect("second encode"),
        written
    );
}
