-- ============================================================================
-- 0001_initial_schema
--
-- Initial SQLite schema for the local, offline-first contacts database.
--
-- Conventions
--   * Table and column names are snake_case; the TypeScript layer maps them to
--     camelCase in packages/database/src/mapping.ts.
--   * Every user-visible entity carries the sync envelope: created_at,
--     updated_at, created_by, updated_by, version, device_id, deleted_at.
--   * Primary keys are ULID TEXT. Auto-increment is forbidden — every device
--     mints IDs offline (docs/DECISIONS.md ADR-004).
--   * Timestamps are ISO-8601 UTC strings. SQLite has no date type and TEXT
--     sorts correctly for this format.
--   * Booleans are INTEGER 0/1 with a CHECK constraint.
--   * deleted_at IS NULL means "live". Every read path must filter on it.
--
-- This file is the single source of truth for the local schema. The Rust shell
-- embeds it with include_str! and the TypeScript mock adapter parses the same
-- text, so the two can never drift.
-- ============================================================================

-- ---------------------------------------------------------------------------
-- Meta
-- ---------------------------------------------------------------------------

-- Small key/value store for installation-scoped state: device id, schema
-- provenance, last successful sync, backup bookkeeping.
CREATE TABLE app_meta (
  key        TEXT PRIMARY KEY,
  value      TEXT,
  updated_at TEXT NOT NULL
) STRICT;

-- ---------------------------------------------------------------------------
-- Identity
-- ---------------------------------------------------------------------------

CREATE TABLE users (
  id                  TEXT PRIMARY KEY,
  email               TEXT NOT NULL,
  display_name        TEXT NOT NULL,
  role                TEXT NOT NULL CHECK (
                        role IN ('super_admin', 'admin', 'editor', 'viewer', 'restricted_viewer')
                      ),
  -- JSON arrays of permission strings layered on top of the role defaults.
  -- Denied wins over granted; see packages/core/src/permissions.ts.
  extra_permissions   TEXT NOT NULL DEFAULT '[]',
  denied_permissions  TEXT NOT NULL DEFAULT '[]',
  is_active           INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
  last_login_at       TEXT,

  created_at          TEXT NOT NULL,
  updated_at          TEXT NOT NULL,
  created_by          TEXT,
  updated_by          TEXT,
  version             INTEGER NOT NULL DEFAULT 1,
  device_id           TEXT,
  deleted_at          TEXT
) STRICT;

CREATE UNIQUE INDEX users_email_unique ON users (email) WHERE deleted_at IS NULL;

CREATE TABLE devices (
  id           TEXT PRIMARY KEY,
  name         TEXT NOT NULL,
  type         TEXT NOT NULL CHECK (type IN ('desktop', 'android', 'ios', 'web')),
  platform     TEXT,
  app_version  TEXT,
  last_seen_at TEXT,
  last_sync_at TEXT,
  revoked_at   TEXT,
  created_at   TEXT NOT NULL
) STRICT;

-- ---------------------------------------------------------------------------
-- Contacts
-- ---------------------------------------------------------------------------

CREATE TABLE contacts (
  id                       TEXT PRIMARY KEY,

  first_name               TEXT,
  last_name                TEXT,
  display_name             TEXT NOT NULL,
  prefix                   TEXT,
  title                    TEXT,

  -- Normalized display name, produced by the Hebrew-aware normalizer in
  -- @yanuka/search. Written on every save so exact lookups never have to
  -- normalize at query time.
  normalized_name          TEXT NOT NULL DEFAULT '',

  country                  TEXT,
  region                   TEXT,
  city                     TEXT,
  address                  TEXT,
  postal_code              TEXT,
  normalized_city          TEXT,

  profession               TEXT,
  role                     TEXT,
  normalized_profession    TEXT,

  notes                    TEXT,
  reason_for_saving        TEXT,
  source                   TEXT,
  introduced_by            TEXT,
  introduced_by_contact_id TEXT REFERENCES contacts (id) ON DELETE SET NULL,

  is_favorite              INTEGER NOT NULL DEFAULT 0 CHECK (is_favorite IN (0, 1)),
  last_viewed_at           TEXT,

  created_at               TEXT NOT NULL,
  updated_at               TEXT NOT NULL,
  created_by               TEXT,
  updated_by               TEXT,
  version                  INTEGER NOT NULL DEFAULT 1,
  device_id                TEXT,
  deleted_at               TEXT
) STRICT;

