//! SavePrism desktop host: Tauri commands over one open document.
//!
//! The logic lives in [`session`] (no Tauri types). The commands here only lock the shared
//! `Option<Session>` and call it. They are `async` so a large file does not block the window thread.

pub mod accent;
pub mod mcp;
pub mod session;

use std::ffi::OsString;
use std::path::{Path as FsPath, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use session::folders::{self, BackgroundImage, DefaultPaths};
use session::{
    ApiError, BackupRow, Change, CompareResult, Field, FieldKind, FileReport, GameView,
    HistoryResult, HistoryState, NodeRow, OpenInfo, Page, SaveLocation, SaveResult, Session,
    ValueChild, ValueHits, W3CardsView, app_codes, read_file_report, save_locations,
};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;

/// The one open document (v0.1 keeps a single document at a time).
#[derive(Default)]
pub struct AppState {
    session: Mutex<Option<Session>>,
    /// A file a second instance (or this start) asked to open; the window takes it once.
    pending_file: Mutex<Option<String>>,
}

/// The file path in the start arguments: the first argument after the executable, unless it is
/// `mcp` (that starts the MCP server on stdio, see `main.rs`).
fn file_argument<I, S>(args: I) -> Option<String>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let mut args = args.into_iter();
    args.next()?;
    let argument: OsString = args.next()?.into();
    let text = argument.to_string_lossy();
    if text.is_empty() || text == "mcp" {
        return None;
    }
    Some(text.into_owned())
}

/// The backup folders the window passed, as file system paths (empty keeps the default).
fn backup_paths(folders: Option<Vec<String>>) -> Vec<PathBuf> {
    folders
        .unwrap_or_default()
        .into_iter()
        .filter(|folder| !folder.trim().is_empty())
        .map(PathBuf::from)
        .collect()
}

/// The open save's path, when a document is open (for the folder checks).
fn save_path(state: &AppState) -> Option<PathBuf> {
    state
        .lock()
        .as_ref()
        .map(|session| session.file_path().to_path_buf())
}

impl AppState {
    fn lock(&self) -> MutexGuard<'_, Option<Session>> {
        // A panic while the lock was held cannot leave the session half-built in a way that
        // matters more than losing the whole app, so a poisoned lock is used as it is.
        self.session.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Runs `action` on the open document, or fails with "no file is open".
    fn with_session<T>(
        &self,
        action: impl FnOnce(&mut Session) -> Result<T, ApiError>,
    ) -> Result<T, ApiError> {
        let mut guard = self.lock();
        match guard.as_mut() {
            Some(session) => action(session),
            None => Err(ApiError::no_document()),
        }
    }

    /// Takes the file path a start (this one or a second instance) handed over, if any.
    fn take_pending_file(&self) -> Option<String> {
        self.pending_file
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
    }

    /// Hands a file path to the window; it opens it through the same checks as a dropped file.
    fn set_pending_file(&self, path: String) {
        *self
            .pending_file
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(path);
    }
}

/// Returns the version of the SavePrism core crate.
#[tauri::command]
fn core_version() -> &'static str {
    saveprism_core::VERSION
}

/// The file a start handed over ("Decode with SavePrism" or a second instance), once. The window
/// opens it through the same checks and guard as a file dropped on the window.
#[tauri::command]
fn take_pending_file(state: State<'_, AppState>) -> Option<String> {
    state.take_pending_file()
}

/// Opens a file (replacing the open document). `password` is only used for encrypted files and is
/// kept in memory only.
#[tauri::command]
async fn open_file(
    state: State<'_, AppState>,
    path: String,
    password: Option<String>,
    backup_folders: Option<Vec<String>>,
) -> Result<OpenInfo, ApiError> {
    let mut session = Session::open(&path, password, None)?;
    session.set_backup_roots(&backup_paths(backup_folders));
    let info = session.info();
    *state.lock() = Some(session);
    Ok(info)
}

/// Opens an encrypted file while also trying the game folder the player picked, so a save whose
/// password is in the game's own files opens without typing it.
#[tauri::command]
async fn open_file_with_game_folder(
    state: State<'_, AppState>,
    path: String,
    game_folder: String,
    backup_folders: Option<Vec<String>>,
) -> Result<OpenInfo, ApiError> {
    let mut session =
        Session::open_with_game_folder(&path, None, None, PathBuf::from(game_folder))?;
    session.set_backup_roots(&backup_paths(backup_folders));
    let info = session.info();
    *state.lock() = Some(session);
    Ok(info)
}

