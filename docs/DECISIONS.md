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

## ADR-014 — System fonts, no web fonts

Segoe UI / Noto Sans Hebrew / Arial Hebrew. The application must start and
render correctly on a machine that has never been online, and a font request
also leaks that it is running. Hebrew gets `line-height: 1.7` and zero letter
spacing — it has no ascenders and descenders to separate lines visually, and it
degrades badly with tracking.

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

## ADR-018 — encryption at rest (delivered by ADR-033)

Originally deferred; delivered in 0.5.0 — see ADR-033. The preparation below
is what made it a contained change.

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

## ADR-029 — הערות וקשרים נכתבים מכרטיס איש הקשר

The archive's value sits in free-text notes and in who-knows-whom edges, yet
until now both were read-only in the UI — the only way in was CSV import or
the single notes field on the contact form. That contradicted the product's
core loop: the sentence the user writes today is how the person will be found
in fifteen years.

Notes: add, edit and delete live on the card itself. Every write goes through
`taxonomy` and reindexes the contact in the same transaction, so a phrase is
findable the moment it is saved — and stops matching once replaced. (The
previous `update_note` IPC wrote SQL directly and skipped the reindex; it now
routes through `taxonomy::update_note`, and a regression test pins that an
edited note's old wording no longer matches.)

Relationships: an edge is stored once, directed, and always written from the
current card outward. The form's type labels complete the sentence
"<this contact> <type> <other>", with a live preview spelling it out, so
direction is never a guess; the far endpoint is picked through the same
suggestion engine the global search uses. Deleting from either side removes
the single stored edge everywhere — pinned by contract tests that run against
both the mock and the SQLite implementations, and by an e2e that reads the
same edge from both cards through the inverse labels.

## ADR-030 — מתקינים מתפרסמים ב־GitHub Releases

Every CI run already produces a Windows installer, but as a workflow
artifact: it expires after 30 days and lives behind the Actions UI — the
wrong home for the one file the offline update story depends on (ADR-021:
compare the version on the settings screen with the version in the
installer's filename).

A pushed `v<version>` tag now triggers a dedicated workflow that builds the
NSIS installer and publishes a GitHub Release with the exe attached and that
version's CHANGELOG section as the notes. Two guards keep the story honest:
the tag must equal the version declared in `tauri.conf.json` (otherwise the
filename would lie about the contents), and the changelog section must exist
(otherwise the release would say nothing — `scripts/release-notes.mjs` fails
the build). The per-push CI artifact remains as a convenience for testing
unreleased builds.

## ADR-031 — סל מחזור והיסטוריה: הרשת של עדיפות 1

Deletion was soft from day one, but the UI made a promise it could not keep:
the delete toast said "הועבר לסל המחזור" while no such screen existed — once
the toast faded, the row was unreachable from the application. Likewise every
write already landed in the mutation journal with the changed fields *and*
their previous values, while nothing could read any of it back.

Trash: `/trash` lists soft-deleted contacts — `list_deleted_contacts` exists
on both backends and is pinned by contract and Rust tests — each row with its
deletion time and a restore button. Deliberately no "empty the trash":
permanent erasure must propagate through sync first (ADR-019), so offering it
now would contradict the reason the screen exists.

History: the card's history is **derived from the mutation journal**
(`mutation::history`), not from the `audit_log` table — nothing writes that
table yet, and it stays reserved for the multi-user era (ADR-020). The
`audit_log` IPC command now reads the journal, mapping payload shapes back to
verbs (a restore is an update whose payload clears `deletedAt`; a merge is
recognizable by `mergedFrom`/`mergedInto`).

Fixed on the way: `update_contact` journaled only `displayName` no matter
what changed — starving both this history and the field-level sync merge the
journal exists to enable. It now diffs every scalar field into
`payload`/`previous`, and the mock mirrors the same diff so both backends
render identical history.

Known gap, deliberately deferred to the sync stage: taxonomy writes (notes,
relationships, tags, categories) do not journal yet. The card shows their
current state but not their history — and the sync engine will need those
entries anyway, so the fix belongs to that stage, once, rather than
piecemeal here.

## ADR-032 — היומן שלם: כל כתיבה נרשמת, והמראה נארז עם התוכנה

Three closures in one stage, the first being the debt ADR-031 recorded.

**Every write journals.** Notes, relationships, tags, categories and
organizations now append to the mutation journal inside their own write
transaction, through one `journal()` helper in taxonomy.rs — payload carries
the changed fields (and the routing keys `contactId`/`fromContactId`/
`toContactId`), `previous` carries what they replaced. A deleted note's body
rides in `previous`, so the wording of a note deleted years ago is
recoverable from the journal — priority 1, applied to the journal itself.
The idempotent tag-reuse path journals nothing, and deleting an
already-deleted edge journals nothing: only real changes replay.

**The card history shows children.** `mutation::history` matches an entity's
own id or any routing key in the payload, so one contact's history includes
its notes and both endpoints of its edges. Linkage keys are filtered out of
the rendered diff; entries are labeled (a note by its wording, truncated at
60 chars by the same rule in both backends; a tag by its name). The mock
mirrors all of it with a hidden `related` routing field, pinned by contract
tests against both implementations.

**Heebo is bundled, not fetched.** ADR-014 forbade *web* fonts — network at
render time — which left the desktop on Segoe UI's dated Hebrew. Bundling
Heebo (SIL OFL, the Hebrew face designed alongside Roboto) as two variable
woff2 subsets in `public/fonts` (~42KB total, license included) keeps the
never-online guarantee while making typography identical on every machine.
The system stack remains as fallback.

**The installer speaks the machine's language.** The NSIS language-selector
dialog rendered with blank option labels on the target machine; it is gone.
NSIS now picks Hebrew or English from the Windows locale directly.

## ADR-033 — הצפנה במנוחה: מפתח אקראי, לא סיסמה

Delivers ADR-018, with one design decision doing all the work: the SQLCipher
key is a **random 256-bit value in the Windows credential store**, surfaced
to the user as a recovery key — not a passphrase. A passphrase would invert
the priority list: forgetting it turns priority 6 (security) into a total
loss under priority 1, database and backups together. A random key costs the
user nothing day to day, opens automatically at launch, and the recovery key
(shown once from settings, kept off the machine) is the reinstall story.

Mechanics, in the order they protect:

- **Raw key, no KDF.** `x'<64 hex>'` form; Argon2id would stretch an
  already-random 256-bit value into nothing but startup latency.
- **Upgrade is export → verify → swap.** `sqlcipher_export` into a staging
  file; the staging file is opened with the key, integrity-checked and
  row-counted before the rename. A crash at any earlier point leaves the
  plaintext original untouched. A failed upgrade opens plaintext and says so
  in settings — the data must open even when encryption cannot be had.
- **Backups are keyed.** The online-backup API writes through the
  destination's codec; an unkeyed destination would have produced plaintext
  copies of an encrypted database. `backup_to`/`daily_backup` now take the
  key. The pre-migration copy is a byte copy and inherits encryption for
  free.
- **`locked` is a real state.** A restored file on a fresh Windows opens via
  a recovery-key screen; the typed key is validated against the file and
  then persisted. Every IPC command answers `DbError::Locked` until then.
- **Dev builds degrade.** Only the Windows keyring backend is compiled; on
  Linux/macOS the store reports unavailable and the shell runs plaintext —
  which is also why the sqlcipher cargo feature stays a feature rather than
  a default: the crate's tests run in both modes.

Cost accepted: vendored OpenSSL on Windows CI (~10 min, predicted by
ADR-018). Deliberately not done: passphrase layering on top of the stored
key, and encrypting the browser build's demo data (there is no file).

## ADR-034 — הפונט הוא Google Sans; תיקון עובדתי ל־ADR-032

ADR-032 bundled Heebo on the claim that Google Sans was proprietary. That
claim expired before it was written down: Google released Google Sans (and
Google Sans Flex) under the SIL OFL in November 2025, and the Google Fonts
build of Google Sans ships a real Hebrew subset — verified from the font
file itself (all 27 Hebrew letters in the hebrew-subset woff2; copyright
"The Google Sans Project Authors"). The user asked for Google Sans by name;
with the license obstacle gone there was no product reason left to refuse.

Mechanics unchanged from ADR-032, which is the part that mattered: the font
is *bundled* (two variable woff2 subsets, hebrew + latin, ~48KB, OFL text
included), never fetched — ADR-014's no-network-at-render guarantee holds.
Weight range is 400–700, which covers every weight the UI uses; the system
stack remains the fallback. Note Google Sans Flex was considered and
rejected: as of this writing its Google Fonts build has no Hebrew subset.

## ADR-035 — שם המוצר: אוצר שלמה

The user named the system **אוצר שלמה**, replacing the working name "מאגר
הקשרים" / "Contacts". The rename is skin-deep by design — split across three
layers:

- **Hebrew everywhere a person reads.** `productName` itself is now the
  Hebrew name, so the Start Menu, the desktop shortcut, "Apps & features"
  and the installer all say אוצר שלמה (Tauri's NSIS is Unicode; our own
  Hebrew.nsh already proved the pipeline handles Hebrew). Window title, the
  app shell, the sidebar art and the settings footer follow.
- **ASCII where a filename travels.** `mainBinaryName` is pinned to
  `OtzarShlomo` so the process name and install artifacts stay
  filesystem-boring, and CI/release rename their outputs to
  `OtzarShlomoSetup.exe` / `OtzarShlomo_<version>_x64-setup.exe` — GitHub
  mangles non-ASCII asset names.
- **Nothing where data lives.** The identifier `digital.baram.yanuka`, the
  credential-store service name, the database path and the crate/package
  names do not change. A rename that moved the appdata directory or the
  encryption key would be a data-loss event for a branding request.

One real migration exists: Tauri keys the uninstall registry entry by
product name, so the new installer would not see the old "Contacts" install
and Windows would list both. A `NSIS_HOOK_PREINSTALL` hook
(`windows/hooks.nsh`) runs the old entry's own uninstaller silently first;
the data survives because of the identifier decision above. The hook is
worth its weight exactly once — it stays until we are confident no machine
still runs a pre-0.6.0 install.

## ADR-036 — חיפוש סמנטי מקומי: multilingual-e5-small על ONNX Runtime

The next stage after 0.6.x, chosen by the priority list: priority 3 ("החיפוש
טוב יוצא דופן") names the one capability still missing — finding the note
that says `יהודי מלונדון… יכול לסייע בבניית בתי כנסת` from the query
`עסקן מאנגליה שעוזר עם בתי כנסת`, where FTS scores zero. Sync needs a server
decision and OCR needs the user's scanned notebooks; semantic search needs
neither, and it serves the product's single criterion directly.

**Model.** Three candidates were evaluated on a 9-query Hebrew paraphrase set
built from the product's own domain (recommendations, sofrim, community
figures):

| model | top-1 | top-3 | size | runtime |
|---|---|---|---|---|
| potion-multilingual-128M (static, MIT) | 6/9 | 7/9 | ~128MB int8 | pure Rust |
| multilingual-e5-small fp32 (MIT) | 7/9 | 9/9 | 470MB | ONNX Runtime |
| **multilingual-e5-small int8 (MIT)** | **8/9** | **9/9** | **118MB** | ONNX Runtime |

The int8 quantization is the official one from the model's repository, pinned
by revision and sha256 in `scripts/fetch-semantic-model.mjs`; it lost nothing
measurable (it gained a case on this set) and embeds a text in ~5ms on CPU.
ONNX Runtime is linked *statically* by `ort`'s pyke binaries — no DLL to
ship — and the model rides in the installer as a Tauri resource (~120MB,
the price of priority 2: no network, ever, including at first run).

**Design.**
- One vector per *document* — a contact's profile (the FTS fields composed as
  readable text) or a single note — in `semantic_index`, inside the same
  SQLCipher file, so vectors are encrypted and backed up with the data they
  describe.
- Maintained by *reconciliation*, not triggers: desired docs are derived from
  the live tables and diffed against the index by content hash. The same
  routine serves the post-mutation sync (one contact, inline, ~10ms), the
  background catch-up on first launch (budgeted steps that yield the DB lock),
  and a future model swap (the `model` column invalidates by comparison).
  Global taxonomy edits (renaming a tag) leave stale profile vectors until the
  next launch's catch-up — accepted, they still match *better* than before the
  rename reached them.
- Query side is layer 4 of search: additive only, gated like fuzzy, capped at
  8 candidates above cosine 0.80, scored strictly below a direct lexical hit
  (`semantic` weight 45 × 0.5), and each hit carries its matching note as the
  snippet with the label "לפי משמעות".
- Degradation is a state, not an error: no model file → search is exactly
  what it was in 0.6.x and settings says so. The browser build reports
  `browser`, mirroring encryption's pattern.

**Rejected:** passing notes to a hosted embedding API (violates the no-network
constraint and sends the archive's most sensitive text off the machine — not
negotiable); a vector extension (brute force over a few thousand 384-float
vectors is milliseconds; an ANN index is complexity without a customer);
Google Sans-style dual sourcing of the model (one pinned artifact, verified
by checksum, is the whole supply chain).

## ADR-037 — ייבוא מחברות: זיכרון כתב־יד לומד, לא מודל מוכן

The archive's origin is handwritten notebooks, and the user asked for OCR
that *learns the writer's difficult hand over time*. A survey of open
Hebrew handwriting models found nothing shippable — a handful of
experimental fine-tunes with double-digit downloads, several marked
untrained/synthetic, and multi-GB VLM adaptations unusable on an old
offline Windows machine. Cloud OCR is excluded outright: the notebooks are
the most sensitive artifact this product touches.

So recognition is not a pretrained model but a **memory of this writer**:

- Segmentation is classical and transparent — Otsu binarization, projection
  profiles for line bands, gap analysis for word boxes, right-to-left. Its
  failure mode (a merged or split box) is visible in the workbench, not
  silently wrong.
- Every word the user transcribes is stored as (shape descriptor → text) in
  `ocr_corrections`. The descriptor is a 48×16 ink-density grid over the
  tight-trimmed word, L2-normalized; matching is cosine. One writer shapes
  one word consistently, and notebook vocabulary repeats — so a word
  corrected once auto-fills on the same page immediately and on every page
  imported after, marked "זוהה מהכתב" (threshold 0.92: a wrong auto-fill
  costs trust, an empty box costs keystrokes).
- The archive doubles as the lexicon: autocomplete offers names, cities,
  professions and previously corrected words while typing.
- A transcribed page becomes a normal note (journaled, FTS-indexed,
  semantically embedded) on the contact the user chooses; the scan itself
  lives as a BLOB *inside* the encrypted database, so pages are protected
  and backed up with the data they belong to.

Two hard-won pixels: Otsu's threshold lands *on* the dark class in a clean
scan, so the ink test is `<=`, not `<`; and a word's descriptor must be
trimmed against a padded window rather than its line band, because band
height is a property of the whole line — the same word on two lines was
clipped differently and read as two shapes.

The corrections table is also a growing labeled dataset of exactly this
hand. When it is large enough, fine-tuning a small HTR model *on this
machine* becomes the natural next iteration — the workbench then starts
from that model's proposals instead of empty boxes, with the same
correction loop on top.
