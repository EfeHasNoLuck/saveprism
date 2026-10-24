//! `format.pokemon-gen3`: Pokémon Ruby/Sapphire, Emerald and FireRed/LeafGreen battery saves.
//!
//! Written from the public Generation III save and Pokémon structure descriptions. Input is
//! parsed as bytes only; no save content is executed.
//!
//! The root contains the current block's `game`, `trainer`, `money`, `inventory` and `party`.
//! Unrecognized trainer names and item IDs are bytes / read-only. Money, coins, item quantities,
//! play time, party stats, EXP, friendship, move PPs, EVs and IVs are editable. The inactive save
//! block and remaining bytes are exposed as read-only `Bytes` nodes. Item pocket layouts and the
//! key offsets are selected only for known game variants; uncertain money and inventory stay
//! read-only.
//!
//! Writing patches the original byte stream, updates Pokémon data encryption and checksums, then
//! refreshes checksums for the current block. Everything outside edited values and those checksum
//! words remains byte-identical.

use std::sync::Arc;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    ByteSpan, Codec, Confidence, DecodeCtx, Decoded, Error, ErrorCode, IntWidth, Node, NodeFlags,
    Params, Registry, Result, SaveDoc, SniffCtx, TypeHint, Value,
};

const SECTION: usize = 0x1000;
const BLOCK: usize = 14 * SECTION;
const MAGIC: u32 = 0x0801_2025;
const ORIGINAL: &str = "original";
pub const PARSE: ErrorCode = ErrorCode::new("SP-GEN3-001");
pub const WRITE: ErrorCode = ErrorCode::new("SP-GEN3-002");
pub const HINT: &str = "GEN3:";

pub fn register(reg: &mut Registry) {
    reg.add_codec(Arc::new(Gen3Codec));
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Gen3Codec;

#[derive(Clone)]
struct BlockInfo {
    base: usize,
    sections: [usize; 14],
    index: u32,
}

fn checksum(data: &[u8]) -> Option<u16> {
    if !data.len().is_multiple_of(4) {
        return None;
    }
    let mut sum = 0u32;
    let (words, remainder) = data.as_chunks::<4>();
    if !remainder.is_empty() {
        return None;
    }
    for word in words {
        let word = u32::from_le_bytes(*word);
        sum = sum.wrapping_add(word);
    }
    Some((sum as u16).wrapping_add((sum >> 16) as u16))
}

fn checked_block(input: &[u8], base: usize) -> Option<BlockInfo> {
    let mut sections = [usize::MAX; 14];
    let mut index = None;
    for sector in 0usize..14 {
        let start = base.checked_add(sector.checked_mul(SECTION)?)?;
        let bytes = input.get(start..start.checked_add(SECTION)?)?;
        if u32::from_le_bytes(bytes.get(0xff8..0xffc)?.try_into().ok()?) != MAGIC {
            return None;
        }
        let id = usize::from(u16::from_le_bytes(
            bytes.get(0xff4..0xff6)?.try_into().ok()?,
        ));
        if id >= 14 || sections.get(id).copied()? != usize::MAX {
            return None;
        }
        let size = match id {
            0 => 3884,
            4 => 3848,
            13 => 2000,
            _ => 3968,
        };
        let stored = u16::from_le_bytes(bytes.get(0xff6..0xff8)?.try_into().ok()?);
        if checksum(bytes.get(..size)?)? != stored {
            return None;
        }
        let current = u32::from_le_bytes(bytes.get(0xffc..0x1000)?.try_into().ok()?);
        if index.is_some_and(|previous| previous != current) {
            return None;
        }
        index = Some(current);
        *sections.get_mut(id)? = start;
    }
    if sections.contains(&usize::MAX) {
        return None;
    }
    Some(BlockInfo {
        base,
        sections,
        index: index?,
    })
}

fn u16_at(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        bytes.get(offset..offset.checked_add(2)?)?.try_into().ok()?,
    ))
}
fn u32_at(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset.checked_add(4)?)?.try_into().ok()?,
    ))
}
fn push(
    doc: &mut SaveDoc,
    value: Value,
    hint: &str,
    offset: Option<usize>,
    width: usize,
    read_only: bool,
) -> Result<saveprism_core::NodeId> {
    let mut node = Node::new(value);
    node.type_hint = Some(TypeHint::new(format!("{HINT}{hint}")));
    if let Some(start) = offset {
        node.span = Some(ByteSpan {
            start: start as u64,
            end: start.saturating_add(width) as u64,
        });
    }
    if read_only {
        node.flags.insert(NodeFlags::READ_ONLY);
    }
    doc.push(node)
}
fn int(
    doc: &mut SaveDoc,
    value: u32,
    width: usize,
    name: &str,
    offset: usize,
    read_only: bool,
) -> Result<saveprism_core::NodeId> {
    let int_width = match width {
        1 => IntWidth::U8,
        2 => IntWidth::U16,
        _ => IntWidth::U32,
    };
    push(
        doc,
        Value::Int {
            v: i128::from(value),
            width: int_width,
        },
        name,
        Some(offset),
        width,
        read_only,
    )
}
fn obj(
    doc: &mut SaveDoc,
    entries: Vec<(&str, saveprism_core::NodeId)>,
) -> Result<saveprism_core::NodeId> {
    doc.push(Node::new(Value::Object(
        entries
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
    )))
}

