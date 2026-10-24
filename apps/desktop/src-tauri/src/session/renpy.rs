//! Ren'Py saves (`1-1-LT1.save`) and the `persistent` file as a Values list.
//!
//! A save is a zip whose `log` entry is the pickle of `(roots, log)` (see
//! `saveprism_codecs::renpy` and `saveprism_codecs::pickle`). `roots` maps `store.<name>` and
//! `store.<namespace>.<name>` to the game's variables; the view lists them as `<name>` and
//! `<namespace>.<name>`, the game's own variables first (by name) and Ren'Py's internals after them.
//! Numbers, on/off values and texts are edited in place; lists, dicts and game objects open as
//! groups; a value stored under two names points at its first place.
//!
//! Signatures (Ren'Py 7.6 / 8.1 and later): an edited save is written with an empty `signatures`
//! entry, so the game asks the player whether to trust it; the UI says to answer Yes. A signed
//! `persistent` file would be thrown away by the game after any change (and the player's data with
//! it), so it opens read-only. Nothing is executed.

use saveprism_codecs::pickle::python_class;
use saveprism_codecs::renpy::SIGNED_PARAM;
use saveprism_core::{Node, Path, Value};

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

    /// True for a signed `persistent` file, which must not be written.
    pub(super) fn renpy_locked(&self) -> bool {
        matches!(
            self.renpy_file(),
            Some(RenpyFile::Persistent { signed: true })
        )
    }

    /// The notice key for the UI and its English text.
    pub(super) fn renpy_notice(&self) -> Option<(&'static str, &'static str)> {
        match self.renpy_file()? {
            RenpyFile::Save { signed: true } => Some((
                NOTICE_SIGNED_SAVE,
                "After you save changes, the game asks whether to trust this save (it was made on another device). Answer Yes.",
            )),
            RenpyFile::Persistent { signed: true } => Some((
                NOTICE_SIGNED_PERSISTENT,
                "This Ren'Py persistent file is signed. The game would throw away an edited copy and start over, so it is open read-only.",
            )),
            _ => None,
        }
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
