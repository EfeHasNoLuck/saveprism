//! RPG Maker 2000 and 2003 saves (LCF, `SaveNN.lsd`) as game sections.
//!
//! The LCF codec reads a save as an object of named fields (see `saveprism_codecs::lcf`). The
//! sections are the ones of the other RPG Maker views ([`RpgMakerView`]): gold, item counts and the
//! party from `/inventory`, the party members from the `/actors` array (matched by element id),
//! variables and switches from `/system` (vector index 0 is id 1). Names come from the game's
//! database `RPG_RT.ldb` next to the save (these engines save next to `RPG_RT.exe`), read with the
//! same codec as a data tree; nothing is executed. Item types from the database split the
//! inventory into items, weapons and armors; without the database every entry is an item.

use std::collections::HashMap;
use std::path::{Path as FsPath, PathBuf};

use saveprism_codecs::lcf::{ID_KEY, LcfCodec};
use saveprism_core::{Codec, DecodeCtx, Limits, NodeId, Path, SaveDoc, Value};

use super::Session;
use super::game::{
    Actor, AddLists, At, Bonus, Bounds, COUNT, Catalog, Ctx, Entry, Field, FieldKind, LEVEL,
    MAX_VALUE_ROWS, NON_NEGATIVE, RpgMakerView, UNLIMITED, read_data_file,
};

/// Gold limit of RPG Maker 2000 and 2003.
const GOLD_2K: Bounds = Bounds::new(Some(0), Some(999_999));
/// Stat bonuses: field and stable key (the UI translates the key).
const BONUSES: [(&str, &str); 6] = [
    ("hp_mod", "maxHp"),
    ("sp_mod", "maxMp"),
    ("attack_mod", "attack"),
    ("defense_mod", "defense"),
    ("spirit_mod", "spirit"),
    ("agility_mod", "agility"),
];
/// Save fields that only RPG Maker 2003 writes: `SaveSystem` and `SaveActor`.
const SYSTEM_2K3: [&str; 1] = ["atb_mode"];
const ACTOR_2K3: [&str; 7] = [
    "battle_commands",
    "row",
    "two_weapon",
    "lock_equipment",
    "auto_battle",
    "super_guard",
    "battler_animation",
];
/// Top-level database chunks that only RPG Maker 2003 writes (0x1B..0x20).
const DATABASE_2K3: [&str; 6] = [
    "0x1b",
    "0x1c",
    "battlecommands",
    "classes",
    "0x1f",
    "battleranimations",
];
/// Hint of the root of an LCF save.
const SAVE_ROOT: &str = "LCF:struct:Save";
/// Hint of the root of an LCF game database.
const DATABASE_ROOT: &str = "LCF:struct:Database";
/// Name of an actor that uses its database name.
const DATABASE_NAME: &[u8] = b"\x01";
/// Item types of weapons and armors (shield, armor, helmet, accessory).
const WEAPON: i128 = 1;
const ARMORS: std::ops::RangeInclusive<i128> = 2..=5;

/// Characters of Windows-1252 bytes 0x80..=0x9F; the five unassigned bytes keep their C1 code.
const CP1252_HIGH: [char; 32] = [
    '\u{20ac}', '\u{81}', '\u{201a}', '\u{192}', '\u{201e}', '\u{2026}', '\u{2020}', '\u{2021}',
    '\u{2c6}', '\u{2030}', '\u{160}', '\u{2039}', '\u{152}', '\u{8d}', '\u{17d}', '\u{8f}',
    '\u{90}', '\u{2018}', '\u{2019}', '\u{201c}', '\u{201d}', '\u{2022}', '\u{2013}', '\u{2014}',
    '\u{2dc}', '\u{2122}', '\u{161}', '\u{203a}', '\u{153}', '\u{9d}', '\u{17e}', '\u{178}',
];

/// Text of name bytes: UTF-8 when valid, else Windows-1252.
fn name_from_bytes(bytes: &[u8]) -> String {
    if let Ok(text) = std::str::from_utf8(bytes) {
        return text.to_string();
    }
    bytes
        .iter()
        .map(|byte| match byte {
            0x80..=0x9F => CP1252_HIGH
                .get(usize::from(byte - 0x80))
                .copied()
                .unwrap_or('\u{fffd}'),
            _ => char::from(*byte),
        })
        .collect()
}

/// Names and item types from the game database.
#[derive(Default)]
pub(super) struct LcfNames {
    folder: Option<PathBuf>,
    actors: HashMap<u32, String>,
    items: HashMap<u32, String>,
    item_types: HashMap<u32, i128>,
    variables: HashMap<u32, String>,
    switches: HashMap<u32, String>,
    is_2k3: bool,
}

