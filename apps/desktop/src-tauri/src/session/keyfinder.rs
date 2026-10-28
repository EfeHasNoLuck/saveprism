//! Finding the password of an encrypted save in the game's own files.
//!
//! An encrypted Easy Save 3 save asks the player for its password. When the game is installed on
//! this PC, its own string literals often hold that password, so [`KeyFinder`] looks for it: it
//! finds the game's install folder, reads the UTF-16 user strings of a managed
//! `Assembly-CSharp.dll` (the ECMA-335 `#US` heap; see ECMA-335 Partition II §24.2.4 and the MIT
//! `dotnet/runtime` `System.Reflection.Metadata` reader) and the UTF-8 string literals of an IL2CPP
//! `global-metadata.dat` (Il2CppDumper's public `Il2CppGlobalMetadataHeader` layout, MIT), plus
//! plain-text `ES3Defaults` settings. Only data is read: no assembly is loaded, decompiled or run.
//!
//! The game folder is found by walking up from the save, by matching Unity's
//! `AppData\LocalLow\<company>\<product>` (Unity's `Application.persistentDataPath` documentation)
//! against the two lines of `<Game>_Data\app.info`, from the Steam libraries listed in
//! `<Steam>\steamapps\libraryfolders.vdf` (Valve's KeyValues structure; the Steam root comes from
//! `HKCU\Software\Valve\Steam\SteamPath`, `HKLM\SOFTWARE\Wow6432Node\Valve\Steam\InstallPath`, or
//! `C:\Program Files (x86)\Steam`), and from a folder the player picked before.
//!
//! A found key is stored with evidence in `%LOCALAPPDATA%\SavePrism\keys.json` (game folder, source
//! file, method, date), never next to a save. The key's `Debug` is redacted, so it cannot reach a
//! log or an error by accident. All searching is bounded: a few thousand directory entries and at
//! most [`MAX_CANDIDATES`] strings.

use std::collections::HashSet;
use std::fmt;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// Most candidate strings one search collects.
pub(crate) const MAX_CANDIDATES: usize = 20_000;
/// Longest candidate key, in characters.
const MAX_CANDIDATE_CHARS: usize = 64;
/// Largest file the metadata readers open.
const MAX_FILE_BYTES: u64 = 256 * 1024 * 1024;
/// Largest `app.info`.
const MAX_APP_INFO_BYTES: u64 = 4 * 1024;
/// Largest Steam `libraryfolders.vdf`.
const MAX_VDF_BYTES: u64 = 1024 * 1024;
/// Most Steam libraries read from one `libraryfolders.vdf`.
const MAX_LIBRARIES: usize = 32;
/// Largest plain-text `ES3Defaults` settings file.
const MAX_TEXT_SETTINGS_BYTES: u64 = 256 * 1024;
/// Largest `keys.json` read.
const MAX_STORE_BYTES: u64 = 1024 * 1024;
/// Directory entries one search may read (a bound on the whole search).
const MAX_DIR_ENTRIES: usize = 4096;
/// Game folders checked in one Steam library.
const MAX_GAME_FOLDERS: usize = 512;
/// Entries read from one Steam game folder while looking for its `_Data` folder (a game root
/// holds a few dozen), so many libraries cannot multiply into millions of reads.
const MAX_GAME_DIR_ENTRIES: usize = 256;
/// Name of the evidence file inside the app data folder.
const STORE_FILE: &str = "keys.json";
/// Version of the evidence file.
const STORE_VERSION: u32 = 1;
/// Method id for strings from the .NET user-string heap.
const METHOD_US_HEAP: &str = "dotnet-us-heap";
/// Method id for IL2CPP string literals.
const METHOD_IL2CPP: &str = "il2cpp-string-literal";
/// Method id for plain-text Easy Save settings.
const METHOD_SETTINGS: &str = "es3-settings-text";
/// Method id for a key read back from the evidence file.
const METHOD_STORED: &str = "stored";
/// Method id for a key read from a .NET `#US` heap outside Unity (Terraria's own `Terraria.exe`).
const METHOD_TERRARIA: &str = "terraria-us-heap";
/// Largest `Terraria.exe` read while looking for its key.
const MAX_TERRARIA_EXE_BYTES: u64 = 256 * 1024 * 1024;
/// Length of Terraria's key in UTF-16 code units: it is also the 16 byte IV, so only strings of
/// exactly this length are candidates.
const TERRARIA_KEY_UNITS: usize = 8;

/// A key whose `Debug` prints no key material.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
struct Secret(String);

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted>")
    }
}

impl Secret {
    fn as_str(&self) -> &str {
        &self.0
    }
}

/// One candidate key and where it came from. Its `Debug` is redacted through [`Secret`].
#[derive(Clone)]
pub(crate) struct Candidate {
    key: Secret,
    /// File the string was read from (the evidence file for a stored key).
    source: PathBuf,
    /// Game install folder the source belongs to.
    game_folder: PathBuf,
    /// Stable method id (`dotnet-us-heap`, `il2cpp-string-literal`, ...).
    method: &'static str,
    /// Which encrypted format this key belongs to.
    kind: GameKind,
}

/// The encrypted format a candidate key is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GameKind {
    /// Unity Easy Save 3 (PBKDF2 + AES-128-CBC).
    Es3,
    /// Terraria `.plr` (AES-CBC with the UTF-16LE key as key and IV).
    Terraria,
}

impl GameKind {
    fn as_str(self) -> &'static str {
        match self {
            GameKind::Es3 => "es3",
            GameKind::Terraria => "terraria",
        }
    }

    fn from_str(text: &str) -> Self {
        match text {
            "terraria" => GameKind::Terraria,
            _ => GameKind::Es3,
        }
    }
}

impl Candidate {
    /// The candidate text, to be tried as the save's key.
    pub(crate) fn key(&self) -> &str {
        self.key.as_str()
    }

    /// True when this key belongs to a Terraria player file.
    pub(crate) fn is_terraria(&self) -> bool {
        self.kind == GameKind::Terraria
    }
}

impl fmt::Debug for Candidate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Candidate")
            .field("key", &self.key)
            .field("source", &self.source)
            .field("game_folder", &self.game_folder)
            .field("method", &self.method)
            .field("kind", &self.kind.as_str())
            .finish()
    }
}

/// A source of candidate keys. Kept generic so another encrypted format can provide its own.
pub(crate) trait KeyCandidates {
    /// The candidates, de-duplicated, in the order they should be tried.
    fn candidates(&mut self) -> Vec<Candidate>;
}

/// A folder of game files that yields candidate keys.
struct GameFiles {
    data_dir: PathBuf,
    collector: Collector,
}

impl GameFiles {
    fn new(game_folder: PathBuf, data_dir: PathBuf, limit: usize) -> Self {
        Self {
            collector: Collector::new(game_folder, limit, GameKind::Es3),
            data_dir,
        }
    }
}

impl KeyCandidates for GameFiles {
    fn candidates(&mut self) -> Vec<Candidate> {
        let dll = self.data_dir.join("Managed").join("Assembly-CSharp.dll");
        if let Some(bytes) = read_file(&dll, MAX_FILE_BYTES)
            && let Some(metadata) = dotnet_metadata(&bytes)
            && let Some(heap) = metadata_stream(metadata, b"#US")
        {
            // The cap is passed in: a huge heap must stop at the collector's room, not decode
            // every tiny string first.
            for text in us_heap_strings(heap, self.collector.room()) {
                self.collector.add(&text, &dll, METHOD_US_HEAP);
            }
        }
        let il2cpp = self
            .data_dir
            .join("il2cpp_data")
            .join("Metadata")
            .join("global-metadata.dat");
        if let Some(bytes) = read_file(&il2cpp, MAX_FILE_BYTES) {
            for text in il2cpp_strings(&bytes) {
                self.collector.add(&text, &il2cpp, METHOD_IL2CPP);
            }
        }
        let settings = self.data_dir.join("StreamingAssets");
        for path in settings_files(&self.data_dir) {
            if let Some(text) = read_text(&path, MAX_TEXT_SETTINGS_BYTES) {
                for value in text_settings_strings(&text) {
                    self.collector.add(&value, &path, METHOD_SETTINGS);
                }
            }
        }
        for path in settings_files(&settings) {
            if let Some(text) = read_text(&path, MAX_TEXT_SETTINGS_BYTES) {
                for value in text_settings_strings(&text) {
                    self.collector.add(&value, &path, METHOD_SETTINGS);
                }
            }
        }
        self.collector.take()
    }
}

