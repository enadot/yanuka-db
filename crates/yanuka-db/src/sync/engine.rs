//! The device side of the protocol: what to push, how to take in what the
//! server streams back, and the cycle that runs both.
//!
//! The engine never holds the database while it waits on the network. Every
//! step is a short function over a connection; `run_cycle` strings them
//! together through the `Database` abstraction so the shell can release its
//! lock between steps and a local write is never queued behind a slow link
//! (SYNC.md, rule 1).

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};

use super::config::{self, KEY_DEFERRED, KEY_LAST_AT, KEY_LAST_ERROR};
use super::state::{self, Applied};
use super::{rank, Change, PullPage, PushItem, PushResult, Transport};
use crate::categories::Meaning;
use crate::error::{DbError, Result};
use crate::mutation::{self, Operation};
use crate::now_iso;
use crate::repository::device_id;

/// Something the engine can borrow the database from, briefly, many times.
pub trait Database {
    fn with<T>(&self, f: impl FnOnce(&mut Connection) -> Result<T>) -> Result<T>;
}

impl Database for std::sync::Mutex<Connection> {
    fn with<T>(&self, f: impl FnOnce(&mut Connection) -> Result<T>) -> Result<T> {
        let mut guard = self.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        f(&mut guard)
    }
}

impl Database for std::cell::RefCell<Connection> {
    fn with<T>(&self, f: impl FnOnce(&mut Connection) -> Result<T>) -> Result<T> {
        f(&mut self.borrow_mut())
    }
}

/// What one cycle did, for the indicator and the log.
#[derive(Debug, Default, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CycleReport {
    pub pulled: usize,
    pub applied: usize,
    pub pushed: usize,
    pub rejected: usize,
    pub conflicts: usize,
    /// Contacts whose text changed — the desktop re-embeds these.
    pub touched_contacts: Vec<String>,
}

const PULL_LIMIT: usize = 200;
const PUSH_CHUNK: usize = 100;
const PUSH_ROUNDS: usize = 3;

// -- what the journal says is dirty -----------------------------------------

/// One record with unsent changes.
#[derive(Debug, Clone)]
pub struct Dirty {
    pub entity_type: String,
    pub entity_id: String,
    pub fields: BTreeSet<String>,
    pub mutation_ids: Vec<String>,
    pub created_at: String,
}

/// Translate a journal row into the record it concerns and the fields it
/// touched. `contact_category` rows are the contact's `categories` field;
/// a merge or a create touches everything; a delete touches `deletedAt`.
pub fn classify(
    entity_type: &str,
    entity_id: &str,
    operation: &str,
    payload: Option<&Value>,
) -> Option<(String, String, BTreeSet<String>)> {
    let all = |kind: &str| -> BTreeSet<String> {
        state::fields(kind).iter().map(|f| f.to_string()).collect()
    };
    let keys = |payload: Option<&Value>| -> BTreeSet<String> {
        payload
            .and_then(Value::as_object)
            .map(|object| object.keys().cloned().collect())
            .unwrap_or_default()
    };
    match entity_type {
        "contact_category" => {
            let contact = payload.and_then(|p| p.get("contactId")).and_then(Value::as_str)?;
            Some(("contact".into(), contact.to_string(), ["categories".to_string()].into()))
        }
        "contact" | "tag" | "category" | "organization" | "relationship" | "note" => {
            let mut fields = match operation {
                "create" => all(entity_type),
                "delete" => ["deletedAt".to_string()].into(),
                _ => keys(payload),
            };
            if fields.contains("mergedFrom") || fields.contains("mergedInto") {
                fields = all(entity_type);
            }
            if entity_type == "contact" && fields.remove("categoryIds") {
                fields.insert("categories".into());
            }
            // Keys that name a related record rather than a field of this one.
            fields.retain(|key| state::fields(entity_type).contains(&key.as_str()));
            if fields.is_empty() {
                fields = all(entity_type);
            }
            Some((entity_type.to_string(), entity_id.to_string(), fields))
        }
        _ => None,
    }
}

/// Records with open conflicts are held back from pushing until a person
/// decides (SYNC.md, rule 5).
fn held(connection: &Connection) -> Result<BTreeSet<(String, String)>> {
    let mut statement = connection
        .prepare("SELECT entity_type, entity_id FROM conflicts WHERE resolved_at IS NULL")?;
    let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
    Ok(rows.collect::<rusqlite::Result<BTreeSet<_>>>()?)
}

