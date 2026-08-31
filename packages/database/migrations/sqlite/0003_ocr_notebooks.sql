-- Notebook import (ADR-037): scanned pages, their segmented word boxes, and
-- the correction memory that learns this writer's hand.
--
-- The scans live inside the encrypted database as BLOBs: the notebooks are
-- the most sensitive artifact this product touches, and a copy on disk next
-- to an encrypted database would defeat the encryption. Backups therefore
-- carry the pages too.
--
-- `ocr_corrections` is the learning: every word the user transcribes is kept
-- as (shape descriptor → text). New word boxes are matched against this
-- memory by cosine similarity — the same writer shapes the same word the
-- same way — so a word corrected once is recognized on every later page.
CREATE TABLE ocr_pages (
  id          TEXT PRIMARY KEY,
  file_name   TEXT NOT NULL,
  image       BLOB NOT NULL,
  width       INTEGER NOT NULL,
  height      INTEGER NOT NULL,
  status      TEXT NOT NULL DEFAULT 'new',
  contact_id  TEXT,
  imported_at TEXT NOT NULL,
  updated_at  TEXT NOT NULL
) STRICT;

CREATE TABLE ocr_tokens (
  id          TEXT PRIMARY KEY,
  page_id     TEXT NOT NULL REFERENCES ocr_pages(id) ON DELETE CASCADE,
  line_index  INTEGER NOT NULL,
  token_index INTEGER NOT NULL,
  x           INTEGER NOT NULL,
  y           INTEGER NOT NULL,
  w           INTEGER NOT NULL,
  h           INTEGER NOT NULL,
  descriptor  BLOB NOT NULL,
  text        TEXT,
  source      TEXT NOT NULL DEFAULT 'none',
  confidence  REAL,
  updated_at  TEXT NOT NULL
) STRICT;

CREATE INDEX idx_ocr_tokens_page ON ocr_tokens(page_id, line_index, token_index);

CREATE TABLE ocr_corrections (
  id         TEXT PRIMARY KEY,
  descriptor BLOB NOT NULL,
  text       TEXT NOT NULL,
  token_id   TEXT,
  created_at TEXT NOT NULL
) STRICT;
