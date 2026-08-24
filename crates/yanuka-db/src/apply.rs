//! Applying a change that arrived from another device.
//!
//! The counterpart to `mutation.rs`. That module records what happened here;
//! this one takes what happened somewhere else and folds it in. Everything in
//! between — the transport, the server — is a courier. All the decisions that
//! can lose data live in this file.
//!
//! Four rules shape it, and each of them is a rule because the obvious
//! alternative silently destroys something:
//!
//! * **Applying never logs a mutation.** A remote change written through the
//!   normal repository path would append a local mutation, push it back, and
//!   the two devices would trade the same edit forever. The writes here go
//!   through `insert_contact_row` / `update_contact_row`, which are shared with
//!   the local path precisely so the two cannot drift apart.
//!
//! * **Merging is per field, decided by `previous`.** The mutation says what the
//!   sending device saw before its edit. If the local value still matches that,
//!   nothing here touched the field and the remote value is simply newer. If it
//!   does not match, two people edited the same field and no rule about clocks
//!   or versions can tell you which one was right.
//!
//! * **A real collision is never resolved silently.** Both values are kept in
//!   `conflicts` and the local one stays in place until a human chooses. Picking
//!   a winner by timestamp is guessing, and two machines that have been offline
//!   do not have comparable clocks anyway.
//!
//! * **A change whose subject has not arrived is deferred, not dropped.** A note
//!   can reach a device before the contact it hangs off. Returning it unapplied
//!   keeps it in the queue for the next pass; writing it would violate a foreign
//!   key, and skipping it would lose the note.
//!
//! See docs/SYNC.md.

use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::error::{DbError, Result};
use crate::index::reindex_contact;
use crate::models::{ContactInput, Ulid};
use crate::repository::{
    as_input, get_contact, insert_contact_row, update_contact_row, MutationRow,
};
use crate::{new_id, now_iso};

/// A mutation received from another device, in the shape the wire carries it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteMutation {
    pub id: Ulid,
    pub entity_type: String,
    pub entity_id: Ulid,
    pub operation: String,
    pub payload: Option<Value>,
    pub previous: Option<Value>,
    #[serde(default)]
    pub base_version: i64,
    pub created_at: String,
    pub device_id: String,
}

/// What applying a mutation actually did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Applied {
    /// Written.
    Applied,
    /// Already applied. The log is at-least-once by design, so this is routine
    /// rather than an error — a pull interrupted halfway replays on the retry.
    AlreadySeen,
    /// Written, except for the named fields: the local copy of each had moved
    /// too. Both values are in `conflicts` and the local one still stands.
    Conflicted(Vec<String>),
    /// The entity this change belongs to is not here yet. Nothing was written
    /// and the mutation is *not* marked as seen, so the next pass retries it.
    Deferred,
}

/// Fold a remote change into the local database.
pub fn apply(connection: &mut Connection, remote: &RemoteMutation) -> Result<Applied> {
    // The primary key of the log is the deduplication. A mutation already
    // recorded here — whether made locally or applied from elsewhere — has
    // already had its effect.
    let seen: Option<i64> = connection
        .query_row("SELECT 1 FROM mutations WHERE id = ?1", params![remote.id], |row| row.get(0))
        .optional()?;
    if seen.is_some() {
        return Ok(Applied::AlreadySeen);
    }

    let tx = connection.transaction()?;
    let outcome = match remote.entity_type.as_str() {
        "contact" => apply_contact(&tx, remote)?,
        "note" => apply_note(&tx, remote)?,
        "relationship" => apply_relationship(&tx, remote)?,
        "tag" => apply_named(&tx, remote, "tags", "contact_tags", "tag_id")?,
        "category" => apply_named(&tx, remote, "categories", "contact_categories", "category_id")?,
        "organization" => {
            apply_named(&tx, remote, "organizations", "contact_organizations", "organization_id")?
        }
        other => return Err(DbError::Validation(format!("סוג רשומה לא מוכר בסנכרון: {other}"))),
    };

    if outcome == Applied::Deferred {
        // Nothing was written and nothing is recorded, so the next pull sees
        // this mutation again — by then its contact will have arrived.
        return Ok(outcome);
    }

    record_as_seen(&tx, remote)?;
    tx.commit()?;
    Ok(outcome)
}