/// A Terraria install folder: the `#US` heap of its managed `Terraria.exe` holds the key string.
struct TerrariaFiles {
    exe: PathBuf,
    collector: Collector,
}

impl TerrariaFiles {
    fn new(game_folder: PathBuf, exe: PathBuf, limit: usize) -> Self {
        Self {
            collector: Collector::new(game_folder, limit, GameKind::Terraria),
            exe,
        }
    }
}

impl KeyCandidates for TerrariaFiles {
    fn candidates(&mut self) -> Vec<Candidate> {
        if let Some(bytes) = read_file(&self.exe, MAX_TERRARIA_EXE_BYTES)
            && let Some(metadata) = dotnet_metadata(&bytes)
            && let Some(heap) = metadata_stream(metadata, b"#US")
        {
            // Only strings of exactly the key length count towards the cap, so a big heap of other
            // strings cannot push the real key out of the collector's room.
            let room = self.collector.room();
            for text in us_heap_strings_of(heap, room, Some(TERRARIA_KEY_UNITS)) {
                self.collector.add(&text, &self.exe, METHOD_TERRARIA);
            }
        }
        self.collector.take()
    }
}

/// True when `save` is a Terraria player file (`.plr`, or the game's own backup `.plr.bak`) or
/// lives in the game's `Players` folder.
fn is_terraria_save(save: &Path) -> bool {
    let plr = save
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(saveprism_codecs::terraria::is_player_file_name);
    if plr {
        return true;
    }
    let parts: Vec<String> = save
        .components()
        .filter_map(|component| match component {
            std::path::Component::Normal(name) => Some(name.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();
    parts
        .iter()
        .any(|part| part.eq_ignore_ascii_case("Terraria"))
        && parts
            .iter()
            .any(|part| part.eq_ignore_ascii_case("Players"))
}

/// The `Terraria.exe` in a game folder, when this is a Terraria install.
fn terraria_exe(folder: &Path) -> Option<PathBuf> {
    let exe = folder.join("Terraria.exe");
    exe.is_file().then_some(exe)
}

/// Collects candidate keys without duplicates, up to a limit.
struct Collector {
    game_folder: PathBuf,
    seen: HashSet<String>,
    out: Vec<Candidate>,
    limit: usize,
    kind: GameKind,
}

impl Collector {
    fn new(game_folder: PathBuf, limit: usize, kind: GameKind) -> Self {
        Self {
            game_folder,
            seen: HashSet::new(),
            out: Vec::new(),
            limit,
            kind,
        }
    }

    fn add(&mut self, text: &str, source: &Path, method: &'static str) {
        if self.out.len() >= self.limit {
            return;
        }
        let key = text.trim_matches('\0');
        if !valid_candidate(key) || !self.seen.insert(key.to_string()) {
            return;
        }
        self.out.push(Candidate {
            key: Secret(key.to_string()),
            source: source.to_path_buf(),
            game_folder: self.game_folder.clone(),
            method,
            kind: self.kind,
        });
    }

    /// How many more candidates the collector may take.
    fn room(&self) -> usize {
        self.limit.saturating_sub(self.out.len())
    }

    fn take(&mut self) -> Vec<Candidate> {
        std::mem::take(&mut self.out)
    }
}

/// True when `text` can be a key: 1 to [`MAX_CANDIDATE_CHARS`] characters, no control characters.
fn valid_candidate(text: &str) -> bool {
    let mut chars = 0;
    for c in text.chars() {
        if c.is_control() {
            return false;
        }
        chars += 1;
        if chars > MAX_CANDIDATE_CHARS {
            return false;
        }
    }
    chars > 0
}

/// Where the player's picker or a previous run points the search.
#[derive(Clone, Debug)]
pub(crate) struct KeyFinder {
    /// Evidence file; `None` disables reading and writing it (tests).
    store_path: Option<PathBuf>,
    /// Steam roots to use; `None` asks the platform (the real Steam), `Some(vec![])` asks nothing
    /// (tests).
    steam_roots: Option<Vec<PathBuf>>,
    /// Folders the player picked (most recent first).
    extra_folders: Vec<PathBuf>,
}

impl KeyFinder {
    /// The finder used by a normal open: the real Steam and the player's evidence file.
    pub(crate) fn production() -> Self {
        Self {
            store_path: default_store_path(),
            steam_roots: None,
            extra_folders: Vec::new(),
        }
    }

    /// Also try the folder the player picked first.
    pub(crate) fn with_folder(mut self, folder: PathBuf) -> Self {
        self.extra_folders.insert(0, folder);
        self
    }

    /// A finder with its own evidence file, for tests; it never reads the real Steam.
    #[cfg(test)]
    pub(crate) fn for_test(store: PathBuf) -> Self {
        Self {
            store_path: Some(store),
            // `Some(vec![])` means "no Steam at all"; only `None` asks the real platform.
            steam_roots: Some(Vec::new()),
            extra_folders: Vec::new(),
        }
    }

    /// Search these Steam roots instead of the platform's own (tests).
    #[cfg(test)]
    pub(crate) fn with_steam_root(mut self, root: PathBuf) -> Self {
        self.steam_roots.get_or_insert_with(Vec::new).push(root);
        self
    }

    /// Keys the evidence file already has for `save`. They are cheap to try before any game file
    /// is read.
    pub(crate) fn stored(&self, save: &Path) -> Vec<Candidate> {
        let Some(store_path) = self.store_path.as_deref() else {
            return Vec::new();
        };
        let store = load_store(store_path);
        let local_low = local_low_parts(save);
        store
            .games
            .iter()
            .filter(|stored| stored_matches(stored, local_low.as_ref(), save))
            .map(|stored| Candidate {
                key: Secret(stored.key.0.clone()),
                source: store_path.to_path_buf(),
                game_folder: PathBuf::from(&stored.game_folder),
                method: METHOD_STORED,
                kind: GameKind::from_str(stored.kind.as_deref().unwrap_or("es3")),
            })
            .collect()
    }

    /// The candidate keys to try for `save` from the game's own files. The search is bounded: the
    /// folder listing has its own budget and every `_Data` scan gets a fresh one, so a big Steam
    /// library cannot starve a folder found later.
    pub(crate) fn search(&self, save: &Path) -> Vec<Candidate> {
        let store = self
            .store_path
            .as_deref()
            .map(load_store)
            .unwrap_or_default();
        let local_low = local_low_parts(save);
        let terraria = is_terraria_save(save);
        let mut out: Vec<Candidate> = Vec::new();
        let mut seen = HashSet::new();
        for game_folder in self.game_folders(save, local_low.as_ref(), &store) {
            if out.len() >= MAX_CANDIDATES {
                break;
            }
            let limit = MAX_CANDIDATES.saturating_sub(out.len());
            let mut files: Box<dyn KeyCandidates> = if terraria {
                let Some(exe) = terraria_exe(&game_folder) else {
                    continue;
                };
                Box::new(TerrariaFiles::new(game_folder, exe, limit))
            } else {
                let mut scan = Budget::new(MAX_DIR_ENTRIES);
                let Some(data_dir) = data_dir_of(&game_folder, &mut scan) else {
                    continue;
                };
                Box::new(GameFiles::new(game_folder, data_dir, limit))
            };
            for candidate in files.candidates() {
                push_unique(&mut out, &mut seen, candidate);
                if out.len() >= MAX_CANDIDATES {
                    return out;
                }
            }
        }
        out
    }

    /// Remember a folder the player picked, even when no key was found in it, so a later open
    /// searches it again.
    pub(crate) fn remember_folder(&self, folder: &Path) {
        let Some(path) = self.store_path.as_deref() else {
            return;
        };
        let mut store = load_store(path);
        let folder = folder.to_string_lossy().into_owned();
        store.picked_folders.retain(|picked| picked != &folder);
        store.picked_folders.insert(0, folder);
        store.picked_folders.truncate(64);
        if store.version == 0 {
            store.version = STORE_VERSION;
        }
        save_store(path, &store);
    }

    /// Store the key that opened `save`, with its evidence, and remember the game folder. A key
    /// that came from the evidence file is not written again.
    pub(crate) fn remember(&self, save: &Path, found: &Candidate) {
        if found.method == METHOD_STORED {
            return;
        }
        let Some(path) = self.store_path.as_deref() else {
            return;
        };
        let mut store = load_store(path);
        let local_low = local_low_parts(save);
        let (company, product) = match &local_low {
            Some((company, product)) => (Some(company.clone()), Some(product.clone())),
            None => (None, None),
        };
        let folder = found.game_folder.to_string_lossy().into_owned();
        let stored_kind = found.kind.as_str();
        store.games.retain(|stored| {
            let same_kind = stored.kind.as_deref().unwrap_or("es3") == stored_kind;
            !(same_kind
                && (stored.game_folder == folder
                    || (company.is_some()
                        && stored.company == company
                        && stored.product == product)))
        });
        store.games.push(StoredKey {
            company,
            product,
            game_folder: folder.clone(),
            key: found.key.clone(),
            source: found.source.to_string_lossy().into_owned(),
            method: found.method.to_string(),
            date: today(),
            kind: Some(found.kind.as_str().to_string()),
        });
        store.version = STORE_VERSION;
        store.picked_folders.retain(|picked| picked != &folder);
        store.picked_folders.insert(0, folder);
        store.picked_folders.truncate(64);
        save_store(path, &store);
    }

    /// The game folders to read, most likely first. Every folder is checked with `app.info` when
    /// the save tells the company and product.
    fn game_folders(
        &self,
        save: &Path,
        local_low: Option<&(String, String)>,
        store: &KeyStore,
    ) -> Vec<PathBuf> {
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        let mut add = |folder: PathBuf| {
            let key = path_key(&folder);
            if seen.insert(key) {
                out.push(folder);
            }
        };
        // The folder the player picked wins, even without a matching app.info.
        for extra in &self.extra_folders {
            add(extra.clone());
        }
        for stored in &store.games {
            if stored_matches(stored, local_low, save) {
                add(PathBuf::from(&stored.game_folder));
            }
        }
        for picked in &store.picked_folders {
            let folder = PathBuf::from(picked);
            let mut scan = Budget::new(MAX_DIR_ENTRIES);
            if local_low.is_none() || folder_matches(&folder, local_low, &mut scan) {
                add(folder);
            }
        }
        // A save that lives inside the game's own folder.
        for ancestor in save.ancestors().skip(1).take(4) {
            add(ancestor.to_path_buf());
        }
        // Terraria is a plain .NET game, not Unity: its install folder is the Steam app folder.
        if is_terraria_save(save) {
            let mut listing = Budget::new(MAX_DIR_ENTRIES);
            for root in self.steam_roots_or_platform() {
                for library in library_folders(&root) {
                    let game = library.join("steamapps").join("common").join("Terraria");
                    let _ = listing.take();
                    if game.is_dir() {
                        add(game);
                    }
                }
            }
        }
        // Steam libraries. Without a company/product pair there is nothing to match against. This
        // listing has its own budget; each `_Data` lookup below gets a fresh one, so a large
        // library cannot exhaust the walk.
        if let Some(local_low) = local_low {
            let mut listing = Budget::new(MAX_DIR_ENTRIES);
            for root in self.steam_roots_or_platform() {
                for library in library_folders(&root) {
                    let common = library.join("steamapps").join("common");
                    for entry in read_dir_bounded(&common, &mut listing)
                        .into_iter()
                        .take(MAX_GAME_FOLDERS)
                    {
                        if !entry.is_dir() {
                            continue;
                        }
                        let mut scan = Budget::new(MAX_GAME_DIR_ENTRIES);
                        if data_dir_of(&entry, &mut scan)
                            .as_deref()
                            .and_then(app_info)
                            .is_some_and(|(company, product)| {
                                same(&company, &local_low.0) && same(&product, &local_low.1)
                            })
                        {
                            add(entry);
                        }
                    }
                }
            }
        }
        out
    }

    fn steam_roots_or_platform(&self) -> Vec<PathBuf> {
        match &self.steam_roots {
            Some(roots) => roots.clone(),
            None => platform_steam_roots(),
        }
    }
}

/// `%LOCALAPPDATA%\SavePrism\keys.json` (next to the backup folder), when the app data folder is
/// known.
fn default_store_path() -> Option<PathBuf> {
    saveprism_core::io::default_backup_root()?
        .parent()
        .map(|app_data| app_data.join(STORE_FILE))
}

/// The company and product of a Unity `LocalLow\<company>\<product>` save.
fn local_low_parts(save: &Path) -> Option<(String, String)> {
    let parts: Vec<String> = save
        .components()
        .filter_map(|component| match component {
            std::path::Component::Normal(name) => Some(name.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();
    let position = parts
        .iter()
        .position(|part| part.eq_ignore_ascii_case("LocalLow"))?;
    let company = parts.get(position.checked_add(1)?)?.clone();
    let product = parts.get(position.checked_add(2)?)?.clone();
    Some((company, product))
}

/// True when a stored entry belongs to this save.
fn stored_matches(stored: &StoredKey, local_low: Option<&(String, String)>, save: &Path) -> bool {
    let terraria = stored.kind.as_deref() == Some("terraria");
    if terraria {
        return is_terraria_save(save) || save.starts_with(Path::new(&stored.game_folder));
    }
    if is_terraria_save(save) {
        return false;
    }
    if let Some((company, product)) = local_low
        && stored
            .company
            .as_deref()
            .is_some_and(|stored| same(stored, company))
        && stored
            .product
            .as_deref()
            .is_some_and(|stored| same(stored, product))
    {
        return true;
    }
    let folder = Path::new(&stored.game_folder);
    !folder.as_os_str().is_empty() && save.starts_with(folder)
}

/// Compares names the way the Windows file system does.
fn same(left: &str, right: &str) -> bool {
    left.to_lowercase() == right.to_lowercase()
}

/// A stable key for de-duplicating paths.
fn path_key(path: &Path) -> String {
    std::fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .to_lowercase()
}

/// Adds `candidate` unless a candidate with the same key is already there.
fn push_unique(out: &mut Vec<Candidate>, seen: &mut HashSet<String>, candidate: Candidate) {
    if seen.insert(candidate.key.as_str().to_string()) {
        out.push(candidate);
    }
}

/// A countdown of directory entries.
struct Budget {
    left: usize,
}

impl Budget {
    fn new(left: usize) -> Self {
        Self { left }
    }

    fn take(&mut self) -> bool {
        if self.left == 0 {
            return false;
        }
        self.left -= 1;
        true
    }
}

/// The first `_Data` folder under `folder` (or `folder` itself) that holds an `app.info`.
fn data_dir_of(folder: &Path, budget: &mut Budget) -> Option<PathBuf> {
    if !folder.is_dir() {
        return None;
    }
    if folder
        .file_name()
        .is_some_and(|name| name.to_string_lossy().ends_with("_Data"))
        && folder.join("app.info").is_file()
    {
        return Some(folder.to_path_buf());
    }
    for entry in read_dir_bounded(folder, budget) {
        // `read_dir_bounded` already spent one unit per entry; do not count them twice.
        if !entry.is_dir() {
            continue;
        }
        let name = entry.file_name()?.to_string_lossy().into_owned();
        if name.ends_with("_Data") && entry.join("app.info").is_file() {
            return Some(entry);
        }
    }
    None
}

/// The company and product in a `_Data\app.info` file.
fn app_info(data_dir: &Path) -> Option<(String, String)> {
    let text = read_text(&data_dir.join("app.info"), MAX_APP_INFO_BYTES)?;
    let mut lines = text.lines().map(str::trim);
    let company = lines.next()?;
    let product = lines.next()?;
    if company.is_empty() || product.is_empty() {
        return None;
    }
    Some((company.to_string(), product.to_string()))
}

/// True when `folder` is the install folder of the save's company and product (or when the save
/// does not say).
fn folder_matches(
    folder: &Path,
    local_low: Option<&(String, String)>,
    budget: &mut Budget,
) -> bool {
    let Some((company, product)) = local_low else {
        return true;
    };
    data_dir_of(folder, budget)
        .as_deref()
        .and_then(app_info)
        .is_some_and(|(found_company, found_product)| {
            same(&found_company, company) && same(&found_product, product)
        })
}

/// The directories directly under `dir`, at most the remaining budget.
fn read_dir_bounded(dir: &Path, budget: &mut Budget) -> Vec<PathBuf> {
    let Ok(reader) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in reader.flatten() {
        if !budget.take() {
            break;
        }
        out.push(entry.path());
    }
    out
}

/// The Steam installation folders on this PC. Never scans a disk: registry and the usual path.
pub(crate) fn platform_steam_roots() -> Vec<PathBuf> {
    let mut out = Vec::new();
    #[cfg(windows)]
    {
        use winreg::{HKCU, HKLM};
        let user = HKCU
            .open_subkey("Software\\Valve\\Steam")
            .and_then(|key| key.get_value::<String, _>("SteamPath"));
        if let Ok(path) = user {
            out.push(PathBuf::from(path));
        }
        let machine = HKLM
            .open_subkey("SOFTWARE\\Wow6432Node\\Valve\\Steam")
            .or_else(|_| HKLM.open_subkey("SOFTWARE\\Valve\\Steam"))
            .and_then(|key| key.get_value::<String, _>("InstallPath"));
        if let Ok(path) = machine {
            out.push(PathBuf::from(path));
        }
        out.push(PathBuf::from("C:\\Program Files (x86)\\Steam"));
    }
    out.sort();
    out.dedup();
    out
}

/// The Steam root and every library listed in its `libraryfolders.vdf`, de-duplicated and capped.
pub(crate) fn library_folders(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let mut push = |path: PathBuf| {
        if out.len() >= MAX_LIBRARIES {
            return;
        }
        if seen.insert(path.to_string_lossy().to_lowercase()) {
            out.push(path);
        }
    };
    push(root.to_path_buf());
    let vdf = root.join("steamapps").join("libraryfolders.vdf");
    if let Some(text) = read_text(&vdf, MAX_VDF_BYTES) {
        for path in vdf_paths(&text) {
            push(PathBuf::from(path));
        }
    }
    out
}

/// The library paths in a `libraryfolders.vdf` (Valve KeyValues text): at most
/// [`MAX_LIBRARIES`], de-duplicated, absolute drive paths only (no UNC shares).
fn vdf_paths(text: &str) -> Vec<String> {
    let tokens = vdf_tokens(text);
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let mut index = 0;
    while index < tokens.len() && out.len() < MAX_LIBRARIES {
        let key = tokens.get(index);
        let value = tokens.get(index.saturating_add(1));
        match (key, value) {
            (Some(key), Some(value)) if key.eq_ignore_ascii_case("path") => {
                if looks_like_path(value) && seen.insert(value.to_lowercase()) {
                    out.push(value.clone());
                }
                index = index.saturating_add(2);
            }
            (Some(key), Some(value))
                if key.chars().all(|c| c.is_ascii_digit())
                    && looks_like_path(value)
                    && seen.insert(value.to_lowercase()) =>
            {
                out.push(value.clone());
                index = index.saturating_add(2);
            }
            _ => index = index.saturating_add(1),
        }
    }
    out
}

/// The quoted strings of a KeyValues file, with `\\` and `\"` unescaped.
fn vdf_tokens(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '"' {
            continue;
        }
        let mut token = String::new();
        while let Some(c) = chars.next() {
            match c {
                '"' => break,
                '\\' => {
                    if let Some(next) = chars.next() {
                        token.push(next);
                    }
                }
                _ => token.push(c),
            }
        }
        tokens.push(token);
    }
    tokens
}

/// True when a VDF value is an absolute drive path (`C:\...` or `C:/...`) rather than a number, an
/// app id or a UNC share (`\\server\share`).
fn looks_like_path(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(drive) = chars.next() else {
        return false;
    };
    if !drive.is_ascii_alphabetic() {
        return false;
    }
    matches!(chars.next(), Some(':')) && matches!(chars.next(), Some('\\' | '/'))
}

/// Reads at most `limit` bytes of a regular file.
fn read_file(path: &Path, limit: u64) -> Option<Vec<u8>> {
    let metadata = fs::metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > limit {
        return None;
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .ok()?
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .ok()?;
    Some(bytes)
}

/// Reads a UTF-8 text file of at most `limit` bytes; a BOM is dropped.
fn read_text(path: &Path, limit: u64) -> Option<String> {
    let bytes = read_file(path, limit)?;
    let text = String::from_utf8(bytes).ok()?;
    Some(text.trim_start_matches('\u{feff}').to_string())
}

/// The plain-text `ES3Defaults` settings files directly under `dir`.
fn settings_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(reader) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in reader.flatten().take(MAX_GAME_FOLDERS) {
        let path = entry.path();
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default()
            .to_lowercase();
        if path.is_file()
            && name.contains("es3")
            && read_file(&path, MAX_TEXT_SETTINGS_BYTES).is_some_and(|bytes| !bytes.contains(&0))
        {
            out.push(path);
        }
    }
    out
}

/// Non-empty lines, quoted strings and right-hand sides of `key=value` lines of a settings file.
fn text_settings_strings(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if valid_candidate(line) {
            out.push(line.to_string());
        }
        let quoted = line.split('"').skip(1).step_by(2);
        for value in quoted {
            if valid_candidate(value) {
                out.push(value.to_string());
            }
        }
        if let Some((_, value)) = line.split_once('=') {
            let value = value.trim().trim_matches('"');
            if valid_candidate(value) {
                out.push(value.to_string());
            }
        }
    }
    out
}

/// A little-endian `u16` at `offset`.
fn u16_at(bytes: &[u8], offset: usize) -> Option<u16> {
    let end = offset.checked_add(2)?;
    Some(u16::from_le_bytes(bytes.get(offset..end)?.try_into().ok()?))
}

/// A little-endian `u32` at `offset`.
fn u32_at(bytes: &[u8], offset: usize) -> Option<u32> {
    let end = offset.checked_add(4)?;
    Some(u32::from_le_bytes(bytes.get(offset..end)?.try_into().ok()?))
}

/// `value` rounded up to the next multiple of four.
fn align4(value: usize) -> Option<usize> {
    Some(value.checked_add(3)? & !3)
}

/// The CLI metadata root of a managed PE image, without loading the image.
fn dotnet_metadata(bytes: &[u8]) -> Option<&[u8]> {
    let pe = usize::try_from(u32_at(bytes, 0x3C)?).ok()?;
    if bytes.get(pe..pe.checked_add(4)?)? != b"PE\0\0" {
        return None;
    }
    let coff = pe.checked_add(4)?;
    let sections = usize::from(u16_at(bytes, coff.checked_add(2)?)?);
    let optional_size = usize::from(u16_at(bytes, coff.checked_add(16)?)?);
    let optional = coff.checked_add(20)?;
    let directory = match u16_at(bytes, optional)? {
        0x10b => optional.checked_add(96)?,
        0x20b => optional.checked_add(112)?,
        _ => return None,
    };
    let cli_rva = usize::try_from(u32_at(bytes, directory.checked_add(14 * 8)?)?).ok()?;
    let table = optional.checked_add(optional_size)?;
    let mut headers = Vec::with_capacity(sections.min(96));
    for index in 0..sections.min(96) {
        let header = table.checked_add(index.checked_mul(40)?)?;
        headers.push(Section {
            virtual_address: u32_at(bytes, header.checked_add(12)?)?,
            virtual_size: u32_at(bytes, header.checked_add(8)?)?,
            raw_address: u32_at(bytes, header.checked_add(20)?)?,
            raw_size: u32_at(bytes, header.checked_add(16)?)?,
        });
    }
    let cli = rva_offset(&headers, cli_rva)?;
    let metadata_rva = usize::try_from(u32_at(bytes, cli.checked_add(8)?)?).ok()?;
    let metadata_size = usize::try_from(u32_at(bytes, cli.checked_add(12)?)?).ok()?;
    let metadata = rva_offset(&headers, metadata_rva)?;
    bytes.get(metadata..metadata.checked_add(metadata_size)?)
}

/// One PE section header, for RVA to file-offset mapping.
struct Section {
    virtual_address: u32,
    virtual_size: u32,
    raw_address: u32,
    raw_size: u32,
}

/// The file offset of an RVA.
fn rva_offset(sections: &[Section], rva: usize) -> Option<usize> {
    for section in sections {
        let start = usize::try_from(section.virtual_address).ok()?;
        let size = usize::try_from(section.virtual_size.max(section.raw_size)).ok()?;
        let end = start.checked_add(size)?;
        if rva >= start && rva < end {
            let raw = usize::try_from(section.raw_address).ok()?;
            return raw.checked_add(rva.checked_sub(start)?);
        }
    }
    None
}

/// The stream called `name` in a CLI metadata root.
fn metadata_stream<'a>(root: &'a [u8], name: &[u8]) -> Option<&'a [u8]> {
    if u32_at(root, 0)? != 0x424A_5342 {
        return None;
    }
    let version_length = usize::try_from(u32_at(root, 12)?).ok()?;
    let after_version = align4(16usize.checked_add(version_length)?)?;
    let streams = usize::from(u16_at(root, after_version.checked_add(2)?)?);
    let mut position = after_version.checked_add(4)?;
    for _ in 0..streams {
        let offset = usize::try_from(u32_at(root, position)?).ok()?;
        let size = usize::try_from(u32_at(root, position.checked_add(4)?)?).ok()?;
        let name_start = position.checked_add(8)?;
        let rest = root.get(name_start..)?;
        let name_length = rest.iter().position(|byte| *byte == 0)?;
        let stream_name = rest.get(..name_length)?;
        if stream_name == name {
            return root.get(offset..offset.checked_add(size)?);
        }
        position = name_start.checked_add(align4(name_length.checked_add(1)?)?)?;
    }
    None
}

/// The UTF-16 strings of an ECMA-335 `#US` heap, at most `max` usable keys. Index 0 is empty; each
/// entry is a compressed length (covering a terminal byte) followed by UTF-16LE code units. The cap
/// is applied here so a huge heap (up to 256 MB of tiny entries) does not build a huge vector first.
fn us_heap_strings(heap: &[u8], max: usize) -> Vec<String> {
    us_heap_strings_of(heap, max, None)
}

/// Like [`us_heap_strings`], but with `units` set only strings of exactly that many UTF-16 code
/// units are kept (and counted against `max`).
fn us_heap_strings_of(heap: &[u8], max: usize, units: Option<usize>) -> Vec<String> {
    let mut out = Vec::new();
    if max == 0 {
        return out;
    }
    let mut position = 1;
    while position < heap.len() && out.len() < max {
        let Some((length, used)) = compressed_uint(heap, position) else {
            break;
        };
        let Some(length) = usize::try_from(length).ok() else {
            break;
        };
        let Some(start) = position.checked_add(used) else {
            break;
        };
        if length == 0 {
            break;
        }
        let Some(end) = start.checked_add(length) else {
            break;
        };
        let Some(data) = heap.get(start..end) else {
            break;
        };
        // All byte counts are odd: the last byte is the terminal flag, the rest is UTF-16. Strings
        // that cannot be a key (empty, too long, control characters) are skipped without decoding
        // more than needed.
        if length % 2 == 1
            && units.is_none_or(|units| length / 2 == units)
            && let Some(text) = decode_utf16(data.get(..length - 1).unwrap_or_default())
            && valid_candidate(&text)
        {
            out.push(text);
        }
        position = end;
    }
    out
}

/// An ECMA-335 compressed unsigned integer at `offset`, with its byte length.
fn compressed_uint(bytes: &[u8], offset: usize) -> Option<(u32, usize)> {
    let first = u32::from(*bytes.get(offset)?);
    if first & 0x80 == 0 {
        return Some((first, 1));
    }
    let second = u32::from(*bytes.get(offset.checked_add(1)?)?);
    if first & 0xC0 == 0x80 {
        return Some((((first & 0x3F) << 8) | second, 2));
    }
    if first & 0xE0 == 0xC0 {
        let third = u32::from(*bytes.get(offset.checked_add(2)?)?);
        let fourth = u32::from(*bytes.get(offset.checked_add(3)?)?);
        let value = ((first & 0x1F) << 24) | (second << 16) | (third << 8) | fourth;
        return Some((value, 4));
    }
    None
}

/// A UTF-16LE byte string, or `None` when it holds an unpaired surrogate.
fn decode_utf16(bytes: &[u8]) -> Option<String> {
    let units = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_le_bytes(*pair))
        .collect::<Vec<u16>>();
    String::from_utf16(&units).ok()
}

/// The UTF-8 string literals of an IL2CPP `global-metadata.dat`.
fn il2cpp_strings(bytes: &[u8]) -> Vec<String> {
    if u32_at(bytes, 0) != Some(0xFAB1_1BAF) {
        return Vec::new();
    }
    let version = u32_at(bytes, 4).unwrap_or_default();
    if !(16..=1000).contains(&version) {
        return Vec::new();
    }
    let Some(table_offset) = u32_at(bytes, 8).and_then(|value| usize::try_from(value).ok()) else {
        return Vec::new();
    };
    let Some(table_size) = u32_at(bytes, 12).and_then(|value| usize::try_from(value).ok()) else {
        return Vec::new();
    };
    let Some(data_offset) = u32_at(bytes, 16).and_then(|value| usize::try_from(value).ok()) else {
        return Vec::new();
    };
    let Some(data_size) = u32_at(bytes, 20).and_then(|value| usize::try_from(value).ok()) else {
        return Vec::new();
    };
    let Some(table) =
        bytes.get(table_offset..table_offset.checked_add(table_size).unwrap_or_default())
    else {
        return Vec::new();
    };
    let Some(data) = bytes.get(data_offset..data_offset.checked_add(data_size).unwrap_or_default())
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for index in 0..table_size
        .checked_div(8)
        .unwrap_or_default()
        .min(MAX_CANDIDATES)
    {
        let Some(entry) = index
            .checked_mul(8)
            .and_then(|offset| table.get(offset..offset.checked_add(8)?))
        else {
            break;
        };
        let length = usize::try_from(u32_at(entry, 0).unwrap_or_default()).unwrap_or_default();
        let start = usize::try_from(u32_at(entry, 4).unwrap_or_default()).unwrap_or_default();
        if length == 0 || length > MAX_CANDIDATE_CHARS * 4 {
            continue;
        }
        let Some(literal) = start
            .checked_add(length)
            .and_then(|end| data.get(start..end))
        else {
            continue;
        };
        if let Ok(text) = std::str::from_utf8(literal) {
            out.push(text.to_string());
        }
    }
    out
}

/// A found key as it is written to the evidence file.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredKey {
    #[serde(default)]
    company: Option<String>,
    #[serde(default)]
    product: Option<String>,
    game_folder: String,
    key: Secret,
    source: String,
    method: String,
    date: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    kind: Option<String>,
}

