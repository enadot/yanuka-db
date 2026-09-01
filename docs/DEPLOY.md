# DEPLOY — the sync server

The server exists so a second computer and a phone can hold the same archive.
It is a courier: an ordered log of sealed envelopes. It cannot read a contact,
and nothing in its code gives it the means to.

This is the only part of the system that runs anywhere but the user's own
machines, so it is the only part where somebody else's mistake could expose
something. Everything below follows from that.

## What the server can and cannot see

| Can see | Cannot see |
|---|---|
| How many changes exist, and when each arrived | Any name, phone number, address or note |
| Which device sent each change | What the change was |
| The size of each change | Anything about who the contacts are |

The payload is sealed with XChaCha20-Poly1305 on the device, under a key
derived from a passphrase that is never transmitted. A dump of the database, a
subpoena served on the provider, or a mistake in the platform's access control
yields opaque blobs.

**A lost passphrase cannot be recovered.** Nobody is holding a copy. Losing it
costs the ability to sync — not the archive, which lives unencrypted on each
machine and in the daily backups.

## Prerequisites

- A [Fly.io](https://fly.io) account and `flyctl` installed.
- Roughly $5–15 a month for the smallest machine and a managed Postgres.

## First deployment

Run from the repository root.

```bash
fly launch --no-deploy --copy-config --config crates/yanuka-sync-server/fly.toml

fly postgres create --name yanuka-sync-db
fly postgres attach yanuka-sync-db        # sets DATABASE_URL for you

# The secret a device presents once, to enrol. Keep a copy — you need it on
# every machine you add, and it is not recoverable from the server.
fly secrets set YANUKA_ENROLMENT_SECRET="$(openssl rand -base64 32)"

fly deploy --config crates/yanuka-sync-server/fly.toml
```

Confirm it is alive:

```bash
curl https://<your-app>.fly.dev/health
# {"status":"ok","devices":0}
```

`/health` queries the database rather than returning a bare 200, so a container
that is up but cannot reach Postgres reports itself as unhealthy instead of
sitting in the load balancer swallowing pushes.

## The two secrets, and why there are two

| Secret | Where it lives | What it does |
|---|---|---|
| **Enrolment secret** | The server, as `YANUKA_ENROLMENT_SECRET`; and typed once on each device | Proves a device is allowed to join |
| **Data passphrase** | Only on the devices, never sent anywhere | Seals and opens the payloads |

Conflating them would be simpler to explain and would destroy the property that
justifies hosting this at all: if the server held the passphrase, the server
could read the archive.

A device presents the enrolment secret once, over TLS, and receives a long
random token. Afterwards it sends only that token. The server stores a SHA-256
hash of it, so a database dump does not let anyone speak as a device.

## Adding a device

The desktop produces a single **connection code** bundling the server URL, the
enrolment secret and the data key. Paste it into the new machine. Only the
enrolment half ever reaches the server.

Codes look like `yanuka1_aHR0cHM6...`. Treat one exactly as you would the
archive itself: anyone holding it has both halves.

## Operating it

```bash
fly logs --config crates/yanuka-sync-server/fly.toml
fly status --config crates/yanuka-sync-server/fly.toml
```

**Backups.** Fly's managed Postgres takes its own snapshots. Note that the
server's log is *also* a backup of the archive, sealed — a device that loses its
database can re-enrol and rebuild from the log, which is why a pull is not
filtered by device.

**Revoking a device.** A lost laptop is cut off by marking its row revoked:

```sql
UPDATE devices SET revoked_at = now() WHERE name = 'מחשב נייד';
```

Its token stops working immediately. Anything it already pulled is still on
that machine, unencrypted; revoking is not remote wipe.

**Rotating the enrolment secret.** `fly secrets set YANUKA_ENROLMENT_SECRET=…`.
Existing devices keep working — they hold tokens, not the secret. Only enrolling
a new device needs the new value.

## Costs of this design, stated plainly

- **No server-side search or web access.** The server cannot read the data, so
  there can be no thin web client that queries it. Every device is a full
  offline replica. That was the deliberate trade (ADR-035).
- **The provider sees traffic patterns.** How much you change and when is
  visible even though what you changed is not.
- **No conflict resolution on the server.** Two devices editing the same field
  produce a conflict resolved on a device, by a human. See SYNC.md.

## Running it somewhere else

Nothing here is Fly-specific except `fly.toml`. The container needs
`DATABASE_URL`, `YANUKA_ENROLMENT_SECRET` and a `PORT`, and it needs to be
behind TLS — the enrolment secret crosses the wire once in the clear otherwise.

```bash
docker build -f crates/yanuka-sync-server/Dockerfile -t yanuka-sync .
docker run -p 8080:8080 \
  -e DATABASE_URL="postgres://…" \
  -e YANUKA_ENROLMENT_SECRET="…" \
  yanuka-sync
```
