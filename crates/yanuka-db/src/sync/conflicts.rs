//! Fields two devices changed differently. Both values are kept, the record
//! is held back from pushing, and a person picks — never the engine and
//! never the clock (SYNC.md §Conflicts).

use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde_json::{json, Value};

use super::state;
use super::Change;
use crate::error::{DbError, Result};
use crate::mutation::{self, Operation};
use crate::repository::device_id;
use crate::{new_id, now_iso};

/// Write a conflict row and hold the record's unsent journal rows.
pub fn record(
    tx: &Transaction<'_>,
    change: &Change,
    local: &Value,
    collisions: &[(String, Value, Value)],
    me: &str,
) -> Result<()> {
    let fields: Vec<Value> = collisions
        .iter()
        .map(|(field, local_value, remote_value)| {
            json!({
                "field": field,
                "localValue": local_value,
                "remoteValue": remote_value,
                "localUpdatedAt": local.get("updatedAt").cloned().unwrap_or(Value::Null),
                "remoteUpdatedAt": change.created_at,
                "localDeviceId": me,
                "remoteDeviceId": change.device_id,
                "remoteDeviceName": change.device_name,
            })
        })
        .collect();
    tx.execute(
        "INSERT INTO conflicts (id, entity_type, entity_id, fields, detected_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            new_id(),
            change.entity_type,
            change.entity_id,
            serde_json::to_string(&fields)?,
            now_iso()
        ],
    )?;
    tx.execute(
        "UPDATE mutations SET status = 'conflict'
          WHERE status IN ('pending', 'failed')
            AND ((entity_type = ?1 AND entity_id = ?2)
                 OR (entity_type = 'contact_category'
                     AND json_extract(payload, '$.contactId') = ?2 AND ?1 = 'contact'))",
        params![change.entity_type, change.entity_id],
    )?;
    Ok(())
}

fn label(connection: &Connection, entity_type: &str, entity_id: &str) -> Result<Option<String>> {
    let sql = match entity_type {
        "contact" => "SELECT display_name FROM contacts WHERE id = ?1",
        "note" => "SELECT body FROM notes WHERE id = ?1",
        "tag" => "SELECT name FROM tags WHERE id = ?1",
        "category" => "SELECT name FROM categories WHERE id = ?1",
        "organization" => "SELECT name FROM organizations WHERE id = ?1",
        "relationship" => {
            "SELECT c.display_name FROM relationships r JOIN contacts c ON c.id = r.from_contact_id
              WHERE r.id = ?1"
        }
        _ => return Ok(None),
    };
    Ok(connection.query_row(sql, params![entity_id], |row| row.get(0)).optional()?)
}

/// Open conflicts, newest first, with the record's name for the screen.
pub fn list(connection: &Connection) -> Result<Vec<Value>> {
    let mut statement = connection.prepare(
        "SELECT id, entity_type, entity_id, fields, detected_at FROM conflicts
          WHERE resolved_at IS NULL ORDER BY detected_at DESC",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
        ))
    })?;
    let mut list = Vec::new();
    for row in rows {
        let (id, entity_type, entity_id, fields, detected_at) = row?;
        let fields: Value = serde_json::from_str(&fields).unwrap_or(Value::Array(vec![]));
        let remote_device_name = fields
            .as_array()
            .and_then(|list| list.first())
            .and_then(|first| first.get("remoteDeviceName"))
            .cloned()
            .unwrap_or(Value::Null);
        list.push(json!({
            "id": id,
            "entityType": entity_type,
            "entityId": entity_id,
            "entityLabel": label(connection, &entity_type, &entity_id)?,
            "fields": fields,
            "detectedAt": detected_at,
            "resolvedAt": Value::Null,
            "resolution": Value::Null,
            "remoteDeviceName": remote_device_name,
        }));
    }
    Ok(list)
}

pub fn open_count(connection: &Connection) -> Result<i64> {
    Ok(connection.query_row(
        "SELECT COUNT(*) FROM conflicts WHERE resolved_at IS NULL",
        [],
        |row| row.get(0),
    )?)
}

