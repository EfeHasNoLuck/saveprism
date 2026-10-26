//! Gwent card names for the open Witcher 3 save, read from the player's own game files.
//!
//! The save stores only card indexes (`W3:gwent-card:<index>`); the names, factions and leader flags
//! come from the game install at runtime. Nothing is bundled with SavePrism and nothing is written
//! to the game folder: the bundles are opened read-only and only the two card XML files are read.
//!
//! # Which files are read
//!
//! `content/content0/bundles/xml.bundle` holds the definitions for the base game,
//! `patch*.bundle` files override them, and DLC bundles override those again. SavePrism does not
//! know the game's own load order beyond that public distribution shape, so it applies the
//! documented fallback: base `xml.bundle`, then patch bundles by ascending patch number (the
//! highest-numbered patch wins), then DLC bundles by name; a later definition replaces an earlier
//! one with the same card index, and `items_plus` (New Game+) replaces `items`. The report records
//! this rule.
//!
//! # Finding the game
//!
//! The folder is found the same way the key finder finds a game: a folder next to the save, the
//! Steam libraries listed in `<Steam>\steamapps\libraryfolders.vdf` (`The Witcher 3`), the GOG
//! registry entries and the usual GOG install folders, or a folder the player picked. When none is
//! found the command returns the `pickGameFolder` state instead of an error; the UI asks the player
//! and calls the command again with `gameFolder`.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::path::{Path, PathBuf};

use saveprism_codecs::witcher3::bundle::{BundleToc, read_entry};
use saveprism_codecs::witcher3::cards::{Card, parse_cards};
use serde::Serialize;

use super::Session;
use super::keyfinder::{library_folders, platform_steam_roots};

/// The codec id of the only save format this view is for.
const WITCHER3_CODEC: &str = "format.witcher3";
/// Largest card XML one bundle entry may decompress to.
const MAX_XML_BYTES: usize = 64 * 1024 * 1024;
/// Largest total card XML all bundles may decompress to in one load.
const MAX_TOTAL_XML_BYTES: usize = 256 * 1024 * 1024;
/// Most bundles one load may inspect.
const MAX_BUNDLES: usize = 512;
/// Most DLC folders one load may inspect.
const MAX_DLC_FOLDERS: usize = 64;
/// Most directory entries one listing may return.
const MAX_DIR_ENTRIES: usize = 4096;
/// Most GOG game keys read from the registry.
const MAX_GOG_KEYS: usize = 512;
/// Folder names a Steam install of the game uses.
const STEAM_FOLDERS: &[&str] = &[
    "The Witcher 3",
    "The Witcher 3 Wild Hunt",
    "The Witcher 3 GOTY",
];
/// Folder names a GOG install of the game uses.
const GOG_FOLDERS: &[&str] = &[
    "The Witcher 3 Wild Hunt GOTY",
    "The Witcher 3 Wild Hunt",
    "The Witcher 3",
];
/// Usual parent folders of a GOG install.
const GOG_ROOTS: &[&str] = &[
    r"C:\Program Files (x86)\GOG Galaxy\Games",
    r"C:\Program Files\GOG Galaxy\Games",
    r"C:\GOG Games",
];

/// The command's answer is always one of these states.
pub const STATE_READY: &str = "ready";
/// No game folder is known; the player should pick one.
pub const STATE_PICK_FOLDER: &str = "pickGameFolder";
/// The game folder exists but holds no card definitions SavePrism could read.
pub const STATE_NO_CARDS: &str = "noCards";
/// The open file is not a Witcher 3 save.
pub const STATE_NOT_WITCHER3: &str = "notWitcher3Save";

/// One card as the UI receives it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct W3CardRow {
    /// The card index the save's Gwent records carry.
    pub index: u32,
    /// A readable name from the game's own XML (`name`, else the `title` key made readable).
    pub name: String,
    /// The localization key from the XML, for a future `.w3strings` reader.
    pub title: String,
    /// The XML `faction_index`.
    pub faction: String,
    /// True for a battle-king (leader) card.
    pub leader: bool,
}

/// Every card the game defines, plus how the view was found.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct W3CardsView {
    /// One of [`STATE_READY`], [`STATE_PICK_FOLDER`], [`STATE_NO_CARDS`] or
    /// [`STATE_NOT_WITCHER3`].
    pub state: &'static str,
    /// The game folder that was used, when one was found.
    pub game_folder: Option<String>,
    /// How the folder was found: `save`, `steam`, `gog` or `picked`.
    pub source: Option<&'static str>,
    /// The number of cards in `cards`.
    pub card_count: usize,
    /// The cards in ascending index order.
    pub cards: Vec<W3CardRow>,
    /// Bundles or XML files that could not be read, in English, for the log and the report.
    pub warnings: Vec<String>,
}

