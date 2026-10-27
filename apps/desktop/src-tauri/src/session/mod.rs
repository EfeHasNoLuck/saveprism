//! The open document of the desktop app, with no Tauri types so it can be unit tested.
//!
//! v0.1 keeps one document at a time: [`Session::open`] detects and decodes a file and runs the
//! round-trip guard; [`Session::get_children`] and [`Session::search`] read the tree;
//! [`Session::edit_value`] changes one scalar, [`Session::edit_values`] many scalars to the same text
//! and [`Session::apply_json`] the scalars of an edited JSON view; [`Session::undo`] and
//! [`Session::redo`] walk the edit history; [`Session::save`] encodes, writes with the core safe
//! write (backup, temp file, atomic replace) and checks every edited value in the written file;
//! [`Session::backups`] lists the backups of the open file and [`Session::restore_backup`] puts one
//! back the same safe way. [`Session::open_code`] opens a pasted save code and
//! [`Session::copy_code`] gives the edited document back as a code. [`Session::add_value`],
//! [`Session::add_inventory`] and [`Session::remove_value`] add and remove entries where the format
//! allows it (see `structure.rs`); [`Session::set_data_folder`] reads names from a folder the player
//! picked.
//!
//! Errors cross the IPC boundary as [`ApiError`] (`code`, `message`, `hint`). The password of an
//! encrypted file lives in memory only; it is never logged, put into a message or written to disk.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path as FsPath, PathBuf};

use saveprism_codecs::es3::{
    ES3_BAD_DATA, ES3_PASSWORD_REQUIRED, ES3_WRONG_PASSWORD, password_unlocks_first_block,
};
use saveprism_codecs::terraria::TERRARIA_PASSWORD_REQUIRED;
use saveprism_codecs::terraria::aes::player_password_unlocks_first_block;
use saveprism_core::detect::{DetectOptions, detect};
use saveprism_core::guard::{Verdict, check_round_trip};
use saveprism_core::io::{WriteOptions, default_backup_root, safe_write};
use saveprism_core::{
    Confidence, DecodeOutput, Edit, Error, ErrorCode, Limits, NodeFlags, NodeId, OwnedValue,
    Params, Path, Recipe, Registry, SaveDoc, SniffCtx, Step, Value, apply_edits, codes, pipeline,
};
use serde::Serialize;

mod backups;
mod code;
mod compare;
mod export;
mod game;
mod gwent;
mod history;
mod json_apply;
mod keyfinder;
mod lcf;
mod locations;
mod naninovel;
mod renpy;
mod report;
mod rgss;
mod satisfactory;
mod structure;
mod sugarcube;
mod w3cards;

pub use backups::BackupRow;
pub use code::MAX_PASTED;
pub use compare::{CompareKind, CompareResult, CompareRow};
use game::check_bounds_in;
pub use game::{
    Actor, AddLists, Bonus, Catalog, Entry, Field, FieldKind, GameView, NamedId, NoteKind,
    ReadOnlyReason, RowNote, RpgMakerView, ValueRow, Witcher3View,
};
pub use gwent::{GwentCard, GwentDeck, GwentView};
use history::{DocStep, History, MAX_VALUES, Mutation, scalar_owned};
pub use history::{HistoryResult, HistoryState};
use keyfinder::KeyFinder;
pub use locations::{SaveLocation, SaveLocationPath, save_locations};
use renpy::RenpyKeySource;
pub use renpy::{
    NOTICE_KEY_SIGNED, NOTICE_KEY_SIGNED_PERSISTENT, NOTICE_SIGNED_PERSISTENT, NOTICE_SIGNED_SAVE,
};
pub use report::{EntropyClass, FileReport, read_file_report};
pub use satisfactory::{InventoryKind, SatisfactoryInventory, SatisfactoryStack, SatisfactoryView};
pub use sugarcube::SugarCubeView;
pub use w3cards::W3CardsView;

#[cfg(test)]
mod tests;

/// Most rows one call returns.
pub const MAX_PAGE: usize = 500;
/// Backups kept per file when the settings do not say.
pub const DEFAULT_KEEP_BACKUPS: usize = 10;
/// Highest number of backups the settings may ask for.
pub const MAX_KEEP_BACKUPS: usize = 50;
/// Most hits one search collects.
pub const MAX_SEARCH_HITS: usize = 10_000;
/// Longest value preview, in characters.
const PREVIEW_CHARS: usize = 120;
/// Longest piece of user input quoted in a message, in characters.
const QUOTE_CHARS: usize = 40;
/// The encryption layer of Easy Save 3.
const ES3_AES_LAYER: &str = "crypto.es3-aes";
/// The encryption layer of a Terraria player save.
const TERRARIA_PLR_LAYER: &str = "crypto.terraria-plr";
/// The notice of a Terraria file whose values look misplaced (the file is open read-only).
const NOTICE_TERRARIA_LAYOUT: &str = "terraria-layout";

/// The backups to keep: the setting, clamped to 1..=50, or 10 when it is missing.
pub(crate) fn clamp_keep_backups(value: Option<usize>) -> usize {
    value
        .unwrap_or(DEFAULT_KEEP_BACKUPS)
        .clamp(1, MAX_KEEP_BACKUPS)
}

/// Error codes of the desktop app itself (the core and the codecs keep their own).
pub mod app_codes {
    /// No file is open.
    pub const NO_DOCUMENT: &str = "SP-APP-002";
    /// The open file cannot be written back safely, so it is read-only.
    pub const READ_ONLY_DOCUMENT: &str = "SP-APP-003";
    /// The text typed for a value does not fit the value's type.
    pub const BAD_VALUE: &str = "SP-APP-004";
    /// The number typed is outside the range of the value's type.
    pub const OUT_OF_RANGE: &str = "SP-APP-005";
    /// "Save as" would replace a different existing file.
    pub const TARGET_EXISTS: &str = "SP-APP-006";
    /// The node is a container or has no editable type.
    pub const NOT_EDITABLE_NODE: &str = "SP-APP-007";
    /// "Export .json" would write onto the open save file, a folder or a name that is not `.json`.
    pub const EXPORT_TARGET: &str = "SP-APP-008";
    /// Internal state could not be used (lock poisoned).
    pub const INTERNAL: &str = "SP-APP-009";
    /// The edited JSON text is not valid JSON.
    pub const JSON_TEXT: &str = "SP-APP-010";
    /// The edited JSON text changes more than values (fields, list items, map keys, placeholders).
    pub const JSON_SHAPE: &str = "SP-APP-011";
    /// The third-party notices are not part of this build, or could not be opened.
    pub const NOTICES: &str = "SP-APP-012";
    /// The backup asked for does not exist (any more).
    pub const BACKUP_NOT_FOUND: &str = "SP-APP-013";
    /// The backup does not open as a save, so it is not put back.
    pub const BACKUP_UNREADABLE: &str = "SP-APP-014";
    /// One edit names more values than one step may hold.
    pub const TOO_MANY_VALUES: &str = "SP-APP-015";
    /// The pasted save code is empty.
    pub const EMPTY_CODE: &str = "SP-APP-016";
    /// The save format is not text, so it has no code to copy.
    pub const NOT_TEXT_CODE: &str = "SP-APP-017";
    /// The picked folder is not a folder, or holds no game data this save can use.
    pub const DATA_FOLDER: &str = "SP-APP-018";
    /// The open file has no game sections that take names from a data folder.
    pub const NO_GAME_SECTIONS: &str = "SP-APP-019";
    /// The entry to add is there already.
    pub const ENTRY_EXISTS: &str = "SP-APP-020";
    /// Nothing can be added here (the format or the container does not allow it).
    pub const NOT_ADDABLE: &str = "SP-APP-021";
    /// This entry cannot be removed (the format or the container does not allow it).
    pub const NOT_REMOVABLE: &str = "SP-APP-022";
    /// The open file changed on disk after it was opened or last saved.
    pub const FILE_CHANGED: &str = "SP-APP-023";
    /// An argument of an MCP tool has a value the tool cannot use.
    pub const MCP_ARGUMENT: &str = "SP-APP-024";
    /// The file picked to compare with is encrypted, so it cannot be opened without a password.
    pub const COMPARE_ENCRYPTED: &str = "SP-APP-025";
    /// A save folder from the "Where is my save?" list could not be opened.
    pub const LOCATION_OPEN: &str = "SP-APP-026";
    /// A file report was asked for something that is not a regular file (a folder, a link).
    pub const FILE_REPORT: &str = "SP-APP-027";
}