/// The node `key` of the object `id`.
fn child(doc: &SaveDoc, id: NodeId, key: &str) -> Option<NodeId> {
    match &doc.get(id)?.value {
        Value::Object(entries) => entries
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, child)| *child),
        _ => None,
    }
}

fn int_of(doc: &SaveDoc, id: NodeId) -> Option<i128> {
    match doc.get(id)?.value {
        Value::Int { v, .. } => Some(v),
        _ => None,
    }
}

/// A non-empty name: a string, or bytes read as UTF-8 or Windows-1252.
fn text_of(doc: &SaveDoc, id: NodeId) -> Option<String> {
    let text = match &doc.get(id)?.value {
        Value::Str(text) => text.clone(),
        Value::Bytes(bytes) => name_from_bytes(bytes),
        _ => return None,
    };
    (!text.is_empty()).then_some(text)
}

/// The elements of the database array `key` with their ids.
fn database_elements(doc: &SaveDoc, key: &str) -> Vec<(u32, NodeId)> {
    let list = child(doc, doc.root(), key).and_then(|id| doc.get(id));
    let Some(Value::Array(elements)) = list.map(|node| &node.value) else {
        return Vec::new();
    };
    elements
        .iter()
        .filter_map(|element| {
            let id = child(doc, *element, ID_KEY).and_then(|id| int_of(doc, id))?;
            Some((u32::try_from(id).ok()?, *element))
        })
        .collect()
}

fn element_names(doc: &SaveDoc, key: &str) -> HashMap<u32, String> {
    database_elements(doc, key)
        .into_iter()
        .filter_map(|(id, element)| {
            let name = child(doc, element, "name").and_then(|name| text_of(doc, name))?;
            Some((id, name))
        })
        .collect()
}

/// The game database read with the LCF codec as a data tree; `None` when it is missing, too large
/// or not an LCF database.
fn read_database(path: &FsPath) -> Option<SaveDoc> {
    let bytes = read_data_file(path)?;
    let limits = Limits::default();
    let mut ctx = DecodeCtx::new(&limits);
    let doc = LcfCodec.decode(&bytes, &mut ctx).ok()?.doc;
    let root = doc.get(doc.root())?.type_hint.as_ref()?.as_str() == DATABASE_ROOT;
    root.then_some(doc)
}

/// Looks for `RPG_RT.ldb` in the save's folder, then one folder up (games that save in a
/// subfolder).
fn load_names(save_path: &FsPath) -> LcfNames {
    // A relative save path would resolve against the process folder, not the game's.
    if !save_path.is_absolute() {
        return LcfNames::default();
    }
    let Some(save_dir) = save_path.parent() else {
        return LcfNames::default();
    };
    let mut folders = vec![save_dir.to_path_buf()];
    if let Some(game_root) = save_dir.parent() {
        folders.push(game_root.to_path_buf());
    }
    for folder in folders {
        let Some(doc) = read_database(&folder.join("RPG_RT.ldb")) else {
            continue;
        };
        let item_types = database_elements(&doc, "items")
            .into_iter()
            .filter_map(|(id, item)| {
                let kind = child(&doc, item, "type").and_then(|kind| int_of(&doc, kind))?;
                Some((id, kind))
            })
            .collect();
        let is_2k3 = DATABASE_2K3
            .iter()
            .any(|key| child(&doc, doc.root(), key).is_some());
        return LcfNames {
            actors: element_names(&doc, "actors"),
            items: element_names(&doc, "items"),
            item_types,
            variables: element_names(&doc, "variables"),
            switches: element_names(&doc, "switches"),
            is_2k3,
            folder: Some(folder),
        };
    }
    LcfNames::default()
}

/// Gold that the save does not store (2000/2003 leave it out at 0): shown as 0, read-only.
fn absent_gold(path: Path) -> Field {
    Field {
        path: path.to_string(),
        value: "0".to_string(),
        original: "0".to_string(),
        kind: FieldKind::Int,
        type_name: None,
        min: Some(0),
        max: Some(999_999),
        hint: "Not stored in this save (0); the game writes gold once the party has some".into(),
        editable: false,
        read_only_reason: None,
        added: false,
    }
}

