//! The tool list and the dispatch of one `tools/call` to the [`Session`] methods.
//!
//! Every tool is described with a JSON Schema so the client can check its arguments; the server
//! checks them again here. A tool that only reads files or changes memory is marked read-only
//! (`open_code` also keeps the pasted code as SavePrism's own file in its app folder, as the window
//! does); only `save_changes` and `restore_backup` write save files and are marked destructive.
//! Folders and backup counts come from the settings file the window shares.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::session::shared_settings::{self, SharedSettings};
use crate::session::{
    ApiError, CompareRow, Entry, FieldKind, GameView, GwentCard, InventoryKind, NamedId,
    SatisfactoryInventory, Session, ValueRow, app_codes, save_locations,
};

use super::{CallError, RpcError, ServerState, bad_arguments, bad_value};

/// Longest tool result sent as text; bigger results point at the paging tools.
const MAX_RESULT_CHARS: usize = 80_000;
/// Most edits one `propose_edits` call accepts.
const MAX_EDITS: usize = 10_000;
/// Most pending changes one `propose_edits` result lists.
const PENDING_PREVIEW: usize = 100;
/// Characters of a save code one `copy_code` result holds (escaped JSON stays under the cap).
const CODE_PART_CHARS: usize = 25_000;

/// The `tools/list` result.
pub(super) fn list() -> Value {
    json!({"tools": definitions()})
}

/// One tool definition with its JSON Schema and annotations.
fn tool(
    name: &str,
    title: &str,
    description: &str,
    properties: Value,
    required: Value,
    annotations: (bool, bool, bool),
) -> Value {
    json!({
        "name": name,
        "title": title,
        "description": description,
        "inputSchema": {
            "type": "object",
            "properties": properties,
            "required": required,
            "additionalProperties": false
        },
        "annotations": {
            "readOnlyHint": annotations.0,
            "destructiveHint": annotations.1,
            "idempotentHint": annotations.2,
            "openWorldHint": false
        }
    })
}

/// A string property.
fn text(description: &str) -> Value {
    json!({"type": "string", "description": description})
}

/// A whole-number property with its default and lower bound.
fn count(description: &str, default: usize) -> Value {
    json!({
        "type": "integer",
        "description": description,
        "minimum": 0,
        "default": default
    })
}

/// The `keepBackups` property. It has no default: a missing value means the number chosen in
/// SavePrism's Settings.
fn keep_property() -> Value {
    json!({
        "type": "integer",
        "description": "Number of backups to keep for the file (1 to 50); missing uses the number chosen in SavePrism's Settings.",
        "minimum": 1,
        "maximum": 50
    })
}

