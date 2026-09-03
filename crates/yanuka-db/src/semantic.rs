//! Semantic search: find a contact by what a note *means*, not the words it
//! uses. See ADR-036.
//!
//! A small multilingual sentence-embedding model (multilingual-e5-small,
//! int8-quantized ONNX) runs locally through ONNX Runtime — nothing leaves the
//! machine, satisfying the offline requirement the same way FTS does. Every
//! searchable document — a contact's profile, or one note — is embedded into a
//! 384-dimensional vector stored in `semantic_index`; a query is embedded the
//! same way and candidates are ranked by cosine similarity.
//!
//! The index is maintained by *reconciliation*, not triggers: the desired set
//! of documents is derived from the live tables, compared against what the
//! index holds, and the difference embedded or deleted. That makes the same
//! routine serve every case — a single edited contact, a fresh install
//! indexing everything in the background, and a model upgrade that invalidates
//! every row via the `model` column.
//!
//! Everything here is additive to search: when the model is missing or fails
//! to load, the lexical layers answer alone and the only trace is the settings
//! screen saying so.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Mutex;

use rusqlite::{params, Connection};

use crate::error::{DbError, Result};
use crate::migrate::checksum;

/// Identifies the embedding model; stored on every row so swapping models
/// invalidates the whole index by comparison rather than by convention.
pub const MODEL_TAG: &str = "multilingual-e5-small-q8";

/// Embedding width of multilingual-e5-small.
pub const DIM: usize = 384;

/// Cosine similarity below this is noise for this model — e5 compresses its
/// score range, and unrelated Hebrew texts already land around 0.78–0.82.
const MIN_COSINE: f32 = 0.80;

/// At most this many semantic candidates join a search's result set. Semantic
/// hits rescue a failed lexical search; they must never flood a good one.
const MAX_CANDIDATES: usize = 8;

/// Token budget per embedded document. Notes are short; this is a guard, not a
/// working limit — e5 itself was trained at 512.
const MAX_TOKENS: usize = 480;

/// A semantic match, before it becomes a search candidate.
pub struct SemanticHit {
    pub contact_id: String,
    /// Cosine similarity in the model's own scale.
    pub cosine: f32,
    /// The text of the best-matching document, when it is a note — shown to
    /// the user as "why did this surface".
    pub snippet: Option<String>,
}

/// Index freshness, as the settings screen reports it.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexStatus {
    pub indexed: usize,
    pub pending: usize,
}

/// Outcome of one background reconciliation step.
pub struct SyncOutcome {
    pub embedded: usize,
    pub pending: usize,
}

/// The embedding model, loaded once and shared behind the app state.
///
/// `ort`'s `Session::run` takes `&mut self`, so the session sits behind its
/// own mutex; embedding a query is milliseconds, and the database connection
/// has its own lock anyway.
pub struct SemanticEngine {
    session: Mutex<ort::session::Session>,
    tokenizer: tokenizers::Tokenizer,
    wants_token_type: bool,
}

fn semantic_error<E: std::fmt::Display>(error: E) -> DbError {
    DbError::Semantic(error.to_string())
}

impl SemanticEngine {
    /// Load the model and tokenizer from disk.
    ///
    /// A missing file is a normal condition — a development run without the
    /// fetched model — and the caller treats the error as "semantic search
    /// unavailable", never as a startup failure.
    pub fn load(model_path: &Path, tokenizer_path: &Path) -> Result<Self> {
        let mut tokenizer =
            tokenizers::Tokenizer::from_file(tokenizer_path).map_err(semantic_error)?;
        tokenizer
            .with_truncation(Some(tokenizers::TruncationParams {
                max_length: MAX_TOKENS,
                ..Default::default()
            }))
            .map_err(semantic_error)?;

        let session = ort::session::Session::builder()
            .map_err(semantic_error)?
            .with_intra_threads(2)
            .map_err(semantic_error)?
            .commit_from_file(model_path)
            .map_err(semantic_error)?;

        let wants_token_type =
            session.inputs().iter().any(|input| input.name() == "token_type_ids");

        Ok(Self { session: Mutex::new(session), tokenizer, wants_token_type })
    }