impl W3CardsView {
    fn state(state: &'static str) -> Self {
        Self {
            state,
            game_folder: None,
            source: None,
            card_count: 0,
            cards: Vec::new(),
            warnings: Vec::new(),
        }
    }

    fn with_folder(
        state: &'static str,
        folder: &Path,
        source: &'static str,
        cards: Vec<W3CardRow>,
        warnings: Vec<String>,
    ) -> Self {
        Self {
            state,
            game_folder: Some(folder.to_string_lossy().into_owned()),
            source: Some(source),
            card_count: cards.len(),
            cards,
            warnings,
        }
    }
}

/// The card catalog loaded for the open file, kept for the rest of the session.
#[derive(Clone, Debug)]
pub(crate) struct W3CardsCache {
    folder: PathBuf,
    source: &'static str,
    cards: Vec<W3CardRow>,
    warnings: Vec<String>,
}

impl W3CardsCache {
    fn view(&self) -> W3CardsView {
        W3CardsView::with_folder(
            STATE_READY,
            &self.folder,
            self.source,
            self.cards.clone(),
            self.warnings.clone(),
        )
    }
}

impl Session {
    /// Every Gwent card for the open save, from the player's own game files.
    pub(crate) fn w3_cards(&mut self, picked: Option<&str>) -> W3CardsView {
        self.w3_cards_with(picked, &GameFolderFinder::production())
    }

    /// The same, with an injectable finder (tests never look at the real Steam or GOG).
    fn w3_cards_with(&mut self, picked: Option<&str>, finder: &GameFolderFinder) -> W3CardsView {
        if !self.is_witcher3() {
            return W3CardsView::state(STATE_NOT_WITCHER3);
        }
        let picked = picked
            .map(str::trim)
            .filter(|folder| !folder.is_empty())
            .map(PathBuf::from);
        if let Some(folder) = &picked
            && !is_witcher3_folder(folder)
        {
            let mut view = W3CardsView::state(STATE_PICK_FOLDER);
            view.game_folder = Some(folder.to_string_lossy().into_owned());
            view.warnings.push(
                "the picked folder does not look like a The Witcher 3 install (it has no content/content0/bundles)"
                    .to_string(),
            );
            return view;
        }
        if let Some(cache) = &self.witcher3_cards
            && cache.folder.is_dir()
            && picked
                .as_deref()
                .is_none_or(|folder| folder == cache.folder)
        {
            return cache.view();
        }

        let found = match picked {
            Some(folder) => Some((folder, "picked")),
            None => finder.find(&self.path),
        };
        let Some((folder, source)) = found else {
            return W3CardsView::state(STATE_PICK_FOLDER);
        };
        let (cards, warnings) = load_cards(&folder);
        if cards.is_empty() {
            let mut view =
                W3CardsView::with_folder(STATE_NO_CARDS, &folder, source, Vec::new(), warnings);
            view.warnings.push(format!(
                "no Gwent card definitions were found under {}",
                folder.join("content/content0/bundles").display()
            ));
            return view;
        }
        let cache = W3CardsCache {
            folder: folder.clone(),
            source,
            cards,
            warnings,
        };
        let view = cache.view();
        self.witcher3_cards = Some(cache);
        view
    }

    pub(super) fn is_witcher3(&self) -> bool {
        self.recipe
            .steps
            .last()
            .is_some_and(|step| step.kind == WITCHER3_CODEC)
    }
}

/// A game folder and the evidence that found it.
struct GameFolderFinder {
    /// Steam roots; `None` asks the platform.
    steam_roots: Option<Vec<PathBuf>>,
    /// GOG folders; `None` asks the platform.
    gog_folders: Option<Vec<PathBuf>>,
}

impl GameFolderFinder {
    fn production() -> Self {
        Self {
            steam_roots: None,
            gog_folders: None,
        }
    }

    #[cfg(test)]
    fn for_test(steam_roots: Vec<PathBuf>, gog_folders: Vec<PathBuf>) -> Self {
        Self {
            steam_roots: Some(steam_roots),
            gog_folders: Some(gog_folders),
        }
    }