/// The evidence file.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct KeyStore {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    games: Vec<StoredKey>,
    #[serde(default)]
    picked_folders: Vec<String>,
}

/// Reads the evidence file; a missing or broken file is empty.
fn load_store(path: &Path) -> KeyStore {
    read_file(path, MAX_STORE_BYTES)
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// Writes the evidence file atomically; a failure leaves the old file in place and is not reported
/// (the key may still be used for this open).
fn save_store(path: &Path, store: &KeyStore) {
    let Some(parent) = path.parent() else {
        return;
    };
    if fs::create_dir_all(parent).is_err() {
        return;
    }
    let Ok(bytes) = serde_json::to_vec_pretty(store) else {
        return;
    };
    let Ok(mut file) = tempfile::NamedTempFile::new_in(parent) else {
        return;
    };
    if std::io::Write::write_all(&mut file, &bytes).is_err() {
        return;
    }
    if file.as_file().sync_all().is_err() {
        return;
    }
    let _ = file.persist(path);
}

/// Today's date as `YYYY-MM-DD` (UTC), without a date dependency.
fn today() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs());
    let days = i64::try_from(seconds / 86_400).unwrap_or(0);
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}")
}

/// A civil date from a count of days since 1970-01-01 (Howard Hinnant's algorithm).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days.saturating_add(719_468);
    let era = if z >= 0 { z } else { z.saturating_sub(146_096) } / 146_097;
    let day_of_era = z.saturating_sub(era.saturating_mul(146_097));
    let year_of_era = (day_of_era
        .saturating_sub(day_of_era / 1460)
        .saturating_add(day_of_era / 36_524)
        .saturating_sub(day_of_era / 146_096))
        / 365;
    let year = year_of_era.saturating_add(era.saturating_mul(400));
    let day_of_year = day_of_era
        .saturating_sub(year_of_era.saturating_mul(365))
        .saturating_sub(year_of_era / 4)
        .saturating_add(year_of_era / 100);
    let month_prime = day_of_year.saturating_mul(5).saturating_add(2) / 153;
    let day = day_of_year
        .saturating_sub(month_prime.saturating_mul(153).saturating_add(2) / 5)
        .saturating_add(1);
    let month = month_prime.saturating_add(if month_prime < 10 { 3 } else { -9 });
    let year = year.saturating_add(i64::from(month <= 2));
    (
        year,
        u32::try_from(month).unwrap_or(1),
        u32::try_from(day).unwrap_or(1),
    )
}

