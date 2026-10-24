//! RPG Maker XP, VX and VX Ace saves (RGSS, Ruby Marshal) as game sections.
//!
//! The Marshal codec reads a save as an `Array` of its dumps (see `saveprism_codecs::marshal`). VX
//! Ace writes a header hash and a contents hash (`{system:, switches:, variables:, actors:, party:,
//! ...}`); XP and VX write one dump per game object (`Game_System`, `Game_Switches`, ...,
//! `Game_Actors`, `Game_Party`, ...), found here by class name. The sections are the ones of MV/MZ
//! ([`RpgMakerView`]): gold, items, weapons and armors from `Game_Party`, the party members from
//! `Game_Actors`, variables and switches from their `@data` arrays. Names come from the game's
//! `Data` folder (`Items.rxdata`, ...), read with the same codec as a data tree; nothing is executed.
//!
//! XP keeps whole `Game_Actor` copies in the party, but the game's load code rebuilds the party from
//! `$game_actors`, so the actor fields always point into `Game_Actors`. An actor slot that is a
//! Marshal link (`Ref`) is followed to the node it points at, and the fields get that node's path.
//!
//! New inventory entries go to the end of the party's `@items`, `@weapons` and `@armors` hashes
//! (the codec lets integer-keyed hashes grow). A count of 0 is harmless: XP and VX keep such
//! entries themselves and list only counts above 0; VX Ace deletes an entry when a count drops to 0
//! in play, and lists an entry it loads with 0 as a greyed "0".

use std::collections::HashMap;
use std::path::Path as FsPath;

use saveprism_codecs::marshal::{RubyMarshalCodec, is_growable_hash, ruby_class};
use saveprism_core::{Codec, DecodeCtx, Limits, NodeId, Path, SaveDoc, Value};

use super::Session;
use super::game::{
    Actor, AddLists, At, Bonus, Bounds, COUNT, Ctx, Entry, GOLD, GameNames, LEVEL,
    MAX_VALUE_ROWS, NON_NEGATIVE, RpgMakerView, UNLIMITED, read_data_file,
};

/// Gold limit of XP and VX (VX Ace has the MV/MZ limit).
const GOLD_XP_VX: Bounds = Bounds::new(Some(0), Some(9_999_999));
/// XP stat bonuses: instance variable and stable key (the UI translates the key).
const XP_BONUSES: [(&str, &str); 6] = [
    ("@maxhp_plus", "maxHp"),
    ("@maxsp_plus", "maxMp"),
    ("@str_plus", "strength"),
    ("@dex_plus", "dexterity"),
    ("@agi_plus", "agility"),
    ("@int_plus", "intelligence"),
];
/// VX stat bonuses: instance variable and stable key.
const VX_BONUSES: [(&str, &str); 6] = [
    ("@maxhp_plus", "maxHp"),
    ("@maxmp_plus", "maxMp"),
    ("@atk_plus", "attack"),
    ("@def_plus", "defense"),
    ("@spi_plus", "spirit"),
    ("@agi_plus", "agility"),
];

/// The RPG Maker version that wrote the save.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Engine {
    /// RPG Maker XP (RGSS1, `.rxdata`).
    Xp,
    /// RPG Maker VX (RGSS2, `.rvdata`).
    Vx,
    /// RPG Maker VX Ace (RGSS3, `.rvdata2`).
    VxAce,
}

impl Engine {
    fn id(self) -> &'static str {
        match self {
            Engine::Xp => "xp",
            Engine::Vx => "vx",
            Engine::VxAce => "vxace",
        }
    }

    /// Extension of the game's data files.
    pub(super) fn extension(self) -> &'static str {
        match self {
            Engine::Xp => "rxdata",
            Engine::Vx => "rvdata",
            Engine::VxAce => "rvdata2",
        }
    }

    fn gold(self) -> Bounds {
        match self {
            Engine::VxAce => GOLD,
            Engine::Xp | Engine::Vx => GOLD_XP_VX,
        }
    }
}

/// The game objects the view reads.
#[derive(Default)]
struct Objects {
    party: Option<At>,
    actors: Option<At>,
    variables: Option<At>,
    switches: Option<At>,
}

