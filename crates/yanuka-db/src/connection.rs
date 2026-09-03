use rusqlite::functions::FunctionFlags;
use rusqlite::Connection;
use std::path::Path;
use yanuka_search::normalize_text;

use crate::error::{DbError, Result};

/// Pragmas applied to every connection.
///
/// `foreign_keys` is per-connection in SQLite and off by default; forgetting it
/// silently disables every foreign key in the schema. `temp_store = MEMORY` is
/// set now rather than when encryption lands, because SQLCipher would otherwise
/// spill plaintext temporary files to disk.
///
/// Mirrors `CONNECTION_PRAGMAS` in `packages/database/src/migrations.ts`.
const PRAGMAS: &[&str] = &[
    "PRAGMA journal_mode = WAL",
    "PRAGMA synchronous = NORMAL",
    "PRAGMA foreign_keys = ON",
    "PRAGMA busy_timeout = 5000",
    "PRAGMA cache_size = -65536",
    "PRAGMA temp_store = MEMORY",
];

/// Open the local database.
///
/// Every connection in the application goes through this one function. That is
/// deliberate: enabling encryption later means issuing `PRAGMA key` immediately
/// after `open` and before any other statement, and having a single entry point
/// is what makes that a contained change rather than an audit of every call
/// site. See docs/SECURITY.md.
///
/// `key` is accepted now and rejected unless the `sqlcipher` feature is built,
/// so callers can be written against the final signature today.
pub fn open(path: &Path, key: Option<&str>) -> Result<Connection> {
    let connection = Connection::open(path)?;
    configure(&connection, key)?;
    Ok(connection)
}

/// Open an in-memory database. Used by tests and by the schema conformance
/// checks; never by the application.
pub fn open_in_memory() -> Result<Connection> {
    let connection = Connection::open_in_memory()?;
    configure(&connection, None)?;
    Ok(connection)
}

/// Key a connection. Must run before any other statement — SQLite reads the
/// header lazily, and the first real statement is what decrypts it.
///
/// The value is handed to `PRAGMA key` verbatim, so both forms SQLCipher
/// accepts work: a passphrase, or the raw-key literal `x'<64 hex>'` that
/// `encryption::raw_key_pragma` produces (which skips the KDF).
pub fn apply_key(connection: &Connection, key: &str) -> Result<()> {
    #[cfg(not(feature = "sqlcipher"))]
    {
        let _ = (connection, key);
        Err(DbError::MissingCapability("הצפנה"))
    }

    #[cfg(feature = "sqlcipher")]
    {
        connection.pragma_update(None, "key", key)?;
        Ok(())
    }
}

fn configure(connection: &Connection, key: Option<&str>) -> Result<()> {
    if let Some(key) = key {
        apply_key(connection, key)?;
    }

    for pragma in PRAGMAS {
        // `journal_mode` returns a row rather than executing silently, and an
        // in-memory database answers `memory` instead of `wal`. Both are fine;
        // what matters is that the statement ran.
        connection.execute_batch(pragma)?;
    }

    // The Hebrew normalizer as a SQL function, so category rules (ADR-038) can
    // compare stored text the way the search index does — niqqud, gershayim
    // and final letters folded — without materializing a normalized copy of
    // every column they may read.
    connection.create_scalar_function(
        "yanuka_normalize",
        1,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |context| {
            let value: Option<String> = context.get(0)?;
            Ok(value.map(|text| normalize_text(&text)))
        },
    )?;

    assert_capabilities(connection)?;
    Ok(())
}

/// Fail loudly at startup if the bundled SQLite lacks something the schema
/// needs, rather than at the first search on a user's machine.
pub fn assert_capabilities(connection: &Connection) -> Result<()> {
    let has_fts5: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_compile_options WHERE compile_options = 'ENABLE_FTS5')",
        [],
        |row| row.get(0),
    )?;
    if !has_fts5 {
        return Err(DbError::MissingCapability("FTS5"));
    }

    // The trigram tokenizer arrived in SQLite 3.34 and backs the fuzzy layer.
    // Creating a throwaway table is the only reliable way to test for it.
    connection
        .execute_batch(
            "CREATE VIRTUAL TABLE temp._capability_probe USING fts5(x, tokenize='trigram');
             DROP TABLE temp._capability_probe;",
        )
        .map_err(|_| DbError::MissingCapability("trigram tokenizer"))?;

    Ok(())
}

/// SQLite version the binary was built against, for the settings screen and
/// for bug reports.
pub fn sqlite_version(connection: &Connection) -> Result<String> {
    Ok(connection.query_row("SELECT sqlite_version()", [], |row| row.get(0))?)
}
