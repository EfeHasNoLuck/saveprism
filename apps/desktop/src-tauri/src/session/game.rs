//! The game view of the open document.
//!
//! RPG Maker MV/MZ saves become game sections (gold, characters, items, weapons, armors,
//! variables, switches) with real names read from the game's `data` folder; XP, VX and VX Ace saves
//! become the same sections (see `rgss.rs`), and so do RPG Maker 2000 and 2003 saves (see
//! `lcf.rs`). SugarCube (Twine) saves become a story view (see `sugarcube.rs`). Ren'Py saves list
//! their store variables (see `renpy.rs`). Every other format becomes a flat "Values" list. Every
//! editable value carries its type and limits as a [`Field`], so the UI can say
//! "Whole number · 0 to 99,999,999 · was 10,000" and block out-of-range input.
//!
//! The view only reads the tree and the game's data files; nothing is executed.

use std::cell::{OnceCell, RefCell};
use std::collections::HashMap;
use std::fs;
use std::io::Read as _;
use std::path::{Path as FsPath, PathBuf};

use saveprism_core::{Node, NodeFlags, NodeId, OwnedValue, Path, SaveDoc, Value};
use serde::Serialize;

use super::sugarcube::SugarCubeView;
use super::{ApiError, Change, Session, app_codes, bad_path, child_count, value_text};

/// Largest game data file that is read for names.
const MAX_DATA_FILE: u64 = 32 * 1024 * 1024;
/// Most rows of the generic Values list.
pub(super) const MAX_VALUE_ROWS: usize = 10_000;
/// Stable keys of `_paramPlus[0..8]`; the UI translates them.
const BONUS_KEYS: [&str; 8] = [
    "maxHp",
    "maxMp",
    "attack",
    "defense",
    "magicAttack",
    "magicDefense",
    "agility",
    "luck",
];

/// The kind of an editable value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FieldKind {
    /// Whole number.
    Int,
    /// Decimal number.
    Float,
    /// On/off.
    Bool,
    /// Text.
    String,
}

/// One scalar value with its type and limits.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Field {
    /// Core path string, passed back to `edit_value`.
    pub path: String,
    /// Current value as text.
    pub value: String,
    /// Value when the file was opened (or last saved).
    pub original: String,
    /// Kind of the value.
    pub kind: FieldKind,
    /// The file format's own type name (Easy Save 3: `Int32`); `None` when the format has none.
    pub type_name: Option<String>,
    /// Lowest allowed number.
    pub min: Option<i64>,
    /// Highest allowed number.
    pub max: Option<i64>,
    /// English description such as "Whole number · 0 to 99,999,999" (logs and tests); the UI
    /// builds its own translated text from `kind`, `min` and `max`.
    pub hint: String,
    /// True when the value can be changed.
    pub editable: bool,
}

/// An item, weapon, armor, variable or switch.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Entry {
    /// Database id (variables and switches: the index).
    pub id: u32,
    /// Name from the game's data folder; `None` when unknown.
    pub name: Option<String>,
    /// The value (count, number, text or on/off).
    pub field: Field,
}

/// A stat bonus of a character.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Bonus {
    /// Stable key of the stat: "maxHp", "attack", ... (see `BONUS_KEYS`; XP and VX add "strength",
    /// "dexterity", "intelligence" and "spirit").
    pub key: String,
    /// The bonus value.
    pub field: Field,
}

/// A party member.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Actor {
    /// Actor id.
    pub id: u32,
    /// Name of the character.
    pub name: String,
    /// Level.
    pub level: Field,
    /// Current HP.
    pub hp: Field,
    /// Current MP.
    pub mp: Field,
    /// Experience of the current class.
    pub exp: Option<Field>,
    /// Stat bonuses (`_paramPlus`; XP and VX: `@maxhp_plus`, ...).
    pub bonuses: Vec<Bonus>,
}

/// The RPG Maker sections.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpgMakerView {
    /// "mv", "mz", "xp", "vx", "vxace", "2k" or "2k3".
    pub engine: String,
    /// The party's gold.
    pub gold: Field,
    /// Party members.
    pub actors: Vec<Actor>,
    /// Items in the inventory.
    pub items: Vec<Entry>,
    /// Weapons in the inventory.
    pub weapons: Vec<Entry>,
    /// Armors in the inventory.
    pub armors: Vec<Entry>,
    /// Game variables.
    pub variables: Vec<Entry>,
    /// Game switches.
    pub switches: Vec<Entry>,
    /// The data folder the names came from.
    pub names_folder: Option<String>,
}

