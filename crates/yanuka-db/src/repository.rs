//! Contact reads and writes.
//!
//! Every mutating function follows the same shape: open a transaction, write
//! the row, append a mutation-log entry, rebuild the search index, commit. All
//! four happen together or none of them do — an index that disagrees with the
//! data, or a change that never syncs, are both worse than a failed write.

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde_json::json;
use yanuka_search::{normalize_name, normalize_text};

use crate::error::{DbError, Result};
use crate::index::reindex_contact;
use crate::models::*;
use crate::mutation::{self, Operation};
use crate::{new_id, now_iso};

/// Identifier for this installation, stamped on every write so the sync engine
/// can tell which device produced a revision.
pub fn device_id(connection: &Connection) -> Result<String> {
    let existing: Option<String> = connection
        .query_row("SELECT value FROM app_meta WHERE key = 'device_id'", [], |row| row.get(0))
        .optional()?;

    if let Some(id) = existing {
        return Ok(id);
    }

    let id = new_id();
    connection.execute(
        "INSERT INTO app_meta (key, value, updated_at) VALUES ('device_id', ?1, ?2)",
        params![id, now_iso()],
    )?;
    Ok(id)
}

fn digits_only(value: &str) -> String {
    value.chars().filter(|c| c.is_ascii_digit()).collect()
}

/// Map a `contacts` row onto the domain struct. Public so the search module
/// can hydrate its candidates through the same mapping.
pub fn contact_from_row(row: &Row<'_>) -> rusqlite::Result<Contact> {
    Ok(Contact {
        id: row.get("id")?,
        first_name: row.get("first_name")?,
        last_name: row.get("last_name")?,
        display_name: row.get("display_name")?,
        prefix: row.get("prefix")?,
        title: row.get("title")?,
        country: row.get("country")?,
        region: row.get("region")?,
        city: row.get("city")?,
        address: row.get("address")?,
        postal_code: row.get("postal_code")?,
        profession: row.get("profession")?,
        role: row.get("role")?,
        notes: row.get("notes")?,
        reason_for_saving: row.get("reason_for_saving")?,
        source: row.get("source")?,
        introduced_by: row.get("introduced_by")?,
        introduced_by_contact_id: row.get("introduced_by_contact_id")?,
        is_favorite: row.get::<_, i64>("is_favorite")? != 0,
        last_viewed_at: row.get("last_viewed_at")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
        created_by: row.get("created_by")?,
        updated_by: row.get("updated_by")?,
        version: row.get("version")?,
        device_id: row.get("device_id")?,
        deleted_at: row.get("deleted_at")?,
    })
}

fn validate(input: &ContactInput) -> Result<()> {
    // Revalidated here rather than trusting the frontend: a webview is not a
    // trust boundary. Only the name is required — the whole product depends on
    // being able to store a person you barely remember.
    if input.display_name.trim().is_empty() {
        return Err(DbError::Validation("יש להזין שם".into()));
    }
    if input.display_name.chars().count() > 200 {
        return Err(DbError::Validation("שם ארוך מדי".into()));
    }
    if let Some(country) = &input.country {
        if !country.is_empty()
            && (country.len() != 2 || !country.chars().all(|c| c.is_ascii_uppercase()))
        {
            return Err(DbError::Validation("קוד מדינה אינו תקין".into()));
        }
    }
    Ok(())
}

/// Insert a contact and everything hanging off it.
pub fn create_contact(
    connection: &mut Connection,
    input: &ContactInput,
    id: Option<String>,
) -> Result<ContactWithRelations> {
    validate(input)?;

    let device = device_id(connection)?;
    // Caller-supplied so an offline create can be retried without duplicating.
    let contact_id = id.unwrap_or_else(new_id);
    let now = now_iso();

    let tx = connection.transaction()?;
    tx.execute(
        "INSERT INTO contacts (id, first_name, last_name, display_name, prefix, title,
                               normalized_name, country, region, city, address, postal_code,
                               normalized_city, profession, role, normalized_profession, notes,
                               reason_for_saving, source, introduced_by, introduced_by_contact_id,
                               is_favorite, created_at, updated_at, version, device_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17,
                 ?18, ?19, ?20, ?21, ?22, ?23, ?24, 1, ?25)",
        params![
            contact_id,
            input.first_name,
            input.last_name,
            input.display_name.trim(),
            input.prefix,
            input.title,
            normalize_name(&input.display_name),
            input.country,
            input.region,
            input.city,
            input.address,
            input.postal_code,
            input.city.as_deref().map(normalize_text),
            input.profession,
            input.role,
            input.profession.as_deref().map(normalize_text),
            input.notes,
            input.reason_for_saving,
            input.source,
            input.introduced_by,
            input.introduced_by_contact_id,
            i64::from(input.is_favorite),
            now,
            now,
            device,
        ],
    )?;

    write_children(&tx, &contact_id, input, &now, &device)?;

    mutation::record(
        &tx,
        mutation::NewMutation {
            entity_type: "contact",
            entity_id: &contact_id,
            operation: Operation::Create,
            payload: Some(&json!({ "displayName": input.display_name })),
            previous: None,
            base_version: 0,
            device_id: &device,
        },
    )?;
    reindex_contact(&tx, &contact_id)?;
    tx.commit()?;

    get_contact(connection, &contact_id)?.ok_or_else(|| DbError::NotFound("איש הקשר".into()))
}

