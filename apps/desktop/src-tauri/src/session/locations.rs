//! Usual save folders per engine, for the "Where is my save?" dialog.
//!
//! The paths are written from public documentation (Unity's `Application.persistentDataPath`
//! page, Godot's "File paths in Godot projects", Ren'Py's `config.savedir`, Unreal's "Saving and
//! Loading Your Game", GameMaker's "The File System", and the game/engine guides for the rest).
//! Each system has its own list: Windows paths use `%APPDATA%`-style variables and `\`; macOS and
//! Linux paths start at `%HOME%` (or, where the program reads it, an XDG base folder) and use `/`.
//! On Linux, games made only for Windows are listed inside the Proton prefix Steam makes for them.
//! Environment variables are expanded here, so the UI only shows text. A path with an unknown
//! `<game>` or `<company>` part opens the folder above that part; a relative path (inside the
//! game's own folder) has no button. Nothing is searched: only the returned candidate folders are
//! checked for existence.

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

/// The systems with a list of their own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Platform {
    Windows,
    MacOs,
    Linux,
}

impl Platform {
    /// The system this build runs on; other Unix systems use the Linux (XDG) list.
    fn current() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::MacOs
        } else {
            Self::Linux
        }
    }
}

/// The engines and their usual folders. A path is either absolute (after environment expansion)
/// or relative to the game's folder.
type Catalog = &'static [(&'static str, &'static [&'static str])];

/// The list of one system.
fn catalog(platform: Platform) -> Catalog {
    match platform {
        Platform::Windows => WINDOWS,
        Platform::MacOs => MACOS,
        Platform::Linux => LINUX,
    }
}

const WINDOWS: Catalog = &[
    (
        "rpgmaker-mv",
        &["<game folder>\\www\\save", "<game folder>\\save"],
    ),
    (
        "rpgmaker-classic",
        &["<game folder>", "<game folder>\\Save"],
    ),
    ("wolf-rpg", &["<game folder>\\Save"]),
    ("ags", &["<game folder>"]),
    ("nscripter", &["<game folder>", "<game folder>\\sav"]),
    ("emuera", &["<game folder>", "<game folder>\\sav"]),
    ("alicesoft", &["<game folder>\\SaveData", "<game folder>"]),
    ("artemis", &["<game folder>\\save"]),
    ("construct", &[]),
    (
        "unity",
        &[
            "%USERPROFILE%\\AppData\\LocalLow\\<company>\\<game>",
            "HKEY_CURRENT_USER\\Software\\<company>\\<game>",
        ],
    ),
    ("unreal", &["%LOCALAPPDATA%\\<game>\\Saved\\SaveGames"]),
    ("gamemaker", &["%LOCALAPPDATA%\\<game>"]),
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
    (
        "satisfactory",
        &["%LOCALAPPDATA%\\FactoryGame\\Saved\\SaveGames\\<id>"],
    ),
    (
        "slime-rancher-2",
        &["%USERPROFILE%\\AppData\\LocalLow\\MonomiPark\\SlimeRancher2"],
    ),
    (
        "terraria",
        &[
            "%USERPROFILE%\\Documents\\My Games\\Terraria\\Players",
            "%USERPROFILE%\\Documents\\My Games\\Terraria\\Worlds",
        ],
    ),
    (
        "neverwinter-nights",
        &[
            "%USERPROFILE%\\Documents\\Neverwinter Nights\\saves",
            "<game folder>\\saves",
        ],
    ),
    (
        "kotor",
        &[
            "<game folder>\\saves",
            "%USERPROFILE%\\Documents\\Knights of the Old Republic\\saves",
        ],
    ),
    ("elden-ring", &["%APPDATA%\\EldenRing"]),
    ("dark-souls-3", &["%APPDATA%\\DarkSoulsIII"]),
    (
        "ets2",
        &["%USERPROFILE%\\Documents\\Euro Truck Simulator 2\\profiles"],
    ),
    (
        "ats",
        &["%USERPROFILE%\\Documents\\American Truck Simulator\\profiles"],
    ),
    (
        "retroarch",
        &[
            "%APPDATA%\\RetroArch\\states",
            "%APPDATA%\\RetroArch\\saves",
        ],
    ),
];