/// One row of the generic Values list.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ValueRow {
    /// Entry name (ES3 key, JSON key or index).
    pub name: String,
    /// Path of the row's value (or group); use it with `get_children` to expand a group.
    pub path: String,
    /// The value; `None` for groups.
    pub field: Option<Field>,
    /// Number of children of a group.
    pub children: usize,
    /// Why the value is shown read-only (story view); `None` elsewhere.
    pub note: Option<RowNote>,
}

/// What a read-only row of the story view holds; the UI translates `kind`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NoteKind {
    /// The value was partly changed during play (a nested change of a later moment).
    Changed,
    /// A JavaScript `Map`.
    Map,
    /// A JavaScript `Set`.
    Set,
    /// A JavaScript `Date`.
    Date,
    /// A value the game rebuilds with its own code (a class, `undefined`, a function).
    Code,
    /// `null`, or an empty list or object.
    Empty,
}

/// Why a row cannot be edited in the list, with what the UI shows instead of a value.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RowNote {
    /// What the value is.
    pub kind: NoteKind,
    /// Entries of a `Map` or `Set`.
    pub count: Option<usize>,
    /// The text of a `Date` as the game wrote it.
    pub text: Option<String>,
}

/// What `game_view` returns.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum GameView {
    /// RPG Maker sections.
    Rpgmaker {
        /// The sections.
        view: Box<RpgMakerView>,
    },
    /// SugarCube (Twine) story: the save's facts and the story variables.
    Sugarcube {
        /// The story view.
        view: Box<SugarCubeView>,
    },
    /// Flat list of the top-level values.
    Values {
        /// The rows.
        rows: Vec<ValueRow>,
    },
}

/// Inclusive limits of a whole number; `None` means unlimited on that side.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Bounds {
    min: Option<i128>,
    max: Option<i128>,
}

pub(super) const GOLD: Bounds = Bounds::new(Some(0), Some(99_999_999));
pub(super) const COUNT: Bounds = Bounds::new(Some(0), Some(99));
pub(super) const LEVEL: Bounds = Bounds::new(Some(1), Some(99));
pub(super) const NON_NEGATIVE: Bounds = Bounds::new(Some(0), None);
pub(super) const UNLIMITED: Bounds = Bounds::new(None, None);

fn pick(a: Option<i128>, b: Option<i128>, choose: fn(i128, i128) -> i128) -> Option<i128> {
    match (a, b) {
        (Some(x), Some(y)) => Some(choose(x, y)),
        (x, None) => x,
        (None, y) => y,
    }
}

/// Largest whole number a JavaScript number holds exactly (`Number.MAX_SAFE_INTEGER`).
const JS_SAFE_INTEGER: i128 = 9_007_199_254_740_991;

/// Limit as sent to the UI: kept within +-(2^53-1) so JavaScript reads it exactly. The Rust side
/// keeps checking the real bound.
fn clamp_i64(value: i128) -> i64 {
    i64::try_from(value.clamp(-JS_SAFE_INTEGER, JS_SAFE_INTEGER)).unwrap_or(0)
}

