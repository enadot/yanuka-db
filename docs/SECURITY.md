# SECURITY

The data is private and sensitive: names, phone numbers, home cities, and
free-text remarks about real people written on the assumption nobody else would
read them. Privacy is not a later feature — it constrains the architecture from
the first commit.

**Status: the model is designed and the schema is in place. Enforcement and
encryption are deferred, and it is stated plainly below which is which.**

## Threat model

What this is actually defending against, in rough order of likelihood:

1. **A lost or stolen laptop.** The primary risk. The database is a file on a
   Windows machine that travels.
2. **Another user on a shared machine** reading the file directly.
3. **Backups leaking** — copied to a USB stick or a cloud folder.
4. **A compromised or malicious webview** driving the IPC surface.
5. **Contact data reaching a third party**, including an AI service.

Not in scope now: a hostile server (there is no server), and a targeted attacker
with administrator access to a running machine.

## What is enforced today

**No secrets in the repository.** No credentials, tokens or keys are committed.
`.gitignore` excludes `.env*`, `*.pem`, `*.key`, `signing/`, and — importantly —
`*.db`, `*.sqlite` and `backups/`, so a real contact database can never be
committed by accident.

**No contact data leaves the machine.** There is no network code. Fonts are the
system stack rather than a CDN, partly for offline correctness and partly
because a font request leaks that the application is running.

**A locked-down webview.** The Tauri CSP is `default-src 'self'` with no
external origins permitted at all.

**The webview is not trusted.** Every command revalidates its arguments in Rust
(`repository::validate`). Client-side validation is a convenience for the user,
never a control. No SQL crosses the IPC boundary — the frontend names an
operation and passes typed arguments, so there is no query-injection surface by
construction.

**No logging of contact data.** Errors carry codes and messages, not record
contents.

## Permissions model

Defined and implemented as a pure function; **not yet enforced**, because the
desktop is single-user and local. ADR-020.

Permissions are the unit of authorization; roles are named bundles. Application
code always tests a permission, never a role, so the role set can change without
touching call sites.

```
super_admin · admin · editor · viewer · restricted_viewer
```

`restricted_viewer` deliberately lacks `contacts:view_phones` and
`contacts:view_sensitive`: it can find a person and read the context, but not
dial them or read private remarks.

Two details worth keeping when this is switched on:

- **Denial wins.** A permission explicitly taken from a user must not be
  restorable by their role or by an extra grant. The safe direction for a
  revocation is always "off".
- **Redaction happens at the repository boundary**, in `redactForPrincipal`,
  not in the view. A screen that forgets to check must not be able to leak a
  phone number.

## Sensitive notes

`notes.is_sensitive` exists in the schema and is respected by
`redactForPrincipal`. It becomes meaningful when there is more than one user.

## Audit log

`audit_log` is append-only — never updated or deleted by application code —
recording who did what, when, from which device, to which record. Written today,
surfaced in the UI when there are multiple users to distinguish.

## Encryption at rest — enabled (ADR-033)

**On, and transparent.** The shipped Windows desktop builds with the
`sqlcipher` feature: the database file is SQLCipher-encrypted (AES-256), and a
pre-encryption database is upgraded in place on first launch — exported into a
staging file that is opened, integrity-checked and row-counted **before** the
swap, so no failure mode touches the plaintext original.

**The key is random, not a passphrase.** 256 bits from the OS, held in the
Windows credential store (`keyring`), applied as a raw key (`x'…'`, no KDF —
stretching an already-random key buys nothing). A passphrase was rejected
deliberately: forgetting it would turn priority 6 into a violation of
priority 1, losing the database and every backup at once. Settings surfaces
the key as a **recovery key** with the instruction to keep it off the
machine; that key is the reinstall/new-machine story, entered once on a
dedicated unlock screen and then re-persisted.

**Backups are keyed.** `backup_to`/`daily_backup` key the destination with
the live database's key — the online-backup API writes through the
destination's codec, so an unkeyed destination would have silently produced
plaintext copies. The pre-migration copy is a byte copy of the encrypted
file and needs no handling. `PRAGMA temp_store = MEMORY` keeps SQLCipher
from spilling plaintext temporaries.

**Degradation is visible, not silent.** No credential store (a Linux/macOS
development build) or a failed upgrade opens the database unencrypted and
says so in settings — the data must open (priorities 1–2) even when
encryption cannot be had. An encrypted file whose key is absent is the one
blocking state: every IPC command answers `locked` until the recovery key
opens it.

What this does **not** defend: an attacker running as the logged-in Windows
user can read the credential store entry. That is threat-model consistent —
threats 1–3 (stolen machine, another account, leaked backup copies) are
covered; a targeted attacker with the user's own session was out of scope
from the start.

## In transit

When the server exists: TLS only, certificate validation never disabled, tokens
in the OS credential store rather than a file.

## Rules

Never:

- hard-code credentials, or commit any secret
- log passwords, tokens or contact details
- send database contents to an AI API
- disable TLS verification
- trust the webview
- write a migration that can lose data

Always:

- take a backup before migrating
- keep both versions when a merge is ambiguous
- validate on the Rust side regardless of what the client did

## Known gaps

Stated plainly rather than left implicit:

| gap | consequence | tracked |
|---|---|---|
| Database not encrypted | a stolen laptop exposes everything | ADR-018 |
| Permissions not enforced | irrelevant while single-user; blocking for multi-user | ADR-020 |
| Installer unsigned | SmartScreen warns on first run | ADR-021 |
| Audit log not surfaced | written but not readable in the UI | ADR-020 |
