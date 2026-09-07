# SYNC

**Status: implemented (0.10.0, ADR-039).** A device pairs with a small server
— the always-on peer — and from then on every local change reaches the other
devices and theirs reach it, field by field, with a person deciding whenever
two devices disagree. The desktop keeps working exactly as before when the
server is unreachable; nothing waits on the network.

```
Desktop A ──push/pull──▶ yanuka-server (SQLite, same schema) ◀──push/pull── Desktop B
   │                            │ sync_log: one ordered stream                  │
   └── mutations (journal)      └── the archive, searchable                     └── mutations
```

## The model

The server is **not** a live database the desktop reads through. It is a peer
that happens to be always on: it holds a copy of the archive in the very same
SQLite schema, accepts a revision when the device's base is the version it
holds, and streams accepted changes to everyone else. It never merges. Merging
happens on the device, where the person who can resolve a conflict is.

Every device can create data offline. No device is authoritative. The server
does not win by virtue of being the server.

## What travels: a revision

The unit of exchange is a **revision** of one record:

```
entityType · entityId · baseVersion · changed[] · state · createdAt
```

- `state` is the whole record as it stands — for a contact, with the
  collections it owns (phones, emails, aliases, specialties, languages, tag
  ids, manual category memberships, organization links). A device that has
  never seen the record needs all of it.
- `changed` names the fields the sending device touched since `baseVersion`.
  A device that *has* seen the record merges by this list, field by field.
- `baseVersion` is the server version the change was computed against; 0 for
  a record the server has never held.

Six record kinds sync, in dependency order: tag, category, organization,
contact, note, relationship. Derived data never travels — normalized text,
phone digits, search rows, rule-category matches are recomputed on arrival.

## The journal is still the source

`mutations` (0001) is unchanged: every local write appends a row in its own
transaction, with the fields it changed. The engine reads the journal to
know *what is dirty* and *which fields moved*; it reads the tables to build
the state it sends. Pre-engine journal rows are honoured too: a record with
no `sync_revisions` row is "never sent", and goes up whole on first pairing.

Remote changes are written into the same journal with `status = 'synced'`
under the device that made them — so the card history shows an edit made on
another machine, with that machine's name.

## Push

1. Collect dirty records: those with `pending`/`failed` journal rows, plus
   those with no `sync_revisions` row. Records held by an open conflict are
   skipped.
2. Send them in chunks, ordered by kind so references resolve.
3. The server compares `baseVersion` with what it holds:
   - **equal** → apply, bump the version, append to `sync_log`, acknowledge.
   - **different** → refuse, answering with its current version.
4. Acknowledged items settle their journal rows (`synced`) and record the new
   server version. Refused items wait: the device pulls, merges, and pushes
   again — at most three rounds per cycle.

## Pull

Incremental by `seq`, the server's stream position, kept in `sync_cursors`.
The server leaves out the requesting device's own entries. Each streamed
change is integrated in its own transaction:

- **Unknown here** → written whole; journaled as a create by the other device.
- **Known, nothing unsent** → written whole; journaled with the fields that
  actually differed.
- **Known, with unsent local changes** → three-way merge (below).

A change that refers to a record not here yet (a note for a contact still on
its way) is kept aside and retried after later pages.

## Conflicts

Resolution is **per field**, not per record, against the fields each side
changed since they last agreed:

```
remote changed \ local changed  → taken from the server
local changed  \ remote changed → kept
both changed, different values  → a conflict; both values kept
both changed, same value        → nothing to do
```

```
Desktop:  city  = ירושלים        Laptop:  notes = "מומלץ על ידי..."
```

Different fields → merged automatically. Both devices' work survives.

```
Desktop:  phones = 054-…          Laptop:  phones = 052-…
```

Same field → a real conflict. Both values are written to `conflicts`, the
record is **held back from pushing**, and the local value stays on screen
until the person chooses on the conflicts screen:

```
במחשב הזה: 054-…            במחשב הנייד: 052-…
[לשמור את הגרסה שלי]  [לקחת את הגרסה מהמחשב הנייד]  [לערוך ידנית]
```

- **local** — the local values stand and are journaled again, so they go up
  on the next cycle over the server's.
- **remote** — the other device's values are written here, journaled under
  that device; the local word on those fields is withdrawn.
- **manual** — the person edited the record through the form; that edit is
  what goes up.

Never resolved silently, and never by last-write-wins on a timestamp. Clocks
on two machines that have been offline are not comparable, and picking a
winner means throwing away something a human typed on purpose. The governing
rule, from PRODUCT.md: **a temporary duplicate is always better than lost
data.**

A collection is one field: two devices editing the phone list produce one
`phones` conflict showing both lists. Two devices creating the same person
produce two records — a duplicate, which the duplicates screen already
handles.

## Deletions

Soft, via `deletedAt`, which is an ordinary field: a delete is a revision
whose `changed` is `["deletedAt"]`, and it merges like any other field. A
contact deleted on one device and edited on another arrives in the trash with
the edit intact. Restoring sets the field back to null.

## Devices and pairing

Each installation has a `device_id` minted on first run. A device pairs
once, with a code a person obtains either from the server's console (at
first start, or `yanuka-server pair-code`) or from any already-paired device
(settings → "קוד למכשיר נוסף"). The server answers with a bearer token the
device stores in its (encrypted) database. Codes are eight characters from
an alphabet without look-alikes, valid for fifteen minutes, single use.
Only SHA-256 digests of codes and tokens are stored on the server.

Revoking a device (`yanuka-server revoke <id>`, or through the API from
another device) invalidates its token; its records stay. Disconnecting from
the desktop forgets the server and every `sync_revisions` row; the records
and the journal stay, and a later pairing starts from a clean comparison.

## The worker

The desktop runs one thread: every 30 seconds it looks for unsent changes
and syncs when there are any, every 5 minutes it asks the server for news
regardless, and "סנכרון עכשיו" or a resolved conflict wakes it at once. It
never holds the database while it waits on the network — the engine borrows
the connection step by step — so a search or a save is never queued behind a
slow link.

## What the user sees

Never the word "mutation".

```
מאגר מקומי: זמין
מסונכרן · לפני 5 דקות
7 שינויים ממתינים לשליחה
2 התנגשויות לטיפול
```

Four facts: the data is safe locally, when it last agreed with the server,
how much has not yet, and whether anything needs a decision. Without a
server the second line says so ("סנכרון: לא הוגדר שרת") and the third counts
what the journal holds for the day one is set up.

## Verification

`crates/yanuka-db/tests/sync.rs` runs the whole protocol in one process —
two devices and a server, three in-memory databases, function calls where
HTTP would be — through the production engine: first pairing, a second
device receiving the archive whole, merges of different fields, a same-field
conflict settled each way, tombstones and restores, a merge of duplicates
travelling whole, rule categories evaluated on arrival, refused stale bases,
revoked devices. `server/yanuka-server/tests/http.rs` repeats the round trip
over a real socket with the desktop's HTTP transport.

## Rules for anyone touching this

1. A local write must never wait on the network.
2. A mutation is written in the same transaction as the change, or not at all.
3. A revision carries the whole state *and* the fields that changed. The
   state is for devices that have nothing; the changed set is for the merge.
4. Merge per field. Only a genuine same-field collision, with different
   values, is a conflict.
5. Never resolve a conflict silently. Never resolve one by timestamp.
6. Deletions are tombstones.
7. When in doubt, keep both versions.
8. The server never merges and never installs anything of its own (not even
   the default categories) — it holds what devices tell it.
