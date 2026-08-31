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
use yanuka_db::{mutation, repository, search, taxonomy, DbError};

use crate::state::AppState;

type Answer<T> = Result<T, DbError>;

#[tauri::command]
pub fn search_contacts(state: State<'_, AppState>, input: SearchQuery) -> Answer<SearchResponse> {
    let engine = state.semantic_engine();
    state.with(|connection| search::search_with_semantic(connection, engine.as_deref(), &input))
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
    let created = state.with(|connection| repository::create_contact(connection, &input, id))?;
    state.semantic_touch(&created.contact.id);
    Ok(created)
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

    let created = state.with(|connection| repository::create_contact(connection, &contact, id))?;
    state.semantic_touch(&created.contact.id);
    Ok(created)
}

#[tauri::command]
pub fn update_contact(
    state: State<'_, AppState>,
    id: String,
    patch: ContactInput,
    base_version: Option<i64>,
) -> Answer<ContactWithRelations> {
    let updated = state
        .with(|connection| repository::update_contact(connection, &id, &patch, base_version))?;
    state.semantic_touch(&id);
    Ok(updated)
}

#[tauri::command]
pub fn delete_contact(state: State<'_, AppState>, id: String) -> Answer<()> {
    state.with(|connection| repository::delete_contact(connection, &id))?;
    state.semantic_touch(&id);
    Ok(())
}

#[tauri::command]
pub fn restore_contact(state: State<'_, AppState>, id: String) -> Answer<ContactWithRelations> {
    let restored = state.with(|connection| repository::restore_contact(connection, &id))?;
    state.semantic_touch(&id);
    Ok(restored)
}

#[tauri::command]
pub fn list_deleted_contacts(
    state: State<'_, AppState>,
    limit: Option<i64>,
) -> Answer<Vec<DeletedContactSummary>> {
    state.with(|connection| repository::list_deleted_contacts(connection, limit.unwrap_or(50)))
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
    let merged = state
        .with(|connection| yanuka_db::merge::merge_contacts(connection, &keep_id, &merge_id))?;
    state.semantic_touch(&keep_id);
    state.semantic_touch(&merge_id);
    Ok(merged)
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

    let answer = state.with(|connection| {
        let id = taxonomy::add_note(connection, &contact_id, &body, is_sensitive)?;
        Ok(serde_json::json!({
            "id": id,
            "contactId": contact_id,
            "body": body,
            "isSensitive": is_sensitive,
        }))
    })?;
    state.semantic_touch(&contact_id);
    Ok(answer)
}

#[tauri::command]
pub fn update_note(
    state: State<'_, AppState>,
    id: String,
    body: String,
    is_sensitive: Option<bool>,
) -> Answer<Value> {
    let contact_id = state.with(|connection| {
        let contact_id = note_contact(connection, &id)?;
        taxonomy::update_note(connection, &id, &body, is_sensitive)?;
        Ok(contact_id)
    })?;
    if let Some(contact_id) = contact_id {
        state.semantic_touch(&contact_id);
    }
    Ok(serde_json::json!({ "id": id, "body": body }))
}

#[tauri::command]
pub fn delete_note(state: State<'_, AppState>, id: String) -> Answer<()> {
    let contact_id = state.with(|connection| {
        let contact_id = note_contact(connection, &id)?;
        taxonomy::delete_note(connection, &id)?;
        Ok(contact_id)
    })?;
    if let Some(contact_id) = contact_id {
        state.semantic_touch(&contact_id);
    }
    Ok(())
}

/// The contact a note belongs to, for the post-mutation semantic sync.
fn note_contact(
    connection: &yanuka_db::rusqlite::Connection,
    note_id: &str,
) -> Result<Option<String>, DbError> {
    Ok(connection
        .query_row("SELECT contact_id FROM notes WHERE id = ?1", [note_id], |row| {
            row.get::<_, String>(0)
        })
        .map(Some)
        .unwrap_or(None))
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
    // History is derived from the mutation journal — the log every write
    // already appends to, in the same transaction, with the changed fields
    // and their previous values. The audit_log table stays reserved for the
    // multi-user era (ADR-020); nothing writes to it yet, and reading it
    // here would show an always-empty history.
    state
        .with(|connection| mutation::history(connection, entity_id.as_deref(), limit.unwrap_or(50)))
}