/// Every tool this server offers, in a stable order.
fn definitions() -> Vec<Value> {
    vec![
        tool(
            "open_save",
            "Open a save file",
            "Opens a save file and replaces the open document. The key of an encrypted Easy Save 3 file or Terraria player file (.plr) is looked for in the game's own files first; when it is not found, ask the user for the game's folder (gameFolder) or the password.",
            json!({
                "path": text("Full path of the save file."),
                "password": text("Password of an encrypted Easy Save 3 file, or the 8 character key of a Terraria player file, when the game uses one."),
                "gameFolder": text("The game's own folder, searched for the key of an encrypted save that was not found automatically.")
            }),
            json!(["path"]),
            (true, false, true),
        ),
        tool(
            "open_code",
            "Open a save code",
            "Opens a save code the user pasted (the text a browser or idle game exports) and replaces the open document. SavePrism keeps the code as its own file in its app folder (the newest 10). Read and edit it like a save; to hand the edited code back, call copy_code.",
            json!({"code": text("The save code text, as the game gave it.")}),
            json!(["code"]),
            (true, false, false),
        ),
        tool(
            "save_locations",
            "Where saves usually are",
            "Lists the usual save folders of each engine on Windows and whether each folder exists on this PC. <game>, <company> and <game folder> stand for the game's own names. Nothing is searched.",
            json!({}),
            json!([]),
            (true, false, true),
        ),
        tool(
            "list_values",
            "List values",
            "Lists the children of one node as a page. Use the empty path to start at the top of the file.",
            json!({
                "path": text("JSON Pointer of the node whose children to list; empty for the top."),
                "offset": count("Index of the first child to return.", 0),
                "limit": count("Most children to return (capped at 500).", 100)
            }),
            json!([]),
            (true, false, true),
        ),
        tool(
            "search_values",
            "Search values",
            "Searches paths and scalar values case-insensitively and returns one page of matches.",
            json!({
                "query": text("Text to look for."),
                "offset": count("Index of the first match to return.", 0),
                "limit": count("Most matches to return (capped at 500).", 100)
            }),
            json!(["query"]),
            (true, false, true),
        ),
        tool(
            "game_view",
            "Game view",
            "Returns the game view of the open document: RPG Maker sections, a SugarCube story, a Witcher 3 save (values plus a gwent section), or a flat value list. Rows can be filtered and paged. For an RPG Maker save without a section the result is only the counts; filter, offset and limit apply to sections.",
            json!({
                "section": text("RPG Maker section: items, weapons, armors, variables, switches, catalogItems, catalogWeapons or catalogArmors. Witcher 3: gwent (the owned Gwent cards with their copy counts, the decks and the selected faction; filter by card index)."),
                "filter": text("Keep rows whose name contains this text (case-insensitive) or whose id/key equals it. An empty or blank filter keeps every row."),
                "offset": count("Index of the first row to return.", 0),
                "limit": count("Most rows to return (capped at 500).", 100)
            }),
            json!([]),
            (true, false, true),
        ),
        tool(
            "field_info",
            "Value details",
            "Returns the type, limits and original value of one value.",
            json!({"path": text("JSON Pointer of the value.")}),
            json!(["path"]),
            (true, false, true),
        ),
        tool(
            "compare_saves",
            "Compare with another save",
            "Compares the open document (pending edits included) with another save file and returns one page of the differences. The other file is only read; an encrypted one is refused. At most 5,000 differences are listed; total counts all of them.",
            json!({
                "path": text("Full path of the other save file."),
                "offset": count("Index of the first difference to return.", 0),
                "limit": count("Most differences to return (capped at 500).", 100)
            }),
            json!(["path"]),
            (true, false, true),
        ),
        tool(
            "set_data_folder",
            "Set data folder",
            "Reads item, weapon, armor, variable and switch names from a game data folder and returns the game view with those names.",
            json!({"folder": text("The game folder or its data folder.")}),
            json!(["folder"]),
            (false, false, true),
        ),
        tool(
            "propose_edits",
            "Propose edits",
            "Changes values in memory only; nothing is written to disk. Show the pending changes to the user before saving. The result lists at most the first 100 pending changes; call pending_changes for all of them.",
            json!({
                "edits": {
                    "type": "array",
                    "description": "One entry per value to change.",
                    "minItems": 1,
                    "maxItems": MAX_EDITS,
                    "items": {
                        "type": "object",
                        "properties": {
                            "path": text("JSON Pointer of the value."),
                            "value": text("New value as text, parsed by the value's current type.")
                        },
                        "required": ["path", "value"],
                        "additionalProperties": false
                    }
                }
            }),
            json!(["edits"]),
            (false, false, true),
        ),
        tool(
            "add_value",
            "Add value",
            "Adds a new value to an object in memory. Nothing is written to disk.",
            json!({
                "parent": text("JSON Pointer of the object to add to."),
                "key": text("Name of the new value."),
                "kind": {
                    "type": "string",
                    "enum": ["int", "float", "bool", "string"],
                    "description": "Type of the new value."
                },
                "value": text("New value as text.")
            }),
            json!(["parent", "key", "kind", "value"]),
            (false, false, false),
        ),
        tool(
            "add_inventory",
            "Add inventory",
            "Adds items, weapons or armors to an RPG Maker party in memory. Nothing is written to disk.",
            json!({
                "list": {
                    "type": "string",
                    "enum": ["items", "weapons", "armors"],
                    "description": "Inventory list to add to."
                },
                "id": {
                    "type": "integer",
                    "description": "Database id of the item, weapon or armor.",
                    "minimum": 1
                },
                "count": {
                    "type": "string",
                    "description": "How many to add (1 to 99), as text.",
                    "default": "1"
                }
            }),
            json!(["list", "id"]),
            (false, false, false),
        ),
        tool(
            "remove_value",
            "Remove value",
            "Removes an entry where the format allows it, in memory only. Nothing is written to disk.",
            json!({"path": text("JSON Pointer of the entry to remove.")}),
            json!(["path"]),
            (false, false, false),
        ),
        tool(
            "pending_changes",
            "Pending changes",
            "Lists the edits made since the file was opened or last saved, as a page.",
            json!({
                "offset": count("Index of the first pending change to return.", 0),
                "limit": count("Most pending changes to return (capped at 500).", 100)
            }),
            json!([]),
            (true, false, true),
        ),
        tool(
            "undo",
            "Undo",
            "Takes back the last edit in memory.",
            json!({}),
            json!([]),
            (false, false, false),
        ),
        tool(
            "discard_changes",
            "Discard changes",
            "Throws the pending edits away. Nothing is written to disk.",
            json!({}),
            json!([]),
            (false, false, true),
        ),
        tool(
            "save_changes",
            "Save changes",
            "Writes the save file. A backup is made first and every edited value is checked in the written file; only call it after the user agreed. A target must be a new file name in the folder of the open save; an existing file is never replaced.",
            json!({
                "target": text("Absolute new file name in the folder of the open save; missing writes the open file."),
                "keepBackups": keep_property()
            }),
            json!([]),
            (false, true, false),
        ),
        tool(
            "copy_code",
            "Copy save code",
            "Returns the open document as a save code with the pending edits, every edited value checked first. Only text formats can be copied. Nothing is written. A long code comes in parts of 25,000 characters: while more is true, call again with the next offset and join the parts.",
            json!({"offset": count("Character index where this part starts.", 0)}),
            json!([]),
            (true, false, true),
        ),
        tool(
            "list_backups",
            "List backups",
            "Lists the backups of the open file, newest first.",
            json!({}),
            json!([]),
            (true, false, true),
        ),
        tool(
            "restore_backup",
            "Restore backup",
            "Puts a backup back in place of the open file and opens it again. Pending edits are lost; only call it after the user agreed.",
            json!({
                "name": text("Backup file name from list_backups."),
                "keepBackups": keep_property()
            }),
            json!(["name"]),
            (false, true, false),
        ),
    ]
}

