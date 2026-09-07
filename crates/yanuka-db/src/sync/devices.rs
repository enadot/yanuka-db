//! Devices and how they prove who they are.
//!
//! A device pairs once, by typing a short code a person obtained from an
//! already-paired device (or from the server's console on first run). The
//! server answers with a bearer token the device keeps. Neither the code nor
//! the token is stored in clear: only SHA-256 digests, so a copied database
//! or backup grants nothing.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{DbError, Result};
use crate::now_iso;

/// How long a pairing code stays valid. Long enough to walk to the other
/// machine, short enough that a code left on a whiteboard is harmless.
pub const PAIR_CODE_TTL_SECONDS: i64 = 15 * 60;

/// What a device says about itself when pairing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    /// `desktop`, `android`, `ios` or `web` — the schema's CHECK.
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub platform: Option<String>,
    #[serde(default)]
    pub app_version: Option<String>,
}

/// A paired device as the settings screen lists it. Never carries a token.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceRecord {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub platform: Option<String>,
    pub app_version: Option<String>,
    pub last_seen_at: Option<String>,
    pub last_sync_at: Option<String>,
    pub revoked_at: Option<String>,
    pub created_at: String,
}

pub fn sha256_hex(input: &str) -> String {
    let digest = Sha256::digest(input.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn random_bytes(n: usize) -> Result<Vec<u8>> {
    let mut buffer = vec![0u8; n];
    getrandom::fill(&mut buffer).map_err(|error| DbError::Sync(format!("no entropy: {error}")))?;
    Ok(buffer)
}

/// 256 bits of entropy as lowercase hex — the bearer token.
pub fn new_token() -> Result<String> {
    Ok(random_bytes(32)?.iter().map(|byte| format!("{byte:02x}")).collect())
}

/// A code a person can read aloud: eight characters from an alphabet without
/// look-alikes, in two groups (`K7PT-4MXQ`). Case- and dash-insensitive on
/// redemption.
pub fn new_pair_code() -> Result<String> {
    const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let bytes = random_bytes(8)?;
    let chars: String =
        bytes.iter().map(|byte| ALPHABET[(*byte as usize) % ALPHABET.len()] as char).collect();
    Ok(format!("{}-{}", &chars[..4], &chars[4..]))
}

fn canonical_code(code: &str) -> String {
    code.chars().filter(|c| c.is_ascii_alphanumeric()).map(|c| c.to_ascii_uppercase()).collect()
}

/// Mint a pairing code (server side). Returns the clear code, once.
pub fn create_pair_code(connection: &Connection) -> Result<(String, String)> {
    let code = new_pair_code()?;
    let now = time::OffsetDateTime::now_utc();
    let expires = now + time::Duration::seconds(PAIR_CODE_TTL_SECONDS);
    let expires_at = expires
        .format(&time::format_description::well_known::Rfc3339)
        .map_err(|error| DbError::Sync(error.to_string()))?;
    connection.execute(
        "INSERT INTO pair_codes (code_hash, created_at, expires_at) VALUES (?1, ?2, ?3)",
        params![sha256_hex(&canonical_code(&code)), now_iso(), expires_at],
    )?;
    Ok((code, expires_at))
}

/// Redeem a pairing code for a device token (server side). The code is
/// consumed whether or not the device was already known: re-pairing a
/// device replaces its token, which is also how a lost token is rotated.
pub fn redeem_pair_code(
    connection: &mut Connection,
    code: &str,
    device: &DeviceInfo,
) -> Result<String> {
    let hash = sha256_hex(&canonical_code(code));
    let now = now_iso();
    let valid: Option<String> = connection
        .query_row(
            "SELECT code_hash FROM pair_codes
              WHERE code_hash = ?1 AND used_at IS NULL AND expires_at > ?2",
            params![hash, now],
            |row| row.get(0),
        )
        .optional()?;
    if valid.is_none() {
        return Err(DbError::Unauthorized);
    }
    if !matches!(device.kind.as_str(), "desktop" | "android" | "ios" | "web") {
        return Err(DbError::Validation("סוג מכשיר אינו מוכר".into()));
    }
    if device.name.trim().is_empty() || device.id.trim().is_empty() {
        return Err(DbError::Validation("למכשיר חסר שם או מזהה".into()));
    }

    let token = new_token()?;
    let tx = connection.transaction()?;
    tx.execute(
        "UPDATE pair_codes SET used_at = ?2, used_by = ?3 WHERE code_hash = ?1",
        params![hash, now, device.id],
    )?;
    tx.execute(
        "INSERT INTO devices (id, name, type, platform, app_version, last_seen_at, created_at, token_hash)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, ?7)
         ON CONFLICT(id) DO UPDATE SET
           name = excluded.name, type = excluded.type, platform = excluded.platform,
           app_version = excluded.app_version, last_seen_at = excluded.last_seen_at,
           token_hash = excluded.token_hash, revoked_at = NULL",
        params![
            device.id,
            device.name.trim(),
            device.kind,
            device.platform,
            device.app_version,
            now,
            sha256_hex(&token)
        ],
    )?;
    // Codes that expired are of no further use; keep the table small.
    tx.execute("DELETE FROM pair_codes WHERE expires_at <= ?1 AND used_at IS NULL", params![now])?;
    tx.commit()?;
    Ok(token)
}

/// Resolve a bearer token to a device id (server side), touching
/// `last_seen_at`. A revoked device is indistinguishable from an unknown
/// token on purpose.
pub fn authenticate(connection: &Connection, token: &str) -> Result<String> {
    let hash = sha256_hex(token);
    let id: Option<String> = connection
        .query_row(
            "SELECT id FROM devices WHERE token_hash = ?1 AND revoked_at IS NULL",
            params![hash],
            |row| row.get(0),
        )
        .optional()?;
    let Some(id) = id else {
        return Err(DbError::Unauthorized);
    };
    connection
        .execute("UPDATE devices SET last_seen_at = ?2 WHERE id = ?1", params![id, now_iso()])?;
    Ok(id)
}

pub fn mark_synced(connection: &Connection, device_id: &str) -> Result<()> {
    connection.execute(
        "UPDATE devices SET last_sync_at = ?2 WHERE id = ?1",
        params![device_id, now_iso()],
    )?;
    Ok(())
}

/// Cut a device off. Its records stay; its token stops working.
pub fn revoke(connection: &Connection, device_id: &str) -> Result<()> {
    let changed = connection.execute(
        "UPDATE devices SET revoked_at = ?2, token_hash = NULL WHERE id = ?1 AND revoked_at IS NULL",
        params![device_id, now_iso()],
    )?;
    if changed == 0 {
        return Err(DbError::NotFound("המכשיר".into()));
    }
    Ok(())
}

pub fn list(connection: &Connection) -> Result<Vec<DeviceRecord>> {
    let mut statement = connection.prepare(
        "SELECT id, name, type, platform, app_version, last_seen_at, last_sync_at, revoked_at,
                created_at
           FROM devices ORDER BY created_at",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(DeviceRecord {
            id: row.get(0)?,
            name: row.get(1)?,
            kind: row.get(2)?,
            platform: row.get(3)?,
            app_version: row.get(4)?,
            last_seen_at: row.get(5)?,
            last_sync_at: row.get(6)?,
            revoked_at: row.get(7)?,
            created_at: row.get(8)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// How many devices can currently sync (server side).
pub fn active_count(connection: &Connection) -> Result<i64> {
    Ok(connection.query_row(
        "SELECT COUNT(*) FROM devices WHERE revoked_at IS NULL AND token_hash IS NOT NULL",
        [],
        |row| row.get(0),
    )?)
}
