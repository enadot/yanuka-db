//! Tags, categories, organizations, relationships and notes.
//!
//! Straightforward CRUD, kept out of `repository.rs` so that file stays about
//! the contact record itself. Anything here that changes what a contact matches
//! on reindexes the affected contact, in the same transaction as the write.

use rusqlite::{params, Connection, OptionalExtension};
use yanuka_search::normalize_text;

use crate::error::{DbError, Result};
use crate::index::reindex_contact;
use crate::models::*;
use crate::{new_id, now_iso};

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
pub fn create_tag(connection: &Connection, name: &str, color: Option<&str>) -> Result<Tag> {
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
        return get_tag(connection, &id);
    }

    let id = new_id();
    let now = now_iso();
    connection.execute(
        "INSERT INTO tags (id, name, normalized, color, created_at, updated_at, version)
         VALUES (?1, ?2, ?3, ?4, ?5, ?5, 1)",
        params![id, name.trim(), normalized, color, now],
    )?;
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

    let now = now_iso();
    let tx = connection.transaction()?;
    tx.execute("UPDATE tags SET deleted_at = ?2 WHERE id = ?1", params![id, now])?;
    tx.execute("UPDATE contact_tags SET deleted_at = ?2 WHERE tag_id = ?1", params![id, now])?;
    for contact_id in &affected {
        reindex_contact(&tx, contact_id)?;
    }
    tx.commit()?;
    Ok(())
}

pub fn list_categories(connection: &Connection) -> Result<Vec<Category>> {
    let mut statement =
        connection.prepare("SELECT * FROM categories WHERE deleted_at IS NULL ORDER BY name")?;
    let rows = statement.query_map([], |row| {
        Ok(Category {
            id: row.get("id")?,
            name: row.get("name")?,
            normalized: row.get("normalized")?,
            description: row.get("description")?,
            parent_id: row.get("parent_id")?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn create_category(
    connection: &Connection,
    name: &str,
    description: Option<&str>,
) -> Result<Category> {
    let normalized = normalize_text(name);
    if normalized.is_empty() {
        return Err(DbError::Validation("יש להזין שם קטגוריה".into()));
    }

    let id = new_id();
    let now = now_iso();
    connection.execute(
        "INSERT INTO categories (id, name, normalized, description, created_at, updated_at, version)
         VALUES (?1, ?2, ?3, ?4, ?5, ?5, 1)",
        params![id, name.trim(), normalized, description, now],
    )?;

    Ok(connection.query_row("SELECT * FROM categories WHERE id = ?1", params![id], |row| {
        Ok(Category {
            id: row.get("id")?,
            name: row.get("name")?,
            normalized: row.get("normalized")?,
            description: row.get("description")?,
            parent_id: row.get("parent_id")?,
        })
    })?)
}

pub fn delete_category(connection: &mut Connection, id: &str) -> Result<()> {
    let affected: Vec<String> = {
        let mut statement = connection.prepare(
            "SELECT contact_id FROM contact_categories WHERE category_id = ?1 AND deleted_at IS NULL",
        )?;
        let rows = statement.query_map(params![id], |row| row.get::<_, String>(0))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };

    let now = now_iso();
    let tx = connection.transaction()?;
    tx.execute("UPDATE categories SET deleted_at = ?2 WHERE id = ?1", params![id, now])?;
    tx.execute(
        "UPDATE contact_categories SET deleted_at = ?2 WHERE category_id = ?1",
        params![id, now],
    )?;
    for contact_id in &affected {
        reindex_contact(&tx, contact_id)?;
    }
    tx.commit()?;
    Ok(())
}

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
    connection: &Connection,
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
    connection.execute(
        "INSERT INTO organizations (id, name, normalized, kind, city, country,
                                    created_at, updated_at, version)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7, 1)",
        params![id, name.trim(), normalized, kind, city, country, now],
    )?;

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

    let now = now_iso();
    let tx = connection.transaction()?;
    tx.execute("UPDATE organizations SET deleted_at = ?2 WHERE id = ?1", params![id, now])?;
    tx.execute(
        "UPDATE contact_organizations SET deleted_at = ?2 WHERE organization_id = ?1",
        params![id, now],
    )?;
    for contact_id in &affected {
        reindex_contact(&tx, contact_id)?;
    }
    tx.commit()?;
    Ok(())
}

/// Create a directed relationship between two contacts.
pub fn create_relationship(
    connection: &Connection,
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
    connection.execute(
        "INSERT INTO relationships (id, from_contact_id, to_contact_id, type, notes,
                                    created_at, updated_at, version)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, 1)",
        params![id, from_id, to_id, kind, notes, now],
    )?;
    Ok(id)
}

pub fn delete_relationship(connection: &Connection, id: &str) -> Result<()> {
    connection.execute(
        "UPDATE relationships SET deleted_at = ?2 WHERE id = ?1",
        params![id, now_iso()],
    )?;
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
    reindex_contact(&tx, contact_id)?;
    tx.commit()?;
    Ok(id)
}

pub fn delete_note(connection: &mut Connection, id: &str) -> Result<()> {
    let contact_id: Option<String> = connection
        .query_row("SELECT contact_id FROM notes WHERE id = ?1", params![id], |row| row.get(0))
        .optional()?;

    let tx = connection.transaction()?;
    tx.execute("UPDATE notes SET deleted_at = ?2 WHERE id = ?1", params![id, now_iso()])?;
    if let Some(contact_id) = contact_id {
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