/// File the remote mutation in the local log, already settled.
///
/// It is stored rather than discarded for two reasons: the primary key is what
/// makes a second delivery a no-op, and the row is the only record of where a
/// change came from once it has been folded into the data.
fn record_as_seen(tx: &Transaction<'_>, remote: &RemoteMutation) -> Result<()> {
    tx.execute(
        "INSERT INTO mutations (id, entity_type, entity_id, operation, payload, previous,
                                base_version, created_at, device_id, status, attempts, synced_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'synced', 0, ?10)",
        params![
            remote.id,
            remote.entity_type,
            remote.entity_id,
            remote.operation,
            remote.payload.as_ref().map(|value| value.to_string()),
            remote.previous.as_ref().map(|value| value.to_string()),
            remote.base_version,
            remote.created_at,
            remote.device_id,
            now_iso(),
        ],
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Contacts
// ---------------------------------------------------------------------------

fn apply_contact(tx: &Transaction<'_>, remote: &RemoteMutation) -> Result<Applied> {
    let local = get_contact(tx, &remote.entity_id)?;

    match (remote.operation.as_str(), local) {
        ("delete", Some(_)) => {
            // A tombstone, not a removal. The row has to survive so a restore
            // on any device still has something to restore.
            tx.execute(
                "UPDATE contacts SET deleted_at = ?2, updated_at = ?2, version = version + 1,
                                     device_id = ?3
                  WHERE id = ?1",
                params![remote.entity_id, remote.created_at, remote.device_id],
            )?;
            reindex_contact(tx, &remote.entity_id)?;
            Ok(Applied::Applied)
        }
        // A delete for a contact this device never received. There is nothing
        // to tombstone and nothing to wait for, so it is settled.
        ("delete", None) => Ok(Applied::Applied),

        ("create", None) => {
            let input = contact_input(remote.payload.as_ref())?;
            insert_contact_row(
                tx,
                &remote.entity_id,
                &input,
                &remote.created_at,
                &remote.device_id,
                1,
            )?;
            reindex_contact(tx, &remote.entity_id)?;
            Ok(Applied::Applied)
        }

        // Either an update, or a create for a contact that is somehow already
        // here — a replayed log, or the same record reaching this device by two
        // routes. Both are merges: the local copy exists and must not be
        // flattened by whatever arrived last.
        (_, Some(stored)) => merge_contact(tx, remote, &stored),

        ("update", None) => Ok(Applied::Deferred),
        (other, None) => Err(DbError::Validation(format!("פעולה לא מוכרת בסנכרון: {other}"))),
    }
}

/// Whether a field holds nothing a person would recognise as data.
///
/// `null`, the empty string and the empty list all mean "not filled in" here,
/// and a value arriving for an unfilled field is new information rather than a
/// competing answer.
fn is_blank(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => true,
        Some(Value::String(text)) => text.trim().is_empty(),
        Some(Value::Array(items)) => items.is_empty(),
        _ => false,
    }
}

fn contact_input(payload: Option<&Value>) -> Result<ContactInput> {
    let payload = payload
        .ok_or_else(|| DbError::Validation("יצירת איש קשר בסנכרון הגיעה ללא תוכן".into()))?;
    Ok(serde_json::from_value(payload.clone())?)
}

