-- ---------------------------------------------------------------------------
-- Smart categories (ADR-038)
--
-- A category was a bare name. It becomes a shelf with a face (icon, colour,
-- position) and, optionally, a rule: who belongs automatically. Rule matches
-- are cached in `category_matches`, maintained by the storage layer alongside
-- the search index — never by triggers, because a rule reads joined tables
-- and the Hebrew normalizer, which SQL cannot. Manual membership keeps its
-- table and gains a mode so one person can be pinned in, or kept out even
-- when the rule says otherwise.
-- ---------------------------------------------------------------------------

ALTER TABLE categories ADD COLUMN icon TEXT;
ALTER TABLE categories ADD COLUMN color TEXT;
-- Serialized CategoryRule from @yanuka/types; NULL for a hand-filled category.
ALTER TABLE categories ADD COLUMN rule TEXT;
ALTER TABLE categories ADD COLUMN sort_order INTEGER NOT NULL DEFAULT 0;
ALTER TABLE categories ADD COLUMN show_on_home INTEGER NOT NULL DEFAULT 1
  CHECK (show_on_home IN (0, 1));

ALTER TABLE contact_categories ADD COLUMN mode TEXT NOT NULL DEFAULT 'include'
  CHECK (mode IN ('include', 'exclude'));

-- Derived, rebuildable, never synced: which contacts each rule selects now.
CREATE TABLE category_matches (
  category_id TEXT NOT NULL REFERENCES categories (id) ON DELETE CASCADE,
  contact_id  TEXT NOT NULL REFERENCES contacts (id) ON DELETE CASCADE,
  PRIMARY KEY (category_id, contact_id)
) STRICT, WITHOUT ROWID;

CREATE INDEX category_matches_contact ON category_matches (contact_id);

-- Effective membership, the one definition every reader shares: a rule match
-- or a manual pin, unless the contact was explicitly excluded. `membership`
-- tells the card whether the person is here by rule or by hand.
CREATE VIEW category_members AS
  SELECT cat.id AS category_id,
         c.id   AS contact_id,
         CASE WHEN cc.id IS NOT NULL THEN 'manual' ELSE 'rule' END AS membership
    FROM categories cat
    JOIN contacts c ON c.deleted_at IS NULL
    LEFT JOIN category_matches m
           ON m.category_id = cat.id AND m.contact_id = c.id
    LEFT JOIN contact_categories cc
           ON cc.category_id = cat.id AND cc.contact_id = c.id AND cc.deleted_at IS NULL
   WHERE cat.deleted_at IS NULL
     AND (m.contact_id IS NOT NULL OR (cc.id IS NOT NULL AND cc.mode = 'include'))
     AND NOT (cc.id IS NOT NULL AND cc.mode = 'exclude');
