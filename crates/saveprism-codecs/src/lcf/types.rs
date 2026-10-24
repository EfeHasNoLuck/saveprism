//! Field tables of the LCF structs this codec names (ids from the public liblcf description).
//! Types with an empty table keep every chunk as read-only bytes.

use super::Kind;

/// Root type of a save (`LcfSaveData`).
pub(crate) const SAVE: &str = "Save";
/// Root type of a game database (`LcfDataBase`).
pub(crate) const DATABASE: &str = "Database";

/// One chunk of a struct: its id, name and kind.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Field {
    pub(crate) id: u32,
    pub(crate) name: &'static str,
    pub(crate) kind: Kind<'static>,
}

/// A struct type and its chunks.
#[derive(Debug)]
pub(crate) struct TypeDef {
    pub(crate) name: &'static str,
    fields: &'static [Field],
}

impl TypeDef {
    /// The field with chunk id `id`.
    pub(crate) fn by_id(&self, id: u32) -> Option<&'static Field> {
        self.fields.iter().find(|field| field.id == id)
    }

    /// The chunk id of the entry `key`: a field name, or `0x..` for ids not in the table.
    pub(crate) fn id_of(&self, key: &str) -> Option<u32> {
        if let Some(field) = self.fields.iter().find(|field| field.name == key) {
            return Some(field.id);
        }
        let hex = key.strip_prefix("0x")?;
        let id = u32::from_str_radix(hex, 16).ok()?;
        // A named id written as hex would not read back under the same key.
        (self.by_id(id).is_none() && id != 0).then_some(id)
    }
}

/// The key of a chunk with id `id` in a struct of type `ty`.
pub(crate) fn key_of(ty: &TypeDef, id: u32) -> String {
    match ty.by_id(id) {
        Some(field) => field.name.to_string(),
        None => format!("0x{id:02x}"),
    }
}

const fn f(id: u32, name: &'static str, kind: Kind<'static>) -> Field {
    Field { id, name, kind }
}

const fn t(name: &'static str, fields: &'static [Field]) -> TypeDef {
    TypeDef { name, fields }
}

use Kind::{Array, Bool, Bools, Count, Double, Int, Int8, Int16s, Int32s, Struct, Text, UInt8s};

const SAVE_FIELDS: &[Field] = &[
    f(0x64, "title", Struct("SaveTitle")),
    f(0x65, "system", Struct("SaveSystem")),
    f(0x66, "screen", Struct("SaveScreen")),
    f(0x67, "pictures", Array("SavePicture")),
    f(0x68, "party_location", Struct("SavePartyLocation")),
    f(0x69, "boat", Struct("SaveVehicleLocation")),
    f(0x6A, "ship", Struct("SaveVehicleLocation")),
    f(0x6B, "airship", Struct("SaveVehicleLocation")),
    f(0x6C, "actors", Array("SaveActor")),
    f(0x6D, "inventory", Struct("SaveInventory")),
    f(0x6E, "targets", Array("SaveTarget")),
    f(0x6F, "map_info", Struct("SaveMapInfo")),
    f(0x70, "panorama", Struct("SavePanorama")),
    f(
        0x71,
        "foreground_event_execstate",
        Struct("SaveEventExecState"),
    ),
    f(0x72, "common_events", Array("SaveCommonEvent")),
];

const TITLE_FIELDS: &[Field] = &[
    f(0x01, "timestamp", Double),
    f(0x0B, "hero_name", Text),
    f(0x0C, "hero_level", Int),
    f(0x0D, "hero_hp", Int),
    f(0x15, "face1_name", Text),
    f(0x16, "face1_id", Int),
    f(0x17, "face2_name", Text),
    f(0x18, "face2_id", Int),
    f(0x19, "face3_name", Text),
    f(0x1A, "face3_id", Int),
    f(0x1B, "face4_name", Text),
    f(0x1C, "face4_id", Int),
];

