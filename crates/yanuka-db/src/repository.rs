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

/// Write a new `contacts` row and its child collections.
///
/// Shared with the sync apply path, which has to produce byte-identical rows to
/// a local create without appending a mutation of its own — echoing a remote
/// change straight back to the device it came from is how a sync loop starts.
/// Keeping one writer is what guarantees the two paths cannot drift.
pub(crate) fn insert_contact_row(
    tx: &rusqlite::Transaction<'_>,
    contact_id: &str,
    input: &ContactInput,
    now: &str,
    device: &str,
    version: i64,
) -> Result<()> {
    tx.execute(
        "INSERT INTO contacts (id, first_name, last_name, display_name, prefix, title,
                               normalized_name, country, region, city, address, postal_code,
                               normalized_city, profession, role, normalized_profession, notes,
                               reason_for_saving, source, introduced_by, introduced_by_contact_id,
                               is_favorite, created_at, updated_at, version, device_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17,
                 ?18, ?19, ?20, ?21, ?22, ?23, ?23, ?24, ?25)",
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
            version,
            device,
        ],
    )?;
    write_children(tx, contact_id, input, now, device)
}

/// Overwrite an existing `contacts` row and replace its child collections.
///
/// The counterpart to `insert_contact_row`, and shared with the apply path for
/// the same reason.
pub(crate) fn update_contact_row(
    tx: &rusqlite::Transaction<'_>,
    contact_id: &str,
    input: &ContactInput,
    now: &str,
    device: &str,
    version: i64,
) -> Result<()> {
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
            version,
            device,
        ],
    )?;
    write_children(tx, contact_id, input, now, device)
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
    insert_contact_row(&tx, &contact_id, input, &now, &device, 1)?;

    // The whole record, because a create has no prior state to diff against and
    // a device replaying this log has nothing else to build the contact from.
    //
    // Read back rather than re-serialising `input`: the ids of the phones and
    // e-mail addresses are minted during the write, and a payload that carries
    // `"id": null` would have every device inventing its own id for the same
    // phone number — which surfaces later as duplicates that no merge can
    // reconcile, because nothing links the two rows.
    let snapshot = serde_json::to_value(as_input(
        &get_contact(&tx, &contact_id)?.ok_or_else(|| DbError::NotFound("איש הקשר".into()))?,
    ))?;
    mutation::record(
        &tx,
        mutation::NewMutation {
            entity_type: "contact",
            entity_id: &contact_id,
            operation: Operation::Create,
            payload: Some(&snapshot),
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
pub(crate) fn write_children(
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
        "contact_organizations",
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

    for (index, link) in input.organizations.iter().enumerate() {
        if link.organization_id.trim().is_empty() {
            continue;
        }
        tx.execute(
            "INSERT INTO contact_organizations (id, contact_id, organization_id, role, is_primary,
                                                started_at, ended_at, created_at, updated_at,
                                                version, device_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 1, ?10)",
            params![
                new_id(),
                contact_id,
                link.organization_id.trim(),
                link.role,
                i64::from(link.is_primary || index == 0),
                link.started_at,
                link.ended_at,
                now,
                now,
                device,
            ],
        )?;
    }

    Ok(())
}

/// Everything a stored contact would submit if the form re-sent it unchanged.
///
/// The round-trip that makes a patch merge safe: a field the caller left out
/// is refilled from here, so it is written back exactly as it was rather than
/// being dropped by the child-collection replace.
pub(crate) fn as_input(stored: &ContactWithRelations) -> ContactInput {
    ContactInput {
        first_name: stored.contact.first_name.clone(),
        last_name: stored.contact.last_name.clone(),
        display_name: stored.contact.display_name.clone(),
        prefix: stored.contact.prefix.clone(),
        title: stored.contact.title.clone(),
        country: stored.contact.country.clone(),
        region: stored.contact.region.clone(),
        city: stored.contact.city.clone(),
        address: stored.contact.address.clone(),
        postal_code: stored.contact.postal_code.clone(),
        profession: stored.contact.profession.clone(),
        role: stored.contact.role.clone(),
        notes: stored.contact.notes.clone(),
        reason_for_saving: stored.contact.reason_for_saving.clone(),
        source: stored.contact.source.clone(),
        introduced_by: stored.contact.introduced_by.clone(),
        introduced_by_contact_id: stored.contact.introduced_by_contact_id.clone(),
        is_favorite: stored.contact.is_favorite,
        phones: stored
            .phones
            .iter()
            .map(|phone| PhoneInput {
                id: Some(phone.id.clone()),
                kind: Some(phone.kind.clone()),
                raw: phone.raw.clone(),
                label: phone.label.clone(),
                is_primary: phone.is_primary,
            })
            .collect(),
        emails: stored
            .emails
            .iter()
            .map(|email| EmailInput {
                id: Some(email.id.clone()),
                kind: Some(email.kind.clone()),
                address: email.address.clone(),
                is_primary: email.is_primary,
            })
            .collect(),
        aliases: stored
            .aliases
            .iter()
            .map(|alias| AliasInput {
                id: Some(alias.id.clone()),
                kind: Some(alias.kind.clone()),
                value: alias.value.clone(),
                language_code: alias.language_code.clone(),
            })
            .collect(),
        specialties: stored.specialties.clone(),
        languages: stored.languages.clone(),
        tag_ids: stored.tags.iter().map(|tag| tag.id.clone()).collect(),
        category_ids: stored.categories.iter().map(|category| category.id.clone()).collect(),
        organizations: stored
            .organizations
            .iter()
            .map(|link| OrganizationLinkInput {
                organization_id: link.organization_id.clone(),
                role: link.role.clone(),
                is_primary: link.is_primary,
                started_at: link.started_at.clone(),
                ended_at: link.ended_at.clone(),
            })
            .collect(),
    }
}

/// Apply a patch to what is already stored.
///
/// `None` means the caller did not touch the field, `Some` means it did — the
/// distinction the `ContactPatch` type exists to carry. A screen therefore only
/// has to know about the fields it actually shows.
fn apply_patch(mut base: ContactInput, patch: &ContactPatch) -> ContactInput {
    macro_rules! set {
        ($($field:ident),* $(,)?) => {
            $(if let Some(value) = patch.$field.clone() {
                base.$field = value;
            })*
        };
    }

    set!(
        first_name,
        last_name,
        display_name,
        prefix,
        title,
        country,
        region,
        city,
        address,
        postal_code,
        profession,
        role,
        notes,
        reason_for_saving,
        source,
        introduced_by,
        introduced_by_contact_id,
        is_favorite,
        phones,
        emails,
        aliases,
        specialties,
        languages,
        tag_ids,
        category_ids,
        organizations,
    );

    base
}

/// Update a contact.
///
/// When `base_version` is supplied and no longer current, the write is refused.
/// That turns a silent lost update into a visible conflict the user can resolve
/// — the alternative is losing whichever edit arrived first, with no trace.
///
/// The patch is merged onto the stored record before it is written, because the
/// write itself replaces the child collections wholesale (`write_children`).
/// Merging first is what stops a form that renders phones but not e-mail
/// addresses from deleting the addresses every time it saves.
pub fn update_contact(
    connection: &mut Connection,
    id: &str,
    patch: &ContactPatch,
    base_version: Option<i64>,
) -> Result<ContactWithRelations> {
    let stored =
        get_contact(connection, id)?.ok_or_else(|| DbError::NotFound("איש הקשר".into()))?;
    let current = (stored.contact.version, stored.contact.display_name.clone());

    if let Some(expected) = base_version {
        if expected != current.0 {
            return Err(DbError::StaleVersion { expected, actual: current.0 });
        }
    }

    let before = as_input(&stored);
    let merged = apply_patch(before.clone(), patch);
    let input = &merged;
    validate(input)?;

    let device = device_id(connection)?;
    let now = now_iso();
    let next_version = current.0 + 1;

    let tx = connection.transaction()?;
    update_contact_row(&tx, id, input, &now, &device, next_version)?;

    // What actually moved, compared against the stored record rather than
    // against the patch: a patch that re-sends an unchanged field is not an
    // edit, and logging it as one would manufacture conflicts on other devices.
    // Both sides are read back from the database so the ids inside the child
    // collections line up and an unchanged phone list compares equal.
    let (changed, replaced) = mutation::changes(
        &serde_json::to_value(&before)?,
        &serde_json::to_value(as_input(
            &get_contact(&tx, id)?.ok_or_else(|| DbError::NotFound("איש הקשר".into()))?,
        ))?,
    );
    mutation::record(
        &tx,
        mutation::NewMutation {
            entity_type: "contact",
            entity_id: id,
            operation: Operation::Update,
            payload: Some(&changed),
            previous: Some(&replaced),
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

    let mut organizations = connection.prepare(
        "SELECT co.id, co.contact_id, co.organization_id, co.role, co.is_primary,
                co.started_at, co.ended_at, co.created_at,
                o.name, o.normalized, o.kind, o.city, o.region, o.country, o.address, o.notes
           FROM contact_organizations co JOIN organizations o ON o.id = co.organization_id
          WHERE co.contact_id = ?1 AND co.deleted_at IS NULL AND o.deleted_at IS NULL
          ORDER BY co.is_primary DESC, co.created_at",
    )?;
    let organizations: Vec<ContactOrganizationLink> = organizations
        .query_map(params![id], |row| {
            Ok(ContactOrganizationLink {
                id: row.get("id")?,
                contact_id: row.get("contact_id")?,
                organization_id: row.get("organization_id")?,
                role: row.get("role")?,
                is_primary: row.get::<_, i64>("is_primary")? != 0,
                started_at: row.get("started_at")?,
                ended_at: row.get("ended_at")?,
                created_at: row.get("created_at")?,
                organization: Organization {
                    id: row.get("organization_id")?,
                    name: row.get("name")?,
                    normalized: row.get("normalized")?,
                    kind: row.get("kind")?,
                    city: row.get("city")?,
                    region: row.get("region")?,
                    country: row.get("country")?,
                    address: row.get("address")?,
                    notes: row.get("notes")?,
                },
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    // Both directions in one pass; the far endpoint is summarized afterwards so
    // the row mapping never queries the connection it is borrowing.
    let mut edges = connection.prepare(
        "SELECT r.id, r.from_contact_id, r.to_contact_id, r.type, r.notes, r.created_at
           FROM relationships r
          WHERE (r.from_contact_id = ?1 OR r.to_contact_id = ?1) AND r.deleted_at IS NULL
          ORDER BY r.created_at",
    )?;
    let edges: Vec<(String, String, String, String, Option<String>, String)> = edges
        .query_map(params![id], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut relationships = Vec::with_capacity(edges.len());
    for (edge_id, from_id, to_id, kind, edge_notes, edge_created) in edges {
        let (direction, other_id) =
            if from_id == id { ("out", to_id.clone()) } else { ("in", from_id.clone()) };
        // A deleted far endpoint silently drops the edge rather than rendering
        // a link into a contact that no longer exists.
        let other = connection
            .query_row(
                "SELECT * FROM contacts WHERE id = ?1 AND deleted_at IS NULL",
                params![other_id],
                contact_from_row,
            )
            .optional()?;
        let Some(other) = other else {
            continue;
        };
        relationships.push(RelationshipEdge {
            id: edge_id,
            from_contact_id: from_id,
            to_contact_id: to_id,
            kind,
            notes: edge_notes,
            created_at: edge_created,
            direction: direction.to_string(),
            other_contact: summarize(connection, other)?,
        });
    }

    let mut contact_notes = connection.prepare(
        "SELECT id, contact_id, body, is_sensitive, author_id, created_at, updated_at
           FROM notes WHERE contact_id = ?1 AND deleted_at IS NULL
          ORDER BY created_at DESC",
    )?;
    let contact_notes: Vec<Note> = contact_notes
        .query_map(params![id], |row| {
            Ok(Note {
                id: row.get("id")?,
                contact_id: row.get("contact_id")?,
                body: row.get("body")?,
                is_sensitive: row.get::<_, i64>("is_sensitive")? != 0,
                author_id: row.get("author_id")?,
                created_at: row.get("created_at")?,
                updated_at: row.get("updated_at")?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(Some(ContactWithRelations {
        contact,
        phones,
        emails,
        aliases,
        tags,
        categories,
        specialties,
        languages,
        organizations,
        relationships,
        contact_notes,
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

/// The recycle bin: soft-deleted contacts, most recently deleted first.
///
/// Deliberately its own query rather than a flag on `list_contacts`. Every
/// other read in this module ends in `deleted_at IS NULL`, and a parameter
/// that can switch that off is one forgotten `false` away from putting deleted
/// people back into the ordinary list; this one can only ever return records
/// that are already gone.
pub fn deleted_contacts(connection: &Connection, limit: i64) -> Result<Vec<DeletedContact>> {
    let mut statement = connection.prepare(
        "SELECT * FROM contacts WHERE deleted_at IS NOT NULL
          ORDER BY deleted_at DESC, id DESC LIMIT ?1",
    )?;
    let contacts: Vec<Contact> = statement
        .query_map(params![limit], contact_from_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    contacts
        .into_iter()
        .map(|contact| {
            // The WHERE clause guarantees this, but an expect here would turn a
            // future query change into a panic on the user's machine.
            let deleted_at =
                contact.deleted_at.clone().unwrap_or_else(|| contact.updated_at.clone());
            Ok(DeletedContact { contact: summarize(connection, contact)?, deleted_at })
        })
        .collect()
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

/// One row of the mutation log, straight off the database.
///
/// Deliberately a separate struct from `apply::RemoteMutation`: the JSON columns
/// arrive as `Option<String>` and only become values once parsed, and folding
/// the two together would mean either parsing inside a rusqlite row callback —
/// where the error type cannot carry a serde failure — or pretending the text is
/// already structured.
pub(crate) struct MutationRow {
    pub id: String,
    pub entity_type: String,
    pub entity_id: String,
    pub operation: String,
    pub payload: Option<String>,
    pub previous: Option<String>,
    pub base_version: i64,
    pub created_at: String,
    pub device_id: String,
}