/// Games made only for Windows are left out: they run in a Wine or CrossOver bottle there, whose
/// folder depends on the tool.
const MACOS: Catalog = &[
    (
        "rpgmaker-mv",
        &["<game folder>/www/save", "<game folder>/save"],
    ),
    ("rpgmaker-classic", &["<game folder>", "<game folder>/Save"]),
    ("wolf-rpg", &["<game folder>/Save"]),
    ("ags", &["<game folder>"]),
    ("nscripter", &["<game folder>", "<game folder>/sav"]),
    ("emuera", &["<game folder>", "<game folder>/sav"]),
    ("alicesoft", &["<game folder>/SaveData", "<game folder>"]),
    ("artemis", &["<game folder>/save"]),
    ("construct", &[]),
    (
        "unity",
        &[
            "%HOME%/Library/Application Support/<company>/<game>",
            "%HOME%/Library/Application Support/unity.<company>.<game>",
            "%HOME%/Library/Preferences/unity.<company>.<game>.plist",
        ],
    ),
    (
        "unreal",
        &["%HOME%/Library/Application Support/Epic/<game>/Saved/SaveGames"],
    ),
    ("gamemaker", &["%HOME%/Library/Application Support/<game>"]),
    (
        "renpy",
        &["%HOME%/Library/RenPy/<game>", "<game folder>/game/saves"],
    ),
    (
        "godot",
        &["%HOME%/Library/Application Support/Godot/app_userdata/<game>"],
    ),
    ("twine", &[]),
    (
        "flash",
        &["%HOME%/Library/Preferences/Macromedia/Flash Player/#SharedObjects"],
    ),
    (
        "terraria",
        &[
            "%HOME%/Library/Application Support/Terraria/Players",
            "%HOME%/Library/Application Support/Terraria/Worlds",
        ],
    ),
    (
        "neverwinter-nights",
        &[
            "%HOME%/Documents/Neverwinter Nights/saves",
            "<game folder>/saves",
        ],
    ),
    ("kotor", &["<game folder>/saves"]),
    (
        "ets2",
        &["%HOME%/Library/Application Support/Euro Truck Simulator 2/profiles"],
    ),
    (
        "ats",
        &["%HOME%/Library/Application Support/American Truck Simulator/profiles"],
    ),
    (
        "retroarch",
        &[
            "%HOME%/Documents/RetroArch/states",
            "%HOME%/Documents/RetroArch/saves",
            "%HOME%/Library/Application Support/RetroArch/states",
            "%HOME%/Library/Application Support/RetroArch/saves",
        ],
    ),
];

/// A folder inside the Proton prefix Steam makes for a Windows game: `$app_id` is the game's Steam
/// id and `$rest` the Windows path under the user's folder. `~/.steam/steam` points at Steam's own
/// folder wherever the distribution installed it.
macro_rules! proton {
    ($app_id:literal, $rest:literal) => {
        concat!(
            "%HOME%/.steam/steam/steamapps/compatdata/",
            $app_id,
            "/pfx/drive_c/users/steamuser/",
            $rest
        )
    };
}

const LINUX: Catalog = &[
    (
        "rpgmaker-mv",
        &["<game folder>/www/save", "<game folder>/save"],
    ),
    ("rpgmaker-classic", &["<game folder>", "<game folder>/Save"]),
    ("wolf-rpg", &["<game folder>/Save"]),
    ("ags", &["<game folder>"]),
    ("nscripter", &["<game folder>", "<game folder>/sav"]),
    ("emuera", &["<game folder>", "<game folder>/sav"]),
    ("alicesoft", &["<game folder>/SaveData", "<game folder>"]),
    ("artemis", &["<game folder>/save"]),
    ("construct", &[]),
    ("unity", &["%XDG_CONFIG_HOME%/unity3d/<company>/<game>"]),
    ("unreal", &["%HOME%/.config/Epic/<game>/Saved/SaveGames"]),
    ("gamemaker", &["%HOME%/.config/<game>"]),
    (
        "renpy",
        &["%HOME%/.renpy/<game>", "<game folder>/game/saves"],
    ),
    ("godot", &["%XDG_DATA_HOME%/godot/app_userdata/<game>"]),
    ("twine", &[]),
    ("flash", &["%HOME%/.macromedia/Flash_Player/#SharedObjects"]),
    (
        "witcher3",
        &[proton!("292030", "Documents/The Witcher 3/gamesaves")],
    ),
    (
        "palworld",
        &[proton!("1623730", "AppData/Local/Pal/Saved/SaveGames")],
    ),
    (
        "satisfactory",
        &[proton!(
            "526870",
            "AppData/Local/FactoryGame/Saved/SaveGames/<id>"
        )],
    ),
    (
        "slime-rancher-2",
        &[proton!(
            "1657630",
            "AppData/LocalLow/MonomiPark/SlimeRancher2"
        )],
    ),
    (
        "terraria",
        &[
            "%XDG_DATA_HOME%/Terraria/Players",
            "%XDG_DATA_HOME%/Terraria/Worlds",
        ],
    ),
    (
        "neverwinter-nights",
        &[
            "%HOME%/.local/share/Neverwinter Nights/saves",
            "<game folder>/saves",
        ],
    ),
    ("kotor", &["<game folder>/saves"]),
    (
        "elden-ring",
        &[proton!("1245620", "AppData/Roaming/EldenRing")],
    ),
    (
        "dark-souls-3",
        &[proton!("374320", "AppData/Roaming/DarkSoulsIII")],
    ),
    (
        "ets2",
        &["%HOME%/.local/share/Euro Truck Simulator 2/profiles"],
    ),
    (
        "ats",
        &["%HOME%/.local/share/American Truck Simulator/profiles"],
    ),
    (
        "retroarch",
        &[
            "%XDG_CONFIG_HOME%/retroarch/states",
            "%XDG_CONFIG_HOME%/retroarch/saves",
        ],
    ),
];

