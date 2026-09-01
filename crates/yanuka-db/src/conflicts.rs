//! Two people, two devices, one field, two answers.
//!
//! `apply.rs` refuses to pick between them. It keeps what is here, records what
//! arrived, and moves on — because a merge that silently discards a sentence
//! somebody typed is the failure this whole design exists to prevent. That
//! leaves a decision outstanding, and this module is where it gets made.
//!
//! Two things about resolving are less obvious than they look:
//!
//! * **A decision is itself a change, and has to travel.** When a conflict is
//!   seen on this device it is also, symmetrically, sitting on the other one:
//!   the archive holds X here and Y there. Choosing X changes nothing locally —
//!   X was already here — so the ordinary "log what changed" path records
//!   nothing, and the other device keeps Y forever. So `resolve` writes its own
//!   mutation with the losing value as the baseline, which is precisely the
//!   shape the other device needs to fold it in without a second conflict.
//!
//! * **A conflict can stop being one without anybody choosing.** When the other
//!   device resolves first, its decision arrives here as an ordinary change and
//!   both sides end up agreeing. The open record has to close then too, or the
//!   user is asked to decide something that is already settled — and each time
//!   they are asked about a non-question, the next real one gets less
//!   attention. `settle` is called from the apply path for exactly that.
//!
//! Only contacts are covered. Notes and relationships are append-and-tombstone
//! rather than edit-in-place, so two devices cannot produce competing values
//! for the same field of one; there is nothing to choose between.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::error::{DbError, Result};
use crate::index::reindex_contact;
use crate::models::ContactInput;
use crate::mutation::{self, NewMutation, Operation};
use crate::now_iso;
use crate::repository::{as_input, device_id, get_contact, update_contact_row};

/// One field where the two devices disagree, and everything needed to tell the
/// two answers apart without leaving the screen.
///
/// Written by `apply.rs` and read here, through this one struct rather than two
/// hand-built JSON shapes — the previous arrangement had the writer emitting a
/// `json!` literal and nothing at all reading it, which is exactly how a key
/// gets renamed on one side only.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldConflict {
    pub field: String,
    pub local_value: Value,
    pub remote_value: Value,
    pub local_updated_at: String,
    pub remote_updated_at: String,
    pub local_device_id: Option<String>,
    pub remote_device_id: Option<String>,
}

/// An outstanding decision, with enough of the contact attached that the screen
/// can say whose details these are.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenConflict {
    pub id: String,
    pub entity_type: String,
    pub entity_id: String,
    /// The contact's name as it stands locally. `None` if the contact has since
    /// been deleted, which is itself an answer: the row is shown as settled.
    pub display_name: Option<String>,
    pub detected_at: String,
    pub fields: Vec<FieldConflict>,
}

/// Which answer the person kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    /// What this machine already held.
    Local,
    /// What arrived from the other one.
    Remote,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldChoice {
    pub field: String,
    pub side: Side,
}

/// Every conflict still waiting on a person, oldest first.
///
/// Oldest first because a conflict does not become less true by sitting there,
/// and the one that has been unanswered longest is the one whose two versions
/// are furthest apart.
pub fn open(connection: &Connection) -> Result<Vec<OpenConflict>> {
    let mut statement = connection.prepare(
        "SELECT c.id, c.entity_type, c.entity_id, c.fields, c.detected_at, k.display_name
           FROM conflicts c
           LEFT JOIN contacts k ON k.id = c.entity_id AND k.deleted_at IS NULL
          WHERE c.resolved_at IS NULL
          ORDER BY c.detected_at ASC",
    )?;

    let rows = statement.query_map([], |row| {
        let fields: String = row.get(3)?;
        Ok((
            OpenConflict {
                id: row.get(0)?,
                entity_type: row.get(1)?,
                entity_id: row.get(2)?,
                display_name: row.get(5)?,
                detected_at: row.get(4)?,
                fields: Vec::new(),
            },
            fields,
        ))
    })?;

    let mut conflicts = Vec::new();
    for row in rows {
        let (mut conflict, fields) = row?;
        conflict.fields = serde_json::from_str(&fields)?;
        // A record whose field list is empty asks nothing. It should not exist
        // — every path that empties one closes it — but showing an empty
        // decision is worse than showing nothing, so it is skipped rather than
        // trusted.
        if !conflict.fields.is_empty() {
            conflicts.push(conflict);
        }
    }
    Ok(conflicts)
}

/// How many decisions are outstanding. Used by the indicator and the settings
/// screen; counts records, not fields, because a person resolves a contact.
pub fn open_count(connection: &Connection) -> Result<i64> {
    Ok(connection.query_row(
        "SELECT COUNT(*) FROM conflicts WHERE resolved_at IS NULL",
        [],
        |row| row.get(0),
    )?)
}

