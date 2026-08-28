<p align="center">
  <img src="apps/desktop/public/logo.png" alt="אוצר שלמה" width="160">
</p>

# אוצר שלמה — offline-first smart contacts database

A contacts database built around one job: **find a person when the name is
forgotten** and all that remains is a profession, a city, who made the
introduction, or a sentence someone wrote years ago.

Hebrew-first, RTL, and fully functional without a network connection.

> Read [`docs/PRODUCT.md`](docs/PRODUCT.md) first — it explains why this is not
> a phone book, and what that changes.

## Quick start

```bash
pnpm install

# The full application in a browser, backed by the in-memory repository and
# 56 fictional contacts. No Rust toolchain required — the workspace packages
# it depends on are built automatically first.
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

The desktop application (Tauri + SQLite) is built by CI on Windows; every
tagged version is published on the repository's **Releases** page with the
installer attached.

## Layout

```
apps/desktop        React + shadcn/ui frontend, and the Tauri shell
crates/yanuka-db    rusqlite storage, migrations, FTS5 search    (no Tauri dep)
crates/yanuka-search Hebrew normalization and ranking            (no Tauri dep)
packages/           types · validation · utils · search · database · core · ui
docs/               architecture, database, search, sync, security, decisions
```

## Importing an existing archive

הגדרות ← ייבוא מקובץ. קבצי CSV מ־Google Contacts, מ־Outlook או מ־Excel; זיהוי
הכותרות אוטומטי, המיפוי ניתן לתיקון לפני הכתיבה, ושורות שלא ניתן לייבא (ללא שם)
מדווחות בסיכום במקום לעצור את השאר. ADR-026.

אחרי ייבוא ממקורות שונים: הגדרות ← איתור כפילויות. הסריקה משווה טלפונים,
אימיילים ושמות; מיזוג מעביר הכול לרשומה הנשמרת, משמר שדות סותרים בהערות,
ורושם את הרשומה הממוזגת במלואה ביומן השינויים. ADR-027.

## Verification

```bash
pnpm lint && pnpm typecheck && pnpm test   # every TypeScript suite
cargo test -p yanuka-db -p yanuka-search   # storage and search crates
pnpm --filter @yanuka/desktop test:e2e     # Playwright in real Chromium
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

## Running on Windows

### No build needed: download the installer

Every released version sits on the repository's **Releases** page as
`OtzarShlomo_<version>_x64-setup.exe`, with that version's changelog as the
release notes. Building locally is only needed for development.

### Prerequisites

| | |
|---|---|
| [Node.js 22 LTS](https://nodejs.org) | then `corepack enable` to get pnpm |
| [Rust](https://rustup.rs) (stable, MSVC toolchain) | rustup's default on Windows |
| [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) | tick **Desktop development with C++** |
| WebView2 | already present on Windows 10 1803+ and Windows 11 |

### Run it

```powershell
git clone https://github.com/enadot/yanuka-db.git
cd yanuka-db
pnpm install

pnpm --filter @yanuka/desktop tauri dev     # the real desktop app, live SQLite
```

The first `tauri dev` compiles SQLite and the Rust dependencies and takes
several minutes; afterwards it is incremental.

### Build the installer

```powershell
pnpm --filter @yanuka/desktop tauri build
```

The NSIS output lands in `target\release\bundle\nsis\`; CI copies it to
`OtzarShlomoSetup.exe`. The installer is **unsigned**, so SmartScreen shows
"Windows protected your PC" on first run — *More info* → *Run anyway*. See
`docs/DECISIONS.md` ADR-021.

The installer's filename carries the product version, and the settings
screen shows the same number — how versions are chosen and bumped is
[`docs/VERSIONING.md`](docs/VERSIONING.md); what changed in each one is
[`CHANGELOG.md`](CHANGELOG.md).

### Where the data lives

`%APPDATA%\digital.baram.yanuka\contacts.db`. Deleting that file resets the
application to empty. Beside it in `backups\`: pre-migration copies (three
kept) and automatic daily backups (`daily-*.db`, seven kept, taken on launch).
הגדרות ← "גיבוי עכשיו" snapshots to any path — e.g. a USB stick — and
"ייצוא CSV" writes a file that Excel opens and the import screen re-imports
with the mapping already detected. ADR-028.

### No Rust installed?

The whole UI runs in a browser against the in-memory repository and the demo
data — no Rust, no Tauri, nothing to compile:

```powershell
pnpm --filter @yanuka/desktop dev
```

Changes are not saved between runs; everything else behaves identically.

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
and the desktop application — search, list, contact card with notes and
relationships written in place, add/edit, CSV import, duplicate detection and
merge, automatic backups and CSV export, and encryption at rest (SQLCipher,
with a recovery key instead of a passphrase — ADR-033).

Designed and deliberately deferred, each with its cost recorded in
[`docs/DECISIONS.md`](docs/DECISIONS.md): the sync transport, permission
enforcement, OCR import, local semantic search, and the
web and Android clients.
