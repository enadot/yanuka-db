# SYNC

**Status: local recording implemented, transport not.** The mutation log,
device registry, cursors and conflict tables ship, and every local change now
appends a mutation carrying the change itself (ADR-033 — for a long time it
carried only a display name, which would have made sync appear to work while
delivering nothing). There is no network code, and no apply path yet. See
ADR-019.

That split is intentional. The expensive-to-change part is the *record* of what
happened locally, and getting it wrong later means the changes made before the
sync engine existed are unrecoverable. The transport is comparatively easy to
add and easy to change.

## The model

The server is **not** a live database the desktop reads through. It is a peer
that happens to be always on.

```
Desktop SQLite ──→ mutation log ──→ sync engine ──→ API ──→ PostgreSQL
      ▲                                                        │
      └────────────────── incremental pull ────────────────────┘
```

Every device can create data offline. No device is authoritative. The server
does not win by virtue of being the server — it holds whatever it was last told,
which may be older than what is on the laptop that has been off the network for
three weeks.

## The mutation log

`mutations`, written inside the same transaction as the change itself. Both
halves matter:

- A change made offline is **durable before anything tries to send it**.
- `payload` holds **only the fields that changed**, not the whole record. That
  is what makes field-level merging possible.

```
id · entity_type · entity_id · operation(create|update|delete)
payload · previous · base_version · created_at · device_id · user_id
status(pending|syncing|synced|failed|conflict) · attempts · last_error
```

What each operation carries:

- **create** — the whole record, including its child collections. Nothing else
  exists for a replaying device to build the contact from.
- **update** — the changed fields only, compared against the *stored* record
  rather than against the patch. A patch that re-sends an unchanged field is not
  an edit; logging it as one manufactures conflicts on other devices.
- **merge** — a field-level diff of what moved onto the surviving contact, plus
  `mergedFrom`. Not just the operation name: re-deriving the merge remotely only
  agrees if that device holds byte-identical copies of both originals.
- **delete** — no payload. The tombstone is the `deleted_at` update.

A **child collection counts as one field**. If any phone changes, the whole
phone list is in the payload, because `write_children` replaces each collection
wholesale — the list genuinely is the unit that changed. The cost: two devices
editing different phone numbers of one contact collide, where two devices
editing the city and the profession merge cleanly.

Snapshots are **read back from the database**, not serialised from the input
struct. Child ids are minted during the write, and a payload carrying
`"id": null` would have each device inventing its own id for the same phone
number — duplicates that no merge can reconcile, because nothing links the rows.

`crates/yanuka-db/tests/repository.rs` asserts that create, update and delete
each append a row, *and* that the rows contain the data: what the user typed
reaches the log, an edit logs the field that moved and not the ones that did
not, every entity kind is logged, and a merge carries the details it moved.
Nothing may change on disk without a mutation, or an edit made offline would
silently never reach another device — and a mutation that names the change
without recording it is the same failure with a passing test. See ADR-033.

## Push

Drain `status = 'pending'` in ULID order — which is creation order, because
ULIDs are time-sortable and minted monotonically.

Each mutation carries `base_version`, the version it was computed against. The
server compares it to what it holds:

- **equal** → apply, bump the version, acknowledge.
- **different** → the record moved underneath us. Return both revisions; the
  client resolves as below.

Retries use exponential backoff with a cap. A mutation that keeps failing goes
to `failed` and is surfaced in the UI rather than retried forever — silent
failure is the one behaviour this must not have.

## Pull

Incremental, never a full download. `sync_cursors` holds an opaque cursor per
entity type; the server returns everything changed after it plus a new cursor.

A pulled change whose entity has pending local mutations is not applied blindly
— it goes through the same merge as a rejected push.

## Conflicts

Resolution is **per field**, not per record.

```
Desktop:  city  = ירושלים        Android:  notes = "מומלץ על ידי..."
```

Different fields → merge automatically. Both devices' work survives.

```
Desktop:  phone = 054-…          Android:  phone = 052-…
```

Same field → a real conflict. **Both values are kept** in `conflicts` and the
user is asked:

```
נמצאו שתי גרסאות
  גרסת Desktop: 054-…
  גרסת Android: 052-…
[בחר Desktop]  [בחר Android]  [ערוך ידנית]
```

Never resolved silently, and never by last-write-wins on a timestamp. Clocks on
two machines that have been offline are not comparable, and picking a winner
means throwing away something a human typed on purpose.

The governing rule, from PRODUCT.md: **a temporary duplicate is always better
than lost data.** If the merge is ambiguous, keep both.

## Deletions

Soft, via `deleted_at`. A hard delete cannot be synced — the other device has no
way to distinguish "deleted" from "not yet received".

A tombstone is a normal update carrying `deleted_at`, and it merges like any
other field. Restoring is setting it back to null, which is why undo is honest:
the row never left.

**Cascades are not logged.** Deleting a tag or an organization also soft-deletes
the join rows pointing at it, and those rows get no mutations of their own: they
follow deterministically from the parent id. The apply side must therefore
perform the cascade itself when it receives a `tag` or `organization` delete.
This keeps the log proportional to what the user did rather than to how many
contacts happened to carry the tag.

## Devices

Each installation registers once and keeps a `device_id`, minted on first run
and stored in `app_meta`. Every write stamps it, so the sync engine can tell
which installation produced a revision and the conflict UI can say
*"גרסת Desktop"* rather than *"גרסה ב"*.

`devices` also holds `last_seen_at` / `last_sync_at`, and `revoked_at` for the
future ability to cut off a lost machine.

## What the user sees

Never the word "mutation".

Once sync ships, three facts: the data is safe locally, when it last left the
machine, and how much has not yet.

```
מאגר מקומי: זמין
סנכרון אחרון: לפני 5 דקות
7 שינויים ממתינים לסנכרון
```

**Until then, `SyncIndicator` must not show that.** With no transport, "סנכרון
אחרון: מעולם לא" and a permanently growing count of changes "waiting to sync"
describe a stalled queue rather than a feature that has not been built, and
that is precisely how they were read. On a product whose first promise is that
nothing gets lost, an accurate line that reads as *your work is stuck* is worse
than useless.

So the indicator leads with the fact that is both true and reassuring today —
the daily backup (ADR-028), which is the whole durability story while ADR-019
is deferred:

```
מאגר מקומי: זמין
גיבוי אחרון: לפני שעתיים
```

The mutation-log count moves to the tooltip and to הגדרות, framed as what it
actually is: a record kept so that work done before sync exists travels with it
when sync arrives. Swap this back the moment a transport lands.

## Rules for anyone implementing this

1. A local write must never wait on the network.
2. A mutation is written in the same transaction as the change, or not at all.
3. `payload` carries changed fields only. Sending whole records makes every edit
   collide with every other edit.
4. Merge per field. Only a genuine same-field collision is a conflict.
5. Never resolve a conflict silently. Never resolve one by timestamp.
6. Deletions are tombstones.
7. When in doubt, keep both versions.