/// The usual save folders of every engine the dialog names, on the system this build runs on.
pub fn save_locations() -> Vec<SaveLocation> {
    catalog(Platform::current())
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
    expand_vars(text, &|name| std::env::var(name).ok())
}

/// Expands `%NAME%` variables with the values `lookup` gives (see [`var_value`]); a variable
/// without a value keeps its `%NAME%` text.
fn expand_vars(text: &str, lookup: &dyn Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) => {
                let name = &after[..end];
                match var_value(name, lookup) {
                    Some(value) => out.push_str(&value),
                    None => {
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

/// The value of one variable, when it is set and not empty. An XDG base folder that is not set
/// falls back to its default under `HOME`, as the XDG Base Directory Specification says.
fn var_value(name: &str, lookup: &dyn Fn(&str) -> Option<String>) -> Option<String> {
    let set = |key: &str| lookup(key).filter(|value| !key.is_empty() && !value.is_empty());
    set(name).or_else(|| {
        let default = match name {
            "XDG_CONFIG_HOME" => ".config",
            "XDG_DATA_HOME" => ".local/share",
            _ => return None,
        };
        set("HOME").map(|home| format!("{}/{default}", home.trim_end_matches('/')))
    })
}

/// The folder to open for a shown path, if this PC has it. When a `<...>` placeholder is in the
/// path, the folder just above the part holding the first placeholder is used (the player picks
/// the game's folder). Both `\` and `/` end a part, so one rule serves the list of every system.
fn open_folder(shown: &str) -> Option<PathBuf> {
    if !PathBuf::from(shown).is_absolute() {
        return None;
    }
    let candidate = match shown.find('<') {
        Some(index) => {
            let cut = shown.get(..index)?.rfind(['\\', '/'])?;
            shown.get(..cut)?
        }
        None => shown.trim_end_matches(['\\', '/']),
    };
    let path = PathBuf::from(candidate);
    path.is_dir().then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLATFORMS: [Platform; 3] = [Platform::Windows, Platform::MacOs, Platform::Linux];

    /// Games made only for Windows; Linux lists them in their Proton prefix.
    const WINDOWS_ONLY: [&str; 6] = [
        "witcher3",
        "palworld",
        "satisfactory",
        "slime-rancher-2",
        "elden-ring",
        "dark-souls-3",
    ];

    fn engines(platform: Platform) -> Vec<&'static str> {
        catalog(platform)
            .iter()
            .map(|(engine, _)| *engine)
            .collect()
    }

    #[test]
    fn every_engine_has_paths_except_browser_storage() {
        for platform in PLATFORMS {
            for (engine, templates) in catalog(platform) {
                if *engine == "twine" || *engine == "construct" {
                    assert!(templates.is_empty(), "{platform:?} {engine}");
                } else {
                    assert!(!templates.is_empty(), "{platform:?} {engine}");
                }
            }
        }
        let engines = engines(Platform::Windows);
        for engine in [
            "rpgmaker-mv",
            "rpgmaker-classic",
            "wolf-rpg",
            "unity",
            "unreal",
            "gamemaker",
            "renpy",
            "godot",
            "twine",
            "flash",
            "witcher3",
            "palworld",
            "satisfactory",
            "terraria",
            "neverwinter-nights",
            "kotor",
            "ags",
            "nscripter",
            "emuera",
            "alicesoft",
            "artemis",
            "construct",
        ] {
            assert!(engines.contains(&engine), "{engine}");
        }
    }

    #[test]
    fn macos_and_linux_list_the_windows_engines_in_the_same_order() {
        let windows = engines(Platform::Windows);
        assert_eq!(engines(Platform::Linux), windows);
        let without_windows_only: Vec<&str> = windows
            .iter()
            .copied()
            .filter(|engine| !WINDOWS_ONLY.contains(engine))
            .collect();
        assert_eq!(engines(Platform::MacOs), without_windows_only);
    }

    #[test]
    fn each_list_uses_the_paths_of_its_system() {
        for (engine, templates) in catalog(Platform::Windows) {
            for template in *templates {
                assert!(!template.contains('/'), "{engine}: {template}");
            }
        }
        for platform in [Platform::MacOs, Platform::Linux] {
            for (engine, templates) in catalog(platform) {
                for template in *templates {
                    assert!(
                        !template.contains('\\'),
                        "{platform:?} {engine}: {template}"
                    );
                    // An absolute path starts at the home folder or an XDG base folder, so it
                    // expands to an absolute path; anything else is inside the game's folder.
                    assert!(
                        [
                            "%HOME%/",
                            "%XDG_CONFIG_HOME%/",
                            "%XDG_DATA_HOME%/",
                            "<game folder>"
                        ]
                        .iter()
                        .any(|start| template.starts_with(start)),
                        "{platform:?} {engine}: {template}"
                    );
                }
            }
        }
    }

    #[test]
    fn windows_only_games_are_in_their_proton_prefix_on_linux() {
        for (engine, templates) in catalog(Platform::Linux) {
            if WINDOWS_ONLY.contains(engine) {
                for template in *templates {
                    assert!(
                        template.starts_with("%HOME%/.steam/steam/steamapps/compatdata/"),
                        "{engine}: {template}"
                    );
                    assert!(
                        template.contains("/pfx/drive_c/users/steamuser/"),
                        "{engine}: {template}"
                    );
                }
            }
        }
    }

    #[test]
    fn relative_paths_have_no_folder() {
        let path = SaveLocationPath::new("<game folder>\\save");
        assert_eq!(path.shown, "<game folder>\\save");
        assert!(path.folder.is_none());
        assert!(!path.exists);
        let path = SaveLocationPath::new("<game folder>/save");
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
    fn a_placeholder_after_a_slash_opens_the_folder_above_it() {
        let root = tempfile::tempdir().unwrap();
        for rest in ["/<company>/<game>", "/unity.<company>.<game>.plist"] {
            let template = format!("{}{rest}", root.path().display());
            let path = SaveLocationPath::new(&template);
            assert_eq!(
                path.folder.as_deref(),
                Some(root.path().to_string_lossy().as_ref()),
                "{rest}"
            );
            assert!(path.exists, "{rest}");
        }
        // The folder above the placeholder does not exist here.
        let template = format!("{}/Epic/<game>/Saved", root.path().display());
        assert!(SaveLocationPath::new(&template).folder.is_none());
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

    #[test]
    fn xdg_folders_fall_back_to_their_defaults_under_home() {
        let only_home = |name: &str| (name == "HOME").then(|| "/home/player".to_string());
        assert_eq!(
            expand_vars("%XDG_CONFIG_HOME%/unity3d", &only_home),
            "/home/player/.config/unity3d"
        );
        assert_eq!(
            expand_vars("%XDG_DATA_HOME%/godot", &only_home),
            "/home/player/.local/share/godot"
        );

        let custom = |name: &str| match name {
            "HOME" => Some("/home/player/".to_string()),
            "XDG_DATA_HOME" => Some("/data".to_string()),
            "XDG_CONFIG_HOME" => Some(String::new()),
            _ => None,
        };
        assert_eq!(expand_vars("%XDG_DATA_HOME%/godot", &custom), "/data/godot");
        // An empty variable counts as not set.
        assert_eq!(
            expand_vars("%XDG_CONFIG_HOME%/retroarch", &custom),
            "/home/player/.config/retroarch"
        );

        // Without HOME there is nothing to fall back to, and other variables have no default.
        let nothing = |_: &str| -> Option<String> { None };
        assert_eq!(
            expand_vars("%XDG_DATA_HOME%/godot", &nothing),
            "%XDG_DATA_HOME%/godot"
        );
        assert_eq!(expand_vars("%HOME%/.renpy", &nothing), "%HOME%/.renpy");
    }
}