/// Handles one `tools/call` request.
pub(super) fn call_message(state: &mut ServerState, params: &Value) -> Result<Value, CallError> {
    let Some(object) = params.as_object() else {
        return Err(bad_arguments("params must be an object"));
    };
    let Some(name) = object.get("name").and_then(Value::as_str) else {
        return Err(bad_arguments("params.name must be a string"));
    };
    let arguments = match object.get("arguments") {
        None | Some(Value::Null) => Map::new(),
        Some(Value::Object(arguments)) => arguments.clone(),
        Some(_) => return Err(bad_arguments("params.arguments must be an object")),
    };
    let value = call(state, name, &arguments)?;
    let text = serde_json::to_string(&value).map_err(|_| CallError::Tool(ApiError::internal()))?;
    let characters = text.chars().count();
    if characters > MAX_RESULT_CHARS {
        return Err(CallError::Tool(ApiError::new(
            app_codes::MCP_ARGUMENT,
            format!(
                "the result is too large ({characters} characters); ask for a smaller page with offset/limit, a section or a filter"
            ),
            "Use offset and limit, a section or a filter to ask for a smaller page.",
        )));
    }
    Ok(json!({"content": [{"type": "text", "text": text}]}))
}

/// Dispatches one tool by name.
fn call(
    state: &mut ServerState,
    name: &str,
    args: &Map<String, Value>,
) -> Result<Value, CallError> {
    match name {
        "open_save" => open_save(state, args),
        "open_code" => open_code(state, args),
        "save_locations" => to_value(&save_locations()),
        "list_values" => list_values(state, args),
        "search_values" => search_values(state, args),
        "game_view" => game_view(state, args),
        "field_info" => field_info(state, args),
        "compare_saves" => compare_saves(state, args),
        "set_data_folder" => set_data_folder(state, args),
        "propose_edits" => propose_edits(state, args),
        "add_value" => add_value(state, args),
        "add_inventory" => add_inventory(state, args),
        "remove_value" => remove_value(state, args),
        "pending_changes" => pending_changes(state, args),
        "undo" => undo(state),
        "discard_changes" => discard_changes(state),
        "save_changes" => save_changes(state, args),
        "copy_code" => copy_code(state, args),
        "list_backups" => list_backups(state),
        "restore_backup" => restore_backup(state, args),
        #[cfg(test)]
        "test_panic" => panic!("test panic"),
        other => Err(CallError::Protocol(RpcError::invalid(format!(
            "unknown tool: {other}"
        )))),
    }
}