#[cfg(test)]
mod tests {
    use saveprism_codecs::es3::Es3AesLayer;
    use saveprism_codecs::terraria::aes::TerrariaPlayerLayer;
    use saveprism_core::{Layer, Params};
    use std::fs;

    use super::*;
    use crate::session::Session;

    /// A minimal PE image whose metadata root has a `#US` heap with `strings`.
    fn assembly_with_user_strings(strings: &[&str]) -> Vec<u8> {
        let mut heap = vec![0_u8];
        for text in strings {
            let utf16: Vec<u8> = text.encode_utf16().flat_map(u16::to_le_bytes).collect();
            let length = utf16.len() + 1;
            if length < 0x80 {
                heap.push(u8::try_from(length).unwrap());
            } else {
                heap.push(u8::try_from(0x80 | (length >> 8)).unwrap());
                heap.push(u8::try_from(length & 0xFF).unwrap());
            }
            heap.extend_from_slice(&utf16);
            heap.push(0);
        }
        let mut metadata = b"BSJB".to_vec();
        metadata.extend_from_slice(&1_u16.to_le_bytes());
        metadata.extend_from_slice(&1_u16.to_le_bytes());
        metadata.extend_from_slice(&0_u32.to_le_bytes());
        metadata.extend_from_slice(&12_u32.to_le_bytes());
        metadata.extend_from_slice(b"v4.0.30319\0\0");
        metadata.extend_from_slice(&0_u16.to_le_bytes());
        metadata.extend_from_slice(&1_u16.to_le_bytes());
        let stream_offset = 44_u32;
        metadata.extend_from_slice(&stream_offset.to_le_bytes());
        metadata.extend_from_slice(&u32::try_from(heap.len()).unwrap().to_le_bytes());
        metadata.extend_from_slice(b"#US\0");
        metadata.extend_from_slice(&heap);
        pe_with_metadata(&metadata)
    }