    /// Embed one text: tokenize, run the model, mean-pool over the attention
    /// mask, L2-normalize. Normalized vectors make cosine similarity a plain
    /// dot product at query time.
    fn embed(&self, prefix: &str, text: &str) -> Result<Vec<f32>> {
        let encoding =
            self.tokenizer.encode(format!("{prefix}{text}"), true).map_err(semantic_error)?;
        let ids: Vec<i64> = encoding.get_ids().iter().map(|&id| id as i64).collect();
        let mask: Vec<i64> = encoding.get_attention_mask().iter().map(|&m| m as i64).collect();
        if ids.is_empty() {
            return Err(DbError::Semantic("טקסט ריק".into()));
        }

        let shape = [1usize, ids.len()];
        let mut inputs = vec![
            (
                std::borrow::Cow::Borrowed("input_ids"),
                ort::session::SessionInputValue::from(
                    ort::value::Tensor::from_array((shape, ids)).map_err(semantic_error)?,
                ),
            ),
            (
                std::borrow::Cow::Borrowed("attention_mask"),
                ort::session::SessionInputValue::from(
                    ort::value::Tensor::from_array((shape, mask.clone()))
                        .map_err(semantic_error)?,
                ),
            ),
        ];
        if self.wants_token_type {
            inputs.push((
                std::borrow::Cow::Borrowed("token_type_ids"),
                ort::session::SessionInputValue::from(
                    ort::value::Tensor::from_array((shape, vec![0i64; shape[1]]))
                        .map_err(semantic_error)?,
                ),
            ));
        }

        let mut session = self
            .session
            .lock()
            .map_err(|_| DbError::Semantic("מנוע ההטמעה נעול על ידי קריאה שנכשלה".into()))?;
        let outputs = session.run(inputs).map_err(semantic_error)?;
        let (out_shape, values) = outputs[0].try_extract_tensor::<f32>().map_err(semantic_error)?;

        // [1, seq, hidden] — mean over the sequence axis, masked.
        let dims: Vec<i64> = out_shape.iter().copied().collect();
        let (seq, hidden) = match dims.as_slice() {
            [1, seq, hidden] => (*seq as usize, *hidden as usize),
            other => {
                return Err(DbError::Semantic(format!("צורת פלט לא צפויה: {other:?}")));
            }
        };

        let mut pooled = vec![0f32; hidden];
        let mut count = 0f32;
        for (position, &keep) in mask.iter().enumerate().take(seq) {
            if keep == 0 {
                continue;
            }
            count += 1.0;
            let row = &values[position * hidden..(position + 1) * hidden];
            for (accumulator, value) in pooled.iter_mut().zip(row) {
                *accumulator += value;
            }
        }
        if count > 0.0 {
            for value in &mut pooled {
                *value /= count;
            }
        }
        let norm = pooled.iter().map(|v| v * v).sum::<f32>().sqrt();
        if norm > 0.0 {
            for value in &mut pooled {
                *value /= norm;
            }
        }
        Ok(pooled)
    }

    /// e5 was trained with asymmetric prefixes; skipping them measurably hurts
    /// retrieval, so they are part of the engine, not the caller's problem.
    pub fn embed_query(&self, text: &str) -> Result<Vec<f32>> {
        self.embed("query: ", text)
    }

    pub fn embed_passage(&self, text: &str) -> Result<Vec<f32>> {
        self.embed("passage: ", text)
    }
}

/// One document the index should hold.
struct Doc {
    doc_id: String,
    contact_id: String,
    text: String,
}

impl Doc {
    fn hash(&self) -> String {
        checksum(&format!("{MODEL_TAG}\u{1}{}", self.text))
    }
}

