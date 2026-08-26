use std::sync::Mutex;

use yanuka_db::rusqlite::Connection;
use yanuka_db::{migrate, open};

/// The open database, guarded for shared access across IPC calls.
///
/// A single connection behind a mutex rather than a pool: SQLite in WAL mode
/// serializes writers anyway, this is a single-user desktop application, and
/// one connection makes the "every write is one transaction" invariant trivial
/// to hold. A pool becomes worthwhile only if read concurrency ever shows up in
/// a profile.
pub struct AppState {
    connection: Mutex<Connection>,
    /// Where the database lives, for the backup commands.
    database_path: std::path::PathBuf,
}

impl AppState {
    pub fn open(path: &std::path::Path) -> Result<Self, Box<dyn std::error::Error>> {
        // `None` for the key: encryption is not enabled in this build. The
        // parameter exists so turning it on is a one-line change here rather
        // than a rewrite. See docs/SECURITY.md.
        let mut connection = open(path, None)?;
        let applied = migrate(&mut connection)?;
        if applied > 0 {
            eprintln!("applied {applied} migration(s)");
        }
        Ok(Self { connection: Mutex::new(connection), database_path: path.to_path_buf() })
    }

    /// Run something against the database.
    ///
    /// A poisoned mutex means a previous command panicked mid-transaction.
    /// Recovering the guard is correct here: SQLite already rolled that
    /// transaction back, so the database is consistent, and refusing every
    /// subsequent request would strand the user's data behind a dead lock.
    pub fn database_path(&self) -> &std::path::Path {
        &self.database_path
    }

    pub fn with<T>(&self, f: impl FnOnce(&mut Connection) -> T) -> T {
        let mut guard = self.connection.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        f(&mut guard)
    }
}