/// `1234567` as `1,234,567`.
fn group_digits(value: i128) -> String {
    let digits = value.unsigned_abs().to_string();
    let len = digits.chars().count();
    let mut out = String::new();
    if value < 0 {
        out.push('-');
    }
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && len.saturating_sub(index) % 3 == 0 {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

impl Bounds {
    pub(super) const fn new(min: Option<i128>, max: Option<i128>) -> Self {
        Self { min, max }
    }

    fn of_range(low: i128, high: i128) -> Self {
        Self::new(Some(low), Some(high))
    }

    /// The limits both `self` and `other` allow.
    fn narrow(self, other: Bounds) -> Bounds {
        Bounds {
            min: pick(self.min, other.min, i128::max),
            max: pick(self.max, other.max, i128::min),
        }
    }

    pub(super) fn contains(self, value: i128) -> bool {
        self.min.is_none_or(|low| value >= low) && self.max.is_none_or(|high| value <= high)
    }

    /// Same as [`Bounds::contains`] for a decimal number.
    pub(super) fn contains_f64(self, value: f64) -> bool {
        // i128 -> f64 may round for huge limits; the limits that matter here are far below 2^53.
        self.min.is_none_or(|low| value >= low as f64)
            && self.max.is_none_or(|high| value <= high as f64)
    }

    /// "0 to 99,999,999", "0 or higher", "99 or lower"; empty when unlimited.
    fn range_text(self) -> String {
        match (self.min, self.max) {
            (Some(low), Some(high)) => {
                format!("{} to {}", group_digits(low), group_digits(high))
            }
            (Some(low), None) => format!("{} or higher", group_digits(low)),
            (None, Some(high)) => format!("{} or lower", group_digits(high)),
            (None, None) => String::new(),
        }
    }

    /// The advice of a range error: "Type a number from 0 to 99,999,999."
    fn advice(self) -> String {
        match (self.min, self.max) {
            (Some(low), Some(high)) => format!(
                "Type a number from {} to {}.",
                group_digits(low),
                group_digits(high)
            ),
            (Some(low), None) => format!("Type a number of {} or higher.", group_digits(low)),
            (None, Some(high)) => format!("Type a number of {} or lower.", group_digits(high)),
            (None, None) => "Type a smaller number.".to_string(),
        }
    }
}

/// The Easy Save 3 type name of a node (`System.Int32`), from its `ES3:` hint.
fn es3_name(node: &Node) -> Option<&str> {
    node.type_hint
        .as_ref()?
        .as_str()
        .strip_prefix("ES3:")
        .filter(|name| !name.is_empty())
}

/// Short type name of a known Easy Save 3 scalar type.
fn es3_label(name: &str) -> Option<&'static str> {
    Some(match name {
        "int" | "System.Int32" => "Int32",
        "long" | "System.Int64" => "Int64",
        "short" | "System.Int16" => "Int16",
        "byte" | "System.Byte" => "Byte",
        "sbyte" | "System.SByte" => "SByte",
        "uint" | "System.UInt32" => "UInt32",
        "ulong" | "System.UInt64" => "UInt64",
        "ushort" | "System.UInt16" => "UInt16",
        "float" | "System.Single" => "Single",
        "double" | "System.Double" => "Double",
        "bool" | "System.Boolean" => "Boolean",
        "string" | "System.String" => "String",
        _ => return None,
    })
}

fn es3_int_range(label: &str) -> Option<Bounds> {
    let (low, high) = match label {
        "Int32" => (i128::from(i32::MIN), i128::from(i32::MAX)),
        "Int64" => (i128::from(i64::MIN), i128::from(i64::MAX)),
        "Int16" => (i128::from(i16::MIN), i128::from(i16::MAX)),
        "SByte" => (i128::from(i8::MIN), i128::from(i8::MAX)),
        "Byte" => (0, i128::from(u8::MAX)),
        "UInt16" => (0, i128::from(u16::MAX)),
        "UInt32" => (0, i128::from(u32::MAX)),
        "UInt64" => (0, i128::from(u64::MAX)),
        _ => return None,
    };
    Some(Bounds::of_range(low, high))
}

/// The limits the type of a node sets (the width of an integer, the Easy Save 3 type).
/// An integer with no fixed width gets the `i64` range when `wide` is true, else no limit.
fn type_bounds(node: &Node, wide: bool) -> Bounds {
    match &node.value {
        Value::Int { width, .. } => {
            let fallback = wide.then_some((i128::from(i64::MIN), i128::from(i64::MAX)));
            let base = match width.range().or(fallback) {
                Some((low, high)) => Bounds::of_range(low, high),
                None => UNLIMITED,
            };
            match es3_name(node).and_then(es3_label).and_then(es3_int_range) {
                Some(es3) => base.narrow(es3),
                None => base,
            }
        }
        _ => UNLIMITED,
    }
}

fn scalar_kind(value: &Value) -> Option<FieldKind> {
    match value {
        Value::Int { .. } => Some(FieldKind::Int),
        Value::Float { .. } => Some(FieldKind::Float),
        Value::Bool(_) => Some(FieldKind::Bool),
        Value::Str(_) => Some(FieldKind::String),
        _ => None,
    }
}

fn hint_text(kind: FieldKind, bounds: Bounds) -> String {
    let base = match kind {
        FieldKind::Int => "Whole number",
        FieldKind::Float => "Decimal number",
        FieldKind::Bool => return "On/off".to_string(),
        FieldKind::String => return "Text".to_string(),
    };
    let range = bounds.range_text();
    if range.is_empty() {
        base.to_string()
    } else {
        format!("{base} \u{b7} {range}")
    }
}

/// The Easy Save 3 type of a node as a short name (`Int32`, `Vector3`).
fn type_name(node: &Node) -> Option<String> {
    let name = es3_name(node)?;
    Some(match es3_label(name) {
        Some(label) => label.to_string(),
        None => name.strip_prefix("System.").unwrap_or(name).to_string(),
    })
}

/// A node and its path.
pub(super) type At = (NodeId, Path);

/// Read access to the tree with everything a [`Field`] needs.
pub(super) struct Ctx<'a> {
    pub(super) doc: &'a SaveDoc,
    editable: bool,
    /// True for formats other than RPG Maker: whole numbers with no width get the `i64` range.
    wide: bool,
    pending: &'a [Change],
    /// Limits of every field built so far, by path.
    seen: RefCell<HashMap<String, Bounds>>,
    /// JsonEx objects by their `@c` id, built when the first `@r` reference is met.
    refs: OnceCell<HashMap<i128, At>>,
    /// Paths of the nodes Marshal links point at, built when the first link is followed (`rgss.rs`).
    pub(super) links: OnceCell<HashMap<NodeId, Path>>,
}