impl Ctx<'_> {
    fn has_any(&self, at: &At, keys: &[&str]) -> bool {
        keys.iter().any(|key| self.entry(at, key).is_some())
    }

    /// "2k3" when the save has 2003-only fields or the database is a 2003 one, else "2k".
    fn lcf_engine(&self, names: &LcfNames) -> &'static str {
        let root: At = (self.doc.root(), Path::root());
        let system = self
            .entry(&root, "system")
            .is_some_and(|system| self.has_any(&system, &SYSTEM_2K3));
        let actors = self.entry(&root, "actors").is_some_and(|actors| {
            self.items(&actors)
                .iter()
                .any(|(_, actor)| self.has_any(actor, &ACTOR_2K3))
        });
        if system || actors || names.is_2k3 {
            "2k3"
        } else {
            "2k"
        }
    }

    /// The actor's name: its own, or the database name when it is `"\x01"` or missing.
    fn lcf_actor_name(&self, id: u32, at: &At, names: &LcfNames) -> String {
        let own = self
            .entry(at, "name")
            .filter(|name| {
                !matches!(self.doc.get(name.0).map(|node| &node.value),
                    Some(Value::Bytes(bytes)) if bytes == DATABASE_NAME)
            })
            .and_then(|name| text_of(self.doc, name.0));
        own.or_else(|| names.actors.get(&id).cloned())
            .unwrap_or_else(|| format!("Actor #{id}"))
    }

    fn lcf_actor(&self, id: u32, at: &At, names: &LcfNames) -> Option<Actor> {
        let level = self.field(&self.entry(at, "level")?, LEVEL)?;
        let hp = self.field(&self.entry(at, "current_hp")?, NON_NEGATIVE)?;
        let mp = self.field(&self.entry(at, "current_sp")?, NON_NEGATIVE)?;
        let exp = self
            .entry(at, "exp")
            .and_then(|exp| self.field(&exp, NON_NEGATIVE));
        let bonuses = BONUSES
            .iter()
            .filter_map(|(key, stat)| {
                Some(Bonus {
                    key: (*stat).to_string(),
                    field: self.field(&self.entry(at, key)?, UNLIMITED)?,
                })
            })
            .collect();
        Some(Actor {
            id,
            name: self.lcf_actor_name(id, at, names),
            level,
            hp,
            mp,
            exp,
            bonuses,
            level_updates_exp: true,
        })
    }

    /// The party members: actor ids from `/inventory/party`, data from the `/actors` element with
    /// that id.
    fn lcf_actors(&self, root: &At, inventory: &At, names: &LcfNames) -> Vec<Actor> {
        let Some(party) = self.entry(inventory, "party") else {
            return Vec::new();
        };
        let mut by_id: HashMap<i128, At> = HashMap::new();
        if let Some(actors) = self.entry(root, "actors") {
            for (_, actor) in self.items(&actors) {
                if let Some(id) = self.entry(&actor, ID_KEY).and_then(|id| self.int_at(&id)) {
                    by_id.entry(id).or_insert(actor);
                }
            }
        }
        let mut out = Vec::new();
        for (_, member) in self.items(&party) {
            let Some(id) = self.int_at(&member) else {
                continue;
            };
            let (Ok(id_u32), Some(at)) = (u32::try_from(id), by_id.get(&id)) else {
                continue;
            };
            if let Some(actor) = self.lcf_actor(id_u32, at, names) {
                out.push(actor);
            }
        }
        out
    }

    /// Items, weapons and armors: `item_ids[i]` with `item_counts[i]`, split by the database type.
    fn lcf_items(&self, inventory: &At, names: &LcfNames) -> [Vec<Entry>; 3] {
        let mut out: [Vec<Entry>; 3] = Default::default();
        let (Some(ids), Some(counts)) = (
            self.entry(inventory, "item_ids"),
            self.entry(inventory, "item_counts"),
        ) else {
            return out;
        };
        let counts: HashMap<usize, At> = self.items(&counts).into_iter().collect();
        let mut total = 0;
        for (index, id_at) in self.items(&ids) {
            if total >= MAX_VALUE_ROWS {
                break;
            }
            let Some(id) = self.int_at(&id_at).and_then(|id| u32::try_from(id).ok()) else {
                continue;
            };
            let Some(field) = counts.get(&index).and_then(|at| self.field(at, COUNT)) else {
                continue;
            };
            let slot = match names.item_types.get(&id) {
                Some(&WEAPON) => 1,
                Some(kind) if ARMORS.contains(kind) => 2,
                _ => 0,
            };
            if let Some(list) = out.get_mut(slot) {
                list.push(Entry {
                    id,
                    name: names.items.get(&id).cloned(),
                    field,
                    removable: false,
                });
                total += 1;
            }
        }
        for list in &mut out {
            list.sort_by_key(|entry| entry.id);
        }
        out
    }

    /// Variables or switches: element `i` is id `i + 1`. An entry with no name and an empty
    /// value (0, off) is left out unless it was changed since the file was opened.
    fn lcf_indexed(&self, list: Option<At>, names: &HashMap<u32, String>) -> Vec<Entry> {
        let Some(list) = list else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for (index, at) in self.items(&list) {
            if out.len() >= MAX_VALUE_ROWS {
                break;
            }
            let Some(id) = index.checked_add(1).and_then(|id| u32::try_from(id).ok()) else {
                continue;
            };
            let Some(field) = self.field(&at, UNLIMITED) else {
                continue;
            };
            let named = names.contains_key(&id);
            let empty = matches!(field.value.as_str(), "0" | "false");
            if !named && empty && field.value == field.original {
                continue;
            }
            out.push(Entry {
                id,
                name: names.get(&id).cloned(),
                field,
                removable: false,
            });
        }
        out
    }

    /// The RPG Maker sections of a 2000/2003 save, or `None` when it has no inventory.
    pub(super) fn lcf_view(&self, names: &LcfNames) -> Option<RpgMakerView> {
        let root: At = (self.doc.root(), Path::root());
        let inventory = self.entry(&root, "inventory")?;
        let gold = match self.entry(&inventory, "gold") {
            Some(at) => self.field(&at, GOLD_2K)?,
            None => {
                let mut path = inventory.1.clone();
                path.push("gold");
                absent_gold(path)
            }
        };
        let system = self.entry(&root, "system");
        let section = |key: &str, names: &HashMap<u32, String>| {
            self.lcf_indexed(system.as_ref().and_then(|at| self.entry(at, key)), names)
        };
        let [items, weapons, armors] = self.lcf_items(&inventory, names);
        Some(RpgMakerView {
            engine: self.lcf_engine(names).to_string(),
            gold,
            actors: self.lcf_actors(&root, &inventory, names),
            items,
            weapons,
            armors,
            variables: section("variables", &names.variables),
            switches: section("switches", &names.switches),
            names_folder: names
                .folder
                .as_ref()
                .map(|folder| folder.to_string_lossy().into_owned()),
            catalog: Catalog::default(),
            can_add: AddLists::default(),
        })
    }
}