/// Settle a conflict.
///
/// * `local` — this device's values stand; they are journaled again so the
///   next push carries them over the server's.
/// * `remote` — the other device's values are written here, journaled under
///   that device, and this device's word on those fields is withdrawn.
/// * `manual` — the person has already edited the record through the form;
///   that edit is what gets pushed.
///
/// In every case the record's held journal rows are released.
pub fn resolve(connection: &mut Connection, conflict_id: &str, resolution: &str) -> Result<()> {
    if !matches!(resolution, "local" | "remote" | "manual") {
        return Err(DbError::Validation("דרך יישוב אינה מוכרת".into()));
    }
    let row: Option<(String, String, String)> = connection
        .query_row(
            "SELECT entity_type, entity_id, fields FROM conflicts
              WHERE id = ?1 AND resolved_at IS NULL",
            params![conflict_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let Some((entity_type, entity_id, fields)) = row else {
        return Err(DbError::NotFound("ההתנגשות".into()));
    };
    let fields: Vec<Value> = serde_json::from_str(&fields)?;
    let me = device_id(connection)?;
    let current = state::load(connection, &entity_type, &entity_id)?;
    let now = now_iso();

    let tx = connection.transaction()?;
    tx.execute_batch("PRAGMA defer_foreign_keys = ON")?;
    match (resolution, current) {
        ("remote", Some(mut merged)) => {
            let names: Vec<String> = fields
                .iter()
                .filter_map(|f| f.get("field").and_then(Value::as_str).map(str::to_string))
                .collect();
            let previous = state::project(&merged, &names);
            for field in &fields {
                if let (Some(name), Some(value)) =
                    (field.get("field").and_then(Value::as_str), field.get("remoteValue"))
                {
                    merged[name] = value.clone();
                }
            }
            let remote_device = fields
                .first()
                .and_then(|f| f.get("remoteDeviceId"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            state::apply(&tx, &entity_type, &entity_id, &merged, &remote_device)?;
            let payload = state::project(&merged, &names);
            let id = mutation::record(
                &tx,
                mutation::NewMutation {
                    entity_type: &entity_type,
                    entity_id: &entity_id,
                    operation: Operation::Update,
                    payload: Some(&payload),
                    previous: Some(&previous),
                    base_version: 0,
                    device_id: &remote_device,
                },
            )?;
            tx.execute(
                "UPDATE mutations SET status = 'synced', synced_at = ?2 WHERE id = ?1",
                params![id, now],
            )?;
        }
        ("local", Some(current)) => {
            // Journal the decision so the record is dirty again even when
            // nothing else is pending, and so history says who chose.
            let names: Vec<String> = fields
                .iter()
                .filter_map(|f| f.get("field").and_then(Value::as_str).map(str::to_string))
                .collect();
            let payload = state::project(&current, &names);
            let mut previous = serde_json::Map::new();
            for field in &fields {
                if let Some(name) = field.get("field").and_then(Value::as_str) {
                    previous.insert(
                        name.to_string(),
                        field.get("remoteValue").cloned().unwrap_or(Value::Null),
                    );
                }
            }
            mutation::record(
                &tx,
                mutation::NewMutation {
                    entity_type: &entity_type,
                    entity_id: &entity_id,
                    operation: Operation::Update,
                    payload: Some(&payload),
                    previous: Some(&Value::Object(previous)),
                    base_version: 0,
                    device_id: &me,
                },
            )?;
        }
        _ => {}
    }
    tx.execute(
        "UPDATE conflicts SET resolved_at = ?2, resolution = ?3 WHERE id = ?1",
        params![conflict_id, now, resolution],
    )?;
    // Released only when no other conflict still holds the record.
    let still_open: i64 = tx.query_row(
        "SELECT COUNT(*) FROM conflicts
          WHERE entity_type = ?1 AND entity_id = ?2 AND resolved_at IS NULL",
        params![entity_type, entity_id],
        |row| row.get(0),
    )?;
    if still_open == 0 {
        tx.execute(
            "UPDATE mutations SET status = 'pending'
              WHERE status = 'conflict'
                AND ((entity_type = ?1 AND entity_id = ?2)
                     OR (entity_type = 'contact_category'
                         AND json_extract(payload, '$.contactId') = ?2 AND ?1 = 'contact'))",
            params![entity_type, entity_id],
        )?;
    }
    tx.commit()?;
    Ok(())
}
