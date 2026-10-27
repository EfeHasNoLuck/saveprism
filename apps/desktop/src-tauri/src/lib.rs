//! SavePrism desktop host: Tauri commands over one open document.
//!
//! The logic lives in [`session`] (no Tauri types). The commands here only lock the shared
//! `Option<Session>` and call it. They are `async` so a large file does not block the window thread.

pub mod mcp;
pub mod session;

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, PoisonError};

use session::{
    ApiError, BackupRow, Change, CompareResult, Field, FieldKind, FileReport, GameView,
    HistoryResult, HistoryState, NodeRow, OpenInfo, Page, SaveLocation, SaveResult, Session,
    ValueChild, ValueHits, W3CardsView, app_codes, read_file_report, save_locations,
};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_opener::OpenerExt;

/// The one open document (v0.1 keeps a single document at a time).
#[derive(Default)]
pub struct AppState {
    session: Mutex<Option<Session>>,
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
}

/// Returns the version of the SavePrism core crate.
#[tauri::command]
fn core_version() -> &'static str {
    saveprism_core::VERSION
}

/// Opens a file (replacing the open document). `password` is only used for encrypted files and is
/// kept in memory only.
#[tauri::command]
async fn open_file(
    state: State<'_, AppState>,
    path: String,
    password: Option<String>,
) -> Result<OpenInfo, ApiError> {
    let session = Session::open(&path, password, None)?;
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
) -> Result<OpenInfo, ApiError> {
    let session = Session::open_with_game_folder(&path, None, None, PathBuf::from(game_folder))?;
    let info = session.info();
    *state.lock() = Some(session);
    Ok(info)
}

/// Opens a pasted save code (replacing the open document). The text is kept as SavePrism's own file
/// in the app data folder; a code that does not open leaves no file behind.
#[tauri::command]
async fn open_code(state: State<'_, AppState>, text: String) -> Result<OpenInfo, ApiError> {
    let session = Session::open_code(&text, None)?;
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
) -> Result<SaveResult, ApiError> {
    state.with_session(|session| session.save(&target, overwrite.unwrap_or(false), keep_backups))
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
async fn list_backups(state: State<'_, AppState>) -> Result<Vec<BackupRow>, ApiError> {
    state.with_session(|session| session.backups())
}

/// Puts the backup `name` back in place of the open file (the current file is backed up first) and
/// opens the file again. Pending edits are lost. `keep_backups` is the number of backups to keep
/// for the file (1 to 50, 10 when missing).
#[tauri::command]
async fn restore_backup(
    state: State<'_, AppState>,
    name: String,
    keep_backups: Option<usize>,
) -> Result<OpenInfo, ApiError> {
    let mut guard = state.lock();
    let session = guard.as_ref().ok_or_else(ApiError::no_document)?;
    let restored = session.restore_backup(&name, keep_backups)?;
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

/// The file name of the third-party notices in the installer.
const NOTICES_FILE: &str = "THIRD_PARTY_NOTICES.md";

/// The bundled notices file, if this build has it: in the resource folder, either at its top or
/// where `bundle.resources` puts a path given with `..` (`_up_/_up_/_up_/`).
fn notices_file(app: &AppHandle) -> Option<PathBuf> {
    let dir = app.path().resource_dir().ok()?;
    [
        dir.join(NOTICES_FILE),
        dir.join("_up_")
            .join("_up_")
            .join("_up_")
            .join(NOTICES_FILE),
    ]
    .into_iter()
    .find(|path| path.is_file())
}

/// Path of the running SavePrism executable, for the setup snippet of an AI app.
#[tauri::command]
fn mcp_command() -> Result<String, ApiError> {
    std::env::current_exe()
        .map(|path| path.to_string_lossy().into_owned())
        .map_err(|_| ApiError::internal())
}

/// True when the third-party notices are bundled with this build.
#[tauri::command]
fn notices_available(app: AppHandle) -> bool {
    notices_file(&app).is_some()
}

/// Opens the bundled third-party notices with the system's default program.
#[tauri::command]
fn open_notices(app: AppHandle) -> Result<(), ApiError> {
    let path = notices_file(&app).ok_or_else(|| {
        ApiError::new(
            app_codes::NOTICES,
            format!("{NOTICES_FILE} is not part of this build"),
            "Read it in the SavePrism repository instead.",
        )
    })?;
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(|error| {
            ApiError::new(
                app_codes::NOTICES,
                format!("{NOTICES_FILE} could not be opened: {error}"),
                "Open the file by hand from the SavePrism install folder.",
            )
        })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            core_version,
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
            notices_available,
            open_notices,
            mcp_command,
        ])
        .run(tauri::generate_context!());

    if let Err(err) = result {
        eprintln!("SP-APP-001: failed to start SavePrism: {err}");
        std::process::exit(1);
    }
}