/// The natural-language profile of a contact — the same information the FTS
/// document indexes, but composed as readable text, because that is what a
/// sentence-embedding model understands. Returns `None` for a deleted or
/// missing contact.
fn profile_text(connection: &Connection, contact_id: &str) -> Result<Option<String>> {
    let row = connection.query_row(
        "SELECT display_name, prefix, profession, role, city, region, country,
                notes, reason_for_saving, introduced_by, deleted_at
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
            ))
        },
    );
    let Ok((
        display_name,
        prefix,
        profession,
        role,
        city,
        region,
        country,
        notes,
        reason,
        introduced_by,
        deleted_at,
    )) = row
    else {
        return Ok(None);
    };
    if deleted_at.is_some() {
        return Ok(None);
    }

    let list = |sql: &str| -> Result<Vec<String>> {
        let mut statement = connection.prepare(sql)?;
        let rows = statement.query_map(params![contact_id], |row| row.get::<_, String>(0))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    };
    let aliases =
        list("SELECT value FROM contact_aliases WHERE contact_id = ?1 AND deleted_at IS NULL")?;
    let specialties =
        list("SELECT value FROM contact_specialties WHERE contact_id = ?1 AND deleted_at IS NULL")?;
    let tags = list(
        "SELECT t.name FROM contact_tags ct JOIN tags t ON t.id = ct.tag_id
          WHERE ct.contact_id = ?1 AND ct.deleted_at IS NULL AND t.deleted_at IS NULL",
    )?;
    let categories = list(
        "SELECT c.name FROM contact_categories cc JOIN categories c ON c.id = cc.category_id
          WHERE cc.contact_id = ?1 AND cc.deleted_at IS NULL AND c.deleted_at IS NULL",
    )?;
    let organizations = list(
        "SELECT o.name FROM contact_organizations co JOIN organizations o ON o.id = co.organization_id
          WHERE co.contact_id = ?1 AND co.deleted_at IS NULL AND o.deleted_at IS NULL",
    )?;

    let mut parts: Vec<String> = Vec::new();
    let full_name = match &prefix {
        Some(prefix) if !prefix.is_empty() => format!("{prefix} {display_name}"),
        _ => display_name,
    };
    parts.push(full_name);
    if !aliases.is_empty() {
        parts.push(format!("נקרא גם {}", aliases.join(", ")));
    }
    if let Some(profession) = profession.filter(|v| !v.is_empty()) {
        parts.push(profession);
    }
    if let Some(role) = role.filter(|v| !v.is_empty()) {
        parts.push(role);
    }
    if !specialties.is_empty() {
        parts.push(format!("מתמחה ב{}", specialties.join(", ")));
    }
    if !organizations.is_empty() {
        parts.push(organizations.join(", "));
    }
    let place: Vec<String> =
        [city, region, country].into_iter().flatten().filter(|v| !v.is_empty()).collect();
    if !place.is_empty() {
        parts.push(place.join(", "));
    }
    if !tags.is_empty() || !categories.is_empty() {
        parts.push([tags, categories].concat().join(", "));
    }
    if let Some(introduced_by) = introduced_by.filter(|v| !v.is_empty()) {
        parts.push(format!("הופנה על ידי {introduced_by}"));
    }
    if let Some(reason) = reason.filter(|v| !v.is_empty()) {
        parts.push(reason);
    }
    if let Some(notes) = notes.filter(|v| !v.is_empty()) {
        parts.push(notes);
    }

    Ok(Some(parts.join(". ")))
}