    /// Wraps a metadata root in a one-section PE32 image.
    fn pe_with_metadata(metadata: &[u8]) -> Vec<u8> {
        let section_raw = 0x200usize;
        let cli_raw = 0x200usize;
        let metadata_raw = cli_raw + 72;
        let metadata_rva = 0x1000u32 + u32::try_from(metadata_raw - section_raw).unwrap();
        let cli_rva = 0x1000u32;
        let mut bytes = vec![0u8; metadata_raw + metadata.len()];
        bytes.get_mut(0..2).unwrap().copy_from_slice(b"MZ");
        bytes
            .get_mut(0x3C..0x40)
            .unwrap()
            .copy_from_slice(&0x40u32.to_le_bytes());
        bytes
            .get_mut(0x40..0x44)
            .unwrap()
            .copy_from_slice(b"PE\0\0");
        let coff = 0x44;
        bytes
            .get_mut(coff..coff + 2)
            .unwrap()
            .copy_from_slice(&0x14cu16.to_le_bytes());
        bytes
            .get_mut(coff + 2..coff + 4)
            .unwrap()
            .copy_from_slice(&1u16.to_le_bytes());
        bytes
            .get_mut(coff + 16..coff + 18)
            .unwrap()
            .copy_from_slice(&0xE0u16.to_le_bytes());
        let optional = 0x58;
        bytes
            .get_mut(optional..optional + 2)
            .unwrap()
            .copy_from_slice(&0x10bu16.to_le_bytes());
        let directory = optional + 96 + 14 * 8;
        bytes
            .get_mut(directory..directory + 4)
            .unwrap()
            .copy_from_slice(&cli_rva.to_le_bytes());
        bytes
            .get_mut(directory + 4..directory + 8)
            .unwrap()
            .copy_from_slice(&72u32.to_le_bytes());
        let section = optional + 0xE0;
        bytes
            .get_mut(section..section + 5)
            .unwrap()
            .copy_from_slice(b".text");
        bytes
            .get_mut(section + 8..section + 12)
            .unwrap()
            .copy_from_slice(&0x200u32.to_le_bytes());
        bytes
            .get_mut(section + 12..section + 16)
            .unwrap()
            .copy_from_slice(&0x1000u32.to_le_bytes());
        bytes
            .get_mut(section + 16..section + 20)
            .unwrap()
            .copy_from_slice(&u32::try_from(metadata.len() + 72).unwrap().to_le_bytes());
        bytes
            .get_mut(section + 20..section + 24)
            .unwrap()
            .copy_from_slice(&u32::try_from(section_raw).unwrap().to_le_bytes());
        let cli = cli_raw;
        bytes
            .get_mut(cli..cli + 4)
            .unwrap()
            .copy_from_slice(&72u32.to_le_bytes());
        bytes
            .get_mut(cli + 8..cli + 12)
            .unwrap()
            .copy_from_slice(&metadata_rva.to_le_bytes());
        bytes
            .get_mut(cli + 12..cli + 16)
            .unwrap()
            .copy_from_slice(&u32::try_from(metadata.len()).unwrap().to_le_bytes());
        bytes
            .get_mut(metadata_raw..metadata_raw + metadata.len())
            .unwrap()
            .copy_from_slice(metadata);
        bytes
    }