/// The open document, or the session's "no file is open" error.
fn session(state: &ServerState) -> Result<&Session, CallError> {
    state
        .session
        .as_ref()
        .ok_or_else(|| CallError::Tool(ApiError::no_document()))
}

/// The open document for changing it, or the session's "no file is open" error.
fn session_mut(state: &mut ServerState) -> Result<&mut Session, CallError> {
    state
        .session
        .as_mut()
        .ok_or_else(|| CallError::Tool(ApiError::no_document()))
}

/// A required string argument.
fn required_text(args: &Map<String, Value>, key: &str) -> Result<String, CallError> {
    match args.get(key) {
        Some(Value::String(value)) => Ok(value.clone()),
        Some(_) => Err(bad_arguments(format!(
            "argument \"{key}\" must be a string"
        ))),
        None => Err(bad_arguments(format!(
            "missing required argument \"{key}\""
        ))),
    }
}

/// An optional string argument (`null` counts as missing).
fn optional_text(args: &Map<String, Value>, key: &str) -> Result<Option<String>, CallError> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => Err(bad_arguments(format!(
            "argument \"{key}\" must be a string"
        ))),
    }
}

/// An optional whole-number argument with a default.
fn optional_count(
    args: &Map<String, Value>,
    key: &str,
    default: usize,
) -> Result<usize, CallError> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(Value::Number(number)) => number
            .as_u64()
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| {
                bad_arguments(format!(
                    "argument \"{key}\" must be a whole number of 0 or more"
                ))
            }),
        Some(_) => Err(bad_arguments(format!(
            "argument \"{key}\" must be a whole number of 0 or more"
        ))),
    }
}

/// A required whole-number argument.
fn required_i64(args: &Map<String, Value>, key: &str) -> Result<i64, CallError> {
    match args.get(key) {
        Some(Value::Number(number)) => number
            .as_i64()
            .ok_or_else(|| bad_arguments(format!("argument \"{key}\" must be a whole number"))),
        Some(_) => Err(bad_arguments(format!(
            "argument \"{key}\" must be a whole number"
        ))),
        None => Err(bad_arguments(format!(
            "missing required argument \"{key}\""
        ))),
    }
}

/// Serializes a value for a tool result.
fn to_value<T: Serialize>(value: &T) -> Result<Value, CallError> {
    serde_json::to_value(value).map_err(|_| CallError::Tool(ApiError::internal()))
}

/// [`to_value`] for each row of a page.
fn rows_value<'a, T: Serialize + 'a>(
    rows: impl Iterator<Item = &'a T>,
) -> Result<Value, CallError> {
    let mut out = Vec::new();
    for row in rows {
        out.push(to_value(row)?);
    }
    Ok(Value::Array(out))
}

/// The Settings choices the window shares (the defaults when there is no file).
fn shared(state: &ServerState) -> SharedSettings {
    state
        .settings_file
        .as_deref()
        .map(shared_settings::read)
        .unwrap_or_default()
}

/// A `keepBackups` argument, else the number chosen in Settings, else the session's default.
fn keep_backups(
    args: &Map<String, Value>,
    settings: &SharedSettings,
) -> Result<Option<usize>, CallError> {
    match args.get("keepBackups") {
        None | Some(Value::Null) => Ok(settings.keep_backups()),
        Some(_) => Ok(Some(optional_count(args, "keepBackups", 10)?)),
    }
}

/// Keeps `session` as the open document and returns its details.
fn opened(state: &mut ServerState, session: Session) -> Result<Value, CallError> {
    let mut value = to_value(&session.info())?;
    if let Some(object) = value.as_object_mut() {
        object.insert("pendingCount".to_string(), json!(0));
    }
    state.session = Some(session);
    Ok(value)
}

fn open_save(state: &mut ServerState, args: &Map<String, Value>) -> Result<Value, CallError> {
    let path = required_text(args, "path")?;
    let password = optional_text(args, "password")?;
    let game_folder = optional_text(args, "gameFolder")?.filter(|folder| !folder.trim().is_empty());
    let backup_root = state.backup_root.clone();
    let mut session = match game_folder {
        Some(folder) => {
            Session::open_with_game_folder(&path, password, backup_root, PathBuf::from(folder))?
        }
        None => Session::open(&path, password, backup_root)?,
    };
    session.set_backup_roots(&shared(state).backup_roots());
    opened(state, session)
}

