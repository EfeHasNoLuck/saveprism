//! Reads an SQLite database into a document tree without ever touching the original file.

use std::collections::HashSet;

use rusqlite::types::ValueRef;
use rusqlite::{Connection, MAIN_DB};
use saveprism_core::{
    Error, FloatWidth, IntWidth, Limits, Node, NodeFlags, NodeId, Params, Result, SaveDoc,
    TypeHint, Value,
};

use super::{
    MAGIC, ORIGINAL, ROW_CAP, ROWID_KEY, SQLITE_QUERY, SQLITE_WAL, TRUNCATED, parse_error,
};

const PSEUDO_ROWIDS: [&str; 3] = ["_rowid_", "rowid", "oid"];

const HINT_TABLES: &str = "SQLITE:tables";
const HINT_TABLE: &str = "SQLITE:table";
const HINT_VIEW: &str = "SQLITE:view";
const HINT_WITHOUT_ROWID: &str = "SQLITE:table/without-rowid";
const HINT_READ_ONLY_TABLE: &str = "SQLITE:table/read-only";
const HINT_TRUNCATED: &str = "/truncated";
const HINT_ROW: &str = "SQLITE:row";
const HINT_ROWID: &str = "SQLITE:rowid";
const HINT_INTEGER: &str = "SQLITE:integer";
const HINT_ROWID_ALIAS: &str = "SQLITE:integer/rowid-alias";
const HINT_REAL: &str = "SQLITE:real";
const HINT_TEXT: &str = "SQLITE:text";
const HINT_BLOB: &str = "SQLITE:blob";
const HINT_NULL: &str = "SQLITE:null";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Table,
    View,
    WithoutRowid,
    ReadOnlyTable,
}

struct Column {
    name: String,
    generated: bool,
    rowid_alias: bool,
}

struct Table {
    name: String,
    kind: Kind,
    columns: Vec<Column>,
    rowid: Option<&'static str>,
}

/// Reads the database bytes into a document and the parameters needed to write it back.
pub(super) fn decode(input: &[u8], limits: &Limits) -> Result<(SaveDoc, Params)> {
    check_header(input)?;
    let connection = open(input, true)?;
    let tables = list_tables(&connection)?;
    let mut doc = SaveDoc::with_node_limit(limits.max_nodes);
    let mut table_nodes = Vec::new();
    let mut truncated = Vec::new();

    for table in &tables {
        let (node, cut) = read_table(&connection, &mut doc, table)?;
        if cut {
            truncated.push(table.name.clone());
        }
        table_nodes.push((table.name.clone(), node));
    }

    let mut root = Node::new(Value::Object(table_nodes));
    root.type_hint = Some(TypeHint::new(HINT_TABLES));
    root.flags.insert(NodeFlags::READ_ONLY);
    let root = doc.push(root)?;
    doc.set_root(root);

    let mut params = Params::new();
    params.set(ORIGINAL, input.to_vec());
    if !truncated.is_empty() {
        params.set(
            TRUNCATED,
            truncated
                .into_iter()
                .map(saveprism_core::ParamValue::from)
                .collect::<Vec<_>>(),
        );
    }
    Ok((doc, params))
}

/// Checks the header and refuses WAL-mode files, whose data may live in a separate `-wal` file.
pub(super) fn check_header(input: &[u8]) -> Result<()> {
    if !input.starts_with(MAGIC) {
        return Err(parse_error(
            "the file does not start with the SQLite 3 header",
        ));
    }
    let read_version = input.get(18).copied();
    let write_version = input.get(19).copied();
    let (Some(read_version), Some(write_version)) = (read_version, write_version) else {
        return Err(parse_error("the SQLite header is truncated"));
    };
    if read_version == 2 || write_version == 2 {
        return Err(Error::new(
            SQLITE_WAL,
            "the database is in WAL mode; its latest data may be in a separate -wal file, so it cannot be read safely on its own",
        ));
    }
    if input.len() < 100 {
        return Err(parse_error("the SQLite header is truncated"));
    }
    let page_size = match be_u16(input, 16) {
        Some(1) => 65_536_u64,
        Some(size) => u64::from(size),
        None => return Err(parse_error("the SQLite header is truncated")),
    };
    let page_count = u64::from(be_u32(input, 28).unwrap_or(0));
    let expected = page_size.saturating_mul(page_count);
    if page_count > 0 && u64::try_from(input.len()).unwrap_or(u64::MAX) < expected {
        return Err(parse_error(
            "the SQLite file is shorter than the page count in its header",
        ));
    }
    Ok(())
}