    /// A minimal IL2CPP metadata file with `strings` in its literal heap.
    fn il2cpp_metadata(strings: &[&str]) -> Vec<u8> {
        let table_offset = 48u32;
        let data_offset = table_offset + u32::try_from(strings.len() * 8).unwrap();
        let mut data = Vec::new();
        let mut table = Vec::new();
        for text in strings {
            table.extend_from_slice(&u32::try_from(text.len()).unwrap().to_le_bytes());
            table.extend_from_slice(&i32::try_from(data.len()).unwrap().to_le_bytes());
            data.extend_from_slice(text.as_bytes());
        }
        let mut out = vec![0u8; usize::try_from(table_offset).unwrap()];
        out.get_mut(0..4)
            .unwrap()
            .copy_from_slice(&0xFAB1_1BAFu32.to_le_bytes());
        out.get_mut(4..8)
            .unwrap()
            .copy_from_slice(&29u32.to_le_bytes());
        out.get_mut(8..12)
            .unwrap()
            .copy_from_slice(&table_offset.to_le_bytes());
        out.get_mut(12..16)
            .unwrap()
            .copy_from_slice(&u32::try_from(table.len()).unwrap().to_le_bytes());
        out.get_mut(16..20)
            .unwrap()
            .copy_from_slice(&data_offset.to_le_bytes());
        out.get_mut(20..24)
            .unwrap()
            .copy_from_slice(&u32::try_from(data.len()).unwrap().to_le_bytes());
        out.extend_from_slice(&table);
        out.extend_from_slice(&data);
        out
    }

    /// `plain` encrypted for `password` with a fixed IV.
    fn encrypted(plain: &[u8], password: &str) -> Vec<u8> {
        let mut params = Params::new();
        params.set("password", password);
        params.set("iv", vec![7_u8; 16]);
        Es3AesLayer.encode(plain, &params).unwrap()
    }