/// Replace a contact's child collections.
///
/// Full replace rather than a per-row diff: the form submits the whole set, and
/// reconciling identity across an unordered list of phone numbers costs more
/// than it saves at this scale.
fn write_children(
    tx: &rusqlite::Transaction<'_>,
    contact_id: &str,
    input: &ContactInput,
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
    ] {
        tx.execute(&format!("DELETE FROM {table} WHERE contact_id = ?1"), params![contact_id])?;
    }

    for (index, phone) in input.phones.iter().enumerate() {
        if phone.raw.trim().is_empty() {
            continue;
        }
        // `raw` is stored exactly as entered. Historical notebook numbers are
        // frequently unparseable, and the original text is often the only clue
        // about who the number belongs to.
        tx.execute(
            "INSERT INTO contact_phones (id, contact_id, kind, raw, e164, digits, country_code,
                                         is_primary, label, created_at, updated_at, version, device_id)
             VALUES (?1, ?2, ?3, ?4, NULL, ?5, ?6, ?7, ?8, ?9, ?10, 1, ?11)",
            params![
                phone.id.clone().unwrap_or_else(new_id),
                contact_id,
                phone.kind.clone().unwrap_or_else(|| "mobile".into()),
                phone.raw.trim(),
                digits_only(&phone.raw),
                input.country,
                i64::from(phone.is_primary || index == 0),
                phone.label,
                now,
                now,
                device,
            ],
        )?;
    }

    for (index, email) in input.emails.iter().enumerate() {
        if email.address.trim().is_empty() {
            continue;
        }
        tx.execute(
            "INSERT INTO contact_emails (id, contact_id, kind, address, normalized, is_primary,
                                         created_at, updated_at, version, device_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 1, ?9)",
            params![
                email.id.clone().unwrap_or_else(new_id),
                contact_id,
                email.kind.clone().unwrap_or_else(|| "personal".into()),
                email.address.trim(),
                email.address.trim().to_lowercase(),
                i64::from(email.is_primary || index == 0),
                now,
                now,
                device,
            ],
        )?;
    }

    for alias in &input.aliases {
        if alias.value.trim().is_empty() {
            continue;
        }
        tx.execute(
            "INSERT INTO contact_aliases (id, contact_id, kind, value, normalized, language_code,
                                          created_at, updated_at, version, device_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 1, ?9)",
            params![
                alias.id.clone().unwrap_or_else(new_id),
                contact_id,
                alias.kind.clone().unwrap_or_else(|| "alias".into()),
                alias.value.trim(),
                normalize_name(&alias.value),
                alias.language_code,
                now,
                now,
                device,
            ],
        )?;
    }

    for specialty in &input.specialties {
        if specialty.trim().is_empty() {
            continue;
        }
        tx.execute(
            "INSERT INTO contact_specialties (id, contact_id, value, normalized, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![new_id(), contact_id, specialty.trim(), normalize_text(specialty), now],
        )?;
    }

    for language in &input.languages {
        tx.execute(
            "INSERT INTO contact_languages (id, contact_id, language_code, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![new_id(), contact_id, language, now],
        )?;
    }

    for tag_id in &input.tag_ids {
        tx.execute(
            "INSERT INTO contact_tags (id, contact_id, tag_id, created_at, updated_at, version, device_id)
             VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6)",
            params![new_id(), contact_id, tag_id, now, now, device],
        )?;
    }

    for category_id in &input.category_ids {
        tx.execute(
            "INSERT INTO contact_categories (id, contact_id, category_id, created_at, updated_at, version, device_id)
             VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6)",
            params![new_id(), contact_id, category_id, now, now, device],
        )?;
    }

    Ok(())
}