fn be_u16(input: &[u8], at: usize) -> Option<u16> {
    let end = at.checked_add(2)?;
    let bytes: [u8; 2] = input.get(at..end)?.try_into().ok()?;
    Some(u16::from_be_bytes(bytes))
}

fn be_u32(input: &[u8], at: usize) -> Option<u32> {
    let end = at.checked_add(4)?;
    let bytes: [u8; 4] = input.get(at..end)?.try_into().ok()?;
    Some(u32::from_be_bytes(bytes))
}

/// Loads `input` into a fresh in-memory connection. The original file is never opened.
pub(super) fn open(input: &[u8], read_only: bool) -> Result<Connection> {
    let mut connection = Connection::open_in_memory().map_err(|error| {
        Error::new(SQLITE_QUERY, "could not create the in-memory database").with_source(error)
    })?;
    connection
        .deserialize_read_exact(MAIN_DB, input, input.len(), read_only)
        .map_err(|error| {
            parse_error("the SQLite database could not be loaded").with_source(error)
        })?;
    connection
        .execute_batch("PRAGMA trusted_schema = OFF; PRAGMA cell_size_check = ON;")
        .map_err(query_error)?;
    if read_only {
        connection
            .execute_batch("PRAGMA query_only = ON;")
            .map_err(query_error)?;
    }
    Ok(connection)
}

fn query_error(error: rusqlite::Error) -> Error {
    Error::new(SQLITE_QUERY, "a query against the SQLite database failed").with_source(error)
}

fn list_tables(connection: &Connection) -> Result<Vec<Table>> {
    let mut statement = connection
        .prepare(
            "SELECT name, type, wr FROM pragma_table_list \
             WHERE schema = 'main' AND type IN ('table', 'view') \
             AND substr(name, 1, 7) <> 'sqlite_' ORDER BY name",
        )
        .map_err(query_error)?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })
        .map_err(query_error)?;
    let mut tables = Vec::new();
    for row in rows {
        let (name, kind, without_rowid) = row.map_err(query_error)?;
        let kind = if kind == "view" {
            Kind::View
        } else if without_rowid != 0 {
            Kind::WithoutRowid
        } else {
            Kind::Table
        };
        let columns = list_columns(connection, &name)?;
        let rowid = if kind == Kind::Table {
            choose_rowid(&columns)
        } else {
            None
        };
        let kind = match (kind, rowid) {
            (Kind::Table, None) => Kind::ReadOnlyTable,
            (kind, _) => kind,
        };
        tables.push(Table {
            name,
            kind,
            columns,
            rowid,
        });
    }
    Ok(tables)
}

fn list_columns(connection: &Connection, table: &str) -> Result<Vec<Column>> {
    let mut statement = connection
        .prepare("SELECT name, hidden, pk, type FROM pragma_table_xinfo(?1) ORDER BY cid")
        .map_err(query_error)?;
    let rows = statement
        .query_map([table], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(query_error)?;
    let mut columns = Vec::new();
    let mut primary_keys = 0_usize;
    for row in rows {
        let (name, hidden, pk, declared) = row.map_err(query_error)?;
        // 1 is a hidden virtual-table column (not part of `SELECT *`); 2 and 3 are generated.
        if hidden != 1 {
            if pk > 0 {
                primary_keys += 1;
            }
            columns.push(Column {
                name,
                generated: hidden >= 2,
                rowid_alias: pk == 1 && declared.eq_ignore_ascii_case("INTEGER") && hidden == 0,
            });
        }
    }
    // `INTEGER PRIMARY KEY` is a rowid alias only as the single primary-key column.
    if primary_keys != 1 {
        for column in &mut columns {
            column.rowid_alias = false;
        }
    }
    Ok(columns)
}

/// The first rowid alias that no column shadows; `None` when all three are regular columns.
fn choose_rowid(columns: &[Column]) -> Option<&'static str> {
    let names: HashSet<String> = columns
        .iter()
        .map(|column| column.name.to_ascii_lowercase())
        .collect();
    PSEUDO_ROWIDS
        .into_iter()
        .find(|candidate| !names.contains(*candidate))
}

