//! IPC surface.
//!
//! Every function here is a thin adapter: take typed arguments, call
//! `yanuka-db`, return the result. No SQL, no business logic, no error
//! translation beyond `DbError`'s own serialization — all of which lives in the
//! crate below so it can be tested without a webview.
//!
//! The names must stay in step with `IPC_COMMANDS` in
//! `apps/desktop/src/lib/tauri-repository.ts`; `commands.test.ts` asserts that.

use serde_json::Value;
use tauri::State;
use yanuka_db::models::*;
use yanuka_db::{repository, search, taxonomy, DbError};

use crate::state::AppState;

type Answer<T> = Result<T, DbError>;

#[tauri::command]
pub fn search_contacts(state: State<'_, AppState>, input: SearchQuery) -> Answer<SearchResponse> {
    state.with(|connection| search::search(connection, &input))
}

/// Typeahead for the command palette.
///
/// Implemented on top of the same search rather than a separate query, so the
/// palette can never rank a contact differently from the results page.
#[tauri::command]
pub fn suggest_contacts(
    state: State<'_, AppState>,
    text: String,
    limit: Option<i64>,
) -> Answer<Vec<Value>> {
    state.with(|connection| {
        let response = search::search(
            connection,
            &SearchQuery { text, limit: Some(limit.unwrap_or(8)), ..Default::default() },
        )?;

        Ok(response
            .results
            .into_iter()
            .map(|result| {
                let sublabel = [result.contact.profession.clone(), result.contact.city.clone()]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join(" · ");
                serde_json::json!({
                    "kind": "contact",
                    "id": result.contact.id,
                    "label": result.contact.display_name,
                    "sublabel": if sublabel.is_empty() { Value::Null } else { Value::String(sublabel) },
                    "count": Value::Null,
                })
            })
            .collect())
    })
}

#[tauri::command]
pub fn list_contacts(state: State<'_, AppState>, input: Value) -> Answer<Page<ContactSummary>> {
    let cursor = input.get("cursor").and_then(Value::as_str).map(str::to_string);
    let limit = input.get("limit").and_then(Value::as_i64).unwrap_or(50);
    let starts_with = input.get("startsWith").and_then(Value::as_str).map(str::to_string);

    state.with(|connection| {
        repository::list_contacts(connection, cursor.as_deref(), limit, starts_with.as_deref())
    })
}

#[tauri::command]
pub fn get_contact(state: State<'_, AppState>, id: String) -> Answer<Option<ContactWithRelations>> {
    state.with(|connection| repository::get_contact(connection, &id))
}

#[tauri::command]
pub fn recent_contacts(
    state: State<'_, AppState>,
    limit: Option<i64>,
) -> Answer<Vec<ContactSummary>> {
    state.with(|connection| repository::recent_contacts(connection, limit.unwrap_or(8)))
}

#[tauri::command]
pub fn favorite_contacts(
    state: State<'_, AppState>,
    limit: Option<i64>,
) -> Answer<Vec<ContactSummary>> {
    state.with(|connection| repository::favorite_contacts(connection, limit.unwrap_or(12)))
}

#[tauri::command]
pub fn create_contact(
    state: State<'_, AppState>,
    input: ContactInput,
    id: Option<String>,
) -> Answer<ContactWithRelations> {
    state.with(|connection| repository::create_contact(connection, &input, id))
}

/// Minimal add: name, one phone, one remark. Everything else can wait.
#[tauri::command]
pub fn quick_add_contact(
    state: State<'_, AppState>,
    input: Value,
    id: Option<String>,
) -> Answer<ContactWithRelations> {
    let display_name =
        input.get("displayName").and_then(Value::as_str).unwrap_or_default().to_string();

    let mut contact = ContactInput { display_name, ..Default::default() };
    contact.notes = input.get("notes").and_then(Value::as_str).map(str::to_string);
    if let Some(phone) = input.get("phone").and_then(Value::as_str) {
        if !phone.trim().is_empty() {
            contact.phones =
                vec![PhoneInput { raw: phone.to_string(), is_primary: true, ..Default::default() }];
        }
    }

    state.with(|connection| repository::create_contact(connection, &contact, id))
}

#[tauri::command]
pub fn update_contact(
    state: State<'_, AppState>,
    id: String,
    patch: ContactInput,
    base_version: Option<i64>,
) -> Answer<ContactWithRelations> {
    state.with(|connection| repository::update_contact(connection, &id, &patch, base_version))
}

#[tauri::command]
pub fn delete_contact(state: State<'_, AppState>, id: String) -> Answer<()> {
    state.with(|connection| repository::delete_contact(connection, &id))
}

#[tauri::command]
pub fn restore_contact(state: State<'_, AppState>, id: String) -> Answer<ContactWithRelations> {
    state.with(|connection| repository::restore_contact(connection, &id))
}