/// An error as the UI receives it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ApiError {
    /// Stable code such as `SP-ES3-005`; the UI maps it to a translated text.
    pub code: String,
    /// Technical message in English (the reason).
    pub message: String,
    /// What the user can do about it (the fix).
    pub hint: String,
}

impl ApiError {
    /// Creates an error from its three parts.
    pub fn new(code: &str, message: impl Into<String>, hint: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            message: message.into(),
            hint: hint.into(),
        }
    }

    /// The "no file is open" error.
    pub fn no_document() -> Self {
        Self::new(
            app_codes::NO_DOCUMENT,
            "no file is open",
            "Open a save file first.",
        )
    }

    /// The internal-state error.
    pub fn internal() -> Self {
        Self::new(
            app_codes::INTERNAL,
            "the application state could not be used",
            "Restart SavePrism. If it happens again, report the code on the issue tracker.",
        )
    }
}

fn hint_for(code: ErrorCode) -> &'static str {
    if code == ES3_PASSWORD_REQUIRED {
        "Enter the password used by the game and open the file again."
    } else if code == ES3_WRONG_PASSWORD {
        "Check the password in the game's Easy Save settings and try again."
    } else if code == codes::CORE_READ_ONLY {
        "This value is read-only and cannot be changed."
    } else if code == codes::CORE_INVALID_PATH {
        "Pick the value again from the tree; the path may have changed."
    } else if code == codes::IO_READ {
        "Check that the file exists and can be read, then try again."
    } else if code == codes::IO_BACKUP || code == codes::IO_NO_BACKUP_ROOT {
        "The backup could not be made, so nothing was written. Check free disk space and permissions of the backup folder."
    } else if code == codes::IO_REPLACE || code == codes::IO_TEMP_WRITE {
        "Close the game and other programs that use the file, check that it is not read-only, then try again."
    } else if code == codes::IO_VERIFY_RESTORED {
        "The written file did not check out, so the backup was restored. Your save is unchanged."
    } else if code == codes::IO_RESTORE_FAILED {
        "Restore the file from the backup folder by hand and report the code on the issue tracker."
    } else if code == codes::DET_NO_MATCH {
        "SavePrism does not know this format yet. Attach a sample file to an issue so support can be added."
    } else {
        "Check the file and try again. If it keeps failing, report the code on the issue tracker."
    }
}

impl From<Error> for ApiError {
    fn from(error: Error) -> Self {
        Self::new(
            error.code().as_str(),
            error.message().to_string(),
            hint_for(error.code()),
        )
    }
}

/// How faithfully the file is written back.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum GuardLevel {
    /// Byte-identical.
    Exact,
    /// Same content, different layout.
    Semantic,
    /// The file cannot be written back safely.
    Failed,
}

/// The type of a node, as the tree shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeKind {
    /// Container with named children (also a map).
    Object,
    /// Container with numbered children.
    Array,
    /// Text.
    String,
    /// Integer.
    Int,
    /// Floating point number.
    Float,
    /// Boolean.
    Bool,
    /// No value.
    Null,
    /// Raw bytes.
    Bytes,
    /// Anything else (references).
    Other,
}

/// One row of the tree.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeRow {
    /// JSON Pointer of the node.
    pub path: String,
    /// Key (objects) or index (arrays, maps) inside the parent; empty for the root.
    pub key: String,
    /// Type of the node.
    pub kind: NodeKind,
    /// Short value text (at most 120 characters).
    pub preview: String,
    /// Number of children.
    pub child_count: usize,
    /// True when the value can be changed.
    pub editable: bool,
    /// True when a value can be added to this object (`add_value` with this path as the parent).
    pub addable: bool,
    /// True when this entry can be removed (`remove_value`).
    pub removable: bool,
}

/// One page of rows.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Page<T> {
    /// The rows of this page.
    pub items: Vec<T>,
    /// Number of rows in all pages.
    pub total: usize,
    /// Index of the first row of this page.
    pub offset: usize,
    /// True when a search stopped at the hit limit.
    pub truncated: bool,
}

/// The root node of the tree.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RootInfo {
    /// Always the empty path.
    pub path: String,
    /// Number of children of the root.
    pub child_count: usize,
}

/// What `open_file` returns.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenInfo {
    /// Name of the file.
    pub file_name: String,
    /// Full path of the file.
    pub path: String,
    /// Id of the format codec (`format.json`).
    pub codec: String,
    /// All step ids of the recipe, layers first.
    pub recipe: Vec<String>,
    /// Size of the file in bytes.
    pub byte_size: u64,
    /// Text encoding the codec detected for the file (`utf-8`, `shift_jis`, ...), when it records
    /// one. The UI already receives this struct; it is shown only where a field exists.
    pub text_encoding: Option<String>,
    /// Result of the round-trip guard.
    pub guard: GuardLevel,
    /// False when the guard failed.
    pub editable: bool,
    /// Text for the user when the guard is not exact.
    pub warning: Option<String>,
    /// The root node.
    pub root: RootInfo,
    /// The game engine known from the document's shape (`sugarcube`, `renpy`, `naninovel`), when the file name
    /// and the codec do not tell it. A pasted code has no telling file name, so its RPG Maker layer names
    /// the engine too (`rpgmaker-mv`, `rpgmaker-mz`).
    pub engine: Option<String>,
    /// True when the document was opened from a pasted save code (the file is SavePrism's own copy).
    pub pasted: bool,
    /// True when the file is text a player can copy and paste (UTF-8 without control characters),
    /// so `copy_code` can give it back as a code.
    pub text_code: bool,
    /// A notice the UI shows in its own words: `renpy-signed` (the game will ask whether to trust
    /// the edited save: answer Yes), `renpy-persistent-signed` (open read-only) or
    /// `terraria-layout` (a Terraria file whose layout is not recognized; open read-only).
    pub notice: Option<String>,
}

/// What a pending edit did.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ChangeKind {
    /// A value changed.
    Set,
    /// An entry was added (`old` is empty).
    Added,
    /// An entry was removed (`new` is empty; `old` is its value text or size).
    Removed,
}