fn open_code(state: &mut ServerState, args: &Map<String, Value>) -> Result<Value, CallError> {
    let code = required_text(args, "code")?;
    let settings = shared(state);
    let mut roots = settings.backup_roots();
    if roots.is_empty() {
        roots.extend(state.backup_root.clone());
    }
    let session = Session::open_code_in(&code, settings.app_dir(), &roots)?;
    opened(state, session)
}

fn compare_saves(state: &ServerState, args: &Map<String, Value>) -> Result<Value, CallError> {
    let path = required_text(args, "path")?;
    let offset = optional_count(args, "offset", 0)?;
    let limit = optional_count(args, "limit", 100)?.min(500);
    let session = session(state)?;
    let result = session.compare_with(&path)?;
    let page: Vec<&CompareRow> = result.rows.iter().skip(offset).take(limit).collect();
    // The names the window's Compare shows, so the client can tell the user what differs.
    let paths: Vec<String> = page.iter().map(|row| row.path.clone()).collect();
    let labels = session.value_labels(&paths);
    let mut rows = Vec::with_capacity(page.len());
    for (row, label) in page.into_iter().zip(labels) {
        let mut value = to_value(row)?;
        if let Some(object) = value.as_object_mut() {
            object.insert("label".to_string(), json!(label));
        }
        rows.push(value);
    }
    Ok(json!({
        "otherFileName": result.other_file_name,
        "total": result.total,
        "listed": result.rows.len(),
        "truncated": result.truncated,
        "offset": offset,
        "rows": rows
    }))
}

fn list_values(state: &ServerState, args: &Map<String, Value>) -> Result<Value, CallError> {
    let path = optional_text(args, "path")?.unwrap_or_default();
    let offset = optional_count(args, "offset", 0)?;
    let limit = optional_count(args, "limit", 100)?;
    let page = session(state)?.get_children(&path, offset, limit)?;
    to_value(&page)
}

fn search_values(state: &ServerState, args: &Map<String, Value>) -> Result<Value, CallError> {
    let query = required_text(args, "query")?;
    let offset = optional_count(args, "offset", 0)?;
    let limit = optional_count(args, "limit", 100)?;
    let page = session(state)?.search(&query, offset, limit)?;
    to_value(&page)
}

fn game_view(state: &ServerState, args: &Map<String, Value>) -> Result<Value, CallError> {
    let section = optional_text(args, "section")?;
    let filter = optional_text(args, "filter")?
        .filter(|filter| !filter.trim().is_empty())
        .map(|filter| filter.to_lowercase());
    let offset = optional_count(args, "offset", 0)?;
    let limit = optional_count(args, "limit", 100)?.min(500);
    let view = session(state)?.game_view()?;
    summarize(&view, section.as_deref(), filter.as_deref(), offset, limit)
}

fn field_info(state: &ServerState, args: &Map<String, Value>) -> Result<Value, CallError> {
    let path = required_text(args, "path")?;
    to_value(&session(state)?.field_info(&path)?)
}

fn set_data_folder(state: &mut ServerState, args: &Map<String, Value>) -> Result<Value, CallError> {
    let folder = required_text(args, "folder")?;
    let view = session_mut(state)?.set_data_folder(&folder)?;
    summarize(&view, None, None, 0, 500)
}

fn propose_edits(state: &mut ServerState, args: &Map<String, Value>) -> Result<Value, CallError> {
    let Some(Value::Array(items)) = args.get("edits") else {
        return Err(bad_arguments(
            "argument \"edits\" must be an array of {path, value} objects",
        ));
    };
    if items.is_empty() || items.len() > MAX_EDITS {
        return Err(bad_arguments(format!(
            "argument \"edits\" must hold 1 to {MAX_EDITS} entries"
        )));
    }
    let mut edits = Vec::with_capacity(items.len());
    for item in items {
        let Some(object) = item.as_object() else {
            return Err(bad_arguments("each edit must be an object"));
        };
        let path = required_text(object, "path")?;
        let value = required_text(object, "value")?;
        edits.push((path, value));
    }
    let session = session_mut(state)?;
    let changed = session.edit_each(&edits)?;
    let pending = session.pending_changes();
    let pending_count = pending.len();
    let listed: Vec<_> = pending.into_iter().take(PENDING_PREVIEW).collect();
    Ok(json!({
        "changedCount": changed.fields.len(),
        "pendingCount": pending_count,
        "pendingChanges": to_value(&listed)?
    }))
}