/// Every record with something to send: unsent journal rows, plus records
/// the server has never been told about at all (a first pairing, or a
/// record that predates the engine).
pub fn dirty(connection: &Connection) -> Result<Vec<Dirty>> {
    let held = held(connection)?;
    let mut by_entity: BTreeMap<(String, String), Dirty> = BTreeMap::new();

    let mut statement = connection.prepare(
        "SELECT id, entity_type, entity_id, operation, payload, created_at FROM mutations
          WHERE status IN ('pending', 'failed') ORDER BY id",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, Option<String>>(4)?,
            row.get::<_, String>(5)?,
        ))
    })?;
    for row in rows {
        let (id, entity_type, entity_id, operation, payload, created_at) = row?;
        let payload: Option<Value> = payload.and_then(|raw| serde_json::from_str(&raw).ok());
        let Some((kind, key, fields)) =
            classify(&entity_type, &entity_id, &operation, payload.as_ref())
        else {
            continue;
        };
        let entry = by_entity.entry((kind.clone(), key.clone())).or_insert_with(|| Dirty {
            entity_type: kind,
            entity_id: key,
            fields: BTreeSet::new(),
            mutation_ids: Vec::new(),
            created_at: created_at.clone(),
        });
        entry.fields.extend(fields);
        entry.mutation_ids.push(id);
    }

    for kind in super::ENTITY_ORDER {
        let table = state::table(kind).unwrap_or_default();
        let mut statement = connection.prepare(&format!(
            "SELECT t.id, t.created_at FROM {table} t
              WHERE NOT EXISTS (SELECT 1 FROM sync_revisions r
                                 WHERE r.entity_type = ?1 AND r.entity_id = t.id)
              ORDER BY t.created_at, t.id"
        ))?;
        let rows = statement.query_map(params![kind], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (id, created_at) = row?;
            let entry = by_entity.entry((kind.to_string(), id.clone())).or_insert_with(|| Dirty {
                entity_type: kind.to_string(),
                entity_id: id,
                fields: BTreeSet::new(),
                mutation_ids: Vec::new(),
                created_at,
            });
            entry.fields.extend(state::fields(kind).iter().map(|f| f.to_string()));
        }
    }

    let mut list: Vec<Dirty> = by_entity
        .into_iter()
        .filter(|(key, _)| !held.contains(key))
        .map(|(_, dirty)| dirty)
        .collect();
    list.sort_by(|a, b| {
        rank(&a.entity_type)
            .cmp(&rank(&b.entity_type))
            .then_with(|| a.created_at.cmp(&b.created_at))
            .then_with(|| a.entity_id.cmp(&b.entity_id))
    });
    Ok(list)
}

/// The fields this device has changed on one record since it last agreed
/// with the server — what a pulled change is merged against.
fn local_changes(
    connection: &Connection,
    entity_type: &str,
    entity_id: &str,
) -> Result<BTreeSet<String>> {
    let mut fields = BTreeSet::new();
    let mut statement = connection.prepare(
        "SELECT entity_type, entity_id, operation, payload FROM mutations
          WHERE status IN ('pending', 'failed', 'conflict')
            AND ((entity_type = ?1 AND entity_id = ?2)
                 OR (entity_type = 'contact_category'
                     AND json_extract(payload, '$.contactId') = ?2 AND ?1 = 'contact'))",
    )?;
    let rows = statement.query_map(params![entity_type, entity_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<String>>(3)?,
        ))
    })?;
    for row in rows {
        let (kind, id, operation, payload) = row?;
        let payload: Option<Value> = payload.and_then(|raw| serde_json::from_str(&raw).ok());
        if let Some((_, _, touched)) = classify(&kind, &id, &operation, payload.as_ref()) {
            fields.extend(touched);
        }
    }
    let known: Option<i64> = revision(connection, entity_type, entity_id)?;
    if known.is_none() {
        // Never agreed with the server: everything here is this device's word.
        fields.extend(state::fields(entity_type).iter().map(|f| f.to_string()));
    }
    Ok(fields)
}

fn revision(connection: &Connection, entity_type: &str, entity_id: &str) -> Result<Option<i64>> {
    Ok(connection
        .query_row(
            "SELECT server_version FROM sync_revisions WHERE entity_type = ?1 AND entity_id = ?2",
            params![entity_type, entity_id],
            |row| row.get(0),
        )
        .optional()?)
}

fn set_revision(
    connection: &Connection,
    entity_type: &str,
    entity_id: &str,
    version: i64,
) -> Result<()> {
    connection.execute(
        "INSERT INTO sync_revisions (entity_type, entity_id, server_version, synced_at)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(entity_type, entity_id) DO UPDATE SET
           server_version = excluded.server_version, synced_at = excluded.synced_at",
        params![entity_type, entity_id, version, now_iso()],
    )?;
    Ok(())
}

