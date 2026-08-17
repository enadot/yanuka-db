/**
 * The local SQLite schema, embedded as text.
 *
 * The `.sql` files under `migrations/sqlite` are the single source of truth.
 * The Rust shell reads that directory with `include_dir!` at compile time and
 * this module re-exports the same text for the TypeScript test harness, so the
 * schema exercised by `vitest` is byte-for-byte the schema the desktop ships.
 *
 * Migrations are forward-only. There are no `down` files: a failed upgrade is
 * recovered by restoring the automatic pre-migration backup, which is the only
 * approach that cannot lose a column's worth of data. See docs/DATABASE.md.
 */
import { GENERATED_MIGRATIONS } from './generated/migrations.gen.js';

export interface Migration {
  /** Numeric prefix of the file name; the applied-migrations ledger key. */
  version: number;
  name: string;
  sql: string;
}

export const SQLITE_MIGRATIONS: readonly Migration[] = GENERATED_MIGRATIONS;

/** Highest version this build knows how to produce. */
export const SCHEMA_VERSION = SQLITE_MIGRATIONS[SQLITE_MIGRATIONS.length - 1]!.version;

/**
 * Ledger of applied migrations.
 *
 * Kept out of the migration files themselves because the runner has to create
 * it before it can read it.
 */
export const MIGRATIONS_TABLE_SQL = `
CREATE TABLE IF NOT EXISTS schema_migrations (
  version    INTEGER PRIMARY KEY,
  name       TEXT NOT NULL,
  checksum   TEXT NOT NULL,
  applied_at TEXT NOT NULL
) STRICT;
`;

/**
 * Pragmas applied to every connection.
 *
 * `foreign_keys` is per-connection in SQLite and off by default — forgetting it
 * silently disables every FK in the schema. `temp_store = MEMORY` is set now
 * rather than later because SQLCipher would otherwise spill plaintext
 * temporary files to disk once encryption is enabled (docs/SECURITY.md).
 */
export const CONNECTION_PRAGMAS = [
  'PRAGMA journal_mode = WAL',
  'PRAGMA synchronous = NORMAL',
  'PRAGMA foreign_keys = ON',
  'PRAGMA busy_timeout = 5000',
  'PRAGMA cache_size = -65536',
  'PRAGMA temp_store = MEMORY',
] as const;

/** Cheap, stable checksum used to detect edits to an already-applied migration. */
export function checksum(sql: string): string {
  let h1 = 0x811c9dc5;
  let h2 = 0x01000193;
  for (let i = 0; i < sql.length; i += 1) {
    const code = sql.charCodeAt(i);
    h1 = Math.imul(h1 ^ code, 0x01000193) >>> 0;
    h2 = Math.imul(h2 + code, 0x85ebca6b) >>> 0;
  }
  return `${h1.toString(16).padStart(8, '0')}${h2.toString(16).padStart(8, '0')}`;
}