/// The path of every node below the root, through objects, arrays and map values (not through
/// links, so every node has one path). One visit per node at most.
fn node_paths(doc: &SaveDoc) -> HashMap<NodeId, Path> {
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
        let mut push = |child: NodeId, segment: String| {
            let mut child_path = path.clone();
            child_path.push(segment);
            stack.push((child, child_path));
        };
        match &node.value {
            Value::Object(entries) => {
                for (key, child) in entries {
                    push(*child, key.clone());
                }
            }
            Value::Array(items) => {
                for (index, child) in items.iter().enumerate() {
                    push(*child, index.to_string());
                }
            }
            Value::Map(entries) => {
                for (index, (_, value)) in entries.iter().enumerate() {
                    push(*value, index.to_string());
                }
            }
            _ => {}
        }
        out.entry(id).or_insert(path);
    }
    out
}

impl Ctx<'_> {
    /// Follows a Marshal link (`Ref`) to the node it points at, with that node's own path; anything
    /// else is returned as it is.
    fn follow(&self, at: At) -> At {
        let Some(Value::Ref(target)) = self.doc.get(at.0).map(|node| &node.value) else {
            return at;
        };
        match self.links.get_or_init(|| node_paths(self.doc)).get(target) {
            Some(path) => (*target, path.clone()),
            None => at,
        }
    }

    /// The dumps of the file (the elements of the root array).
    fn dumps(&self) -> Vec<At> {
        let root: At = (self.doc.root(), Path::root());
        self.items(&root).into_iter().map(|(_, at)| at).collect()
    }

    /// The VX Ace contents hash: the dump with `party` and `actors`.
    fn contents(&self) -> Option<At> {
        self.dumps().into_iter().find(|dump| {
            self.entry(dump, "party").is_some() && self.entry(dump, "actors").is_some()
        })
    }

    /// The first dump of the Ruby class `class` (XP and VX).
    fn dump_of(&self, class: &str) -> Option<At> {
        self.dumps()
            .into_iter()
            .find(|dump| self.doc.get(dump.0).and_then(ruby_class) == Some(class))
    }

    /// VX Ace when a dump is the contents hash, else XP when an actor has `@sp`, else VX.
    pub(super) fn rgss_engine(&self) -> Engine {
        if self.contents().is_some() {
            return Engine::VxAce;
        }
        let has_sp = self
            .dump_of("Game_Actors")
            .and_then(|actors| self.entry(&actors, "@data"))
            .is_some_and(|data| {
                self.items(&data)
                    .into_iter()
                    .any(|(_, slot)| self.entry(&self.follow(slot), "@sp").is_some())
            });
        if has_sp { Engine::Xp } else { Engine::Vx }
    }

    fn objects(&self, engine: Engine) -> Objects {
        match engine {
            Engine::VxAce => {
                let Some(contents) = self.contents() else {
                    return Objects::default();
                };
                let get = |key: &str| self.entry(&contents, key).map(|at| self.follow(at));
                Objects {
                    party: get("party"),
                    actors: get("actors"),
                    variables: get("variables"),
                    switches: get("switches"),
                }
            }
            Engine::Xp | Engine::Vx => Objects {
                party: self.dump_of("Game_Party"),
                actors: self.dump_of("Game_Actors"),
                variables: self.dump_of("Game_Variables"),
                switches: self.dump_of("Game_Switches"),
            },
        }
    }

    /// The pairs of a Ruby hash read as a `Map`: the key when it is an integer, and the value (its
    /// path segment is the pair's index). Empty for anything else, such as an empty hash.
    fn int_pairs(&self, at: &At) -> Vec<(Option<i128>, At)> {
        let Some(Value::Map(entries)) = self.doc.get(at.0).map(|node| &node.value) else {
            return Vec::new();
        };
        entries
            .iter()
            .enumerate()
            .map(|(index, (key, value))| {
                let key = match self.doc.get(*key).map(|node| &node.value) {
                    Some(Value::Int { v, .. }) => Some(*v),
                    _ => None,
                };
                let mut path = at.1.clone();
                path.push(index.to_string());
                (key, (*value, path))
            })
            .collect()
    }

    /// Text of a string; a string that is not UTF-8 (read as bytes) is shown lossily.
    fn text_at(&self, at: &At) -> Option<String> {
        match &self.doc.get(at.0)?.value {
            Value::Str(text) => Some(text.clone()),
            Value::Bytes(bytes) => Some(String::from_utf8_lossy(bytes).into_owned()),
            _ => None,
        }
    }

    /// `{id => count}` hashes of the party (`@items`, `@weapons`, `@armors`).
    fn rgss_counts(&self, party: &At, key: &str, names: &HashMap<u32, String>) -> Vec<Entry> {
        let Some(map) = self.entry(party, key) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for (id, at) in self.int_pairs(&map) {
            if out.len() >= MAX_VALUE_ROWS {
                break;
            }
            let Some(id) = id.and_then(|id| u32::try_from(id).ok()) else {
                continue;
            };
            if let Some(field) = self.field(&at, COUNT) {
                // Entries that were there stay: removing one would renumber the ones after it.
                out.push(Entry {
                    id,
                    name: names.get(&id).cloned(),
                    field,
                    removable: false,
                });
            }
        }
        out.sort_by_key(|entry| entry.id);
        out
    }

    /// The party (`Game_Party`) of the save.
    pub(super) fn rgss_party(&self, engine: Engine) -> Option<At> {
        self.objects(engine).party
    }

    /// True when the party hash `key` takes new entries.
    fn takes_entries(&self, party: &At, key: &str) -> bool {
        self.editable
            && self
                .entry(party, key)
                .and_then(|at| self.doc.get(at.0))
                .is_some_and(is_growable_hash)
    }

    fn rgss_actor(&self, engine: Engine, id: u32, at: &At) -> Option<Actor> {
        let mp_key = if engine == Engine::Xp { "@sp" } else { "@mp" };
        let level = self.field(&self.entry(at, "@level")?, LEVEL)?;
        let hp = self.field(&self.entry(at, "@hp")?, NON_NEGATIVE)?;
        let mp = self.field(&self.entry(at, mp_key)?, NON_NEGATIVE)?;
        let name = self
            .entry(at, "@name")
            .and_then(|name| self.text_at(&name))
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| format!("Actor #{id}"));

        // XP and VX keep one number; VX Ace keeps `{class_id => exp}`: take the entry of the current
        // class, else the first one.
        let exp = self.entry(at, "@exp").and_then(|exp| {
            let pairs = self.int_pairs(&exp);
            if pairs.is_empty() {
                return self.field(&exp, NON_NEGATIVE);
            }
            let class = self
                .entry(at, "@class_id")
                .and_then(|class| self.int_at(&class));
            let (_, chosen) = class
                .and_then(|class| pairs.iter().find(|(key, _)| *key == Some(class)))
                .or_else(|| pairs.first())?;
            self.field(chosen, NON_NEGATIVE)
        });

        let bonuses = match engine {
            Engine::VxAce => self
                .entry(at, "@param_plus")
                .map(|list| self.bonus_list(&list))
                .unwrap_or_default(),
            Engine::Xp | Engine::Vx => {
                let table = if engine == Engine::Xp {
                    &XP_BONUSES
                } else {
                    &VX_BONUSES
                };
                table
                    .iter()
                    .filter_map(|(ivar, key)| {
                        Some(Bonus {
                            key: (*key).to_string(),
                            field: self.field(&self.entry(at, ivar)?, UNLIMITED)?,
                        })
                    })
                    .collect()
            }
        };

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

    /// The party members. VX and VX Ace keep actor ids in `@actors`; XP keeps `Game_Actor` copies
    /// (or links to them), of which only `@actor_id` is read. The data always comes from
    /// `Game_Actors` `@data[id]`.
    fn rgss_actors(&self, engine: Engine, party: &At, actors: Option<&At>) -> Vec<Actor> {
        let Some(list) = self.entry(party, "@actors") else {
            return Vec::new();
        };
        let Some(data) = actors.and_then(|actors| self.entry(actors, "@data")) else {
            return Vec::new();
        };
        let by_index: HashMap<usize, At> = self.items(&data).into_iter().collect();
        let mut out = Vec::new();
        for (_, member) in self.items(&list) {
            let member = self.follow(member);
            let id = self.int_at(&member).or_else(|| {
                self.entry(&member, "@actor_id")
                    .and_then(|id| self.int_at(&id))
            });
            let Some(id) = id else {
                continue;
            };
            let (Ok(id), Ok(index)) = (u32::try_from(id), usize::try_from(id)) else {
                continue;
            };
            let Some(slot) = by_index.get(&index) else {
                continue;
            };
            if let Some(actor) = self.rgss_actor(engine, id, &self.follow(slot.clone())) {
                out.push(actor);
            }
        }
        out
    }

    /// The RPG Maker sections of an XP, VX or VX Ace save, or `None` when there is no
    /// `Game_Party` with `@gold`.
    pub(super) fn rgss_view(&self, engine: Engine, names: &GameNames) -> Option<RpgMakerView> {
        let objects = self.objects(engine);
        let party = objects.party?;
        let gold = self.field(&self.entry(&party, "@gold")?, engine.gold())?;
        let section = |at: Option<At>, names: &HashMap<u32, String>| {
            at.map(|at| self.indexed(&at, "@data", names))
                .unwrap_or_default()
        };
        Some(RpgMakerView {
            engine: engine.id().to_string(),
            gold,
            actors: self.rgss_actors(engine, &party, objects.actors.as_ref()),
            items: self.rgss_counts(&party, "@items", &names.items),
            weapons: self.rgss_counts(&party, "@weapons", &names.weapons),
            armors: self.rgss_counts(&party, "@armors", &names.armors),
            variables: section(objects.variables, &names.variables),
            switches: section(objects.switches, &names.switches),
            names_folder: names
                .folder
                .as_ref()
                .map(|folder| folder.to_string_lossy().into_owned()),
            catalog: names.catalog(),
            can_add: AddLists {
                items: self.takes_entries(&party, "@items"),
                weapons: self.takes_entries(&party, "@weapons"),
                armors: self.takes_entries(&party, "@armors"),
            },
        })
    }
}

