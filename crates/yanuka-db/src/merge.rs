//! Duplicate detection across the whole database, and lossless contact merge.
//!
//! Both exist for the same moment: an archive imported from several sources
//! (notebooks, Google, Outlook) inevitably holds the same person more than
//! once. The product's first priority — מידע לא הולך לאיבוד — decides every
//! rule in this module:
//!
//! - the merged (losing) contact is soft-deleted, never erased, and the
//!   mutation log records its complete pre-merge state;
//! - child rows move to the kept contact, skipping only true value-duplicates;
//! - a scalar field where both contacts disagree keeps the kept contact's
//!   value and preserves the other one as a line in the notes.

use rusqlite::{params, Connection};
use serde_json::json;

use crate::error::{DbError, Result};
use crate::index::reindex_contact;
use crate::models::*;
use crate::mutation::{self, Operation};
use crate::now_iso;
use crate::repository::{device_id, get_contact, summarize};

/// Two contacts that are likely the same person.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicatePair {
    pub first: ContactSummary,
    pub second: ContactSummary,
    /// 0-1. The strongest single signal, not a sum.
    pub confidence: f64,
    pub reasons: Vec<String>,
}

/// Scan for likely-duplicate pairs: shared phone (last 7 digits), shared
/// email, or an identical normalized name.
///
/// A shared phone or email is close to proof; a shared name alone is a hint
/// (`כהן` is everywhere in this dataset) and scores accordingly. Capped
/// because the screen shows pairs one decision at a time — nobody resolves
/// two hundred in a sitting, and the next scan finds the rest.
pub fn list_duplicate_pairs(connection: &Connection, limit: i64) -> Result<Vec<DuplicatePair>> {
    use std::collections::BTreeMap;

    // (first_id, second_id) -> (confidence, reasons)
    let mut pairs: BTreeMap<(String, String), (f64, Vec<String>)> = BTreeMap::new();
    let mut merge_signal = |a: String, b: String, confidence: f64, reason: &str| {
        let key = if a < b { (a, b) } else { (b, a) };
        let entry = pairs.entry(key).or_insert((0.0, Vec::new()));
        if entry.0 < confidence {
            entry.0 = confidence;
        }
        if !entry.1.iter().any(|r| r == reason) {
            entry.1.push(reason.to_string());
        }
    };

    let mut phones = connection.prepare(
        "SELECT DISTINCT p1.contact_id, p2.contact_id
           FROM contact_phones p1
           JOIN contact_phones p2
             ON substr(p1.digits, -7) = substr(p2.digits, -7)
            AND p1.contact_id < p2.contact_id
           JOIN contacts c1 ON c1.id = p1.contact_id
           JOIN contacts c2 ON c2.id = p2.contact_id
          WHERE length(p1.digits) >= 7 AND length(p2.digits) >= 7
            AND p1.deleted_at IS NULL AND p2.deleted_at IS NULL
            AND c1.deleted_at IS NULL AND c2.deleted_at IS NULL",
    )?;
    let phone_rows: Vec<(String, String)> = phones
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (a, b) in phone_rows {
        merge_signal(a, b, 0.9, "אותו מספר טלפון");
    }

    let mut emails = connection.prepare(
        "SELECT DISTINCT e1.contact_id, e2.contact_id
           FROM contact_emails e1
           JOIN contact_emails e2
             ON e1.normalized = e2.normalized AND e1.contact_id < e2.contact_id
           JOIN contacts c1 ON c1.id = e1.contact_id
           JOIN contacts c2 ON c2.id = e2.contact_id
          WHERE e1.normalized <> ''
            AND e1.deleted_at IS NULL AND e2.deleted_at IS NULL
            AND c1.deleted_at IS NULL AND c2.deleted_at IS NULL",
    )?;
    let email_rows: Vec<(String, String)> = emails
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (a, b) in email_rows {
        merge_signal(a, b, 0.85, "אותה כתובת אימייל");
    }

    let mut names = connection.prepare(
        "SELECT c1.id, c2.id
           FROM contacts c1
           JOIN contacts c2 ON c1.normalized_name = c2.normalized_name AND c1.id < c2.id
          WHERE c1.normalized_name <> ''
            AND c1.deleted_at IS NULL AND c2.deleted_at IS NULL",
    )?;
    let name_rows: Vec<(String, String)> = names
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (a, b) in name_rows {
        merge_signal(a, b, 0.5, "שם זהה");
    }

    // Strongest evidence first, so the screen leads with the near-certain.
    let mut ordered: Vec<_> = pairs.into_iter().collect();
    ordered.sort_by(|a, b| b.1 .0.partial_cmp(&a.1 .0).unwrap_or(std::cmp::Ordering::Equal));
    ordered.truncate(limit.max(0) as usize);

    let mut result = Vec::with_capacity(ordered.len());
    for ((first_id, second_id), (confidence, reasons)) in ordered {
        let first = contact_row(connection, &first_id)?;
        let second = contact_row(connection, &second_id)?;
        let (Some(first), Some(second)) = (first, second) else {
            continue;
        };
        result.push(DuplicatePair {
            first: summarize(connection, first)?,
            second: summarize(connection, second)?,
            confidence,
            reasons,
        });
    }
    Ok(result)
}

