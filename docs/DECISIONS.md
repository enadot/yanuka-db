# DECISIONS

Architectural decisions and their reasons, including the things deliberately
**not** built. A deferral recorded with its cost is a decision; a deferral left
implicit is a bug waiting to be discovered.

Check this file and ARCHITECTURE.md before any significant architectural change,
and update them as part of it.

---

## ADR-001 — Turborepo + pnpm workspaces

One repository, one dependency graph, one CI run. Turborepo caches per package
so a change in `@yanuka/search` does not rebuild the desktop app.

## ADR-002 — rusqlite with `bundled`, behind custom Tauri commands

Rejected `tauri-plugin-sql`, which ships **raw SQL strings across IPC from
JavaScript**. That inverts the whole design: the frontend becomes
dialect-coupled, which kills the in-browser mock repository and any future
Postgres-backed web client, and it hands a webview a query interface to the
crown jewels.

`bundled` compiles SQLite into the binary rather than linking whatever the host
ships. That is what guarantees FTS5, the trigram tokenizer and `STRICT` tables
exist on a user's Windows machine, and it pins the version to what the tests
ran against. `connection::assert_capabilities` verifies this at startup rather
than trusting it.

## ADR-003 — Plain `.sql` files as the single source of truth; no ORM schema

`packages/database/migrations/sqlite/*.sql` is authoritative. Rust embeds it with
`include_str!`; TypeScript inlines the same text via a generator and runs it
through `node:sqlite` in tests.

An ORM schema definition (Drizzle or otherwise) would have added a **third**
representation — schema TS → generated SQL → the SQL Rust actually runs — to
keep in sync. It adds drift surface rather than removing it, because the
desktop's real database access is Rust and never loads the ORM. It earns its
place when Postgres and a JavaScript runtime both exist.

## ADR-004 — ULID primary keys, client-generated

Auto-increment is unusable offline: two devices would mint the same id. ULIDs
are time-sortable, so index locality is preserved and the mutation log drains in
creation order for free. Client generation makes an offline create idempotent
under retry.

UUIDv7 would serve equally well; the brief permits either and ULID was already
in place.

## ADR-005 — Rust split into three crates

`yanuka-search` and `yanuka-db` have no Tauri dependency, so they compile and
test on any machine. Only the thin shell needs `libwebkit2gtk`. Without this,
none of the storage or search code could be tested in CI or in a development
container.

## ADR-006 — The normalizer is implemented twice, conformance-tested

TypeScript for the browser, mobile and the in-memory engine; Rust for the
desktop's index and query path. Both are held to
`packages/search/fixtures/normalization.cases.json`, read by `vitest` and by
`cargo test`.

**Deferred:** compiling the Rust normalizer to wasm and dropping the TypeScript
copy. Correct long-term, but it adds a wasm toolchain and a build artifact to
every package that searches. Revisit when the web client lands.

## ADR-007 — Proclitic stripping: query-side everywhere, index-side for free text only

A four-letter minimum stem, because at three the strip destroys ordinary words
(`מלון → לון`, `שלום → לום`). Query-side variants are discounted ×0.5 so a hit on
the literal query always outranks a grammatical guess.

Notes additionally index the stripped form, because FTS5 tokenizes `בתפילין` as
one word and no query expansion can reach it. Names never do — recall is not
worth a spurious variant in the highest-weighted field.

## ADR-008 — DEFERRED: algorithmic Hebrew↔Latin transliteration

`Cohen`/`Kohen`/`Kohn`/`Cahn`/`Kagan` is a research problem, not a weekend.
Shipped instead: both scripts in the same FTS row, aliases as a first-class
entity weighted at 90, and phonetic keys covering the common families.

## ADR-009 — DEFERRED: matres lectionis folding at the index level

`דן`/`דין` and `רב`/`ריב` are different words. Handled by the fuzzy layer at edit
distance 1 with a score discount instead.

## ADR-010 — Own trigram table over spellfix1 / editdist3

Those are SQLite *extensions*, absent from the bundled amalgamation. Enabling
them needs custom `libsqlite3-sys` build flags and a matching custom build for
the Node test harness. FTS5's trigram tokenizer is in the amalgamation and
sufficient. Names, aliases and phone digits only — trigram-indexing free text
multiplies the index for a layer that exists to rescue misspelled *names*.

