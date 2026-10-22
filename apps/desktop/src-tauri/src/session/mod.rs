//! The open document of the desktop app, with no Tauri types so it can be unit tested.
//!
//! v0.1 keeps one document at a time: [`Session::open`] detects and decodes a file and runs the
//! round-trip guard; [`Session::get_children`] and [`Session::search`] read the tree;
//! [`Session::edit_value`] changes one scalar; [`Session::save`] encodes, writes with the core safe
//! write (backup, temp file, atomic replace) and checks every edited value in the written file.
//!
//! Errors cross the IPC boundary as [`ApiError`] (`code`, `message`, `hint`). The password of an
//! encrypted file lives in memory only; it is never logged, put into a message or written to disk.

use std::fs;
use std::path::{Path as FsPath, PathBuf};

use saveprism_codecs::es3::{ES3_BAD_DATA, ES3_PASSWORD_REQUIRED, ES3_WRONG_PASSWORD};
use saveprism_core::detect::{DetectOptions, detect};
use saveprism_core::guard::{Verdict, check_round_trip};
use saveprism_core::io::{WriteOptions, default_backup_root, safe_write};
use saveprism_core::{
    Confidence, DecodeOutput, Edit, Error, ErrorCode, Limits, NodeFlags, NodeId, OwnedValue,
    Params, Path, Recipe, Registry, SaveDoc, SniffCtx, Step, Value, apply_edits, codes, pipeline,
};
use serde::Serialize;

mod export;
mod game;

pub use game::{Actor, Bonus, Entry, Field, FieldKind, GameView, RpgMakerView, ValueRow};

#[cfg(test)]
mod tests;

