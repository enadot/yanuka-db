//! Maintenance of the full-text and trigram indexes.
//!
//! The searchable document for a contact is a join across eight tables, so it
//! cannot be maintained by SQL triggers: a trigger on `contact_tags` has no
//! view of the contact's aliases, and none of them can call the Hebrew
//! normalizer. Instead, `reindex_contact` is called at the end of every
//! mutating operation, inside the same transaction, so the index can never be
//! out of step with the row it describes.

use rusqlite::{params, Transaction};
use yanuka_search::{expand_token, normalize_text, phonetic_key, tokenize};

use crate::error::Result;

/// Rebuild the FTS5 and trigram rows for one contact.
///
/// Delete-then-insert rather than update: FTS5 external rows have no stable
/// rowid we control, and a partial update that silently failed would leave a
/// contact findable under a name they no longer have.
pub fn reindex_contact(tx: &Transaction<'_>, contact_id: &str) -> Result<()> {
    tx.execute("DELETE FROM contact_fts WHERE contact_id = ?1", params![contact_id])?;
    tx.execute("DELETE FROM contact_trigram WHERE contact_id = ?1", params![contact_id])?;

    let row = tx.query_row(
        "SELECT display_name, first_name, last_name, prefix, profession, role,
                city, region, country, notes, reason_for_saving, introduced_by, deleted_at
           FROM contacts WHERE id = ?1",
        params![contact_id],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, Option<String>>(7)?,
                row.get::<_, Option<String>>(8)?,
                row.get::<_, Option<String>>(9)?,
                row.get::<_, Option<String>>(10)?,
                row.get::<_, Option<String>>(11)?,
                row.get::<_, Option<String>>(12)?,
            ))
        },
    );

    let Ok((
        display_name,
        first_name,
        last_name,
        prefix,
        profession,
        role,
        city,
        region,
        country,
        notes,
        reason_for_saving,
        introduced_by,
        deleted_at,
    )) = row
    else {
        // The contact is gone; the deletes above are the whole job.
        return Ok(());
    };

    // A soft-deleted contact stays in the table so the deletion can sync, but it
    // must not appear in search until it is restored.
    if deleted_at.is_some() {
        return Ok(());
    }

    let aliases = collect(
        tx,
        "SELECT value FROM contact_aliases WHERE contact_id = ?1 AND deleted_at IS NULL",
        contact_id,
    )?;
    let specialties = collect(
        tx,
        "SELECT value FROM contact_specialties WHERE contact_id = ?1 AND deleted_at IS NULL",
        contact_id,
    )?;
    let tags = collect(
        tx,
        "SELECT t.name FROM contact_tags ct JOIN tags t ON t.id = ct.tag_id
          WHERE ct.contact_id = ?1 AND ct.deleted_at IS NULL AND t.deleted_at IS NULL",
        contact_id,
    )?;
    let categories = collect(
        tx,
        "SELECT c.name FROM contact_categories cc JOIN categories c ON c.id = cc.category_id
          WHERE cc.contact_id = ?1 AND cc.deleted_at IS NULL AND c.deleted_at IS NULL",
        contact_id,
    )?;
    let organizations = collect(
        tx,
        "SELECT o.name FROM contact_organizations co JOIN organizations o ON o.id = co.organization_id
          WHERE co.contact_id = ?1 AND co.deleted_at IS NULL AND o.deleted_at IS NULL",
        contact_id,
    )?;
    let phone_digits = collect(
        tx,
        "SELECT digits FROM contact_phones WHERE contact_id = ?1 AND deleted_at IS NULL",
        contact_id,
    )?;
    let extra_notes = collect(
        tx,
        "SELECT body FROM notes WHERE contact_id = ?1 AND deleted_at IS NULL",
        contact_id,
    )?;

    // The phonetic skeleton is stored alongside the real spelling so a
    // misspelled query can be answered by the same index rather than a second
    // lookup. See docs/SEARCH.md.
    let mut name_parts = vec![
        display_name.clone(),
        first_name.unwrap_or_default(),
        last_name.unwrap_or_default(),
        prefix.unwrap_or_default(),
    ];
    for token in tokenize(&normalize_text(&format!("{display_name} {}", aliases.join(" ")))) {
        name_parts.push(phonetic_key(&token));
    }

    tx.execute(
        "INSERT INTO contact_fts (contact_id, name, aliases, profession, role, specialties,
                                  organization, city, country, tags, categories, notes,
                                  reason_for_saving)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            contact_id,
            join_normalized(&name_parts),
            join_normalized(&aliases),
            join_normalized(&[profession.unwrap_or_default()]),
            join_normalized(&[role.unwrap_or_default()]),
            join_normalized(&specialties),
            join_normalized(&organizations),
            join_normalized(&[city.unwrap_or_default(), region.unwrap_or_default()]),
            join_normalized(&[country.unwrap_or_default()]),
            join_normalized(&tags),
            join_normalized(&categories),
            // `introduced_by` lives with the notes because "who sent them to us"
            // is the same kind of recall cue and users search it the same way.
            join_free_text(
                &[vec![notes.unwrap_or_default(), introduced_by.unwrap_or_default()], extra_notes]
                    .concat()
            ),
            join_free_text(&[reason_for_saving.unwrap_or_default()]),
        ],
    )?;

    // Names, aliases and phone digits only. Trigram-indexing free text would
    // multiply the index size for a layer that exists to rescue misspelled
    // *names*.
    let haystack = join_normalized(&[vec![display_name], aliases, phone_digits].concat());
    tx.execute(
        "INSERT INTO contact_trigram (contact_id, haystack) VALUES (?1, ?2)",
        params![contact_id, haystack],
    )?;

    Ok(())
}