/// Snapshot the live database to a user-chosen destination (typically a USB
/// stick), via SQLite's online-backup API — consistent even mid-write.
#[tauri::command]
pub fn backup_database(state: State<'_, AppState>, target_path: String) -> Answer<String> {
    let target = std::path::PathBuf::from(&target_path);
    // The backup is keyed with the live database's key, so a copy on a USB
    // stick is as protected as the original (threat 3 in docs/SECURITY.md).
    let key = state.key_pragma();
    state.with(|connection| yanuka_db::backup::backup_to(connection, &target, key.as_deref()))?;
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

/// Encryption state for the settings screen: `encrypted`, `plaintext` (no
/// credential store, or a failed upgrade), or `locked` (an encrypted file
/// whose key the store does not hold — a restored backup on a new machine).
#[tauri::command]
pub fn security_status(state: State<'_, AppState>) -> Answer<Value> {
    Ok(state.security_status())
}

/// The recovery key in display form. Shown in settings behind a click, so it
/// can be written down and kept off the machine — the only thing that opens
/// the database and its backups if Windows is ever reinstalled.
/// Semantic search state for the settings screen: `unavailable` (no model),
/// `indexing` (catch-up in progress, with counters), or `ready`.
#[tauri::command]
pub fn semantic_status(state: State<'_, AppState>) -> Answer<Value> {
    Ok(state.semantic_status())
}

#[tauri::command]
pub fn recovery_key(state: State<'_, AppState>) -> Answer<Value> {
    Ok(serde_json::json!({ "key": state.recovery_key() }))
}

/// Open a locked database with a recovery key the user typed, and remember
/// the key in the OS credential store for the next launch.
#[tauri::command]
pub fn unlock_database(state: State<'_, AppState>, key: String) -> Answer<Value> {
    state.unlock(&key)?;
    Ok(state.security_status())
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
// Notebook import (ADR-037).
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn ocr_import_page(
    state: State<'_, AppState>,
    file_name: String,
    data_base64: String,
) -> Answer<Value> {
    use base64::Engine as _;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data_base64.as_bytes())
        .map_err(|_| DbError::Validation("קובץ התמונה לא הגיע תקין".into()))?;
    let id =
        state.with(|connection| yanuka_db::ocr::import_page(connection, &bytes, &file_name))?;
    Ok(serde_json::json!({ "id": id }))
}

#[tauri::command]
pub fn ocr_list_pages(state: State<'_, AppState>) -> Answer<Vec<yanuka_db::ocr::PageSummary>> {
    state.with(|connection| yanuka_db::ocr::list_pages(connection))
}

#[tauri::command]
pub fn ocr_get_page(state: State<'_, AppState>, id: String) -> Answer<yanuka_db::ocr::PageDetail> {
    state.with(|connection| yanuka_db::ocr::get_page(connection, &id))
}

/// Record a correction and return the page's tokens — the learning may have
/// just filled other boxes, and the workbench wants to show that immediately.
#[tauri::command]
pub fn ocr_set_token_text(
    state: State<'_, AppState>,
    token_id: String,
    text: String,
) -> Answer<Vec<yanuka_db::ocr::Token>> {
    state.with(|connection| {
        yanuka_db::ocr::record_correction(connection, &token_id, &text)?;
        let page_id: String = connection.query_row(
            "SELECT page_id FROM ocr_tokens WHERE id = ?1",
            [&token_id],
            |row| row.get(0),
        )?;
        Ok(yanuka_db::ocr::get_page(connection, &page_id)?.tokens)
    })
}

#[tauri::command]
pub fn ocr_lexicon(state: State<'_, AppState>, prefix: String) -> Answer<Vec<String>> {
    state.with(|connection| yanuka_db::ocr::lexicon(connection, &prefix, 8))
}

#[tauri::command]
pub fn ocr_save_note(
    state: State<'_, AppState>,
    page_id: String,
    contact_id: String,
) -> Answer<Value> {
    let note_id =
        state.with(|connection| yanuka_db::ocr::save_as_note(connection, &page_id, &contact_id))?;
    state.semantic_touch(&contact_id);
    Ok(serde_json::json!({ "noteId": note_id }))
}

#[tauri::command]
pub fn ocr_delete_page(state: State<'_, AppState>, id: String) -> Answer<()> {
    state.with(|connection| yanuka_db::ocr::delete_page(connection, &id))
}
