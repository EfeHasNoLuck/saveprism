//! Usual save folders per engine, for the "Where is my save?" dialog.
//!
//! The paths are written from public documentation (Unity's `Application.persistentDataPath`
//! page, Godot's "File paths in Godot projects", Ren'Py's `config.savedir`, Unreal's "Saving and
//! Loading Your Game", and the game/engine guides for the rest). Environment variables are
//! expanded here, so the UI only shows text. A path with an unknown `<game>` or `<company>` part
//! opens the folder above that part; a relative path (inside the game's own folder) has no button.
//! Nothing is searched: only the returned candidate folders are checked for existence.

use std::path::PathBuf;

use serde::Serialize;

/// One path the dialog shows.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveLocationPath {
    /// Path with environment variables expanded; `<game>` / `<company>` stay as placeholders.
    pub shown: String,
    /// Folder to open in the system's file manager, when it exists on this PC.
    pub folder: Option<String>,
    /// True when `folder` exists (a relative path is never "found").
    pub exists: bool,
}

/// The paths of one engine, in the order the dialog lists them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveLocation {
    /// Stable engine id; the UI translates it (`locations.engine.<id>`).
    pub engine: &'static str,
    /// Usual folders, most likely first.
    pub paths: Vec<SaveLocationPath>,
}

impl SaveLocationPath {
    /// Builds one entry from its raw template.
    fn new(template: &str) -> Self {
        let shown = expand_env(template);
        let folder = open_folder(&shown);
        Self {
            exists: folder.is_some(),
            folder: folder.map(|path| path.to_string_lossy().into_owned()),
            shown,
        }
    }
}

/// The engines and their usual folders. A path is either absolute (after environment expansion)
/// or relative to the game's folder.
const CATALOG: &[(&str, &[&str])] = &[
    (
        "rpgmaker-mv",
        &["<game folder>\\www\\save", "<game folder>\\save"],
    ),
    (
        "rpgmaker-classic",
        &["<game folder>", "<game folder>\\Save"],
    ),
    ("wolf-rpg", &["<game folder>\\Save"]),
    (
        "unity",
        &["%USERPROFILE%\\AppData\\LocalLow\\<company>\\<game>"],
    ),
    ("unreal", &["%LOCALAPPDATA%\\<game>\\Saved\\SaveGames"]),
    (
        "renpy",
        &["%APPDATA%\\RenPy\\<game>", "<game folder>\\game\\saves"],
    ),
    ("godot", &["%APPDATA%\\Godot\\app_userdata\\<game>"]),
    ("twine", &[]),
    (
        "flash",
        &["%APPDATA%\\Macromedia\\Flash Player\\#SharedObjects"],
    ),
    (
        "witcher3",
        &["%USERPROFILE%\\Documents\\The Witcher 3\\gamesaves"],
    ),
    ("palworld", &["%LOCALAPPDATA%\\Pal\\Saved\\SaveGames"]),
];

/// The usual save folders of every engine the dialog names.
pub fn save_locations() -> Vec<SaveLocation> {
    CATALOG
        .iter()
        .map(|(engine, templates)| SaveLocation {
            engine,
            paths: templates
                .iter()
                .map(|raw| SaveLocationPath::new(raw))
                .collect(),
        })
        .collect()
}

/// Expands `%NAME%` environment variables; a missing variable keeps its `%NAME%` text.
fn expand_env(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) => {
                let name = &after[..end];
                match std::env::var(name) {
                    Ok(value) if !name.is_empty() && !value.is_empty() => out.push_str(&value),
                    _ => {
                        out.push('%');
                        out.push_str(name);
                        out.push('%');
                    }
                }
                rest = &after[end + 1..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// The folder to open for a shown path, if this PC has it. When a `<...>` placeholder is in the
/// path, the folder just above the first placeholder is used (the player picks the game's folder).
fn open_folder(shown: &str) -> Option<PathBuf> {
    if !PathBuf::from(shown).is_absolute() {
        return None;
    }
    let parts: Vec<&str> = shown.split('\\').collect();
    let first_unknown = parts.iter().position(|part| part.contains('<'));
    let candidate = match first_unknown {
        Some(index) if index > 0 => parts.get(..index)?.join("\\"),
        Some(_) => return None,
        None => shown.trim_end_matches('\\').to_string(),
    };
    let path = PathBuf::from(candidate);
    path.is_dir().then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_engine_has_paths_except_browser_storage() {
        let locations = save_locations();
        for location in &locations {
            if location.engine == "twine" {
                assert!(location.paths.is_empty());
            } else {
                assert!(!location.paths.is_empty(), "{}", location.engine);
            }
        }
        let engines: Vec<&str> = locations.iter().map(|location| location.engine).collect();
        for engine in [
            "rpgmaker-mv",
            "rpgmaker-classic",
            "wolf-rpg",
            "unity",
            "unreal",
            "renpy",
            "godot",
            "twine",
            "flash",
            "witcher3",
            "palworld",
        ] {
            assert!(engines.contains(&engine), "{engine}");
        }
    }

    #[test]
    fn relative_paths_have_no_folder() {
        let path = SaveLocationPath::new("<game folder>\\save");
        assert_eq!(path.shown, "<game folder>\\save");
        assert!(path.folder.is_none());
        assert!(!path.exists);
    }

    #[test]
    fn a_placeholder_opens_the_folder_above_it() {
        let root = tempfile::tempdir().unwrap();
        let template = format!("{}\\<game>\\Saved\\SaveGames", root.path().display());
        let path = SaveLocationPath::new(&template);
        assert_eq!(
            path.folder.as_deref(),
            Some(root.path().to_string_lossy().as_ref())
        );
        assert!(path.exists);
    }

    #[test]
    fn a_known_missing_folder_has_no_button() {
        let root = tempfile::tempdir().unwrap();
        let template = format!("{}\\does-not-exist", root.path().display());
        let path = SaveLocationPath::new(&template);
        assert!(!path.exists);
        assert!(path.folder.is_none());
    }

    #[test]
    fn environment_variables_expand_and_unknown_ones_stay() {
        let user = std::env::var("USERPROFILE").unwrap_or_default();
        let expanded = expand_env("%USERPROFILE%\\Saved");
        if user.is_empty() {
            assert_eq!(expanded, "%USERPROFILE%\\Saved");
        } else {
            assert_eq!(expanded, format!("{user}\\Saved"));
        }
        assert_eq!(
            expand_env("%SAVEPRISM_NOT_SET%\\x"),
            "%SAVEPRISM_NOT_SET%\\x"
        );
    }
}