/// Opens a pasted save code (replacing the open document). The text is kept as SavePrism's own file
/// in the app data folder; a code that does not open leaves no file behind.
#[tauri::command]
async fn open_code(
    state: State<'_, AppState>,
    text: String,
    app_folder: Option<String>,
    backup_folders: Option<Vec<String>>,
) -> Result<OpenInfo, ApiError> {
    let roots = backup_paths(backup_folders);
    let session = Session::open_code_in(&text, app_folder.map(PathBuf::from), &roots)?;
    let info = session.info();
    *state.lock() = Some(session);
    Ok(info)
}

/// The open document as a save code (pending edits included, every edited value checked). Formats
/// that are not text are refused. Nothing is written and the pending edits stay.
#[tauri::command]
async fn copy_code(state: State<'_, AppState>) -> Result<String, ApiError> {
    state.with_session(|session| session.copy_code())
}

/// One page of the children of the node at `path` (`limit` is capped at 500).
#[tauri::command]
async fn get_children(
    state: State<'_, AppState>,
    path: String,
    offset: usize,
    limit: usize,
) -> Result<Page<NodeRow>, ApiError> {
    state.with_session(|session| session.get_children(&path, offset, limit))
}

/// One page of the children of the group at `path` as the Values list shows them: wrappers
/// folded, a map entry as one row named after its key (`limit` is capped at 500).
#[tauri::command]
async fn value_children(
    state: State<'_, AppState>,
    path: String,
    offset: usize,
    limit: usize,
) -> Result<Page<ValueChild>, ApiError> {
    state.with_session(|session| session.value_children(&path, offset, limit))
}

/// The search of the Values list: entries whose own key or value holds `query`.
#[tauri::command]
async fn value_search(state: State<'_, AppState>, query: String) -> Result<ValueHits, ApiError> {
    state.with_session(|session| Ok(session.value_search(&query)))
}

/// The name a player reads for each path ("STATUS · 0 · currentValue"), in the same order.
#[tauri::command]
async fn value_labels(
    state: State<'_, AppState>,
    paths: Vec<String>,
) -> Result<Vec<String>, ApiError> {
    state.with_session(|session| Ok(session.value_labels(&paths)))
}

/// Case-insensitive search over paths and scalar values.
#[tauri::command]
async fn search(
    state: State<'_, AppState>,
    query: String,
    offset: usize,
    limit: usize,
) -> Result<Page<NodeRow>, ApiError> {
    state.with_session(|session| session.search(&query, offset, limit))
}

/// Changes one scalar value; `text` is parsed by the node's current type.
#[tauri::command]
async fn edit_value(
    state: State<'_, AppState>,
    path: String,
    text: String,
) -> Result<NodeRow, ApiError> {
    state.with_session(|session| session.edit_value(&path, &text))
}

/// Changes every value in `paths` to `text` as one undo step; nothing changes on an error.
#[tauri::command]
async fn edit_values(
    state: State<'_, AppState>,
    paths: Vec<String>,
    text: String,
) -> Result<HistoryResult, ApiError> {
    state.with_session(|session| session.edit_values(&paths, &text))
}

/// The game view: RPG Maker sections for MV/MZ saves, else a flat list of values.
#[tauri::command]
async fn game_view(state: State<'_, AppState>) -> Result<GameView, ApiError> {
    state.with_session(|session| session.game_view())
}

/// Type, limits and original value of the scalar at `path`.
#[tauri::command]
async fn field_info(state: State<'_, AppState>, path: String) -> Result<Field, ApiError> {
    state.with_session(|session| session.field_info(&path))
}

/// Every Gwent card for the open Witcher 3 save, with names read from the player's own game files.
/// When no game folder is found the result is the `pickGameFolder` state, not an error; pass the
/// folder the player picked as `gameFolder` to try it. The card catalog is kept for this session.
#[tauri::command]
async fn w3_cards(
    state: State<'_, AppState>,
    game_folder: Option<String>,
) -> Result<W3CardsView, ApiError> {
    state.with_session(|session| Ok(session.w3_cards(game_folder.as_deref())))
}

/// Reads the names from a game data folder the player picked and returns the view with them.
/// MV/MZ take the `data` folder or the game folder; XP, VX and VX Ace the `Data` folder or the
/// game folder.
#[tauri::command]
async fn set_data_folder(state: State<'_, AppState>, folder: String) -> Result<GameView, ApiError> {
    state.with_session(|session| session.set_data_folder(&folder))
}

/// Adds `count` (1 to 99) of the database entry `id` to the party's `list` ("items", "weapons" or
/// "armors") of an RPG Maker save. One undo step.
#[tauri::command]
async fn add_inventory(
    state: State<'_, AppState>,
    list: String,
    id: i64,
    count: String,
) -> Result<HistoryResult, ApiError> {
    state.with_session(|session| session.add_inventory(&list, id, &count))
}