/// Three-way merge of one contact.
///
/// The three sides are the local record, the remote device's view of it before
/// its edit (`previous`), and the edit itself (`payload`). Comparing the local
/// value against `previous` is what separates "nobody here touched this, take
/// theirs" from "we both changed it, ask a human" — a version number alone
/// cannot tell those apart, because it moves when *any* field changes.
fn merge_contact(
    tx: &Transaction<'_>,
    remote: &RemoteMutation,
    stored: &crate::models::ContactWithRelations,
) -> Result<Applied> {
    let Some(payload) = remote.payload.as_ref().and_then(Value::as_object) else {
        return Ok(Applied::Applied);
    };
    let empty = Map::new();
    let base = remote.previous.as_ref().and_then(Value::as_object);
    // Whether the sender told us what it was overwriting. An update always
    // does. A `create` does not — it has no prior state — so a create landing
    // on a row that already exists here arrives with no evidence at all about
    // which side is stale, and the two cases have to be treated differently.
    let has_baseline = base.is_some();
    let base = base.unwrap_or(&empty);

    let local_value = serde_json::to_value(as_input(stored))?;
    let mut merged = local_value.as_object().cloned().unwrap_or_default();

    let mut conflicts: Vec<Value> = Vec::new();
    let mut conflicted: Vec<String> = Vec::new();

    for (field, incoming) in payload {
        // Bookkeeping the merge module adds to describe a merge; it names an
        // operation rather than a column, so there is nothing to reconcile.
        if field == "mergedFrom" {
            continue;
        }
        let here = merged.get(field);
        if here == Some(incoming) {
            continue; // Both sides already agree.
        }

        let untouched_here = match base.get(field) {
            Some(before) => here == Some(before),
            // The sender reported a baseline but not for this field, so it did
            // not consider the field part of its edit. Nothing here diverged
            // from anything: take theirs, as a fresh field would get.
            None if has_baseline => true,
            // No baseline at all — a create meeting a row that is already here.
            // Overwriting on that basis is how a redelivered create silently
            // erases edits made in between, so only an empty slot is filled and
            // anything else is treated as the disagreement it is.
            None => is_blank(here),
        };

        if untouched_here {
            merged.insert(field.clone(), incoming.clone());
        } else {
            conflicted.push(field.clone());
            conflicts.push(serde_json::json!({
                "field": field,
                "localValue": here.cloned().unwrap_or(Value::Null),
                "remoteValue": incoming.clone(),
                "localUpdatedAt": stored.contact.updated_at,
                "remoteUpdatedAt": remote.created_at,
                "localDeviceId": stored.contact.device_id,
                "remoteDeviceId": remote.device_id,
            }));
        }
    }

    let input: ContactInput = serde_json::from_value(Value::Object(merged))?;
    let version = stored.contact.version.max(remote.base_version) + 1;
    update_contact_row(
        tx,
        &remote.entity_id,
        &input,
        &remote.created_at,
        &remote.device_id,
        version,
    )?;
    reindex_contact(tx, &remote.entity_id)?;

    if conflicts.is_empty() {
        return Ok(Applied::Applied);
    }

    tx.execute(
        "INSERT INTO conflicts (id, entity_type, entity_id, fields, detected_at)
         VALUES (?1, 'contact', ?2, ?3, ?4)",
        params![new_id(), remote.entity_id, Value::Array(conflicts).to_string(), now_iso(),],
    )?;
    Ok(Applied::Conflicted(conflicted))
}

// ---------------------------------------------------------------------------
// Everything hanging off a contact
// ---------------------------------------------------------------------------