/// One pending edit.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    /// JSON Pointer of the edited node.
    pub path: String,
    /// Value text before the first edit.
    pub old: String,
    /// Value text now.
    pub new: String,
    /// What the edit did.
    pub kind: ChangeKind,
}

impl Change {
    /// A changed value.
    pub(super) fn set(path: String, old: String, new: String) -> Self {
        Self {
            path,
            old,
            new,
            kind: ChangeKind::Set,
        }
    }

    /// False for a value edited back to its first text (it is no change any more); added and
    /// removed entries always count.
    fn counts(&self) -> bool {
        self.kind != ChangeKind::Set || self.old != self.new
    }
}

/// What `save` returns.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveResult {
    /// File that was written.
    pub written_path: String,
    /// Backup of the file that was replaced, if there was one.
    pub backup_path: Option<String>,
    /// True: the written file was read again and every edited value matched.
    pub verified: bool,
}

/// The open document.
pub struct Session {
    reg: Registry,
    limits: Limits,
    backup_root: Option<PathBuf>,
    path: PathBuf,
    /// Recipe returned by the decode (no password).
    recipe: Recipe,
    password: Option<String>,
    /// The bytes the document was read from (or last saved as); `discard_changes` decodes them again.
    original: Vec<u8>,
    doc: SaveDoc,
    editable: bool,
    guard: GuardLevel,
    pending: Vec<Change>,
    /// Undo and redo steps; empty after opening and after saving.
    history: History,
    /// Opened from a pasted save code; `path` is SavePrism's own copy until a "save as".
    pasted: bool,
    /// The game data folder the player picked (the folder the names are read from), if any.
    data_folder: Option<PathBuf>,
    /// Where the player's Ren'Py `security_keys.txt` may be found. Only a signed Ren'Py document
    /// resolves it, and it is kept so a restored backup uses the same source (tests use their own).
    renpy_keys: RenpyKeySource,
    /// The player's Ren'Py key file, when the document is a signed Ren'Py file and one was found.
    /// The key itself is read only while signing; the path is all that is kept.
    renpy_key: Option<PathBuf>,
    /// Gwent card definitions read from the player's own game files, kept for this session.
    witcher3_cards: Option<w3cards::W3CardsCache>,
}

fn io_error(path: &FsPath, source: std::io::Error) -> Error {
    Error::new(
        codes::IO_READ,
        format!("cannot read \"{}\": {source}", path.display()),
    )
    .with_source(source)
}

fn quote(text: &str) -> String {
    truncate(text, QUOTE_CHARS)
}

fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut short: String = text.chars().take(max_chars.saturating_sub(1)).collect();
    short.push('\u{2026}');
    short
}

/// Adds the password to the encryption step of a recipe (the layer's own parameters never hold it).
fn with_password(recipe: &Recipe, password: Option<&str>) -> Recipe {
    let mut recipe = recipe.clone();
    if let Some(password) = password {
        for step in &mut recipe.steps {
            if step.kind == ES3_AES_LAYER || step.kind == TERRARIA_PLR_LAYER {
                step.params.set("password", password);
            }
        }
    }
    recipe
}

/// The encrypted layer `bytes` looks like, if any. Both are detected by shape alone; a `.plr` name
/// makes the Terraria layer win, because an Easy Save 3 file never has that extension.
fn encrypted_layer(reg: &Registry, path: &FsPath, bytes: &[u8]) -> Option<&'static str> {
    let ctx = SniffCtx {
        file_name: path.file_name().and_then(|name| name.to_str()),
        depth: 0,
    };
    if reg
        .layer(TERRARIA_PLR_LAYER)
        .is_some_and(|layer| layer.sniff(bytes, &ctx) > Confidence::NONE)
    {
        return Some(TERRARIA_PLR_LAYER);
    }
    reg.layer(ES3_AES_LAYER)
        .is_some_and(|layer| layer.sniff(bytes, &ctx) > Confidence::NONE)
        .then_some(ES3_AES_LAYER)
}

/// Tries the keys the game's own files offer. Each candidate is checked cheaply (the first block
/// must decrypt to a plausible start), then with a full decode; the first full success wins. The
/// caller stores the winning candidate only after the round-trip guard accepts the document.
fn find_key(
    reg: &Registry,
    limits: &Limits,
    path: &FsPath,
    bytes: &[u8],
    finder: &KeyFinder,
    encrypted: &'static str,
) -> Option<(keyfinder::Candidate, DecodeOutput)> {
    // A key found before is tried first, so no game file is read when it still works.
    for candidate in finder.stored(path) {
        if let Some(out) = candidate_opens(reg, limits, path, bytes, &candidate, encrypted) {
            return Some((candidate, out));
        }
    }
    // `search` reads game files, so it is only called when the stored keys did not help.
    for candidate in finder.search(path) {
        if let Some(out) = candidate_opens(reg, limits, path, bytes, &candidate, encrypted) {
            return Some((candidate, out));
        }
    }
    None
}

/// The decoded document when `candidate` is the password: a cheap first-block check, then a full
/// decode.
fn candidate_opens(
    reg: &Registry,
    limits: &Limits,
    path: &FsPath,
    bytes: &[u8],
    candidate: &keyfinder::Candidate,
    encrypted: &'static str,
) -> Option<DecodeOutput> {
    let terraria = encrypted == TERRARIA_PLR_LAYER;
    if candidate.is_terraria() != terraria {
        return None;
    }
    let first_block_ok = if terraria {
        player_password_unlocks_first_block(bytes, candidate.key())
    } else {
        password_unlocks_first_block(bytes, candidate.key())
    };
    if !first_block_ok {
        return None;
    }
    decode_file(reg, limits, path, bytes, Some(candidate.key())).ok()
}

/// Detects and decodes `bytes`. Encrypted Easy Save 3 files cannot be detected without the
/// password, so they get a second try with the known layer chains.
fn decode_file(
    reg: &Registry,
    limits: &Limits,
    path: &FsPath,
    bytes: &[u8],
    password: Option<&str>,
) -> Result<DecodeOutput, Error> {
    let opts = DetectOptions {
        file_name: path
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_owned),
        ..DetectOptions::default()
    };
    // An encrypted Terraria player is random bytes to the detector, which can match them by
    // chance (about one file in a few hundred looks like XOR-obfuscated Ruby Marshal). When the
    // name and shape say Terraria, only a sure detection beats the Terraria key path below.
    let shape = encrypted_layer(reg, path, bytes);
    let terraria_shaped = shape == Some(TERRARIA_PLR_LAYER);
    let detect_error = match detect(bytes, reg, limits, &opts) {
        Ok(detection) if !terraria_shaped || detection.best.confidence >= Confidence::HIGH => {
            let mut recipe = detection.best.recipe;
            // RPG Maker 2000/2003 saves do not carry their text encoding; the game's
            // `RPG_RT.ini` can name it when EasyRPG wrote one.
            if recipe
                .steps
                .last()
                .is_some_and(|step| step.kind == "format.lcf")
                && let Some(encoding) = lcf::easyrpg_encoding(path)
                && let Some(step) = recipe.steps.last_mut()
            {
                step.params.set("text_encoding", encoding);
            }
            return pipeline::decode(bytes, &recipe, reg, limits)
                .map_err(|e| e.context(format!("while opening {}", path.display())));
        }
        Ok(_) => Error::new(
            codes::DET_NO_MATCH,
            "only a weak guess matched this Terraria-shaped file",
        ),
        Err(error) => error,
    };

    let Some(encrypted) = shape else {
        return Err(detect_error);
    };
    let Some(password) = password else {
        return Err(if encrypted == TERRARIA_PLR_LAYER {
            Error::new(
                TERRARIA_PASSWORD_REQUIRED,
                "this Terraria player save is encrypted and needs the key from the game's own Terraria.exe",
            )
        } else {
            Error::new(
                ES3_PASSWORD_REQUIRED,
                "this file is Easy Save 3 encrypted and needs a password",
            )
        });
    };

    let mut last = detect_error;
    if encrypted == TERRARIA_PLR_LAYER {
        let mut layer = Params::new();
        layer.set("password", password);
        let steps = vec![
            Step::new(TERRARIA_PLR_LAYER, layer),
            Step::bare("format.terraria-player"),
        ];
        return pipeline::decode(bytes, &Recipe::new(steps), reg, limits);
    }
    let tails: [&[&str]; 2] = [&["format.es3"], &["compression.gzip", "format.es3"]];
    for tail in tails {
        let mut aes = Params::new();
        aes.set("password", password);
        let mut steps = vec![Step::new(ES3_AES_LAYER, aes)];
        steps.extend(tail.iter().map(|kind| Step::bare(*kind)));
        match pipeline::decode(bytes, &Recipe::new(steps), reg, limits) {
            Ok(out) => return Ok(out),
            Err(error) => {
                let fatal = error.code() == ES3_WRONG_PASSWORD || error.code() == ES3_BAD_DATA;
                if fatal {
                    return Err(error);
                }
                last = error;
            }
        }
    }
    Err(last)
}