CREATE INDEX contacts_normalized_name ON contacts (normalized_name) WHERE deleted_at IS NULL;
CREATE INDEX contacts_last_name       ON contacts (last_name)       WHERE deleted_at IS NULL;
CREATE INDEX contacts_country_city    ON contacts (country, city)   WHERE deleted_at IS NULL;
CREATE INDEX contacts_profession      ON contacts (normalized_profession) WHERE deleted_at IS NULL;
CREATE INDEX contacts_updated_at      ON contacts (updated_at)      WHERE deleted_at IS NULL;
CREATE INDEX contacts_favorite        ON contacts (is_favorite, display_name) WHERE deleted_at IS NULL;
CREATE INDEX contacts_last_viewed     ON contacts (last_viewed_at)  WHERE deleted_at IS NULL;
-- Keyset pagination for the contact list orders by (display_name, id).
CREATE INDEX contacts_name_keyset     ON contacts (display_name, id) WHERE deleted_at IS NULL;

CREATE TABLE contact_phones (
  id           TEXT PRIMARY KEY,
  contact_id   TEXT NOT NULL REFERENCES contacts (id) ON DELETE CASCADE,
  kind         TEXT NOT NULL CHECK (
                 kind IN ('mobile', 'office', 'home', 'whatsapp', 'fax', 'assistant', 'other')
               ),
  -- Exactly as typed. Never rewritten: the original entry is evidence about
  -- where the number came from.
  raw          TEXT NOT NULL,
  -- E.164 when the number could be parsed, else NULL.
  e164         TEXT,
  -- Digits only. Search matches on a suffix of this so that 054-123-4567,
  -- +972541234567 and 0541234567 all find the same row.
  digits       TEXT NOT NULL DEFAULT '',
  country_code TEXT,
  is_primary   INTEGER NOT NULL DEFAULT 0 CHECK (is_primary IN (0, 1)),
  label        TEXT,

  created_at   TEXT NOT NULL,
  updated_at   TEXT NOT NULL,
  created_by   TEXT,
  updated_by   TEXT,
  version      INTEGER NOT NULL DEFAULT 1,
  device_id    TEXT,
  deleted_at   TEXT
) STRICT;

CREATE INDEX contact_phones_contact ON contact_phones (contact_id) WHERE deleted_at IS NULL;
CREATE INDEX contact_phones_digits  ON contact_phones (digits)     WHERE deleted_at IS NULL;
CREATE INDEX contact_phones_e164    ON contact_phones (e164)       WHERE deleted_at IS NULL;

CREATE TABLE contact_emails (
  id         TEXT PRIMARY KEY,
  contact_id TEXT NOT NULL REFERENCES contacts (id) ON DELETE CASCADE,
  kind       TEXT NOT NULL CHECK (kind IN ('personal', 'work', 'other')),
  address    TEXT NOT NULL,
  normalized TEXT NOT NULL,
  is_primary INTEGER NOT NULL DEFAULT 0 CHECK (is_primary IN (0, 1)),

  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  created_by TEXT,
  updated_by TEXT,
  version    INTEGER NOT NULL DEFAULT 1,
  device_id  TEXT,
  deleted_at TEXT
) STRICT;

CREATE INDEX contact_emails_contact    ON contact_emails (contact_id) WHERE deleted_at IS NULL;
CREATE INDEX contact_emails_normalized ON contact_emails (normalized) WHERE deleted_at IS NULL;

-- Alternate spellings and transliterations. `ר' משה כהן`, `הרב משה כהן`,
-- `Moshe Cohen` and `Moishe Cohen` are one person; searching any of them must
-- reach that person. See docs/SEARCH.md.
CREATE TABLE contact_aliases (
  id            TEXT PRIMARY KEY,
  contact_id    TEXT NOT NULL REFERENCES contacts (id) ON DELETE CASCADE,
  kind          TEXT NOT NULL CHECK (
                  kind IN ('alias', 'nickname', 'maiden', 'transliteration', 'formal', 'former', 'other')
                ),
  value         TEXT NOT NULL,
  normalized    TEXT NOT NULL,
  language_code TEXT,

  created_at    TEXT NOT NULL,
  updated_at    TEXT NOT NULL,
  created_by    TEXT,
  updated_by    TEXT,
  version       INTEGER NOT NULL DEFAULT 1,
  device_id     TEXT,
  deleted_at    TEXT
) STRICT;

CREATE INDEX contact_aliases_contact    ON contact_aliases (contact_id) WHERE deleted_at IS NULL;
CREATE INDEX contact_aliases_normalized ON contact_aliases (normalized) WHERE deleted_at IS NULL;

