//! Ren'Py saves (`1-1-LT1.save`) and the `persistent` file as a Values list.
//!
//! A save is a zip whose `log` entry is the pickle of `(roots, log)` (see
//! `saveprism_codecs::renpy` and `saveprism_codecs::pickle`). `roots` maps `store.<name>` and
//! `store.<namespace>.<name>` to the game's variables; the view lists them as `<name>` and
//! `<namespace>.<name>`, the game's own variables first (by name) and Ren'Py's internals after them.
//! Numbers, on/off values and texts are edited in place; lists, dicts and game objects open as
//! groups; a value stored under two names points at its first place.
//!
//! Signatures (Ren'Py 7.6 / 8.1 and later): a file that was signed before is signed again with the
//! player's own `security_keys.txt` when it is found, so the game takes an edited save without
//! asking whether to trust it, and an edited `persistent` file is not thrown away. Without a key
//! file an edited save gets an empty `signatures` entry (the game asks the player to trust it) and a
//! signed `persistent` file stays read-only, because Ren'Py would throw an edited copy away. A file
//! that carried no signature never gets one, and a signed `persistent` file nobody changed keeps
//! the signature bytes it has. The key file is looked for only when a signed Ren'Py file is opened
//! (never for another format), read at most [`MAX_KEY_FILE_BYTES`], and its text lives in memory
//! while a save is written; it never enters the recipe and its `Debug` is redacted. Nothing is
//! executed.

use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::io::Read as _;
use std::path::{Path as FsPath, PathBuf};

use saveprism_codecs::pickle::python_class;
use saveprism_codecs::renpy::sign;
use saveprism_codecs::renpy::{RENPY_KEY, SIGNATURES_PARAM, SIGNED_PARAM};
use saveprism_core::{Node, Path, Recipe, Value, pipeline};

use super::game::{At, Ctx, MAX_VALUE_ROWS, NoteKind, RowNote, UNLIMITED, ValueRow};
use super::{ApiError, Session, app_codes, child_count, read_only_document};

/// The Ren'Py save layer.
const SAVE_LAYER: &str = "container.renpy-save";
/// The pickle codec.
const PICKLE_CODEC: &str = "format.python-pickle";
/// The zlib layer of the `persistent` file.
const ZLIB_LAYER: &str = "compression.zlib";
/// The zlib layer's parameter with the bytes after the compressed data (the signature text).
const TRAILING_PARAM: &str = "trailing";
/// The class of the object pickled into `persistent`.
const PERSISTENT_CLASS: &str = "renpy.persistent.Persistent";
/// Notice for a signed save: the game asks whether to trust the edited save.
pub const NOTICE_SIGNED_SAVE: &str = "renpy-signed";
/// Notice for a signed `persistent` file: it is open read-only.
pub const NOTICE_SIGNED_PERSISTENT: &str = "renpy-persistent-signed";
/// Notice when the player's own key signs the edited save.
pub const NOTICE_KEY_SIGNED: &str = "renpy-key-signed";
/// Notice when the player's own key signs the edited `persistent` data.
pub const NOTICE_KEY_SIGNED_PERSISTENT: &str = "renpy-persistent-key-signed";

/// File name of Ren'Py's key list (`savetoken.init_tokens`).
const KEY_FILE: &str = "security_keys.txt";
/// The folder of the key file inside Ren'Py's save location: `path_to_saves(gamedir, "tokens")`.
const TOKEN_DIR: &str = "tokens";
/// Largest key file read; a bigger file is treated as no key file at all.
const MAX_KEY_FILE_BYTES: u64 = 64 * 1024;
/// The folder `renpy.py:path_to_saves` prefers when it exists above the game.
const PORTABLE_DIR: &str = "Ren'Py Data";
/// The environment variable that moves Ren'Py's save location (`renpy.py:path_to_saves`).
const SAVES_VAR: &str = "RENPY_PATH_TO_SAVES";

/// The systems whose default save location differs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Platform {
    Windows,
    MacOs,
    Other,
}

