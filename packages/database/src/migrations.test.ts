import { DatabaseSync } from 'node:sqlite';
import { beforeEach, describe, expect, it } from 'vitest';
import {
  CONNECTION_PRAGMAS,
  MIGRATIONS_TABLE_SQL,
  SCHEMA_VERSION,
  SQLITE_MIGRATIONS,
  checksum,
} from './migrations.js';

/**
 * These tests run the production migration files, unmodified, against real
 * SQLite. Node ships SQLite 3.51 with FTS5 and the trigram tokenizer, so the
 * schema the desktop will actually create is exercised on every CI run without
 * a Rust toolchain or a Tauri shell.
 */

function open(): DatabaseSync {
  const db = new DatabaseSync(':memory:');
  for (const pragma of CONNECTION_PRAGMAS) {
    // WAL is meaningless for an in-memory database and SQLite reports it back
    // as `memory`; the rest apply normally.
    if (pragma.includes('journal_mode')) continue;
    db.exec(pragma);
  }
  return db;
}

/** The same forward-only runner the Rust shell implements, in miniature. */
function migrate(db: DatabaseSync): number {
  db.exec(MIGRATIONS_TABLE_SQL);
  const applied = new Map(
    (db.prepare('SELECT version, checksum FROM schema_migrations').all() as Array<{
      version: number;
      checksum: string;
    }>).map((row) => [row.version, row.checksum]),
  );

  let count = 0;
  for (const migration of SQLITE_MIGRATIONS) {
    const existing = applied.get(migration.version);
    if (existing != null) {
      if (existing !== checksum(migration.sql)) {
        throw new Error(
          `Migration ${migration.name} was modified after being applied. ` +
            'Shipped migrations are immutable — add a new one instead.',
        );
      }
      continue;
    }

    db.exec('BEGIN');
    try {
      db.exec(migration.sql);
      db.prepare(
        'INSERT INTO schema_migrations (version, name, checksum, applied_at) VALUES (?, ?, ?, ?)',
      ).run(migration.version, migration.name, checksum(migration.sql), new Date().toISOString());
      db.exec('COMMIT');
      count += 1;
    } catch (error) {
      db.exec('ROLLBACK');
      throw error;
    }
  }
  return count;
}

let db: DatabaseSync;

beforeEach(() => {
  db = open();
});

describe('migrations', () => {
  it('applies cleanly to an empty database', () => {
    expect(migrate(db)).toBe(SQLITE_MIGRATIONS.length);
    const version = (
      db.prepare('SELECT MAX(version) AS v FROM schema_migrations').get() as { v: number }
    ).v;
    expect(version).toBe(SCHEMA_VERSION);
  });

  it('is idempotent — a second run applies nothing', () => {
    migrate(db);
    expect(migrate(db)).toBe(0);
  });

  it('refuses to run when an applied migration was edited afterwards', () => {
    migrate(db);
    db.prepare('UPDATE schema_migrations SET checksum = ? WHERE version = 1').run('tampered');
    expect(() => migrate(db)).toThrow(/modified after being applied/);
  });

  it('creates every entity table named in the product specification', () => {
    migrate(db);
    const tables = new Set(
      (
        db.prepare("SELECT name FROM sqlite_master WHERE type = 'table'").all() as Array<{
          name: string;
        }>
      ).map((row) => row.name),
    );

    for (const expected of [
      'contacts',
      'contact_phones',
      'contact_emails',
      'contact_aliases',
      'contact_specialties',
      'contact_languages',
      'tags',
      'contact_tags',
      'categories',
      'contact_categories',
      'organizations',
      'contact_organizations',
      'relationships',
      'notes',
      'users',
      'devices',
      'audit_log',
      'mutations',
      'sync_cursors',
      'conflicts',
      'saved_searches',
    ]) {
      expect(tables, `missing table ${expected}`).toContain(expected);
    }
  });

  it('enforces foreign keys', () => {
    migrate(db);
    expect(() =>
      db
        .prepare(
          `INSERT INTO contact_phones (id, contact_id, kind, raw, digits, is_primary, created_at, updated_at)
           VALUES ('01ORPHAN0000000000000000AA', '01MISSING000000000000000AA', 'mobile', '05', '05', 0, ?, ?)`,
        )
        .run(new Date().toISOString(), new Date().toISOString()),
    ).toThrow(/FOREIGN KEY/i);
  });

  it('rejects a relationship from a contact to itself', () => {
    migrate(db);
    const now = new Date().toISOString();
    db.prepare(
      `INSERT INTO contacts (id, display_name, created_at, updated_at) VALUES (?, ?, ?, ?)`,
    ).run('01SELF00000000000000000AAA', 'בדיקה', now, now);

    expect(() =>
      db
        .prepare(
          `INSERT INTO relationships (id, from_contact_id, to_contact_id, type, created_at, updated_at)
           VALUES (?, ?, ?, 'knows', ?, ?)`,
        )
        .run('01REL000000000000000000AAA', '01SELF00000000000000000AAA', '01SELF00000000000000000AAA', now, now),
    ).toThrow(/CHECK constraint/i);
  });
});

