//! Tags, organizations, relationships and notes.
//!
//! Straightforward CRUD, kept out of `repository.rs` so that file stays about
//! the contact record itself. Anything here that changes what a contact matches
//! on reindexes the affected contact, in the same transaction as the write.

use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde_json::json;
use yanuka_search::normalize_text;

use crate::error::{DbError, Result};
use crate::index::reindex_contact;
use crate::models::*;
use crate::mutation::{self, Operation};
use crate::repository::device_id;
use crate::{new_id, now_iso};

/// Append this write to the sync journal, inside the caller's transaction.
/// Every durable change goes through here: a write the journal misses is a
/// write sync will never deliver, and one the card history cannot show
/// (ADR-032).
fn journal(
    tx: &Transaction<'_>,
    entity_type: &str,
    entity_id: &str,
    operation: Operation,
    payload: Option<&serde_json::Value>,
    previous: Option<&serde_json::Value>,
    base_version: i64,
) -> Result<()> {
    let device = device_id(tx)?;
    mutation::record(
        tx,
        mutation::NewMutation {
            entity_type,
            entity_id,
            operation,
            payload,
            previous,
            base_version,
            device_id: &device,
        },
    )?;
    Ok(())
}

pub fn list_tags(connection: &Connection) -> Result<Vec<Tag>> {
    let mut statement =
        connection.prepare("SELECT * FROM tags WHERE deleted_at IS NULL ORDER BY name")?;
    let rows = statement.query_map([], |row| {
        Ok(Tag {
            id: row.get("id")?,
            name: row.get("name")?,
            normalized: row.get("normalized")?,
            color: row.get("color")?,
            description: row.get("description")?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Create a tag, or return the existing one with the same normalized name.
///
/// Idempotent on purpose: `סת"ם` and `סתם` are the same tag, and quietly
/// reusing it is better than accumulating near-duplicates that split the facet
/// counts and make the filter panel useless.
pub fn create_tag(connection: &mut Connection, name: &str, color: Option<&str>) -> Result<Tag> {
    let normalized = normalize_text(name);
    if normalized.is_empty() {
        return Err(DbError::Validation("יש להזין שם תגית".into()));
    }

    let existing: Option<String> = connection
        .query_row(
            "SELECT id FROM tags WHERE normalized = ?1 AND deleted_at IS NULL",
            params![normalized],
            |row| row.get(0),
        )
        .optional()?;

    if let Some(id) = existing {
        // Reused, not created — nothing changed, so nothing to journal.
        return get_tag(connection, &id);
    }

    let id = new_id();
    let now = now_iso();
    let tx = connection.transaction()?;
    tx.execute(
        "INSERT INTO tags (id, name, normalized, color, created_at, updated_at, version)
         VALUES (?1, ?2, ?3, ?4, ?5, ?5, 1)",
        params![id, name.trim(), normalized, color, now],
    )?;
    journal(
        &tx,
        "tag",
        &id,
        Operation::Create,
        Some(&json!({ "name": name.trim(), "color": color })),
        None,
        0,
    )?;
    tx.commit()?;
    get_tag(connection, &id)
}

fn get_tag(connection: &Connection, id: &str) -> Result<Tag> {
    Ok(connection.query_row("SELECT * FROM tags WHERE id = ?1", params![id], |row| {
        Ok(Tag {
            id: row.get("id")?,
            name: row.get("name")?,
            normalized: row.get("normalized")?,
            color: row.get("color")?,
            description: row.get("description")?,
        })
    })?)
}

/// Soft-delete a tag and reindex everyone who carried it.
pub fn delete_tag(connection: &mut Connection, id: &str) -> Result<()> {
    let affected: Vec<String> = {
        let mut statement = connection.prepare(
            "SELECT contact_id FROM contact_tags WHERE tag_id = ?1 AND deleted_at IS NULL",
        )?;
        let rows = statement.query_map(params![id], |row| row.get::<_, String>(0))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };

    let name: Option<String> = connection
        .query_row("SELECT name FROM tags WHERE id = ?1", params![id], |row| row.get(0))
        .optional()?;

    let now = now_iso();
    let tx = connection.transaction()?;
    tx.execute("UPDATE tags SET deleted_at = ?2 WHERE id = ?1", params![id, now])?;
    tx.execute("UPDATE contact_tags SET deleted_at = ?2 WHERE tag_id = ?1", params![id, now])?;
    journal(&tx, "tag", id, Operation::Delete, None, Some(&json!({ "name": name })), 0)?;
    for contact_id in &affected {
        reindex_contact(&tx, contact_id)?;
    }
    tx.commit()?;
    Ok(())
}

// Categories moved to `categories.rs` (ADR-038).

pub fn list_organizations(
    connection: &Connection,
    query: Option<&str>,
    limit: i64,
) -> Result<Vec<Organization>> {
    let needle = query.map(normalize_text).unwrap_or_default();
    let mut statement = connection.prepare(
        "SELECT * FROM organizations
          WHERE deleted_at IS NULL AND (?1 = '' OR normalized LIKE '%' || ?1 || '%')
          ORDER BY name LIMIT ?2",
    )?;
    let rows = statement.query_map(params![needle, limit], |row| {
        Ok(Organization {
            id: row.get("id")?,
            name: row.get("name")?,
            normalized: row.get("normalized")?,
            kind: row.get("kind")?,
            city: row.get("city")?,
            region: row.get("region")?,
            country: row.get("country")?,
            address: row.get("address")?,
            notes: row.get("notes")?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn create_organization(
    connection: &mut Connection,
    name: &str,
    kind: &str,
    city: Option<&str>,
    country: Option<&str>,
) -> Result<Organization> {
    let normalized = normalize_text(name);
    if normalized.is_empty() {
        return Err(DbError::Validation("יש להזין שם מוסד".into()));
    }

    let id = new_id();
    let now = now_iso();
    let tx = connection.transaction()?;
    tx.execute(
        "INSERT INTO organizations (id, name, normalized, kind, city, country,
                                    created_at, updated_at, version)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7, 1)",
        params![id, name.trim(), normalized, kind, city, country, now],
    )?;
    journal(
        &tx,
        "organization",
        &id,
        Operation::Create,
        Some(&json!({ "name": name.trim(), "kind": kind, "city": city, "country": country })),
        None,
        0,
    )?;
    tx.commit()?;

    list_organizations(connection, Some(name), 1)?
        .into_iter()
        .next()
        .ok_or_else(|| DbError::NotFound("המוסד".into()))
}

pub fn delete_organization(connection: &mut Connection, id: &str) -> Result<()> {
    let affected: Vec<String> = {
        let mut statement = connection.prepare(
            "SELECT contact_id FROM contact_organizations WHERE organization_id = ?1 AND deleted_at IS NULL",
        )?;
        let rows = statement.query_map(params![id], |row| row.get::<_, String>(0))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };

    let name: Option<String> = connection
        .query_row("SELECT name FROM organizations WHERE id = ?1", params![id], |row| row.get(0))
        .optional()?;

    let now = now_iso();
    let tx = connection.transaction()?;
    tx.execute("UPDATE organizations SET deleted_at = ?2 WHERE id = ?1", params![id, now])?;
    tx.execute(
        "UPDATE contact_organizations SET deleted_at = ?2 WHERE organization_id = ?1",
        params![id, now],
    )?;
    journal(&tx, "organization", id, Operation::Delete, None, Some(&json!({ "name": name })), 0)?;
    for contact_id in &affected {
        reindex_contact(&tx, contact_id)?;
    }
    tx.commit()?;
    Ok(())
}

/// Create a directed relationship between two contacts.
pub fn create_relationship(
    connection: &mut Connection,
    from_id: &str,
    to_id: &str,
    kind: &str,
    notes: Option<&str>,
) -> Result<String> {
    if from_id == to_id {
        return Err(DbError::Validation("לא ניתן לקשר איש קשר לעצמו".into()));
    }

    let id = new_id();
    let now = now_iso();
    let tx = connection.transaction()?;
    tx.execute(
        "INSERT INTO relationships (id, from_contact_id, to_contact_id, type, notes,
                                    created_at, updated_at, version)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, 1)",
        params![id, from_id, to_id, kind, notes, now],
    )?;
    // Both endpoint ids ride in the payload so the card history of either
    // contact finds this entry (see mutation::history).
    journal(
        &tx,
        "relationship",
        &id,
        Operation::Create,
        Some(&json!({
            "fromContactId": from_id,
            "toContactId": to_id,
            "type": kind,
            "notes": notes,
        })),
        None,
        0,
    )?;
    tx.commit()?;
    Ok(id)
}

pub fn delete_relationship(connection: &mut Connection, id: &str) -> Result<()> {
    let edge: Option<(String, String, String, i64)> = connection
        .query_row(
            "SELECT from_contact_id, to_contact_id, type, version FROM relationships
              WHERE id = ?1 AND deleted_at IS NULL",
            params![id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?;
    // Deleting an edge that is already gone changes nothing — and journaling
    // a no-op would replay nothing when sync arrives.
    let Some((from_id, to_id, kind, version)) = edge else {
        return Ok(());
    };

    let tx = connection.transaction()?;
    tx.execute("UPDATE relationships SET deleted_at = ?2 WHERE id = ?1", params![id, now_iso()])?;
    journal(
        &tx,
        "relationship",
        id,
        Operation::Delete,
        Some(&json!({ "fromContactId": from_id, "toContactId": to_id })),
        Some(&json!({ "type": kind })),
        version,
    )?;
    tx.commit()?;
    Ok(())
}

/// Add a timestamped note and reindex, since notes are searchable.
pub fn add_note(
    connection: &mut Connection,
    contact_id: &str,
    body: &str,
    is_sensitive: bool,
) -> Result<String> {
    if body.trim().is_empty() {
        return Err(DbError::Validation("יש להזין תוכן להערה".into()));
    }

    let id = new_id();
    let now = now_iso();
    let tx = connection.transaction()?;
    tx.execute(
        "INSERT INTO notes (id, contact_id, body, is_sensitive, created_at, updated_at, version)
         VALUES (?1, ?2, ?3, ?4, ?5, ?5, 1)",
        params![id, contact_id, body.trim(), i64::from(is_sensitive), now],
    )?;
    journal(
        &tx,
        "note",
        &id,
        Operation::Create,
        Some(&json!({
            "contactId": contact_id,
            "body": body.trim(),
            "isSensitive": is_sensitive,
        })),
        None,
        0,
    )?;
    reindex_contact(&tx, contact_id)?;
    tx.commit()?;
    Ok(id)
}

/// Rewrite a note's body and reindex, so the edit is searchable immediately
/// and the replaced wording stops matching.
pub fn update_note(
    connection: &mut Connection,
    id: &str,
    body: &str,
    is_sensitive: Option<bool>,
) -> Result<()> {
    if body.trim().is_empty() {
        return Err(DbError::Validation("יש להזין תוכן להערה".into()));
    }

    let before: Option<(String, String, i64)> = connection
        .query_row(
            "SELECT contact_id, body, version FROM notes WHERE id = ?1",
            params![id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let Some((contact_id, old_body, version)) = before else {
        return Err(DbError::Validation("ההערה לא נמצאה".into()));
    };

    let tx = connection.transaction()?;
    tx.execute(
        "UPDATE notes SET body = ?2, is_sensitive = COALESCE(?3, is_sensitive),
                          updated_at = ?4, version = version + 1
          WHERE id = ?1",
        params![id, body.trim(), is_sensitive.map(i64::from), now_iso()],
    )?;
    journal(
        &tx,
        "note",
        id,
        Operation::Update,
        Some(&json!({ "contactId": contact_id, "body": body.trim() })),
        Some(&json!({ "body": old_body })),
        version,
    )?;
    reindex_contact(&tx, &contact_id)?;
    tx.commit()?;
    Ok(())
}

pub fn delete_note(connection: &mut Connection, id: &str) -> Result<()> {
    let before: Option<(String, String, i64)> = connection
        .query_row(
            "SELECT contact_id, body, version FROM notes WHERE id = ?1",
            params![id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;

    let tx = connection.transaction()?;
    tx.execute("UPDATE notes SET deleted_at = ?2 WHERE id = ?1", params![id, now_iso()])?;
    if let Some((contact_id, old_body, version)) = before {
        // The body rides in `previous`: a deleted note stays readable from
        // the journal, which is what priority 1 means here.
        journal(
            &tx,
            "note",
            id,
            Operation::Delete,
            Some(&json!({ "contactId": contact_id })),
            Some(&json!({ "body": old_body })),
            version,
        )?;
        reindex_contact(&tx, &contact_id)?;
    }
    tx.commit()?;
    Ok(())
}

/// Counts for the settings screen and the offline indicator.
pub fn stats(connection: &Connection) -> Result<serde_json::Value> {
    let count = |sql: &str| -> Result<i64> { Ok(connection.query_row(sql, [], |row| row.get(0))?) };

    Ok(serde_json::json!({
        "contacts": count("SELECT COUNT(*) FROM contacts WHERE deleted_at IS NULL")?,
        "organizations": count("SELECT COUNT(*) FROM organizations WHERE deleted_at IS NULL")?,
        "tags": count("SELECT COUNT(*) FROM tags WHERE deleted_at IS NULL")?,
        "relationships": count("SELECT COUNT(*) FROM relationships WHERE deleted_at IS NULL")?,
        "notes": count("SELECT COUNT(*) FROM notes WHERE deleted_at IS NULL")?,
        "sync": {
            "online": false,
            "lastSyncAt": serde_json::Value::Null,
            "pendingMutations": crate::mutation::pending_count(connection)?,
            "failedMutations": count("SELECT COUNT(*) FROM mutations WHERE status = 'failed'")?,
            "openConflicts": count("SELECT COUNT(*) FROM conflicts WHERE resolved_at IS NULL")?,
            "syncing": false,
        }
    }))
}
