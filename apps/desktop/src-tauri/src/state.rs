use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use yanuka_db::rusqlite::Connection;
use yanuka_db::semantic::SemanticEngine;
use yanuka_db::{encryption, migrate, open, DbError};

use crate::keys;

/// The database, which is either open or waiting for its key.
///
/// `Locked` happens on exactly one path: the file on disk is encrypted and
/// the OS credential store did not produce the key that opens it — a backup
/// restored onto a fresh Windows, or a reinstalled machine. The user unlocks
/// it with the recovery key; nothing else is allowed through.
enum DbState {
    Ready(Connection),
    Locked,
}

/// What the settings screen needs to say about encryption.
struct Security {
    encrypted: bool,
    /// 64 lowercase hex digits; the recovery key is its display form.
    key_hex: Option<String>,
    /// Whether the OS credential store holds the key (vs. memory-only).
    key_persisted: bool,
}

/// What the shell knows about semantic search right now.
///
/// `engine: None` is a normal state — a development run without the fetched
/// model — and it must cost nothing: search skips the layer, settings says
/// "unavailable". The counters describe the background catch-up so settings
/// can show progress instead of a spinner.
struct Semantic {
    engine: Option<Arc<SemanticEngine>>,
    indexed: usize,
    pending: usize,
    catching_up: bool,
}

/// The open database, guarded for shared access across IPC calls.
///
/// A single connection behind a mutex rather than a pool: SQLite in WAL mode
/// serializes writers anyway, this is a single-user desktop application, and
/// one connection makes the "every write is one transaction" invariant trivial
/// to hold. A pool becomes worthwhile only if read concurrency ever shows up in
/// a profile.
pub struct AppState {
    inner: Mutex<DbState>,
    security: Mutex<Security>,
    semantic: Mutex<Semantic>,
    /// Where the database lives, for the backup commands.
    database_path: PathBuf,
}

impl AppState {
    /// Open the database, encrypting on the way when the platform allows it.
    ///
    /// The decision tree is written from the product's priority list: the data
    /// must open (1, 2) even when encryption (6) cannot be had — so a failed
    /// upgrade or a missing credential store degrades to plaintext with a
    /// visible status, never to a refusal to start. The one state that blocks
    /// is an encrypted file without its key, where opening is impossible by
    /// construction and only the recovery key helps.
    pub fn open(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let Some(key) = keys::load_or_create() else {
            // No credential store (a development build on Linux/macOS).
            if path.exists() && !encryption::is_plaintext(path) {
                return Ok(Self::assemble(DbState::Locked, locked_security(), path));
            }
            let connection = open_ready(path, None)?;
            let security = Security { encrypted: false, key_hex: None, key_persisted: false };
            return Ok(Self::assemble(DbState::Ready(connection), security, path));
        };

        let pragma = encryption::raw_key_pragma(&key.hex)?;

        // A database from the pre-encryption era is upgraded in place, once.
        // If the upgrade fails the plaintext file is untouched — open it, and
        // let settings show that encryption is off rather than lock the user
        // out of their own data.
        if encryption::is_plaintext(path) {
            if let Err(error) = encryption::encrypt_in_place(path, &pragma) {
                eprintln!("encryption upgrade failed, staying plaintext: {error}");
                let connection = open_ready(path, None)?;
                let security = Security {
                    encrypted: false,
                    key_hex: Some(key.hex),
                    key_persisted: key.persisted,
                };
                return Ok(Self::assemble(DbState::Ready(connection), security, path));
            }
            eprintln!("database encrypted in place");
        }

        match open_ready(path, Some(&pragma)) {
            Ok(connection) => {
                let security = Security {
                    encrypted: true,
                    key_hex: Some(key.hex),
                    key_persisted: key.persisted,
                };
                Ok(Self::assemble(DbState::Ready(connection), security, path))
            }
            // The store's key does not open this file: it was encrypted on
            // another machine (a restored backup). Recovery key territory.
            Err(error) if encryption::is_wrong_key(&error) => {
                Ok(Self::assemble(DbState::Locked, locked_security(), path))
            }
            Err(error) => Err(error.into()),
        }
    }

    fn assemble(state: DbState, security: Security, path: &Path) -> Self {
        Self {
            inner: Mutex::new(state),
            security: Mutex::new(security),
            semantic: Mutex::new(Semantic {
                engine: None,
                indexed: 0,
                pending: 0,
                catching_up: false,
            }),
            database_path: path.to_path_buf(),
        }
    }

    /// Hand the loaded embedding model to the state; called once at setup.
    pub fn attach_semantic(&self, engine: SemanticEngine) {
        let mut semantic = self.semantic.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        semantic.engine = Some(Arc::new(engine));
        semantic.catching_up = true;
    }

    pub fn semantic_engine(&self) -> Option<Arc<SemanticEngine>> {
        let semantic = self.semantic.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        semantic.engine.clone()
    }