    fn find(&self, save: &Path) -> Option<(PathBuf, &'static str)> {
        for ancestor in save.ancestors().skip(1).take(4) {
            if is_witcher3_folder(ancestor) {
                return Some((ancestor.to_path_buf(), "save"));
            }
        }
        for root in self.steam_roots_or_platform() {
            for library in library_folders(&root).into_iter().take(64) {
                let common = library.join("steamapps").join("common");
                for name in STEAM_FOLDERS {
                    let folder = common.join(name);
                    if is_witcher3_folder(&folder) {
                        return Some((folder, "steam"));
                    }
                }
            }
        }
        for folder in self.gog_folders_or_platform() {
            if is_witcher3_folder(&folder) {
                return Some((folder, "gog"));
            }
        }
        None
    }

    fn steam_roots_or_platform(&self) -> Vec<PathBuf> {
        match &self.steam_roots {
            Some(roots) => roots.clone(),
            None => platform_steam_roots(),
        }
    }

    fn gog_folders_or_platform(&self) -> Vec<PathBuf> {
        match &self.gog_folders {
            Some(folders) => folders.clone(),
            None => gog_platform_folders(),
        }
    }
}

/// True when `folder` is a The Witcher 3 install (its content bundles are there).
fn is_witcher3_folder(folder: &Path) -> bool {
    folder
        .join("content")
        .join("content0")
        .join("bundles")
        .is_dir()
}

/// The usual GOG install folders plus the paths GOG Galaxy registered on this PC.
fn gog_platform_folders() -> Vec<PathBuf> {
    let mut out = Vec::new();
    for root in GOG_ROOTS {
        for name in GOG_FOLDERS {
            out.push(PathBuf::from(root).join(name));
        }
    }
    #[cfg(windows)]
    out.extend(gog_registry_folders());
    dedup_folders(out)
}

#[cfg(windows)]
fn gog_registry_folders() -> Vec<PathBuf> {
    use winreg::RegKey;

    fn scan(hive: &RegKey) -> Vec<PathBuf> {
        let mut out = Vec::new();
        for path in [
            r"SOFTWARE\WOW6432Node\GOG.com\Games",
            r"SOFTWARE\GOG.com\Games",
        ] {
            let Ok(games) = hive.open_subkey(path) else {
                continue;
            };
            for id in games.enum_keys().flatten().take(MAX_GOG_KEYS) {
                let Ok(game) = games.open_subkey(&id) else {
                    continue;
                };
                let name: String = game.get_value("gameName").unwrap_or_default();
                let folder: String = game.get_value("path").unwrap_or_default();
                if folder.is_empty() {
                    continue;
                }
                let matches = name.to_lowercase().contains("witcher 3")
                    || folder.to_lowercase().contains("witcher 3");
                if matches {
                    out.push(PathBuf::from(folder));
                }
            }
        }
        out
    }

    let mut out = scan(winreg::HKLM);
    out.extend(scan(winreg::HKCU));
    out
}

fn dedup_folders(folders: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for folder in folders {
        let key = folder.to_string_lossy().to_lowercase();
        if !out
            .iter()
            .any(|seen: &PathBuf| seen.to_string_lossy().to_lowercase() == key)
        {
            out.push(folder);
        }
    }
    out
}

/// Reads every card definition from the bundles of one game folder.
fn load_cards(folder: &Path) -> (Vec<W3CardRow>, Vec<String>) {
    let mut catalog: BTreeMap<u32, Card> = BTreeMap::new();
    let mut warnings = Vec::new();
    let mut total_xml = 0_usize;
    for bundle in bundle_paths(folder) {
        match cards_from_bundle(&bundle, &mut catalog, &mut total_xml) {
            Ok(_) => {}
            Err(message) => warnings.push(message),
        }
    }
    let cards = catalog.into_values().map(W3CardRow::from).collect();
    (cards, warnings)
}

impl From<Card> for W3CardRow {
    fn from(card: Card) -> Self {
        Self {
            index: card.index,
            name: card.name,
            title: card.title,
            faction: card.faction,
            leader: card.leader,
        }
    }
}

