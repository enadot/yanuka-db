//! Live backups of the open database.
//!
//! The archive this product manages exists as one SQLite file on one machine
//! that is frequently offline — no sync, no cloud, no second copy anywhere.
//! Priority 1 (מידע לא הולך לאיבוד) therefore needs two things the
//! pre-migration copy in the Tauri shell does not provide: a backup that runs
//! on an ordinary day, and a backup the user can point at a USB stick.
//!
//! Both use SQLite's online-backup API rather than a file copy, because here a
//! connection *is* open: the API produces a consistent snapshot mid-write,
//! WAL included, with nothing to flush first.

use std::io;
use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::error::{DbError, Result};
use crate::now_iso;

fn io_error(error: io::Error) -> DbError {
    DbError::Validation(format!("שגיאת קבצים בגיבוי: {error}"))
}

/// Snapshot the open database into `target`, consistently, while in use.
///
/// `key_pragma` is the SQLCipher raw-key literal of the *source* database, or
/// `None` when it is unencrypted. The online-backup API writes pages through
/// the destination's codec, so an unkeyed destination would silently produce a
/// **plaintext** copy of an encrypted database — the classic way encryption at
/// rest leaks through its own backups (threat 3 in docs/SECURITY.md).
pub fn backup_to(connection: &Connection, target: &Path, key_pragma: Option<&str>) -> Result<()> {
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(io_error)?;
    }
    let mut destination = Connection::open(target)?;
    if let Some(key) = key_pragma {
        crate::connection::apply_key(&destination, key)?;
    }
    let backup = rusqlite::backup::Backup::new(connection, &mut destination)?;
    backup.run_to_completion(64, std::time::Duration::from_millis(5), None)?;
    Ok(())
}

/// One backup per calendar day, taken on launch, rotated.
///
/// The file is named by date (`daily-2026-08-21.db`), so "already backed up
/// today" is a filename check — no clock arithmetic, no state. Returns the
/// path when a backup was taken, `None` when today's already exists.
pub fn daily_backup(
    connection: &Connection,
    database_path: &Path,
    keep: usize,
    key_pragma: Option<&str>,
) -> Result<Option<PathBuf>> {
    let backups = backups_directory(database_path);
    let today = &now_iso()[..10];
    let target = backups.join(format!("daily-{today}.db"));
    if target.exists() {
        return Ok(None);
    }

    backup_to(connection, &target, key_pragma)?;
    prune_daily(&backups, keep)?;
    Ok(Some(target))
}

/// When the newest backup of any kind (daily or pre-migration) was taken,
/// as an ISO date-time, for the settings screen. `None` when none exist.
pub fn last_backup_at(database_path: &Path) -> Option<String> {
    let backups = backups_directory(database_path);
    let newest = std::fs::read_dir(backups)
        .ok()?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_name().to_string_lossy().ends_with(".db"))
        .filter_map(|entry| entry.metadata().ok()?.modified().ok())
        .max()?;
    let stamp = time::OffsetDateTime::from(newest);
    stamp.format(&time::format_description::well_known::Rfc3339).ok()
}

fn backups_directory(database_path: &Path) -> PathBuf {
    database_path.parent().unwrap_or_else(|| Path::new(".")).join("backups")
}

fn prune_daily(backups: &Path, keep: usize) -> Result<()> {
    let mut dailies: Vec<_> = std::fs::read_dir(backups)
        .map_err(io_error)?
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            name.starts_with("daily-") && name.ends_with(".db")
        })
        .collect();

    // Dates in the names make lexical order chronological order.
    dailies.sort_by_key(|entry| entry.file_name());
    while dailies.len() > keep {
        let oldest = dailies.remove(0);
        let _ = std::fs::remove_file(oldest.path());
        for suffix in ["-wal", "-shm"] {
            let _ =
                std::fs::remove_file(PathBuf::from(format!("{}{suffix}", oldest.path().display())));
        }
    }
    Ok(())
}