/// Every document the index should currently hold. `contact_id: Some` narrows
/// the scan to one contact for post-mutation syncs.
fn desired_docs(connection: &Connection, contact_id: Option<&str>) -> Result<Vec<Doc>> {
    let ids: Vec<String> = match contact_id {
        Some(id) => vec![id.to_string()],
        None => {
            let mut statement =
                connection.prepare("SELECT id FROM contacts WHERE deleted_at IS NULL")?;
            let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        }
    };

    let mut docs = Vec::new();
    for id in ids {
        let Some(profile) = profile_text(connection, &id)? else {
            continue;
        };
        docs.push(Doc { doc_id: format!("p:{id}"), contact_id: id.clone(), text: profile });

        let mut statement = connection
            .prepare("SELECT id, body FROM notes WHERE contact_id = ?1 AND deleted_at IS NULL")?;
        let rows = statement.query_map(params![id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (note_id, body) = row?;
            if body.trim().is_empty() {
                continue;
            }
            docs.push(Doc { doc_id: format!("n:{note_id}"), contact_id: id.clone(), text: body });
        }
    }
    Ok(docs)
}

/// Stored `(doc_id, hash)` pairs for the compared scope.
fn stored_hashes(
    connection: &Connection,
    contact_id: Option<&str>,
) -> Result<HashMap<String, String>> {
    let (sql, id) = match contact_id {
        Some(id) => (
            "SELECT doc_id, source_hash FROM semantic_index WHERE model = ?1 AND contact_id = ?2",
            Some(id),
        ),
        None => ("SELECT doc_id, source_hash FROM semantic_index WHERE model = ?1", None),
    };
    let mut statement = connection.prepare(sql)?;
    let map = |row: &rusqlite::Row<'_>| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?));
    let rows = match id {
        Some(id) => statement.query_map(params![MODEL_TAG, id], map)?,
        None => statement.query_map(params![MODEL_TAG], map)?,
    };
    Ok(rows.collect::<rusqlite::Result<HashMap<_, _>>>()?)
}

/// Reconcile part of the index: delete rows that should not exist, embed up to
/// `budget` documents that are missing or stale, report how many remain.
///
/// Deletions always run to completion — they are cheap and leaving a deleted
/// note findable would be wrong — while embedding is budgeted so the startup
/// catch-up can yield the database lock between steps.
fn reconcile(
    connection: &Connection,
    engine: &SemanticEngine,
    contact_id: Option<&str>,
    budget: usize,
) -> Result<SyncOutcome> {
    let desired = desired_docs(connection, contact_id)?;
    let stored = stored_hashes(connection, contact_id)?;

    let desired_ids: HashSet<&str> = desired.iter().map(|doc| doc.doc_id.as_str()).collect();
    for stale in stored.keys().filter(|doc_id| !desired_ids.contains(doc_id.as_str())) {
        connection.execute("DELETE FROM semantic_index WHERE doc_id = ?1", params![stale])?;
    }
    // Rows written by an older model linger under a different `model` value;
    // sweep them here so a model upgrade converges without a special path.
    match contact_id {
        Some(id) => connection.execute(
            "DELETE FROM semantic_index WHERE model != ?1 AND contact_id = ?2",
            params![MODEL_TAG, id],
        )?,
        None => connection
            .execute("DELETE FROM semantic_index WHERE model != ?1", params![MODEL_TAG])?,
    };

    let mut dirty: Vec<&Doc> = desired
        .iter()
        .filter(|doc| stored.get(&doc.doc_id).map(|hash| hash != &doc.hash()).unwrap_or(true))
        .collect();
    let pending_total = dirty.len();
    dirty.truncate(budget);

    let mut embedded = 0;
    for doc in dirty {
        let vector = engine.embed_passage(&doc.text)?;
        let bytes: Vec<u8> = vector.iter().flat_map(|value| value.to_le_bytes()).collect();
        connection.execute(
            "INSERT INTO semantic_index (doc_id, contact_id, model, source_hash, vector, indexed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(doc_id) DO UPDATE SET
               contact_id = excluded.contact_id, model = excluded.model,
               source_hash = excluded.source_hash, vector = excluded.vector,
               indexed_at = excluded.indexed_at",
            params![doc.doc_id, doc.contact_id, MODEL_TAG, doc.hash(), bytes, crate::now_iso()],
        )?;
        embedded += 1;
    }

    Ok(SyncOutcome { embedded, pending: pending_total - embedded })
}

/// Bring one contact's documents fully up to date — called after a mutation.
pub fn sync_contact(
    connection: &Connection,
    engine: &SemanticEngine,
    contact_id: &str,
) -> Result<()> {
    reconcile(connection, engine, Some(contact_id), usize::MAX)?;
    Ok(())
}

/// One bounded step of the full catch-up; loop until `pending` is zero.
pub fn sync_step(
    connection: &Connection,
    engine: &SemanticEngine,
    budget: usize,
) -> Result<SyncOutcome> {
    reconcile(connection, engine, None, budget)
}

/// How much of the desired index exists, without embedding anything.
pub fn status(connection: &Connection) -> Result<IndexStatus> {
    let desired = desired_docs(connection, None)?;
    let stored = stored_hashes(connection, None)?;
    let pending = desired
        .iter()
        .filter(|doc| stored.get(&doc.doc_id).map(|hash| hash != &doc.hash()).unwrap_or(true))
        .count();
    Ok(IndexStatus { indexed: desired.len() - pending, pending })
}

/// Rank the indexed documents against a query.
///
/// Vectors are normalized at write time, so similarity is a dot product; a
/// contact is scored by its best document, and `exclude` keeps the semantic
/// layer additive — it never re-ranks a contact the lexical layers found.
/// Every live contact with a document at or above `min_cosine` to `text` —
/// the set a category's `meaning` condition selects (ADR-038).
pub fn similar_contacts(
    connection: &Connection,
    engine: &SemanticEngine,
    text: &str,
    min_cosine: f32,
) -> Result<HashSet<String>> {
    let query_vector = engine.embed_query(text)?;
    let mut statement = connection.prepare(
        "SELECT s.contact_id, s.vector
           FROM semantic_index s JOIN contacts c ON c.id = s.contact_id
          WHERE s.model = ?1 AND c.deleted_at IS NULL",
    )?;
    let rows = statement.query_map(params![MODEL_TAG], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
    })?;
    let mut similar = HashSet::new();
    for row in rows {
        let (contact_id, bytes) = row?;
        if similar.contains(&contact_id) {
            continue;
        }
        let score: f32 = bytes
            .chunks_exact(4)
            .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .zip(&query_vector)
            .map(|(stored, query)| stored * query)
            .sum();
        if score >= min_cosine {
            similar.insert(contact_id);
        }
    }
    Ok(similar)
}

