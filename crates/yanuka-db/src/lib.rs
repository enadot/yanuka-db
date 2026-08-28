//! Local SQLite storage for the contacts database.
//!
//! Free of any Tauri dependency by design, so it builds and its tests run on
//! any machine — including CI runners without a system webview. The Tauri shell
//! in `apps/desktop/src-tauri` is a thin layer over this crate that does
//! nothing but marshal arguments across the IPC boundary.
//!
//! ```text
//! cargo test -p yanuka-db -p yanuka-search
//! ```

pub mod backup;
pub mod connection;
pub mod encryption;
pub mod error;
pub mod index;
pub mod merge;
pub mod migrate;
pub mod models;
pub mod mutation;
pub mod repository;
pub mod search;
pub mod taxonomy;

pub use connection::{open, open_in_memory, sqlite_version};
pub use error::{DbError, Result};
pub use migrate::{current_version, migrate, target_version};

/// Re-exported so the Tauri shell can hold a `Connection` without taking its
/// own rusqlite dependency, which would risk two different versions of the
/// bundled SQLite ending up in one binary.
pub use rusqlite;

use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

/// Current time as an ISO-8601 UTC string.
///
/// Every timestamp written to the database goes through here. Storing UTC and
/// formatting to local time only for display is what keeps `updated_at`
/// comparisons meaningful when two devices sit in different time zones.
pub fn now_iso() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

/// Mint a new ULID.
///
/// Client-generated so a record can be created while offline and a retry cannot
/// produce two rows. Auto-increment integers are unusable here for exactly that
/// reason — see docs/DECISIONS.md ADR-004.
pub fn new_id() -> String {
    ulid::Ulid::new().to_string()
}