fn section_offset(block: &BlockInfo, id: usize, off: usize) -> Result<usize> {
    block
        .sections
        .get(id)
        .copied()
        .and_then(|base| base.checked_add(off))
        .ok_or_else(|| Error::new(PARSE, "section offset is invalid"))
}

#[allow(clippy::too_many_arguments)]
fn item_pocket(
    doc: &mut SaveDoc,
    input: &[u8],
    section: usize,
    offset: usize,
    slots: usize,
    quantity_hint: &str,
    key: u32,
    keyed: bool,
    read_only: bool,
) -> Result<saveprism_core::NodeId> {
    let mut items = Vec::with_capacity(slots);
    for slot in 0..slots {
        let at = section
            .checked_add(offset + slot * 4)
            .ok_or_else(|| Error::new(PARSE, "item offset overflow"))?;
        let item_id = u16_at(input, at).ok_or_else(|| Error::new(PARSE, "item id is truncated"))?;
        let id = int(doc, u32::from(item_id), 2, "item-id", at, true)?;
        let stored =
            u16_at(input, at + 2).ok_or_else(|| Error::new(PARSE, "item quantity is truncated"))?;
        let decoded = if quantity_hint == "item-quantity-bag" && keyed {
            stored ^ key as u16
        } else {
            stored
        };
        let quantity = int(
            doc,
            u32::from(decoded),
            2,
            quantity_hint,
            at + 2,
            read_only || item_id == 0,
        )?;
        items.push(obj(doc, vec![("item_id", id), ("quantity", quantity)])?);
    }
    doc.push(Node::new(Value::Array(items)))
}