/// Reads the matching XML entries of one bundle into `catalog`.
fn cards_from_bundle(
    path: &Path,
    catalog: &mut BTreeMap<u32, Card>,
    total_xml: &mut usize,
) -> Result<usize, String> {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    let mut file = File::open(path).map_err(|error| format!("{name}: {error}"))?;
    let toc = BundleToc::read(&mut file).map_err(|error| format!("{name}: {}", error.message()))?;
    let mut matches: Vec<(bool, bool, usize)> = Vec::new();
    for (index, entry) in toc.entries.iter().enumerate() {
        if let Some((ngplus, leaders)) = card_target(&entry.name) {
            matches.push((ngplus, leaders, index));
        }
    }
    matches.sort_by_key(|(ngplus, _, _)| *ngplus);
    let mut parsed = 0_usize;
    for (_, leaders, index) in matches {
        let Some(entry) = toc.entries.get(index) else {
            continue;
        };
        let xml = read_entry(&mut file, entry, MAX_XML_BYTES)
            .map_err(|error| format!("{name}: {}", error.message()))?;
        *total_xml = total_xml.saturating_add(xml.len());
        if *total_xml > MAX_TOTAL_XML_BYTES {
            return Err(format!(
                "{name}: the card XML files are larger than the {MAX_TOTAL_XML_BYTES} byte budget"
            ));
        }
        let cards = parse_cards(&xml, leaders)
            .map_err(|error| format!("{name} ({}): {}", entry.name, error.message()))?;
        for card in cards {
            catalog.insert(card.index, card);
        }
        parsed = parsed.saturating_add(1);
    }
    Ok(parsed)
}

/// Classifies a bundle path as one of the card definition files.
fn card_target(name: &str) -> Option<(bool, bool)> {
    let path = name.replace('\\', "/").to_ascii_lowercase();
    let (folder, file) = path.rsplit_once('/')?;
    let leaders = file == "def_gwint_battle_king_cards.xml";
    let cards = file == "def_gwint_cards_final.xml";
    if !leaders && !cards {
        return None;
    }
    let ngplus = folder.ends_with("items_plus");
    if ngplus || folder.ends_with("items") {
        Some((ngplus, leaders))
    } else {
        None
    }
}

/// The bundles to read, lowest priority first: base `xml.bundle`, then patch bundles by number,
/// then DLC bundles.
fn bundle_paths(game: &Path) -> Vec<PathBuf> {
    let mut base: Vec<(u8, u32, String, PathBuf)> = Vec::new();
    let bundles = game.join("content").join("content0").join("bundles");
    for path in list_files(&bundles, MAX_DIR_ENTRIES) {
        let Some(name) = path
            .file_name()
            .map(|name| name.to_string_lossy().to_lowercase())
        else {
            continue;
        };
        if name == "xml.bundle" {
            base.push((0, 0, name, path));
        } else if name.starts_with("patch") && name.ends_with(".bundle") {
            let number = name
                .strip_prefix("patch")
                .and_then(|rest| rest.strip_suffix(".bundle"))
                .and_then(|digits| digits.parse::<u32>().ok())
                .unwrap_or(0);
            base.push((1, number, name, path));
        }
    }
    base.sort_by(|left, right| (left.0, left.1, &left.2).cmp(&(right.0, right.1, &right.2)));

    let mut dlc: Vec<(String, PathBuf)> = Vec::new();
    for root in dlc_roots(game) {
        for folder in list_dirs(&root, MAX_DLC_FOLDERS) {
            for bundles in [
                folder.join("content").join("bundles"),
                folder.join("content").join("content0").join("bundles"),
            ] {
                for path in list_files(&bundles, MAX_DIR_ENTRIES) {
                    let is_bundle = path.file_name().is_some_and(|name| {
                        name.to_string_lossy().to_lowercase().ends_with(".bundle")
                    });
                    if is_bundle {
                        dlc.push((path.to_string_lossy().to_lowercase(), path));
                    }
                }
            }
        }
    }
    dlc.sort_by(|left, right| left.0.cmp(&right.0));

    let mut out: Vec<PathBuf> = base.into_iter().map(|(_, _, _, path)| path).collect();
    out.extend(dlc.into_iter().map(|(_, path)| path));
    out.truncate(MAX_BUNDLES);
    out
}

fn dlc_roots(game: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in fs::read_dir(game).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir()
            && path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case("dlc"))
        {
            out.push(path);
        }
    }
    dedup_folders(out)
}

fn list_files(dir: &Path, limit: usize) -> Vec<PathBuf> {
    fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .take(limit)
        .collect()
}