pub fn candidates(
    connection: &Connection,
    engine: &SemanticEngine,
    query: &str,
    exclude: &HashSet<String>,
) -> Result<Vec<SemanticHit>> {
    let query_vector = engine.embed_query(query)?;

    let mut statement = connection.prepare(
        "SELECT s.doc_id, s.contact_id, s.vector
           FROM semantic_index s JOIN contacts c ON c.id = s.contact_id
          WHERE s.model = ?1 AND c.deleted_at IS NULL",
    )?;
    let rows = statement.query_map(params![MODEL_TAG], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, Vec<u8>>(2)?))
    })?;

    let mut best: HashMap<String, (f32, String)> = HashMap::new();
    for row in rows {
        let (doc_id, contact_id, bytes) = row?;
        if exclude.contains(&contact_id) {
            continue;
        }
        let score: f32 = bytes
            .chunks_exact(4)
            .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .zip(&query_vector)
            .map(|(stored, query)| stored * query)
            .sum();
        let entry = best.entry(contact_id).or_insert((f32::MIN, String::new()));
        if score > entry.0 {
            *entry = (score, doc_id);
        }
    }

    let mut hits: Vec<(String, f32, String)> = best
        .into_iter()
        .filter(|(_, (score, _))| *score >= MIN_COSINE)
        .map(|(contact_id, (score, doc_id))| (contact_id, score, doc_id))
        .collect();
    hits.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    hits.truncate(MAX_CANDIDATES);

    hits.into_iter()
        .map(|(contact_id, cosine, doc_id)| {
            let snippet = doc_id.strip_prefix("n:").and_then(|note_id| {
                connection
                    .query_row("SELECT body FROM notes WHERE id = ?1", params![note_id], |row| {
                        row.get::<_, String>(0)
                    })
                    .ok()
                    .map(|body| {
                        let mut short: String = body.chars().take(120).collect();
                        if body.chars().count() > 120 {
                            short.push('…');
                        }
                        short
                    })
            });
            Ok(SemanticHit { contact_id, cosine, snippet })
        })
        .collect()
}

/// Map a cosine score onto the candidate-score factor used by the search
/// layer: 0.80 (the floor) → 0.5, 0.90 and above → 1.0.
pub fn cosine_factor(cosine: f32) -> f64 {
    let squashed = ((cosine - MIN_COSINE) / 0.10).clamp(0.0, 1.0) as f64;
    0.5 + 0.5 * squashed
}