-- Specialties are their own rows rather than a JSON blob so that facet counts
-- ("מזוזות 8, תפילין 7") are a plain GROUP BY.
CREATE TABLE contact_specialties (
  id         TEXT PRIMARY KEY,
  contact_id TEXT NOT NULL REFERENCES contacts (id) ON DELETE CASCADE,
  value      TEXT NOT NULL,
  normalized TEXT NOT NULL,
  created_at TEXT NOT NULL,
  deleted_at TEXT
) STRICT;

CREATE INDEX contact_specialties_contact    ON contact_specialties (contact_id) WHERE deleted_at IS NULL;
CREATE INDEX contact_specialties_normalized ON contact_specialties (normalized) WHERE deleted_at IS NULL;

CREATE TABLE contact_languages (
  id            TEXT PRIMARY KEY,
  contact_id    TEXT NOT NULL REFERENCES contacts (id) ON DELETE CASCADE,
  language_code TEXT NOT NULL,
  created_at    TEXT NOT NULL,
  deleted_at    TEXT
) STRICT;

CREATE INDEX contact_languages_contact ON contact_languages (contact_id) WHERE deleted_at IS NULL;

-- ---------------------------------------------------------------------------
-- Tags and categories
-- ---------------------------------------------------------------------------

CREATE TABLE tags (
  id          TEXT PRIMARY KEY,
  name        TEXT NOT NULL,
  normalized  TEXT NOT NULL,
  color       TEXT,
  description TEXT,

  created_at  TEXT NOT NULL,
  updated_at  TEXT NOT NULL,
  created_by  TEXT,
  updated_by  TEXT,
  version     INTEGER NOT NULL DEFAULT 1,
  device_id   TEXT,
  deleted_at  TEXT
) STRICT;

CREATE UNIQUE INDEX tags_normalized_unique ON tags (normalized) WHERE deleted_at IS NULL;

CREATE TABLE contact_tags (
  id         TEXT PRIMARY KEY,
  contact_id TEXT NOT NULL REFERENCES contacts (id) ON DELETE CASCADE,
  tag_id     TEXT NOT NULL REFERENCES tags (id) ON DELETE CASCADE,

  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  created_by TEXT,
  updated_by TEXT,
  version    INTEGER NOT NULL DEFAULT 1,
  device_id  TEXT,
  deleted_at TEXT
) STRICT;

CREATE UNIQUE INDEX contact_tags_unique ON contact_tags (contact_id, tag_id) WHERE deleted_at IS NULL;
CREATE INDEX contact_tags_tag ON contact_tags (tag_id) WHERE deleted_at IS NULL;

CREATE TABLE categories (
  id          TEXT PRIMARY KEY,
  name        TEXT NOT NULL,
  normalized  TEXT NOT NULL,
  description TEXT,
  parent_id   TEXT REFERENCES categories (id) ON DELETE SET NULL,

  created_at  TEXT NOT NULL,
  updated_at  TEXT NOT NULL,
  created_by  TEXT,
  updated_by  TEXT,
  version     INTEGER NOT NULL DEFAULT 1,
  device_id   TEXT,
  deleted_at  TEXT
) STRICT;

CREATE UNIQUE INDEX categories_normalized_unique ON categories (normalized) WHERE deleted_at IS NULL;

CREATE TABLE contact_categories (
  id          TEXT PRIMARY KEY,
  contact_id  TEXT NOT NULL REFERENCES contacts (id) ON DELETE CASCADE,
  category_id TEXT NOT NULL REFERENCES categories (id) ON DELETE CASCADE,

  created_at  TEXT NOT NULL,
  updated_at  TEXT NOT NULL,
  created_by  TEXT,
  updated_by  TEXT,
  version     INTEGER NOT NULL DEFAULT 1,
  device_id   TEXT,
  deleted_at  TEXT
) STRICT;

CREATE UNIQUE INDEX contact_categories_unique
  ON contact_categories (contact_id, category_id) WHERE deleted_at IS NULL;
CREATE INDEX contact_categories_category ON contact_categories (category_id) WHERE deleted_at IS NULL;

-- ---------------------------------------------------------------------------
-- Organizations
-- ---------------------------------------------------------------------------

CREATE TABLE organizations (
  id         TEXT PRIMARY KEY,
  name       TEXT NOT NULL,
  normalized TEXT NOT NULL,
  kind       TEXT NOT NULL CHECK (
               kind IN ('organization', 'institution', 'community', 'synagogue',
                        'business', 'yeshiva', 'kollel', 'charity', 'other')
             ),
  city       TEXT,
  region     TEXT,
  country    TEXT,
  address    TEXT,
  notes      TEXT,

  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  created_by TEXT,
  updated_by TEXT,
  version    INTEGER NOT NULL DEFAULT 1,
  device_id  TEXT,
  deleted_at TEXT
) STRICT;