fn apply_note(tx: &Transaction<'_>, remote: &RemoteMutation) -> Result<Applied> {
    if remote.operation == "delete" {
        tx.execute(
            "UPDATE notes SET deleted_at = ?2 WHERE id = ?1",
            params![remote.entity_id, remote.created_at],
        )?;
        if let Some(contact_id) = note_owner(tx, &remote.entity_id)? {
            reindex_contact(tx, &contact_id)?;
        }
        return Ok(Applied::Applied);
    }

    let payload = remote
        .payload
        .as_ref()
        .and_then(Value::as_object)
        .ok_or_else(|| DbError::Validation("הערה בסנכרון הגיעה ללא תוכן".into()))?;
    let contact_id = payload.get("contactId").and_then(Value::as_str).unwrap_or_default();

    if get_contact(tx, contact_id)?.is_none() {
        return Ok(Applied::Deferred);
    }

    tx.execute(
        "INSERT INTO notes (id, contact_id, body, is_sensitive, created_at, updated_at,
                            version, device_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?5, 1, ?6)",
        params![
            remote.entity_id,
            contact_id,
            payload.get("body").and_then(Value::as_str).unwrap_or_default(),
            i64::from(payload.get("isSensitive").and_then(Value::as_bool).unwrap_or(false)),
            remote.created_at,
            remote.device_id,
        ],
    )?;
    reindex_contact(tx, contact_id)?;
    Ok(Applied::Applied)
}

fn note_owner(tx: &Transaction<'_>, id: &str) -> Result<Option<String>> {
    Ok(tx
        .query_row("SELECT contact_id FROM notes WHERE id = ?1", params![id], |row| row.get(0))
        .optional()?)
}

fn apply_relationship(tx: &Transaction<'_>, remote: &RemoteMutation) -> Result<Applied> {
    if remote.operation == "delete" {
        tx.execute(
            "UPDATE relationships SET deleted_at = ?2 WHERE id = ?1",
            params![remote.entity_id, remote.created_at],
        )?;
        return Ok(Applied::Applied);
    }

    let payload = remote
        .payload
        .as_ref()
        .and_then(Value::as_object)
        .ok_or_else(|| DbError::Validation("קשר בסנכרון הגיע ללא תוכן".into()))?;
    let from = payload.get("fromContactId").and_then(Value::as_str).unwrap_or_default();
    let to = payload.get("toContactId").and_then(Value::as_str).unwrap_or_default();

    // An edge needs both of its ends. Deferring is what keeps the graph honest:
    // half an edge is worse than no edge, because nothing will ever repair it.
    if get_contact(tx, from)?.is_none() || get_contact(tx, to)?.is_none() {
        return Ok(Applied::Deferred);
    }

    tx.execute(
        "INSERT INTO relationships (id, from_contact_id, to_contact_id, type, notes,
                                    created_at, updated_at, version)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, 1)",
        params![
            remote.entity_id,
            from,
            to,
            payload.get("type").and_then(Value::as_str).unwrap_or("knows"),
            payload.get("notes").and_then(Value::as_str),
            remote.created_at,
        ],
    )?;
    reindex_contact(tx, from)?;
    reindex_contact(tx, to)?;
    Ok(Applied::Applied)
}