## ADR-011 — Tailwind v4, CSS-first

No `tailwind.config.js`; tokens live in `@theme` in
`packages/ui/src/styles/globals.css`. Logical properties (`ms-`, `ps-`,
`text-start`) and `rtl:` variants are first-class, which is exactly what an RTL
app needs.

**The trap:** Tailwind v4 only scans what it is pointed at. Without
`@source "../"` in the UI package and in the app, every class used only inside
`packages/ui` is dropped from the bundle and the components render unstyled. A
build check asserts a UI-package-only class survives into the CSS.

## ADR-012 — shadcn/ui components live in `packages/ui`

Installed there via the shadcn CLI and consumed as source by Vite, so the future
web client inherits them without a second copy. Source-only, no build step.

## ADR-013 — RTL needs three things, all of them mandatory

1. `dir="rtl" lang="he"` on the document.
2. **Radix `DirectionProvider`.** Radix computes its own directionality and does
   *not* read the DOM attribute. Without it, dropdown alignment, select
   positioning and arrow-key navigation stay left-to-right on a page that
   renders right-to-left. This is the single most-missed piece of RTL setup in a
   shadcn application.
3. A physical→logical sweep of the generated components (`ml-`→`ms-`,
   `left-`→`start-`, `text-left`→`text-start`). The Radix `data-[side=…]`
   attributes and slide-in animations are left physical — Radix resolves those
   through the provider.

`Ctrl+K` binds on `event.code`, not `event.key`: under a Hebrew layout the
physical K key reports `'ל'`, so a `key`-based binding fails for exactly the
users it was built for.

## ADR-014 — No *fetched* fonts (superseded in part by ADR-032)

Originally: Segoe UI / Noto Sans Hebrew / Arial Hebrew, on the grounds that the
application must start and render correctly on a machine that has never been
online, and that a font request also leaks that it is running.

Both reasons survive and still bind. What they actually forbid is a *request*,
not a typeface — ADR-032 bundles one locally and neither reason applies to it.
The system stack stays behind it as a fallback that has to keep working.

Hebrew keeps `line-height: 1.7` and zero letter spacing regardless of the face:
it has no ascenders and descenders to separate lines visually, and it degrades
badly with tracking.

## ADR-015 — "Only shadcn/ui" means no competing component library

The rule is: every UI element is a shadcn/ui registry component composed with
Tailwind utilities, and app-specific pieces are built from those primitives.

shadcn components **are** their dependencies — `radix-ui`, `cmdk`, `sonner`,
`react-hook-form`, `class-variance-authority`, `lucide-react`. Installing those
is compliance with the rule, not a breach of it; they arrive with the components
themselves.

What the rule forbids: MUI, Ant, Chakra, Mantine — and convenience libraries
like `react-virtuoso` or `@tanstack/react-virtual`, which is why ADR-016 exists.

## ADR-016 — Keyset pagination and `content-visibility` instead of virtualization

`OFFSET 99950` makes SQLite walk 99,950 rows, which breaks the performance
target at exactly the scale the product promises. The cursor is the last row of
the previous page, so page nine hundred costs what page one costs.

That rules out numbered page links, which need OFFSET. The alphabet index
replaces them, and is closer to how anyone actually navigates a list of names.

Offscreen rows get `content-visibility: auto`, so the browser skips their layout
and paint. That is most of what a virtualization library provides, in two lines
of CSS, without violating ADR-015.

## ADR-017 — DEFERRED: `apps/web`, `apps/mobile` and `server/` are not created

Empty placeholder packages slow every build, distort coverage, and rot. Their
shape is documented in ARCHITECTURE.md. What actually preserves the option is
keeping the shared packages platform-agnostic and contract-tested — which is
enforced by `runRepositoryContractTests`, not by an empty directory.

## ADR-018 — DEFERRED: encryption at rest

The database file is unencrypted. This is the largest open security item; see
SECURITY.md.

Prepared for: every connection goes through one `open(path, key)` function that
already accepts a key, the cargo feature exists, and `temp_store = MEMORY` is
already set so SQLCipher will not spill plaintext temp files.

