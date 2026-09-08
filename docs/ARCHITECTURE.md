# ARCHITECTURE

## The shape

```
┌─────────────────────────────────────────────────────────────┐
│  apps/desktop            React + shadcn/ui, RTL Hebrew      │
│      │                                                       │
│      │  ContactsRepository  ← the only boundary that matters │
│      ├──────────────┬───────────────────────────────────────┤
│      │              │                                        │
│  TauriRepository   MockRepository                            │
│      │              │  (in-memory, real search engine)       │
│      │ IPC          └─→ browser, tests, design review        │
│      ▼                                                       │
│  src-tauri (thin)                                            │
│      ▼                                                       │
│  crates/yanuka-db ──→ crates/yanuka-search                   │
│      ▼         └── sync engine ──HTTP──▶ server/yanuka-server│
│  SQLite  (bundled, FTS5 + trigram, WAL)      (same crate,    │
│                                               same schema)   │
└─────────────────────────────────────────────────────────────┘

packages/  types · validation · utils · search · database · core · ui · config
```

## The one decision everything else follows from

**`ContactsRepository` (`packages/core/src/repository.ts`) is the only place the
application touches data, and it never mentions SQL, IPC or Tauri.**

Everything above that interface — screens, hooks, domain logic, ranking — is
platform independent. That is what lets the same code run against SQLite on the
desktop today and against Postgres from a browser later. The moment a raw query
string crosses that line, the web and mobile clients stop being able to reuse
anything above it.

Two implementations exist, and both must pass `runRepositoryContractTests`:

- `TauriRepository` — IPC to the Rust shell, real local database.
- `MockRepository` — in-memory, but running the *real* search engine over the
  *real* demo dataset. Not a stub: it is what makes the app fully usable in a
  plain browser, which is how it is tested in CI.

## Why the Rust side is three crates

Tauri links against the system webview. On Linux that means `libwebkit2gtk`,
which is absent from most CI images and from this project's development
container. If the storage and search code lived inside the Tauri crate, none of
it could be compiled or tested there.

So it does not:

| crate | depends on Tauri | testable anywhere |
|---|---|---|
| `crates/yanuka-search` | no | yes |
| `crates/yanuka-db` | no | yes |
| `server/yanuka-server` | no | yes |
| `apps/desktop/src-tauri` | yes | CI only (Windows shell, Android shell) |

```bash
cargo test -p yanuka-db -p yanuka-search   # works everywhere
cargo check --workspace                    # needs libwebkit2gtk-4.1-dev
```

The shell is deliberately thin — it opens the database, runs migrations, takes a
backup, and marshals arguments. No SQL and no business logic. Everything
interesting is one crate down, where it can be tested.

## The normalizer exists twice, on purpose

`normalize_text` is implemented in TypeScript (`packages/search`) and in Rust
(`crates/yanuka-search`). The desktop indexes with the Rust one; the browser,
the mock repository and any future web client query with the TypeScript one.

They must agree **exactly** — a contact indexed under one spelling would be
unreachable under the other if they drifted. The contract is a shared fixture,
`packages/search/fixtures/normalization.cases.json`, read by both `cargo test`
and `vitest`. Neither implementation can change behaviour without editing that
file, which makes the divergence visible in review.

Compiling the Rust normalizer to wasm and using it from TypeScript would remove
the duplication, and is the right long-term answer. It is deferred because it
adds a wasm toolchain and a build artifact to every package that searches; see
DECISIONS.md ADR-006.

## The SQL lives in one place

`packages/database/migrations/sqlite/*.sql` is the single source of truth.

- Rust embeds it with `include_str!` at compile time (`crates/yanuka-db/src/migrate.rs`).
- TypeScript inlines the same text via a generator script and runs it through
  Node's built-in `node:sqlite` in tests.

So `pnpm test` exercises byte-for-byte the schema the desktop ships. An ORM
schema definition would have added a *third* representation to keep in sync
rather than removing one — see DECISIONS.md ADR-003.

## Packages

| package | holds | depends on |
|---|---|---|
| `@yanuka/types` | domain types, no runtime code | — |
| `@yanuka/utils` | ULID, phone, dates, edit distance, locale | types |
| `@yanuka/validation` | Zod schemas for every entity and query | types |
| `@yanuka/search` | normalization, query parsing, ranking, in-memory engine | types, utils |
| `@yanuka/database` | SQL migrations, demo dataset | types, utils |
| `@yanuka/core` | repository interface, permissions, duplicates, mock | all of the above |
| `@yanuka/ui` | shadcn/ui components and design tokens | — |
| `@yanuka/config` | tsconfig, eslint, shared setup | — |

The rule that keeps this honest: if a function is needed by both the desktop and
a future web client and it is platform-independent, it belongs in a package.
Platform-specific things — the SQLite driver, filesystem access, Windows APIs,
Android intents — stay in the app.

## The server

`server/yanuka-server` (ADR-039) is the always-on peer: axum over the same
`yanuka-db` crate, holding a copy of the archive in the same SQLite schema.
It has no logic of its own — `yanuka_db::sync::hub` accepts revisions and
serves the stream, `yanuka_db::sync::devices` pairs and authenticates — so
the whole protocol is tested in the storage crate without a socket, and the
server's test drives the real router through the desktop's own transport.
Deployment shapes are in `server/README.md`.

## Where the deferred pieces attach

None of these are built. The point of listing them is that none of them require
changing what exists.

- **Web client.** Next.js app consuming `@yanuka/core` with a
  `HttpRepository` against `yanuka-server`, whose copy of the archive is
  already searchable. Screens and ranking are reused as-is; only the
  repository implementation is new — and this is where permissions
  (ADR-020) start being enforced.
- **Android** shipped in 0.11.0 (ADR-040) as the same Tauri shell: the
  React screens in the system WebView, `yanuka-db` cross-compiled for
  arm64, the phone layout chosen by viewport. `gen/android` holds the
  generated Android project; `tauri.android.conf.json` drops the bundled
  model; Cargo target tables leave the semantic and SQLCipher features out
  of the phone build.
- **Postgres.** Not needed at this scale (ADR-039); if it ever is, a second
  migrations directory under `packages/database/migrations/postgres`, with
  the dialect differences contained inside the repository implementation.
  See DATABASE.md.

`apps/web`, `apps/mobile` and `server/` are deliberately *not* created as empty
placeholders — empty packages slow the build, distort coverage and rot. Their
shape is documented here instead; ADR-017.

## Verification

| what | how | runs where |
|---|---|---|
| unit + integration (TS) | `pnpm test` — 133 tests | anywhere |
| migrations + FTS5 | `node:sqlite` in vitest | anywhere |
| storage + search (Rust) | `cargo test -p yanuka-db -p yanuka-search` — 54 tests | anywhere |
| the sync protocol | two devices + a server in one process (`tests/sync.rs`) | anywhere |
| the server over HTTP | `cargo test -p yanuka-server` — real socket, real transport | anywhere |
| normalizer conformance | shared JSON fixture, both languages | anywhere |
| IPC name parity | regex over `commands.rs` vs `IPC_COMMANDS` | anywhere |
| the real UI | Playwright + Chromium — 26 desktop + 3 phone-viewport tests | anywhere |
| the Android APK builds | `tauri android build` (composite action) | CI (ubuntu + SDK/NDK) |
| the Tauri shell compiles | `cargo check -p yanuka-desktop` | CI (ubuntu + apt deps) |
| the installer builds | `tauri build` | CI (windows-latest) |
