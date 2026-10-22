//! SavePrism desktop host: Tauri commands over one open document.
//!
//! The logic lives in [`session`] (no Tauri types). The commands here only lock the shared
//! `Option<Session>` and call it. They are `async` so a large file does not block the window thread.

pub mod session;

use std::sync::{Mutex, MutexGuard, PoisonError};

use session::{ApiError, Change, Field, GameView, NodeRow, OpenInfo, Page, SaveResult, Session};
use tauri::State;

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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
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
        ])
        .run(tauri::generate_context!());

    if let Err(err) = result {
        eprintln!("SP-APP-001: failed to start SavePrism: {err}");
        std::process::exit(1);
    }
}