fn add_value(state: &mut ServerState, args: &Map<String, Value>) -> Result<Value, CallError> {
    let parent = required_text(args, "parent")?;
    let key = required_text(args, "key")?;
    let kind_text = required_text(args, "kind")?;
    let kind = match kind_text.as_str() {
        "int" => FieldKind::Int,
        "float" => FieldKind::Float,
        "bool" => FieldKind::Bool,
        "string" => FieldKind::String,
        other => {
            return Err(bad_arguments(format!(
                "argument \"kind\" must be int, float, bool or string, not \"{other}\""
            )));
        }
    };
    let value = required_text(args, "value")?;
    to_value(&session_mut(state)?.add_value(&parent, &key, kind, &value)?)
}

fn add_inventory(state: &mut ServerState, args: &Map<String, Value>) -> Result<Value, CallError> {
    let list = required_text(args, "list")?;
    let id = required_i64(args, "id")?;
    let count = optional_text(args, "count")?.unwrap_or_else(|| "1".to_string());
    to_value(&session_mut(state)?.add_inventory(&list, id, &count)?)
}

fn remove_value(state: &mut ServerState, args: &Map<String, Value>) -> Result<Value, CallError> {
    let path = required_text(args, "path")?;
    to_value(&session_mut(state)?.remove_value(&path)?)
}

fn pending_changes(state: &ServerState, args: &Map<String, Value>) -> Result<Value, CallError> {
    let offset = optional_count(args, "offset", 0)?;
    let limit = optional_count(args, "limit", 100)?.min(500);
    let pending = session(state)?.pending_changes();
    let total = pending.len();
    let items = rows_value(pending.iter().skip(offset).take(limit))?;
    Ok(json!({
        "total": total,
        "offset": offset,
        "items": items
    }))
}

fn undo(state: &mut ServerState) -> Result<Value, CallError> {
    to_value(&session_mut(state)?.undo()?)
}

fn discard_changes(state: &mut ServerState) -> Result<Value, CallError> {
    session_mut(state)?.discard_changes()?;
    Ok(Value::Null)
}

fn save_changes(state: &mut ServerState, args: &Map<String, Value>) -> Result<Value, CallError> {
    let target = optional_text(args, "target")?;
    let settings = shared(state);
    let keep_backups = keep_backups(args, &settings)?;
    let session = session_mut(state)?;
    session.set_backup_roots(&settings.backup_roots());
    let open = session.info().path;
    let target = match target {
        Some(target) => {
            if !writes_next_to(Path::new(&target), Path::new(&open)) {
                return Err(bad_value(
                    "save_changes can only write next to the open file",
                    "Give an absolute file name in the folder of the open save, or leave target out to write the open file.",
                ));
            }
            target
        }
        None => open,
    };
    to_value(&session.save_with_days(&target, false, keep_backups, settings.keep_days())?)
}

fn copy_code(state: &ServerState, args: &Map<String, Value>) -> Result<Value, CallError> {
    let offset = optional_count(args, "offset", 0)?;
    let code = session(state)?.copy_code()?;
    let total = code.chars().count();
    let part: String = code.chars().skip(offset).take(CODE_PART_CHARS).collect();
    Ok(json!({
        "code": part,
        "offset": offset,
        "totalChars": total,
        "more": offset.saturating_add(CODE_PART_CHARS) < total
    }))
}

/// True when `target` is an absolute path whose folder is the open file's folder.
fn writes_next_to(target: &Path, open: &Path) -> bool {
    if !target.is_absolute() {
        return false;
    }
    let (Some(target_folder), Some(open_folder)) = (target.parent(), open.parent()) else {
        return false;
    };
    match (
        fs::canonicalize(target_folder),
        fs::canonicalize(open_folder),
    ) {
        (Ok(target_folder), Ok(open_folder)) => target_folder == open_folder,
        _ => false,
    }
}

fn list_backups(state: &mut ServerState) -> Result<Value, CallError> {
    let roots = shared(state).backup_roots();
    let session = session_mut(state)?;
    session.set_backup_roots(&roots);
    to_value(&session.backups()?)
}

fn restore_backup(state: &mut ServerState, args: &Map<String, Value>) -> Result<Value, CallError> {
    let name = required_text(args, "name")?;
    let settings = shared(state);
    let keep_backups = keep_backups(args, &settings)?;
    let session = session_mut(state)?;
    session.set_backup_roots(&settings.backup_roots());
    let restored = session.restore_backup_with_days(&name, keep_backups, settings.keep_days())?;
    let info = restored.info();
    state.session = Some(restored);
    to_value(&info)
}