/// Adds the value `key` (of kind `kind`, typed as `text`) to the object at `parent`: any object of
/// a JSON file, the top of an Easy Save 3 file, or the story variables of a SugarCube save.
#[tauri::command]
async fn add_value(
    state: State<'_, AppState>,
    parent: String,
    key: String,
    kind: FieldKind,
    text: String,
) -> Result<HistoryResult, ApiError> {
    state.with_session(|session| session.add_value(&parent, &key, kind, &text))
}

/// Removes the value at `path` where the format allows it (JSON and Easy Save 3 entries, SugarCube
/// story variables, MV/MZ inventory entries, entries added in this session). One undo step.
#[tauri::command]
async fn remove_value(state: State<'_, AppState>, path: String) -> Result<HistoryResult, ApiError> {
    state.with_session(|session| session.remove_value(&path))
}

/// The edits made since opening or the last save.
#[tauri::command]
async fn pending_changes(state: State<'_, AppState>) -> Result<Vec<Change>, ApiError> {
    state.with_session(|session| Ok(session.pending_changes()))
}

/// Throws the pending edits away.
#[tauri::command]
async fn discard_changes(state: State<'_, AppState>) -> Result<(), ApiError> {
    state.with_session(|session| session.discard_changes())
}

/// Closes the open document; its pending edits are dropped and nothing is written.
#[tauri::command]
async fn close_file(state: State<'_, AppState>) -> Result<(), ApiError> {
    *state.lock() = None;
    Ok(())
}

/// Saves to `target` (the open file = overwrite with backup; another path = save as).
/// `keep_backups` is the number of backups to keep for the file (1 to 50, 10 when missing).
#[tauri::command]
async fn save(
    state: State<'_, AppState>,
    target: String,
    overwrite: Option<bool>,
    keep_backups: Option<usize>,
    keep_days: Option<u32>,
    backup_folders: Option<Vec<String>>,
) -> Result<SaveResult, ApiError> {
    state.with_session(|session| {
        session.set_backup_roots(&backup_paths(backup_folders));
        session.save_with_days(&target, overwrite.unwrap_or(false), keep_backups, keep_days)
    })
}

/// Pretty JSON text of the open document (the view the JSON editor edits; at most 50 MB).
#[tauri::command]
async fn export_json(state: State<'_, AppState>) -> Result<String, ApiError> {
    state.with_session(|session| session.export_json())
}

/// Writes the text of `export_json` to `target` as UTF-8 (temporary file, then rename) and returns
/// the written path. Refuses the open save file, folders and names that do not end in `.json`.
#[tauri::command]
async fn export_json_file(state: State<'_, AppState>, target: String) -> Result<String, ApiError> {
    state.with_session(|session| session.export_json_file(&target))
}

/// Applies the edited text of `export_json`: only scalar values may change. Everything is checked
/// first and nothing is applied on an error. Returns the number of values that changed.
#[tauri::command]
async fn apply_json(state: State<'_, AppState>, text: String) -> Result<usize, ApiError> {
    state.with_session(|session| session.apply_json(&text))
}

/// Writes a theme file. Same checks as "Export .json": never over the open save, never a folder,
/// a name ending in `.saveprism-theme.json`, and a temporary file renamed into place.
#[tauri::command]
async fn export_theme_file(
    state: State<'_, AppState>,
    target: String,
    text: String,
) -> Result<String, ApiError> {
    let save = save_path(&state);
    session::theme_file::write(&target, &text, save.as_deref())
}

/// Reads a theme file (at most 64 KB). The window validates the JSON before applying anything.
#[tauri::command]
async fn import_theme_file(source: String) -> Result<String, ApiError> {
    session::theme_file::read(&source)
}

/// Takes back the last action (one edit, "Undo all" or one JSON Apply).
#[tauri::command]
async fn undo(state: State<'_, AppState>) -> Result<HistoryResult, ApiError> {
    state.with_session(|session| session.undo())
}

/// Does again the action that was taken back last.
#[tauri::command]
async fn redo(state: State<'_, AppState>) -> Result<HistoryResult, ApiError> {
    state.with_session(|session| session.redo())
}

/// Whether there is something to undo or redo.
#[tauri::command]
async fn history_state(state: State<'_, AppState>) -> Result<HistoryState, ApiError> {
    state.with_session(|session| Ok(session.history_state()))
}

/// Where saves usually are on Windows, per engine, with the folders that exist on this PC.
#[tauri::command]
fn save_location_list() -> Vec<SaveLocation> {
    save_locations()
}