Cost of enabling: key derivation and storage, a plaintext→encrypted upgrade
path, and an OpenSSL build on Windows CI adding roughly ten minutes.

## ADR-019 — DEFERRED: sync transport

`mutations`, `sync_cursors`, `conflicts` and `devices` ship, and every write
appends a mutation row, so nothing done offline is lost. There is no network
code. SYNC.md specifies the protocol.

The split is deliberate: the expensive-to-change part is the local record of
what happened, and getting it wrong means the changes made before the sync
engine existed are unrecoverable.

## ADR-020 — DEFERRED: permission enforcement

`users`, roles, permissions and `audit_log` ship and are respected by
`redactForPrincipal`. The desktop is single-user and local, so nothing is
enforced yet and the audit log is written but not surfaced. Enforcement lands
with the server.

## ADR-021 — DEFERRED: installer code signing

An unsigned NSIS installer triggers SmartScreen on first run. An EV or Azure
Trusted Signing certificate is a purchase decision, not a code decision.

## ADR-022 — Facet counts are computed post-filter

Selecting a country shows the other countries at zero. Proper per-dimension
exclusion needs one pass per facet. The UI compensates by keeping selected chips
visible with a clear-all beside them.

## ADR-023 — Forward-only migrations with an automatic pre-migration backup

No `down` files. A hand-written reverse migration is exactly the kind of code
that silently drops a column's worth of data, and it runs precisely when the
user is already in trouble. Three backups are kept, WAL and SHM included.

The runner verifies the checksum of every applied migration and hard-errors on a
mismatch, because an edited migration means the database in front of us was
built by different SQL than this binary expects.

## ADR-024 — `node:sqlite` for the TypeScript test harness

Node 22 ships SQLite 3.51 with FTS5, the trigram tokenizer and `STRICT` tables.
That lets the real migrations and the real search SQL run under `vitest` with no
native module and no compiler toolchain — chosen over `better-sqlite3`, which
needed a native build that the container could not produce.

## ADR-025 — The `<100 ms` target is a query-time budget

Achievable for the query, and measured as such in Rust. Stating it end-to-end
would fold in React render and IPC, which are a separate budget and a separate
set of fixes. See SEARCH.md.

## ADR-026 — CSV import: pure mapping logic behind the repository boundary

The archive this product exists for lives in notebooks and old exports, so the
first deferred item to land was CSV import. The split follows the codebase's
one rule: parsing (`@yanuka/utils` csv.ts) and header-detection/row-mapping
(`@yanuka/core` import.ts) are pure and unit-tested; the screen feeds the
result to `ContactsRepository.createContact` row by row, so the flow is
identical against the in-memory repository and SQLite, and the e2e suite
exercises it without a Tauri shell.

Import decisions all follow "מידע לא הולך לאיבוד": only a nameless row fails
(and is reported, not silently skipped); phones import exactly as written; a
malformed email becomes a notes line; every imported contact records its file
name in `source`. Encoding is UTF-8 with a windows-1255 retry on damage —
detection by decode failure, not heuristics. The parser is hand-rolled
(RFC 4180 + BOM + bare-CR): a dependency would not cover the part that is
actually hard here, which is the mapping.

OCR import stays deferred: it needs a model or a service, and both collide
with the offline constraint. The mapping layer is the contained seam it will
slot into.

## ADR-027 — Duplicate detection is whole-database; merge is lossless by rule

Follows directly from CSV import (ADR-026): an archive assembled from several
sources holds the same person more than once, and the moment to resolve that
is after import, with both records visible — not at entry time, where the
per-contact `find_duplicates` warning already exists.

Detection (`yanuka-db::merge::list_duplicate_pairs`) pairs contacts by shared
phone (last-7 digits), shared normalized email, or identical normalized name,
strongest signal first. It only ever *suggests*: the screen shows the pair
with its evidence and the user decides, including "אלו אנשים שונים".

Merge (`merge_contacts`) is governed by priority 1, מידע לא הולך לאיבוד:
children move to the kept contact (only exact value-duplicates are skipped —
suffix-based dedupe could silently drop a genuinely different number sharing
a local suffix); blank scalars fill from the merged side; conflicting scalars
are preserved as labeled notes lines; relationship edges re-point; and the
merged contact is soft-deleted with its complete prior state in the mutation
log. The same semantics are implemented in MockRepository and pinned by the
repository contract tests, so the browser build behaves like the desktop.