/// Names from the game's data folder.
#[derive(Default)]
pub(super) struct GameNames {
    pub(super) folder: Option<PathBuf>,
    pub(super) items: HashMap<u32, String>,
    pub(super) weapons: HashMap<u32, String>,
    pub(super) armors: HashMap<u32, String>,
    pub(super) variables: HashMap<u32, String>,
    pub(super) switches: HashMap<u32, String>,
}

/// The bytes of a game data file; `None` when it is missing, not a file or larger than
/// [`MAX_DATA_FILE`].
pub(super) fn read_data_file(path: &FsPath) -> Option<Vec<u8>> {
    let file = fs::File::open(path).ok()?;
    if !file.metadata().ok()?.is_file() {
        return None;
    }
    // Read one byte more than the limit: a longer file counts as "no names".
    let mut bytes = Vec::new();
    file.take(MAX_DATA_FILE + 1).read_to_end(&mut bytes).ok()?;
    if u64::try_from(bytes.len()).ok()? > MAX_DATA_FILE {
        return None;
    }
    Some(bytes)
}

fn read_json(path: &FsPath) -> Option<serde_json::Value> {
    let bytes = read_data_file(path)?;
    let bytes = bytes
        .strip_prefix(&[0xEF, 0xBB, 0xBF][..])
        .unwrap_or(&bytes);
    serde_json::from_slice(bytes).ok()
}

/// Names of a database file: an array of `{"id": n, "name": "..."}` (null entries are skipped).
fn id_names(file: &serde_json::Value) -> HashMap<u32, String> {
    let mut out = HashMap::new();
    if let Some(list) = file.as_array() {
        for entry in list {
            let id = entry
                .get("id")
                .and_then(serde_json::Value::as_u64)
                .and_then(|id| u32::try_from(id).ok());
            let name = entry
                .get("name")
                .and_then(serde_json::Value::as_str)
                .filter(|name| !name.is_empty());
            if let (Some(id), Some(name)) = (id, name) {
                out.insert(id, name.to_string());
            }
        }
    }
    out
}

/// Names of an array of strings, by index (`System.json` switches and variables).
fn index_names(list: Option<&serde_json::Value>) -> HashMap<u32, String> {
    let mut out = HashMap::new();
    if let Some(list) = list.and_then(serde_json::Value::as_array) {
        for (index, name) in list.iter().enumerate() {
            let name = name.as_str().filter(|name| !name.is_empty());
            if let (Some(name), Ok(index)) = (name, u32::try_from(index)) {
                out.insert(index, name.to_string());
            }
        }
    }
    out
}

/// Looks for the game's `data` folder next to the save's folder (`<save dir>/../data`, then
/// `<save dir>/../www/data`, then a copied `<save dir>/data`); the first folder with a readable
/// data file wins.
fn load_names(save_path: &FsPath) -> GameNames {
    // A relative save path would resolve against the process folder, not the game's.
    if !save_path.is_absolute() {
        return GameNames::default();
    }
    let Some(save_dir) = save_path.parent() else {
        return GameNames::default();
    };
    let Some(game_root) = save_dir.parent() else {
        return GameNames::default();
    };
    for folder in [
        game_root.join("data"),
        game_root.join("www").join("data"),
        save_dir.join("data"),
    ] {
        let mut names = GameNames::default();
        let mut found = false;
        if let Some(file) = read_json(&folder.join("Items.json")) {
            names.items = id_names(&file);
            found = true;
        }
        if let Some(file) = read_json(&folder.join("Weapons.json")) {
            names.weapons = id_names(&file);
            found = true;
        }
        if let Some(file) = read_json(&folder.join("Armors.json")) {
            names.armors = id_names(&file);
            found = true;
        }
        if let Some(file) = read_json(&folder.join("System.json")) {
            names.variables = index_names(file.get("variables"));
            names.switches = index_names(file.get("switches"));
            found = true;
        }
        if found {
            names.folder = Some(folder);
            return names;
        }
    }
    GameNames::default()
}

