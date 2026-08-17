# מאגר הקשרים — offline-first smart contacts database

A contacts database built around one job: **find a person when the name is
forgotten** and all that remains is a profession, a city, who made the
introduction, or a sentence someone wrote years ago.

Hebrew-first, RTL, and fully functional without a network connection.

> Read [`docs/PRODUCT.md`](docs/PRODUCT.md) first — it explains why this is not
> a phone book, and what that changes.

## Quick start

```bash
pnpm install
pnpm build

# The full application in a browser, backed by the in-memory repository and
# 56 fictional contacts. No Rust toolchain required.
pnpm --filter @yanuka/desktop dev
```

Then try:

| type this | to find |
|---|---|
| `סופר סתם ירושלים` | a scribe, by trade and city |
| `בורו פארק` | someone by a phrase inside a note |
| `פרידמאן` | `יעקב פרידמן`, despite the misspelling |
| `Friedman` | the same person, by Latin transliteration |
| `+972 54 555 0134` | a contact, by a number in a different format |
| `Ctrl` + `K` | the command palette, from anywhere |

The desktop application (Tauri + SQLite) is built by CI on Windows and produced
as `ContactsSetup.exe`.

## Layout

```
apps/desktop        React + shadcn/ui frontend, and the Tauri shell
crates/yanuka-db    rusqlite storage, migrations, FTS5 search    (no Tauri dep)
crates/yanuka-search Hebrew normalization and ranking            (no Tauri dep)
packages/           types · validation · utils · search · database · core · ui
docs/               architecture, database, search, sync, security, decisions
```

## Verification

```bash
pnpm lint && pnpm typecheck && pnpm test   # 133 TypeScript tests
cargo test -p yanuka-db -p yanuka-search   # 29 Rust tests
pnpm --filter @yanuka/desktop test:e2e     # 11 Playwright tests in Chromium
```

The migration tests run the **real** production `.sql` against real SQLite via
Node's built-in `node:sqlite`, so the schema exercised in CI is byte-for-byte
the schema the desktop ships.

### ⚠ Do not run `cargo build --workspace` on Linux without webview packages

`apps/desktop/src-tauri` links against the system webview. On Linux that means
`libwebkit2gtk-4.1-dev`, and without it the whole workspace build fails.

This is exactly why the storage and search code lives in separate crates:

```bash
cargo test -p yanuka-db -p yanuka-search   # ✓ works anywhere
cargo check --workspace                    # ✗ needs libwebkit2gtk-4.1-dev
```

CI covers the shell on both `ubuntu-latest` (with the apt packages) and
`windows-latest`.

## Building the Windows installer

Needs Rust and the Tauri prerequisites on a Windows machine:

```bash
pnpm --filter @yanuka/desktop tauri build
```

The NSIS output lands in `target/release/bundle/nsis/`; CI copies it to
`ContactsSetup.exe`. The installer is **unsigned**, so SmartScreen warns on
first run — see `docs/DECISIONS.md` ADR-021.

## The design rules worth knowing before editing

- **UI is shadcn/ui only.** Every element is a registry component composed with
  Tailwind; app-specific pieces are built from those primitives. No second
  component library. ADR-015.
- **`ContactsRepository` is the only data boundary.** Nothing above it mentions
  SQL, IPC or Tauri — that is what lets the same screens run against SQLite, an
  in-memory store and, later, Postgres.
- **RTL needs the Radix `DirectionProvider`**, not just `dir="rtl"`. ADR-013.
- **The normalizer exists in TypeScript and Rust** and is held to a shared
  fixture. Changing one without the other breaks searchability across clients.
- **Migrations are forward-only.** Never edit a shipped migration; the runner
  checksums them and will refuse to start.

## Status

Built and verified: the data model, the search engine, the local SQLite layer,
and the Desktop MVP (search, list, detail, add, edit).

Designed and deliberately deferred, each with its cost recorded in
[`docs/DECISIONS.md`](docs/DECISIONS.md): the sync transport, permission
enforcement, encryption at rest, CSV/OCR import, local semantic search, and the
web and Android clients.