## ADR-028 — Daily rotating backups and a round-tripping CSV export

The archive exists as one SQLite file on one frequently-offline machine, with
no sync and no cloud (ADR-019 deferred them). Until a server exists, backup
*is* the durability story, so it cannot remain a manual habit.

Three layers, all offline:
- **Automatic**: one backup per calendar day, taken at launch via SQLite's
  online-backup API (consistent while the database is open, WAL included),
  named `daily-<date>.db` beside the pre-migration copies, seven kept. A
  failed backup is reported and never blocks startup — the user's access to
  their data outranks the safety net.
- **On demand**: הגדרות ← "גיבוי עכשיו" snapshots to a user-chosen path,
  typically a USB stick. Same API, arbitrary destination.
- **Portable**: CSV export whose Hebrew headers are exactly what the import
  auto-detection recognizes, so an exported file re-imports with the mapping
  already correct — pinned by a round-trip test. The BOM is for Excel. What
  CSV cannot carry (relationship edges, organization links, note metadata)
  lives in the database backups; the CSV is the human-readable snapshot.

The export walks the repository like any screen (no SQL side door), which is
why the browser build exports identically via a Blob download. The desktop
write path is a deliberately narrow command — `.csv` paths only — rather than
a general file-write IPC: the webview is not a trust boundary.

## ADR-029 — A patch says what changed; absent, `null` and `[]` are three things

The bug this fixes was invisible and total. A write replaces a contact's child
collections wholesale (`write_children`), and the edit form rebuilt its state
from a blank template that carried phones and tags but not e-mail addresses,
aliases, languages, categories or organization links. So every save deleted all
five, silently, on a screen that showed no sign anything had been touched.
Priority 1 — מידע לא הולך לאיבוד — was being decided by which inputs a screen
happened to render.

Two boundaries were conflating states that have to stay apart:

- **Absent vs. empty.** `ContactInput` is `#[serde(default)]`, so a payload that
  omits `emails` arrives as `[]` and wipes the addresses. `update_contact` now
  takes a `ContactPatch` whose every field is an `Option` and merges it onto the
  stored record before writing. Absent means untouched; `[]` means the user
  actually removed everything.
- **Absent vs. `null`.** Serde collapses an explicit `null` into the outer
  `None` of a nested `Option`, and TypeScript's `??` treats `null` and
  `undefined` alike — so on both sides "clear the city" read as "leave the city
  alone", and a cleared field could not be saved. The Rust patch deserializes
  its nullable scalars through a `double_option` helper; `MockRepository` merges
  on key presence rather than `??`. Both are pinned by tests, in both languages.

The form is fixed as well as the boundary. It now loads and returns every
collection, including the ones it has no control for, because a boundary that
tolerates a careless caller is not a licence to write one.

## ADR-030 — The connection graph is writable from the card

`crates/yanuka-db` has stored relationships, timestamped notes and
contact–organization memberships since the first migration, the repository
interface has exposed them since the first commit, and the contact card has
displayed them all along. Nothing could create one. The graph was demo data.

That is the wrong gap to leave open in *this* product. "מי היה היהודי הזה
מלונדון שהרב המליץ עליו?" is a question about an edge and about a sentence
someone wrote down years ago — the two things a user is most likely to remember
were the two things they could not record.

- **Relationships** are added from the card, and both readings of every
  asymmetric type are offered. An edge is stored once and directed, but the
  person entering it is standing on one particular card: from the recommended
  contact the natural sentence is "הומלץ על ידי הרב", and offering only the
  outgoing phrasing would force the user to work out which of two cards the
  relationship belongs to before they could write it down. Picking a reversed
  reading simply swaps the endpoints on save. The far end is chosen from search
  results, never typed — an edge to a name-shaped string is not traversable and
  would answer "who else did he recommend" with nothing.
- **Notes** are added, edited and deleted in place. They stay separate from
  `contacts.notes`, the single always-visible remark, so a new dated entry never
  has to be appended to an existing paragraph — which is how the date of the
  original remark gets lost. They are indexed like every other note, so a
  sentence written here answers the question the archive exists for.
