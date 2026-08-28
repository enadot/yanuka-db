//! Encryption at rest (SQLCipher). See docs/SECURITY.md and ADR-033.
//!
//! The key is a random 256-bit value the desktop shell keeps in the OS
//! credential store — not a passphrase. A passphrase would put priority 6
//! (security) above priority 1 (no data loss): the user who forgets it has
//! lost every contact and every backup at once. A random key costs nothing to
//! remember, and the shell surfaces it as a recovery key to be stored off the
//! machine.
//!
//! Everything here works on the *pragma value* form of the key — the SQLCipher
//! raw-key literal `x'<64 hex>'` — which skips the passphrase KDF entirely;
//! stretching an already-random 256-bit value adds startup latency and nothing
//! else.

use std::path::Path;
#[cfg(feature = "sqlcipher")]
use std::path::PathBuf;

use crate::error::{DbError, Result};

/// Every SQLite database begins with these 16 bytes; a SQLCipher database
/// begins with its random salt instead, which is what makes this check valid.
const SQLITE_MAGIC: [u8; 16] = *b"SQLite format 3\0";

/// Whether the file on disk is an *unencrypted* SQLite database.
///
/// A missing or unreadable file answers `false`: a database that does not
/// exist yet must be created through the encrypted path, never "upgraded".
pub fn is_plaintext(path: &Path) -> bool {
    use std::io::Read;
    std::fs::File::open(path)
        .and_then(|mut file| {
            let mut header = [0u8; 16];
            file.read_exact(&mut header)?;
            Ok(header == SQLITE_MAGIC)
        })
        .unwrap_or(false)
}

/// Normalize a user- or store-supplied key into the SQLCipher raw-key literal.
///
/// Accepts the 64 hex digits in any case, with any separators (the recovery
/// key is displayed in dashed groups, and people paste what they see).
pub fn raw_key_pragma(hex: &str) -> Result<String> {
    let normalized: String =
        hex.chars().filter(|c| c.is_ascii_hexdigit()).map(|c| c.to_ascii_lowercase()).collect();
    if normalized.len() != 64 {
        return Err(DbError::Validation("מפתח שחזור חייב להכיל 64 ספרות הקסדצימליות".into()));
    }
    Ok(format!("x'{normalized}'"))
}

/// Whether an open error means "this file is encrypted and the key is wrong or
/// missing" — SQLite reports both as "file is not a database", because with the
/// wrong key the decrypted header is noise.
pub fn is_wrong_key(error: &DbError) -> bool {
    matches!(
        error,
        DbError::Sqlite(rusqlite::Error::SqliteFailure(failure, _))
            if failure.code == rusqlite::ErrorCode::NotADatabase
    )
}

#[cfg(feature = "sqlcipher")]
fn sibling(path: &Path, suffix: &str) -> PathBuf {
    PathBuf::from(format!("{}{suffix}", path.display()))
}

#[cfg(feature = "sqlcipher")]
fn count(connection: &rusqlite::Connection, table: &str) -> Result<i64> {
    Ok(connection.query_row(&format!("SELECT count(*) FROM {table}"), [], |row| row.get(0))?)
}

/// Upgrade a plaintext database file to an encrypted one, in place.
///
/// The order is what makes this safe under priority 1: export into a staging
/// file, *verify the staging file end to end* (open with the key, integrity
/// check, row counts), and only then swap. A crash anywhere before the final
/// rename leaves the plaintext original untouched; the pre-migration copy the
/// shell takes on every launch covers the swap itself.
#[cfg(feature = "sqlcipher")]
pub fn encrypt_in_place(path: &Path, key_pragma: &str) -> Result<()> {
    let io_error =
        |error: std::io::Error| DbError::Validation(format!("שגיאת קבצים בהצפנה: {error}"));

    let staging = sibling(path, ".encrypting");
    let _ = std::fs::remove_file(&staging);
    for suffix in ["-wal", "-shm"] {
        let _ = std::fs::remove_file(sibling(&staging, suffix));
    }

    let (contacts, mutations);
    {
        let source = crate::connection::open(path, None)?;
        contacts = count(&source, "contacts")?;
        mutations = count(&source, "mutations")?;
        source.execute(
            "ATTACH DATABASE ?1 AS encrypted KEY ?2",
            rusqlite::params![staging.to_string_lossy(), key_pragma],
        )?;
        // sqlcipher_export copies the whole schema and every row through the
        // destination's codec; it is SQLCipher's own supported migration path.
        source.query_row("SELECT sqlcipher_export('encrypted')", [], |_| Ok(()))?;
        source.execute("DETACH DATABASE encrypted", [])?;
    }

    {
        let check = crate::connection::open(&staging, Some(key_pragma))?;
        let verdict: String = check.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
        if verdict != "ok" {
            return Err(DbError::Validation(format!("אימות ההצפנה נכשל: {verdict}")));
        }
        if count(&check, "contacts")? != contacts || count(&check, "mutations")? != mutations {
            return Err(DbError::Validation("אימות ההצפנה נכשל: מספר הרשומות אינו תואם".into()));
        }
    }
    for suffix in ["-wal", "-shm"] {
        let _ = std::fs::remove_file(sibling(&staging, suffix));
    }

    // The swap. Stale -wal/-shm files carry the *plaintext* database's name,
    // and SQLite would try to recover them against the encrypted file — they
    // must be gone before the rename.
    let retired = sibling(path, ".plaintext-old");
    std::fs::rename(path, &retired).map_err(io_error)?;
    for suffix in ["-wal", "-shm"] {
        let _ = std::fs::remove_file(sibling(path, suffix));
    }
    std::fs::rename(&staging, path).map_err(io_error)?;
    std::fs::remove_file(&retired).map_err(io_error)?;
    Ok(())
}