/// True when `needle` (already lowercase) is part of `name` (case-insensitive).
fn name_matches(name: Option<&str>, needle: &str) -> bool {
    name.is_some_and(|name| name.to_lowercase().contains(needle))
}

/// The filter rule of an RPG Maker entry: its name contains the text, or its id equals it.
fn entry_matches(entry: &Entry, filter: &str) -> bool {
    name_matches(entry.name.as_deref(), filter) || filter.parse::<u32>().ok() == Some(entry.id)
}

/// The filter rule of a database entry: its name contains the text, or its id equals it.
fn named_matches(item: &NamedId, filter: &str) -> bool {
    name_matches(Some(&item.name), filter) || filter.parse::<u32>().ok() == Some(item.id)
}

/// The filter rule of a value row: its name contains the text, or its key equals it.
fn row_matches(row: &ValueRow, filter: &str) -> bool {
    name_matches(Some(&row.name), filter)
        || row
            .path
            .rsplit('/')
            .next()
            .is_some_and(|key| key.to_lowercase() == filter)
}

/// The shape of `game_view` and `set_data_folder`: the whole view, or one filtered and paged
/// section of it.
fn summarize(
    view: &GameView,
    section: Option<&str>,
    filter: Option<&str>,
    offset: usize,
    limit: usize,
) -> Result<Value, CallError> {
    match view {
        GameView::Values {
            rows, add_parent, ..
        } => {
            let filtered: Vec<&ValueRow> = rows
                .iter()
                .filter(|row| filter.is_none_or(|filter| row_matches(row, filter)))
                .collect();
            let total = filtered.len();
            let rows = rows_value(filtered.into_iter().skip(offset).take(limit))?;
            Ok(json!({
                "kind": "values",
                "addParent": add_parent,
                "total": total,
                "offset": offset,
                "rows": rows
            }))
        }
        GameView::Witcher3 { view } => {
            if section == Some("gwent") {
                let Some(gwent) = &view.gwent else {
                    return Err(bad_value(
                        "this save has no Gwent collection",
                        "Leave out the section to list the values.",
                    ));
                };
                let filtered: Vec<&GwentCard> = gwent
                    .cards
                    .iter()
                    .filter(|card| {
                        filter.is_none_or(|filter| filter.parse::<u32>().ok() == Some(card.index))
                    })
                    .collect();
                let total = filtered.len();
                let cards = rows_value(filtered.into_iter().skip(offset).take(limit))?;
                return Ok(json!({
                    "kind": "witcher3",
                    "section": "gwent",
                    "decks": to_value(&gwent.decks)?,
                    "selected": &gwent.selected,
                    "total": total,
                    "offset": offset,
                    "cards": cards
                }));
            }
            let filtered: Vec<&ValueRow> = view
                .rows
                .iter()
                .filter(|row| filter.is_none_or(|filter| row_matches(row, filter)))
                .collect();
            let total = filtered.len();
            let rows = rows_value(filtered.into_iter().skip(offset).take(limit))?;
            Ok(json!({
                "kind": "witcher3",
                "addParent": view.add_parent,
                "gwentCards": view.gwent.as_ref().map(|gwent| gwent.cards.len()),
                "total": total,
                "offset": offset,
                "rows": rows
            }))
        }
        GameView::Satisfactory { view } => {
            if section == Some("inventories") {
                let filtered: Vec<&SatisfactoryInventory> = view
                    .inventories
                    .iter()
                    .filter(|inventory| {
                        filter.is_none_or(|filter| {
                            let needle = filter.to_lowercase();
                            inventory.label.to_lowercase().contains(&needle)
                                || match inventory.kind {
                                    InventoryKind::Player => needle == "player",
                                    InventoryKind::Storage => needle == "storage",
                                    InventoryKind::Other => needle == "other",
                                }
                                || inventory
                                    .stacks
                                    .iter()
                                    .any(|stack| stack.name.to_lowercase().contains(&needle))
                        })
                    })
                    .collect();
                let total = filtered.len();
                let inventories = rows_value(filtered.into_iter().skip(offset).take(limit))?;
                return Ok(json!({
                    "kind": "satisfactory",
                    "section": "inventories",
                    "total": total,
                    "offset": offset,
                    "inventories": inventories
                }));
            }
            let filtered: Vec<&ValueRow> = view
                .rows
                .iter()
                .filter(|row| filter.is_none_or(|filter| row_matches(row, filter)))
                .collect();
            let total = filtered.len();
            let rows = rows_value(filtered.into_iter().skip(offset).take(limit))?;
            Ok(json!({
                "kind": "satisfactory",
                "addParent": view.add_parent,
                "inventories": view.inventories.len(),
                "stacks": view.inventories.iter().map(|inventory| inventory.stacks.len()).sum::<usize>(),
                "total": total,
                "offset": offset,
                "rows": rows
            }))
        }
        GameView::Sugarcube { view } => {
            let mut object = match to_value(&**view)? {
                Value::Object(object) => object,
                _ => return Err(CallError::Tool(ApiError::internal())),
            };
            let filtered: Vec<&ValueRow> = view
                .variables
                .iter()
                .filter(|row| filter.is_none_or(|filter| row_matches(row, filter)))
                .collect();
            let total = filtered.len();
            let variables = rows_value(filtered.into_iter().skip(offset).take(limit))?;
            object.insert("total".to_string(), json!(total));
            object.insert("offset".to_string(), json!(offset));
            object.insert("variables".to_string(), variables);
            Ok(Value::Object(object))
        }
        GameView::Rpgmaker { view } => {
            let Some(section) = section else {
                let counts = json!({
                    "items": view.items.len(),
                    "weapons": view.weapons.len(),
                    "armors": view.armors.len(),
                    "variables": view.variables.len(),
                    "switches": view.switches.len(),
                    "catalogItems": view.catalog.items.len(),
                    "catalogWeapons": view.catalog.weapons.len(),
                    "catalogArmors": view.catalog.armors.len()
                });
                return Ok(json!({
                    "kind": "rpgmaker",
                    "engine": &view.engine,
                    "gold": to_value(&view.gold)?,
                    "actors": to_value(&view.actors)?,
                    "namesFolder": to_value(&view.names_folder)?,
                    "canAdd": to_value(&view.can_add)?,
                    "counts": counts
                }));
            };
            match section {
                "items" => entry_section(&view.items, section, filter, offset, limit),
                "weapons" => entry_section(&view.weapons, section, filter, offset, limit),
                "armors" => entry_section(&view.armors, section, filter, offset, limit),
                "variables" => entry_section(&view.variables, section, filter, offset, limit),
                "switches" => entry_section(&view.switches, section, filter, offset, limit),
                "catalogItems" => {
                    named_section(&view.catalog.items, section, filter, offset, limit)
                }
                "catalogWeapons" => {
                    named_section(&view.catalog.weapons, section, filter, offset, limit)
                }
                "catalogArmors" => {
                    named_section(&view.catalog.armors, section, filter, offset, limit)
                }
                other => Err(bad_value(
                    format!("there is no RPG Maker section \"{other}\""),
                    "Use items, weapons, armors, variables, switches, catalogItems, catalogWeapons or catalogArmors.",
                )),
            }
        }
    }
}

