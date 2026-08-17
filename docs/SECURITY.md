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

## Encryption at rest — deferred, but designed for

**Not enabled.** The database file is currently unencrypted. This is the largest
open item against threat 1, and it is a deliberate, dated decision rather than an
oversight (ADR-018).

What has already been done so that enabling it is a contained change:

- **Every connection is opened through one function**, `yanuka_db::open(path,
  key)`, which already takes a key parameter and rejects it with
  `MissingCapability` unless the `sqlcipher` feature is compiled. No call site
  will need to change.
- **`PRAGMA temp_store = MEMORY` is already set.** Without it SQLCipher spills
  plaintext temporary files to disk — the classic way an encrypted database
  leaks anyway.
- The cargo feature exists: `sqlcipher = ["rusqlite/bundled-sqlcipher-vendored-openssl"]`.

What remains: key derivation (Argon2id over a passphrase, or the OS keychain via
`keyring`), a plaintext→encrypted upgrade path (`ATTACH … KEY …;
SELECT sqlcipher_export('enc');`), and accepting an OpenSSL build on Windows CI
that adds roughly ten minutes.

**Backups inherit whatever the database has.** They are unencrypted today
because the database is. When encryption lands, the backup copy is already a
byte copy of the encrypted file and needs no separate handling.

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