/// A game data file read with the Marshal codec as a data tree; `None` when it is missing, too
/// large or not Marshal data.
fn read_data(path: &FsPath) -> Option<SaveDoc> {
    let bytes = read_data_file(path)?;
    let limits = Limits::default();
    let mut ctx = DecodeCtx::new(&limits);
    RubyMarshalCodec
        .decode(&bytes, &mut ctx)
        .ok()
        .map(|decoded| decoded.doc)
}

/// The node itself, or the node a link points at.
fn target(doc: &SaveDoc, id: NodeId) -> NodeId {
    match doc.get(id).map(|node| &node.value) {
        Some(Value::Ref(target)) => *target,
        _ => id,
    }
}

/// The first dump of a data file.
fn first_dump(doc: &SaveDoc) -> Option<NodeId> {
    match &doc.get(doc.root())?.value {
        Value::Array(dumps) => dumps.first().map(|id| target(doc, *id)),
        _ => None,
    }
}

/// The instance variable `name` of the object `id`.
fn ivar(doc: &SaveDoc, id: NodeId, name: &str) -> Option<NodeId> {
    match &doc.get(target(doc, id))?.value {
        Value::Object(entries) => entries
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, child)| target(doc, *child)),
        _ => None,
    }
}

/// A non-empty name: a string, or bytes read lossily.
fn name_text(doc: &SaveDoc, id: NodeId) -> Option<String> {
    let text = match &doc.get(id)?.value {
        Value::Str(text) => text.clone(),
        Value::Bytes(bytes) => String::from_utf8_lossy(bytes).into_owned(),
        _ => return None,
    };
    (!text.is_empty()).then_some(text)
}

