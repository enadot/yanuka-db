use rusqlite::Connection;

use crate::error::{DbError, Result};

/// The schema, embedded at compile time from the shared migrations directory.
///
/// `packages/database/migrations/sqlite/*.sql` is the single source of truth:
/// the TypeScript test harness runs the same text through `node:sqlite`, so the
/// schema exercised by `pnpm test` is byte-for-byte the schema shipped here.
/// A `build.rs` marks the directory so edits trigger a rebuild.
pub struct Migration {
    pub version: i64,
    pub name: &'static str,
    pub sql: &'static str,
}

pub const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "0001_initial_schema",
        sql: include_str!("../../../packages/database/migrations/sqlite/0001_initial_schema.sql"),
    },
    Migration {
        version: 2,
        name: "0002_semantic_index",
        sql: include_str!("../../../packages/database/migrations/sqlite/0002_semantic_index.sql"),
    },
    Migration {
        version: 3,
        name: "0003_ocr_notebooks",
        sql: include_str!("../../../packages/database/migrations/sqlite/0003_ocr_notebooks.sql"),
    },
    Migration {
        version: 4,
        name: "0004_smart_categories",
        sql: include_str!("../../../packages/database/migrations/sqlite/0004_smart_categories.sql"),
    },
];

/// Highest schema version this build can produce.
pub fn target_version() -> i64 {
    MIGRATIONS.last().map(|m| m.version).unwrap_or(0)
}

const MIGRATIONS_TABLE: &str = "
CREATE TABLE IF NOT EXISTS schema_migrations (
  version    INTEGER PRIMARY KEY,
  name       TEXT NOT NULL,
  checksum   TEXT NOT NULL,
  applied_at TEXT NOT NULL
) STRICT;
";

/// FNV-1a-derived checksum, matching `checksum()` in
/// `packages/database/src/migrations.ts` so both runners detect the same drift.
pub fn checksum(sql: &str) -> String {
    let mut h1: u32 = 0x811c9dc5;
    let mut h2: u32 = 0x01000193;
    for unit in sql.encode_utf16() {
        h1 = (h1 ^ unit as u32).wrapping_mul(0x01000193);
        h2 = (h2.wrapping_add(unit as u32)).wrapping_mul(0x85ebca6b);
    }
    format!("{h1:08x}{h2:08x}")
}

/// Apply every pending migration. Returns how many ran.
///
/// Forward-only by design. There are no `down` files: a failed upgrade is
/// recovered by restoring the automatic pre-migration backup, which is the only
/// approach that cannot lose a column's worth of data. Losing user data is the
/// one outcome this product must never produce — see docs/DATABASE.md.
pub fn migrate(connection: &mut Connection) -> Result<usize> {
    connection.execute_batch(MIGRATIONS_TABLE)?;

    let applied: Vec<(i64, String)> = {
        let mut statement =
            connection.prepare("SELECT version, checksum FROM schema_migrations")?;
        let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };

    let mut count = 0;
    for migration in MIGRATIONS {
        if let Some((_, existing)) = applied.iter().find(|(v, _)| *v == migration.version) {
            // An edited migration means the database in front of us was built
            // by different SQL than this binary expects. Continuing would leave
            // the schema in a state nobody has tested.
            if existing != &checksum(migration.sql) {
                return Err(DbError::MigrationChanged(migration.name.to_string()));
            }
            continue;
        }

        let transaction = connection.transaction()?;
        transaction.execute_batch(migration.sql)?;
        transaction.execute(
            "INSERT INTO schema_migrations (version, name, checksum, applied_at)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                migration.version,
                migration.name,
                checksum(migration.sql),
                crate::now_iso(),
            ],
        )?;
        transaction.commit()?;
        count += 1;
    }

    Ok(count)
}

/// Schema version currently applied to this database.
pub fn current_version(connection: &Connection) -> Result<i64> {
    let exists: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='schema_migrations')",
        [],
        |row| row.get(0),
    )?;
    if !exists {
        return Ok(0);
    }
    Ok(connection.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
        [],
        |row| row.get(0),
    )?)
}
