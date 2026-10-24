//! Ren'Py saves (`1-1-LT1.save`) and the `persistent` file as a Values list.
//!
//! A save is a zip whose `log` entry is the pickle of `(roots, log)` (see
//! `saveprism_codecs::renpy` and `saveprism_codecs::pickle`). `roots` maps `store.<name>` and
//! `store.<namespace>.<name>` to the game's variables; the view lists them as `<name>` and
//! `<namespace>.<name>`, the game's own variables first (by name) and Ren'Py's internals after them.
//! Numbers, on/off values and texts are edited in place; lists, dicts and game objects open as
//! groups; a value stored under two names points at its first place.
//!
//! Signatures (Ren'Py 7.6 / 8.1 and later): when the player's own `security_keys.txt` is found, an
//! edited save is signed with it and the game loads it without asking whether to trust it; a signed
//! `persistent` file is edited and signed the same way. Without a key file an edited save gets an
//! empty `signatures` entry (the game asks the player to trust it) and a signed `persistent` file
//! stays read-only, because Ren'Py would throw an edited copy away. The key is read only while
//! signing, lives in memory only and is never logged. Nothing is executed.

use std::fs;
use std::path::{Path as FsPath, PathBuf};

use saveprism_codecs::pickle::python_class;
use saveprism_codecs::renpy::sign;
use saveprism_codecs::renpy::{RENPY_KEY, SIGN_KEY_PARAM, SIGNED_PARAM};
use saveprism_core::{Node, Path, Recipe, Value};

use super::game::{At, Ctx, MAX_VALUE_ROWS, NoteKind, RowNote, UNLIMITED, ValueRow};
use super::{ApiError, Session, app_codes, child_count, read_only_document};

/// The Ren'Py save layer.
const SAVE_LAYER: &str = "container.renpy-save";
/// The pickle codec.
const PICKLE_CODEC: &str = "format.python-pickle";
/// The zlib layer of the `persistent` file.
const ZLIB_LAYER: &str = "compression.zlib";
/// The class of the object pickled into `persistent`.
const PERSISTENT_CLASS: &str = "renpy.persistent.Persistent";
/// Notice for a signed save: the game asks whether to trust the edited save.
pub const NOTICE_SIGNED_SAVE: &str = "renpy-signed";
/// Notice for a signed `persistent` file: it is open read-only.
pub const NOTICE_SIGNED_PERSISTENT: &str = "renpy-persistent-signed";
/// Notice when the player's own key signs the edited file.
pub const NOTICE_KEY_SIGNED: &str = "renpy-key-signed";

/// File name of Ren'Py's key list (`savetoken.load_tokens`).
const KEY_FILE: &str = "security_keys.txt";

/// Where the player's key file comes from. Production reads the default Ren'Py folder; tests name
/// a file of their own or turn the lookup off.
#[derive(Clone, Debug)]
pub(super) enum RenpyKeySource {
    /// `%APPDATA%/RenPy/tokens/security_keys.txt` and the platform equivalents.
    Default,
    /// An explicit file (or none); tests only.
    #[cfg(test)]
    At(Option<PathBuf>),
}

impl RenpyKeySource {
    /// The key file to use, when it exists and holds at least one `signing-key` line.
    pub(super) fn resolve(self) -> Option<PathBuf> {
        match self {
            RenpyKeySource::Default => default_key_file(),
            #[cfg(test)]
            RenpyKeySource::At(path) => path.filter(|path| key_file_works(path)),
        }
    }
}

/// The Ren'Py token folder for this platform, the one `renpy.py:path_to_saves` builds.
fn default_key_file() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(appdata) = std::env::var_os("APPDATA") {
        candidates.push(
            PathBuf::from(appdata)
                .join("RenPy")
                .join("tokens")
                .join(KEY_FILE),
        );
    }
    if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
        let base = if cfg!(target_os = "macos") {
            PathBuf::from(home).join("Library").join("RenPy")
        } else {
            PathBuf::from(home).join(".renpy")
        };
        candidates.push(base.join("tokens").join(KEY_FILE));
    }
    candidates.into_iter().find(|path| key_file_works(path))
}

/// True when `path` is a readable key file with at least one usable signing key.
fn key_file_works(path: &FsPath) -> bool {
    fs::read_to_string(path)
        .ok()
        .is_some_and(|text| !sign::parse_key_file(&text).is_empty())
}

/// The error for a key file that was found when the save was opened but cannot be used now.
fn key_error(path: Option<&FsPath>) -> ApiError {
    let where_ = path
        .map(|path| format!("\"{}\"", path.display()))
        .unwrap_or_else(|| "the Ren'Py key file".to_string());
    ApiError::new(
        RENPY_KEY.as_str(),
        format!("{where_} could not be read as a Ren'Py key file"),
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
                "After you save changes, the save is signed with your own Ren'Py key, so the game opens it without asking whether to trust it.",
            )),
            RenpyFile::Save { signed: true } => Some((
                NOTICE_SIGNED_SAVE,
                "After you save changes, the game asks whether to trust this save (it was made on another device). Answer Yes.",
            )),
            RenpyFile::Persistent { signed: true } if key => Some((
                NOTICE_KEY_SIGNED,
                "This Ren'Py persistent file is signed; changes are saved and signed again with your own Ren'Py key.",
            )),
            RenpyFile::Persistent { signed: true } => Some((
                NOTICE_SIGNED_PERSISTENT,
                "This Ren'Py persistent file is signed. The game would throw away an edited copy and start over, so it is open read-only.",
            )),
            _ => None,
        }
    }

    /// The recipe for a write: when the player's key is known, it is added to the save layer (an
    /// edited save gets fresh `signatures`), or used to sign the uncompressed persistent data and
    /// replace the zlib `trailing` signature. The key text lives in this copy only.
    pub(super) fn renpy_recipe(&self, recipe: &Recipe) -> Result<Recipe, ApiError> {
        let Some(file) = self.renpy_file() else {
            return Ok(recipe.clone());
        };
        let Some(path) = self.renpy_key.as_deref() else {
            return Ok(recipe.clone());
        };
        let text = fs::read_to_string(path).map_err(|_| key_error(Some(path)))?;
        let mut recipe = recipe.clone();
        match file {
            RenpyFile::Save { .. } => {
                let mut found = false;
                for step in &mut recipe.steps {
                    if step.kind == SAVE_LAYER {
                        step.params.set(SIGN_KEY_PARAM, text.clone());
                        found = true;
                    }
                }
                if !found {
                    return Err(key_error(Some(path)));
                }
            }
            RenpyFile::Persistent { .. } => {
                let keys = sign::parse_key_file(&text);
                if keys.is_empty() {
                    return Err(key_error(Some(path)));
                }
                let (codec_step, layer_steps) = recipe
                    .steps
                    .split_last_mut()
                    .ok_or_else(ApiError::internal)?;
                let codec = self
                    .reg
                    .codec(PICKLE_CODEC)
                    .ok_or_else(ApiError::internal)?;
                let inner = codec.encode(&self.doc, &codec_step.params)?;
                let signatures = sign::sign_data(&inner, &keys);
                let mut found = false;
                for step in layer_steps.iter_mut() {
                    if step.kind == ZLIB_LAYER {
                        step.params.set("trailing", signatures.clone().into_bytes());
                        found = true;
                    }
                }
                if !found {
                    return Err(key_error(Some(path)));
                }
            }
        }
        Ok(recipe)
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
}
