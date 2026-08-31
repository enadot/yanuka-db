-- Semantic index: one embedding per searchable document — a contact's profile
-- or a single note. Maintained by reconciliation (crates/yanuka-db/src/
-- semantic.rs), never by triggers: producing a vector requires the embedding
-- model, which only the desktop shell holds.
--
-- `vector` is little-endian f32, `model` names the embedding model that
-- produced it so a future model swap invalidates every row by comparison, and
-- `source_hash` is the checksum of the embedded text — the reconciler re-embeds
-- a document only when its text actually changed.
CREATE TABLE semantic_index (
  doc_id      TEXT PRIMARY KEY,
  contact_id  TEXT NOT NULL,
  model       TEXT NOT NULL,
  source_hash TEXT NOT NULL,
  vector      BLOB NOT NULL,
  indexed_at  TEXT NOT NULL
) STRICT;

CREATE INDEX idx_semantic_index_contact ON semantic_index(contact_id);