/// Opens one folder of the save location list in the file manager. Only a folder the list itself
/// found on this PC is opened, so the page cannot ask for any other path.
#[tauri::command]
fn open_save_location(app: AppHandle, folder: String) -> Result<(), ApiError> {
    let listed = save_locations()
        .into_iter()
        .flat_map(|location| location.paths)
        .any(|path| path.exists && path.folder.as_deref() == Some(folder.as_str()));
    if !listed {
        return Err(ApiError::new(
            app_codes::LOCATION_OPEN,
            "this folder is not one of the save folders found on this PC",
            "Open the dialog again; the folder may have been moved.",
        ));
    }
    app.opener()
        .open_path(folder.as_str(), None::<&str>)
        .map_err(|error| {
            ApiError::new(
                app_codes::LOCATION_OPEN,
                format!("the folder could not be opened: {error}"),
                "Open it by hand in File Explorer.",
            )
        })
}

/// Compares the open document with another save. The other file is opened read-only (same
/// detection, no password prompt: encrypted files are refused) and nothing is written.
#[tauri::command]
async fn compare_with(state: State<'_, AppState>, path: String) -> Result<CompareResult, ApiError> {
    state.with_session(|session| session.compare_with(&path))
}

/// The backups of the open file, newest first.
#[tauri::command]
async fn list_backups(
    state: State<'_, AppState>,
    backup_folders: Option<Vec<String>>,
) -> Result<Vec<BackupRow>, ApiError> {
    state.with_session(|session| {
        session.set_backup_roots(&backup_paths(backup_folders));
        session.backups()
    })
}

/// Puts the backup `name` back in place of the open file (the current file is backed up first) and
/// opens the file again. Pending edits are lost. `keep_backups` is the number of backups to keep
/// for the file (1 to 50, 10 when missing).
#[tauri::command]
async fn restore_backup(
    state: State<'_, AppState>,
    name: String,
    keep_backups: Option<usize>,
    keep_days: Option<u32>,
    backup_folders: Option<Vec<String>>,
) -> Result<OpenInfo, ApiError> {
    let mut guard = state.lock();
    let session = guard.as_mut().ok_or_else(ApiError::no_document)?;
    session.set_backup_roots(&backup_paths(backup_folders));
    let restored = session.restore_backup_with_days(&name, keep_backups, keep_days)?;
    let info = restored.info();
    *guard = Some(restored);
    Ok(info)
}

/// A short, local report about the file at `path` for a GitHub issue (extension, size, first bytes,
/// SHA-256), made when SavePrism does not know the format. It holds no path or file name, the file
/// is only read, and nothing is sent anywhere. Only a regular file is accepted.
#[tauri::command]
async fn file_report(path: String) -> Result<FileReport, ApiError> {
    read_file_report(&PathBuf::from(path))
}

/// Path of the running SavePrism executable, for the setup snippet of an AI app.
#[tauri::command]
fn mcp_command() -> Result<String, ApiError> {
    std::env::current_exe()
        .map(|path| path.to_string_lossy().into_owned())
        .map_err(|_| ApiError::internal())
}

/// The folders SavePrism uses before the player changes them.
#[tauri::command]
fn default_paths() -> Result<DefaultPaths, ApiError> {
    folders::default_paths()
}

/// Checks a folder picked for the backups and returns the folder list to store: the new folder
/// first, then every folder used before (and the default), so older backups stay listed.
#[tauri::command]
fn set_backup_folder(
    state: State<'_, AppState>,
    folder: String,
    current: Option<Vec<String>>,
) -> Result<Vec<String>, ApiError> {
    let save = save_path(&state);
    let picked = folders::check_folder(FsPath::new(&folder), save.as_deref())?;
    let mut roots = vec![picked.to_string_lossy().into_owned()];
    let mut before = current.unwrap_or_default();
    if before.is_empty()
        && let Ok(default) = folders::default_paths()
    {
        before.push(default.backup_dir);
    }
    for root in before {
        if !root.trim().is_empty() && !roots.iter().any(|seen| seen == &root) {
            roots.push(root);
        }
    }
    Ok(roots)
}

/// Checks a folder picked for the app's own files and moves the pasted codes and the background
/// photo there. Returns the new folder.
#[tauri::command]
fn set_app_folder(
    state: State<'_, AppState>,
    folder: String,
    old_folder: Option<String>,
) -> Result<String, ApiError> {
    let save = save_path(&state);
    let old = old_folder
        .filter(|path| !path.trim().is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            folders::default_paths()
                .ok()
                .map(|paths| PathBuf::from(paths.app_dir))
        });
    let picked = folders::set_app_folder(FsPath::new(&folder), old.as_deref(), save.as_deref())?;
    Ok(picked.to_string_lossy().into_owned())
}

