//! The wire form of each syncable record, and how it is written back.
//!
//! `load` renders a record — and, for a contact, the collections it owns —
//! as one JSON object with the camelCase keys the frontend already uses.
//! `apply` does the reverse: it writes that object over whatever the row
//! holds, replacing the owned collections wholesale exactly as
//! `write_children` does for a form submission, and rebuilds the search
//! index in the same transaction. Derived columns (normalized text, phone
//! digits) are recomputed here, never trusted from the wire.
//!
//! Nothing in this module journals a mutation: whether a write is a local
//! edit or a remote one is the engine's business.

use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde_json::{json, Map, Value};
use yanuka_search::{normalize_name, normalize_text};

use crate::error::{DbError, Result};
use crate::index::reindex_contact;
use crate::{new_id, now_iso};

/// Merge-granular fields per entity type. `id` and `createdAt` are identity,
/// not fields; `updatedAt` follows whichever side is applied.
pub const CONTACT_FIELDS: &[&str] = &[
    "firstName",
    "lastName",
    "displayName",
    "prefix",
    "title",
    "country",
    "region",
    "city",
    "address",
    "postalCode",
    "profession",
    "role",
    "notes",
    "reasonForSaving",
    "source",
    "introducedBy",
    "introducedByContactId",
    "isFavorite",
    "deletedAt",
    "phones",
    "emails",
    "aliases",
    "specialties",
    "languages",
    "tagIds",
    "categories",
    "organizations",
];
pub const TAG_FIELDS: &[&str] = &["name", "color", "description", "deletedAt"];
pub const CATEGORY_FIELDS: &[&str] = &[
    "name",
    "description",
    "parentId",
    "icon",
    "color",
    "rule",
    "sortOrder",
    "showOnHome",
    "deletedAt",
];
pub const ORGANIZATION_FIELDS: &[&str] =
    &["name", "kind", "city", "region", "country", "address", "notes", "deletedAt"];
pub const RELATIONSHIP_FIELDS: &[&str] =
    &["fromContactId", "toContactId", "type", "notes", "deletedAt"];
pub const NOTE_FIELDS: &[&str] = &["contactId", "body", "isSensitive", "authorId", "deletedAt"];

pub fn fields(entity_type: &str) -> &'static [&'static str] {
    match entity_type {
        "contact" => CONTACT_FIELDS,
        "tag" => TAG_FIELDS,
        "category" => CATEGORY_FIELDS,
        "organization" => ORGANIZATION_FIELDS,
        "relationship" => RELATIONSHIP_FIELDS,
        "note" => NOTE_FIELDS,
        _ => &[],
    }
}

pub fn table(entity_type: &str) -> Option<&'static str> {
    Some(match entity_type {
        "contact" => "contacts",
        "tag" => "tags",
        "category" => "categories",
        "organization" => "organizations",
        "relationship" => "relationships",
        "note" => "notes",
        _ => return None,
    })
}

/// Contacts whose search documents the caller may want to refresh further
/// (the desktop re-embeds them), and rule categories that must be
/// re-evaluated over the archive once the transaction has committed.
#[derive(Debug, Default)]
pub struct Applied {
    pub contacts: Vec<String>,
    pub categories: Vec<String>,
}

// -- reading -----------------------------------------------------------------

fn text(row: &rusqlite::Row<'_>, column: &str) -> rusqlite::Result<Value> {
    Ok(row.get::<_, Option<String>>(column)?.map(Value::String).unwrap_or(Value::Null))
}

fn flag(row: &rusqlite::Row<'_>, column: &str) -> rusqlite::Result<Value> {
    Ok(Value::Bool(row.get::<_, i64>(column)? != 0))
}