    fn write(path: &Path, bytes: &[u8]) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, bytes).unwrap();
    }

    fn open_with(
        save: &Path,
        finder: KeyFinder,
        store: &Path,
    ) -> Result<Session, crate::session::ApiError> {
        Session::open_with_finder(
            save.to_str().unwrap(),
            None,
            Some(store.parent().unwrap().join("backups")),
            Some(finder),
        )
    }

    #[test]
    fn the_ecma_us_heap_yields_utf16_strings() {
        let image = assembly_with_user_strings(&["alpha", "GameSecret42"]);
        let metadata = dotnet_metadata(&image).unwrap();
        let heap = metadata_stream(metadata, b"#US").unwrap();
        let strings = us_heap_strings(heap, 100);
        assert_eq!(strings, ["alpha", "GameSecret42"]);
    }

    /// A bare `#US` heap with `count` short entries.
    fn us_heap_with_entries(count: usize) -> Vec<u8> {
        let mut heap = vec![0_u8];
        for index in 0..count {
            let utf16: Vec<u8> = format!("k{index}")
                .encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect();
            let length = utf16.len() + 1;
            heap.push(u8::try_from(length).unwrap());
            heap.extend_from_slice(&utf16);
            heap.push(0);
        }
        heap
    }

    #[test]
    fn a_huge_us_heap_stops_at_the_cap() {
        let heap = us_heap_with_entries(5_000);
        let strings = us_heap_strings(&heap, 25);
        assert_eq!(strings.len(), 25);
        assert_eq!(strings.first().map(String::as_str), Some("k0"));
        // The cap is honest: no entry may be decoded past it into the result.
        assert_eq!(us_heap_strings(&heap, 0).len(), 0);
        // Strings that cannot be a key are not collected, so the cap counts usable keys only.
        assert!(us_heap_strings(&heap, 1).len() <= 1);
    }

    #[test]
    fn the_il2cpp_literal_table_yields_utf8_strings() {
        let metadata = il2cpp_metadata(&["first", "GameSecret42"]);
        assert_eq!(il2cpp_strings(&metadata), ["first", "GameSecret42"]);
        // A file without the sanity value or with a silly version is not metadata.
        assert!(il2cpp_strings(b"not metadata").is_empty());
    }

    /// `Users\me\AppData\LocalLow\Studio\MyGame\save.es3`, built with this system's separator.
    fn local_low_save() -> std::path::PathBuf {
        [
            "Users", "me", "AppData", "LocalLow", "Studio", "MyGame", "save.es3",
        ]
        .iter()
        .collect()
    }

    #[test]
    fn a_local_low_save_names_its_company_and_product() {
        let save = local_low_save();
        assert_eq!(
            local_low_parts(&save),
            Some(("Studio".to_string(), "MyGame".to_string()))
        );
        assert_eq!(local_low_parts(Path::new("C:\\Games\\save.es3")), None);
    }

    #[test]
    fn app_info_matches_the_local_low_pair_and_steam_libraries() {
        let root = tempfile::tempdir().unwrap();
        let steam = root.path().join("Steam");
        let game = steam.join("steamapps").join("common").join("MyGame");
        let data = game.join("MyGame_Data");
        write(&data.join("app.info"), b"Studio\r\nMyGame\r\n");
        write(
            &data.join("Managed").join("Assembly-CSharp.dll"),
            &assembly_with_user_strings(&["GameSecret42"]),
        );
        let vdf = steam.join("steamapps").join("libraryfolders.vdf");
        write(
            &vdf,
            b"\"libraryfolders\"\n{\n \"0\" { \"path\" \"C:\\\\Steam\" }\n}\n",
        );
        let finder =
            KeyFinder::for_test(root.path().join("keys.json")).with_steam_root(steam.clone());
        let candidates = finder.search(&local_low_save());
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates.first().unwrap().key(), "GameSecret42");
        assert_eq!(
            candidates.first().unwrap().source,
            data.join("Managed").join("Assembly-CSharp.dll")
        );
    }

    #[test]
    fn a_key_is_found_opened_and_stored_with_evidence() {
        let root = tempfile::tempdir().unwrap();
        let steam = root.path().join("Steam");
        let game_data = steam
            .join("steamapps")
            .join("common")
            .join("MyGame")
            .join("MyGame_Data");
        write(&game_data.join("app.info"), b"Studio\nMyGame\n");
        write(
            &game_data.join("Managed").join("Assembly-CSharp.dll"),
            &assembly_with_user_strings(&["not-it", "GameSecret42"]),
        );
        let plain = br#"{"slot":{"__type":"int","value":3}}"#;
        let save = root
            .path()
            .join("AppData")
            .join("LocalLow")
            .join("Studio")
            .join("MyGame")
            .join("save.es3");
        write(&save, &encrypted(plain, "GameSecret42"));
        let store = root.path().join("app data").join("keys.json");
        let session = open_with(
            &save,
            KeyFinder::for_test(store.clone()).with_steam_root(steam),
            &store,
        )
        .unwrap();
        assert_eq!(session.info().guard, crate::session::GuardLevel::Exact);

        let text = fs::read_to_string(&store).unwrap();
        assert!(text.contains("GameSecret42"), "{text}");
        assert!(text.contains("dotnet-us-heap"), "{text}");
        assert!(text.contains("Assembly-CSharp.dll"), "{text}");
        assert!(text.contains("Studio"), "{text}");
        assert!(text.contains("MyGame"), "{text}");
    }

    #[test]
    fn a_stored_key_opens_without_reading_the_game() {
        let root = tempfile::tempdir().unwrap();
        let save = root
            .path()
            .join("AppData")
            .join("LocalLow")
            .join("Studio")
            .join("MyGame")
            .join("save.es3");
        let plain = format!("{{\"k\":\"{}\"}}", "x".repeat(400));
        write(&save, &encrypted(plain.as_bytes(), "GameSecret42"));
        let store_path = root.path().join("keys.json");
        let mut store = KeyStore {
            version: STORE_VERSION,
            ..KeyStore::default()
        };
        store.games.push(StoredKey {
            company: Some("Studio".into()),
            product: Some("MyGame".into()),
            game_folder: root.path().join("nowhere").to_string_lossy().into_owned(),
            key: Secret("GameSecret42".into()),
            source: "earlier run".into(),
            method: METHOD_US_HEAP.into(),
            date: today(),
            kind: None,
        });
        save_store(&store_path, &store);

        let session =
            open_with(&save, KeyFinder::for_test(store_path.clone()), &store_path).unwrap();
        assert_eq!(session.info().guard, crate::session::GuardLevel::Exact);
        assert_eq!(load_store(&store_path).games.len(), 1);
    }

    #[test]
    fn a_key_that_is_not_there_asks_for_the_password() {
        let root = tempfile::tempdir().unwrap();
        let game_data = root
            .path()
            .join("Steam")
            .join("steamapps")
            .join("common")
            .join("MyGame")
            .join("MyGame_Data");
        write(&game_data.join("app.info"), b"Studio\nMyGame\n");
        write(
            &game_data.join("Managed").join("Assembly-CSharp.dll"),
            &assembly_with_user_strings(&["wrong-key"]),
        );
        let save = root
            .path()
            .join("AppData")
            .join("LocalLow")
            .join("Studio")
            .join("MyGame")
            .join("save.es3");
        write(
            &save,
            &encrypted(
                format!("{{\"k\":\"{}\"}}", "x".repeat(400)).as_bytes(),
                "GameSecret42",
            ),
        );
        let store = root.path().join("keys.json");
        let error = open_with(
            &save,
            KeyFinder::for_test(store.clone()).with_steam_root(root.path().join("Steam")),
            &store,
        )
        .err()
        .unwrap();
        assert_eq!(error.code, "SP-ES3-005");
        assert!(!fs::exists(store).unwrap_or(false));
    }

    #[test]
    fn a_picked_folder_is_tried_even_without_app_info() {
        let root = tempfile::tempdir().unwrap();
        let game = root.path().join("Somewhere").join("MyGame");
        let data = game.join("MyGame_Data");
        write(&data.join("app.info"), b"Other\nMyGame\n");
        write(
            &data.join("Managed").join("Assembly-CSharp.dll"),
            &assembly_with_user_strings(&["GameSecret42"]),
        );
        let save = root.path().join("elsewhere").join("save.es3");
        write(
            &save,
            &encrypted(
                format!("{{\"k\":\"{}\"}}", "x".repeat(400)).as_bytes(),
                "GameSecret42",
            ),
        );
        let store = root.path().join("keys.json");
        let session = open_with(
            &save,
            KeyFinder::for_test(store.clone()).with_folder(game),
            &store,
        )
        .unwrap();
        assert_eq!(session.info().guard, crate::session::GuardLevel::Exact);
    }

    #[test]
    fn candidates_are_deduped_and_stop_at_the_limit() {
        let mut collector = Collector::new(PathBuf::from("game"), 3, GameKind::Es3);
        for text in ["one", "two", "one", "three", "four"] {
            collector.add(text, Path::new("source"), METHOD_US_HEAP);
        }
        let out = collector.take();
        let keys: Vec<&str> = out.iter().map(Candidate::key).collect();
        assert_eq!(keys, ["one", "two", "three"]);
        assert_eq!(
            format!("{:?}", out.first().unwrap()),
            "Candidate { key: <redacted>, source: \"source\", game_folder: \"game\", method: \"dotnet-us-heap\", kind: \"es3\" }"
        );
    }

    /// A Steam library with `count` game folders, each padded with files so a shared directory
    /// budget would run out before the end. Returns the root.
    fn padded_steam_library(root: &Path, count: usize) -> PathBuf {
        let steam = root.join("Steam");
        let common = steam.join("steamapps").join("common");
        for index in 0..count {
            let game = common.join(format!("Game{index:03}"));
            let data = game.join(format!("Game{index:03}_Data"));
            write(&data.join("app.info"), b"Other\nSomethingElse\n");
            for filler in 0..24 {
                write(&game.join(format!("filler{filler:02}.bin")), b"x");
            }
        }
        steam
    }

    #[test]
    fn a_big_steam_library_still_finds_the_matching_game() {
        let root = tempfile::tempdir().unwrap();
        let steam = padded_steam_library(root.path(), 200);
        // Alphabetically last, and got plenty of budget spent before it by the padded folders.
        let game = steam.join("steamapps").join("common").join("zzz-Target");
        let data = game.join("Target_Data");
        write(&data.join("app.info"), b"Studio\nMyGame\n");
        write(
            &data.join("Managed").join("Assembly-CSharp.dll"),
            &assembly_with_user_strings(&["GameSecret42"]),
        );
        let finder =
            KeyFinder::for_test(root.path().join("keys.json")).with_steam_root(steam.clone());
        let save = root
            .path()
            .join("LocalLow")
            .join("Studio")
            .join("MyGame")
            .join("save.es3");
        let candidates = finder.search(&save);
        let keys: Vec<&str> = candidates.iter().map(Candidate::key).collect();
        assert!(keys.contains(&"GameSecret42"), "{keys:?}");
    }

    #[test]
    fn a_picked_folder_still_resolves_after_a_big_steam_scan() {
        let root = tempfile::tempdir().unwrap();
        let steam = padded_steam_library(root.path(), 200);
        let picked = root.path().join("Picked").join("TheGame");
        let data = picked.join("TheGame_Data");
        write(&data.join("app.info"), b"Unknown\nOther\n");
        write(
            &data.join("Managed").join("Assembly-CSharp.dll"),
            &assembly_with_user_strings(&["GameSecret42"]),
        );
        let finder = KeyFinder::for_test(root.path().join("keys.json"))
            .with_folder(picked)
            .with_steam_root(steam);
        let save = root
            .path()
            .join("LocalLow")
            .join("Studio")
            .join("MyGame")
            .join("save.es3");
        let candidates = finder.search(&save);
        let keys: Vec<&str> = candidates.iter().map(Candidate::key).collect();
        assert!(keys.contains(&"GameSecret42"), "{keys:?}");
    }

    #[test]
    fn vdf_libraries_are_capped_deduped_and_drive_paths_only() {
        let mut text = String::from("\"libraryfolders\"\n{\n");
        for index in 0..40 {
            text.push_str(&format!(
                " \"{index}\" {{ \"path\" \"D:\\\\Library{index}\" }}\n"
            ));
        }
        text.push_str(
            " \"dup\" { \"path\" \"D:\\\\Library0\" }\n\
             \"unc\" { \"path\" \"\\\\\\\\server\\\\share\" }\n\
             \"rel\" { \"path\" \"relative/folder\" }\n}\n",
        );
        let paths = vdf_paths(&text);
        assert_eq!(paths.len(), MAX_LIBRARIES);
        assert!(paths.iter().all(|path| path.starts_with("D:\\Library")));
        let mut unique = paths.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), paths.len());
        assert!(looks_like_path("C:\\Steam"));
        assert!(looks_like_path("c:/steam"));
        assert!(!looks_like_path("\\\\server\\share"));
        assert!(!looks_like_path("relative/folder"));
        assert!(!looks_like_path("12345"));
    }

    #[test]
    fn a_directory_budget_stops_the_walk() {
        let mut budget = Budget::new(2);
        let dir = tempfile::tempdir().unwrap();
        for index in 0..5 {
            fs::write(dir.path().join(format!("{index}.txt")), b"x").unwrap();
        }
        assert_eq!(read_dir_bounded(dir.path(), &mut budget).len(), 2);
        assert!(!budget.take());
    }

    #[test]
    fn the_store_round_trips_a_redacted_key() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("keys.json");
        let mut store = KeyStore::default();
        store.games.push(StoredKey {
            company: Some("Studio".into()),
            product: Some("MyGame".into()),
            game_folder: "C:\\Games\\MyGame".into(),
            key: Secret("GameSecret42".into()),
            source: "C:\\Games\\MyGame\\MyGame_Data\\Managed\\Assembly-CSharp.dll".into(),
            method: METHOD_US_HEAP.into(),
            date: today(),
            kind: None,
        });
        save_store(&path, &store);
        let loaded = load_store(&path);
        assert_eq!(loaded.games.first().unwrap().key.as_str(), "GameSecret42");
        assert!(!format!("{:?}", loaded.games.first().unwrap().key).contains("GameSecret42"));
    }

    #[test]
    fn dates_are_civil() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19_000), (2022, 1, 8));
    }

    #[test]
    fn a_terraria_key_is_read_from_the_games_own_exe() {
        let root = tempfile::tempdir().unwrap();
        let steam = root.path().join("Steam");
        let game = steam.join("steamapps").join("common").join("Terraria");
        write(
            &game.join("Terraria.exe"),
            &assembly_with_user_strings(&["not-the-key", "FakeKey1"]),
        );
        let plain = fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../fixtures/synthetic/terraria/player-decrypted.plr"),
        )
        .unwrap();
        let mut params = Params::new();
        params.set("password", "FakeKey1");
        let encrypted = TerrariaPlayerLayer.encode(&plain, &params).unwrap();
        let save = root
            .path()
            .join("Documents")
            .join("My Games")
            .join("Terraria")
            .join("Players")
            .join("Mira.plr");
        write(&save, &encrypted);

        let store = root.path().join("app data").join("keys.json");
        let finder = KeyFinder::for_test(store.clone()).with_steam_root(steam.clone());
        let candidates = finder.search(&save);
        assert!(
            candidates
                .iter()
                .any(|candidate| candidate.key() == "FakeKey1" && candidate.is_terraria()),
            "{candidates:?}"
        );

        let session = open_with(&save, finder, &store).unwrap();
        assert_eq!(session.info().guard, crate::session::GuardLevel::Exact);
        let text = fs::read_to_string(&store).unwrap();
        assert!(text.contains("FakeKey1"), "{text}");
        assert!(text.contains("terraria-us-heap"), "{text}");
        assert!(text.contains("Terraria.exe"), "{text}");
        assert!(text.contains("\"kind\": \"terraria\""), "{text}");

        // The stored key opens it again without reading the game folder.
        let again = open_with(&save, KeyFinder::for_test(store.clone()), &store).unwrap();
        assert_eq!(again.info().guard, crate::session::GuardLevel::Exact);
    }

    #[test]
    fn the_games_own_plr_backup_opens_like_the_plr() {
        assert!(is_terraria_save(Path::new(r"C:\Saves\Mira.plr")));
        assert!(is_terraria_save(Path::new(r"C:\Saves\Mira.PLR.BAK")));
        assert!(!is_terraria_save(Path::new(r"C:\Saves\Mira.bak")));
        assert!(!is_terraria_save(Path::new(r"C:\Saves\slot1.es3")));

        let root = tempfile::tempdir().unwrap();
        let steam = root.path().join("Steam");
        let game = steam.join("steamapps").join("common").join("Terraria");
        write(
            &game.join("Terraria.exe"),
            &assembly_with_user_strings(&["FakeKey1"]),
        );
        let plain = fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../fixtures/synthetic/terraria/player-decrypted.plr"),
        )
        .unwrap();
        let mut params = Params::new();
        params.set("password", "FakeKey1");
        let encrypted = TerrariaPlayerLayer.encode(&plain, &params).unwrap();
        // Outside the game's folders, so only the file name says it is a Terraria player.
        let save = root.path().join("Backups").join("Mira.plr.bak");
        write(&save, &encrypted);

        let store = root.path().join("app data").join("keys.json");
        let finder = KeyFinder::for_test(store.clone()).with_steam_root(steam);
        let session = open_with(&save, finder, &store).unwrap();
        assert_eq!(session.info().guard, crate::session::GuardLevel::Exact);
    }

    #[test]
    fn the_terraria_key_is_found_behind_more_strings_than_the_cap() {
        // More other strings than the cap, a decoy of the right length, then the real key.
        let mut strings: Vec<String> = (0..MAX_CANDIDATES + 100)
            .map(|index| format!("k{index}"))
            .collect();
        strings.push("DecoyKey".to_string());
        strings.push("RealKey8".to_string());
        let texts: Vec<&str> = strings.iter().map(String::as_str).collect();
        let image = assembly_with_user_strings(&texts);

        // Without the length filter the cap is spent on the other strings.
        let heap = metadata_stream(dotnet_metadata(&image).unwrap(), b"#US").unwrap();
        assert!(!us_heap_strings(heap, MAX_CANDIDATES).contains(&"RealKey8".to_string()));
        let keys = us_heap_strings_of(heap, MAX_CANDIDATES, Some(TERRARIA_KEY_UNITS));
        assert_eq!(keys, ["DecoyKey", "RealKey8"]);

        // The same through the Terraria source: only 8-unit strings are candidates.
        let root = tempfile::tempdir().unwrap();
        let exe = root.path().join("Terraria.exe");
        write(&exe, &image);
        let mut files = TerrariaFiles::new(root.path().to_path_buf(), exe, MAX_CANDIDATES);
        let found: Vec<String> = files
            .candidates()
            .iter()
            .map(|candidate| candidate.key().to_string())
            .collect();
        assert_eq!(found, ["DecoyKey", "RealKey8"]);
    }
}