const SYSTEM_FIELDS: &[Field] = &[
    f(0x01, "scene", Int),
    f(0x0B, "frame_count", Int),
    f(0x15, "graphics_name", Text),
    f(0x1F, "switches_size", Count),
    f(0x20, "switches", Bools),
    f(0x21, "variables_size", Count),
    f(0x22, "variables", Int32s),
    f(0x47, "title_music", Struct("Music")),
    f(0x48, "battle_music", Struct("Music")),
    f(0x49, "battle_end_music", Struct("Music")),
    f(0x4A, "inn_music", Struct("Music")),
    f(0x4B, "current_music", Struct("Music")),
    f(0x4C, "before_vehicle_music", Struct("Music")),
    f(0x4D, "before_battle_music", Struct("Music")),
    f(0x4E, "stored_music", Struct("Music")),
    f(0x4F, "boat_music", Struct("Music")),
    f(0x50, "ship_music", Struct("Music")),
    f(0x51, "airship_music", Struct("Music")),
    f(0x52, "gameover_music", Struct("Music")),
    f(0x5B, "cursor_se", Struct("Sound")),
    f(0x5C, "decision_se", Struct("Sound")),
    f(0x5D, "cancel_se", Struct("Sound")),
    f(0x5E, "buzzer_se", Struct("Sound")),
    f(0x5F, "battle_se", Struct("Sound")),
    f(0x60, "escape_se", Struct("Sound")),
    f(0x61, "enemy_attack_se", Struct("Sound")),
    f(0x62, "enemy_hurt_se", Struct("Sound")),
    f(0x63, "actor_hurt_se", Struct("Sound")),
    f(0x64, "dodge_se", Struct("Sound")),
    f(0x65, "enemy_death_se", Struct("Sound")),
    f(0x66, "item_se", Struct("Sound")),
    f(0x6F, "transition_out", Int8),
    f(0x70, "transition_in", Int8),
    f(0x71, "battle_start_fadeout", Int8),
    f(0x72, "battle_start_fadein", Int8),
    f(0x73, "battle_end_fadeout", Int8),
    f(0x74, "battle_end_fadein", Int8),
    f(0x79, "teleport_allowed", Bool),
    f(0x7A, "escape_allowed", Bool),
    f(0x7B, "save_allowed", Bool),
    f(0x7C, "menu_allowed", Bool),
    f(0x83, "save_count", Int),
    f(0x84, "save_slot", Int),
    f(0x8C, "atb_mode", Int),
];

const MUSIC_FIELDS: &[Field] = &[
    f(0x01, "name", Text),
    f(0x02, "fadein", Int),
    f(0x03, "volume", Int),
    f(0x04, "tempo", Int),
    f(0x05, "balance", Int),
];

const SOUND_FIELDS: &[Field] = &[
    f(0x01, "name", Text),
    f(0x03, "volume", Int),
    f(0x04, "tempo", Int),
    f(0x05, "balance", Int),
];

const INVENTORY_FIELDS: &[Field] = &[
    f(0x01, "party_size", Count),
    f(0x02, "party", Int16s),
    f(0x0B, "item_ids_size", Count),
    f(0x0C, "item_ids", Int16s),
    f(0x0D, "item_counts", UInt8s),
    f(0x0E, "item_usage", UInt8s),
    f(0x15, "gold", Int),
    f(0x17, "timer1_seconds", Int),
    f(0x18, "timer1_active", Bool),
    f(0x19, "timer1_visible", Bool),
    f(0x1A, "timer1_battle", Bool),
    f(0x1B, "timer2_seconds", Int),
    f(0x1C, "timer2_active", Bool),
    f(0x1D, "timer2_visible", Bool),
    f(0x1E, "timer2_battle", Bool),
    f(0x20, "battles", Int),
    f(0x21, "defeats", Int),
    f(0x22, "escapes", Int),
    f(0x23, "victories", Int),
    f(0x29, "turns", Int),
    f(0x2A, "steps", Int),
];

const ACTOR_FIELDS: &[Field] = &[
    f(0x01, "name", Text),
    f(0x02, "title", Text),
    f(0x0B, "sprite_name", Text),
    f(0x0C, "sprite_id", Int),
    f(0x0D, "transparency", Int),
    f(0x15, "face_name", Text),
    f(0x16, "face_id", Int),
    f(0x1F, "level", Int),
    f(0x20, "exp", Int),
    f(0x21, "hp_mod", Int),
    f(0x22, "sp_mod", Int),
    f(0x29, "attack_mod", Int),
    f(0x2A, "defense_mod", Int),
    f(0x2B, "spirit_mod", Int),
    f(0x2C, "agility_mod", Int),
    f(0x33, "skills_size", Count),
    f(0x34, "skills", Int16s),
    f(0x3D, "equipped", Int16s),
    f(0x47, "current_hp", Int),
    f(0x48, "current_sp", Int),
    f(0x50, "battle_commands", Int32s),
    f(0x51, "status_size", Count),
    f(0x52, "status", Int16s),
    f(0x53, "changed_battle_commands", Bool),
    f(0x5A, "class_id", Int),
    f(0x5B, "row", Int),
    f(0x5C, "two_weapon", Bool),
    f(0x5D, "lock_equipment", Bool),
    f(0x5E, "auto_battle", Bool),
    f(0x5F, "super_guard", Bool),
    f(0x60, "battler_animation", Int),
];

const TARGET_FIELDS: &[Field] = &[
    f(0x01, "map_id", Int),
    f(0x02, "map_x", Int),
    f(0x03, "map_y", Int),
    f(0x04, "switch_on", Bool),
    f(0x05, "switch_id", Int),
];