fn column_list(
    connection: &Connection,
    sql: &str,
    id: &str,
    map: impl Fn(&rusqlite::Row<'_>) -> rusqlite::Result<Value>,
) -> Result<Vec<Value>> {
    let mut statement = connection.prepare(sql)?;
    let rows = statement.query_map(params![id], |row| map(row))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// The record as it stands, or `None` when this database has never held it.
pub fn load(connection: &Connection, entity_type: &str, id: &str) -> Result<Option<Value>> {
    match entity_type {
        "contact" => load_contact(connection, id),
        "tag" => Ok(connection
            .query_row("SELECT * FROM tags WHERE id = ?1", params![id], |row| {
                Ok(json!({
                    "id": row.get::<_, String>("id")?,
                    "name": row.get::<_, String>("name")?,
                    "color": text(row, "color")?,
                    "description": text(row, "description")?,
                    "createdAt": row.get::<_, String>("created_at")?,
                    "updatedAt": row.get::<_, String>("updated_at")?,
                    "deletedAt": text(row, "deleted_at")?,
                }))
            })
            .optional()?),
        "category" => Ok(connection
            .query_row("SELECT * FROM categories WHERE id = ?1", params![id], |row| {
                let rule: Option<String> = row.get("rule")?;
                Ok(json!({
                    "id": row.get::<_, String>("id")?,
                    "name": row.get::<_, String>("name")?,
                    "description": text(row, "description")?,
                    "parentId": text(row, "parent_id")?,
                    "icon": text(row, "icon")?,
                    "color": text(row, "color")?,
                    "rule": rule.and_then(|raw| serde_json::from_str::<Value>(&raw).ok()).unwrap_or(Value::Null),
                    "sortOrder": row.get::<_, i64>("sort_order")?,
                    "showOnHome": flag(row, "show_on_home")?,
                    "createdAt": row.get::<_, String>("created_at")?,
                    "updatedAt": row.get::<_, String>("updated_at")?,
                    "deletedAt": text(row, "deleted_at")?,
                }))
            })
            .optional()?),
        "organization" => Ok(connection
            .query_row("SELECT * FROM organizations WHERE id = ?1", params![id], |row| {
                Ok(json!({
                    "id": row.get::<_, String>("id")?,
                    "name": row.get::<_, String>("name")?,
                    "kind": row.get::<_, String>("kind")?,
                    "city": text(row, "city")?,
                    "region": text(row, "region")?,
                    "country": text(row, "country")?,
                    "address": text(row, "address")?,
                    "notes": text(row, "notes")?,
                    "createdAt": row.get::<_, String>("created_at")?,
                    "updatedAt": row.get::<_, String>("updated_at")?,
                    "deletedAt": text(row, "deleted_at")?,
                }))
            })
            .optional()?),
        "relationship" => Ok(connection
            .query_row("SELECT * FROM relationships WHERE id = ?1", params![id], |row| {
                Ok(json!({
                    "id": row.get::<_, String>("id")?,
                    "fromContactId": row.get::<_, String>("from_contact_id")?,
                    "toContactId": row.get::<_, String>("to_contact_id")?,
                    "type": row.get::<_, String>("type")?,
                    "notes": text(row, "notes")?,
                    "createdAt": row.get::<_, String>("created_at")?,
                    "updatedAt": row.get::<_, String>("updated_at")?,
                    "deletedAt": text(row, "deleted_at")?,
                }))
            })
            .optional()?),
        "note" => Ok(connection
            .query_row("SELECT * FROM notes WHERE id = ?1", params![id], |row| {
                Ok(json!({
                    "id": row.get::<_, String>("id")?,
                    "contactId": row.get::<_, String>("contact_id")?,
                    "body": row.get::<_, String>("body")?,
                    "isSensitive": flag(row, "is_sensitive")?,
                    "authorId": text(row, "author_id")?,
                    "createdAt": row.get::<_, String>("created_at")?,
                    "updatedAt": row.get::<_, String>("updated_at")?,
                    "deletedAt": text(row, "deleted_at")?,
                }))
            })
            .optional()?),
        other => Err(DbError::Validation(format!("סוג רשומה לא מסונכרן: {other}"))),
    }
}

fn load_contact(connection: &Connection, id: &str) -> Result<Option<Value>> {
    let base = connection
        .query_row("SELECT * FROM contacts WHERE id = ?1", params![id], |row| {
            Ok(json!({
                "id": row.get::<_, String>("id")?,
                "firstName": text(row, "first_name")?,
                "lastName": text(row, "last_name")?,
                "displayName": row.get::<_, String>("display_name")?,
                "prefix": text(row, "prefix")?,
                "title": text(row, "title")?,
                "country": text(row, "country")?,
                "region": text(row, "region")?,
                "city": text(row, "city")?,
                "address": text(row, "address")?,
                "postalCode": text(row, "postal_code")?,
                "profession": text(row, "profession")?,
                "role": text(row, "role")?,
                "notes": text(row, "notes")?,
                "reasonForSaving": text(row, "reason_for_saving")?,
                "source": text(row, "source")?,
                "introducedBy": text(row, "introduced_by")?,
                "introducedByContactId": text(row, "introduced_by_contact_id")?,
                "isFavorite": flag(row, "is_favorite")?,
                "createdAt": row.get::<_, String>("created_at")?,
                "updatedAt": row.get::<_, String>("updated_at")?,
                "deletedAt": text(row, "deleted_at")?,
            }))
        })
        .optional()?;
    let Some(Value::Object(mut state)) = base else {
        return Ok(None);
    };

    state.insert(
        "phones".into(),
        Value::Array(column_list(
            connection,
            "SELECT id, kind, raw, label, is_primary, country_code FROM contact_phones
              WHERE contact_id = ?1 AND deleted_at IS NULL ORDER BY rowid",
            id,
            |row| {
                Ok(json!({
                    "id": row.get::<_, String>(0)?,
                    "kind": row.get::<_, String>(1)?,
                    "raw": row.get::<_, String>(2)?,
                    "label": row.get::<_, Option<String>>(3)?,
                    "isPrimary": row.get::<_, i64>(4)? != 0,
                    "countryCode": row.get::<_, Option<String>>(5)?,
                }))
            },
        )?),
    );
    state.insert(
        "emails".into(),
        Value::Array(column_list(
            connection,
            "SELECT id, kind, address, is_primary FROM contact_emails
              WHERE contact_id = ?1 AND deleted_at IS NULL ORDER BY rowid",
            id,
            |row| {
                Ok(json!({
                    "id": row.get::<_, String>(0)?,
                    "kind": row.get::<_, String>(1)?,
                    "address": row.get::<_, String>(2)?,
                    "isPrimary": row.get::<_, i64>(3)? != 0,
                }))
            },
        )?),
    );
    state.insert(
        "aliases".into(),
        Value::Array(column_list(
            connection,
            "SELECT id, kind, value, language_code FROM contact_aliases
              WHERE contact_id = ?1 AND deleted_at IS NULL ORDER BY rowid",
            id,
            |row| {
                Ok(json!({
                    "id": row.get::<_, String>(0)?,
                    "kind": row.get::<_, String>(1)?,
                    "value": row.get::<_, String>(2)?,
                    "languageCode": row.get::<_, Option<String>>(3)?,
                }))
            },
        )?),
    );
    state.insert(
        "specialties".into(),
        Value::Array(column_list(
            connection,
            "SELECT value FROM contact_specialties WHERE contact_id = ?1 AND deleted_at IS NULL
              ORDER BY rowid",
            id,
            |row| Ok(Value::String(row.get(0)?)),
        )?),
    );
    state.insert(
        "languages".into(),
        Value::Array(column_list(
            connection,
            "SELECT language_code FROM contact_languages WHERE contact_id = ?1 AND deleted_at IS NULL
              ORDER BY rowid",
            id,
            |row| Ok(Value::String(row.get(0)?)),
        )?),
    );
    state.insert(
        "tagIds".into(),
        Value::Array(column_list(
            connection,
            "SELECT tag_id FROM contact_tags WHERE contact_id = ?1 AND deleted_at IS NULL
              ORDER BY tag_id",
            id,
            |row| Ok(Value::String(row.get(0)?)),
        )?),
    );
    state.insert(
        "categories".into(),
        Value::Array(column_list(
            connection,
            "SELECT category_id, mode FROM contact_categories
              WHERE contact_id = ?1 AND deleted_at IS NULL ORDER BY category_id",
            id,
            |row| {
                Ok(json!({
                    "categoryId": row.get::<_, String>(0)?,
                    "mode": row.get::<_, String>(1)?,
                }))
            },
        )?),
    );
    state.insert(
        "organizations".into(),
        Value::Array(column_list(
            connection,
            "SELECT id, organization_id, role, is_primary, started_at, ended_at
               FROM contact_organizations
              WHERE contact_id = ?1 AND deleted_at IS NULL ORDER BY rowid",
            id,
            |row| {
                Ok(json!({
                    "id": row.get::<_, String>(0)?,
                    "organizationId": row.get::<_, String>(1)?,
                    "role": row.get::<_, Option<String>>(2)?,
                    "isPrimary": row.get::<_, i64>(3)? != 0,
                    "startedAt": row.get::<_, Option<String>>(4)?,
                    "endedAt": row.get::<_, Option<String>>(5)?,
                }))
            },
        )?),
    );
    Ok(Some(Value::Object(state)))
}

// -- writing -----------------------------------------------------------------

fn str_of(state: &Value, key: &str) -> Option<String> {
    state.get(key).and_then(Value::as_str).map(str::to_string)
}

fn bool_of(state: &Value, key: &str) -> bool {
    state.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn int_of(state: &Value, key: &str) -> i64 {
    state.get(key).and_then(Value::as_i64).unwrap_or(0)
}

fn list_of(state: &Value, key: &str) -> Vec<Value> {
    state.get(key).and_then(Value::as_array).cloned().unwrap_or_default()
}

fn required(state: &Value, key: &str) -> Result<String> {
    str_of(state, key)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| DbError::Validation(format!("רשומה מסונכרנת ללא {key}")))
}

fn digits_only(value: &str) -> String {
    value.chars().filter(|c| c.is_ascii_digit()).collect()
}

fn id_or_new(item: &Value) -> String {
    str_of(item, "id").filter(|id| !id.is_empty()).unwrap_or_else(new_id)
}

/// Write a record's state over the row, creating it when absent, and bring
/// the search index up to date. The local `version` counter still advances:
/// a form open on this record must learn that it moved.
pub fn apply(
    tx: &Transaction<'_>,
    entity_type: &str,
    id: &str,
    state: &Value,
    device_id: &str,
) -> Result<Applied> {
    let mut applied = Applied::default();
    let now = now_iso();
    let created_at = str_of(state, "createdAt").unwrap_or_else(|| now.clone());
    let updated_at = str_of(state, "updatedAt").unwrap_or_else(|| now.clone());
    let deleted_at = str_of(state, "deletedAt");

    match entity_type {
        "contact" => {
            let display_name = required(state, "displayName")?;
            let city = str_of(state, "city");
            let profession = str_of(state, "profession");
            tx.execute(
                "INSERT INTO contacts (id, first_name, last_name, display_name, prefix, title,
                                       normalized_name, country, region, city, address, postal_code,
                                       normalized_city, profession, role, normalized_profession,
                                       notes, reason_for_saving, source, introduced_by,
                                       introduced_by_contact_id, is_favorite, created_at,
                                       updated_at, version, device_id, deleted_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16,
                         ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, 1, ?25, ?26)
                 ON CONFLICT(id) DO UPDATE SET
                   first_name = excluded.first_name, last_name = excluded.last_name,
                   display_name = excluded.display_name, prefix = excluded.prefix,
                   title = excluded.title, normalized_name = excluded.normalized_name,
                   country = excluded.country, region = excluded.region, city = excluded.city,
                   address = excluded.address, postal_code = excluded.postal_code,
                   normalized_city = excluded.normalized_city, profession = excluded.profession,
                   role = excluded.role, normalized_profession = excluded.normalized_profession,
                   notes = excluded.notes, reason_for_saving = excluded.reason_for_saving,
                   source = excluded.source, introduced_by = excluded.introduced_by,
                   introduced_by_contact_id = excluded.introduced_by_contact_id,
                   is_favorite = excluded.is_favorite, updated_at = excluded.updated_at,
                   version = contacts.version + 1, device_id = excluded.device_id,
                   deleted_at = excluded.deleted_at",
                params![
                    id,
                    str_of(state, "firstName"),
                    str_of(state, "lastName"),
                    display_name.trim(),
                    str_of(state, "prefix"),
                    str_of(state, "title"),
                    normalize_name(&display_name),
                    str_of(state, "country"),
                    str_of(state, "region"),
                    city,
                    str_of(state, "address"),
                    str_of(state, "postalCode"),
                    city.as_deref().map(normalize_text),
                    profession,
                    str_of(state, "role"),
                    profession.as_deref().map(normalize_text),
                    str_of(state, "notes"),
                    str_of(state, "reasonForSaving"),
                    str_of(state, "source"),
                    str_of(state, "introducedBy"),
                    str_of(state, "introducedByContactId"),
                    i64::from(bool_of(state, "isFavorite")),
                    created_at,
                    updated_at,
                    device_id,
                    deleted_at,
                ],
            )?;
            write_contact_children(tx, id, state, &now, device_id)?;
            reindex_contact(tx, id)?;
            applied.contacts.push(id.to_string());
        }
        "tag" => {
            let name = required(state, "name")?;
            tx.execute(
                "INSERT INTO tags (id, name, normalized, color, description, created_at, updated_at,
                                   version, device_id, deleted_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, ?8, ?9)
                 ON CONFLICT(id) DO UPDATE SET
                   name = excluded.name, normalized = excluded.normalized, color = excluded.color,
                   description = excluded.description, updated_at = excluded.updated_at,
                   version = tags.version + 1, device_id = excluded.device_id,
                   deleted_at = excluded.deleted_at",
                params![
                    id,
                    name.trim(),
                    normalize_text(&name),
                    str_of(state, "color"),
                    str_of(state, "description"),
                    created_at,
                    updated_at,
                    device_id,
                    deleted_at,
                ],
            )?;
            if let Some(deleted) = &deleted_at {
                tx.execute(
                    "UPDATE contact_tags SET deleted_at = ?2 WHERE tag_id = ?1 AND deleted_at IS NULL",
                    params![id, deleted],
                )?;
            }
            // The name is indexed text on every carrier, live or just removed.
            for contact_id in
                ids(tx, "SELECT DISTINCT contact_id FROM contact_tags WHERE tag_id = ?1", id)?
            {
                reindex_contact(tx, &contact_id)?;
                applied.contacts.push(contact_id);
            }
        }
        "category" => {
            let name = required(state, "name")?;
            let rule = match state.get("rule") {
                Some(Value::Null) | None => None,
                Some(rule) => Some(serde_json::to_string(rule)?),
            };
            tx.execute(
                "INSERT INTO categories (id, name, normalized, description, parent_id, icon, color,
                                         rule, sort_order, show_on_home, created_at, updated_at,
                                         version, device_id, deleted_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, 1, ?13, ?14)
                 ON CONFLICT(id) DO UPDATE SET
                   name = excluded.name, normalized = excluded.normalized,
                   description = excluded.description, parent_id = excluded.parent_id,
                   icon = excluded.icon, color = excluded.color, rule = excluded.rule,
                   sort_order = excluded.sort_order, show_on_home = excluded.show_on_home,
                   updated_at = excluded.updated_at, version = categories.version + 1,
                   device_id = excluded.device_id, deleted_at = excluded.deleted_at",
                params![
                    id,
                    name.trim(),
                    normalize_text(&name),
                    str_of(state, "description"),
                    str_of(state, "parentId"),
                    str_of(state, "icon"),
                    str_of(state, "color"),
                    rule,
                    int_of(state, "sortOrder"),
                    i64::from(state.get("showOnHome").and_then(Value::as_bool).unwrap_or(true)),
                    created_at,
                    updated_at,
                    device_id,
                    deleted_at,
                ],
            )?;
            if let Some(deleted) = &deleted_at {
                tx.execute(
                    "UPDATE contact_categories SET deleted_at = ?2
                      WHERE category_id = ?1 AND deleted_at IS NULL",
                    params![id, deleted],
                )?;
                tx.execute("DELETE FROM category_matches WHERE category_id = ?1", params![id])?;
            } else {
                applied.categories.push(id.to_string());
            }
            for contact_id in ids(
                tx,
                "SELECT contact_id FROM category_matches WHERE category_id = ?1
                 UNION SELECT contact_id FROM contact_categories WHERE category_id = ?1",
                id,
            )? {
                reindex_contact(tx, &contact_id)?;
                applied.contacts.push(contact_id);
            }
        }
        "organization" => {
            let name = required(state, "name")?;
            tx.execute(
                "INSERT INTO organizations (id, name, normalized, kind, city, region, country,
                                            address, notes, created_at, updated_at, version,
                                            device_id, deleted_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 1, ?12, ?13)
                 ON CONFLICT(id) DO UPDATE SET
                   name = excluded.name, normalized = excluded.normalized, kind = excluded.kind,
                   city = excluded.city, region = excluded.region, country = excluded.country,
                   address = excluded.address, notes = excluded.notes,
                   updated_at = excluded.updated_at, version = organizations.version + 1,
                   device_id = excluded.device_id, deleted_at = excluded.deleted_at",
                params![
                    id,
                    name.trim(),
                    normalize_text(&name),
                    str_of(state, "kind").unwrap_or_else(|| "organization".into()),
                    str_of(state, "city"),
                    str_of(state, "region"),
                    str_of(state, "country"),
                    str_of(state, "address"),
                    str_of(state, "notes"),
                    created_at,
                    updated_at,
                    device_id,
                    deleted_at,
                ],
            )?;
            if let Some(deleted) = &deleted_at {
                tx.execute(
                    "UPDATE contact_organizations SET deleted_at = ?2
                      WHERE organization_id = ?1 AND deleted_at IS NULL",
                    params![id, deleted],
                )?;
            }
            for contact_id in ids(
                tx,
                "SELECT DISTINCT contact_id FROM contact_organizations WHERE organization_id = ?1",
                id,
            )? {
                reindex_contact(tx, &contact_id)?;
                applied.contacts.push(contact_id);
            }
        }
        "relationship" => {
            let from = required(state, "fromContactId")?;
            let to = required(state, "toContactId")?;
            if from == to {
                return Err(DbError::Validation("קשר של איש קשר לעצמו".into()));
            }
            tx.execute(
                "INSERT INTO relationships (id, from_contact_id, to_contact_id, type, notes,
                                            created_at, updated_at, version, device_id, deleted_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, ?8, ?9)
                 ON CONFLICT(id) DO UPDATE SET
                   from_contact_id = excluded.from_contact_id,
                   to_contact_id = excluded.to_contact_id, type = excluded.type,
                   notes = excluded.notes, updated_at = excluded.updated_at,
                   version = relationships.version + 1, device_id = excluded.device_id,
                   deleted_at = excluded.deleted_at",
                params![
                    id,
                    from,
                    to,
                    str_of(state, "type").unwrap_or_else(|| "knows".into()),
                    str_of(state, "notes"),
                    created_at,
                    updated_at,
                    device_id,
                    deleted_at,
                ],
            )?;
        }
        "note" => {
            let contact_id = required(state, "contactId")?;
            let previous_contact: Option<String> = tx
                .query_row("SELECT contact_id FROM notes WHERE id = ?1", params![id], |row| {
                    row.get(0)
                })
                .optional()?;
            tx.execute(
                "INSERT INTO notes (id, contact_id, body, is_sensitive, author_id, created_at,
                                    updated_at, version, device_id, deleted_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, ?8, ?9)
                 ON CONFLICT(id) DO UPDATE SET
                   contact_id = excluded.contact_id, body = excluded.body,
                   is_sensitive = excluded.is_sensitive, author_id = excluded.author_id,
                   updated_at = excluded.updated_at, version = notes.version + 1,
                   device_id = excluded.device_id, deleted_at = excluded.deleted_at",
                params![
                    id,
                    contact_id,
                    str_of(state, "body").unwrap_or_default(),
                    i64::from(bool_of(state, "isSensitive")),
                    str_of(state, "authorId"),
                    created_at,
                    updated_at,
                    device_id,
                    deleted_at,
                ],
            )?;
            reindex_contact(tx, &contact_id)?;
            applied.contacts.push(contact_id.clone());
            if let Some(previous) = previous_contact.filter(|previous| *previous != contact_id) {
                reindex_contact(tx, &previous)?;
                applied.contacts.push(previous);
            }
        }
        other => return Err(DbError::Validation(format!("סוג רשומה לא מסונכרן: {other}"))),
    }
    Ok(applied)
}

