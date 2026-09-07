-- ---------------------------------------------------------------------------
-- Sync (ADR-039)
--
-- The mutation log (0001) records what happened on this device. Syncing it
-- needs three more things: what the server holds for each record
-- (`sync_revisions`), the server's own ordered stream of accepted changes
-- (`sync_log` — populated only on the peer that acts as the server), and a way
-- for devices to prove who they are (`devices.token_hash`, `pair_codes`).
--
-- Every table here is bookkeeping about records, never the records
-- themselves: dropping them loses nothing but the ability to sync, and a
-- fresh pairing rebuilds them.
-- ---------------------------------------------------------------------------

-- The server's version of each record as last agreed with this device. A
-- record without a row was never synced; a push carries this as the base the
-- change was computed against, and the server refuses a base it has moved
-- past — which is what turns a concurrent edit into a merge instead of an
-- overwrite (docs/SYNC.md).
CREATE TABLE sync_revisions (
  entity_type    TEXT    NOT NULL,
  entity_id      TEXT    NOT NULL,
  server_version INTEGER NOT NULL,
  synced_at      TEXT    NOT NULL,
  PRIMARY KEY (entity_type, entity_id)
) STRICT, WITHOUT ROWID;

-- The server's stream. `seq` is the cursor devices pull by; `changed` lists
-- the fields the pushing device touched (a JSON array), `state` is the whole
-- record after the change (JSON), which is what a device that has never seen
-- the record needs and what a device that has merges field by field.
CREATE TABLE sync_log (
  seq         INTEGER PRIMARY KEY AUTOINCREMENT,
  entity_type TEXT    NOT NULL,
  entity_id   TEXT    NOT NULL,
  version     INTEGER NOT NULL,
  changed     TEXT    NOT NULL,
  state       TEXT    NOT NULL,
  device_id   TEXT    NOT NULL,
  created_at  TEXT    NOT NULL,
  received_at TEXT    NOT NULL
) STRICT;

CREATE INDEX sync_log_entity ON sync_log (entity_type, entity_id, version);

-- A device authenticates with a bearer token minted at pairing. Only its
-- SHA-256 is stored: the database, its backups and this table can leak
-- without leaking the ability to sync.
ALTER TABLE devices ADD COLUMN token_hash TEXT;

-- One-time pairing codes, short-lived, shown to a person and typed on the
-- new device. Hashed for the same reason as tokens.
CREATE TABLE pair_codes (
  code_hash  TEXT PRIMARY KEY,
  created_at TEXT NOT NULL,
  expires_at TEXT NOT NULL,
  used_at    TEXT,
  used_by    TEXT
) STRICT;