/// Update a contact.
///
/// When `base_version` is supplied and no longer current, the write is refused.
/// That turns a silent lost update into a visible conflict the user can resolve
/// — the alternative is losing whichever edit arrived first, with no trace.
pub fn update_contact(
    connection: &mut Connection,
    id: &str,
    input: &ContactInput,
    base_version: Option<i64>,
) -> Result<ContactWithRelations> {
    validate(input)?;

    let current: (i64, String) = connection
        .query_row("SELECT version, display_name FROM contacts WHERE id = ?1", params![id], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .optional()?
        .ok_or_else(|| DbError::NotFound("איש הקשר".into()))?;

    if let Some(expected) = base_version {
        if expected != current.0 {
            return Err(DbError::StaleVersion { expected, actual: current.0 });
        }
    }

    let device = device_id(connection)?;
    let now = now_iso();
    let next_version = current.0 + 1;

    let tx = connection.transaction()?;
    tx.execute(
        "UPDATE contacts
            SET first_name = ?2, last_name = ?3, display_name = ?4, prefix = ?5, title = ?6,
                normalized_name = ?7, country = ?8, region = ?9, city = ?10, address = ?11,
                postal_code = ?12, normalized_city = ?13, profession = ?14, role = ?15,
                normalized_profession = ?16, notes = ?17, reason_for_saving = ?18, source = ?19,
                introduced_by = ?20, introduced_by_contact_id = ?21, is_favorite = ?22,
                updated_at = ?23, version = ?24, device_id = ?25
          WHERE id = ?1",
        params![
            id,
            input.first_name,
            input.last_name,
            input.display_name.trim(),
            input.prefix,
            input.title,
            normalize_name(&input.display_name),
            input.country,
            input.region,
            input.city,
            input.address,
            input.postal_code,
            input.city.as_deref().map(normalize_text),
            input.profession,
            input.role,
            input.profession.as_deref().map(normalize_text),
            input.notes,
            input.reason_for_saving,
            input.source,
            input.introduced_by,
            input.introduced_by_contact_id,
            i64::from(input.is_favorite),
            now,
            next_version,
            device,
        ],
    )?;

    write_children(&tx, id, input, &now, &device)?;

    mutation::record(
        &tx,
        mutation::NewMutation {
            entity_type: "contact",
            entity_id: id,
            operation: Operation::Update,
            payload: Some(&json!({ "displayName": input.display_name })),
            previous: Some(&json!({ "displayName": current.1 })),
            base_version: current.0,
            device_id: &device,
        },
    )?;
    reindex_contact(&tx, id)?;
    tx.commit()?;

    get_contact(connection, id)?.ok_or_else(|| DbError::NotFound("איש הקשר".into()))
}

/// Soft delete. The row survives so the deletion can propagate and be undone.
pub fn delete_contact(connection: &mut Connection, id: &str) -> Result<()> {
    let version: i64 = connection
        .query_row("SELECT version FROM contacts WHERE id = ?1", params![id], |row| row.get(0))
        .optional()?
        .ok_or_else(|| DbError::NotFound("איש הקשר".into()))?;

    let device = device_id(connection)?;
    let now = now_iso();

    let tx = connection.transaction()?;
    tx.execute(
        "UPDATE contacts SET deleted_at = ?2, updated_at = ?2, version = ?3, device_id = ?4
          WHERE id = ?1",
        params![id, now, version + 1, device],
    )?;
    mutation::record(
        &tx,
        mutation::NewMutation {
            entity_type: "contact",
            entity_id: id,
            operation: Operation::Delete,
            payload: None,
            previous: None,
            base_version: version,
            device_id: &device,
        },
    )?;
    // Drops the contact out of the index; the row itself stays.
    reindex_contact(&tx, id)?;
    tx.commit()?;
    Ok(())
}

pub fn restore_contact(connection: &mut Connection, id: &str) -> Result<ContactWithRelations> {
    let version: i64 = connection
        .query_row("SELECT version FROM contacts WHERE id = ?1", params![id], |row| row.get(0))
        .optional()?
        .ok_or_else(|| DbError::NotFound("איש הקשר".into()))?;

    let device = device_id(connection)?;
    let now = now_iso();

    let tx = connection.transaction()?;
    tx.execute(
        "UPDATE contacts SET deleted_at = NULL, updated_at = ?2, version = ?3, device_id = ?4
          WHERE id = ?1",
        params![id, now, version + 1, device],
    )?;
    mutation::record(
        &tx,
        mutation::NewMutation {
            entity_type: "contact",
            entity_id: id,
            operation: Operation::Update,
            payload: Some(&json!({ "deletedAt": null })),
            previous: None,
            base_version: version,
            device_id: &device,
        },
    )?;
    reindex_contact(&tx, id)?;
    tx.commit()?;

    get_contact(connection, id)?.ok_or_else(|| DbError::NotFound("איש הקשר".into()))
}

pub fn set_favorite(connection: &Connection, id: &str, is_favorite: bool) -> Result<()> {
    let changed = connection.execute(
        "UPDATE contacts SET is_favorite = ?2, updated_at = ?3, version = version + 1 WHERE id = ?1",
        params![id, i64::from(is_favorite), now_iso()],
    )?;
    if changed == 0 {
        return Err(DbError::NotFound("איש הקשר".into()));
    }
    Ok(())
}

/// Record that a contact was opened.
///
/// Not a versioned change and not logged as a mutation: reading a record is not
/// an edit, and turning every view into something other devices must reconcile
/// would flood the sync queue for no benefit.
pub fn touch_contact(connection: &Connection, id: &str) -> Result<()> {
    connection
        .execute("UPDATE contacts SET last_viewed_at = ?2 WHERE id = ?1", params![id, now_iso()])?;
    Ok(())
}

pub fn get_contact(connection: &Connection, id: &str) -> Result<Option<ContactWithRelations>> {
    let contact = connection
        .query_row("SELECT * FROM contacts WHERE id = ?1", params![id], contact_from_row)
        .optional()?;

    let Some(contact) = contact else {
        return Ok(None);
    };

    let mut phones = connection.prepare(
        "SELECT * FROM contact_phones WHERE contact_id = ?1 AND deleted_at IS NULL
          ORDER BY is_primary DESC, created_at",
    )?;
    let phones: Vec<ContactPhone> = phones
        .query_map(params![id], |row| {
            Ok(ContactPhone {
                id: row.get("id")?,
                contact_id: row.get("contact_id")?,
                kind: row.get("kind")?,
                raw: row.get("raw")?,
                e164: row.get("e164")?,
                digits: row.get("digits")?,
                country_code: row.get("country_code")?,
                is_primary: row.get::<_, i64>("is_primary")? != 0,
                label: row.get("label")?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut emails = connection
        .prepare("SELECT * FROM contact_emails WHERE contact_id = ?1 AND deleted_at IS NULL")?;
    let emails: Vec<ContactEmail> = emails
        .query_map(params![id], |row| {
            Ok(ContactEmail {
                id: row.get("id")?,
                contact_id: row.get("contact_id")?,
                kind: row.get("kind")?,
                address: row.get("address")?,
                normalized: row.get("normalized")?,
                is_primary: row.get::<_, i64>("is_primary")? != 0,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut aliases = connection
        .prepare("SELECT * FROM contact_aliases WHERE contact_id = ?1 AND deleted_at IS NULL")?;
    let aliases: Vec<ContactAlias> = aliases
        .query_map(params![id], |row| {
            Ok(ContactAlias {
                id: row.get("id")?,
                contact_id: row.get("contact_id")?,
                kind: row.get("kind")?,
                value: row.get("value")?,
                normalized: row.get("normalized")?,
                language_code: row.get("language_code")?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut tags = connection.prepare(
        "SELECT t.* FROM contact_tags ct JOIN tags t ON t.id = ct.tag_id
          WHERE ct.contact_id = ?1 AND ct.deleted_at IS NULL AND t.deleted_at IS NULL",
    )?;
    let tags: Vec<Tag> = tags
        .query_map(params![id], |row| {
            Ok(Tag {
                id: row.get("id")?,
                name: row.get("name")?,
                normalized: row.get("normalized")?,
                color: row.get("color")?,
                description: row.get("description")?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut categories = connection.prepare(
        "SELECT c.* FROM contact_categories cc JOIN categories c ON c.id = cc.category_id
          WHERE cc.contact_id = ?1 AND cc.deleted_at IS NULL AND c.deleted_at IS NULL",
    )?;
    let categories: Vec<Category> = categories
        .query_map(params![id], |row| {
            Ok(Category {
                id: row.get("id")?,
                name: row.get("name")?,
                normalized: row.get("normalized")?,
                description: row.get("description")?,
                parent_id: row.get("parent_id")?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let specialties = simple_column(
        connection,
        "SELECT value FROM contact_specialties WHERE contact_id = ?1 AND deleted_at IS NULL",
        id,
    )?;
    let languages = simple_column(
        connection,
        "SELECT language_code FROM contact_languages WHERE contact_id = ?1 AND deleted_at IS NULL",
        id,
    )?;

    Ok(Some(ContactWithRelations {
        contact,
        phones,
        emails,
        aliases,
        tags,
        categories,
        specialties,
        languages,
    }))
}

fn simple_column(connection: &Connection, sql: &str, id: &str) -> Result<Vec<String>> {
    let mut statement = connection.prepare(sql)?;
    let rows = statement.query_map(params![id], |row| row.get::<_, String>(0))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Browse contacts with keyset pagination.
///
/// The cursor is the last row of the previous page rather than an offset, so
/// page nine hundred costs what page one costs. `OFFSET 99950` would make
/// SQLite walk every skipped row, which breaks the stated performance target at
/// exactly the scale the product promises to handle.
pub fn list_contacts(
    connection: &Connection,
    cursor: Option<&str>,
    limit: i64,
    starts_with: Option<&str>,
) -> Result<Page<ContactSummary>> {
    let mut sql = String::from("SELECT c.* FROM contacts c WHERE c.deleted_at IS NULL");
    let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(prefix) = starts_with {
        sql.push_str(" AND c.normalized_name >= ?");
        args.push(Box::new(normalize_name(prefix)));
        sql.push_str(" AND c.normalized_name < ?");
        // Upper bound of the prefix range, so the index can be scanned rather
        // than every row tested.
        args.push(Box::new(format!("{}\u{10FFFF}", normalize_name(prefix))));
    }

    if let Some(after) = cursor {
        sql.push_str(
            " AND (c.display_name, c.id) > (SELECT display_name, id FROM contacts WHERE id = ?)",
        );
        args.push(Box::new(after.to_string()));
    }

    sql.push_str(" ORDER BY c.display_name, c.id LIMIT ?");
    args.push(Box::new(limit));

    let mut statement = connection.prepare(&sql)?;
    let params_ref: Vec<&dyn rusqlite::ToSql> = args.iter().map(|a| a.as_ref()).collect();
    let contacts: Vec<Contact> = statement
        .query_map(params_ref.as_slice(), contact_from_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let total: i64 = connection.query_row(
        "SELECT COUNT(*) FROM contacts WHERE deleted_at IS NULL",
        [],
        |row| row.get(0),
    )?;

    let next_cursor =
        if contacts.len() as i64 == limit { contacts.last().map(|c| c.id.clone()) } else { None };

    let items = contacts
        .into_iter()
        .map(|contact| summarize(connection, contact))
        .collect::<Result<Vec<_>>>()?;

    Ok(Page { items, next_cursor, total })
}

/// Project a contact into the list/search shape, attaching its primary phone
/// and tag names.
pub fn summarize(connection: &Connection, contact: Contact) -> Result<ContactSummary> {
    let primary_phone: Option<String> = connection
        .query_row(
            "SELECT raw FROM contact_phones WHERE contact_id = ?1 AND deleted_at IS NULL
              ORDER BY is_primary DESC, created_at LIMIT 1",
            params![contact.id],
            |row| row.get(0),
        )
        .optional()?;

    let mut statement = connection.prepare(
        "SELECT t.name FROM contact_tags ct JOIN tags t ON t.id = ct.tag_id
          WHERE ct.contact_id = ?1 AND ct.deleted_at IS NULL AND t.deleted_at IS NULL",
    )?;
    let tags: Vec<String> = statement
        .query_map(params![contact.id], |row| row.get(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(ContactSummary {
        id: contact.id,
        display_name: contact.display_name,
        prefix: contact.prefix,
        profession: contact.profession,
        role: contact.role,
        city: contact.city,
        country: contact.country,
        primary_phone,
        tags,
        is_favorite: contact.is_favorite,
        updated_at: contact.updated_at,
    })
}

pub fn recent_contacts(connection: &Connection, limit: i64) -> Result<Vec<ContactSummary>> {
    let mut statement = connection.prepare(
        "SELECT * FROM contacts WHERE deleted_at IS NULL AND last_viewed_at IS NOT NULL
          ORDER BY last_viewed_at DESC LIMIT ?1",
    )?;
    let contacts: Vec<Contact> = statement
        .query_map(params![limit], contact_from_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    contacts.into_iter().map(|c| summarize(connection, c)).collect()
}

pub fn favorite_contacts(connection: &Connection, limit: i64) -> Result<Vec<ContactSummary>> {
    let mut statement = connection.prepare(
        "SELECT * FROM contacts WHERE deleted_at IS NULL AND is_favorite = 1
          ORDER BY display_name LIMIT ?1",
    )?;
    let contacts: Vec<Contact> = statement
        .query_map(params![limit], contact_from_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    contacts.into_iter().map(|c| summarize(connection, c)).collect()
}