impl Platform {
    const CURRENT: Platform = if cfg!(target_os = "windows") {
        Platform::Windows
    } else if cfg!(target_os = "macos") {
        Platform::MacOs
    } else {
        Platform::Other
    };
}

/// The text of a key file. Its `Debug` is redacted, so the private keys in it cannot reach a log, a
/// panic message or an error by accident.
struct KeyText(String);

impl fmt::Debug for KeyText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted>")
    }
}

/// Where the player's key file comes from. Production reads the one folder Ren'Py itself uses;
/// tests name a file of their own, turn the lookup off, or count the calls.
#[derive(Clone, Debug)]
pub(super) enum RenpyKeySource {
    /// The token folder `renpy.py:path_to_saves` would use on this system.
    Default,
    /// An explicit file (or none); tests only.
    #[cfg(test)]
    At(Option<PathBuf>),
    /// Like `At`, and counts how often it was asked; tests only.
    #[cfg(test)]
    Recording {
        calls: std::sync::Arc<std::sync::atomic::AtomicUsize>,
        path: Option<PathBuf>,
    },
}

impl RenpyKeySource {
    /// The key file to use, when it exists and holds at least one `signing-key` line. `save` is the
    /// open file, used to find a portable `Ren'Py Data` folder around it.
    pub(super) fn resolve(&self, save: &FsPath) -> Option<PathBuf> {
        match self {
            RenpyKeySource::Default => {
                let dir = token_dir(save, Platform::CURRENT, &|name| std::env::var_os(name))?;
                let file = dir.join(KEY_FILE);
                key_file_works(&file).then_some(file)
            }
            #[cfg(test)]
            RenpyKeySource::At(path) => path.as_ref().filter(|path| key_file_works(path)).cloned(),
            #[cfg(test)]
            RenpyKeySource::Recording { calls, path } => {
                calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                path.as_ref().filter(|path| key_file_works(path)).cloned()
            }
        }
    }
}

/// The folder Ren'Py keeps its tokens in, `path_to_saves(gamedir, "tokens")` of `renpy.py`. Ren'Py
/// takes the first rule that applies and nothing else:
/// 1. `RENPY_PATH_TO_SAVES`, when set: `$RENPY_PATH_TO_SAVES/tokens`.
/// 2. A `Ren'Py Data` folder above the game: `<that folder>/tokens`. Ren'Py searches upward from its
///    own folder, which is not known here; a save inside such a folder is where that rule put it.
/// 3. The system default: `%APPDATA%/RenPy/tokens` (or `~/RenPy/tokens` without `APPDATA`) on
///    Windows, `~/Library/RenPy/tokens` on macOS and `~/.renpy/tokens` elsewhere.
///
/// `var` reads an environment variable; an empty value counts as not set.
fn token_dir(
    save: &FsPath,
    platform: Platform,
    var: &dyn Fn(&str) -> Option<OsString>,
) -> Option<PathBuf> {
    let var = |name: &str| var(name).filter(|value| !value.is_empty());
    let home_under = |home: OsString, parts: &[&str]| {
        let mut path = PathBuf::from(home);
        path.extend(parts);
        path.push(TOKEN_DIR);
        path
    };
    if let Some(base) = var(SAVES_VAR) {
        return Some(PathBuf::from(base).join(TOKEN_DIR));
    }
    // The file systems of Windows and macOS ignore case, so Ren'Py's `isdir` check does too.
    let same_name = |name: &std::ffi::OsStr| {
        if platform == Platform::Other {
            name == PORTABLE_DIR
        } else {
            name.eq_ignore_ascii_case(PORTABLE_DIR)
        }
    };
    for ancestor in save.ancestors().skip(1) {
        if ancestor.file_name().is_some_and(same_name) {
            return Some(ancestor.join(TOKEN_DIR));
        }
    }
    match platform {
        Platform::Windows => match var("APPDATA") {
            Some(appdata) => Some(home_under(appdata, &["RenPy"])),
            None => var("USERPROFILE").map(|home| home_under(home, &["RenPy"])),
        },
        Platform::MacOs => var("HOME").map(|home| home_under(home, &["Library", "RenPy"])),
        Platform::Other => var("HOME").map(|home| home_under(home, &[".renpy"])),
    }
}