#[tauri::command]
pub fn set_favorite(state: State<'_, AppState>, id: String, is_favorite: bool) -> Answer<()> {
    state.with(|connection| repository::set_favorite(connection, &id, is_favorite))
}

#[tauri::command]
pub fn touch_contact(state: State<'_, AppState>, id: String) -> Answer<()> {
    state.with(|connection| repository::touch_contact(connection, &id))
}

/// Warn before a near-duplicate is created.
///
/// A shared phone number is close to proof of the same person; a shared name is
/// barely a hint, because `כהן` is everywhere in this dataset. The result is a
/// warning, never a block — the user always decides.
#[tauri::command]
pub fn find_duplicates(
    state: State<'_, AppState>,
    input: Value,
    exclude_id: Option<String>,
) -> Answer<Vec<Value>> {
    let name = input.get("displayName").and_then(Value::as_str).unwrap_or_default();
    let phones: Vec<String> = input
        .get("phones")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|p| p.get("raw").and_then(Value::as_str))
                .map(|raw| raw.chars().filter(|c| c.is_ascii_digit()).collect())
                .collect()
        })
        .unwrap_or_default();

    state.with(|connection| {
        let mut found = Vec::new();

        for digits in phones.iter().filter(|d: &&String| d.len() >= 6) {
            let suffix = &digits[digits.len().saturating_sub(7)..];
            let mut statement = connection.prepare(
                "SELECT DISTINCT c.id, c.display_name FROM contact_phones p
                   JOIN contacts c ON c.id = p.contact_id
                  WHERE p.deleted_at IS NULL AND c.deleted_at IS NULL
                    AND p.digits LIKE '%' || ?1",
            )?;
            let rows = statement.query_map(yanuka_db::rusqlite::params![suffix], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            for row in rows {
                let (id, display_name) = row?;
                if Some(&id) == exclude_id.as_ref() {
                    continue;
                }
                found.push(serde_json::json!({
                    "contact": { "id": id, "displayName": display_name },
                    "confidence": 0.75,
                    "reasons": ["אותו מספר טלפון"],
                }));
            }
        }

        if !name.trim().is_empty() {
            let normalized = yanuka_search::normalize_name(name);
            let mut statement = connection.prepare(
                "SELECT id, display_name FROM contacts
                  WHERE deleted_at IS NULL AND normalized_name = ?1 LIMIT 5",
            )?;
            let rows = statement.query_map(yanuka_db::rusqlite::params![normalized], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            for row in rows {
                let (id, display_name) = row?;
                if Some(&id) == exclude_id.as_ref() {
                    continue;
                }
                found.push(serde_json::json!({
                    "contact": { "id": id, "displayName": display_name },
                    "confidence": 0.5,
                    "reasons": ["שם זהה"],
                }));
            }
        }

        Ok(found)
    })
}

#[tauri::command]
pub fn list_tags(state: State<'_, AppState>) -> Answer<Vec<Tag>> {
    state.with(|connection| taxonomy::list_tags(connection))
}

#[tauri::command]
pub fn create_tag(state: State<'_, AppState>, input: Value) -> Answer<Tag> {
    let name = input.get("name").and_then(Value::as_str).unwrap_or_default().to_string();
    let color = input.get("color").and_then(Value::as_str).map(str::to_string);
    state.with(|connection| taxonomy::create_tag(connection, &name, color.as_deref()))
}

#[tauri::command]
pub fn delete_tag(state: State<'_, AppState>, id: String) -> Answer<()> {
    state.with(|connection| taxonomy::delete_tag(connection, &id))
}

#[tauri::command]
pub fn list_categories(state: State<'_, AppState>) -> Answer<Vec<Category>> {
    state.with(|connection| taxonomy::list_categories(connection))
}

#[tauri::command]
pub fn create_category(state: State<'_, AppState>, input: Value) -> Answer<Category> {
    let name = input.get("name").and_then(Value::as_str).unwrap_or_default().to_string();
    let description = input.get("description").and_then(Value::as_str).map(str::to_string);
    state.with(|connection| taxonomy::create_category(connection, &name, description.as_deref()))
}

#[tauri::command]
pub fn delete_category(state: State<'_, AppState>, id: String) -> Answer<()> {
    state.with(|connection| taxonomy::delete_category(connection, &id))
}

#[tauri::command]
pub fn list_organizations(
    state: State<'_, AppState>,
    query: Option<String>,
    limit: Option<i64>,
) -> Answer<Vec<Organization>> {
    state.with(|connection| {
        taxonomy::list_organizations(connection, query.as_deref(), limit.unwrap_or(50))
    })
}

#[tauri::command]
pub fn create_organization(state: State<'_, AppState>, input: Value) -> Answer<Organization> {
    let name = input.get("name").and_then(Value::as_str).unwrap_or_default().to_string();
    let kind = input.get("kind").and_then(Value::as_str).unwrap_or("organization").to_string();
    let city = input.get("city").and_then(Value::as_str).map(str::to_string);
    let country = input.get("country").and_then(Value::as_str).map(str::to_string);

    state.with(|connection| {
        taxonomy::create_organization(connection, &name, &kind, city.as_deref(), country.as_deref())
    })
}

#[tauri::command]
pub fn delete_organization(state: State<'_, AppState>, id: String) -> Answer<()> {
    state.with(|connection| taxonomy::delete_organization(connection, &id))
}

#[tauri::command]
pub fn create_relationship(state: State<'_, AppState>, input: Value) -> Answer<Value> {
    let from = input.get("fromContactId").and_then(Value::as_str).unwrap_or_default().to_string();
    let to = input.get("toContactId").and_then(Value::as_str).unwrap_or_default().to_string();
    let kind = input.get("type").and_then(Value::as_str).unwrap_or("knows").to_string();
    let notes = input.get("notes").and_then(Value::as_str).map(str::to_string);

    state.with(|connection| {
        let id = taxonomy::create_relationship(connection, &from, &to, &kind, notes.as_deref())?;
        Ok(serde_json::json!({
            "id": id,
            "fromContactId": from,
            "toContactId": to,
            "type": kind,
            "notes": notes,
        }))
    })
}

#[tauri::command]
pub fn delete_relationship(state: State<'_, AppState>, id: String) -> Answer<()> {
    state.with(|connection| taxonomy::delete_relationship(connection, &id))
}

#[tauri::command]
pub fn add_note(state: State<'_, AppState>, input: Value) -> Answer<Value> {
    let contact_id = input.get("contactId").and_then(Value::as_str).unwrap_or_default().to_string();
    let body = input.get("body").and_then(Value::as_str).unwrap_or_default().to_string();
    let is_sensitive = input.get("isSensitive").and_then(Value::as_bool).unwrap_or(false);

    state.with(|connection| {
        let id = taxonomy::add_note(connection, &contact_id, &body, is_sensitive)?;
        Ok(serde_json::json!({
            "id": id,
            "contactId": contact_id,
            "body": body,
            "isSensitive": is_sensitive,
        }))
    })
}

#[tauri::command]
pub fn update_note(
    state: State<'_, AppState>,
    id: String,
    body: String,
    is_sensitive: Option<bool>,
) -> Answer<Value> {
    state.with(|connection| {
        connection.execute(
            "UPDATE notes SET body = ?2, is_sensitive = COALESCE(?3, is_sensitive),
                              updated_at = ?4, version = version + 1
              WHERE id = ?1",
            yanuka_db::rusqlite::params![
                id,
                body,
                is_sensitive.map(i64::from),
                yanuka_db::now_iso()
            ],
        )?;
        Ok(serde_json::json!({ "id": id, "body": body }))
    })
}

#[tauri::command]
pub fn delete_note(state: State<'_, AppState>, id: String) -> Answer<()> {
    state.with(|connection| taxonomy::delete_note(connection, &id))
}

#[tauri::command]
pub fn database_stats(state: State<'_, AppState>) -> Answer<Value> {
    state.with(|connection| taxonomy::stats(connection))
}

#[tauri::command]
pub fn audit_log(
    state: State<'_, AppState>,
    entity_id: Option<String>,
    limit: Option<i64>,
) -> Answer<Vec<Value>> {
    state.with(|connection| {
        let limit = limit.unwrap_or(50);
        let mut statement = connection.prepare(
            "SELECT id, user_id, user_display_name, action, entity_type, entity_id,
                    entity_label, device_id, device_name, created_at
               FROM audit_log
              WHERE (?1 IS NULL OR entity_id = ?1)
              ORDER BY created_at DESC LIMIT ?2",
        )?;
        let rows = statement.query_map(yanuka_db::rusqlite::params![entity_id, limit], |row| {
            Ok(serde_json::json!({
                "id": row.get::<_, String>(0)?,
                "userId": row.get::<_, Option<String>>(1)?,
                "userDisplayName": row.get::<_, Option<String>>(2)?,
                "action": row.get::<_, String>(3)?,
                "entityType": row.get::<_, String>(4)?,
                "entityId": row.get::<_, Option<String>>(5)?,
                "entityLabel": row.get::<_, Option<String>>(6)?,
                "changes": Value::Null,
                "deviceId": row.get::<_, Option<String>>(7)?,
                "deviceName": row.get::<_, Option<String>>(8)?,
                "createdAt": row.get::<_, String>(9)?,
            }))
        })?;
        Ok(rows.collect::<yanuka_db::rusqlite::Result<Vec<_>>>()?)
    })
}
