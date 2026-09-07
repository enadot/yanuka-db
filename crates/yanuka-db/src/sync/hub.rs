//! The server side of the protocol, as functions over a connection.
//!
//! The hub is an ordinary database of the same schema that happens to be
//! always on. It accepts a pushed revision when the device's base is the
//! version it holds, applies the state to its own tables — so it can search
//! and serve the archive to thinner clients later — and appends the change
//! to `sync_log`, the stream every device pulls. It refuses a stale base
//! rather than merging: merging is the device's job, where the person who
//! can resolve a conflict is.

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::json;

use super::state;
use super::{Change, PullPage, PushItem, PushResult};
use crate::error::{DbError, Result};
use crate::mutation::{self, Operation};
use crate::now_iso;

fn current_version(connection: &Connection, entity_type: &str, entity_id: &str) -> Result<i64> {
    Ok(connection
        .query_row(
            "SELECT server_version FROM sync_revisions WHERE entity_type = ?1 AND entity_id = ?2",
            params![entity_type, entity_id],
            |row| row.get(0),
        )
        .optional()?
        .unwrap_or(0))
}

/// Take pushed items from one device. Each is its own transaction so one
/// refusal (or one dangling reference) never drags the others down.
pub fn accept(
    connection: &mut Connection,
    device_id: &str,
    items: &[PushItem],
) -> Result<Vec<PushResult>> {
    let mut results = Vec::with_capacity(items.len());
    for item in items {
        results.push(accept_one(connection, device_id, item)?);
    }
    super::devices::mark_synced(connection, device_id)?;
    Ok(results)
}

fn accept_one(connection: &mut Connection, device_id: &str, item: &PushItem) -> Result<PushResult> {
    if state::table(&item.entity_type).is_none() {
        return Ok(PushResult {
            entity_type: item.entity_type.clone(),
            entity_id: item.entity_id.clone(),
            accepted: false,
            version: 0,
            error: Some("סוג רשומה לא מוכר".into()),
        });
    }
    let current = current_version(connection, &item.entity_type, &item.entity_id)?;
    if item.base_version != current {
        return Ok(PushResult {
            entity_type: item.entity_type.clone(),
            entity_id: item.entity_id.clone(),
            accepted: false,
            version: current,
            error: None,
        });
    }

    let previous = state::load(connection, &item.entity_type, &item.entity_id)?;
    let tx = connection.transaction()?;
    tx.execute_batch("PRAGMA defer_foreign_keys = ON")?;
    let applied = state::apply(&tx, &item.entity_type, &item.entity_id, &item.state, device_id);
    let version = current + 1;
    let now = now_iso();
    let outcome = applied.and_then(|_| {
        tx.execute(
            "INSERT INTO sync_revisions (entity_type, entity_id, server_version, synced_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(entity_type, entity_id) DO UPDATE SET
               server_version = excluded.server_version, synced_at = excluded.synced_at",
            params![item.entity_type, item.entity_id, version, now],
        )?;
        tx.execute(
            "INSERT INTO sync_log (entity_type, entity_id, version, changed, state, device_id,
                                   created_at, received_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                item.entity_type,
                item.entity_id,
                version,
                serde_json::to_string(&item.changed)?,
                serde_json::to_string(&item.state)?,
                device_id,
                item.created_at,
                now
            ],
        )?;
        // The hub keeps a history too, under the device that made the change.
        let operation = if previous.is_none() { Operation::Create } else { Operation::Update };
        let payload = state::project(&item.state, &item.changed);
        let before = previous.as_ref().map(|state| state::project(state, &item.changed));
        let id = mutation::record(
            &tx,
            mutation::NewMutation {
                entity_type: &item.entity_type,
                entity_id: &item.entity_id,
                operation,
                payload: Some(&payload),
                previous: before.as_ref(),
                base_version: current,
                device_id,
            },
        )?;
        tx.execute(
            "UPDATE mutations SET status = 'synced', synced_at = ?2 WHERE id = ?1",
            params![id, now],
        )?;
        Ok(())
    });
    match outcome.and_then(|_| tx.commit().map_err(DbError::from)) {
        Ok(()) => Ok(PushResult {
            entity_type: item.entity_type.clone(),
            entity_id: item.entity_id.clone(),
            accepted: true,
            version,
            error: None,
        }),
        Err(DbError::Sqlite(rusqlite::Error::SqliteFailure(code, _)))
            if code.code == rusqlite::ErrorCode::ConstraintViolation =>
        {
            Ok(PushResult {
                entity_type: item.entity_type.clone(),
                entity_id: item.entity_id.clone(),
                accepted: false,
                version: current,
                error: Some("missing_reference".into()),
            })
        }
        Err(DbError::Validation(message)) => Ok(PushResult {
            entity_type: item.entity_type.clone(),
            entity_id: item.entity_id.clone(),
            accepted: false,
            version: current,
            error: Some(message),
        }),
        Err(error) => Err(error),
    }
}

/// A page of the stream after `after`, without the requesting device's own
/// entries. `next` advances over everything scanned, skipped rows included,
/// so a device that pushed a thousand records does not re-read them.
pub fn pull(
    connection: &Connection,
    after: i64,
    limit: usize,
    exclude_device: &str,
) -> Result<PullPage> {
    let limit = limit.clamp(1, 1000) as i64;
    let mut statement = connection.prepare(
        "SELECT l.seq, l.entity_type, l.entity_id, l.version, l.changed, l.state, l.device_id,
                l.created_at, d.name
           FROM sync_log l LEFT JOIN devices d ON d.id = l.device_id
          WHERE l.seq > ?1 ORDER BY l.seq LIMIT ?2",
    )?;
    let rows = statement.query_map(params![after, limit + 1], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, String>(6)?,
            row.get::<_, String>(7)?,
            row.get::<_, Option<String>>(8)?,
        ))
    })?;
    let scanned: Vec<_> = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    let has_more = scanned.len() as i64 > limit;
    let window: Vec<_> = scanned.into_iter().take(limit as usize).collect();
    let next = window.last().map(|row| row.0).unwrap_or(after);
    let changes = window
        .into_iter()
        .filter(|row| row.6 != exclude_device)
        .map(
            |(
                seq,
                entity_type,
                entity_id,
                version,
                changed,
                state,
                device_id,
                created_at,
                name,
            )| {
                Ok(Change {
                    seq,
                    entity_type,
                    entity_id,
                    version,
                    changed: serde_json::from_str(&changed)?,
                    state: serde_json::from_str(&state)?,
                    device_id,
                    device_name: name,
                    created_at,
                })
            },
        )
        .collect::<Result<Vec<_>>>()?;
    Ok(PullPage { changes, next, has_more })
}

/// The latest sequence number, for status displays.
pub fn head(connection: &Connection) -> Result<i64> {
    Ok(connection.query_row("SELECT COALESCE(MAX(seq), 0) FROM sync_log", [], |row| row.get(0))?)
}

/// What the server's status endpoint reports.
pub fn summary(connection: &Connection) -> Result<serde_json::Value> {
    let count = |sql: &str| -> Result<i64> { Ok(connection.query_row(sql, [], |row| row.get(0))?) };
    Ok(json!({
        "contacts": count("SELECT COUNT(*) FROM contacts WHERE deleted_at IS NULL")?,
        "devices": super::devices::active_count(connection)?,
        "head": head(connection)?,
        "lastChangeAt": connection
            .query_row("SELECT MAX(received_at) FROM sync_log", [], |row| row.get::<_, Option<String>>(0))?,
    }))
}