/// Tags, categories and organizations.
///
/// The three are the same shape — a named row plus a join table pointing at it —
/// and their deletes carry the cascade that `taxonomy.rs` deliberately does not
/// log: the join rows follow from the parent id, so they are reproduced here
/// rather than shipped.
fn apply_named(
    tx: &Transaction<'_>,
    remote: &RemoteMutation,
    table: &str,
    join_table: &str,
    join_column: &str,
) -> Result<Applied> {
    let affected: Vec<String> = {
        let mut statement = tx.prepare(&format!(
            "SELECT contact_id FROM {join_table} WHERE {join_column} = ?1 AND deleted_at IS NULL"
        ))?;
        let rows = statement.query_map(params![remote.entity_id], |row| row.get::<_, String>(0))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };

    if remote.operation == "delete" {
        tx.execute(
            &format!("UPDATE {table} SET deleted_at = ?2 WHERE id = ?1"),
            params![remote.entity_id, remote.created_at],
        )?;
        tx.execute(
            &format!("UPDATE {join_table} SET deleted_at = ?2 WHERE {join_column} = ?1"),
            params![remote.entity_id, remote.created_at],
        )?;
        for contact_id in &affected {
            reindex_contact(tx, contact_id)?;
        }
        return Ok(Applied::Applied);
    }

    let payload = remote
        .payload
        .as_ref()
        .and_then(Value::as_object)
        .ok_or_else(|| DbError::Validation("רשומה בסנכרון הגיעה ללא תוכן".into()))?;
    let name = payload.get("name").and_then(Value::as_str).unwrap_or_default();
    let normalized = yanuka_search::normalize_text(name);

    // Names are the identity here, not the ids: two devices offline both
    // creating "ישיבת מיר" mint different ids for the same institution, and
    // letting both land would split every facet count that mentions it.
    let existing: Option<String> = tx
        .query_row(
            &format!("SELECT id FROM {table} WHERE normalized = ?1 AND deleted_at IS NULL"),
            params![normalized],
            |row| row.get(0),
        )
        .optional()?;
    if existing.is_some() {
        return Ok(Applied::Applied);
    }

    match table {
        "tags" => tx.execute(
            "INSERT INTO tags (id, name, normalized, color, created_at, updated_at, version)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5, 1)",
            params![
                remote.entity_id,
                name,
                normalized,
                payload.get("color").and_then(Value::as_str),
                remote.created_at,
            ],
        )?,
        "categories" => tx.execute(
            "INSERT INTO categories (id, name, normalized, description, created_at, updated_at,
                                     version)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5, 1)",
            params![
                remote.entity_id,
                name,
                normalized,
                payload.get("description").and_then(Value::as_str),
                remote.created_at,
            ],
        )?,
        _ => tx.execute(
            "INSERT INTO organizations (id, name, normalized, kind, city, country,
                                        created_at, updated_at, version)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7, 1)",
            params![
                remote.entity_id,
                name,
                normalized,
                payload.get("kind").and_then(Value::as_str).unwrap_or("other"),
                payload.get("city").and_then(Value::as_str),
                payload.get("country").and_then(Value::as_str),
                remote.created_at,
            ],
        )?,
    };
    Ok(Applied::Applied)
}

// ---------------------------------------------------------------------------
// Reading the queue
// ---------------------------------------------------------------------------

/// Local changes waiting to be sent, oldest first.
///
/// ULIDs sort by creation time, so this is the order the edits were made — which
/// is the order they have to be replayed in, or a note can arrive before the
/// contact it belongs to and bounce off the foreign key.
pub fn pending(connection: &Connection, limit: i64) -> Result<Vec<RemoteMutation>> {
    let mut statement = connection.prepare(
        "SELECT id, entity_type, entity_id, operation, payload, previous, base_version,
                created_at, device_id
           FROM mutations
          WHERE status IN ('pending', 'failed')
          ORDER BY id
          LIMIT ?1",
    )?;
    let rows = statement.query_map(params![limit], |row| {
        Ok(MutationRow {
            id: row.get(0)?,
            entity_type: row.get(1)?,
            entity_id: row.get(2)?,
            operation: row.get(3)?,
            payload: row.get(4)?,
            previous: row.get(5)?,
            base_version: row.get(6)?,
            created_at: row.get(7)?,
            device_id: row.get(8)?,
        })
    })?;

    rows.collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .map(|row| {
            Ok(RemoteMutation {
                id: row.id,
                entity_type: row.entity_type,
                entity_id: row.entity_id,
                operation: row.operation,
                payload: row.payload.as_deref().map(serde_json::from_str).transpose()?,
                previous: row.previous.as_deref().map(serde_json::from_str).transpose()?,
                base_version: row.base_version,
                created_at: row.created_at,
                device_id: row.device_id,
            })
        })
        .collect()
}

/// Mark pushed mutations as settled so they are not sent again.
pub fn mark_synced(connection: &Connection, ids: &[String]) -> Result<()> {
    let now = now_iso();
    for id in ids {
        connection.execute(
            "UPDATE mutations SET status = 'synced', synced_at = ?2 WHERE id = ?1",
            params![id, now],
        )?;
    }
    Ok(())
}