// -- push --------------------------------------------------------------------

/// Everything ready to send, with the journal rows each item settles.
pub fn prepare_push(connection: &Connection) -> Result<Vec<(PushItem, Vec<String>)>> {
    let mut items = Vec::new();
    for dirty in dirty(connection)? {
        let Some(state) = state::load(connection, &dirty.entity_type, &dirty.entity_id)? else {
            // Journaled but gone from the table: nothing to send. Settle the
            // rows so they stop counting as pending.
            settle(connection, &dirty.mutation_ids)?;
            continue;
        };
        let base_version = revision(connection, &dirty.entity_type, &dirty.entity_id)?.unwrap_or(0);
        items.push((
            PushItem {
                entity_type: dirty.entity_type,
                entity_id: dirty.entity_id,
                base_version,
                changed: dirty.fields.into_iter().collect(),
                state,
                created_at: dirty.created_at,
            },
            dirty.mutation_ids,
        ));
    }
    Ok(items)
}

fn settle(connection: &Connection, mutation_ids: &[String]) -> Result<()> {
    let now = now_iso();
    for id in mutation_ids {
        connection.execute(
            "UPDATE mutations SET status = 'synced', synced_at = ?2, last_error = NULL WHERE id = ?1",
            params![id, now],
        )?;
    }
    Ok(())
}

/// Record the server's verdicts. Accepted items settle their journal rows and
/// remember the new server version; refused ones wait for the next pull.
pub fn apply_push_results(
    connection: &mut Connection,
    sent: &[(PushItem, Vec<String>)],
    results: &[PushResult],
) -> Result<(usize, usize)> {
    let mut accepted = 0;
    let mut rejected = 0;
    let tx = connection.transaction()?;
    for (item, mutation_ids) in sent {
        let verdict = results
            .iter()
            .find(|r| r.entity_type == item.entity_type && r.entity_id == item.entity_id);
        match verdict {
            Some(result) if result.accepted => {
                set_revision(&tx, &item.entity_type, &item.entity_id, result.version)?;
                settle(&tx, mutation_ids)?;
                accepted += 1;
            }
            Some(result) => {
                rejected += 1;
                let reason = result.error.clone().unwrap_or_else(|| "הרשומה השתנתה בשרת".into());
                for id in mutation_ids {
                    tx.execute(
                        "UPDATE mutations SET attempts = attempts + 1, last_error = ?2 WHERE id = ?1",
                        params![id, reason],
                    )?;
                }
            }
            None => rejected += 1,
        }
    }
    tx.execute(
        "UPDATE sync_cursors SET last_pushed_at = ?1 WHERE entity_type = 'all'",
        params![now_iso()],
    )?;
    tx.commit()?;
    Ok((accepted, rejected))
}

// -- pull --------------------------------------------------------------------

/// The outcome of taking one streamed change into the local database.
#[derive(Debug)]
pub enum Integrated {
    /// Already known (this device's own push, or an older version).
    Skipped,
    Applied(Applied),
    /// Applied where possible; the overlapping fields were recorded as a
    /// conflict and the record is held back from pushing.
    Conflict(Applied),
    /// Refers to a record this database does not have yet; retried later.
    Deferred,
}

/// Journal a remote change so the card history shows it, under the device
/// that made it. `synced` means "nothing to push".
fn journal_remote(
    tx: &rusqlite::Transaction<'_>,
    change: &Change,
    fields: &[String],
    previous: Option<&Value>,
    existed: bool,
) -> Result<()> {
    if fields.is_empty() && existed {
        return Ok(());
    }
    let operation = if !existed {
        Operation::Create
    } else if fields == ["deletedAt".to_string()]
        && change.state.get("deletedAt").map(|v| !v.is_null()).unwrap_or(false)
    {
        Operation::Delete
    } else {
        Operation::Update
    };
    let payload = state::project(&change.state, fields);
    let before = previous.map(|state| state::project(state, fields));
    let id = mutation::record(
        tx,
        mutation::NewMutation {
            entity_type: &change.entity_type,
            entity_id: &change.entity_id,
            operation,
            payload: Some(&payload),
            previous: before.as_ref(),
            base_version: change.version,
            device_id: &change.device_id,
        },
    )?;
    tx.execute(
        "UPDATE mutations SET status = 'synced', synced_at = ?2 WHERE id = ?1",
        params![id, now_iso()],
    )?;
    Ok(())
}

