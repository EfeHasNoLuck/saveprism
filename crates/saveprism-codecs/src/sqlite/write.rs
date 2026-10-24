//! Writes edits back into the original database and serializes it again.

use rusqlite::types::Value as SqlValue;
use rusqlite::{MAIN_DB, params};
use saveprism_core::{Error, Limits, Node, NodeFlags, ParamValue, Params, Result, SaveDoc, Value};

use super::read::{self, quote_ident};
use super::{HINT_PREFIX, ORIGINAL, ROWID_KEY, SQLITE_QUERY, SQLITE_WRITE};

struct Update {
    table: String,
    column: String,
    rowid: i64,
    value: SqlValue,
    /// The column is an `INTEGER PRIMARY KEY` rowid alias, so it must change last.
    rowid_alias: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TableKind {
    Editable,
    View,
    WithoutRowid,
    ReadOnly,
}

fn write_error(message: impl Into<String>) -> Error {
    Error::new(SQLITE_WRITE, message)
}

fn query_error(error: rusqlite::Error) -> Error {
    Error::new(SQLITE_QUERY, "a query against the SQLite database failed").with_source(error)
}

/// Writes the document. Unchanged documents return the original bytes exactly.
pub(super) fn encode(doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
    let original = match params.get(ORIGINAL) {
        Some(ParamValue::Bytes(bytes)) => bytes.as_slice(),
        _ => return Err(write_error("original database bytes are missing")),
    };
    let limits = Limits::default();
    limits.check_input(saveprism_core::limits::len_u64(original.len()))?;
    let (baseline, _) = read::decode(original, &limits)?;
    if doc.tree_eq(&baseline) {
        return Ok(original.to_vec());
    }

    let mut updates = Vec::new();
    compare_root(doc, &baseline, &mut updates)?;
    if updates.is_empty() {
        return Err(write_error(
            "the document changed but no writable cell was found",
        ));
    }

    updates.sort_by_key(|update| update.rowid_alias);

    let mut connection = read::open(original, false)?;
    let transaction = connection.transaction().map_err(query_error)?;
    for update in &updates {
        let pseudo = rowid_alias(&update.table, &update.column, doc, &baseline)?;
        let sql = format!(
            "UPDATE {} SET {} = ?1 WHERE {} = ?2",
            quote_ident(&update.table),
            quote_ident(&update.column),
            pseudo
        );
        let changed = transaction
            .execute(&sql, params![update.value, update.rowid])
            .map_err(query_error)?;
        if changed == 0 {
            return Err(write_error(format!(
                "row {} of table {} no longer exists",
                update.rowid, update.table
            )));
        }
    }
    transaction.commit().map_err(query_error)?;
    let data = connection.serialize(MAIN_DB).map_err(query_error)?;
    Ok(data.to_vec())
}

fn compare_root(doc: &SaveDoc, baseline: &SaveDoc, updates: &mut Vec<Update>) -> Result<()> {
    let doc_tables = object_entries(doc, doc.root())?;
    let baseline_tables = object_entries(baseline, baseline.root())?;
    if doc_tables.len() != baseline_tables.len() {
        return Err(write_error("tables cannot be added or removed"));
    }
    for ((doc_name, doc_id), (name, baseline_id)) in doc_tables.iter().zip(baseline_tables.iter()) {
        if doc_name != name {
            return Err(write_error("tables cannot be renamed or reordered"));
        }
        compare_table(doc, baseline, name, *doc_id, *baseline_id, updates)?;
    }
    Ok(())
}

fn compare_table(
    doc: &SaveDoc,
    baseline: &SaveDoc,
    name: &str,
    doc_id: saveprism_core::NodeId,
    baseline_id: saveprism_core::NodeId,
    updates: &mut Vec<Update>,
) -> Result<()> {
    let hint = hint_of(baseline, baseline_id)?;
    if !hint.starts_with(HINT_PREFIX) {
        return Err(write_error(format!("table {name} has no SQLite hint")));
    }
    let kind = if hint.starts_with("SQLITE:view") {
        TableKind::View
    } else if hint.contains("/without-rowid") {
        TableKind::WithoutRowid
    } else if hint.contains("/read-only") {
        TableKind::ReadOnly
    } else {
        TableKind::Editable
    };
    let doc_rows = array_items(doc, doc_id)?;
    let baseline_rows = array_items(baseline, baseline_id)?;
    if doc_rows.len() != baseline_rows.len() {
        return Err(write_error("rows cannot be added or removed"));
    }
    for (doc_row, baseline_row) in doc_rows.iter().zip(baseline_rows.iter()) {
        compare_row(doc, baseline, name, kind, *doc_row, *baseline_row, updates)?;
    }
    Ok(())
}

fn compare_row(
    doc: &SaveDoc,
    baseline: &SaveDoc,
    table: &str,
    kind: TableKind,
    doc_id: saveprism_core::NodeId,
    baseline_id: saveprism_core::NodeId,
    updates: &mut Vec<Update>,
) -> Result<()> {
    let doc_entries = object_entries(doc, doc_id)?;
    let baseline_entries = object_entries(baseline, baseline_id)?;
    if doc_entries.len() != baseline_entries.len() {
        return Err(write_error("columns cannot be added or removed"));
    }
    let rowid = baseline_entries
        .iter()
        .find(|(key, _)| key == ROWID_KEY)
        .and_then(|(_, id)| match baseline.get(*id).map(|node| &node.value) {
            Some(Value::Int { v, .. }) => i64::try_from(*v).ok(),
            _ => None,
        });

    for ((doc_key, doc_cell), (key, baseline_cell)) in
        doc_entries.iter().zip(baseline_entries.iter())
    {
        if doc_key != key {
            return Err(write_error("columns cannot be renamed or reordered"));
        }
        let base_node = node(baseline, *baseline_cell)?;
        let doc_node = node(doc, *doc_cell)?;
        if same_value(&base_node.value, &doc_node.value) {
            continue;
        }
        if key == ROWID_KEY {
            return Err(write_error("row ids are read-only"));
        }
        if base_node.flags.contains(NodeFlags::READ_ONLY) {
            return Err(write_error(format!(
                "column {key} of table {table} is read-only"
            )));
        }
        if kind != TableKind::Editable {
            return Err(write_error(format!("table {table} is read-only")));
        }
        let rowid =
            rowid.ok_or_else(|| write_error(format!("table {table} has no usable rowid")))?;
        let value = sql_value(&base_node.value, &doc_node.value)?;
        let rowid_alias = base_node
            .type_hint
            .as_ref()
            .is_some_and(|hint| hint.as_str().ends_with("/rowid-alias"));
        updates.push(Update {
            table: table.to_string(),
            column: key.clone(),
            rowid,
            value,
            rowid_alias,
        });
    }
    Ok(())
}

/// Recomputes the rowid alias the reader used for a table, from the baseline row keys.
fn rowid_alias(
    table: &str,
    _column: &str,
    _doc: &SaveDoc,
    baseline: &SaveDoc,
) -> Result<&'static str> {
    let root = object_entries(baseline, baseline.root())?;
    let baseline_id = root
        .iter()
        .find(|(name, _)| name == table)
        .map(|(_, id)| *id)
        .ok_or_else(|| write_error(format!("table {table} is missing")))?;
    let rows = array_items(baseline, baseline_id)?;
    let first = rows
        .first()
        .copied()
        .ok_or_else(|| write_error(format!("table {table} has no rows")))?;
    let entries = object_entries(baseline, first)?;
    let columns: Vec<String> = entries
        .iter()
        .filter(|(key, _)| key != ROWID_KEY)
        .map(|(key, _)| key.to_ascii_lowercase())
        .collect();
    ["_rowid_", "rowid", "oid"]
        .into_iter()
        .find(|candidate| !columns.iter().any(|column| column == candidate))
        .ok_or_else(|| write_error(format!("table {table} has no usable rowid")))
}