- **Organization links** are search-then-create. "ישיבת מיר" typed three
  different ways is three institutions, and the archive stops being able to
  answer "who else is from there"; but refusing to save until the user goes and
  defines the institution first is exactly the friction that keeps a
  half-remembered detail out of the database. So the picker searches existing
  organizations and offers to create the one being typed, inline. SQLite also
  had to start *writing* `contact_organizations` — it only ever read them.

All of it goes through `ContactsRepository`, so the browser build behaves
identically, the contract tests pin both implementations, and the e2e suite
exercises the flows without a Tauri shell.

## ADR-031 — The recycle bin, and why it has no "empty" button

Deleting a contact has always been a soft delete: the row stays so the change
can sync and be undone, and the confirmation dialog said as much — *"המידע נשמר
וניתן לשחזר אותו"*. But every list and every search ends in `deleted_at IS
NULL`, and `restoreContact` was reachable from exactly one place: the undo
action on the toast shown immediately after deleting. Once that toast faded the
record was unreachable. A soft delete nothing can list is a hard delete with
extra steps, and the interface was promising otherwise.

`deletedContacts()` is a separate read rather than an `includeDeleted` flag on
`listContacts`. The flag exists in the query schema and was never implemented in
SQLite, which is the tell: a parameter that switches off the one clause every
other read depends on is one forgotten `false` away from putting deleted people
back into the ordinary list. This method can only ever return records that are
already gone.

`DeletedContact` carries `deletedAt` beside the summary instead of letting the
screen read `updatedAt`. The two coincide today only because a delete writes
both, and that is the kind of coupling that starts printing wrong dates the
first time anything else touches a deleted row.

**There is no way to empty the bin, and that is the decision, not an omission.**
A permanent delete is the only operation in this product that actually destroys
something, it is irreversible by definition, and it would be reached at the
exact moment a user is annoyed at a bad import and in a hurry — the same
reasoning that refuses `down` migrations in ADR-023. The cost of not having it
is a table of soft-deleted rows that no query reads; at one user and one
lifetime of contacts that is nothing. The cost of having it is the one outcome
priority 1 forbids. If a real need appears — a three-hundred-row import from the
wrong file — the answer is a scoped "undo this import", which knows exactly what
it is destroying, not a general delete-forever button.

## ADR-032 — A bundled typeface, and a scale built for one glance

The user is one person, frequently offline, not technical, and reads this
screen while doing something else. The interface was competent and quiet: 14px
body text, one weight step between a heading and a paragraph, a one-pixel focus
ring, and a search box whose entire capability was described in a placeholder.
Everything was findable if you already knew where to look.

Google Sans ships as local WOFF2 at four weights (400/500/600/700), full
character set, no subsetting — this is an archive of contacts from all over the
world, and dropping the glyphs for a name in Yiddish or Russian is exactly the
silent loss priority 1 forbids. It is 2.0 MB in the bundle, down from 7.7 MB as
TTF. `font-display: block` because there is no network round-trip to wait out:
nothing is gained by a swap period and a flash of fallback is lost.

The point of four real weights is that a glance resolves the page without
reading it. Applied consistently:

- **Body 17px, headings 30/20/18 bold** — set once in the base layer instead of
  being re-decided per screen, so hierarchy cannot drift.
- **One heavy thing per row.** In a search result only the name is large and
  bold; profession, city and phone are deliberately quieter. A row where four
  things compete has to be read rather than scanned.
- **The active nav item is marked three ways** — background, weight, and a bar
  on the start edge. "Where am I" after an interruption should not require
  comparing two shades of grey.
- **A 3px focus ring, and a 44px floor on anything clickable.** The floor is
  the accessibility minimum for a pointer target and doubles as an
  anti-misclick rule; the alphabet index in particular was 30 adjacent 32px
  buttons, where hitting ג instead of ב was a coin toss.
- **`prefers-reduced-motion` is honoured.** Movement is a distraction cost for
  this audience and the operating system already knows the answer.

The largest change is not typographic. **The home screen now shows what you can
type, as chips you can press** — a profession, a city, a misspelling, half a
phone number, a word from a note. A search box is only self-explanatory to
someone who already knows what the engine accepts; someone who does not reads a
blank box as "I must know the name", which is precisely the case this product
exists to handle and the one where it looks broken. The chips are real queries
against the real data, drawn from the user-story table in PRODUCT.md, so each
one works and each one teaches the shape of the next query the user types
themselves. They disappear once there is a query — a starting point, not
permanent furniture.

Two fixes fell out of doing this. `.numeric` set `direction: ltr` on block
elements, which also flips what `text-align: start` resolves to, so every phone
number was thrown to the far left of the window, visually orphaned from its
row; the digits still read LTR, the block now stays put. And the bundled font
is asserted in the e2e suite rather than assumed: a font that fails to load
does not throw and does not look broken, it looks like a slightly different
application — the kind of regression a bundler upgrade introduces silently.

Note on licensing: Google Sans is Google's proprietary brand typeface and is
not licensed for third-party redistribution. It was supplied deliberately for
this single-user, internally-distributed build. If this application is ever
distributed more widely, the face has to be replaced — the stack is ordered so
that swapping the `@font-face` block and the first entry in `--font-sans` is
the whole change.

## ADR-033 — The mutation log records the change, not a receipt for it

ADR-019 defers the sync transport but claims the local record is ready: "every
write appends a mutation row, so nothing done offline is lost". SYNC.md is more
specific still — "`payload` holds only the fields that changed" — and names
that as the property field-level merging depends on.

The rows were being appended. Their contents were a placeholder. A contact
created with a city, a phone number, an e-mail address and a note produced
exactly one mutation:

```
contact  create  {"displayName":"שרה כהן"}
```

`mutation::diff`, the function written to compute the changed subset, existed
and was never called from anywhere — dead code since it was introduced. Both
contact write sites passed a hardcoded `json!({ "displayName": … })`. Child
collections were not logged at all, because `write_children` never touched the
log. Neither were tags, categories, organizations, relationships or notes,
because `taxonomy.rs` never imported the module. A merge logged
`{"mergedFrom": …}` — the name of the operation with none of its effect.

The test that was supposed to catch this asserted a *count*: three writes, three
rows. It passed throughout.

None of this was visible in the running application, and it would not have
become visible when a server was added either. Sync would have appeared to work
— devices exchanging rows, counters draining — while delivering bare display
names. The failure mode is the one the product exists to prevent, arriving
through the mechanism built to prevent it.

What the log now carries:

- **create** — the whole record. There is no prior state to diff against, and a
  device replaying the log has nothing else to build the contact from.
- **update** — the changed fields only, compared against the stored record
  rather than against the patch. A patch that re-sends an unchanged field is
  not an edit, and logging it as one manufactures conflicts elsewhere.
- **merge** — a field-level diff of what moved onto the surviving contact,
  keeping `mergedFrom` alongside so the history stays readable. Re-deriving the
  result from the two originals would only agree if the other device held
  byte-identical copies of both, which is exactly what cannot be assumed.
- **tags, categories, organizations, relationships, notes** — their own
  create/delete rows, with real payloads.

Two decisions inside that are worth stating, because both are trade-offs:

**A child collection is one field.** If any phone changes, the whole phone list
is in the payload. This is not a shortcut — `write_children` replaces each
collection wholesale, so the list genuinely is the unit that changed, and a log
claiming finer granularity would not describe what the database did. The cost is
real: two devices editing different phone numbers of one contact collide, where
two devices editing the city and the profession merge cleanly.

**Snapshots are read back from disk, not serialised from the input.** The ids of
phones and e-mail addresses are minted during the write. A payload carrying
`"id": null` would have each device inventing its own id for the same phone
number, surfacing later as duplicates that no merge can reconcile because
nothing links the two rows. The extra read per write is worth that.

Cascading deletes — the join rows dropped when a tag or organization is deleted
— are deliberately *not* logged. They follow deterministically from the parent
id, so replaying the parent delete reproduces them. This obliges the apply side
to perform the cascade, which SYNC.md now states as a rule.

The count-based test is kept and joined by four that read the payloads: that
what the user typed is in the log, that an edit logs the field that moved and
not the ones that did not, that every entity kind is logged, and that a merge
carries the details it moved across.

## ADR-034 — The apply path, and the four ways it refuses to lose data

`apply.rs` takes a mutation made on another device and folds it into this one.
It is the half of sync where data actually gets destroyed if the reasoning is
wrong: the transport can only be slow or broken, but a bad merge is silent and
permanent.

**Applying never logs a mutation.** A remote change written through
`update_contact` would append a local mutation, push it back, and the two
devices would trade one edit forever. So the apply path writes through
`insert_contact_row` / `update_contact_row`, extracted from the local path for
this purpose and shared with it — one writer, so the two cannot drift into
producing different rows for the same change.

**Merging is per field, decided by `previous`, not by version.** A version
number moves when *any* field changes, so it cannot distinguish "they edited
the city while I edited the profession" from "we both edited the city". The
mutation carries what the sender saw before its edit; if the local value still
equals that, nothing here touched the field and theirs is simply newer. This is
the entire reason ADR-033 had to land first — without a real `previous` there
is no three-way merge, only a guess.

**A genuine collision is never resolved silently.** Both values go to
`conflicts` and the local one stays in place until a human picks. Not
last-write-wins: two machines that have been offline do not have comparable
clocks, and a timestamp is not evidence about which person was right.

**A change whose subject has not arrived is deferred, not dropped.** A note or a
relationship can reach a device before the contact it hangs off. Writing it
violates a foreign key; skipping it loses it. `Applied::Deferred` returns it
unapplied *and unrecorded*, so the next pass retries — and an edge specifically
waits for **both** of its endpoints, because half an edge is worse than no edge:
nothing will ever come back to repair it.

Three further decisions worth naming:

**A create landing on a contact that already exists only fills blanks.** A
create carries the whole record and no `previous`, which makes it the one
payload that could flatten local work. The mutation id normally stops a second
delivery dead, but a log restored from backup or replayed through a rebuilt
server can present the same contact under a fresh id. In that case an unfilled
field may be filled and a filled one is treated as a disagreement. The test for
this fails if the guard is removed, which was checked rather than assumed.

**Names are the identity for tags, categories and organizations.** Two devices
offline both adding "ישיבת מיר" mint different ids for one institution. Letting
both land would split every facet count that mentions it and make "who else is
from Mir" answer half the truth, so an incoming row whose normalized name is
already present is dropped. Contacts deliberately do *not* work this way —
two people really can share a name, and merging them is a decision for the
duplicate-merge flow with a human present.

**Delete cascades are reproduced, not shipped.** Following ADR-033, deleting a
tag does not log the join rows it soft-deletes; the apply path re-runs the
cascade from the parent id. This keeps the log proportional to what the user
did rather than to how many contacts happened to carry the tag.

Verified by `crates/yanuka-db/tests/sync.rs`, which runs two real databases and
moves mutations between them by hand — the same thing a sync engine will do,
without one existing yet. The load-bearing test is convergence: each device does
a spread of independent work while out of contact, and after the logs are
exchanged both describe the same archive.

Still absent, by design: the transport (ADR-019), a UI for resolving conflicts,
and any parity in `MockRepository`, which keeps a counter rather than a log
because the browser build has nothing to sync with.

## ADR-035 — The server relays a sealed log; it does not hold a replica

Multiple computers and an Android phone need the same archive, and the main
machine is frequently offline. That settles the shape of every device — a full
local replica — and leaves one question: what is on the server.

The conventional answer is a mirror. The server holds the same schema in
PostgreSQL, applies incoming changes, resolves conflicts, and can answer
queries. Rejected, for three reasons:

**A second schema is a second implementation of every rule about this data**,
in a different language against a different database. They drift. The symptom
of drift is a contact that reads differently depending on which device you ask,
with no authority to consult about which is right — on a product whose first
promise is that information does not get lost.

**A server that merges is a server that reads.** Sealing the payloads is what
makes it defensible to keep a private contact archive on rented infrastructure
while ADR-018 leaves the desktop database unencrypted. A server with opinions
about the contents cannot also be blind to them.

**The hard part is already written.** Three-way merge, conflict detection,
tombstones and deferral live in `apply.rs` and are tested against two real
databases (ADR-034). Rewriting them server-side would double the surface
without adding a capability.

So the server stores `(seq, id, device_id, created_at, nonce, ciphertext)` and
hands back everything after a cursor. It is small enough to read in one sitting,
which is the property that matters for the component nobody looks at again until
it misbehaves.

**Two secrets, deliberately.** The enrolment secret lives on the server and
proves a device may join; the data passphrase never leaves the devices and seals
the payloads. Conflating them would be easier to explain to one non-technical
user and would hand the server the ability to read everything. A `ConnectionCode`
bundles both into one string to paste, and only the enrolment half is ever sent.

**Pushes are serialised with a Postgres advisory lock.** Without it two pushes
can take sequence numbers 10 and 11 and commit in the opposite order; a device
pulling in between records 11 as its cursor and never asks for 10 again. Nothing
errors, and one change simply never arrives on one device. The test for this
holds the lock and asserts a push blocks — the property is invisible in the
finished rows, so a test that inspects them afterwards cannot prove it. Removing
the lock makes that test fail, which was checked.

**A pull is not filtered by device.** Returning a device its own history costs a
little bandwidth and buys the ability to rebuild a machine that lost its
database: it re-enrols and replays everything. The device already recognises a
change it has applied. This makes the sealed log an off-site backup as well as a
courier, which serves the first priority directly.

Costs, stated rather than discovered later:

- No server-side search, and therefore no thin web client. Every device is a
  full replica or it is nothing.
- The provider sees how much changes and when, though not what.
- A lost data passphrase is unrecoverable. It costs syncing, not the archive —
  the local database is unencrypted and the daily backups still work — but that
  distinction has to reach the user before they choose a passphrase.

Deployment is Fly.io with managed Postgres, per the hosting decision; nothing
but `fly.toml` is specific to it. See docs/DEPLOY.md.

## ADR-036 — The sync loop, and never holding the database across the network

The three earlier stages each worked in isolation: the log records the change
(ADR-033), the apply path folds in a remote one (ADR-034), the server keeps
them in order (ADR-035). `yanuka-sync-client` is what makes them one system —
seal, send, fetch, open, hand to `apply`. There is deliberately no cleverness
left in it.

**The loop borrows the database through a trait, never across an await.** The
first version took a `&mut Connection` for the whole round, which reads fine
and would have frozen the interface for the duration of every HTTP request:
the desktop keeps one connection behind a mutex that every screen also needs.
On the one product whose defining claim is that it works regardless of the
network, an interface that stops responding when the network is slow is an
unusually bad failure. The `Database` trait takes the lock inside a closure and
releases it before any request, and the desktop's `AppState` implements it with
the same `with` every command already uses.

**A change is marked settled only after the server confirms it stored it.**
Anything else stays pending and is sent again. Sending twice is free — the
mutation id makes the second delivery a no-op at both ends — and the opposite
error, dropping something from the queue that never arrived, is silent and
permanent.

**The cursor follows what was applied, not what was fetched.** When one change
cannot be applied yet, the cursor stops below it rather than past it. This is
why `Envelope` carries a per-item `seq`: a single cursor for a whole page cannot
express "everything up to here, but not that one", and advancing past a deferred
change means never asking for it again.

**Two secrets, one paste.** A `ConnectionCode` bundles the server address, the
enrolment secret and the data key into one string. The enrolment secret is *not*
stored on the device — a machine that could mint enrolment codes from its own
credentials would turn one stolen laptop into permission to add more devices —
so producing a code for a new machine asks for it. That is a real inconvenience
and the correct one.

The offline indicator changed for the third time, and the reason is the point:
it now follows the device's actual state rather than the project's roadmap.
Connected, it says when work last left and how much has not. Not connected, it
says when the last backup was taken. Neither version says "never" to a user who
has done nothing wrong. See the note in SYNC.md.

Verified by `crates/yanuka-sync-client/tests/end_to_end.rs`: two real SQLite
databases, a real axum server on a real port, real PostgreSQL, real HTTP, real
sealing. It covers a contact typed on one machine appearing *and being
searchable* on the other, both machines working offline and reconciling, a
replacement machine rebuilding the archive from nothing, a wrong connection code
being refused without leaving settings behind, a wrong data key failing loudly
instead of writing rubbish, and a same-field collision surfacing as a conflict
rather than a loss.

Still absent: a background timer (syncing is a button today), and a screen for
resolving conflicts — they are recorded and counted and surfaced in a toast, but
choosing between two versions still means editing the contact by hand.