/// The round-trip verdict. The core guard cannot re-encode an encrypted file (the password is not
/// part of the returned parameters), so encrypted files are checked here with the password added.
fn assess(
    reg: &Registry,
    limits: &Limits,
    bytes: &[u8],
    out: &DecodeOutput,
    password: Option<&str>,
) -> Verdict {
    let encrypted = out
        .recipe
        .steps
        .iter()
        .any(|s| s.kind == ES3_AES_LAYER || s.kind == TERRARIA_PLR_LAYER);
    if !encrypted {
        return check_round_trip(bytes, &out.recipe, reg, limits)
            .map(|report| report.overall)
            .unwrap_or(Verdict::Different);
    }
    let recipe = with_password(&out.recipe, password);
    match pipeline::encode(&out.doc, &recipe, reg) {
        Err(_) => Verdict::Different,
        Ok(rebuilt) if rebuilt == bytes => Verdict::Exact,
        Ok(rebuilt) => match pipeline::decode(&rebuilt, &recipe, reg, limits) {
            Ok(again) if again.doc.tree_eq(&out.doc) => Verdict::Semantic,
            _ => Verdict::Different,
        },
    }
}

fn kind_of(value: &Value) -> NodeKind {
    match value {
        Value::Null => NodeKind::Null,
        Value::Bool(_) => NodeKind::Bool,
        Value::Int { .. } => NodeKind::Int,
        Value::Float { .. } => NodeKind::Float,
        Value::Str(_) => NodeKind::String,
        Value::Bytes(_) => NodeKind::Bytes,
        Value::Array(_) => NodeKind::Array,
        Value::Object(_) | Value::Map(_) => NodeKind::Object,
        Value::Ref(_) => NodeKind::Other,
    }
}

fn child_count(value: &Value) -> usize {
    match value {
        Value::Array(items) => items.len(),
        Value::Object(entries) => entries.len(),
        Value::Map(entries) => entries.len(),
        _ => 0,
    }
}

/// The text of a scalar value, `None` for containers, bytes, null and references.
fn value_text(node: &saveprism_core::Node) -> Option<String> {
    match &node.value {
        Value::Bool(v) => Some(v.to_string()),
        Value::Int { v, .. } => Some(v.to_string()),
        // Some codecs keep a marker such as `sii:original` in the lexeme, not the number text.
        Value::Float { v, .. } => Some(
            node.lexeme
                .as_deref()
                .filter(|text| text.trim().parse::<f64>().is_ok())
                .map_or_else(|| format!("{v:?}"), str::to_string),
        ),
        Value::Str(text) => Some(text.clone()),
        _ => None,
    }
}

fn preview(node: &saveprism_core::Node) -> String {
    match &node.value {
        Value::Null => "null".to_string(),
        Value::Bytes(bytes) => format!("<{} bytes>", bytes.len()),
        Value::Array(items) => format!("[{}]", items.len()),
        Value::Object(entries) => format!("{{{}}}", entries.len()),
        Value::Map(entries) => format!("{{{}}}", entries.len()),
        Value::Ref(target) => format!("-> #{}", target.index()),
        _ => truncate(&value_text(node).unwrap_or_default(), PREVIEW_CHARS),
    }
}

/// The `index`-th child: its path segment and node.
fn child_at(doc: &SaveDoc, id: NodeId, index: usize) -> Option<(String, NodeId)> {
    match &doc.get(id)?.value {
        Value::Array(items) => items.get(index).map(|child| (index.to_string(), *child)),
        Value::Object(entries) => entries.get(index).map(|(key, child)| (key.clone(), *child)),
        Value::Map(entries) => entries
            .get(index)
            .map(|(_, child)| (index.to_string(), *child)),
        _ => None,
    }
}

fn make_row(doc: &SaveDoc, doc_editable: bool, id: NodeId, path: &Path) -> Option<NodeRow> {
    let node = doc.get(id)?;
    let kind = kind_of(&node.value);
    let scalar = matches!(
        kind,
        NodeKind::String | NodeKind::Int | NodeKind::Float | NodeKind::Bool
    );
    // A repeated key resolves to its first entry, so the later entries cannot be edited by path.
    let editable = doc_editable
        && scalar
        && !node.flags.contains(NodeFlags::READ_ONLY)
        && doc.resolve(path) == Some(id);
    Some(NodeRow {
        path: path.to_string(),
        key: path.last().unwrap_or_default().to_string(),
        kind,
        preview: preview(node),
        child_count: child_count(&node.value),
        editable,
        addable: false,
        removable: false,
    })
}

fn bad_path(path: &str) -> Error {
    Error::new(
        codes::CORE_INVALID_PATH,
        format!("path '{path}' does not exist"),
    )
}

fn read_only_document() -> ApiError {
    ApiError::new(
        app_codes::READ_ONLY_DOCUMENT,
        "this file cannot be written back safely yet, so it is open read-only",
        "Attach a sample file to an issue so support for this format can be added.",
    )
}