impl Codec for Gen3Codec {
    fn id(&self) -> &'static str {
        "format.pokemon-gen3"
    }
    fn sniff(&self, input: &[u8], _ctx: &SniffCtx<'_>) -> Confidence {
        if !(0x10000..=0x20020).contains(&input.len()) || (input.len() < 0x10000) {
            return Confidence::NONE;
        }
        let found = (0..=input.len().saturating_sub(SECTION))
            .step_by(SECTION)
            .any(|at| {
                input
                    .get(at + 0xff8..at + 0xffc)
                    .and_then(|v| <[u8; 4]>::try_from(v).ok())
                    .is_some_and(|v| u32::from_le_bytes(v) == MAGIC)
                    && input
                        .get(at + 0xff4..at + 0xff6)
                        .and_then(|v| <[u8; 2]>::try_from(v).ok())
                        .is_some_and(|v| u16::from_le_bytes(v) < 14)
            });
        if found {
            Confidence::HIGH
        } else {
            Confidence::NONE
        }
    }
    #[allow(clippy::indexing_slicing)] // Indices below come from checked record lengths and fixed tables.
    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<Decoded> {
        ctx.limits().check_input(len_u64(input.len()))?;
        if input.len() < 0x10000 || input.len() > 0x20020 {
            return Err(Error::new(
                PARSE,
                "expected a 64 KiB or 128 KiB Gen 3 save with an optional small footer",
            ));
        }
        let first = checked_block(input, 0);
        let second = if input.len() >= BLOCK * 2 {
            checked_block(input, BLOCK)
        } else {
            None
        };
        let block = match (first, second) {
            (Some(a), Some(b)) => {
                if b.index > a.index {
                    b
                } else {
                    a
                }
            }
            (Some(a), None) => a,
            (None, Some(b)) => b,
            (None, None) => {
                return Err(Error::new(
                    PARSE,
                    "no complete Gen 3 save block with valid signatures and checksums",
                ));
            }
        };
        let mut doc = ctx.new_doc();
        let s0 = *block
            .sections
            .first()
            .ok_or_else(|| Error::new(PARSE, "section 0 is missing"))?;
        let s1 = *block
            .sections
            .get(1)
            .ok_or_else(|| Error::new(PARSE, "section 1 is missing"))?;
        let game_code = u32_at(input, s0 + 0xac).unwrap_or(0);
        let emerald_key = u32_at(input, s0 + 0xac).unwrap_or(0);
        let emerald_copy = u32_at(input, s0 + 0x1f4).unwrap_or(0);
        let frlg_key = u32_at(input, s0 + 0xaf8).unwrap_or(0);
        let frlg_copy = u32_at(input, s0 + 0xf20).unwrap_or(0);
        let (variant, key, keyed, known) = if game_code == 1 {
            (
                "FireRed/LeafGreen",
                frlg_key,
                frlg_key != 0,
                frlg_key == frlg_copy,
            )
        } else if emerald_key != 0 && emerald_key == emerald_copy {
            ("Emerald", emerald_key, true, true)
        } else if emerald_key == 0 && emerald_copy == 0 && frlg_key == 0 && frlg_copy == 0 {
            ("Ruby/Sapphire-like (variant uncertain)", 0, false, false)
        } else {
            ("Unknown Gen 3 variant", 0, false, false)
        };
        let trainer_name = push(
            &mut doc,
            Value::Bytes(input.get(s0..s0 + 7).unwrap_or(&[]).to_vec()),
            "trainer-name",
            None,
            0,
            true,
        )?;
        let gender = int(
            &mut doc,
            u32::from(
                *input
                    .get(s0 + 8)
                    .ok_or_else(|| Error::new(PARSE, "trainer gender is truncated"))?,
            ),
            1,
            "trainer-gender",
            s0 + 8,
            false,
        )?;
        let tid = int(
            &mut doc,
            u32::from(
                u16_at(input, s0 + 0xa)
                    .ok_or_else(|| Error::new(PARSE, "trainer id is truncated"))?,
            ),
            2,
            "trainer-id",
            s0 + 0xa,
            true,
        )?;
        let sid = int(
            &mut doc,
            u32::from(
                u16_at(input, s0 + 0xc)
                    .ok_or_else(|| Error::new(PARSE, "secret id is truncated"))?,
            ),
            2,
            "secret-id",
            s0 + 0xc,
            true,
        )?;
        let hours = int(
            &mut doc,
            u32::from(
                u16_at(input, s0 + 0xe)
                    .ok_or_else(|| Error::new(PARSE, "play time is truncated"))?,
            ),
            2,
            "hours",
            s0 + 0xe,
            false,
        )?;
        let minutes = int(
            &mut doc,
            u32::from(
                *input
                    .get(s0 + 0x10)
                    .ok_or_else(|| Error::new(PARSE, "play time is truncated"))?,
            ),
            1,
            "minutes",
            s0 + 0x10,
            false,
        )?;
        let seconds = int(
            &mut doc,
            u32::from(
                *input
                    .get(s0 + 0x11)
                    .ok_or_else(|| Error::new(PARSE, "play time is truncated"))?,
            ),
            1,
            "seconds",
            s0 + 0x11,
            false,
        )?;
        let trainer = obj(
            &mut doc,
            vec![
                ("name", trainer_name),
                ("gender", gender),
                ("trainer_id", tid),
                ("secret_id", sid),
                ("hours", hours),
                ("minutes", minutes),
                ("seconds", seconds),
            ],
        )?;
        let frlg = variant == "FireRed/LeafGreen";
        let team_offset = if frlg { 0x34 } else { 0x234 };
        let money_offset = if frlg { 0x290 } else { 0x490 };
        let count = u32_at(input, s1 + team_offset).unwrap_or(0).min(6) as usize;
        let money_stored = u32_at(input, s1 + money_offset)
            .ok_or_else(|| Error::new(PARSE, "money field is truncated"))?;
        let money = if keyed {
            money_stored ^ key
        } else {
            money_stored
        };
        let money_node = int(&mut doc, money, 4, "money", s1 + money_offset, !known)?;
        let coin_stored = u16_at(input, s1 + money_offset + 4)
            .ok_or_else(|| Error::new(PARSE, "coin field is truncated"))?;
        let coins = if keyed {
            coin_stored ^ key as u16
        } else {
            coin_stored
        };
        let coins_node = int(
            &mut doc,
            u32::from(coins),
            2,
            "coins",
            s1 + money_offset + 4,
            !known,
        )?;
        let mut party = Vec::new();
        for slot in 0..count {
            let start = s1
                .checked_add(team_offset + 4 + slot * 100)
                .ok_or_else(|| Error::new(PARSE, "party offset overflow"))?;
            let record = input
                .get(start..start + 100)
                .ok_or_else(|| Error::new(PARSE, "party Pokémon record is truncated"))?;
            let pv =
                u32_at(record, 0).ok_or_else(|| Error::new(PARSE, "personality is truncated"))?;
            let ot =
                u32_at(record, 4).ok_or_else(|| Error::new(PARSE, "trainer id is truncated"))?;
            let mut shuffled = [[0u8; 12]; 4];
            let cipher = record
                .get(0x20..0x50)
                .ok_or_else(|| Error::new(PARSE, "Pokémon data is truncated"))?;
            let keypoke = pv ^ ot;
            let mut plain = [0u8; 48];
            for i in 0..12 {
                let word = u32_at(cipher, i * 4)
                    .ok_or_else(|| Error::new(PARSE, "Pokémon word is truncated"))?
                    ^ keypoke;
                let dst = i * 4;
                plain
                    .get_mut(dst..dst + 4)
                    .ok_or_else(|| Error::new(PARSE, "decrypted Pokémon offset is invalid"))?
                    .copy_from_slice(&word.to_le_bytes());
            }
            let order = ORDERS[(pv % 24) as usize];
            for (part, tag) in order.iter().enumerate() {
                let dest = match tag {
                    b'G' => 0,
                    b'A' => 1,
                    b'E' => 2,
                    _ => 3,
                };
                shuffled[dest].copy_from_slice(
                    plain
                        .get(part * 12..part * 12 + 12)
                        .ok_or_else(|| Error::new(PARSE, "Pokémon substructure is truncated"))?,
                );
            }
            let get32 = |sub: usize, off: usize| -> Result<u32> {
                u32_at(&shuffled[sub], off)
                    .ok_or_else(|| Error::new(PARSE, "Pokémon field is truncated"))
            };
            let exp = get32(0, 4)?;
            let ivs = get32(3, 4)?;
            let pokemon_checksum_valid = u16_at(record, 0x1c) == checksum(&plain);
            let hintbase = format!("pkmn:{start}");
            let leaf = |doc: &mut SaveDoc,
                        value: u32,
                        field: &str,
                        width: usize,
                        read_only: bool|
             -> Result<saveprism_core::NodeId> {
                push(
                    doc,
                    Value::Int {
                        v: i128::from(value),
                        width: match width {
                            1 => IntWidth::U8,
                            2 => IntWidth::U16,
                            _ => IntWidth::U32,
                        },
                    },
                    &format!("{hintbase}:{field}"),
                    None,
                    0,
                    read_only || !pokemon_checksum_valid,
                )
            };
            let iv_names = [
                "hp_iv",
                "attack_iv",
                "defense_iv",
                "speed_iv",
                "sp_attack_iv",
                "sp_defense_iv",
            ];
            let mut pokemon_entries = vec![
                (
                    "personality",
                    push(
                        &mut doc,
                        Value::Int {
                            v: i128::from(pv),
                            width: IntWidth::U32,
                        },
                        "pokemon-metadata",
                        None,
                        0,
                        true,
                    )?,
                ),
                (
                    "ot_id",
                    push(
                        &mut doc,
                        Value::Int {
                            v: i128::from(ot),
                            width: IntWidth::U32,
                        },
                        "pokemon-metadata",
                        None,
                        0,
                        true,
                    )?,
                ),
                (
                    "nickname",
                    push(
                        &mut doc,
                        Value::Bytes(record.get(8..0x12).unwrap_or(&[]).to_vec()),
                        "pokemon-text",
                        None,
                        0,
                        true,
                    )?,
                ),
                (
                    "checksum_valid",
                    push(
                        &mut doc,
                        Value::Bool(pokemon_checksum_valid),
                        "pokemon-checksum",
                        None,
                        0,
                        true,
                    )?,
                ),
                (
                    "species",
                    leaf(&mut doc, get32(0, 0)? as u16 as u32, "species", 2, true)?,
                ),
                (
                    "held_item",
                    leaf(&mut doc, get32(0, 2)? as u16 as u32, "held-item", 2, true)?,
                ),
                ("experience", leaf(&mut doc, exp, "experience", 4, false)?),
                (
                    "friendship",
                    leaf(
                        &mut doc,
                        u32::from(
                            *shuffled[0]
                                .get(9)
                                .ok_or_else(|| Error::new(PARSE, "friendship is missing"))?,
                        ),
                        "friendship",
                        1,
                        false,
                    )?,
                ),
            ];
            for i in 0..4 {
                let pp = *shuffled[1]
                    .get(8 + i)
                    .ok_or_else(|| Error::new(PARSE, "move PP is missing"))?;
                pokemon_entries.push((
                    ["move1_pp", "move2_pp", "move3_pp", "move4_pp"][i],
                    leaf(
                        &mut doc,
                        u32::from(pp),
                        ["move1_pp", "move2_pp", "move3_pp", "move4_pp"][i],
                        1,
                        false,
                    )?,
                ));
            }
            for (i, name) in iv_names.iter().enumerate() {
                let value = (ivs >> (i * 5)) & 31;
                pokemon_entries.push((*name, leaf(&mut doc, value, name, 1, false)?));
            }
            let mut ev_names = [
                "hp_ev",
                "attack_ev",
                "defense_ev",
                "speed_ev",
                "sp_attack_ev",
                "sp_defense_ev",
            ];
            for (i, name) in ev_names.iter_mut().enumerate() {
                pokemon_entries.push((
                    *name,
                    leaf(
                        &mut doc,
                        u32::from(
                            *shuffled[2]
                                .get(i)
                                .ok_or_else(|| Error::new(PARSE, "EV field is missing"))?,
                        ),
                        name,
                        1,
                        false,
                    )?,
                ));
            }
            for (idx, name) in [
                (0x54, "level"),
                (0x56, "current_hp"),
                (0x58, "max_hp"),
                (0x5a, "attack"),
                (0x5c, "defense"),
                (0x5e, "speed"),
                (0x60, "special_attack"),
                (0x62, "special_defense"),
            ] {
                let val = if idx == 0x54 {
                    u32::from(
                        *record
                            .get(idx)
                            .ok_or_else(|| Error::new(PARSE, "level is missing"))?,
                    )
                } else {
                    u32::from(
                        u16_at(record, idx)
                            .ok_or_else(|| Error::new(PARSE, "party stat is missing"))?,
                    )
                };
                pokemon_entries.push((
                    name,
                    leaf(&mut doc, val, name, if idx == 0x54 { 1 } else { 2 }, false)?,
                ));
            }
            party.push(obj(&mut doc, pokemon_entries)?);
        }
        let party_node = doc.push(Node::new(Value::Array(party)))?;
        let money_group = obj(
            &mut doc,
            vec![("balance", money_node), ("coins", coins_node)],
        )?;
        let (pc_offset, pc_slots, pocket_offsets, pocket_sizes) = if frlg {
            (
                0x298,
                30,
                [0x310, 0x3b8, 0x430, 0x464, 0x54c],
                [42, 30, 13, 58, 43],
            )
        } else if variant == "Emerald" {
            (
                0x498,
                50,
                [0x560, 0x5d8, 0x650, 0x690, 0x790],
                [30, 30, 16, 64, 46],
            )
        } else {
            (
                0x498,
                50,
                [0x560, 0x5b0, 0x600, 0x640, 0x740],
                [20, 20, 16, 64, 46],
            )
        };
        let pc_items = item_pocket(
            &mut doc,
            input,
            s1,
            pc_offset,
            pc_slots,
            "item-quantity-pc",
            key,
            keyed,
            !known,
        )?;
        let pocket_names = ["items", "key_items", "balls", "tm_hm", "berries"];
        let mut pockets = Vec::with_capacity(5);
        for i in 0..5 {
            pockets.push((
                pocket_names[i],
                item_pocket(
                    &mut doc,
                    input,
                    s1,
                    pocket_offsets[i],
                    pocket_sizes[i],
                    "item-quantity-bag",
                    key,
                    keyed,
                    !known,
                )?,
            ));
        }
        let bag = obj(&mut doc, pockets)?;
        let inventory = obj(&mut doc, vec![("pc", pc_items), ("bag", bag)])?;
        let game_node = push(&mut doc, Value::Str(variant.into()), "game", None, 0, true)?;
        let mut root_fields = vec![
            ("game", game_node),
            ("trainer", trainer),
            ("money", money_group),
            ("inventory", inventory),
            ("party", party_node),
        ];
        if input.len() >= BLOCK * 2 {
            let inactive_base = if block.base == 0 { BLOCK } else { 0 };
            let previous = push(
                &mut doc,
                Value::Bytes(
                    input
                        .get(inactive_base..inactive_base + BLOCK)
                        .unwrap_or(&[])
                        .to_vec(),
                ),
                "inactive-save-block",
                None,
                0,
                true,
            )?;
            root_fields.push(("inactive_save_block", previous));
            let trailing = push(
                &mut doc,
                Value::Bytes(input.get(BLOCK * 2..).unwrap_or(&[]).to_vec()),
                "trailing-data",
                None,
                0,
                true,
            )?;
            root_fields.push(("trailing_data", trailing));
        } else {
            let trailing = push(
                &mut doc,
                Value::Bytes(input.get(BLOCK..).unwrap_or(&[]).to_vec()),
                "trailing-data",
                None,
                0,
                true,
            )?;
            root_fields.push(("trailing_data", trailing));
        }
        let root = obj(&mut doc, root_fields)?;
        doc.set_root(root);
        let mut params = Params::new();
        params.set(ORIGINAL, input.to_vec());
        params.set("active_base", block.base as i64);
        params.set("key", i64::from(key));
        params.set("keyed", keyed);
        params.set("variant", variant);
        params.set("known_variant", known);
        let _ = section_offset(&block, 0, 0)?;
        ctx.charge_nodes(len_u64(doc.len()))?;
        Ok(Decoded { doc, params })
    }
    #[allow(clippy::indexing_slicing)] // Indices below come from validated fixed-size records.
    fn encode(&self, doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
        let original = match params.get(ORIGINAL) {
            Some(saveprism_core::ParamValue::Bytes(bytes)) => bytes.clone(),
            _ => return Err(Error::new(WRITE, "original save bytes are missing")),
        };
        let base = match params.get("active_base") {
            Some(saveprism_core::ParamValue::Int(v)) => usize::try_from(*v)
                .map_err(|_| Error::new(WRITE, "active block offset is invalid"))?,
            _ => return Err(Error::new(WRITE, "active block metadata is missing")),
        };
        let block = checked_block(&original, base)
            .ok_or_else(|| Error::new(WRITE, "the active block is invalid"))?;
        let mut out = original.clone();
        for id in 0..doc.len() {
            let Some(node) = doc.get_index(id) else {
                continue;
            };
            let hint = node.type_hint.as_ref().map(TypeHint::as_str).unwrap_or("");
            if !hint.starts_with(HINT) {
                continue;
            }
            let field = hint.trim_start_matches(HINT);
            if field.starts_with("pkmn:") {
                continue;
            }
            let Some(span) = node.span else { continue };
            let Value::Int { v, .. } = node.value else {
                continue;
            };
            let width = usize::try_from(span.end.saturating_sub(span.start))
                .map_err(|_| Error::new(WRITE, "field width is invalid"))?;
            let absolute = usize::try_from(span.start)
                .map_err(|_| Error::new(WRITE, "field offset is invalid"))?;
            let val = u32::try_from(v)
                .map_err(|_| Error::new(WRITE, "edited field is outside its unsigned range"))?;
            if (field == "money" && val > 999_999)
                || (field == "coins" && val > 9_999)
                || (field == "trainer-gender" && val > 1)
                || (field == "minutes" && val > 59)
                || (field == "seconds" && val > 59)
                || (field == "item-quantity-pc" && val > 999)
                || (field == "item-quantity-bag" && val > 99)
            {
                return Err(Error::new(
                    WRITE,
                    format!("edited {field} value is outside its game range"),
                ));
            }
            let stored = if field == "money" {
                let key = match params.get("key") {
                    Some(saveprism_core::ParamValue::Int(v)) => *v as u32,
                    _ => 0,
                };
                if matches!(
                    params.get("keyed"),
                    Some(saveprism_core::ParamValue::Bool(true))
                ) {
                    val ^ key
                } else {
                    val
                }
            } else if field == "coins" || field == "item-quantity-bag" {
                let key = match params.get("key") {
                    Some(saveprism_core::ParamValue::Int(v)) => *v as u16,
                    _ => 0,
                };
                if matches!(
                    params.get("keyed"),
                    Some(saveprism_core::ParamValue::Bool(true))
                ) {
                    u32::from((val as u16) ^ key)
                } else {
                    val
                }
            } else {
                val
            };
            let target = out
                .get_mut(
                    absolute
                        ..absolute
                            .checked_add(width)
                            .ok_or_else(|| Error::new(WRITE, "field range overflow"))?,
                )
                .ok_or_else(|| Error::new(WRITE, "edited field is outside the save"))?;
            match width {
                1 => {
                    *target
                        .first_mut()
                        .ok_or_else(|| Error::new(WRITE, "field is missing"))? = stored as u8
                }
                2 => target.copy_from_slice(&(stored as u16).to_le_bytes()),
                4 => target.copy_from_slice(&stored.to_le_bytes()),
                _ => return Err(Error::new(WRITE, "unsupported field width")),
            }
        }
        // Current party Pokémon field edits are applied to the decrypted substructure tree below.
        for id in 0..doc.len() {
            let Some(node) = doc.get_index(id) else {
                continue;
            };
            let hint = node.type_hint.as_ref().map(TypeHint::as_str).unwrap_or("");
            let Some(tail) = hint.strip_prefix("GEN3:pkmn:") else {
                continue;
            };
            let Some((start_text, field)) = tail.split_once(':') else {
                continue;
            };
            let Ok(start) = start_text.parse::<usize>() else {
                return Err(Error::new(WRITE, "Pokémon record location is invalid"));
            };
            let Value::Int { v, .. } = node.value else {
                continue;
            };
            let val = u32::try_from(v)
                .map_err(|_| Error::new(WRITE, "Pokémon field is outside its unsigned range"))?;
            if (field.ends_with("_iv") && val > 31)
                || (field.ends_with("_ev") && val > 255)
                || (field.ends_with("_pp") && val > 255)
                || (field == "friendship" && val > 255)
                || (field == "level" && !(1..=100).contains(&val))
            {
                return Err(Error::new(
                    WRITE,
                    format!("Pokémon {field} value is outside its game range"),
                ));
            }
            if field == "experience"
                || field.ends_with("_iv")
                || field.ends_with("_ev")
                || field.ends_with("_pp")
                || field == "friendship"
            {
                let record = out
                    .get(start..start + 100)
                    .ok_or_else(|| Error::new(WRITE, "Pokémon record is outside the save"))?
                    .to_vec();
                let pv = u32_at(&record, 0)
                    .ok_or_else(|| Error::new(WRITE, "personality is missing"))?;
                let ot =
                    u32_at(&record, 4).ok_or_else(|| Error::new(WRITE, "trainer id is missing"))?;
                let mut plain = [0u8; 48];
                let encrypted = record
                    .get(0x20..0x50)
                    .ok_or_else(|| Error::new(WRITE, "encrypted data is missing"))?;
                for i in 0..12 {
                    let word = u32_at(encrypted, i * 4)
                        .ok_or_else(|| Error::new(WRITE, "encrypted word is invalid"))?
                        ^ (pv ^ ot);
                    plain
                        .get_mut(i * 4..i * 4 + 4)
                        .ok_or_else(|| Error::new(WRITE, "decrypted word is outside the record"))?
                        .copy_from_slice(&word.to_le_bytes());
                }
                let order = ORDERS[(pv % 24) as usize];
                let mut subs = [[0u8; 12]; 4];
                for (part, tag) in order.iter().enumerate() {
                    let index = match tag {
                        b'G' => 0,
                        b'A' => 1,
                        b'E' => 2,
                        _ => 3,
                    };
                    subs[index].copy_from_slice(&plain[part * 12..part * 12 + 12]);
                }
                match field {
                    "experience" => subs[0][4..8].copy_from_slice(&val.to_le_bytes()),
                    "friendship" => subs[0][9] = val as u8,
                    "move1_pp" | "move2_pp" | "move3_pp" | "move4_pp" => {
                        let n = field
                            .as_bytes()
                            .get(4)
                            .and_then(|b| b.checked_sub(b'1'))
                            .map(usize::from)
                            .unwrap_or(0);
                        if let Some(slot) = subs[1].get_mut(8 + n) {
                            *slot = val as u8;
                        }
                    }
                    "hp_ev" | "attack_ev" | "defense_ev" | "speed_ev" | "sp_attack_ev"
                    | "sp_defense_ev" => {
                        let n = [
                            "hp_ev",
                            "attack_ev",
                            "defense_ev",
                            "speed_ev",
                            "sp_attack_ev",
                            "sp_defense_ev",
                        ]
                        .iter()
                        .position(|x| *x == field)
                        .ok_or_else(|| Error::new(WRITE, "EV field is invalid"))?;
                        subs[2][n] = val as u8;
                    }
                    _ => {
                        let n = [
                            "hp_iv",
                            "attack_iv",
                            "defense_iv",
                            "speed_iv",
                            "sp_attack_iv",
                            "sp_defense_iv",
                        ]
                        .iter()
                        .position(|x| *x == field)
                        .ok_or_else(|| Error::new(WRITE, "IV field is invalid"))?;
                        let ivword = u32_at(&subs[3], 4)
                            .ok_or_else(|| Error::new(WRITE, "IV field is missing"))?;
                        let mask = 31u32 << (n * 5);
                        subs[3][4..8].copy_from_slice(
                            &((ivword & !mask) | ((val & 31) << (n * 5))).to_le_bytes(),
                        );
                    }
                }
                let mut decrypted = [0u8; 48];
                for (part, tag) in order.iter().enumerate() {
                    let index = match tag {
                        b'G' => 0,
                        b'A' => 1,
                        b'E' => 2,
                        _ => 3,
                    };
                    decrypted[part * 12..part * 12 + 12].copy_from_slice(&subs[index]);
                }
                let sum = checksum(&decrypted)
                    .ok_or_else(|| Error::new(WRITE, "Pokémon checksum length is invalid"))?;
                let dst = out
                    .get_mut(start + 0x1c..start + 0x1e)
                    .ok_or_else(|| Error::new(WRITE, "Pokémon checksum is outside the save"))?;
                dst.copy_from_slice(&sum.to_le_bytes());
                let enc = out
                    .get_mut(start + 0x20..start + 0x50)
                    .ok_or_else(|| Error::new(WRITE, "Pokémon data is outside the save"))?;
                for i in 0..12 {
                    let word = u32_at(&decrypted, i * 4)
                        .ok_or_else(|| Error::new(WRITE, "Pokémon word is invalid"))?
                        ^ (pv ^ ot);
                    enc.get_mut(i * 4..i * 4 + 4)
                        .ok_or_else(|| Error::new(WRITE, "encrypted word is outside the record"))?
                        .copy_from_slice(&word.to_le_bytes());
                }
            } else if [
                "level",
                "current_hp",
                "max_hp",
                "attack",
                "defense",
                "speed",
                "special_attack",
                "special_defense",
            ]
            .contains(&field)
            {
                let idx = match field {
                    "level" => 0x54,
                    "current_hp" => 0x56,
                    "max_hp" => 0x58,
                    "attack" => 0x5a,
                    "defense" => 0x5c,
                    "speed" => 0x5e,
                    "special_attack" => 0x60,
                    _ => 0x62,
                };
                let width = if field == "level" { 1 } else { 2 };
                let dst = out
                    .get_mut(start + idx..start + idx + width)
                    .ok_or_else(|| Error::new(WRITE, "party stat is outside the save"))?;
                if width == 1 {
                    dst[0] = val as u8
                } else {
                    dst.copy_from_slice(&(val as u16).to_le_bytes())
                }
            }
        }
        // Every sector checksum is refreshed only in the edited active block.
        for id in 0..14 {
            let at = *block
                .sections
                .get(id)
                .ok_or_else(|| Error::new(WRITE, "section is missing"))?;
            let size = match id {
                0 => 3884,
                4 => 3848,
                13 => 2000,
                _ => 3968,
            };
            let sum = checksum(
                out.get(at..at + size)
                    .ok_or_else(|| Error::new(WRITE, "section data is truncated"))?,
            )
            .ok_or_else(|| Error::new(WRITE, "section checksum length is invalid"))?;
            out.get_mut(at + 0xff6..at + 0xff8)
                .ok_or_else(|| Error::new(WRITE, "section checksum is outside the save"))?
                .copy_from_slice(&sum.to_le_bytes());
        }
        Ok(out)
    }
}

const ORDERS: [[u8; 4]; 24] = [
    *b"GAEM", *b"GAME", *b"GEAM", *b"GEMA", *b"GMAE", *b"GMEA", *b"AGEM", *b"AGME", *b"AEGM",
    *b"AEMG", *b"AMGE", *b"AMEG", *b"EGAM", *b"EGMA", *b"EAGM", *b"EAMG", *b"EMGA", *b"EMAG",
    *b"MGAE", *b"MGEA", *b"MAGE", *b"MAEG", *b"MEGA", *b"MEAG",
];

#[cfg(test)]
mod tests;