/// Reads a key file, at most [`MAX_KEY_FILE_BYTES`]; a bigger, unreadable or non-regular file is no key file.
fn read_key_file(path: &FsPath) -> Option<KeyText> {
    if !fs::metadata(path).ok()?.is_file() {
        return None;
    }
    let file = fs::File::open(path).ok()?;
    let mut bytes = Vec::new();
    file.take(MAX_KEY_FILE_BYTES.saturating_add(1))
        .read_to_end(&mut bytes)
        .ok()?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_KEY_FILE_BYTES {
        return None;
    }
    String::from_utf8(bytes).ok().map(KeyText)
}

/// True when `path` is a readable key file with at least one usable signing key.
fn key_file_works(path: &FsPath) -> bool {
    read_key_file(path).is_some_and(|text| !sign::parse_key_file(&text.0).is_empty())
}

/// The signing keys of the key file at `path`, or the error for a file that was found when the save
/// was opened but cannot be used now.
fn load_keys(path: &FsPath) -> Result<Vec<sign::Key>, ApiError> {
    let keys = read_key_file(path)
        .map(|text| sign::parse_key_file(&text.0))
        .unwrap_or_default();
    if keys.is_empty() {
        return Err(key_error(path));
    }
    Ok(keys)
}

/// The error for a key file that cannot be read as a Ren'Py key file.
fn key_error(path: &FsPath) -> ApiError {
    ApiError::new(
        RENPY_KEY.as_str(),
        format!(
            "\"{}\" could not be read as a Ren'Py key file",
            path.display()
        ),
        "Check that security_keys.txt exists and has a signing-key line, then open the save again.",
    )
}

/// First name parts of the variables Ren'Py itself keeps in the store.
const INTERNAL_NAMES: [&str; 40] = [
    "achievement",
    "adv",
    "anim",
    "audio",
    "bubble",
    "build",
    "centered",
    "config",
    "default_mouse",
    "default_transition",
    "director",
    "extend",
    "gui",
    "iap",
    "im",
    "layeredimage",
    "layout",
    "library",
    "main_menu",
    "menu",
    "menu_window",
    "mouse",
    "mouse_visible",
    "name_only",
    "narrator",
    "nvl",
    "persistent",
    "predict_menu",
    "predict_say",
    "preferences",
    "renpy",
    "save_name",
    "say",
    "store",
    "style",
    "suppress_overlay",
    "theme",
    "ui",
    "updater",
    "vcentered",
];

/// What kind of Ren'Py file is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RenpyFile {
    /// A save; `signed` when it had a non-empty `signatures` entry.
    Save { signed: bool },
    /// The `persistent` file; `signed` when a signature follows the zlib data.
    Persistent { signed: bool },
}

/// True for a variable Ren'Py keeps for itself: a name starting with `_`, a known Ren'Py name or
/// namespace (`config`, `nvl_list`, ...), or a value of one of Ren'Py's own classes (characters,
/// displayables), except its revertable containers that hold game data.
fn is_internal(name: &str, node: Option<&Node>) -> bool {
    let first = name.split('.').next().unwrap_or(name);
    if first.starts_with('_') || first.starts_with("nvl_") || INTERNAL_NAMES.contains(&first) {
        return true;
    }
    node.and_then(python_class).is_some_and(|class| {
        class.starts_with("renpy.")
            && !class
                .rsplit('.')
                .next()
                .unwrap_or("")
                .starts_with("Revertable")
    })
}