/// Names of `Items`, `Weapons` or `Armors`: an array of objects (or nil) with `@id` and `@name`.
fn id_names(doc: &SaveDoc) -> HashMap<u32, String> {
    let mut out = HashMap::new();
    let list = first_dump(doc).and_then(|id| doc.get(id));
    if let Some(Value::Array(entries)) = list.map(|node| &node.value) {
        for entry in entries {
            let id = ivar(doc, *entry, "@id").and_then(|id| match &doc.get(id)?.value {
                Value::Int { v, .. } => u32::try_from(*v).ok(),
                _ => None,
            });
            let name = ivar(doc, *entry, "@name").and_then(|name| name_text(doc, name));
            if let (Some(id), Some(name)) = (id, name) {
                out.insert(id, name);
            }
        }
    }
    out
}

/// Names by index of an array of strings of `RPG::System` (`@switches`, `@variables`).
fn index_names(doc: &SaveDoc, key: &str) -> HashMap<u32, String> {
    let mut out = HashMap::new();
    let list = first_dump(doc)
        .and_then(|system| ivar(doc, system, key))
        .and_then(|id| doc.get(id));
    if let Some(Value::Array(entries)) = list.map(|node| &node.value) {
        for (index, entry) in entries.iter().enumerate() {
            let name = name_text(doc, target(doc, *entry));
            if let (Some(name), Ok(index)) = (name, u32::try_from(index)) {
                out.insert(index, name);
            }
        }
    }
    out
}

