# DATABASE

Source of truth: `packages/database/migrations/sqlite/*.sql`.
Tested by `packages/database/src/migrations.test.ts` (Node's `node:sqlite`) and
`crates/yanuka-db/tests/repository.rs` (rusqlite). Both run the same files.

## Conventions

| | |
|---|---|
| Primary keys | ULID `TEXT`, client-generated. Never auto-increment — ADR-004 |
| Timestamps | ISO-8601 UTC `TEXT`. Sorts correctly, survives a dump, casts to `timestamptz` |
| Booleans | `INTEGER` 0/1 with a `CHECK` |
| Deletion | `deleted_at IS NULL` means live. Every read path filters on it |
| Tables | `STRICT`, so a type error is an error and not a silent coercion |
| Names | snake_case in SQL, camelCase in TypeScript, mapped in one place |

## ERD

```
                      ┌───────────┐
              ┌───────│  contacts │───────┐
              │       └─────┬─────┘       │
              │             │             │
   ┌──────────┴───┐  ┌──────┴──────┐  ┌───┴──────────┐
   │contact_phones│  │contact_emails│  │contact_aliases│
   └──────────────┘  └─────────────┘  └──────────────┘
              │             │             │
   ┌──────────┴───────┐  ┌──┴───────────┐ │
   │contact_specialties│ │contact_languages│
   └──────────────────┘  └──────────────┘ │
                                          │
   ┌────────┐   ┌──────────────┐          │
   │  tags  │───│ contact_tags │──────────┤
   └────────┘   └──────────────┘          │
   ┌──────────┐ ┌───────────────────┐     │
   │categories│─│contact_categories │─────┤
   └──────────┘ └───────────────────┘     │
   ┌─────────────┐ ┌──────────────────────┴┐
   │organizations│─│ contact_organizations │
   └─────────────┘ └───────────────────────┘
                                          │
   ┌──────────────┐                       │
   │relationships │  from ──┐   ┌── to ───┤   directed edges, contact↔contact
   └──────────────┘         └───┴─────────┤
   ┌───────┐                              │
   │ notes │──────────────────────────────┘   timestamped, may be sensitive
   └───────┘

   sync:   mutations · sync_cursors · conflicts · devices
   admin:  users · audit_log · saved_searches · app_meta
   search: contact_fts (FTS5) · contact_trigram (FTS5 trigram)
```

## Why the model is not one flat table

The brief is explicit, and the reason holds up under use:

- **Specialties are rows, not a JSON array.** `מזוזות 8, תפילין 7` in the facet
  panel is a `GROUP BY`. With a JSON blob it is a full scan and a parse.
- **Tags are an entity, not a comma-separated string.** They are renamed,
  coloured, counted and filtered on.
- **Aliases are rows.** `ר' משה כהן`, `הרב משה כהן`, `Moshe Cohen` and
  `Moishe Cohen` are one person, and each spelling must be independently
  searchable.
- **Organizations are an entity.** A person belongs to several; an institution
  outlives any one contact.
- **Relationships are directed edges** with an inverse defined in
  `RELATIONSHIP_INVERSES`, so an edge is stored once and rendered from either
  end.

## Phone numbers

Four columns, and each earns its place:

| column | why |
|---|---|
| `raw` | exactly as typed. `02-6521234 שלוחה 4` and `בבית של אדלר` are real notebook entries and often the only clue to whose number it is |
| `e164` | nullable. A number that will not parse is still worth keeping |
| `digits` | digits only. Search matches on a **suffix** of this, which is what makes format irrelevant |
| `country_code` | nullable even when parsing succeeded — reserved ranges parse to valid E.164 with no assignable country |

## Search tables

```sql
CREATE VIRTUAL TABLE contact_fts USING fts5(
  contact_id UNINDEXED, name, aliases, profession, role, specialties,
  organization, city, country, tags, categories, notes, reason_for_saving,
  tokenize = "unicode61 remove_diacritics 2"
);

CREATE VIRTUAL TABLE contact_trigram USING fts5(
  contact_id UNINDEXED, haystack, tokenize = "trigram remove_diacritics 1"
);
```

`contact_fts` is **not** an external-content table. The document aggregates
eight tables, so there is no single row to point at.

Maintained by `yanuka_db::index::reindex_contact`, called at the end of every
mutating operation **inside the same transaction**. Not by SQL triggers: a
trigger on `contact_tags` has no view of the contact's aliases, none of them can
call the Hebrew normalizer, and covering the eight source tables would take
roughly sixteen triggers that cannot be unit-tested.

`contact_trigram` indexes names, aliases and phone digits only. Trigram-indexing
free text would multiply the index size for a layer whose only job is rescuing a
misspelled *name*.

## Migrations

**Forward-only. There are no `down` files.**

A hand-written reverse migration is exactly the kind of code that silently drops
a column's worth of data, and it is run precisely when the user is already in
trouble. The recovery path is the automatic pre-migration backup instead
(`apps/desktop/src-tauri/src/backup.rs`, three kept, WAL and SHM included).

The runner:

1. Sorts by numeric prefix and skips what is already applied.
2. **Verifies the checksum of every applied migration** and hard-errors on a
   mismatch. An edited migration means the database in front of us was built by
   different SQL than this binary expects; continuing would leave the schema in
   a state nobody has tested.
3. Runs each migration and its ledger row in one transaction.

Adding one: write `000N_name.sql`, and never edit a shipped file.

## Pragmas

Applied on every connection, in `CONNECTION_PRAGMAS` (TS) and `PRAGMAS` (Rust):

```sql
PRAGMA journal_mode = WAL;      -- readers do not block the writer
PRAGMA synchronous  = NORMAL;   -- safe under WAL, much faster than FULL
PRAGMA foreign_keys = ON;       -- per-connection and OFF by default in SQLite
PRAGMA busy_timeout = 5000;
PRAGMA cache_size   = -65536;   -- 64 MiB
PRAGMA temp_store   = MEMORY;   -- set now: SQLCipher would spill plaintext temp files
```

`foreign_keys` is the one that bites. It is per-connection, off by default, and
forgetting it silently disables every foreign key in the schema.

## Postgres, when the server arrives

A second directory, `migrations/postgres`, authored against the same manifest.
The mapping is mechanical:

| SQLite | Postgres |
|---|---|
| `TEXT` ULID | `text` (ULIDs are not UUIDs; keep them as text) |
| `TEXT` ISO-8601 | `timestamptz` |
| `INTEGER` 0/1 | `boolean` |
| `STRICT` | native typing |
| `contact_fts` | `tsvector` + GIN |
| `contact_trigram` | `pg_trgm` GIN index; the table disappears |
| `TEXT` JSON payload | `jsonb` |

Search is the only place the dialects genuinely diverge, and that divergence is
contained inside the repository implementation.

## Index sizing at 100,000 contacts

Rough, and worth knowing before it surprises someone:

| | approx |
|---|---|
| `contacts` + children | 120–180 MB |
| `contact_fts` | 60–90 MB |
| `contact_trigram` (names/aliases/phones only) | 30–50 MB |

If the trigram index ever becomes a problem it can be dropped and the fuzzy
layer disabled, degrading to exact + full-text search, without a schema change.
