//! Where a device keeps its half of the sync arrangement.
//!
//! In `app_meta`, alongside the device id — the same unencrypted SQLite file as
//! the contacts. That includes the data key, which deserves stating plainly
//! rather than hiding: storing the key next to the data it protects adds no
//! exposure, because anyone who can read the key file can already read the
//! contacts in the row below it. The sealing protects the archive *in transit
//! and on the server*, which is where it leaves the user's control. ADR-018
//! covers the local file, and remains deferred.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use yanuka_db::error::Result;
use yanuka_db::rusqlite::{params, Connection, OptionalExtension};

use crate::SyncError;

const SERVER_URL: &str = "sync_server_url";
const DEVICE_ID: &str = "sync_device_id";
const TOKEN: &str = "sync_token";
const KEY: &str = "sync_key";
const CURSOR: &str = "sync_cursor";
const LAST_SYNC: &str = "sync_last_at";

#[derive(Debug, Clone)]
pub struct SyncSettings {
    pub server_url: String,
    /// The identity the *server* knows this device by. Distinct from the
    /// `device_id` stamped on rows, which is minted locally and outlives any
    /// particular enrolment — a machine that re-enrols after losing its
    /// database gets a new server identity and its history is still its own.
    pub device_id: String,
    pub token: String,
    pub key: [u8; 32],
    pub cursor: i64,
    pub last_sync_at: Option<String>,
}

fn read(connection: &Connection, key: &str) -> Result<Option<String>> {
    Ok(connection
        .query_row("SELECT value FROM app_meta WHERE key = ?1", params![key], |row| row.get(0))
        .optional()?)
}

fn write(connection: &Connection, key: &str, value: &str) -> Result<()> {
    connection.execute(
        "INSERT INTO app_meta (key, value, updated_at) VALUES (?1, ?2, ?3)
         ON CONFLICT (key) DO UPDATE SET value = ?2, updated_at = ?3",
        params![key, value, yanuka_db::now_iso()],
    )?;
    Ok(())
}

/// The stored arrangement, or `None` when this device has never been connected.
pub fn load(connection: &Connection) -> Result<Option<SyncSettings>> {
    let (Some(server_url), Some(device_id), Some(token), Some(encoded)) = (
        read(connection, SERVER_URL)?,
        read(connection, DEVICE_ID)?,
        read(connection, TOKEN)?,
        read(connection, KEY)?,
    ) else {
        return Ok(None);
    };

    let Ok(bytes) = URL_SAFE_NO_PAD.decode(&encoded) else {
        return Ok(None);
    };
    let Ok(key): std::result::Result<[u8; 32], _> = bytes.try_into() else {
        return Ok(None);
    };

    Ok(Some(SyncSettings {
        server_url,
        device_id,
        token,
        key,
        // A missing or unparseable cursor means start from the beginning. That
        // costs one redundant pass over the log, which the mutation ids make
        // harmless — the alternative, guessing forward, skips changes.
        cursor: read(connection, CURSOR)?.and_then(|value| value.parse().ok()).unwrap_or(0),
        last_sync_at: read(connection, LAST_SYNC)?,
    }))
}

pub fn save(connection: &Connection, settings: &SyncSettings) -> Result<()> {
    write(connection, SERVER_URL, &settings.server_url)?;
    write(connection, DEVICE_ID, &settings.device_id)?;
    write(connection, TOKEN, &settings.token)?;
    write(connection, KEY, &URL_SAFE_NO_PAD.encode(settings.key))?;
    write(connection, CURSOR, &settings.cursor.to_string())?;
    if let Some(last) = &settings.last_sync_at {
        write(connection, LAST_SYNC, last)?;
    }
    Ok(())
}

/// Forget the server.
///
/// The contacts stay. So does the mutation log, which means a device that is
/// disconnected and later reconnected sends everything it did in between — the
/// work done while detached is not a gap.
pub fn clear(connection: &Connection) -> Result<()> {
    for key in [SERVER_URL, DEVICE_ID, TOKEN, KEY, CURSOR, LAST_SYNC] {
        connection.execute("DELETE FROM app_meta WHERE key = ?1", params![key])?;
    }
    Ok(())
}

/// The settings, or the error the UI should show if there are none.
pub fn require(connection: &Connection) -> crate::Result<SyncSettings> {
    load(connection)?.ok_or(SyncError::NotConfigured)
}