fn is_missing_reference(error: &DbError) -> bool {
    matches!(error, DbError::Sqlite(rusqlite::Error::SqliteFailure(code, _))
        if code.code == rusqlite::ErrorCode::ConstraintViolation)
}

/// Take one streamed change into the local database: apply it outright when
/// this device has nothing unsent on the record, otherwise merge field by
/// field and record what collides.
pub fn integrate(connection: &mut Connection, change: &Change, me: &str) -> Result<Integrated> {
    if change.device_id == me {
        // Our own push, streamed back. The revision was recorded on ack.
        if revision(connection, &change.entity_type, &change.entity_id)?
            .map(|known| known >= change.version)
            .unwrap_or(false)
        {
            return Ok(Integrated::Skipped);
        }
    }
    if let Some(known) = revision(connection, &change.entity_type, &change.entity_id)? {
        if known >= change.version {
            return Ok(Integrated::Skipped);
        }
    }

    let local = state::load(connection, &change.entity_type, &change.entity_id)?;
    let mine = match &local {
        Some(_) => local_changes(connection, &change.entity_type, &change.entity_id)?,
        None => BTreeSet::new(),
    };

    let tx = connection.transaction()?;
    tx.execute_batch("PRAGMA defer_foreign_keys = ON")?;

    let outcome = match &local {
        None => {
            let applied = match state::apply(
                &tx,
                &change.entity_type,
                &change.entity_id,
                &change.state,
                &change.device_id,
            ) {
                Ok(applied) => applied,
                Err(error) if is_missing_reference(&error) => return Ok(Integrated::Deferred),
                Err(error) => return Err(error),
            };
            let fields: Vec<String> =
                state::fields(&change.entity_type).iter().map(|f| f.to_string()).collect();
            journal_remote(&tx, change, &fields, None, false)?;
            Integrated::Applied(applied)
        }
        Some(local) if mine.is_empty() => {
            let applied = match state::apply(
                &tx,
                &change.entity_type,
                &change.entity_id,
                &change.state,
                &change.device_id,
            ) {
                Ok(applied) => applied,
                Err(error) if is_missing_reference(&error) => return Ok(Integrated::Deferred),
                Err(error) => return Err(error),
            };
            let fields: Vec<String> = change
                .changed
                .iter()
                .filter(|f| local.get(f.as_str()) != change.state.get(f.as_str()))
                .cloned()
                .collect();
            journal_remote(&tx, change, &fields, Some(local), true)?;
            Integrated::Applied(applied)
        }
        Some(local) => {
            // Three-way, per field. The base is what both sides agreed on
            // last; fields only the server side moved come in, fields only
            // this side moved stay, and fields both moved — to different
            // values — are a conflict. Nothing is chosen by the clock.
            let mut merged = local.clone();
            let mut incoming = Vec::new();
            let mut collisions = Vec::new();
            for field in &change.changed {
                let remote_value = change.state.get(field.as_str()).cloned().unwrap_or(Value::Null);
                let local_value = local.get(field.as_str()).cloned().unwrap_or(Value::Null);
                if remote_value == local_value {
                    continue;
                }
                if mine.contains(field) {
                    collisions.push((field.clone(), local_value, remote_value));
                } else {
                    merged[field.as_str()] = remote_value;
                    incoming.push(field.clone());
                }
            }
            // Timestamps follow the newer word; identity never changes.
            if let Some(updated) = change.state.get("updatedAt") {
                merged["updatedAt"] = updated.clone();
            }
            let applied = match state::apply(
                &tx,
                &change.entity_type,
                &change.entity_id,
                &merged,
                &change.device_id,
            ) {
                Ok(applied) => applied,
                Err(error) if is_missing_reference(&error) => return Ok(Integrated::Deferred),
                Err(error) => return Err(error),
            };
            journal_remote(&tx, change, &incoming, Some(local), true)?;
            if collisions.is_empty() {
                Integrated::Applied(applied)
            } else {
                super::conflicts::record(&tx, change, local, &collisions, me)?;
                Integrated::Conflict(applied)
            }
        }
    };

    set_revision(&tx, &change.entity_type, &change.entity_id, change.version)?;
    tx.commit()?;
    Ok(outcome)
}

fn load_deferred(connection: &Connection) -> Result<Vec<Change>> {
    Ok(config::get_meta(connection, KEY_DEFERRED)?
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default())
}

fn save_deferred(connection: &Connection, changes: &[Change]) -> Result<()> {
    if changes.is_empty() {
        config::set_meta(connection, KEY_DEFERRED, None)
    } else {
        config::set_meta(connection, KEY_DEFERRED, Some(&serde_json::to_string(changes)?))
    }
}