fn contact_row(connection: &Connection, id: &str) -> Result<Option<Contact>> {
    use rusqlite::OptionalExtension;
    Ok(connection
        .query_row(
            "SELECT * FROM contacts WHERE id = ?1 AND deleted_at IS NULL",
            params![id],
            crate::repository::contact_from_row,
        )
        .optional()?)
}

/// The scalar fields carried over when the kept contact's own value is empty,
/// with the Hebrew label used when a conflicting value is preserved in notes.
const SCALAR_FIELDS: &[(&str, &str)] = &[
    ("first_name", "שם פרטי"),
    ("last_name", "שם משפחה"),
    ("prefix", "תואר"),
    ("title", "תואר אחרי השם"),
    ("country", "מדינה"),
    ("region", "אזור"),
    ("city", "עיר"),
    ("address", "כתובת"),
    ("postal_code", "מיקוד"),
    ("profession", "מקצוע"),
    ("role", "תפקיד"),
    ("reason_for_saving", "נשמר בגלל"),
    ("source", "מקור"),
    ("introduced_by", "הכיר בינינו"),
];

/// Merge `merge_id` into `keep_id`, transactionally and without losing data.
///
/// Children (phones, emails, aliases, tags, categories, specialties,
/// languages, organization links, notes, relationship edges) move to the kept
/// contact unless the kept contact already holds the same value, in which case
/// the duplicate row is soft-deleted in place. Scalar conflicts keep the kept
/// contact's value and append the other to notes. The merged contact is
/// soft-deleted, and the mutation log holds its full pre-merge state.
pub fn merge_contacts(
    connection: &mut Connection,
    keep_id: &str,
    merge_id: &str,
) -> Result<ContactWithRelations> {
    if keep_id == merge_id {
        return Err(DbError::Validation("לא ניתן למזג איש קשר עם עצמו".into()));
    }
    let keep = get_contact(connection, keep_id)?
        .ok_or_else(|| DbError::NotFound("איש הקשר שנשמר".into()))?;
    let merged = get_contact(connection, merge_id)?
        .ok_or_else(|| DbError::NotFound("איש הקשר שממוזג".into()))?;
    if keep.contact.deleted_at.is_some() || merged.contact.deleted_at.is_some() {
        return Err(DbError::Validation("לא ניתן למזג איש קשר שנמחק".into()));
    }

    let device = device_id(connection)?;
    let now = now_iso();
    // The complete pre-merge state of the losing contact, kept in the log so
    // the merge is explainable and reversible by hand even after the fact.
    let previous = serde_json::to_value(&merged)
        .map_err(|e| DbError::Validation(format!("serialization failed: {e}")))?;

    let tx = connection.transaction()?;

    // -- scalar fields: fill blanks on the kept side, preserve conflicts -----
    let mut extra_notes: Vec<String> = Vec::new();
    for (column, label) in SCALAR_FIELDS {
        let keep_value: Option<String> = tx.query_row(
            &format!("SELECT {column} FROM contacts WHERE id = ?1"),
            params![keep_id],
            |row| row.get(0),
        )?;
        let merge_value: Option<String> = tx.query_row(
            &format!("SELECT {column} FROM contacts WHERE id = ?1"),
            params![merge_id],
            |row| row.get(0),
        )?;
        match (keep_value, merge_value) {
            (None, Some(value)) => {
                tx.execute(
                    &format!("UPDATE contacts SET {column} = ?2 WHERE id = ?1"),
                    params![keep_id, value],
                )?;
            }
            (Some(kept), Some(other)) if kept != other => {
                extra_notes.push(format!("{label}: {other}"));
            }
            _ => {}
        }
    }
    if let Some(merge_notes) = &merged.contact.notes {
        if !merge_notes.trim().is_empty()
            && keep.contact.notes.as_deref() != Some(merge_notes.as_str())
        {
            extra_notes.push(merge_notes.clone());
        }
    }
    if merged.contact.is_favorite {
        tx.execute("UPDATE contacts SET is_favorite = 1 WHERE id = ?1", params![keep_id])?;
    }
    if !extra_notes.is_empty() {
        let addition = format!(
            "— מוזג מ״{}״ ({}) —\n{}",
            merged.contact.display_name,
            now,
            extra_notes.join("\n")
        );
        tx.execute(
            "UPDATE contacts
                SET notes = CASE
                      WHEN notes IS NULL OR notes = '' THEN ?2
                      ELSE notes || char(10) || char(10) || ?2
                    END
              WHERE id = ?1",
            params![keep_id, addition],
        )?;
    }

    // -- children: move unless the kept side already has the same value ------
    // (comparison column per table; a would-be duplicate is soft-deleted).
    let moves: &[(&str, &str)] = &[
        ("contact_phones", "digits"),
        ("contact_emails", "normalized"),
        ("contact_aliases", "normalized"),
        ("contact_specialties", "normalized"),
        ("contact_languages", "language_code"),
        ("contact_tags", "tag_id"),
        ("contact_categories", "category_id"),
        ("contact_organizations", "organization_id"),
    ];
    for (table, compare) in moves {
        tx.execute(
            &format!(
                "UPDATE {table} SET deleted_at = ?3
                  WHERE contact_id = ?2 AND deleted_at IS NULL
                    AND {compare} IN (SELECT {compare} FROM {table}
                                       WHERE contact_id = ?1 AND deleted_at IS NULL)"
            ),
            params![keep_id, merge_id, now],
        )?;
        tx.execute(
            &format!(
                "UPDATE {table} SET contact_id = ?1
                  WHERE contact_id = ?2 AND deleted_at IS NULL"
            ),
            params![keep_id, merge_id],
        )?;
    }
    // One primary per kind of channel: the kept contact's primaries win.
    for table in ["contact_phones", "contact_emails"] {
        tx.execute(
            &format!(
                "UPDATE {table} SET is_primary = 0
                  WHERE contact_id = ?1 AND deleted_at IS NULL
                    AND id NOT IN (SELECT id FROM {table}
                                    WHERE contact_id = ?1 AND deleted_at IS NULL
                                      AND is_primary = 1
                                    ORDER BY created_at LIMIT 1)
                    AND is_primary = 1
                    AND (SELECT count(*) FROM {table}
                          WHERE contact_id = ?1 AND deleted_at IS NULL
                            AND is_primary = 1) > 1"
            ),
            params![keep_id],
        )?;
    }

    // -- notes rows and relationship edges re-point to the kept contact ------
    tx.execute(
        "UPDATE notes SET contact_id = ?1 WHERE contact_id = ?2 AND deleted_at IS NULL",
        params![keep_id, merge_id],
    )?;
    // An edge that would duplicate an existing one, or point at the kept
    // contact itself, is soft-deleted rather than moved.
    for (side, other) in
        [("from_contact_id", "to_contact_id"), ("to_contact_id", "from_contact_id")]
    {
        tx.execute(
            &format!(
                "UPDATE relationships SET deleted_at = ?3
                  WHERE {side} = ?2 AND deleted_at IS NULL
                    AND ({other} = ?1
                         OR EXISTS (SELECT 1 FROM relationships r2
                                     WHERE r2.{side} = ?1
                                       AND r2.{other} = relationships.{other}
                                       AND r2.type = relationships.type
                                       AND r2.deleted_at IS NULL))"
            ),
            params![keep_id, merge_id, now],
        )?;
        tx.execute(
            &format!(
                "UPDATE relationships SET {side} = ?1 WHERE {side} = ?2 AND deleted_at IS NULL"
            ),
            params![keep_id, merge_id],
        )?;
    }
    tx.execute(
        "UPDATE contacts SET introduced_by_contact_id = ?1 WHERE introduced_by_contact_id = ?2",
        params![keep_id, merge_id],
    )?;

    // -- close out: soft-delete the merged contact, log both sides -----------
    tx.execute(
        "UPDATE contacts SET deleted_at = ?2, updated_at = ?2, version = version + 1,
                             device_id = ?3
          WHERE id = ?1",
        params![merge_id, now, device],
    )?;
    tx.execute(
        "UPDATE contacts SET updated_at = ?2, version = version + 1, device_id = ?3
          WHERE id = ?1",
        params![keep_id, now, device],
    )?;
    mutation::record(
        &tx,
        mutation::NewMutation {
            entity_type: "contact",
            entity_id: keep_id,
            operation: Operation::Update,
            payload: Some(&json!({ "mergedFrom": merge_id })),
            previous: None,
            base_version: keep.contact.version,
            device_id: &device,
        },
    )?;
    mutation::record(
        &tx,
        mutation::NewMutation {
            entity_type: "contact",
            entity_id: merge_id,
            operation: Operation::Delete,
            payload: Some(&json!({ "mergedInto": keep_id })),
            previous: Some(&previous),
            base_version: merged.contact.version,
            device_id: &device,
        },
    )?;
    reindex_contact(&tx, keep_id)?;
    reindex_contact(&tx, merge_id)?;
    tx.commit()?;

    get_contact(connection, keep_id)?.ok_or_else(|| DbError::NotFound("איש הקשר".into()))
}