impl Ctx<'_> {
    /// One row: a field for a number, on/off or text; a group for a container; a note otherwise.
    fn renpy_row(&self, name: String, at: &At) -> ValueRow {
        let note = |kind| ValueRow {
            name: name.clone(),
            path: at.1.to_string(),
            field: None,
            children: 0,
            preview: String::new(),
            note: Some(RowNote {
                kind,
                count: None,
                text: None,
            }),
            addable: false,
            removable: false,
        };
        let Some(node) = self.doc.get(at.0) else {
            return note(NoteKind::Code);
        };
        match &node.value {
            Value::Null => note(NoteKind::Empty),
            value @ (Value::Object(_) | Value::Array(_) | Value::Map(_)) => {
                let children = child_count(value);
                if children == 0 {
                    return note(NoteKind::Empty);
                }
                ValueRow {
                    name,
                    path: at.1.to_string(),
                    field: None,
                    children,
                    preview: String::new(),
                    note: None,
                    addable: false,
                    removable: false,
                }
            }
            _ => match self.field(at, UNLIMITED) {
                Some(field) => ValueRow {
                    name,
                    path: at.1.to_string(),
                    field: Some(field),
                    children: 0,
                    preview: String::new(),
                    note: None,
                    addable: false,
                    removable: false,
                },
                None => note(NoteKind::Code),
            },
        }
    }

    /// The store variables of a save (`roots`, the first item of the root tuple) or the fields of
    /// the persistent object; `None` when the tree does not have that shape.
    pub(super) fn renpy_rows(&self, file: RenpyFile) -> Option<Vec<ValueRow>> {
        let root: At = (self.doc.root(), Path::root());
        let (table, strip) = match file {
            RenpyFile::Save { .. } => (self.items(&root).into_iter().next()?.1, true),
            RenpyFile::Persistent { .. } => (root, false),
        };
        let Some(Value::Object(entries)) = self.doc.get(table.0).map(|node| &node.value) else {
            return None;
        };
        let mut game = Vec::new();
        let mut internal = Vec::new();
        for (key, child) in entries {
            if game.len() + internal.len() >= MAX_VALUE_ROWS {
                break;
            }
            let name = if strip {
                key.strip_prefix("store.").unwrap_or(key)
            } else {
                key.as_str()
            };
            let mut path = table.1.clone();
            path.push(key.as_str());
            let at = self.follow((*child, path));
            let row = self.renpy_row(name.to_string(), &at);
            if is_internal(name, self.doc.get(at.0)) {
                internal.push(row);
            } else {
                game.push(row);
            }
        }
        let by_name = |a: &ValueRow, b: &ValueRow| {
            a.name
                .to_lowercase()
                .cmp(&b.name.to_lowercase())
                .then_with(|| a.name.cmp(&b.name))
        };
        game.sort_by(by_name);
        internal.sort_by(by_name);
        game.extend(internal);
        Some(game)
    }
}

impl Session {
    /// The kind of Ren'Py file that is open, from the recipe and the tree; `None` for other files.
    pub(super) fn renpy_file(&self) -> Option<RenpyFile> {
        let steps = &self.recipe.steps;
        if steps.last()?.kind != PICKLE_CODEC {
            return None;
        }
        if let Some(step) = steps.iter().find(|step| step.kind == SAVE_LAYER) {
            let signed = step.params.opt_bool(SIGNED_PARAM).ok().flatten() == Some(true);
            return Some(RenpyFile::Save { signed });
        }
        let root = self.doc.get(self.doc.root())?;
        if python_class(root) != Some(PERSISTENT_CLASS) {
            return None;
        }
        // Ren'Py 8.1 and later write the signature as text after the zlib data.
        let signed = steps.iter().any(|step| {
            step.kind == ZLIB_LAYER
                && step
                    .params
                    .opt_bytes("trailing")
                    .ok()
                    .flatten()
                    .is_some_and(|trailing| !trailing.is_empty())
        });
        Some(RenpyFile::Persistent { signed })
    }