const PANORAMA_FIELDS: &[Field] = &[f(0x01, "pan_x", Int), f(0x02, "pan_y", Int)];

const DATABASE_FIELDS: &[Field] = &[
    f(0x0B, "actors", Array("Actor")),
    f(0x0C, "skills", Array("Skill")),
    f(0x0D, "items", Array("Item")),
    f(0x0E, "enemies", Array("Enemy")),
    f(0x0F, "troops", Array("Troop")),
    f(0x10, "terrains", Array("Terrain")),
    f(0x11, "attributes", Array("Attribute")),
    f(0x12, "states", Array("State")),
    f(0x13, "animations", Array("Animation")),
    f(0x14, "chipsets", Array("Chipset")),
    f(0x15, "terms", Struct("Terms")),
    f(0x16, "system", Struct("System")),
    f(0x17, "switches", Array("Switch")),
    f(0x18, "variables", Array("Variable")),
    f(0x19, "commonevents", Array("CommonEvent")),
    f(0x1A, "version", Int),
    f(0x1D, "battlecommands", Struct("BattleCommands")),
    f(0x1E, "classes", Array("Class")),
    f(0x20, "battleranimations", Array("BattlerAnimation")),
];

const DB_ACTOR_FIELDS: &[Field] = &[f(0x01, "name", Text), f(0x02, "title", Text)];

const DB_ITEM_FIELDS: &[Field] = &[
    f(0x01, "name", Text),
    f(0x02, "description", Text),
    f(0x03, "type", Int),
    f(0x05, "price", Int),
];

const NAME_ONLY: &[Field] = &[f(0x01, "name", Text)];

static TYPES: &[TypeDef] = &[
    t(SAVE, SAVE_FIELDS),
    t("SaveTitle", TITLE_FIELDS),
    t("SaveSystem", SYSTEM_FIELDS),
    t("Music", MUSIC_FIELDS),
    t("Sound", SOUND_FIELDS),
    t("SaveScreen", &[]),
    t("SavePicture", &[]),
    t("SavePartyLocation", &[]),
    t("SaveVehicleLocation", &[]),
    t("SaveActor", ACTOR_FIELDS),
    t("SaveInventory", INVENTORY_FIELDS),
    t("SaveTarget", TARGET_FIELDS),
    t("SaveMapInfo", &[]),
    t("SavePanorama", PANORAMA_FIELDS),
    t("SaveEventExecState", &[]),
    t("SaveCommonEvent", &[]),
    t(DATABASE, DATABASE_FIELDS),
    t("Actor", DB_ACTOR_FIELDS),
    t("Skill", &[]),
    t("Item", DB_ITEM_FIELDS),
    t("Enemy", &[]),
    t("Troop", &[]),
    t("Terrain", &[]),
    t("Attribute", &[]),
    t("State", &[]),
    t("Animation", &[]),
    t("Chipset", &[]),
    t("Terms", &[]),
    t("System", &[]),
    t("Switch", NAME_ONLY),
    t("Variable", NAME_ONLY),
    t("CommonEvent", &[]),
    t("BattleCommands", &[]),
    t("Class", &[]),
    t("BattlerAnimation", &[]),
];

/// The struct type named `name`.
pub(crate) fn type_def(name: &str) -> Option<&'static TypeDef> {
    TYPES.iter().find(|ty| ty.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_struct_and_array_kind_names_a_known_type_and_ids_are_unique() {
        for ty in TYPES {
            for (index, field) in ty.fields.iter().enumerate() {
                if let Struct(name) | Array(name) = field.kind {
                    assert!(
                        type_def(name).is_some(),
                        "{}.{} -> {name}",
                        ty.name,
                        field.name
                    );
                }
                assert!(field.id != 0, "{}.{}", ty.name, field.name);
                for other in ty.fields.iter().skip(index + 1) {
                    assert_ne!(field.id, other.id, "{}", ty.name);
                    assert_ne!(field.name, other.name, "{}", ty.name);
                }
            }
        }
    }

    #[test]
    fn keys_map_back_to_ids() {
        let system = type_def("SaveSystem").unwrap();
        assert_eq!(system.id_of("variables"), Some(0x22));
        assert_eq!(key_of(system, 0x22), "variables");
        assert_eq!(key_of(system, 0x16), "0x16");
        assert_eq!(system.id_of("0x16"), Some(0x16));
        assert_eq!(key_of(system, 0x1234), "0x1234");
        assert_eq!(system.id_of("0x1234"), Some(0x1234));
        // A named id written as hex, id 0 and junk are refused.
        assert_eq!(system.id_of("0x22"), None);
        assert_eq!(system.id_of("0x00"), None);
        assert_eq!(system.id_of("gold"), None);
    }
}