/// Parses the text typed for a value of the same type as `current`.
fn parse_input(current: &Value, text: &str) -> Result<OwnedValue, ApiError> {
    let trimmed = text.trim();
    match current {
        Value::Int { width, .. } => {
            let (low, high) = width
                .range()
                .unwrap_or((i128::from(i64::MIN), i128::from(i64::MAX)));
            let value = trimmed.parse::<i128>().map_err(|_| {
                ApiError::new(
                    app_codes::BAD_VALUE,
                    format!("\"{}\" is not a whole number", quote(text)),
                    "Type digits only, for example 42 or -7.",
                )
            })?;
            if value < low || value > high {
                return Err(ApiError::new(
                    app_codes::OUT_OF_RANGE,
                    format!("{value} is outside the range of this value ({low} to {high})"),
                    format!("Type a number from {low} to {high}."),
                ));
            }
            Ok(OwnedValue::Int(value))
        }
        Value::Float { .. } => match trimmed.parse::<f64>() {
            Ok(value) if value.is_finite() => Ok(OwnedValue::Float(value)),
            _ => Err(ApiError::new(
                app_codes::BAD_VALUE,
                format!("\"{}\" is not a finite number", quote(text)),
                "Type a number such as 1.5 or -0.25.",
            )),
        },
        Value::Bool(_) => match trimmed.to_ascii_lowercase().as_str() {
            "true" => Ok(OwnedValue::Bool(true)),
            "false" => Ok(OwnedValue::Bool(false)),
            _ => Err(ApiError::new(
                app_codes::BAD_VALUE,
                format!("\"{}\" is not true or false", quote(text)),
                "Type true or false.",
            )),
        },
        Value::Str(_) => Ok(OwnedValue::Str(text.to_string())),
        _ => Err(ApiError::new(
            app_codes::NOT_EDITABLE_NODE,
            "only numbers, booleans and text can be edited",
            "Open the container and pick one of its values.",
        )),
    }
}

fn same_file(a: &FsPath, b: &FsPath) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(left), Ok(right)) => left == right,
        _ => a == b,
    }
}

impl Session {
    /// Opens a file: detect, decode, round-trip guard. `password` is used for encrypted Easy Save 3
    /// files and Terraria player files only. `backup_root` overrides the default backup folder
    /// (tests).
    pub fn open(
        path: &str,
        password: Option<String>,
        backup_root: Option<PathBuf>,
    ) -> Result<Session, ApiError> {
        Self::open_path(
            PathBuf::from(path),
            password,
            backup_root,
            RenpyKeySource::Default,
            Some(KeyFinder::production()),
        )
    }

    /// Opens a file, also trying the game folder the player picked for an encrypted save whose key
    /// is not found automatically.
    pub fn open_with_game_folder(
        path: &str,
        password: Option<String>,
        backup_root: Option<PathBuf>,
        game_folder: PathBuf,
    ) -> Result<Session, ApiError> {
        let finder = KeyFinder::production().with_folder(game_folder.clone());
        finder.remember_folder(&game_folder);
        Self::open_path(
            PathBuf::from(path),
            password,
            backup_root,
            RenpyKeySource::Default,
            Some(finder),
        )
    }

    /// Opens a file with an explicit Ren'Py key source; tests pass a key file of their own (or none)
    /// so they never read the player's real `security_keys.txt`.
    #[cfg(test)]
    pub(in crate::session) fn open_with_keys(
        path: &str,
        password: Option<String>,
        backup_root: Option<PathBuf>,
        keys: RenpyKeySource,
    ) -> Result<Session, ApiError> {
        Self::open_path(PathBuf::from(path), password, backup_root, keys, None)
    }

    /// [`Session::open`] with an explicit key finder (tests use their own folders and evidence
    /// file, so they never read the real Steam or app data folder).
    #[cfg(test)]
    pub(in crate::session) fn open_with_finder(
        path: &str,
        password: Option<String>,
        backup_root: Option<PathBuf>,
        finder: Option<KeyFinder>,
    ) -> Result<Session, ApiError> {
        Self::open_path(
            PathBuf::from(path),
            password,
            backup_root,
            RenpyKeySource::At(None),
            finder,
        )
    }

    /// [`Session::open`] with the path already as a file system path.
    fn open_path(
        path: PathBuf,
        password: Option<String>,
        backup_root: Option<PathBuf>,
        keys: RenpyKeySource,
        finder: Option<KeyFinder>,
    ) -> Result<Session, ApiError> {
        let mut reg = Registry::default();
        saveprism_codecs::register_all(&mut reg);
        let limits = Limits::default();

        let meta = fs::metadata(&path).map_err(|e| io_error(&path, e))?;
        limits
            .check_input(meta.len())
            .map_err(|e| e.context(format!("while reading {}", path.display())))?;
        let bytes = fs::read(&path).map_err(|e| io_error(&path, e))?;

        let mut password = password.filter(|text| !text.is_empty());
        let mut found = None;
        let mut predecoded = None;
        if password.is_none()
            && let Some(finder) = finder.as_ref()
            && let Some(encrypted) = encrypted_layer(&reg, &path, &bytes)
            && let Some((candidate, out)) =
                find_key(&reg, &limits, &path, &bytes, finder, encrypted)
        {
            password = Some(candidate.key().to_string());
            found = Some(candidate);
            predecoded = Some(out);
        }
        let out = match predecoded {
            Some(out) => out,
            None => decode_file(&reg, &limits, &path, &bytes, password.as_deref())?,
        };
        let verdict = assess(&reg, &limits, &bytes, &out, password.as_deref());
        let guard = match verdict {
            Verdict::Exact => GuardLevel::Exact,
            Verdict::Semantic => GuardLevel::Semantic,
            Verdict::Different => GuardLevel::Failed,
        };
        // Evidence is only written for a key whose document the round-trip guard accepts.
        if matches!(guard, GuardLevel::Exact | GuardLevel::Semantic)
            && let (Some(finder), Some(candidate)) = (finder.as_ref(), found.as_ref())
        {
            finder.remember(&path, candidate);
        }
        let mut session = Session {
            reg,
            limits,
            backup_root,
            path,
            recipe: out.recipe,
            password,
            original: bytes,
            doc: out.doc,
            editable: guard != GuardLevel::Failed,
            guard,
            pending: Vec::new(),
            history: History::default(),
            pasted: false,
            data_folder: None,
            renpy_keys: keys,
            renpy_key: None,
            witcher3_cards: None,
        };
        // Only a signed Ren'Py document looks for the player's key file (only such a file is signed
        // again); every other format never touches it.
        if session.renpy_signed() {
            session.renpy_key = session.renpy_keys.resolve(&session.path);
        }
        // A signed Ren'Py persistent file is thrown away by the game after any change.
        if session.renpy_locked() {
            session.editable = false;
        }
        // A Terraria file whose values no real file has was probably read from the wrong place.
        if session.terraria_layout_notice().is_some() {
            session.editable = false;
        }
        Ok(session)
    }

    /// The notice key and English text for a Terraria file whose values look misplaced (max life,
    /// max mana or the world size no real file has): its layout is not the one this version of
    /// the game writes, so it stays read-only.
    fn terraria_layout_notice(&self) -> Option<(&'static str, &'static str)> {
        saveprism_codecs::terraria::layout_looks_wrong(&self.doc).then_some((
            NOTICE_TERRARIA_LAYOUT,
            "This Terraria file's layout is not recognized for this version of the game, so it is open read-only.",
        ))
    }