    /// True for a signed `persistent` file, which must not be written. With the player's own key it
    /// can be signed again, so it is editable.
    pub(super) fn renpy_locked(&self) -> bool {
        matches!(
            self.renpy_file(),
            Some(RenpyFile::Persistent { signed: true })
        ) && self.renpy_key.is_none()
    }

    /// The notice key for the UI and its English text.
    pub(super) fn renpy_notice(&self) -> Option<(&'static str, &'static str)> {
        let key = self.renpy_key.is_some();
        match self.renpy_file()? {
            RenpyFile::Save { signed: true } if key => Some((
                NOTICE_KEY_SIGNED,
                "After you save changes, the save is signed with your own Ren'Py key.",
            )),
            RenpyFile::Save { signed: true } => Some((
                NOTICE_SIGNED_SAVE,
                "After you save changes, the game asks whether to trust this save (it was made on another device). Answer Yes.",
            )),
            RenpyFile::Persistent { signed: true } if key => Some((
                NOTICE_KEY_SIGNED_PERSISTENT,
                "This Ren'Py persistent data is signed; changes are saved and signed with your own Ren'Py key.",
            )),
            RenpyFile::Persistent { signed: true } => Some((
                NOTICE_SIGNED_PERSISTENT,
                "This Ren'Py persistent data is signed. The game would throw away an edited copy and start over, so it is open read-only.",
            )),
            _ => None,
        }
    }

    /// True for a Ren'Py save or `persistent` file that carries a signature. Only such a file is
    /// signed again, so only for such a file the player's key file is looked up.
    pub(super) fn renpy_signed(&self) -> bool {
        matches!(
            self.renpy_file(),
            Some(RenpyFile::Save { signed: true } | RenpyFile::Persistent { signed: true })
        )
    }

    /// The recipe for a write. A file that was signed before and has the player's key gets a fresh
    /// signature: the `signatures` text for the save layer, or the signature of the uncompressed
    /// persistent data as the zlib `trailing` bytes. A file that was not signed stays unsigned. The
    /// key text itself never enters the recipe.
    pub(super) fn renpy_recipe(&self, recipe: &Recipe) -> Result<Recipe, ApiError> {
        let Some(file) = self.renpy_file().filter(|_| self.renpy_signed()) else {
            return Ok(recipe.clone());
        };
        let Some(path) = self.renpy_key.as_deref() else {
            return Ok(recipe.clone());
        };
        let keys = load_keys(path)?;
        let codec = self
            .reg
            .codec(PICKLE_CODEC)
            .ok_or_else(ApiError::internal)?;
        let mut out = recipe.clone();
        let (codec_step, layers) = out.steps.split_last_mut().ok_or_else(ApiError::internal)?;
        let inner = codec.encode(&self.doc, &codec_step.params)?;
        match file {
            RenpyFile::Save { .. } => {
                let signatures = sign::sign_data(&inner, &keys);
                let mut found = false;
                for step in layers.iter_mut().filter(|step| step.kind == SAVE_LAYER) {
                    step.params.set(SIGNATURES_PARAM, signatures.clone());
                    found = true;
                }
                if !found {
                    return Err(key_error(path));
                }
            }
            RenpyFile::Persistent { .. } => {
                // Ren'Py signs with a random nonce and we do not, so signing the same data again
                // would change the file and drop the signatures of other keys. Data nobody changed
                // keeps the signature text the file has now (read from the bytes on disk: the
                // recipe of an earlier save in this session is out of date).
                let unchanged = pipeline::decode(&self.original, recipe, &self.reg, &self.limits)
                    .ok()
                    .filter(|current| current.frames.last() == Some(&inner));
                let signatures = match &unchanged {
                    Some(current) => current
                        .recipe
                        .steps
                        .iter()
                        .filter(|step| step.kind == ZLIB_LAYER)
                        .find_map(|step| step.params.opt_bytes(TRAILING_PARAM).ok().flatten())
                        .map(<[u8]>::to_vec)
                        .unwrap_or_default(),
                    None => sign::sign_data(&inner, &keys).into_bytes(),
                };
                let mut found = false;
                for step in layers.iter_mut().filter(|step| step.kind == ZLIB_LAYER) {
                    step.params.set(TRAILING_PARAM, signatures.clone());
                    found = true;
                }
                if !found {
                    return Err(key_error(path));
                }
            }
        }
        Ok(out)
    }