/// Take one page of the stream in. Changes that refer to records not here
/// yet are kept aside and retried after later pages.
pub fn integrate_page(
    connection: &mut Connection,
    page: &PullPage,
    me: &str,
    report: &mut CycleReport,
) -> Result<()> {
    let mut deferred = load_deferred(connection)?;
    let mut pending: Vec<Change> = std::mem::take(&mut deferred);
    pending.extend(page.changes.iter().cloned());
    pending.sort_by_key(|change| change.seq);

    // Two passes: a change deferred by the first pass may only have needed a
    // later change in the same page.
    for _ in 0..2 {
        let batch = std::mem::take(&mut pending);
        for change in batch {
            match integrate(connection, &change, me)? {
                Integrated::Skipped => {}
                Integrated::Applied(applied) => {
                    report.applied += 1;
                    absorb(connection, applied, report)?;
                }
                Integrated::Conflict(applied) => {
                    report.applied += 1;
                    report.conflicts += 1;
                    absorb(connection, applied, report)?;
                }
                Integrated::Deferred => pending.push(change),
            }
        }
        if pending.is_empty() {
            break;
        }
    }
    save_deferred(connection, &pending)?;
    config::set_cursor(connection, page.next)?;
    Ok(())
}

/// Rule categories are re-evaluated once the change has committed; the
/// touched contacts are reported for the caller's own follow-up.
fn absorb(connection: &mut Connection, applied: Applied, report: &mut CycleReport) -> Result<()> {
    report.touched_contacts.extend(applied.contacts);
    for category_id in applied.categories {
        if let Err(error) = crate::categories::refresh_category(connection, &category_id, None) {
            eprintln!("category {category_id} refresh after sync failed: {error}");
        }
    }
    Ok(())
}

// -- the cycle ---------------------------------------------------------------

fn pull_all<D: Database>(
    db: &D,
    transport: &dyn Transport,
    me: &str,
    report: &mut CycleReport,
) -> Result<()> {
    loop {
        let after = db.with(|connection| config::cursor(connection))?;
        let page = transport.pull(after, PULL_LIMIT)?;
        report.pulled += page.changes.len();
        let more = page.has_more;
        db.with(|connection| integrate_page(connection, &page, me, report))?;
        if !more {
            return Ok(());
        }
    }
}

/// One full exchange: take in what the server has, send what this device
/// has, and if the server refused something because it moved meanwhile,
/// take that in and send again. Bounded, so a busy server cannot keep a
/// device in the loop forever.
pub fn run_cycle<D: Database>(
    db: &D,
    transport: &dyn Transport,
    meaning: Meaning<'_>,
) -> Result<CycleReport> {
    let mut report = CycleReport::default();
    let me = db.with(|connection| device_id(connection))?;

    let outcome = (|| -> Result<()> {
        pull_all(db, transport, &me, &mut report)?;
        for _ in 0..PUSH_ROUNDS {
            let items = db.with(|connection| prepare_push(connection))?;
            if items.is_empty() {
                break;
            }
            let mut refused = false;
            for chunk in items.chunks(PUSH_CHUNK) {
                let wire: Vec<PushItem> = chunk.iter().map(|(item, _)| item.clone()).collect();
                let results = transport.push(&wire)?;
                let (accepted, rejected) =
                    db.with(|connection| apply_push_results(connection, chunk, &results))?;
                report.pushed += accepted;
                report.rejected += rejected;
                refused |= rejected > 0;
            }
            if !refused {
                break;
            }
            pull_all(db, transport, &me, &mut report)?;
        }
        Ok(())
    })();

    match outcome {
        Ok(()) => {
            db.with(|connection| {
                config::set_meta(connection, KEY_LAST_AT, Some(&now_iso()))?;
                config::set_meta(connection, KEY_LAST_ERROR, None)
            })?;
        }
        Err(error) => {
            db.with(|connection| {
                config::set_meta(connection, KEY_LAST_ERROR, Some(&error.to_string()))
            })?;
            return Err(error);
        }
    }

    // Meaning-based categories see the new text only after the vectors are
    // rebuilt, which is the caller's job; the lexical ones are already right.
    let _ = meaning;
    report.touched_contacts.sort();
    report.touched_contacts.dedup();
    Ok(report)
}

/// The shape the settings screen and the indicator read, with the live
/// half filled in by the caller.
pub fn status_json(connection: &Connection, online: bool, syncing: bool) -> Result<Value> {
    let mut status = config::status(connection)?;
    status["online"] = json!(online);
    status["syncing"] = json!(syncing);
    Ok(status)
}