fn sql_value(baseline: &Value, edited: &Value) -> Result<SqlValue> {
    match (baseline, edited) {
        (Value::Int { .. }, Value::Int { v, .. }) => i64::try_from(*v)
            .map(SqlValue::Integer)
            .map_err(|_| write_error("the integer does not fit in 64 bits")),
        (Value::Float { .. }, Value::Float { v, .. }) => {
            if v.is_finite() {
                Ok(SqlValue::Real(*v))
            } else {
                Err(write_error("a REAL value must be finite"))
            }
        }
        (Value::Str(_), Value::Str(text)) => Ok(SqlValue::Text(text.clone())),
        _ => Err(write_error(
            "the value changed its type; an SQLite value keeps its type",
        )),
    }
}

fn same_value(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Null, Value::Null) => true,
        (Value::Bool(a), Value::Bool(b)) => a == b,
        (Value::Int { v: a, width: wa }, Value::Int { v: b, width: wb }) => a == b && wa == wb,
        (Value::Float { v: a, width: wa }, Value::Float { v: b, width: wb }) => {
            a.to_bits() == b.to_bits() && wa == wb
        }
        (Value::Str(a), Value::Str(b)) => a == b,
        (Value::Bytes(a), Value::Bytes(b)) => a == b,
        _ => false,
    }
}

fn node(doc: &SaveDoc, id: saveprism_core::NodeId) -> Result<&Node> {
    doc.get(id)
        .ok_or_else(|| write_error("the document has a missing node"))
}

fn hint_of(doc: &SaveDoc, id: saveprism_core::NodeId) -> Result<&str> {
    node(doc, id)?
        .type_hint
        .as_ref()
        .map(|hint| hint.as_str())
        .ok_or_else(|| write_error("the document has a node without a hint"))
}

fn object_entries(
    doc: &SaveDoc,
    id: saveprism_core::NodeId,
) -> Result<&[(String, saveprism_core::NodeId)]> {
    match &node(doc, id)?.value {
        Value::Object(entries) => Ok(entries),
        _ => Err(write_error("a container changed its type")),
    }
}

fn array_items(doc: &SaveDoc, id: saveprism_core::NodeId) -> Result<&[saveprism_core::NodeId]> {
    match &node(doc, id)?.value {
        Value::Array(items) => Ok(items),
        _ => Err(write_error("a container changed its type")),
    }
}