    /// The error for an edit or save of a read-only document.
    pub(super) fn read_only_error(&self) -> ApiError {
        if self.renpy_locked() {
            ApiError::new(
                app_codes::READ_ONLY_DOCUMENT,
                "this Ren'Py persistent file is signed, so it is open read-only",
                "Ren'Py would throw away an edited persistent file and start over. Edit a save file instead.",
            )
        } else {
            read_only_document()
        }
    }

    /// The Values list of a Ren'Py save or `persistent` file; `None` for other files.
    pub(super) fn renpy_view(&self, ctx: &Ctx<'_>) -> Option<Vec<ValueRow>> {
        ctx.renpy_rows(self.renpy_file()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn internal_names() {
        for name in [
            "_window",
            "_last_say_who",
            "nvl_list",
            "config.rollback",
            "save_name",
            "mouse_visible",
            "_errorhandling.x",
        ] {
            assert!(is_internal(name, None), "{name}");
        }
        for name in ["money", "hero_name", "mystery.clue_count", "nvl", "visited"] {
            assert_eq!(is_internal(name, None), name == "nvl", "{name}");
        }
        let mut character = Node::new(Value::Object(Vec::new()));
        character.type_hint = Some(saveprism_core::TypeHint::new(
            "PY:object:renpy.character.ADVCharacter",
        ));
        assert!(is_internal("eileen", Some(&character)));
        let mut list = Node::new(Value::Array(Vec::new()));
        list.type_hint = Some(saveprism_core::TypeHint::new(
            "PY:list:renpy.revertable.RevertableList",
        ));
        assert!(!is_internal("visited", Some(&list)));
    }

    /// An environment of the given variables.
    fn vars(pairs: &[(&'static str, &'static str)]) -> impl Fn(&str) -> Option<OsString> {
        let pairs = pairs.to_vec();
        move |name: &str| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| OsString::from(value))
        }
    }

    fn tokens(base: &str) -> PathBuf {
        PathBuf::from(base).join(TOKEN_DIR)
    }

    /// A save somewhere that is not inside a `Ren'Py Data` folder.
    fn plain_save() -> PathBuf {
        PathBuf::from("games").join("saves").join("1-1-LT1.save")
    }

    #[test]
    fn the_token_folder_follows_renpys_platform_defaults() {
        let save = plain_save();
        let windows = |pairs: &[(&'static str, &'static str)]| {
            token_dir(&save, Platform::Windows, &vars(pairs))
        };
        assert_eq!(
            windows(&[("APPDATA", "appdata"), ("USERPROFILE", "home")]),
            Some(PathBuf::from("appdata").join("RenPy").join(TOKEN_DIR))
        );
        assert_eq!(
            windows(&[("USERPROFILE", "home")]),
            Some(PathBuf::from("home").join("RenPy").join(TOKEN_DIR))
        );
        // Windows has no `~/.renpy`: HOME alone gives nothing.
        assert_eq!(windows(&[("HOME", "home")]), None);
        assert_eq!(windows(&[]), None);
        assert_eq!(
            token_dir(&save, Platform::MacOs, &vars(&[("HOME", "home")])),
            Some(
                PathBuf::from("home")
                    .join("Library")
                    .join("RenPy")
                    .join(TOKEN_DIR)
            )
        );
        assert_eq!(
            token_dir(&save, Platform::Other, &vars(&[("HOME", "home")])),
            Some(PathBuf::from("home").join(".renpy").join(TOKEN_DIR))
        );
        // The other systems do not look at the Windows variables.
        let windows_only = vars(&[("APPDATA", "appdata"), ("USERPROFILE", "home")]);
        assert_eq!(token_dir(&save, Platform::Other, &windows_only), None);
        assert_eq!(token_dir(&save, Platform::MacOs, &windows_only), None);
    }