    pub fn set_semantic_progress(&self, indexed: usize, pending: usize, catching_up: bool) {
        let mut semantic = self.semantic.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        semantic.indexed = indexed;
        semantic.pending = pending;
        semantic.catching_up = catching_up;
    }

    pub fn semantic_status(&self) -> serde_json::Value {
        let semantic = self.semantic.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        serde_json::json!({
            "state": if semantic.engine.is_none() {
                "unavailable"
            } else if semantic.catching_up {
                "indexing"
            } else {
                "ready"
            },
            "indexed": semantic.indexed,
            "pending": semantic.pending,
        })
    }

    /// Bring one contact's semantic documents up to date after a mutation.
    /// Failures are swallowed by design: a stale vector row is a worse search
    /// result, not a broken application, and the reconciler will retry it on
    /// the next startup catch-up.
    pub fn semantic_touch(&self, contact_id: &str) {
        let Some(engine) = self.semantic_engine() else {
            return;
        };
        let _ = self.with(|connection| {
            if let Err(error) = yanuka_db::semantic::sync_contact(connection, &engine, contact_id) {
                eprintln!("semantic sync for {contact_id} failed: {error}");
            }
            // Categories defined "by meaning" can only be judged once the
            // contact's vectors are current — which is now.
            if let Err(error) =
                yanuka_db::categories::refresh_contact_meaning(connection, &engine, contact_id)
            {
                eprintln!("meaning categories for {contact_id} failed: {error}");
            }
            Ok(())
        });
    }

    /// Open the locked database with a recovery key the user supplied, then
    /// remember the key in the OS store so the next launch unlocks itself.
    pub fn unlock(&self, typed: &str) -> Result<(), DbError> {
        let pragma = encryption::raw_key_pragma(typed)?;
        let mut connection = match open(&self.database_path, Some(&pragma)) {
            Ok(connection) => connection,
            Err(error) if encryption::is_wrong_key(&error) => {
                return Err(DbError::Validation("מפתח השחזור אינו מתאים למאגר הזה".into()));
            }
            Err(error) => return Err(error),
        };
        let applied = migrate(&mut connection)?;
        if applied > 0 {
            eprintln!("applied {applied} migration(s)");
        }

        let hex: String = typed
            .chars()
            .filter(|c| c.is_ascii_hexdigit())
            .map(|c| c.to_ascii_lowercase())
            .collect();
        let persisted = keys::persist(&hex);

        let mut guard = self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        *guard = DbState::Ready(connection);
        let mut security = self.security.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        *security = Security { encrypted: true, key_hex: Some(hex), key_persisted: persisted };
        Ok(())
    }

    pub fn database_path(&self) -> &std::path::Path {
        &self.database_path
    }

    /// The key in `PRAGMA key` form, for keying backup destinations.
    pub fn key_pragma(&self) -> Option<String> {
        let security = self.security.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if !security.encrypted {
            return None;
        }
        security.key_hex.as_deref().and_then(|hex| encryption::raw_key_pragma(hex).ok())
    }

    /// The recovery key in its display form, when the database is encrypted.
    pub fn recovery_key(&self) -> Option<String> {
        let security = self.security.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if !security.encrypted {
            return None;
        }
        security.key_hex.as_deref().map(keys::format_for_display)
    }

    pub fn security_status(&self) -> serde_json::Value {
        let locked = {
            let guard = self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            matches!(*guard, DbState::Locked)
        };
        let security = self.security.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        serde_json::json!({
            "state": if locked {
                "locked"
            } else if security.encrypted {
                "encrypted"
            } else {
                "plaintext"
            },
            "keyPersisted": security.key_persisted,
        })
    }

    /// Run something against the database.
    ///
    /// A poisoned mutex means a previous command panicked mid-transaction.
    /// Recovering the guard is correct here: SQLite already rolled that
    /// transaction back, so the database is consistent, and refusing every
    /// subsequent request would strand the user's data behind a dead lock.
    pub fn with<T>(
        &self,
        f: impl FnOnce(&mut Connection) -> Result<T, DbError>,
    ) -> Result<T, DbError> {
        let mut guard = self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        match &mut *guard {
            DbState::Ready(connection) => f(connection),
            DbState::Locked => Err(DbError::Locked),
        }
    }
}

fn locked_security() -> Security {
    Security { encrypted: true, key_hex: None, key_persisted: false }
}

fn open_ready(path: &Path, key: Option<&str>) -> Result<Connection, DbError> {
    let mut connection = open(path, key)?;
    let applied = migrate(&mut connection)?;
    if applied > 0 {
        eprintln!("applied {applied} migration(s)");
    }
    // The default shelves, once, into an archive that has none (ADR-038).
    let installed = yanuka_db::categories::install_defaults(&mut connection, None)?;
    if installed > 0 {
        eprintln!("installed {installed} default categories");
    }
    Ok(connection)
}