/// Record the decision, apply it, and tell the other device.
///
/// `choices` need not cover every field: what is not chosen stays open. That
/// matters for a contact where one disagreement is obvious and another needs a
/// phone call to settle — the obvious one should not have to wait.
pub fn resolve(
    connection: &mut Connection,
    conflict_id: &str,
    choices: &[FieldChoice],
) -> Result<()> {
    if choices.is_empty() {
        return Err(DbError::Validation("לא נבחרה אף גרסה".into()));
    }

    let device = device_id(connection)?;
    let now = now_iso();
    let tx = connection.transaction()?;

    let row: Option<(String, String, String)> = tx
        .query_row(
            "SELECT entity_type, entity_id, fields
               FROM conflicts WHERE id = ?1 AND resolved_at IS NULL",
            params![conflict_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let Some((entity_type, entity_id, recorded)) = row else {
        // Already settled — most likely by the other device, between the screen
        // being drawn and the button being pressed. Not an error: the outcome
        // the caller wanted is the outcome that holds.
        return Ok(());
    };
    if entity_type != "contact" {
        return Err(DbError::Validation(format!("אין מסך הכרעה עבור {entity_type}")));
    }

    let recorded: Vec<FieldConflict> = serde_json::from_str(&recorded)?;
    let stored =
        get_contact(&tx, &entity_id)?.ok_or_else(|| DbError::NotFound("איש הקשר".to_string()))?;

    let mut merged: Map<String, Value> =
        serde_json::to_value(as_input(&stored))?.as_object().cloned().unwrap_or_default();
    let mut payload = Map::new();
    let mut previous = Map::new();
    let mut unresolved: Vec<FieldConflict> = Vec::new();
    let mut sides: Vec<Side> = Vec::new();

    for entry in recorded {
        let Some(choice) = choices.iter().find(|choice| choice.field == entry.field) else {
            unresolved.push(entry);
            continue;
        };
        let (kept, lost) = match choice.side {
            Side::Local => (&entry.local_value, &entry.remote_value),
            Side::Remote => (&entry.remote_value, &entry.local_value),
        };
        merged.insert(entry.field.clone(), kept.clone());
        payload.insert(entry.field.clone(), kept.clone());
        // The losing value, not the local one. It is what the *other* device
        // still holds, and stating it as the baseline is what lets that device
        // recognise the decision as settling its own copy rather than as a
        // fresh edit to argue with.
        previous.insert(entry.field.clone(), lost.clone());
        sides.push(choice.side);
    }

    if payload.is_empty() {
        return Err(DbError::Validation("הבחירה אינה מתאימה לשדות שבמחלוקת".into()));
    }

    let input: ContactInput = serde_json::from_value(Value::Object(merged))?;
    let version = stored.contact.version + 1;
    update_contact_row(&tx, &entity_id, &input, &now, &device, version)?;
    reindex_contact(&tx, &entity_id)?;

    mutation::record(
        &tx,
        NewMutation {
            entity_type: "contact",
            entity_id: &entity_id,
            operation: Operation::Update,
            payload: Some(&Value::Object(payload)),
            previous: Some(&Value::Object(previous)),
            base_version: stored.contact.version,
            device_id: &device,
        },
    )?;

    if unresolved.is_empty() {
        // `local`/`remote` when the whole record went one way, `manual` when the
        // person took some of each. Informational only — nothing reads it back
        // — so a partial resolution finished over two sittings is labelled by
        // the sitting that closed it.
        let resolution = if sides.iter().all(|side| *side == Side::Local) {
            "local"
        } else if sides.iter().all(|side| *side == Side::Remote) {
            "remote"
        } else {
            "manual"
        };
        tx.execute(
            "UPDATE conflicts SET fields = '[]', resolved_at = ?2, resolution = ?3 WHERE id = ?1",
            params![conflict_id, now, resolution],
        )?;
    } else {
        tx.execute(
            "UPDATE conflicts SET fields = ?2 WHERE id = ?1",
            params![conflict_id, serde_json::to_string(&unresolved)?],
        )?;
    }

    tx.commit()?;
    Ok(())
}

/// Close what an incoming change has already answered.
///
/// Called from the apply path with the fields on which the two devices now
/// agree — either because the arriving value matched what was here, or because
/// nothing local had touched the field and theirs was taken. Either way there
/// is no longer a question, and leaving the record open would teach the user
/// that this screen shows things that do not need them.
pub(crate) fn settle(
    tx: &rusqlite::Transaction<'_>,
    entity_id: &str,
    agreed: &[String],
) -> Result<()> {
    if agreed.is_empty() {
        return Ok(());
    }

    let mut statement = tx
        .prepare("SELECT id, fields FROM conflicts WHERE entity_id = ?1 AND resolved_at IS NULL")?;
    let rows: Vec<(String, String)> = statement
        .query_map(params![entity_id], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<std::result::Result<_, _>>()?;
    drop(statement);

    let now = now_iso();
    for (id, fields) in rows {
        let entries: Vec<FieldConflict> = serde_json::from_str(&fields)?;
        let remaining: Vec<FieldConflict> =
            entries.into_iter().filter(|entry| !agreed.contains(&entry.field)).collect();

        if remaining.is_empty() {
            tx.execute(
                "UPDATE conflicts
                    SET fields = '[]', resolved_at = ?2, resolution = 'remote'
                  WHERE id = ?1",
                params![id, now],
            )?;
        } else {
            tx.execute(
                "UPDATE conflicts SET fields = ?2 WHERE id = ?1",
                params![id, serde_json::to_string(&remaining)?],
            )?;
        }
    }
    Ok(())
}