    /// The facts the UI shows after opening.
    pub fn info(&self) -> OpenInfo {
        let notice = self
            .renpy_notice()
            .or_else(|| self.terraria_layout_notice());
        let guard_warning = match self.guard {
            GuardLevel::Exact => None,
            GuardLevel::Semantic => Some(
                "The file will be written with a slightly different layout (for example spacing or compression). The content stays the same."
                    .to_string(),
            ),
            GuardLevel::Failed => Some(
                "This file cannot be written back safely yet, so it is open read-only. Attach a sample file to an issue so support can be added."
                    .to_string(),
            ),
        };
        let warning = notice.map(|(_, text)| text.to_string()).or(guard_warning);
        let recipe: Vec<String> = self.recipe.steps.iter().map(|s| s.kind.clone()).collect();
        OpenInfo {
            file_name: self
                .path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
            path: self.path.to_string_lossy().into_owned(),
            codec: recipe.last().cloned().unwrap_or_default(),
            recipe,
            byte_size: u64::try_from(self.original.len()).unwrap_or(u64::MAX),
            text_encoding: self
                .recipe
                .steps
                .last()
                .and_then(|step| {
                    step.params
                        .opt_str("text_encoding")
                        .ok()
                        .flatten()
                        .or_else(|| step.params.opt_str("encoding").ok().flatten())
                })
                .map(str::to_owned),
            guard: self.guard,
            editable: self.editable,
            warning,
            root: RootInfo {
                path: String::new(),
                child_count: self
                    .doc
                    .get(self.doc.root())
                    .map_or(0, |node| child_count(&node.value)),
            },
            engine: if self.is_sugarcube() {
                Some("sugarcube".to_string())
            } else if self.is_naninovel() {
                Some("naninovel".to_string())
            } else if self.renpy_file().is_some() {
                Some("renpy".to_string())
            } else if self
                .recipe
                .steps
                .iter()
                .any(|step| step.kind == "encoding.url-escape")
            {
                Some("tyranoscript".to_string())
            } else if self.pasted {
                self.rpg_engine().map(|engine| format!("rpgmaker-{engine}"))
            } else {
                None
            },
            pasted: self.pasted,
            text_code: code::is_text_code(&self.original),
            notice: notice.map(|(key, _)| key.to_string()),
        }
    }

