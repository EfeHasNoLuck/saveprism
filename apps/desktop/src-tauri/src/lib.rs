//! SavePrism desktop host: Tauri commands over one open document.
//!
//! The logic lives in [`session`] (no Tauri types). The commands here only lock the shared
//! `Option<Session>` and call it. They are `async` so a large file does not block the window thread.

pub mod session;

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, PoisonError};

use session::{
    ApiError, Change, Field, GameView, HistoryResult, HistoryState, NodeRow, OpenInfo, Page,
    SaveResult, Session, app_codes,
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

/// Saves to `target` (the open file = overwrite with backup; another path = save as).
#[tauri::command]
async fn save(
    state: State<'_, AppState>,
    target: String,
    overwrite: Option<bool>,
) -> Result<SaveResult, ApiError> {
    state.with_session(|session| session.save(&target, overwrite.unwrap_or(false)))
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
            get_children,
            search,
            edit_value,
            game_view,
            field_info,
            pending_changes,
            discard_changes,
            save,
            export_json,
            export_json_file,
            apply_json,
            undo,
            redo,
            history_state,
            notices_available,
            open_notices,
        ])
        .run(tauri::generate_context!());

    if let Err(err) = result {
        eprintln!("SP-APP-001: failed to start SavePrism: {err}");
        std::process::exit(1);
    }
}