/// The names in the data folder `folder` (`Items.<ext>`, `Weapons.<ext>`, `Armors.<ext>`,
/// `System.<ext>` of the engine), or `None` when it has none of them readable.
fn rgss_names_in(folder: &FsPath, engine: Engine) -> Option<GameNames> {
    let extension = engine.extension();
    let read = |name: &str| read_data(&folder.join(format!("{name}.{extension}")));
    let mut names = GameNames::default();
    let mut found = false;
    if let Some(doc) = read("Items") {
        names.items = id_names(&doc);
        found = true;
    }
    if let Some(doc) = read("Weapons") {
        names.weapons = id_names(&doc);
        found = true;
    }
    if let Some(doc) = read("Armors") {
        names.armors = id_names(&doc);
        found = true;
    }
    if let Some(doc) = read("System") {
        names.variables = index_names(&doc, "@variables");
        names.switches = index_names(&doc, "@switches");
        found = true;
    }
    found.then(|| {
        names.folder = Some(folder.to_path_buf());
        names
    })
}

/// Looks for the game's `Data` folder next to the save (`<save dir>/Data`: these engines save next
/// to `Game.exe`), then one folder up (games that save in a subfolder); the first folder with a
/// readable data file wins. Encrypted archives (`Game.rgssad`, `.rgss2a`, `.rgss3a`) are not read.
fn load_names(save_path: &FsPath, engine: Engine) -> GameNames {
    // A relative save path would resolve against the process folder, not the game's.
    if !save_path.is_absolute() {
        return GameNames::default();
    }
    let Some(save_dir) = save_path.parent() else {
        return GameNames::default();
    };
    let mut folders = vec![save_dir.join("Data")];
    if let Some(game_root) = save_dir.parent() {
        folders.push(game_root.join("Data"));
    }
    folders
        .iter()
        .find_map(|folder| rgss_names_in(folder, engine))
        .unwrap_or_default()
}

/// The names of a folder the player picked: the `Data` folder itself, or the game folder (its
/// `Data`).
pub(super) fn rgss_names_picked(folder: &FsPath, engine: Engine) -> Option<GameNames> {
    [
        folder.to_path_buf(),
        folder.join("Data"),
        folder.join("data"),
    ]
    .iter()
    .find_map(|candidate| rgss_names_in(candidate, engine))
}

/// What to pick when a folder has no data files of the engine.
pub(super) fn data_hint(engine: Engine) -> String {
    let extension = engine.extension();
    format!(
        "Pick the game's folder or its Data folder (the one with Items.{extension} and System.{extension}). Encrypted games (Game.rgssad, .rgss2a, .rgss3a) have no readable Data folder."
    )
}

impl Session {
    /// The RPG Maker version of a Ruby Marshal save, read from the tree; `None` for other formats.
    pub(super) fn rgss_engine(&self, ctx: &Ctx<'_>) -> Option<Engine> {
        let marshal = self
            .recipe
            .steps
            .iter()
            .any(|step| step.kind == "format.ruby-marshal");
        marshal.then(|| ctx.rgss_engine())
    }

    /// The RPG Maker sections of an XP, VX or VX Ace save, with names from the game's `Data`
    /// folder; `None` for other files.
    pub(super) fn rgss_view(&self, ctx: &Ctx<'_>) -> Option<RpgMakerView> {
        let engine = self.rgss_engine(ctx)?;
        // The folder the player picked, else the one next to the save (a pasted code has none).
        let names = match &self.data_folder {
            Some(folder) => rgss_names_in(folder, engine).unwrap_or_default(),
            None if self.pasted => GameNames::default(),
            None => load_names(&self.path, engine),
        };
        ctx.rgss_view(engine, &names)
    }
}