/// Every object that has an `@c` id, by that id (depth-first, one visit per node at most).
fn collect_ids(doc: &SaveDoc) -> HashMap<i128, At> {
    let mut out = HashMap::new();
    let mut stack: Vec<At> = vec![(doc.root(), Path::root())];
    let mut budget = doc.len();
    while let Some((id, path)) = stack.pop() {
        if budget == 0 {
            break;
        }
        budget -= 1;
        let Some(node) = doc.get(id) else {
            continue;
        };
        match &node.value {
            Value::Object(entries) => {
                for (key, child) in entries {
                    if key == "@c"
                        && let Some(Value::Int { v, .. }) = doc.get(*child).map(|n| &n.value)
                    {
                        out.entry(*v).or_insert_with(|| (id, path.clone()));
                    }
                    let mut child_path = path.clone();
                    child_path.push(key.as_str());
                    stack.push((*child, child_path));
                }
            }
            Value::Array(items) => {
                for (index, child) in items.iter().enumerate() {
                    let mut child_path = path.clone();
                    child_path.push(index.to_string());
                    stack.push((*child, child_path));
                }
            }
            _ => {}
        }
    }
    out
}

impl Ctx<'_> {
    /// The scalar at `at` as a field, with `extra` limits on top of the type's own.
    pub(super) fn field(&self, at: &(NodeId, Path), extra: Bounds) -> Option<Field> {
        let (id, path) = at;
        let node = self.doc.get(*id)?;
        let kind = scalar_kind(&node.value)?;
        let value = value_text(node)?;
        let path_text = path.to_string();
        let original = self
            .pending
            .iter()
            .find(|change| change.path == path_text)
            .map_or_else(|| value.clone(), |change| change.old.clone());
        let bounds = type_bounds(node, self.wide).narrow(extra);
        if let Ok(mut seen) = self.seen.try_borrow_mut() {
            seen.insert(path_text.clone(), bounds);
        }
        let numeric = matches!(kind, FieldKind::Int | FieldKind::Float);
        let editable = self.editable
            && !node.flags.contains(NodeFlags::READ_ONLY)
            && self.doc.resolve(path) == Some(*id);
        Some(Field {
            path: path_text,
            value,
            original,
            kind,
            type_name: type_name(node),
            min: bounds.min.filter(|_| numeric).map(clamp_i64),
            max: bounds.max.filter(|_| numeric).map(clamp_i64),
            hint: hint_text(kind, bounds),
            editable,
        })
    }

    /// The child `key` of the object at `at`.
    pub(super) fn entry(&self, at: &At, key: &str) -> Option<At> {
        match &self.doc.get(at.0)?.value {
            Value::Object(entries) => {
                entries
                    .iter()
                    .find(|(name, _)| name == key)
                    .map(|(_, child)| {
                        let mut path = at.1.clone();
                        path.push(key);
                        (*child, path)
                    })
            }
            _ => None,
        }
    }

    pub(super) fn int_at(&self, at: &At) -> Option<i128> {
        match &self.doc.get(at.0)?.value {
            Value::Int { v, .. } => Some(*v),
            _ => None,
        }
    }

    pub(super) fn str_at(&self, at: &At) -> Option<&str> {
        match &self.doc.get(at.0)?.value {
            Value::Str(text) => Some(text.as_str()),
            _ => None,
        }
    }

    /// The elements of a JSON array, or of a JsonEx wrapped array (`{"@c": 3, "@a": [...]}`).
    pub(super) fn items(&self, at: &At) -> Vec<(usize, At)> {
        let Some(node) = self.doc.get(at.0) else {
            return Vec::new();
        };
        match &node.value {
            Value::Array(ids) => ids
                .iter()
                .enumerate()
                .map(|(index, id)| {
                    let mut path = at.1.clone();
                    path.push(index.to_string());
                    (index, (*id, path))
                })
                .collect(),
            Value::Object(_) => match self.entry(at, "@a") {
                Some(inner)
                    if matches!(
                        self.doc.get(inner.0).map(|n| &n.value),
                        Some(Value::Array(_))
                    ) =>
                {
                    self.items(&inner)
                }
                _ => Vec::new(),
            },
            _ => Vec::new(),
        }
    }

    /// Follows a JsonEx reference (`{"@r": 3}`) to the object that has `@c` 3; anything else is
    /// returned as it is.
    fn deref(&self, at: At) -> At {
        let target = self
            .entry(&at, "@r")
            .and_then(|reference| self.int_at(&reference));
        match target {
            Some(wanted) => self
                .refs
                .get_or_init(|| collect_ids(self.doc))
                .get(&wanted)
                .cloned()
                .unwrap_or(at),
            None => at,
        }
    }

    /// `{"<id>": count}` maps of the party (items, weapons, armors).
    fn counts(&self, party: &At, key: &str, names: &HashMap<u32, String>) -> Vec<Entry> {
        let Some(map) = self.entry(party, key) else {
            return Vec::new();
        };
        let Some(Value::Object(entries)) = self.doc.get(map.0).map(|node| &node.value) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for (name, _) in entries {
            if out.len() >= MAX_VALUE_ROWS {
                break;
            }
            let Ok(id) = name.parse::<u32>() else {
                continue;
            };
            let field = self.entry(&map, name).and_then(|at| self.field(&at, COUNT));
            if let Some(field) = field {
                out.push(Entry {
                    id,
                    name: names.get(&id).cloned(),
                    field,
                });
            }
        }
        out.sort_by_key(|entry| entry.id);
        out
    }

    /// Variables or switches: `<root>.<key>[index]` (`_data` in MV/MZ, `@data` in XP, VX and VX
    /// Ace). Index 0 is never used. An entry with no name and an empty value (0, "", off) is unused
    /// and left out, unless it was changed to that value since the file was opened (so an edit to
    /// off does not make the entry disappear).
    pub(super) fn indexed(&self, root: &At, key: &str, names: &HashMap<u32, String>) -> Vec<Entry> {
        let Some(data) = self.entry(root, key) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for (index, at) in self.items(&data) {
            if out.len() >= MAX_VALUE_ROWS {
                break;
            }
            let Ok(id) = u32::try_from(index) else {
                continue;
            };
            if id == 0 {
                continue;
            }
            let Some(field) = self.field(&at, UNLIMITED) else {
                continue;
            };
            let named = names.contains_key(&id);
            let empty = matches!(field.value.as_str(), "0" | "" | "false" | "0.0");
            if !named && empty && field.value == field.original {
                continue;
            }
            out.push(Entry {
                id,
                name: names.get(&id).cloned(),
                field,
            });
        }
        out
    }

    fn actor(&self, id: u32, at: &At) -> Option<Actor> {
        let level = self.field(&self.entry(at, "_level")?, LEVEL)?;
        let hp = self.field(&self.entry(at, "_hp")?, NON_NEGATIVE)?;
        let mp = self.field(&self.entry(at, "_mp")?, NON_NEGATIVE)?;
        let name = self
            .entry(at, "_name")
            .and_then(|name| self.str_at(&name).map(str::to_string))
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| format!("Actor #{id}"));

        // Experience is kept per class: take the entry of the current class, else the first one.
        let exp = self.entry(at, "_exp").and_then(|map| {
            let class_key = self
                .entry(at, "_classId")
                .and_then(|class| self.int_at(&class))
                .map(|class| class.to_string());
            let keys: Vec<String> = match self.doc.get(map.0).map(|node| &node.value) {
                Some(Value::Object(entries)) => entries
                    .iter()
                    .map(|(key, _)| key.clone())
                    .filter(|key| !key.starts_with('@'))
                    .collect(),
                _ => Vec::new(),
            };
            let key = class_key
                .filter(|wanted| keys.contains(wanted))
                .or_else(|| keys.first().cloned())?;
            self.field(&self.entry(&map, &key)?, NON_NEGATIVE)
        });

        let bonuses = self
            .entry(at, "_paramPlus")
            .map(|list| self.bonus_list(&list))
            .unwrap_or_default();

        Some(Actor {
            id,
            name,
            level,
            hp,
            mp,
            exp,
            bonuses,
        })
    }

    /// Stat bonuses kept as a list in `BONUS_KEYS` order (MV/MZ `_paramPlus`, VX Ace `@param_plus`).
    pub(super) fn bonus_list(&self, list: &At) -> Vec<Bonus> {
        self.items(list)
            .into_iter()
            .filter_map(|(index, item)| {
                let key = BONUS_KEYS.get(index)?;
                Some(Bonus {
                    key: (*key).to_string(),
                    field: self.field(&item, UNLIMITED)?,
                })
            })
            .collect()
    }

    /// The party members: ids from `party._actors`, data from `actors._data[id]`.
    fn actors(&self, root: &At, party: &At) -> Vec<Actor> {
        let Some(list) = self.entry(party, "_actors") else {
            return Vec::new();
        };
        let Some(data) = self
            .entry(root, "actors")
            .and_then(|actors| self.entry(&actors, "_data"))
        else {
            return Vec::new();
        };
        let by_index: HashMap<usize, At> = self.items(&data).into_iter().collect();
        let mut out = Vec::new();
        for (_, id_at) in self.items(&list) {
            let Some(id) = self.int_at(&id_at) else {
                continue;
            };
            let (Ok(id), Ok(index)) = (u32::try_from(id), usize::try_from(id)) else {
                continue;
            };
            let Some(at) = by_index.get(&index) else {
                continue;
            };
            let at = self.deref(at.clone());
            if let Some(actor) = self.actor(id, &at) {
                out.push(actor);
            }
        }
        out
    }

    /// The RPG Maker sections, or `None` when the tree has no `party._gold`.
    fn rpg_view(&self, engine: &str, names: &GameNames) -> Option<RpgMakerView> {
        let root: At = (self.doc.root(), Path::root());
        let party = self.entry(&root, "party")?;
        let gold = self.field(&self.entry(&party, "_gold")?, GOLD)?;
        let section = |key: &str, map: &HashMap<u32, String>| {
            self.entry(&root, key)
                .map(|at| self.indexed(&at, "_data", map))
                .unwrap_or_default()
        };
        Some(RpgMakerView {
            engine: engine.to_string(),
            gold,
            actors: self.actors(&root, &party),
            items: self.counts(&party, "_items", &names.items),
            weapons: self.counts(&party, "_weapons", &names.weapons),
            armors: self.counts(&party, "_armors", &names.armors),
            variables: section("variables", &names.variables),
            switches: section("switches", &names.switches),
            names_folder: names
                .folder
                .as_ref()
                .map(|folder| folder.to_string_lossy().into_owned()),
        })
    }

    /// One row of the Values list for the entry `key` at `id`.
    fn value_row(&self, name: String, id: NodeId, path: Path, easy_save: bool) -> ValueRow {
        // Easy Save 3 keeps the value of an entry in `{"__type": ..., "value": ...}`.
        let mut at: At = (id, path);
        if easy_save {
            let has_type = self.entry(&at, "__type").is_some();
            if has_type && let Some(inner) = self.entry(&at, "value") {
                at = inner;
            }
        }
        let children = self
            .doc
            .get(at.0)
            .map_or(0, |node| child_count(&node.value));
        ValueRow {
            name,
            path: at.1.to_string(),
            field: self.field(&at, UNLIMITED),
            children,
            note: None,
        }
    }

    pub(super) fn value_rows(&self, easy_save: bool) -> Vec<ValueRow> {
        let root = self.doc.root();
        let Some(node) = self.doc.get(root) else {
            return Vec::new();
        };
        let entries: Vec<(String, NodeId)> = match &node.value {
            Value::Object(entries) => entries.clone(),
            Value::Array(items) => items
                .iter()
                .enumerate()
                .map(|(index, id)| (index.to_string(), *id))
                .collect(),
            _ => vec![(String::new(), root)],
        };
        let is_root_scalar = !matches!(node.value, Value::Object(_) | Value::Array(_));
        entries
            .into_iter()
            .take(MAX_VALUE_ROWS)
            .map(|(name, id)| {
                let mut path = Path::root();
                if !is_root_scalar {
                    path.push(name.as_str());
                }
                self.value_row(name, id, path, easy_save)
            })
            .collect()
    }
}