    #[test]
    fn the_saves_variable_wins_and_an_empty_one_is_not_set() {
        let save = plain_save();
        let portable = PathBuf::from("games")
            .join(PORTABLE_DIR)
            .join("game")
            .join("persistent");
        let env = vars(&[("RENPY_PATH_TO_SAVES", "elsewhere"), ("APPDATA", "appdata")]);
        // It comes before the portable folder and before the system default, as in `path_to_saves`.
        assert_eq!(
            token_dir(&portable, Platform::Windows, &env),
            Some(tokens("elsewhere"))
        );
        assert_eq!(
            token_dir(&save, Platform::Windows, &env),
            Some(tokens("elsewhere"))
        );
        let empty = vars(&[("RENPY_PATH_TO_SAVES", ""), ("APPDATA", "appdata")]);
        assert_eq!(
            token_dir(&save, Platform::Windows, &empty),
            Some(PathBuf::from("appdata").join("RenPy").join(TOKEN_DIR))
        );
    }

    #[test]
    fn a_save_inside_a_portable_folder_uses_the_tokens_beside_it() {
        let inside = PathBuf::from("games")
            .join("Pack")
            .join(PORTABLE_DIR)
            .join("mygame-123")
            .join("1-1-LT1.save");
        let expected = PathBuf::from("games")
            .join("Pack")
            .join(PORTABLE_DIR)
            .join(TOKEN_DIR);
        let env = vars(&[("APPDATA", "appdata"), ("HOME", "home")]);
        for platform in [Platform::Windows, Platform::MacOs, Platform::Other] {
            assert_eq!(
                token_dir(&inside, platform, &env),
                Some(expected.clone()),
                "{platform:?}"
            );
        }
        // Windows and macOS file systems ignore case; Linux's does not.
        let lower = PathBuf::from("games")
            .join("ren'py data")
            .join("mygame-123")
            .join("persistent");
        assert_eq!(
            token_dir(&lower, Platform::Windows, &env),
            Some(PathBuf::from("games").join("ren'py data").join(TOKEN_DIR))
        );
        assert_eq!(
            token_dir(&lower, Platform::Other, &env),
            Some(PathBuf::from("home").join(".renpy").join(TOKEN_DIR))
        );
        // The save file itself is not the folder, even with that name.
        let named = PathBuf::from("games").join(PORTABLE_DIR);
        assert_eq!(
            token_dir(&named, Platform::Other, &env),
            Some(PathBuf::from("home").join(".renpy").join(TOKEN_DIR))
        );
    }

    #[test]
    fn the_key_text_is_redacted_and_a_big_key_file_is_no_key_file() {
        let text = KeyText("signing-key SECRET".to_string());
        assert_eq!(format!("{text:?}"), "<redacted>");
        assert_eq!(format!("{:?}", Some(&text)), "Some(<redacted>)");

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(KEY_FILE);
        let line = "signing-key SECRET\n";
        let limit = usize::try_from(MAX_KEY_FILE_BYTES).unwrap();
        // Exactly at the limit is read; one byte more is not.
        let mut at_limit = line.to_string();
        at_limit.push_str(&"#".repeat(limit - line.len()));
        fs::write(&path, &at_limit).unwrap();
        assert_eq!(read_key_file(&path).map(|text| text.0), Some(at_limit));
        fs::write(
            &path,
            format!("{line}{}", "#".repeat(limit - line.len() + 1)),
        )
        .unwrap();
        assert!(read_key_file(&path).is_none());
        // A missing file and text that is not UTF-8 are no key file either.
        assert!(read_key_file(&dir.path().join("missing.txt")).is_none());
        fs::write(&path, [0xff_u8, 0xfe, 0xfd]).unwrap();
        assert!(read_key_file(&path).is_none());
    }
}