CREATE INDEX organizations_normalized ON organizations (normalized) WHERE deleted_at IS NULL;

CREATE TABLE contact_organizations (
  id              TEXT PRIMARY KEY,
  contact_id      TEXT NOT NULL REFERENCES contacts (id) ON DELETE CASCADE,
  organization_id TEXT NOT NULL REFERENCES organizations (id) ON DELETE CASCADE,
  role            TEXT,
  is_primary      INTEGER NOT NULL DEFAULT 0 CHECK (is_primary IN (0, 1)),
  started_at      TEXT,
  ended_at        TEXT,

  created_at      TEXT NOT NULL,
  updated_at      TEXT NOT NULL,
  created_by      TEXT,
  updated_by      TEXT,
  version         INTEGER NOT NULL DEFAULT 1,
  device_id       TEXT,
  deleted_at      TEXT
) STRICT;

CREATE UNIQUE INDEX contact_organizations_unique
  ON contact_organizations (contact_id, organization_id) WHERE deleted_at IS NULL;
CREATE INDEX contact_organizations_org
  ON contact_organizations (organization_id) WHERE deleted_at IS NULL;

-- ---------------------------------------------------------------------------
-- Relationships between people
-- ---------------------------------------------------------------------------

-- Directed edges. The inverse of each type is defined in @yanuka/types
-- (RELATIONSHIP_INVERSES) so an edge can be rendered from either endpoint
-- without storing it twice.
CREATE TABLE relationships (
  id              TEXT PRIMARY KEY,
  from_contact_id TEXT NOT NULL REFERENCES contacts (id) ON DELETE CASCADE,
  to_contact_id   TEXT NOT NULL REFERENCES contacts (id) ON DELETE CASCADE,
  type            TEXT NOT NULL CHECK (
                    type IN ('recommended', 'knows', 'related_to', 'works_with', 'family_of',
                             'referred_us_to', 'member_of', 'student_of', 'teacher_of')
                  ),
  notes           TEXT,

  created_at      TEXT NOT NULL,
  updated_at      TEXT NOT NULL,
  created_by      TEXT,
  updated_by      TEXT,
  version         INTEGER NOT NULL DEFAULT 1,
  device_id       TEXT,
  deleted_at      TEXT,

  CHECK (from_contact_id <> to_contact_id)
) STRICT;

CREATE UNIQUE INDEX relationships_unique
  ON relationships (from_contact_id, to_contact_id, type) WHERE deleted_at IS NULL;
CREATE INDEX relationships_from ON relationships (from_contact_id) WHERE deleted_at IS NULL;
CREATE INDEX relationships_to   ON relationships (to_contact_id)   WHERE deleted_at IS NULL;

-- ---------------------------------------------------------------------------
-- Notes
-- ---------------------------------------------------------------------------

-- Timestamped notes, separate from contacts.notes (which is the single always-
-- visible remark). Sensitive notes require the contacts:view_sensitive
-- permission — see docs/SECURITY.md.
CREATE TABLE notes (
  id           TEXT PRIMARY KEY,
  contact_id   TEXT NOT NULL REFERENCES contacts (id) ON DELETE CASCADE,
  body         TEXT NOT NULL,
  is_sensitive INTEGER NOT NULL DEFAULT 0 CHECK (is_sensitive IN (0, 1)),
  author_id    TEXT,

  created_at   TEXT NOT NULL,
  updated_at   TEXT NOT NULL,
  created_by   TEXT,
  updated_by   TEXT,
  version      INTEGER NOT NULL DEFAULT 1,
  device_id    TEXT,
  deleted_at   TEXT
) STRICT;

CREATE INDEX notes_contact ON notes (contact_id, created_at) WHERE deleted_at IS NULL;

-- ---------------------------------------------------------------------------
-- Saved searches
-- ---------------------------------------------------------------------------

CREATE TABLE saved_searches (
  id         TEXT PRIMARY KEY,
  name       TEXT NOT NULL,
  -- Serialized SearchQuery from @yanuka/types.
  query      TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  created_by TEXT,
  deleted_at TEXT
) STRICT;

-- ---------------------------------------------------------------------------
-- Sync
-- ---------------------------------------------------------------------------