fn read_table(connection: &Connection, doc: &mut SaveDoc, table: &Table) -> Result<(NodeId, bool)> {
    let sql = select_sql(table);
    let mut statement = connection.prepare(&sql).map_err(query_error)?;
    let mut rows = statement.query([]).map_err(query_error)?;
    let mut row_nodes = Vec::new();
    let mut truncated = false;
    let read_only = matches!(
        table.kind,
        Kind::View | Kind::WithoutRowid | Kind::ReadOnlyTable
    );

    while let Some(row) = rows.next().map_err(query_error)? {
        if row_nodes.len() == ROW_CAP {
            truncated = true;
            break;
        }
        let mut entries = Vec::new();
        let mut index = 0_usize;
        if let Some(_pseudo) = table.rowid {
            let rowid = row.get::<_, i64>(index).map_err(query_error)?;
            index += 1;
            let mut node = Node::new(Value::Int {
                v: i128::from(rowid),
                width: IntWidth::I64,
            });
            node.type_hint = Some(TypeHint::new(HINT_ROWID));
            node.flags.insert(NodeFlags::READ_ONLY);
            entries.push((ROWID_KEY.to_string(), doc.push(node)?));
        }
        for column in &table.columns {
            let raw = row.get_ref(index).map_err(query_error)?;
            index += 1;
            let (value, hint, forced_read_only) = cell(raw);
            let hint = if column.rowid_alias && table.rowid.is_some() {
                HINT_ROWID_ALIAS
            } else {
                hint
            };
            let mut node = Node::new(value);
            node.type_hint = Some(TypeHint::new(hint));
            if read_only || column.generated || forced_read_only {
                node.flags.insert(NodeFlags::READ_ONLY);
            }
            entries.push((column.name.clone(), doc.push(node)?));
        }
        let mut row_node = Node::new(Value::Object(entries));
        row_node.type_hint = Some(TypeHint::new(HINT_ROW));
        row_node.flags.insert(NodeFlags::READ_ONLY);
        row_nodes.push(doc.push(row_node)?);
    }

    let hint = match table.kind {
        Kind::Table => HINT_TABLE,
        Kind::View => HINT_VIEW,
        Kind::WithoutRowid => HINT_WITHOUT_ROWID,
        Kind::ReadOnlyTable => HINT_READ_ONLY_TABLE,
    };
    let hint = if truncated {
        format!("{hint}{HINT_TRUNCATED}")
    } else {
        hint.to_string()
    };
    let mut node = Node::new(Value::Array(row_nodes));
    node.type_hint = Some(TypeHint::new(hint));
    node.flags.insert(NodeFlags::READ_ONLY);
    Ok((doc.push(node)?, truncated))
}

fn select_sql(table: &Table) -> String {
    let mut expressions = Vec::new();
    if let Some(pseudo) = table.rowid {
        expressions.push(format!("{pseudo} AS \"__saveprism_rowid\""));
    }
    for column in &table.columns {
        expressions.push(quote_ident(&column.name));
    }
    if expressions.is_empty() {
        expressions.push("NULL AS \"__saveprism_empty\"".to_string());
    }
    let mut sql = format!(
        "SELECT {} FROM {}",
        expressions.join(", "),
        quote_ident(&table.name)
    );
    if let Some(pseudo) = table.rowid {
        sql.push_str(" ORDER BY ");
        sql.push_str(pseudo);
    }
    sql.push_str(" LIMIT ");
    sql.push_str(&(ROW_CAP + 1).to_string());
    sql
}

fn cell(raw: ValueRef<'_>) -> (Value, &'static str, bool) {
    match raw {
        ValueRef::Null => (Value::Null, HINT_NULL, true),
        ValueRef::Integer(value) => (
            Value::Int {
                v: i128::from(value),
                width: IntWidth::I64,
            },
            HINT_INTEGER,
            false,
        ),
        ValueRef::Real(value) => (
            Value::Float {
                v: value,
                width: FloatWidth::F64,
            },
            HINT_REAL,
            false,
        ),
        ValueRef::Text(bytes) => match std::str::from_utf8(bytes) {
            Ok(text) => (Value::Str(text.to_string()), HINT_TEXT, false),
            Err(_) => (Value::Bytes(bytes.to_vec()), HINT_BLOB, true),
        },
        ValueRef::Blob(bytes) => (Value::Bytes(bytes.to_vec()), HINT_BLOB, true),
    }
}

/// Doubles `"` inside an identifier and wraps it in quotes.
pub(super) fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}