impl Session {
    pub(super) fn game_ctx(&self) -> Ctx<'_> {
        Ctx {
            doc: &self.doc,
            editable: self.editable,
            wide: self.rpg_engine().is_none(),
            pending: &self.pending,
            seen: RefCell::default(),
            refs: OnceCell::new(),
            links: OnceCell::new(),
        }
    }

    /// "mv" or "mz" when the file was unpacked by an RPG Maker layer.
    pub(super) fn rpg_engine(&self) -> Option<&'static str> {
        let has = |kind: &str| self.recipe.steps.iter().any(|step| step.kind == kind);
        if has("encoding.js-binary-string") {
            Some("mz")
        } else if has("compression.lz-string") {
            Some("mv")
        } else {
            None
        }
    }

    /// The limits of every field of the RPG Maker view (MV/MZ, XP/VX/VX Ace or 2000/2003), by path
    /// (empty for other formats).
    pub(super) fn rpg_bounds(&self) -> HashMap<String, Bounds> {
        let ctx = self.game_ctx();
        if let Some(engine) = self.rpg_engine() {
            let _ = ctx.rpg_view(engine, &GameNames::default());
        } else if let Some(engine) = self.rgss_engine(&ctx) {
            let _ = ctx.rgss_view(engine, &GameNames::default());
        } else if self.is_lcf_save() {
            self.lcf_bounds(&ctx);
        } else {
            return HashMap::new();
        }
        ctx.seen.into_inner()
    }

    /// The SugarCube story view, the RPG Maker sections for MV/MZ, XP/VX/VX Ace and 2000/2003
    /// saves, else the flat Values list.
    pub fn game_view(&self) -> Result<GameView, ApiError> {
        let ctx = self.game_ctx();
        if let Some(view) = ctx.sugarcube_view() {
            return Ok(GameView::Sugarcube {
                view: Box::new(view),
            });
        }
        if let Some(rows) = self.naninovel_view(&ctx) {
            return Ok(GameView::Values { rows });
        }
        if let Some(rows) = self.renpy_view(&ctx) {
            return Ok(GameView::Values { rows });
        }
        if let Some(engine) = self.rpg_engine() {
            // A pasted code has no game folder next to it.
            let names = if self.pasted {
                GameNames::default()
            } else {
                load_names(&self.path)
            };
            if let Some(view) = ctx.rpg_view(engine, &names) {
                return Ok(GameView::Rpgmaker {
                    view: Box::new(view),
                });
            }
        }
        if let Some(view) = self.rgss_view(&ctx) {
            return Ok(GameView::Rpgmaker {
                view: Box::new(view),
            });
        }
        if let Some(view) = self.lcf_view(&ctx) {
            return Ok(GameView::Rpgmaker {
                view: Box::new(view),
            });
        }
        let easy_save = self
            .recipe
            .steps
            .iter()
            .any(|step| step.kind == "format.es3");
        Ok(GameView::Values {
            rows: ctx.value_rows(easy_save),
        })
    }

    /// Type, limits and original value of the scalar at `path`.
    pub fn field_info(&self, path: &str) -> Result<Field, ApiError> {
        let parsed = Path::parse(path)?;
        let id = self.doc.resolve(&parsed).ok_or_else(|| bad_path(path))?;
        let extra = self
            .rpg_bounds()
            .get(&parsed.to_string())
            .copied()
            .unwrap_or(UNLIMITED);
        self.game_ctx().field(&(id, parsed), extra).ok_or_else(|| {
            ApiError::new(
                app_codes::NOT_EDITABLE_NODE,
                "only numbers, booleans and text have a type and limits",
                "Open the container and pick one of its values.",
            )
        })
    }

    /// [`Session::field_info`] for many paths with one build of the RPG Maker limits. Paths that
    /// are not values with a type are left out.
    pub(super) fn fields_info(&self, paths: &[String]) -> Vec<Field> {
        let bounds = self.rpg_bounds();
        let ctx = self.game_ctx();
        paths
            .iter()
            .filter_map(|text| {
                let parsed = Path::parse(text).ok()?;
                let id = self.doc.resolve(&parsed)?;
                let extra = bounds
                    .get(&parsed.to_string())
                    .copied()
                    .unwrap_or(UNLIMITED);
                ctx.field(&(id, parsed), extra)
            })
            .collect()
    }

    /// Refuses a whole number outside the limits of the value at `path` (RPG Maker limits, the
    /// Easy Save 3 type or the integer width).
    pub(super) fn check_bounds(
        &self,
        path: &Path,
        node: &Node,
        value: &OwnedValue,
    ) -> Result<(), ApiError> {
        check_bounds_in(&self.rpg_bounds(), path, node, value)
    }
}

/// [`Session::check_bounds`] with the RPG Maker limits already built (one build for many values).
pub(super) fn check_bounds_in(
    rpg_bounds: &HashMap<String, Bounds>,
    path: &Path,
    node: &Node,
    value: &OwnedValue,
) -> Result<(), ApiError> {
    let (inside, shown) = match value {
        OwnedValue::Int(number) => (None, number.to_string()),
        OwnedValue::Float(number) => (Some(*number), number.to_string()),
        _ => return Ok(()),
    };
    let bounds = rpg_bounds
        .get(&path.to_string())
        .copied()
        .unwrap_or_else(|| type_bounds(node, true));
    let fits = match (value, inside) {
        (OwnedValue::Int(number), _) => bounds.contains(*number),
        (_, Some(number)) => bounds.contains_f64(number),
        _ => true,
    };
    if fits {
        return Ok(());
    }
    Err(ApiError::new(
        app_codes::OUT_OF_RANGE,
        format!(
            "{shown} is outside the range of this value ({})",
            bounds.range_text()
        ),
        bounds.advice(),
    ))
}