fn ids(tx: &Transaction<'_>, sql: &str, id: &str) -> Result<Vec<String>> {
    let mut statement = tx.prepare(sql)?;
    let rows = statement.query_map(params![id], |row| row.get::<_, String>(0))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Replace a contact's owned collections with the wire form's, keeping the
/// ids it carries so a later change to the same phone stays the same row
/// on every device.
fn write_contact_children(
    tx: &Transaction<'_>,
    contact_id: &str,
    state: &Value,
    now: &str,
    device: &str,
) -> Result<()> {
    for table in [
        "contact_phones",
        "contact_emails",
        "contact_aliases",
        "contact_specialties",
        "contact_languages",
        "contact_tags",
        "contact_categories",
        "contact_organizations",
    ] {
        tx.execute(&format!("DELETE FROM {table} WHERE contact_id = ?1"), params![contact_id])?;
    }
    let country = str_of(state, "country");

    for (index, phone) in list_of(state, "phones").iter().enumerate() {
        let Some(raw) = str_of(phone, "raw").filter(|raw| !raw.trim().is_empty()) else {
            continue;
        };
        tx.execute(
            "INSERT INTO contact_phones (id, contact_id, kind, raw, e164, digits, country_code,
                                         is_primary, label, created_at, updated_at, version, device_id)
             VALUES (?1, ?2, ?3, ?4, NULL, ?5, ?6, ?7, ?8, ?9, ?9, 1, ?10)",
            params![
                id_or_new(phone),
                contact_id,
                str_of(phone, "kind").unwrap_or_else(|| "mobile".into()),
                raw.trim(),
                digits_only(&raw),
                str_of(phone, "countryCode").or_else(|| country.clone()),
                i64::from(bool_of(phone, "isPrimary") || index == 0),
                str_of(phone, "label"),
                now,
                device,
            ],
        )?;
    }
    for (index, email) in list_of(state, "emails").iter().enumerate() {
        let Some(address) = str_of(email, "address").filter(|a| !a.trim().is_empty()) else {
            continue;
        };
        tx.execute(
            "INSERT INTO contact_emails (id, contact_id, kind, address, normalized, is_primary,
                                         created_at, updated_at, version, device_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7, 1, ?8)",
            params![
                id_or_new(email),
                contact_id,
                str_of(email, "kind").unwrap_or_else(|| "personal".into()),
                address.trim(),
                address.trim().to_lowercase(),
                i64::from(bool_of(email, "isPrimary") || index == 0),
                now,
                device,
            ],
        )?;
    }
    for alias in list_of(state, "aliases") {
        let Some(value) = str_of(&alias, "value").filter(|v| !v.trim().is_empty()) else {
            continue;
        };
        tx.execute(
            "INSERT INTO contact_aliases (id, contact_id, kind, value, normalized, language_code,
                                          created_at, updated_at, version, device_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7, 1, ?8)",
            params![
                id_or_new(&alias),
                contact_id,
                str_of(&alias, "kind").unwrap_or_else(|| "alias".into()),
                value.trim(),
                normalize_name(&value),
                str_of(&alias, "languageCode"),
                now,
                device,
            ],
        )?;
    }
    for specialty in list_of(state, "specialties") {
        let Some(value) = specialty.as_str().map(str::trim).filter(|v| !v.is_empty()) else {
            continue;
        };
        tx.execute(
            "INSERT INTO contact_specialties (id, contact_id, value, normalized, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![new_id(), contact_id, value, normalize_text(value), now],
        )?;
    }
    for language in list_of(state, "languages") {
        let Some(code) = language.as_str().filter(|v| !v.is_empty()) else {
            continue;
        };
        tx.execute(
            "INSERT INTO contact_languages (id, contact_id, language_code, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![new_id(), contact_id, code, now],
        )?;
    }
    for tag_id in list_of(state, "tagIds") {
        let Some(tag_id) = tag_id.as_str().filter(|v| !v.is_empty()) else {
            continue;
        };
        tx.execute(
            "INSERT OR IGNORE INTO contact_tags (id, contact_id, tag_id, created_at, updated_at,
                                                 version, device_id)
             VALUES (?1, ?2, ?3, ?4, ?4, 1, ?5)",
            params![new_id(), contact_id, tag_id, now, device],
        )?;
    }
    for membership in list_of(state, "categories") {
        let Some(category_id) = str_of(&membership, "categoryId").filter(|v| !v.is_empty()) else {
            continue;
        };
        let mode = str_of(&membership, "mode").unwrap_or_else(|| "include".into());
        if !matches!(mode.as_str(), "include" | "exclude") {
            continue;
        }
        tx.execute(
            "INSERT OR IGNORE INTO contact_categories (id, contact_id, category_id, mode, created_at,
                                                       updated_at, version, device_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5, 1, ?6)",
            params![new_id(), contact_id, category_id, mode, now, device],
        )?;
    }
    for link in list_of(state, "organizations") {
        let Some(organization_id) = str_of(&link, "organizationId").filter(|v| !v.is_empty())
        else {
            continue;
        };
        tx.execute(
            "INSERT OR IGNORE INTO contact_organizations (id, contact_id, organization_id, role,
                                                          is_primary, started_at, ended_at,
                                                          created_at, updated_at, version, device_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, 1, ?9)",
            params![
                id_or_new(&link),
                contact_id,
                organization_id,
                str_of(&link, "role"),
                i64::from(bool_of(&link, "isPrimary")),
                str_of(&link, "startedAt"),
                str_of(&link, "endedAt"),
                now,
                device,
            ],
        )?;
    }
    Ok(())
}

/// Restrict a state to some of its fields — what a journal entry records
/// about a remote change, and what a conflict shows side by side.
pub fn project(state: &Value, keys: &[String]) -> Value {
    let mut map = Map::new();
    for key in keys {
        map.insert(key.clone(), state.get(key).cloned().unwrap_or(Value::Null));
    }
    Value::Object(map)
}