fn collect(tx: &Transaction<'_>, sql: &str, contact_id: &str) -> Result<Vec<String>> {
    let mut statement = tx.prepare(sql)?;
    let rows = statement.query_map(params![contact_id], |row| row.get::<_, String>(0))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Normalize free text and additionally index the proclitic-stripped form of
/// each word.
///
/// FTS5 tokenizes `בתפילין` as a single word, so a search for `תפילין` cannot
/// reach it. Query-side expansion covers the opposite case (the user types the
/// prefixed form), but not this one — and in a note, the prefixed form is the
/// normal way to write it.
///
/// Applied to notes and `reason_for_saving` only, never to names. In a long
/// piece of free text recall is what matters and an extra token costs little;
/// in a name, a spurious variant would pollute the highest-weighted field in
/// the index.
fn join_free_text(values: &[String]) -> String {
    let base = join_normalized(values);
    let mut out: Vec<String> = Vec::new();

    for token in tokenize(&base) {
        if !out.contains(&token) {
            out.push(token.clone());
        }
        for variant in expand_token(&token) {
            if variant != token && !out.contains(&variant) {
                out.push(variant);
            }
        }
    }

    out.join(" ")
}

fn join_normalized(values: &[String]) -> String {
    let mut seen = Vec::new();
    for value in values {
        let normalized = normalize_text(value);
        if !normalized.is_empty() && !seen.contains(&normalized) {
            seen.push(normalized);
        }
    }
    seen.join(" ")
}

/// Rebuild every contact's index rows.
///
/// Used after a bulk import or a sync apply, where per-row reindexing would
/// dominate the runtime, and by the maintenance action in Settings.
pub fn reindex_all(tx: &Transaction<'_>) -> Result<usize> {
    let ids: Vec<String> = {
        let mut statement = tx.prepare("SELECT id FROM contacts WHERE deleted_at IS NULL")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };

    for id in &ids {
        reindex_contact(tx, id)?;
    }

    tx.execute_batch("INSERT INTO contact_fts(contact_fts) VALUES('optimize')")?;
    Ok(ids.len())
}
