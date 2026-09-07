//! Where this device's sync settings live: `app_meta`, next to the device id.
//! The token is stored here rather than in the OS credential store because
//! the database itself is encrypted at rest (ADR-033) and a backup restored
//! elsewhere should carry its pairing with it.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::error::Result;
use crate::now_iso;

pub const KEY_SERVER_URL: &str = "sync_server_url";
pub const KEY_TOKEN: &str = "sync_token";
pub const KEY_SERVER_NAME: &str = "sync_server_name";
pub const KEY_DEVICE_NAME: &str = "sync_device_name";
pub const KEY_LAST_AT: &str = "sync_last_at";
pub const KEY_LAST_ERROR: &str = "sync_last_error";
pub const KEY_DEFERRED: &str = "sync_deferred";

pub fn get_meta(connection: &Connection, key: &str) -> Result<Option<String>> {
    Ok(connection
        .query_row("SELECT value FROM app_meta WHERE key = ?1", params![key], |row| row.get(0))
        .optional()?
        .flatten())
}

pub fn set_meta(connection: &Connection, key: &str, value: Option<&str>) -> Result<()> {
    match value {
        Some(value) => {
            connection.execute(
                "INSERT INTO app_meta (key, value, updated_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
                params![key, value, now_iso()],
            )?;
        }
        None => {
            connection.execute("DELETE FROM app_meta WHERE key = ?1", params![key])?;
        }
    }
    Ok(())
}

/// The pairing this device holds, if any.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncConfig {
    pub server_url: String,
    pub token: String,
    pub server_name: Option<String>,
    pub device_name: Option<String>,
}

pub fn load_config(connection: &Connection) -> Result<Option<SyncConfig>> {
    let (Some(server_url), Some(token)) =
        (get_meta(connection, KEY_SERVER_URL)?, get_meta(connection, KEY_TOKEN)?)
    else {
        return Ok(None);
    };
    Ok(Some(SyncConfig {
        server_url,
        token,
        server_name: get_meta(connection, KEY_SERVER_NAME)?,
        device_name: get_meta(connection, KEY_DEVICE_NAME)?,
    }))
}

pub fn save_config(connection: &Connection, config: &SyncConfig) -> Result<()> {
    set_meta(connection, KEY_SERVER_URL, Some(&config.server_url))?;
    set_meta(connection, KEY_TOKEN, Some(&config.token))?;
    set_meta(connection, KEY_SERVER_NAME, config.server_name.as_deref())?;
    set_meta(connection, KEY_DEVICE_NAME, config.device_name.as_deref())?;
    set_meta(connection, KEY_LAST_ERROR, None)?;
    Ok(())
}

/// Forget the server. The records stay; so does the journal. What is
/// dropped is the bookkeeping about what the server holds — a later pairing
/// (to the same server or another) starts from a clean comparison, which is
/// exactly what the merge rules are for.
pub fn clear_config(connection: &mut Connection) -> Result<()> {
    let tx = connection.transaction()?;
    for key in [
        KEY_SERVER_URL,
        KEY_TOKEN,
        KEY_SERVER_NAME,
        KEY_DEVICE_NAME,
        KEY_LAST_AT,
        KEY_LAST_ERROR,
        KEY_DEFERRED,
    ] {
        set_meta(&tx, key, None)?;
    }
    tx.execute("DELETE FROM sync_revisions", [])?;
    tx.execute(
        "UPDATE sync_cursors SET cursor = NULL, last_pulled_at = NULL, last_pushed_at = NULL",
        [],
    )?;
    // Held-back changes are released: with no server there is nothing to
    // disagree with, and the local values are the ones the person sees.
    tx.execute(
        "UPDATE conflicts SET resolved_at = ?1, resolution = 'local' WHERE resolved_at IS NULL",
        params![now_iso()],
    )?;
    tx.execute("UPDATE mutations SET status = 'pending' WHERE status = 'conflict'", [])?;
    tx.commit()?;
    Ok(())
}

/// The pull cursor: the last server `seq` this device integrated.
pub fn cursor(connection: &Connection) -> Result<i64> {
    let raw: Option<String> = connection
        .query_row("SELECT cursor FROM sync_cursors WHERE entity_type = 'all'", [], |row| {
            row.get(0)
        })
        .optional()?
        .flatten();
    Ok(raw.and_then(|value| value.parse().ok()).unwrap_or(0))
}

pub fn set_cursor(connection: &Connection, seq: i64) -> Result<()> {
    connection.execute(
        "UPDATE sync_cursors SET cursor = ?1, last_pulled_at = ?2 WHERE entity_type = 'all'",
        params![seq.to_string(), now_iso()],
    )?;
    Ok(())
}

/// The persisted half of what the indicator shows. The shell adds the live
/// half (`online`, `syncing`) from its worker.
pub fn status(connection: &Connection) -> Result<Value> {
    let count = |sql: &str| -> Result<i64> { Ok(connection.query_row(sql, [], |row| row.get(0))?) };
    let config = load_config(connection)?;
    Ok(json!({
        "configured": config.is_some(),
        "serverUrl": config.as_ref().map(|c| c.server_url.clone()),
        "serverName": config.as_ref().and_then(|c| c.server_name.clone()),
        "deviceName": config.as_ref().and_then(|c| c.device_name.clone()),
        "deviceId": crate::repository::device_id(connection)?,
        "lastSyncAt": get_meta(connection, KEY_LAST_AT)?,
        "lastError": get_meta(connection, KEY_LAST_ERROR)?,
        "pendingMutations": crate::mutation::pending_count(connection)?,
        "failedMutations": count("SELECT COUNT(*) FROM mutations WHERE status = 'failed'")?,
        "openConflicts": count("SELECT COUNT(*) FROM conflicts WHERE resolved_at IS NULL")?,
        "online": false,
        "syncing": false,
    }))
}