impl Session {
    /// True for an RPG Maker 2000/2003 save (not its database).
    pub(super) fn is_lcf_save(&self) -> bool {
        let lcf = self
            .recipe
            .steps
            .iter()
            .any(|step| step.kind == "format.lcf");
        lcf && self
            .doc
            .get(self.doc.root())
            .and_then(|root| root.type_hint.as_ref())
            .is_some_and(|hint| hint.as_str() == SAVE_ROOT)
    }

    /// The limits of the 2000/2003 view (no database is read: the limits do not depend on names).
    pub(super) fn lcf_bounds(&self, ctx: &Ctx<'_>) {
        let _ = ctx.lcf_view(&LcfNames::default());
    }

    /// The RPG Maker sections of a 2000/2003 save, with names from `RPG_RT.ldb`; `None` for other
    /// files.
    pub(super) fn lcf_view(&self, ctx: &Ctx<'_>) -> Option<RpgMakerView> {
        if !self.is_lcf_save() {
            return None;
        }
        // A pasted code has no game folder next to it.
        let names = if self.pasted {
            LcfNames::default()
        } else {
            load_names(&self.path)
        };
        ctx.lcf_view(&names)
    }
}

/// The `[EasyRPG] Encoding=` code page from `RPG_RT.ini` next to the save, when present.
/// EasyRPG documents the setting as a code page number (932 for Shift-JIS, 1252 for Western);
/// see `https://wiki.easyrpg.org/doku.php?id=user:player:special-features`.
pub(super) fn easyrpg_encoding(save: &FsPath) -> Option<&'static str> {
    let path = save.parent()?.join("RPG_RT.ini");
    let bytes = std::fs::read(path).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    let mut section = String::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if let Some(inner) = line
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
        {
            section = inner.trim().to_ascii_lowercase();
            continue;
        }
        if section != "easyrpg" {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if !key.trim().eq_ignore_ascii_case("encoding") {
            continue;
        }
        let value = value.trim().trim_matches('"').trim_matches('\'');
        return saveprism_codecs::resolve_text_encoding(value);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::name_from_bytes;

    #[test]
    fn names_are_utf8_or_windows_1252() {
        assert_eq!(name_from_bytes(b"Alex"), "Alex");
        assert_eq!(name_from_bytes("Jos\u{e9}".as_bytes()), "Jos\u{e9}");
        assert_eq!(name_from_bytes(b"Jos\xe9"), "Jos\u{e9}");
        assert_eq!(
            name_from_bytes(b"\x80 \x8a\x9f\x81"),
            "\u{20ac} \u{160}\u{178}\u{81}"
        );
    }
}
