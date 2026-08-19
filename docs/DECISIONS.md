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
