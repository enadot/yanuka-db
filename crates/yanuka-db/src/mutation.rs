//! The local mutation log.
//!
//! Every write appends a row here, inside the same transaction as the write
//! itself. Two consequences follow, and both are the point:
//!
//! * A change made offline is never lost — it is durable before anything tries
//!   to send it anywhere.
//! * The payload records only the fields that actually changed, which is what
//!   makes field-level merging possible: two devices editing different parts of
//!   the same contact reconcile without a human, and only a genuine collision
//!   on the same field becomes a conflict.
//!
//! See docs/SYNC.md.

use rusqlite::{params, Transaction};
use serde_json::Value;

use crate::error::Result;
use crate::{new_id, now_iso};

#[derive(Debug, Clone, Copy)]
pub enum Operation {
    Create,
    Update,
    Delete,
}

impl Operation {
    fn as_str(self) -> &'static str {
        match self {
            Operation::Create => "create",
            Operation::Update => "update",
            Operation::Delete => "delete",
        }
    }
}

/// One entry for the log.
///
/// A struct rather than a long parameter list: the four `Option`/`&str`
/// arguments in the middle were trivially transposable at a call site, and
/// transposing `payload` with `previous` would silently record the change
/// backwards — which nobody would notice until a sync tried to replay it.
pub struct NewMutation<'a> {
    pub entity_type: &'a str,
    pub entity_id: &'a str,
    pub operation: Operation,
    /// Changed fields only. Whole records make every edit collide.
    pub payload: Option<&'a Value>,
    /// Values before the change, so a failed push can be explained.
    pub previous: Option<&'a Value>,
    pub base_version: i64,
    pub device_id: &'a str,
}

/// Append a mutation. Must be called inside the transaction that performs the
/// write, or a crash between the two would leave a change that never syncs.
pub fn record(tx: &Transaction<'_>, entry: NewMutation<'_>) -> Result<String> {
    let id = new_id();
    tx.execute(
        "INSERT INTO mutations (id, entity_type, entity_id, operation, payload, previous,
                                base_version, created_at, device_id, status, attempts)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'pending', 0)",
        params![
            id,
            entry.entity_type,
            entry.entity_id,
            entry.operation.as_str(),
            entry.payload.map(|value| value.to_string()),
            entry.previous.map(|value| value.to_string()),
            entry.base_version,
            now_iso(),
            entry.device_id,
        ],
    )?;
    Ok(id)
}

/// How many local changes are waiting to reach a server. Shown in the offline
/// indicator so the user can see their work is queued, not lost.
pub fn pending_count(connection: &rusqlite::Connection) -> Result<i64> {
    Ok(connection.query_row(
        "SELECT COUNT(*) FROM mutations WHERE status IN ('pending', 'failed')",
        [],
        |row| row.get(0),
    )?)
}

/// Compute the changed subset of a contact update.
///
/// Sending the whole record would make every edit collide with every other
/// edit; sending only what moved is what lets two devices touch one contact
/// without a conflict.
pub fn diff(previous: &Value, next: &Value) -> Value {
    let (Some(before), Some(after)) = (previous.as_object(), next.as_object()) else {
        return next.clone();
    };

    let mut changed = serde_json::Map::new();
    for (key, value) in after {
        if before.get(key) != Some(value) {
            changed.insert(key.clone(), value.clone());
        }
    }
    Value::Object(changed)
}