/// Opens an existing folder in the file manager (a folder the player picked in Settings).
#[tauri::command]
fn open_folder(app: AppHandle, folder: String) -> Result<(), ApiError> {
    let path = PathBuf::from(&folder);
    if !path.is_dir() {
        return Err(ApiError::new(
            session::app_codes::FOLDER,
            format!("\"{}\" is not a folder", path.display()),
            "Open it by hand in File Explorer.",
        ));
    }
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(|error| {
            ApiError::new(
                session::app_codes::FOLDER,
                format!("the folder could not be opened: {error}"),
                "Open it by hand in File Explorer.",
            )
        })
}

/// The app folder used by the background commands (the player's choice, else the default).
fn app_dir_of(app_folder: Option<String>) -> Result<PathBuf, ApiError> {
    match app_folder.filter(|path| !path.trim().is_empty()) {
        Some(path) => Ok(PathBuf::from(path)),
        None => folders::default_paths().map(|paths| PathBuf::from(paths.app_dir)),
    }
}

/// Copies the picked image into the app folder as the background photo (replacing the previous
/// one) and returns it for the Preview and the whole window.
#[tauri::command]
fn choose_background(
    source: String,
    app_folder: Option<String>,
) -> Result<BackgroundImage, ApiError> {
    let app = app_dir_of(app_folder)?;
    folders::import_background(FsPath::new(&source), &app)
}

/// The background photo stored in the app folder, when there is one.
#[tauri::command]
fn background_image(app_folder: Option<String>) -> Result<Option<BackgroundImage>, ApiError> {
    let app = app_dir_of(app_folder)?;
    folders::load_background(&app)
}

/// The Windows accent colour as `#rrggbb`, or null when it cannot be read. The window reads it
/// again whenever it gets focus, so a change in Windows Personalization shows up at once.
#[tauri::command]
fn app_accent_color() -> Option<String> {
    accent::read_accent_color()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let startup_file = file_argument(std::env::args_os());
    let result = tauri::Builder::default()
        // Must be the first plugin: a second start hands its file path to this window and exits.
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            if let Some(path) = file_argument(argv) {
                app.state::<AppState>().set_pending_file(path);
                let _ = app.emit("open-path", ());
            }
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState {
            session: Mutex::new(None),
            pending_file: Mutex::new(startup_file),
        })
        .invoke_handler(tauri::generate_handler![
            core_version,
            app_accent_color,
            take_pending_file,
            open_file,
            open_file_with_game_folder,
            open_code,
            copy_code,
            get_children,
            value_children,
            value_labels,
            value_search,
            search,
            edit_value,
            edit_values,
            game_view,
            w3_cards,
            set_data_folder,
            add_inventory,
            add_value,
            remove_value,
            field_info,
            pending_changes,
            discard_changes,
            close_file,
            save,
            export_json,
            export_json_file,
            export_theme_file,
            import_theme_file,
            apply_json,
            undo,
            redo,
            history_state,
            save_location_list,
            open_save_location,
            compare_with,
            file_report,
            list_backups,
            restore_backup,
            mcp_command,
            default_paths,
            set_backup_folder,
            set_app_folder,
            open_folder,
            choose_background,
            background_image,
        ])
        .run(tauri::generate_context!());

    if let Err(err) = result {
        eprintln!("SP-APP-001: failed to start SavePrism: {err}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::file_argument;

    #[test]
    fn start_arguments_pick_the_file_after_the_exe() {
        assert_eq!(
            file_argument(["saveprism-desktop.exe", r"C:\Saves\slot1.dat"]).as_deref(),
            Some(r"C:\Saves\slot1.dat")
        );
        assert_eq!(
            file_argument(["saveprism-desktop.exe", "/home/ann/slot1.sav"]).as_deref(),
            Some("/home/ann/slot1.sav")
        );
    }

    #[test]
    fn start_arguments_ignore_mcp_and_missing_arguments() {
        assert_eq!(file_argument(["saveprism-desktop.exe", "mcp"]), None);
        assert_eq!(file_argument(["saveprism-desktop.exe"]), None);
        assert_eq!(file_argument(Vec::<String>::new()), None);
    }

    #[test]
    fn a_second_instance_uses_its_own_exe_as_the_first_argument() {
        let args = vec![
            r"C:\Program Files\SavePrism\saveprism-desktop.exe".to_string(),
            r"D:\game\save.json".to_string(),
        ];
        assert_eq!(file_argument(args).as_deref(), Some(r"D:\game\save.json"));
    }
}