describe('full-text search infrastructure', () => {
  beforeEach(() => migrate(db));

  it('builds an FTS5 index that matches Hebrew tokens', () => {
    db.prepare(
      `INSERT INTO contact_fts (contact_id, name, profession, city, tags, notes)
       VALUES (?, ?, ?, ?, ?, ?)`,
    ).run('c1', 'ישראל סופר', 'סופר סתמ', 'ירושלימ', 'תפילינ', 'כתב אשכנזי מהודר');

    const rows = db
      .prepare('SELECT contact_id FROM contact_fts WHERE contact_fts MATCH ?')
      .all('"סופר" AND "סתמ"') as Array<{ contact_id: string }>;

    expect(rows).toHaveLength(1);
    expect(rows[0]!.contact_id).toBe('c1');
  });

  it('supports prefix queries for typeahead', () => {
    db.prepare('INSERT INTO contact_fts (contact_id, name) VALUES (?, ?)').run('c1', 'משה כהנ');

    const rows = db
      .prepare('SELECT contact_id FROM contact_fts WHERE contact_fts MATCH ?')
      .all('"מש"*');

    expect(rows).toHaveLength(1);
  });

  it('ranks a short field above the same term buried in a long note', () => {
    const insert = db.prepare(
      'INSERT INTO contact_fts (contact_id, name, notes) VALUES (?, ?, ?)',
    );
    insert.run('exact', 'תפילינ', '');
    insert.run('buried', 'משה כהנ', 'עוסק בעיקר תפילינ ומזוזות וגם ספרי תורה ומגילות אסתר');

    const rows = db
      .prepare(
        'SELECT contact_id FROM contact_fts WHERE contact_fts MATCH ? ORDER BY bm25(contact_fts)',
      )
      .all('"תפילינ"') as Array<{ contact_id: string }>;

    // bm25 divides by field length, so the one-word name outranks the note.
    expect(rows.map((row) => row.contact_id)).toEqual(['exact', 'buried']);
  });

  it('does not match a proclitic-prefixed word without query expansion', () => {
    // This is the whole reason `expandToken` exists. FTS5 tokenizes `בתפילין`
    // as one word, so a search for `תפילין` cannot reach it — the query has to
    // be widened before it is handed to SQLite, and the engine is responsible
    // for doing that. If this test ever starts passing, the tokenizer changed
    // and the expansion logic needs revisiting.
    db.prepare('INSERT INTO contact_fts (contact_id, notes) VALUES (?, ?)').run(
      'c1',
      'עוסק בתפילינ ומזוזות',
    );

    const bare = db
      .prepare('SELECT contact_id FROM contact_fts WHERE contact_fts MATCH ?')
      .all('"תפילינ"');
    expect(bare).toHaveLength(0);

    const expanded = db
      .prepare('SELECT contact_id FROM contact_fts WHERE contact_fts MATCH ?')
      .all('("תפילינ" OR "בתפילינ")');
    expect(expanded).toHaveLength(1);
  });

  it('finds substrings through the trigram index', () => {
    db.prepare('INSERT INTO contact_trigram (contact_id, haystack) VALUES (?, ?)').run(
      'c1',
      'פרידמנ יעקב',
    );

    const rows = db
      .prepare('SELECT contact_id FROM contact_trigram WHERE contact_trigram MATCH ?')
      .all('"רידמ"');

    expect(rows).toHaveLength(1);
  });
});