fn list_dirs(dir: &Path, limit: usize) -> Vec<PathBuf> {
    fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .take(limit)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixtures() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../fixtures/synthetic/witcher3-bundle")
    }

    fn card(cards: &[W3CardRow], index: u32) -> &W3CardRow {
        cards
            .iter()
            .find(|card| card.index == index)
            .unwrap_or_else(|| panic!("card {index}"))
    }

    #[test]
    fn patches_dlc_and_items_plus_replace_the_base_cards() {
        let (cards, warnings) = load_cards(&fixtures().join("game"));
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(cards.len(), 5);
        assert_eq!(card(&cards, 1).name, "Alpha Plus");
        assert_eq!(card(&cards, 2).name, "Beta Patched");
        assert_eq!(card(&cards, 3).name, "Gamma Patch2");
        assert_eq!(card(&cards, 1001).name, "Leader One Dlc");
        assert_eq!(card(&cards, 1002).name, "Leader Two");
        assert!(card(&cards, 1001).leader);
        assert!(!card(&cards, 1).leader);
        assert_eq!(card(&cards, 2).faction, "F_NORTHERN_KINGDOM");
    }

    #[test]
    fn the_finder_uses_a_save_inside_the_game_folder_first() {
        let game = fixtures().join("game");
        let save = game
            .join("content")
            .join("content0")
            .join("bundles")
            .join("xml.bundle");
        let finder = GameFolderFinder::for_test(Vec::new(), Vec::new());
        let (found, source) = finder.find(&save).expect("a folder is found");
        assert_eq!(found, game);
        assert_eq!(source, "save");
    }

    #[test]
    fn the_finder_reads_a_steam_library() {
        let root = fixtures().join("steam");
        let save = fixtures().join("save.sav");
        let finder = GameFolderFinder::for_test(vec![root], Vec::new());
        let (found, source) = finder.find(&save).expect("a folder is found");
        assert_eq!(
            found,
            fixtures().join("steam/steamapps/common/The Witcher 3")
        );
        assert_eq!(source, "steam");
    }

    #[test]
    fn the_finder_reads_a_gog_folder() {
        let gog = fixtures().join("gog/The Witcher 3 Wild Hunt GOTY");
        let save = fixtures().join("save.sav");
        let finder = GameFolderFinder::for_test(Vec::new(), vec![gog.clone()]);
        let (found, source) = finder.find(&save).expect("a folder is found");
        assert_eq!(found, gog);
        assert_eq!(source, "gog");
    }

    #[test]
    fn no_folder_is_the_pick_state_not_an_error() {
        let finder = GameFolderFinder::for_test(Vec::new(), Vec::new());
        assert!(finder.find(&fixtures().join("save.sav")).is_none());
    }

    fn open_session(save: &Path) -> Session {
        Session::open(&save.to_string_lossy(), None, None).expect("the fixture save opens")
    }

    #[test]
    fn the_command_reports_the_pick_state_for_a_witcher_save_without_a_game() {
        let mut session = open_session(
            &PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../../fixtures/synthetic/witcher3/ManualSave_invented.sav"),
        );
        let view = session.w3_cards_with(None, &GameFolderFinder::for_test(Vec::new(), Vec::new()));
        assert_eq!(view.state, STATE_PICK_FOLDER);
        assert!(view.cards.is_empty());
        assert!(view.game_folder.is_none());
    }

    #[test]
    fn the_command_returns_cards_for_a_picked_game_folder() {
        let mut session = open_session(
            &PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../../fixtures/synthetic/witcher3/ManualSave_invented.sav"),
        );
        let folder = fixtures().join("game");
        let view = session.w3_cards_with(
            Some(&folder.to_string_lossy()),
            &GameFolderFinder::for_test(Vec::new(), Vec::new()),
        );
        assert_eq!(view.state, STATE_READY);
        assert_eq!(view.source, Some("picked"));
        assert_eq!(view.card_count, 5);
        assert_eq!(card(&view.cards, 2).name, "Beta Patched");
        // The second call comes from the in-memory cache.
        let again = session.w3_cards_with(
            Some(&folder.to_string_lossy()),
            &GameFolderFinder::for_test(Vec::new(), Vec::new()),
        );
        assert_eq!(again.cards, view.cards);
    }

    #[test]
    fn a_picked_folder_that_is_not_a_game_keeps_asking() {
        let mut session = open_session(
            &PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../../fixtures/synthetic/witcher3/ManualSave_invented.sav"),
        );
        let view = session.w3_cards_with(
            Some(&fixtures().join("standalone").to_string_lossy()),
            &GameFolderFinder::for_test(Vec::new(), Vec::new()),
        );
        assert_eq!(view.state, STATE_PICK_FOLDER);
        assert!(!view.warnings.is_empty());
    }

    #[test]
    fn a_non_witcher_save_is_not_an_error_toast() {
        let mut session = open_session(
            &PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../../fixtures/synthetic/json/compact.json"),
        );
        let view = session.w3_cards(None);
        assert_eq!(view.state, STATE_NOT_WITCHER3);
        assert!(view.cards.is_empty());
    }
}
