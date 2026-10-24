//! `format.sqlite`: SQLite database saves (`.sqlite`, `.sqlite3`, `.db`, and any `.save` that
//! starts with the SQLite header).
//!
//! Some games keep their whole save in one SQLite database. The file is loaded into an in-memory
//! connection (`sqlite3_deserialize`); the original file is never opened or written, no extension
//! is loaded, no schema object is executed, and the connection reads with `query_only` on,
//! `trusted_schema` off and `cell_size_check` on.
//!
//! # Tree
//!
//! The root is an `Object` of tables (hint `SQLITE:tables`), sorted by name; internal `sqlite_*`
//! tables are skipped. A table is an `Array` of row `Object`s (`SQLITE:table`), keyed by column
//! name in table order. Each row of a rowid table also has a read-only `(rowid)` entry. Values map
//! as follows:
//!
//! | SQLite | Node | Editable |
//! |---|---|---|
//! | INTEGER | `Int` (`I64`) | yes |
//! | REAL | `Float` (`F64`) | yes |
//! | TEXT | `Str` (UTF-8) | yes |
//! | BLOB | `Bytes`, read-only | no |
//! | NULL | `Null`, read-only | no |
//!
//! Views, `WITHOUT ROWID` tables, generated columns, text that is not valid UTF-8 and tables whose
//! `rowid` is hidden by all three aliases are read-only. At most 10,000 rows are read per table;
//! a truncated table is marked `.../truncated` and the names are in the `sqlite:truncated`
//! parameter.
//!
//! # Exact writing
//!
//! The original database bytes are kept in `sqlite:original`. When the document still equals a
//! fresh decode of those bytes, `encode` returns them unchanged. Otherwise it loads the original
//! into an in-memory connection and runs one parameterized `UPDATE "<table>" SET "<column>" = ?
//! WHERE rowid = ?` per changed cell inside one transaction, then serializes the database. Types
//! cannot change (an integer stays an integer), and read-only cells are refused.
//!
//! Errors: `SP-SQLITE-001` malformed or truncated database, `SP-SQLITE-002` WAL-mode database that
//! needs its `-wal` file, `SP-SQLITE-003` a change that cannot be written, `SP-SQLITE-004` a
//! database query failed.

mod read;
mod write;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    Codec, Confidence, DecodeCtx, Decoded, Error, ErrorCode, Params, Registry, Result, SaveDoc,
    SniffCtx,
};

/// The data is malformed, truncated, or not an SQLite database.
pub const SQLITE_PARSE: ErrorCode = ErrorCode::new("SP-SQLITE-001");
/// The file is in WAL mode, so it cannot be read without its `-wal` file.
pub const SQLITE_WAL: ErrorCode = ErrorCode::new("SP-SQLITE-002");
/// The document cannot be written back to the database.
pub const SQLITE_WRITE: ErrorCode = ErrorCode::new("SP-SQLITE-003");
/// A query against the in-memory database failed.
pub const SQLITE_QUERY: ErrorCode = ErrorCode::new("SP-SQLITE-004");

/// The 16-byte header of every SQLite 3 database.
pub(crate) const MAGIC: &[u8; 16] = b"SQLite format 3\0";
/// Most rows read per table.
pub(crate) const ROW_CAP: usize = 10_000;
/// Key of the rowid entry of a row.
pub(crate) const ROWID_KEY: &str = "(rowid)";
/// Prefix of every hint this codec writes.
pub(crate) const HINT_PREFIX: &str = "SQLITE:";
/// Key of the original database bytes in the step parameters.
pub(crate) const ORIGINAL: &str = "sqlite:original";
/// Key of the list of tables cut at [`ROW_CAP`].
pub(crate) const TRUNCATED: &str = "sqlite:truncated";

const EXTENSIONS: [&str; 3] = [".sqlite", ".sqlite3", ".db"];

/// Registers the SQLite codec as `format.sqlite`.
pub fn register(reg: &mut Registry) {
    reg.add_codec(Arc::new(SqliteCodec));
}

/// The `format.sqlite` codec.
#[derive(Clone, Copy, Debug, Default)]
pub struct SqliteCodec;

impl Codec for SqliteCodec {
    fn id(&self) -> &'static str {
        "format.sqlite"
    }

    /// The 16-byte header is high confidence; the usual database extensions are a weak hint.
    fn sniff(&self, input: &[u8], ctx: &SniffCtx<'_>) -> Confidence {
        if input.starts_with(MAGIC) {
            return Confidence::new(0.99);
        }
        let named = ctx.file_name.is_some_and(|name| {
            let lower = name.to_ascii_lowercase();
            EXTENSIONS
                .iter()
                .any(|extension| lower.ends_with(extension))
        });
        if named {
            Confidence::new(0.5)
        } else {
            Confidence::NONE
        }
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<Decoded> {
        ctx.limits().check_input(len_u64(input.len()))?;
        let (doc, params) = read::decode(input, ctx.limits())?;
        ctx.charge_nodes(len_u64(doc.len().saturating_sub(1)))?;
        Ok(Decoded { doc, params })
    }

    fn encode(&self, doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
        write::encode(doc, params)
    }
}

pub(crate) fn parse_error(message: impl Into<String>) -> Error {
    Error::new(SQLITE_PARSE, message)
}