/// One filtered and paged section of RPG Maker entries.
fn entry_section(
    entries: &[Entry],
    section: &str,
    filter: Option<&str>,
    offset: usize,
    limit: usize,
) -> Result<Value, CallError> {
    let filtered: Vec<&Entry> = entries
        .iter()
        .filter(|entry| filter.is_none_or(|filter| entry_matches(entry, filter)))
        .collect();
    let total = filtered.len();
    let rows = rows_value(filtered.into_iter().skip(offset).take(limit))?;
    Ok(json!({
        "kind": "rpgmaker",
        "section": section,
        "total": total,
        "offset": offset,
        "rows": rows
    }))
}

/// One filtered and paged catalog section (items, weapons or armors of the game's database).
fn named_section(
    items: &[NamedId],
    section: &str,
    filter: Option<&str>,
    offset: usize,
    limit: usize,
) -> Result<Value, CallError> {
    let filtered: Vec<&NamedId> = items
        .iter()
        .filter(|item| filter.is_none_or(|filter| named_matches(item, filter)))
        .collect();
    let total = filtered.len();
    let rows = rows_value(filtered.into_iter().skip(offset).take(limit))?;
    Ok(json!({
        "kind": "rpgmaker",
        "section": section,
        "total": total,
        "offset": offset,
        "rows": rows
    }))
}