    /// One page of the children of the node at `path`. `limit` is capped at 500.
    pub fn get_children(
        &self,
        path: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Page<NodeRow>, ApiError> {
        let limit = limit.min(MAX_PAGE);
        let parent = Path::parse(path)?;
        let id = self.doc.resolve(&parent).ok_or_else(|| bad_path(path))?;
        let total = self.doc.get(id).map_or(0, |node| child_count(&node.value));
        let end = offset.saturating_add(limit).min(total);
        let shape = self.shape_rules(&parent, id);
        let mut items = Vec::new();
        for index in offset..end {
            let Some((segment, child)) = child_at(&self.doc, id, index) else {
                break;
            };
            let mut child_path = parent.clone();
            child_path.push(segment);
            if let Some(mut row) = make_row(&self.doc, self.editable, child, &child_path) {
                row.removable = shape.removable(&self.doc, &row.key, child);
                row.addable = shape.addable(&self.doc, &child_path, child);
                items.push(row);
            }
        }
        Ok(Page {
            items,
            total,
            offset,
            truncated: false,
        })
    }

    /// Case-insensitive substring search over the path text and the scalar value text of every node.
    /// An empty query gives an empty page. At most 10 000 hits are collected (`truncated`).
    pub fn search(
        &self,
        query: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Page<NodeRow>, ApiError> {
        let limit = limit.min(MAX_PAGE);
        if query.is_empty() {
            return Ok(Page {
                items: Vec::new(),
                total: 0,
                offset,
                truncated: false,
            });
        }
        let needle = query.to_lowercase();
        let (hits, truncated) = self.collect_hits(&needle);
        let items = hits
            .iter()
            .skip(offset)
            .take(limit)
            .filter_map(|(id, path)| make_row(&self.doc, self.editable, *id, path))
            .collect();
        Ok(Page {
            items,
            total: hits.len(),
            offset,
            truncated,
        })
    }

    /// Depth-first walk in document order. The visit budget (one visit per node) stops cyclic graphs.
    fn collect_hits(&self, needle: &str) -> (Vec<(NodeId, Path)>, bool) {
        struct Frame {
            id: NodeId,
            path: Path,
            next: usize,
        }
        let doc = &self.doc;
        let matches = |id: NodeId, path: &Path| -> bool {
            if path.to_string().to_lowercase().contains(needle) {
                return true;
            }
            doc.get(id)
                .and_then(value_text)
                .is_some_and(|text| text.to_lowercase().contains(needle))
        };

        let mut hits: Vec<(NodeId, Path)> = Vec::new();
        let root = doc.root();
        let root_path = Path::root();
        if matches(root, &root_path) {
            hits.push((root, root_path.clone()));
        }
        let mut stack = vec![Frame {
            id: root,
            path: root_path,
            next: 0,
        }];
        let mut budget = doc.len();
        while let Some(frame) = stack.last_mut() {
            let (frame_id, index) = (frame.id, frame.next);
            frame.next = frame.next.saturating_add(1);
            let Some((segment, child)) = child_at(doc, frame_id, index) else {
                stack.pop();
                continue;
            };
            if budget == 0 {
                break;
            }
            budget -= 1;
            let mut child_path = frame.path.clone();
            child_path.push(segment);
            if matches(child, &child_path) {
                if hits.len() >= MAX_SEARCH_HITS {
                    return (hits, true);
                }
                hits.push((child, child_path.clone()));
            }
            if doc.get(child).is_some_and(|n| child_count(&n.value) > 0) {
                stack.push(Frame {
                    id: child,
                    path: child_path,
                    next: 0,
                });
            }
        }
        (hits, false)
    }

    /// Changes the scalar at `path` to `text`, parsed by the node's current type.
    pub fn edit_value(&mut self, path: &str, text: &str) -> Result<NodeRow, ApiError> {
        if !self.editable {
            return Err(self.read_only_error());
        }
        let parsed = Path::parse(path)?;
        let Some(id) = self.doc.resolve(&parsed) else {
            return self.edit_named_missing(&parsed, text);
        };
        let node = self.doc.get(id).ok_or_else(|| bad_path(path))?;
        if node.flags.contains(NodeFlags::READ_ONLY) {
            return Err(Error::new(
                codes::CORE_READ_ONLY,
                format!("node at '{parsed}' is read-only"),
            )
            .into());
        }
        let old = value_text(node).unwrap_or_default();
        let before = scalar_owned(&node.value);
        let value = parse_input(&node.value, text)?;
        self.check_bounds(&parsed, node, &value)?;
        // A level edit also moves the actor's EXP to the minimum of the new level (MV/MZ only,
        // and only when the game's Classes.json is known).
        let exp = match &value {
            OwnedValue::Int(level) => self.level_exp_edit(&parsed, *level),
            _ => None,
        };
        let mut set_edits = vec![Edit::Set {
            path: parsed.clone(),
            value,
        }];
        let mut targets = vec![(parsed.clone(), old, before)];
        if let Some((exp_path, edit)) = exp {
            let exp_old = self
                .doc
                .resolve(&exp_path)
                .and_then(|id| self.doc.get(id))
                .and_then(value_text)
                .unwrap_or_default();
            let exp_before = self
                .doc
                .resolve(&exp_path)
                .and_then(|id| self.doc.get(id))
                .and_then(|node| scalar_owned(&node.value));
            targets.push((exp_path, exp_old, exp_before));
            set_edits.push(edit);
        }
        apply_edits(&mut self.doc, &set_edits, &self.limits)?;
        let mut changes = Vec::with_capacity(targets.len());
        let mut step = Vec::new();
        for (path, old, before) in targets {
            let id = self
                .doc
                .resolve(&path)
                .ok_or_else(|| bad_path(&path.to_string()))?;
            let node = self
                .doc
                .get(id)
                .ok_or_else(|| bad_path(&path.to_string()))?;
            let new = value_text(node).unwrap_or_default();
            let after = scalar_owned(&node.value);
            changes.push(Change::set(path.to_string(), old, new));
            if let (Some(before), Some(after)) = (before, after)
                && before != after
            {
                step.push(Mutation::Set {
                    path: path.to_string(),
                    before,
                    after,
                });
            }
        }
        self.record_changes(changes);
        self.history.record(step);
        make_row(&self.doc, self.editable, id, &parsed).ok_or_else(|| bad_path(path).into())
    }

    /// Edits a named MV/MZ switch or variable the save does not hold yet: grows its `_data` array
    /// (filling the gap with `null`) and sets the value, as one undo step.
    fn edit_named_missing(&mut self, path: &Path, text: &str) -> Result<NodeRow, ApiError> {
        let Some(target) = self.named_target(path) else {
            return Err(bad_path(&path.to_string()).into());
        };
        let value = target.parse(text)?;
        let before_doc = self.doc.clone();
        let before_pending = self.pending.clone();
        let edits = Session::named_growth_edits(&[(target.clone(), value)]);
        apply_edits(&mut self.doc, &edits, &self.limits)?;
        let id = self
            .doc
            .resolve(path)
            .ok_or_else(|| bad_path(&path.to_string()))?;
        let new = self.doc.get(id).and_then(value_text).unwrap_or_default();
        self.pending = before_pending.clone();
        self.record_change(path.to_string(), target.default_text(), new);
        self.history
            .record(vec![Mutation::Document(Box::new(DocStep {
                doc: before_doc,
                pending: before_pending,
            }))]);
        make_row(&self.doc, self.editable, id, path)
            .ok_or_else(|| bad_path(&path.to_string()).into())
    }

    /// Changes every scalar in `paths` to `text`, each parsed by its own type and checked against its
    /// own limits. Repeated paths are skipped. Everything is checked first: on any problem nothing
    /// is changed. The whole edit is one undo step. Returns the values that changed, like an undo
    /// does.
    pub fn edit_values(&mut self, paths: &[String], text: &str) -> Result<HistoryResult, ApiError> {
        if !self.editable {
            return Err(self.read_only_error());
        }
        if paths.len() > MAX_VALUES {
            return Err(ApiError::new(
                app_codes::TOO_MANY_VALUES,
                format!(
                    "{} values were sent at once; the limit is {MAX_VALUES}",
                    paths.len()
                ),
                "Narrow the list with its search box and set the values in parts.",
            ));
        }
        let mut seen = HashSet::with_capacity(paths.len());
        let mut seen_missing = HashSet::with_capacity(paths.len());
        let mut edits = Vec::with_capacity(paths.len());
        for path in paths {
            let parsed = Path::parse(path)?;
            match self.doc.resolve(&parsed) {
                Some(id) => {
                    if seen.insert(id) {
                        edits.push((path.clone(), text.to_string()));
                    }
                }
                None => {
                    // A named MV/MZ switch or variable the save leaves out is not in the tree
                    // yet; edit_each grows its array.
                    if self.named_target(&parsed).is_none() {
                        return Err(bad_path(path).into());
                    }
                    if seen_missing.insert(path.clone()) {
                        edits.push((path.clone(), text.to_string()));
                    }
                }
            }
        }
        self.edit_each(&edits)
    }

    /// Changes each scalar in `edits` to its own text, each parsed by its own type and checked
    /// against its own limits. Everything is checked first: on any problem nothing is changed. A
    /// path that appears twice is refused. The whole edit is one undo step. Returns the values that
    /// changed, like an undo does.
    pub fn edit_each(&mut self, edits: &[(String, String)]) -> Result<HistoryResult, ApiError> {
        if !self.editable {
            return Err(self.read_only_error());
        }
        if edits.len() > MAX_VALUES {
            return Err(ApiError::new(
                app_codes::TOO_MANY_VALUES,
                format!(
                    "{} values were sent at once; the limit is {MAX_VALUES}",
                    edits.len()
                ),
                "Narrow the list with its search box and set the values in parts.",
            ));
        }
        let bounds = self.rpg_bounds();
        let mut seen = HashSet::with_capacity(edits.len());
        let mut touched: Vec<(String, String, Option<OwnedValue>)> =
            Vec::with_capacity(edits.len());
        let mut set_edits = Vec::with_capacity(edits.len());
        let mut missing: Vec<(game::NamedTarget, OwnedValue)> = Vec::new();
        for (path, text) in edits {
            if !seen.insert(path.as_str()) {
                return Err(ApiError::new(
                    app_codes::BAD_VALUE,
                    format!("the path \"{}\" is in the list twice", quote(path)),
                    "Send each path only once.",
                ));
            }
            let parsed = Path::parse(path)?;
            let Some(id) = self.doc.resolve(&parsed) else {
                // A named MV/MZ switch or variable the save leaves out: grow its array.
                let target = self.named_target(&parsed).ok_or_else(|| bad_path(path))?;
                let value = target.parse(text)?;
                touched.push((path.clone(), target.default_text(), None));
                missing.push((target, value));
                continue;
            };
            let node = self.doc.get(id).ok_or_else(|| bad_path(path))?;
            if node.flags.contains(NodeFlags::READ_ONLY) {
                return Err(Error::new(
                    codes::CORE_READ_ONLY,
                    format!("node at '{parsed}' is read-only"),
                )
                .into());
            }
            let value = parse_input(&node.value, text)?;
            check_bounds_in(&bounds, &parsed, node, &value)?;
            let old = value_text(node).unwrap_or_default();
            let before = scalar_owned(&node.value);
            touched.push((path.clone(), old, before));
            set_edits.push(Edit::Set {
                path: parsed,
                value,
            });
        }
        let structural = !missing.is_empty();
        let (before_doc, before_pending) = if structural {
            (Some(self.doc.clone()), Some(self.pending.clone()))
        } else {
            (None, None)
        };
        let mut all_edits = Session::named_growth_edits(&missing);
        all_edits.extend(set_edits);
        // One atomic edit: on an error the document is unchanged.
        apply_edits(&mut self.doc, &all_edits, &self.limits)?;
        let mut changes = Vec::with_capacity(touched.len());
        let mut step = Vec::with_capacity(touched.len());
        for (path, old, before) in touched {
            let node = self
                .doc
                .resolve(&Path::parse(&path)?)
                .and_then(|id| self.doc.get(id));
            let new = node.and_then(value_text).unwrap_or_default();
            let after = node.and_then(|node| scalar_owned(&node.value));
            if let (Some(before), Some(after)) = (before, after)
                && before != after
            {
                step.push(Mutation::Set {
                    path: path.clone(),
                    before,
                    after,
                });
            }
            changes.push(Change::set(path, old, new));
        }
        let changed: Vec<String> = if structural {
            changes.iter().map(|change| change.path.clone()).collect()
        } else {
            step.iter()
                .filter_map(|mutation| match mutation {
                    Mutation::Set { path, .. } => Some(path.clone()),
                    _ => None,
                })
                .collect()
        };
        self.record_changes(changes);
        if let (Some(doc), Some(pending)) = (before_doc, before_pending) {
            self.history
                .record(vec![Mutation::Document(Box::new(DocStep { doc, pending }))]);
            let mut result = self.history_result(changed);
            result.structural = true;
            return Ok(result);
        }
        self.history.record(step);
        Ok(self.history_result(changed))
    }

    /// Keeps the oldest `old` per path; an edit back to the first value cancels the entry (an
    /// added entry stays added).
    fn record_change(&mut self, path: String, old: String, new: String) {
        match self.pending.iter().position(|c| c.path == path) {
            Some(index) => {
                let unchanged = match self.pending.get_mut(index) {
                    Some(change) => {
                        change.new = new;
                        !change.counts()
                    }
                    None => false,
                };
                if unchanged {
                    self.pending.remove(index);
                }
            }
            None => {
                if old != new {
                    self.pending.push(Change::set(path, old, new));
                }
            }
        }
    }

    /// [`Session::record_change`] for many paths at once, with an index instead of a scan per path.
    fn record_changes(&mut self, changes: Vec<Change>) {
        let mut index: HashMap<String, usize> = self
            .pending
            .iter()
            .enumerate()
            .map(|(position, change)| (change.path.clone(), position))
            .collect();
        for change in changes {
            match index.get(&change.path) {
                Some(&position) => {
                    if let Some(entry) = self.pending.get_mut(position) {
                        entry.new = change.new;
                    }
                }
                None => {
                    if change.old != change.new {
                        index.insert(change.path.clone(), self.pending.len());
                        self.pending.push(change);
                    }
                }
            }
        }
        // An edit back to the first value cancels its entry.
        self.pending.retain(Change::counts);
    }

    /// The edits made since opening or the last save.
    pub fn pending_changes(&self) -> Vec<Change> {
        self.pending.clone()
    }

    /// Throws the edits away by reading the document from its bytes again. This is one history step,
    /// so "Undo all" can itself be undone. When entries were added or removed (now or in a step that
    /// can still be undone), the step keeps the whole document, so undo puts back the very nodes the
    /// other steps refer to.
    pub fn discard_changes(&mut self) -> Result<(), ApiError> {
        if self.pending.is_empty() {
            return Ok(());
        }
        let recipe = with_password(&self.recipe, self.password.as_deref());
        let out = pipeline::decode(&self.original, &recipe, &self.reg, &self.limits)?;
        let structural = self.history.has_structural()
            || self
                .pending
                .iter()
                .any(|change| change.kind != ChangeKind::Set);
        if structural {
            let doc = std::mem::replace(&mut self.doc, out.doc);
            let pending = std::mem::take(&mut self.pending);
            self.history
                .record(vec![Mutation::Document(Box::new(DocStep { doc, pending }))]);
            return Ok(());
        }
        let scalar_at = |doc: &SaveDoc, path: &Path| {
            doc.resolve(path)
                .and_then(|id| doc.get(id))
                .and_then(|node| scalar_owned(&node.value))
        };
        let step: Vec<Mutation> = self
            .pending
            .iter()
            .filter_map(|change| {
                let path = Path::parse(&change.path).ok()?;
                let before = scalar_at(&self.doc, &path)?;
                let after = scalar_at(&out.doc, &path)?;
                (before != after).then(|| Mutation::Set {
                    path: change.path.clone(),
                    before,
                    after,
                })
            })
            .collect();
        self.doc = out.doc;
        self.pending.clear();
        self.history.record(step);
        Ok(())
    }

    /// Encodes the document and writes it to `target` with the core safe write, then reads the
    /// written file again and checks every edited value. `target` equal to the open file replaces it
    /// (a backup is always made); another path is "save as" and refuses to replace an existing file
    /// unless `overwrite` is true. `keep_backups` is the number of backups kept for the file,
    /// clamped to 1..=50 (10 when missing).
    pub fn save(
        &mut self,
        target: &str,
        overwrite: bool,
        keep_backups: Option<usize>,
    ) -> Result<SaveResult, ApiError> {
        if !self.editable {
            return Err(self.read_only_error());
        }
        let target_path = PathBuf::from(target);
        let same = same_file(&target_path, &self.path);
        if !same && !overwrite && fs::metadata(&target_path).is_ok() {
            return Err(ApiError::new(
                app_codes::TARGET_EXISTS,
                format!("\"{}\" already exists", target_path.display()),
                "Pick another file name, or confirm that the existing file may be replaced (a backup is made first).",
            ));
        }
        if same && let Ok(meta) = fs::metadata(&target_path) {
            self.limits.check_input(meta.len()).map_err(|error| {
                error.context(format!("while reading {}", target_path.display()))
            })?;
            let current = fs::read(&target_path).map_err(|error| io_error(&target_path, error))?;
            if current != self.original {
                let name = target_path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| target_path.display().to_string());
                return Err(ApiError::new(
                    app_codes::FILE_CHANGED,
                    format!("\"{name}\" changed on disk after it was opened"),
                    "Use Save as to keep your edits in another file, or open the file again.",
                ));
            }
        }

        let recipe = self.renpy_recipe(&with_password(&self.recipe, self.password.as_deref()))?;
        let bytes = pipeline::encode(&self.doc, &recipe, &self.reg)?;
        let mut opts = WriteOptions::new(self.backup_root()?);
        opts.keep_last = clamp_keep_backups(keep_backups);

        let verify = |written: &[u8]| self.verify_edits(&recipe, written);
        let report = safe_write(&target_path, &bytes, &opts, &verify)?;

        if !same {
            // A pasted code saved as a file is that file from now on.
            self.pasted = false;
        }
        self.path = target_path.clone();
        self.original = bytes;
        self.pending.clear();
        self.history = History::default();
        Ok(SaveResult {
            written_path: target_path.to_string_lossy().into_owned(),
            backup_path: report
                .backup_path
                .map(|path| path.to_string_lossy().into_owned()),
            verified: true,
        })
    }

    /// Decodes `written` with `recipe` and checks that it holds every edited and added value of the
    /// document and none of the removed entries.
    fn verify_edits(&self, recipe: &Recipe, written: &[u8]) -> saveprism_core::Result<()> {
        let again = pipeline::decode(written, recipe, &self.reg, &self.limits)?;
        let doc = &self.doc;
        for change in &self.pending {
            let Ok(path) = Path::parse(&change.path) else {
                continue;
            };
            let found = again.doc.resolve(&path).and_then(|id| again.doc.get(id));
            if change.kind == ChangeKind::Removed {
                if found.is_some() {
                    return Err(Error::new(
                        codes::CORE_EDIT_FAILED,
                        format!("the written file still holds the removed entry at '{path}'"),
                    ));
                }
                continue;
            }
            let expected = doc.resolve(&path).and_then(|id| doc.get(id));
            match (expected, found) {
                (Some(want), Some(got)) if want.value == got.value => {}
                _ => {
                    return Err(Error::new(
                        codes::CORE_EDIT_FAILED,
                        format!("the written file does not hold the edited value at '{path}'"),
                    ));
                }
            }
        }
        Ok(())
    }

    /// The backup folder: the one given to [`Session::open`], else the app's default.
    fn backup_root(&self) -> Result<PathBuf, ApiError> {
        self.backup_root
            .clone()
            .or_else(default_backup_root)
            .ok_or_else(|| {
                Error::new(
                    codes::IO_NO_BACKUP_ROOT,
                    "no backup directory is known for this system",
                )
                .into()
            })
    }
}
