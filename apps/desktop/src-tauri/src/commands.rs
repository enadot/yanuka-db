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
pub fn deleted_contacts(
    state: State<'_, AppState>,
    limit: Option<i64>,
) -> Answer<Vec<DeletedContact>> {
    state.with(|connection| repository::deleted_contacts(connection, limit.unwrap_or(100)))
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
    patch: ContactPatch,
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

/// Whole-database duplicate scan for the dedup screen.
#[tauri::command]
pub fn list_duplicate_pairs(
    state: State<'_, AppState>,
    limit: Option<i64>,
) -> Answer<Vec<yanuka_db::merge::DuplicatePair>> {
    state
        .with(|connection| yanuka_db::merge::list_duplicate_pairs(connection, limit.unwrap_or(100)))
}

/// Merge one contact into another without losing data. See yanuka_db::merge.
#[tauri::command]
pub fn merge_contacts(
    state: State<'_, AppState>,
    keep_id: String,
    merge_id: String,
) -> Answer<ContactWithRelations> {
    state.with(|connection| yanuka_db::merge::merge_contacts(connection, &keep_id, &merge_id))
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

/// Snapshot the live database to a user-chosen destination (typically a USB
/// stick), via SQLite's online-backup API — consistent even mid-write.
#[tauri::command]
pub fn backup_database(state: State<'_, AppState>, target_path: String) -> Answer<String> {
    let target = std::path::PathBuf::from(&target_path);
    state.with(|connection| yanuka_db::backup::backup_to(connection, &target))?;
    Ok(target_path)
}

/// When the newest backup (daily or pre-migration) was taken, for settings.
#[tauri::command]
pub fn backup_status(state: State<'_, AppState>) -> Answer<Value> {
    Ok(serde_json::json!({
        "lastBackupAt": yanuka_db::backup::last_backup_at(state.database_path()),
        "backupsDirectory": state
            .database_path()
            .parent()
            .map(|parent| parent.join("backups").display().to_string()),
    }))
}

/// Write an exported CSV where the user chose to save it.
///
/// Deliberately narrow — `.csv` only — rather than a general file-write IPC:
/// the webview is not a trust boundary, and this command is the only file
/// write it can request.
#[tauri::command]
pub fn save_exported_csv(path: String, contents: String) -> Answer<String> {
    if !path.to_lowercase().ends_with(".csv") {
        return Err(DbError::Validation("נתיב הייצוא חייב להסתיים ב־.csv".into()));
    }
    std::fs::write(&path, contents)
        .map_err(|error| DbError::Validation(format!("שמירת הקובץ נכשלה: {error}")))?;
    Ok(path)
}

// ---------------------------------------------------------------------------
// Sync
// ---------------------------------------------------------------------------
//
// The only commands here that touch the network, and the only ones that are
// `async`. Errors come back as plain strings rather than `DbError` because most
// of what can go wrong is not a database problem — an unreachable server, a
// mistyped connection code, a revoked device — and each already carries a
// sentence written for the person reading it.

/// What the settings screen and the offline indicator need to know.
///
/// Answers "is my work safe and has it left this machine" without the word
/// "mutation" appearing anywhere.
#[tauri::command]
pub fn sync_status(state: State<'_, AppState>) -> Answer<Value> {
    state.with(|connection| {
        let settings = yanuka_sync_client::load(connection)?;
        let pending = yanuka_db::mutation::pending_count(connection)?;
        let conflicts = yanuka_db::conflicts::open_count(connection)?;

        Ok(serde_json::json!({
            "connected": settings.is_some(),
            "serverUrl": settings.as_ref().map(|s| s.server_url.clone()),
            "lastSyncAt": settings.as_ref().and_then(|s| s.last_sync_at.clone()),
            "pendingChanges": pending,
            "openConflicts": conflicts,
        }))
    })
}

/// Join this device to a server with a pasted connection code.
#[tauri::command]
pub async fn sync_connect(
    state: State<'_, AppState>,
    code: String,
    device_name: String,
) -> Result<Value, String> {
    yanuka_sync_client::connect(&*state, &code, &device_name, "desktop")
        .await
        .map_err(|error| error.to_string())?;
    // Straight into a first round, because a connection screen that says
    // "connected" and shows an empty archive has not finished the job the user
    // asked for.
    sync_now(state).await
}

/// Send what is local, fetch what is not.
#[tauri::command]
pub async fn sync_now(state: State<'_, AppState>) -> Result<Value, String> {
    let mut settings = state
        .with(|connection| yanuka_sync_client::load(connection))
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "המכשיר אינו מחובר לשרת".to_string())?;

    // The same gate the background loop takes: pressing the button while a
    // timed round is in flight should wait for it, not race it.
    let outcome = {
        let _gate = state.sync_gate().lock().await;
        yanuka_sync_client::sync_once(&*state, &mut settings).await
    }
    .map_err(|error| error.to_string())?;

    serde_json::to_value(outcome).map_err(|error| error.to_string())
}

/// A code for adding another device to this same archive.
///
/// The enrolment secret is supplied by the caller rather than stored: this
/// device holds a token, not the secret, and a machine that could mint
/// enrolment codes from its own credentials would turn one compromised laptop
/// into permission to add more.
#[tauri::command]
pub fn sync_share_code(
    state: State<'_, AppState>,
    enrolment_secret: String,
) -> Result<String, String> {
    let settings = state
        .with(|connection| yanuka_sync_client::load(connection))
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "המכשיר אינו מחובר לשרת".to_string())?;
    Ok(yanuka_sync_client::share_code(&settings, enrolment_secret.trim()))
}

/// Forget the server. The contacts and the pending log both stay, so a device
/// that is disconnected and later reconnected sends everything it did between.
#[tauri::command]
pub fn sync_disconnect(state: State<'_, AppState>) -> Answer<()> {
    state.with(|connection| yanuka_sync_client::clear(connection))
}

// ---------------------------------------------------------------------------
// Conflicts
// ---------------------------------------------------------------------------

/// Every disagreement still waiting on a person.
#[tauri::command]
pub fn conflicts_open(
    state: State<'_, AppState>,
) -> Answer<Vec<yanuka_db::conflicts::OpenConflict>> {
    state.with(|connection| yanuka_db::conflicts::open(connection))
}

/// Record which answer the person kept.
///
/// Fields not named in `choices` stay open, so a contact with one obvious
/// disagreement and one that needs a phone call does not have to wait for the
/// phone call.
#[tauri::command]
pub fn conflicts_resolve(
    state: State<'_, AppState>,
    conflict_id: String,
    choices: Vec<yanuka_db::conflicts::FieldChoice>,
) -> Answer<()> {
    state.with(|connection| yanuka_db::conflicts::resolve(connection, &conflict_id, &choices))
}