/// Most rows one call returns.
pub const MAX_PAGE: usize = 500;
/// Most hits one search collects.
pub const MAX_SEARCH_HITS: usize = 10_000;
/// Longest value preview, in characters.
const PREVIEW_CHARS: usize = 120;
/// Longest piece of user input quoted in a message, in characters.
const QUOTE_CHARS: usize = 40;
/// The encryption layer of Easy Save 3.
const ES3_AES_LAYER: &str = "crypto.es3-aes";

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
    /// Internal state could not be used (lock poisoned).
    pub const INTERNAL: &str = "SP-APP-009";
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
    /// Result of the round-trip guard.
    pub guard: GuardLevel,
    /// False when the guard failed.
    pub editable: bool,
    /// Text for the user when the guard is not exact.
    pub warning: Option<String>,
    /// The root node.
    pub root: RootInfo,
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
            if step.kind == ES3_AES_LAYER {
                step.params.set("password", password);
            }
        }
    }
    recipe
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
    let detect_error = match detect(bytes, reg, limits, &opts) {
        Ok(detection) => {
            return pipeline::decode(bytes, &detection.best.recipe, reg, limits)
                .map_err(|e| e.context(format!("while opening {}", path.display())));
        }
        Err(error) => error,
    };

    let sniff_ctx = SniffCtx {
        file_name: opts.file_name.as_deref(),
        depth: 0,
    };
    let looks_encrypted = reg
        .layer(ES3_AES_LAYER)
        .is_some_and(|layer| layer.sniff(bytes, &sniff_ctx) > Confidence::NONE);
    if !looks_encrypted {
        return Err(detect_error);
    }
    let Some(password) = password else {
        return Err(Error::new(
            ES3_PASSWORD_REQUIRED,
            "this file is Easy Save 3 encrypted and needs a password",
        ));
    };

    let mut last = detect_error;
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
    let encrypted = out.recipe.steps.iter().any(|s| s.kind == ES3_AES_LAYER);
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
        Value::Float { v, .. } => Some(
            node.lexeme
                .as_deref()
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
    /// files only. `backup_root` overrides the default backup folder (tests).
    pub fn open(
        path: &str,
        password: Option<String>,
        backup_root: Option<PathBuf>,
    ) -> Result<Session, ApiError> {
        let path = PathBuf::from(path);
        let mut reg = Registry::default();
        saveprism_codecs::register_all(&mut reg);
        let limits = Limits::default();

        let meta = fs::metadata(&path).map_err(|e| io_error(&path, e))?;
        limits
            .check_input(meta.len())
            .map_err(|e| e.context(format!("while reading {}", path.display())))?;
        let bytes = fs::read(&path).map_err(|e| io_error(&path, e))?;

        let password = password.filter(|text| !text.is_empty());
        let out = decode_file(&reg, &limits, &path, &bytes, password.as_deref())?;
        let verdict = assess(&reg, &limits, &bytes, &out, password.as_deref());
        let guard = match verdict {
            Verdict::Exact => GuardLevel::Exact,
            Verdict::Semantic => GuardLevel::Semantic,
            Verdict::Different => GuardLevel::Failed,
        };
        Ok(Session {
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
        })
    }

    /// The facts the UI shows after opening.
    pub fn info(&self) -> OpenInfo {
        let warning = match self.guard {
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
        let mut items = Vec::new();
        for index in offset..end {
            let Some((segment, child)) = child_at(&self.doc, id, index) else {
                break;
            };
            let mut child_path = parent.clone();
            child_path.push(segment);
            if let Some(row) = make_row(&self.doc, self.editable, child, &child_path) {
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
            return Err(read_only_document());
        }
        let parsed = Path::parse(path)?;
        let id = self.doc.resolve(&parsed).ok_or_else(|| bad_path(path))?;
        let node = self.doc.get(id).ok_or_else(|| bad_path(path))?;
        if node.flags.contains(NodeFlags::READ_ONLY) {
            return Err(Error::new(
                codes::CORE_READ_ONLY,
                format!("node at '{parsed}' is read-only"),
            )
            .into());
        }
        let old = value_text(node).unwrap_or_default();
        let value = parse_input(&node.value, text)?;
        self.check_bounds(&parsed, node, &value)?;
        apply_edits(
            &mut self.doc,
            &[Edit::Set {
                path: parsed.clone(),
                value,
            }],
            &self.limits,
        )?;
        let node = self.doc.get(id).ok_or_else(|| bad_path(path))?;
        let new = value_text(node).unwrap_or_default();
        self.record_change(parsed.to_string(), old, new);
        make_row(&self.doc, self.editable, id, &parsed).ok_or_else(|| bad_path(path).into())
    }

    /// Keeps the oldest `old` per path; an edit back to the first value cancels the entry.
    fn record_change(&mut self, path: String, old: String, new: String) {
        match self.pending.iter().position(|c| c.path == path) {
            Some(index) => {
                let unchanged = match self.pending.get_mut(index) {
                    Some(change) => {
                        change.new = new;
                        change.old == change.new
                    }
                    None => false,
                };
                if unchanged {
                    self.pending.remove(index);
                }
            }
            None => {
                if old != new {
                    self.pending.push(Change { path, old, new });
                }
            }
        }
    }

    /// The edits made since opening or the last save.
    pub fn pending_changes(&self) -> Vec<Change> {
        self.pending.clone()
    }

    /// Throws the edits away by reading the document from its bytes again.
    pub fn discard_changes(&mut self) -> Result<(), ApiError> {
        if self.pending.is_empty() {
            return Ok(());
        }
        let recipe = with_password(&self.recipe, self.password.as_deref());
        let out = pipeline::decode(&self.original, &recipe, &self.reg, &self.limits)?;
        self.doc = out.doc;
        self.pending.clear();
        Ok(())
    }

    /// Encodes the document and writes it to `target` with the core safe write, then reads the
    /// written file again and checks every edited value. `target` equal to the open file replaces it
    /// (a backup is always made); another path is "save as" and refuses to replace an existing file
    /// unless `overwrite` is true.
    pub fn save(&mut self, target: &str, overwrite: bool) -> Result<SaveResult, ApiError> {
        if !self.editable {
            return Err(read_only_document());
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

        let recipe = with_password(&self.recipe, self.password.as_deref());
        let bytes = pipeline::encode(&self.doc, &recipe, &self.reg)?;
        let backup_root = self
            .backup_root
            .clone()
            .or_else(default_backup_root)
            .ok_or_else(|| {
                Error::new(
                    codes::IO_NO_BACKUP_ROOT,
                    "no backup directory is known for this system",
                )
            })?;
        let opts = WriteOptions::new(backup_root);

        let edited: Vec<Path> = self
            .pending
            .iter()
            .filter_map(|change| Path::parse(&change.path).ok())
            .collect();
        let doc = &self.doc;
        let verify = |written: &[u8]| -> saveprism_core::Result<()> {
            let again = pipeline::decode(written, &recipe, &self.reg, &self.limits)?;
            for path in &edited {
                let expected = doc.resolve(path).and_then(|id| doc.get(id));
                let found = again.doc.resolve(path).and_then(|id| again.doc.get(id));
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
        };
        let report = safe_write(&target_path, &bytes, &opts, &verify)?;

        self.path = target_path.clone();
        self.original = bytes;
        self.pending.clear();
        Ok(SaveResult {
            written_path: target_path.to_string_lossy().into_owned(),
            backup_path: report
                .backup_path
                .map(|path| path.to_string_lossy().into_owned()),
            verified: true,
        })
    }
}