-- Every local write appends a row here before it is acknowledged by a server.
-- `payload` holds only changed fields, which is what makes field-level merges
-- possible when two devices edit different parts of the same contact.
CREATE TABLE mutations (
  id           TEXT PRIMARY KEY,
  entity_type  TEXT NOT NULL,
  entity_id    TEXT NOT NULL,
  operation    TEXT NOT NULL CHECK (operation IN ('create', 'update', 'delete')),
  payload      TEXT,
  previous     TEXT,
  base_version INTEGER NOT NULL DEFAULT 0,
  created_at   TEXT NOT NULL,
  device_id    TEXT NOT NULL,
  user_id      TEXT,
  status       TEXT NOT NULL DEFAULT 'pending' CHECK (
                 status IN ('pending', 'syncing', 'synced', 'failed', 'conflict')
               ),
  attempts     INTEGER NOT NULL DEFAULT 0,
  last_error   TEXT,
  synced_at    TEXT
) STRICT;

-- The push loop drains in creation order; this index keeps that scan cheap.
CREATE INDEX mutations_pending ON mutations (status, created_at);
CREATE INDEX mutations_entity  ON mutations (entity_type, entity_id);

CREATE TABLE sync_cursors (
  id             TEXT PRIMARY KEY,
  entity_type    TEXT NOT NULL,
  cursor         TEXT,
  last_pulled_at TEXT,
  last_pushed_at TEXT
) STRICT;

CREATE UNIQUE INDEX sync_cursors_entity ON sync_cursors (entity_type);

-- Unresolved concurrent edits. Both values are retained until a human chooses.
-- A temporary duplicate is always preferable to losing a keystroke.
CREATE TABLE conflicts (
  id          TEXT PRIMARY KEY,
  entity_type TEXT NOT NULL,
  entity_id   TEXT NOT NULL,
  -- JSON array of FieldConflict from @yanuka/types.
  fields      TEXT NOT NULL,
  detected_at TEXT NOT NULL,
  resolved_at TEXT,
  resolution  TEXT CHECK (resolution IS NULL OR resolution IN ('local', 'remote', 'manual'))
) STRICT;

CREATE INDEX conflicts_open ON conflicts (resolved_at, detected_at);

-- ---------------------------------------------------------------------------
-- Audit
-- ---------------------------------------------------------------------------

-- Append-only. Rows are never updated or deleted by application code.
CREATE TABLE audit_log (
  id                TEXT PRIMARY KEY,
  user_id           TEXT,
  user_display_name TEXT,
  action            TEXT NOT NULL,
  entity_type       TEXT NOT NULL,
  entity_id         TEXT,
  entity_label      TEXT,
  changes           TEXT,
  device_id         TEXT,
  device_name       TEXT,
  created_at        TEXT NOT NULL
) STRICT;

CREATE INDEX audit_log_entity  ON audit_log (entity_type, entity_id, created_at);
CREATE INDEX audit_log_created ON audit_log (created_at);

-- ---------------------------------------------------------------------------
-- Full-text search
-- ---------------------------------------------------------------------------

-- Token search over a flattened, pre-normalized document per contact.
--
-- Not an external-content table: the document aggregates seven tables
-- (aliases, tags, specialties, organizations, …), so there is no single row to
-- point at. The application rebuilds a contact's row after any write touching
-- it or its children — see rebuild_contact_index() in src-tauri/src/search.rs.
--
-- remove_diacritics 2 strips Hebrew niqqud at the tokenizer level as a second
-- line of defence; the text is already normalized before insertion.
CREATE VIRTUAL TABLE contact_fts USING fts5 (
  contact_id UNINDEXED,
  name,
  aliases,
  profession,
  role,
  specialties,
  organization,
  city,
  country,
  tags,
  categories,
  notes,
  reason_for_saving,
  tokenize = "unicode61 remove_diacritics 2"
);

-- Substring and fuzzy candidate generation. The trigram tokenizer makes
-- `LIKE '%…%'`-style matching indexable, which is what lets a misspelling such
-- as `פרידמאן` retrieve `פרידמן` as a candidate before edit distance ranks it.
-- Requires SQLite >= 3.34, which the bundled build satisfies.
CREATE VIRTUAL TABLE contact_trigram USING fts5 (
  contact_id UNINDEXED,
  haystack,
  tokenize = "trigram remove_diacritics 1"
);

-- ---------------------------------------------------------------------------
-- Seed rows
-- ---------------------------------------------------------------------------

INSERT INTO sync_cursors (id, entity_type, cursor, last_pulled_at, last_pushed_at)
VALUES ('00000000000000000000000000', 'all', NULL, NULL, NULL);
