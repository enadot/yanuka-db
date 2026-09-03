//! Search execution against SQLite.
//!
//! Layered, and each layer only runs when the one before it has not already
//! answered the question:
//!
//! 1. **Phone** — a bare run of digits is a number lookup, matched on a digit
//!    suffix so the format the user types does not matter.
//! 2. **Full text** — FTS5 over the pre-normalized document, ordered by bm25.
//! 3. **Fuzzy** — only when the first two return almost nothing. Trigram
//!    overlap proposes candidates, edit distance ranks them.
//! 4. **Semantic** — the embedding model, when the shell provides one, adds
//!    contacts whose notes *mean* what the query asks even though no word
//!    matches. Additive only: it proposes contacts the lexical layers missed
//!    and never re-ranks one they found. See ADR-036.
//!
//! Facets are computed in the same pass over the candidate set, so the result
//! count, the rows and the filter counts are one round trip rather than three.

use rusqlite::{params, Connection};
use std::collections::HashMap;
use std::time::Instant;
use yanuka_search::{
    expand_token, min_shared_trigrams, normalize_text, score_match, similarity, strip_honorifics,
    tokenize, trigrams, MatchQuality, MatchSource, FUZZY_MIN_SIMILARITY, PROCLITIC_PENALTY,
};

use crate::error::Result;
use crate::models::*;
use crate::repository::summarize;

/// Number of candidates pulled out of FTS5 before ranking. Generous enough that
/// the true best answer is inside it, small enough that ranking stays trivial.
const CANDIDATE_LIMIT: i64 = 500;

/// Below this many hits the fuzzy layer is worth running. Above it, the user
/// already has plenty to look at and edit distance would only add noise.
const FUZZY_TRIGGER: usize = 20;

fn escape_fts(term: &str) -> String {
    format!("\"{}\"", term.replace('"', "\"\""))
}

/// Build an FTS5 MATCH expression from a normalized query.
///
/// Terms are AND-ed so that adding a word narrows the result set; each term's
/// proclitic variants are OR-ed, and the last term gets a `*` so results appear
/// while it is still being typed.
pub fn build_match(terms: &[String], prefix_last: bool) -> String {
    let mut clauses = Vec::new();
    for (index, term) in terms.iter().enumerate() {
        let is_last = index == terms.len() - 1;
        let variants = expand_token(term);
        let rendered: Vec<String> = variants
            .iter()
            .map(|variant| {
                if prefix_last && is_last {
                    format!("{}*", escape_fts(variant))
                } else {
                    escape_fts(variant)
                }
            })
            .collect();

        clauses.push(if rendered.len() > 1 {
            format!("({})", rendered.join(" OR "))
        } else {
            rendered.into_iter().next().unwrap_or_default()
        });
    }
    clauses.join(" AND ")
}

struct Candidate {
    contact_id: String,
    score: f64,
    reasons: Vec<MatchReason>,
}

/// The engine handle the semantic layer runs with; `()` when the feature is
/// compiled out, so `run` keeps one signature either way.
#[cfg(feature = "semantic")]
type SemanticRef<'a> = Option<&'a crate::semantic::SemanticEngine>;
#[cfg(not(feature = "semantic"))]
type SemanticRef<'a> = Option<&'a ()>;

pub fn search(connection: &Connection, query: &SearchQuery) -> Result<SearchResponse> {
    run(connection, query, None)
}

/// Search with the semantic layer active. The desktop shell calls this; every
/// other caller (tests, tools) uses `search` and gets the lexical layers.
#[cfg(feature = "semantic")]
pub fn search_with_semantic(
    connection: &Connection,
    engine: Option<&crate::semantic::SemanticEngine>,
    query: &SearchQuery,
) -> Result<SearchResponse> {
    run(connection, query, engine)
}

fn run(
    connection: &Connection,
    query: &SearchQuery,
    semantic: SemanticRef<'_>,
) -> Result<SearchResponse> {
    let started = Instant::now();

    let normalized = strip_honorifics(&normalize_text(&query.text));
    let terms = tokenize(&normalized);
    let digits: String = query.text.chars().filter(|c| c.is_ascii_digit()).collect();
    let is_phone_query = !query.text.trim().is_empty()
        && !query.text.chars().any(|c| c.is_alphabetic())
        && digits.len() >= 4;

    let mut candidates: Vec<Candidate> = if is_phone_query {
        phone_candidates(connection, &digits)?
    } else if terms.is_empty() {
        browse_candidates(connection)?
    } else {
        let mut found = fulltext_candidates(connection, &terms)?;
        // Fuzzy only rescues a search that has otherwise failed. Running it
        // always would cost time on every keystroke and dilute good results.
        if found.len() < FUZZY_TRIGGER {
            let existing: Vec<String> = found.iter().map(|c| c.contact_id.clone()).collect();
            found.extend(fuzzy_candidates(connection, &terms, &existing)?);
        }
        found
    };

    // Layer 4: meaning. Gated like fuzzy — a sentence-shaped query, or a
    // lexical search that came back thin, is where the model earns its keep;
    // a surname being typed letter by letter is not. Engine failures degrade
    // to the lexical answer silently: search must never break because a model
    // file is missing or corrupt.
    #[cfg(feature = "semantic")]
    if let Some(engine) = semantic {
        let wants_meaning = terms.len() >= 2 || candidates.len() < FUZZY_TRIGGER;
        if !is_phone_query && !terms.is_empty() && wants_meaning {
            let existing: std::collections::HashSet<String> =
                candidates.iter().map(|c| c.contact_id.clone()).collect();
            if let Ok(hits) =
                crate::semantic::candidates(connection, engine, &query.text, &existing)
            {
                candidates.extend(hits.into_iter().map(|hit| {
                    let score = score_match(MatchSource::Semantic, MatchQuality::Semantic, 1.0)
                        * crate::semantic::cosine_factor(hit.cosine);
                    Candidate {
                        contact_id: hit.contact_id,
                        score,
                        reasons: vec![MatchReason {
                            source: "semantic".into(),
                            quality: "semantic".into(),
                            term: query.text.clone(),
                            snippet: hit.snippet,
                            score,
                        }],
                    }
                }));
            }
        }
    }
    #[cfg(not(feature = "semantic"))]
    let _ = semantic;

    candidates.retain(|candidate| passes_filters(connection, candidate, query).unwrap_or(false));
    candidates.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

    let total = candidates.len() as i64;
    let facets = compute_facets(connection, &candidates)?;

    let limit = query.limit.unwrap_or(50) as usize;
    let offset = query.offset.unwrap_or(0) as usize;

    let mut results = Vec::new();
    for candidate in candidates.into_iter().skip(offset).take(limit) {
        let contact = connection.query_row(
            "SELECT * FROM contacts WHERE id = ?1",
            params![candidate.contact_id],
            crate::repository::contact_from_row,
        )?;
        results.push(SearchResult {
            contact: summarize(connection, contact)?,
            score: candidate.score,
            reasons: candidate.reasons,
        });
    }

    Ok(SearchResponse {
        results,
        total,
        facets,
        took_ms: started.elapsed().as_secs_f64() * 1000.0,
        normalized_terms: terms,
    })
}

/// Match on a digit suffix so `054-555-0134`, `+972545550134` and `5550134` all
/// reach the same record.
fn phone_candidates(connection: &Connection, digits: &str) -> Result<Vec<Candidate>> {
    let suffix = if digits.len() > 7 { &digits[digits.len() - 7..] } else { digits };

    let mut statement = connection.prepare(
        "SELECT DISTINCT p.contact_id, p.digits
           FROM contact_phones p JOIN contacts c ON c.id = p.contact_id
          WHERE p.deleted_at IS NULL AND c.deleted_at IS NULL
            AND p.digits LIKE '%' || ?1",
    )?;

    let rows = statement.query_map(params![suffix], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;

    let mut candidates = Vec::new();
    for row in rows {
        let (contact_id, stored) = row?;
        let quality = if stored == digits { MatchQuality::Exact } else { MatchQuality::Prefix };
        let score = score_match(MatchSource::Phone, quality, 1.0);
        candidates.push(Candidate {
            contact_id,
            score,
            reasons: vec![MatchReason {
                source: "phone".into(),
                quality: if matches!(quality, MatchQuality::Exact) {
                    "exact".into()
                } else {
                    "prefix".into()
                },
                term: digits.to_string(),
                snippet: None,
                score,
            }],
        });
    }
    Ok(candidates)
}

fn fulltext_candidates(connection: &Connection, terms: &[String]) -> Result<Vec<Candidate>> {
    let expression = build_match(terms, true);
    if expression.is_empty() {
        return Ok(Vec::new());
    }

    let mut statement = connection.prepare(
        "SELECT f.contact_id, bm25(contact_fts, 10.0, 9.0, 5.0, 3.0, 3.5, 3.5, 3.0, 2.0, 4.0, 4.0, 2.0, 2.5) AS rank
           FROM contact_fts f
           JOIN contacts c ON c.id = f.contact_id
          WHERE contact_fts MATCH ?1 AND c.deleted_at IS NULL
          ORDER BY rank
          LIMIT ?2",
    )?;

    let rows = statement.query_map(params![expression, CANDIDATE_LIMIT], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
    })?;

    let collected: Vec<(String, f64)> = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    if collected.is_empty() {
        return Ok(Vec::new());
    }

    // bm25 returns a negative number where more negative is more relevant. It is
    // only comparable inside this one result set, so it is used to order within
    // the full-text layer and never against the exact or fuzzy layers.
    let best = collected.first().map(|(_, rank)| *rank).unwrap_or(0.0);
    let worst = collected.last().map(|(_, rank)| *rank).unwrap_or(0.0);

    Ok(collected
        .into_iter()
        .map(|(contact_id, rank)| {
            let factor = yanuka_search::bm25_to_quality_factor(rank, best, worst);
            let score = score_match(MatchSource::Name, MatchQuality::Fulltext, 1.0) * factor;
            Candidate {
                contact_id,
                score,
                reasons: vec![MatchReason {
                    source: "name".into(),
                    quality: "fulltext".into(),
                    term: terms.join(" "),
                    snippet: None,
                    score,
                }],
            }
        })
        .collect())
}

/// Propose candidates by trigram overlap, then rank them by real edit distance.
///
/// The trigram table makes substring matching indexable, which turns "find
/// something close to `פרידמאן`" from a full-table scan into a lookup.
///
/// Crucially this preserves the AND semantics of the query: a contact is only
/// proposed if *every* term matches it closely. Scoring each term independently
/// and unioning the results would make a two-word search return more rows than
/// the one-word search it refines, which is the opposite of what typing another
/// word is meant to do.
fn fuzzy_candidates(
    connection: &Connection,
    terms: &[String],
    exclude: &[String],
) -> Result<Vec<Candidate>> {
    let mut per_term: Vec<HashMap<String, (f64, String)>> = Vec::new();

    for term in terms {
        if term.chars().count() < 3 {
            // Too short to be selective; nothing meaningful is "close" to it.
            return Ok(Vec::new());
        }
        let grams = trigrams(term);
        if grams.is_empty() {
            return Ok(Vec::new());
        }

        let mut statement = connection.prepare(
            "SELECT t.contact_id, t.haystack
               FROM contact_trigram t JOIN contacts c ON c.id = t.contact_id
              WHERE c.deleted_at IS NULL AND t.haystack LIKE '%' || ?1 || '%'
              LIMIT 200",
        )?;

        // A three-character seed keeps the LIKE selective; the trigram index is
        // what makes the substring scan affordable.
        let seed: String = term.chars().take(3).collect();
        let rows = statement.query_map(params![seed], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;

        let needed = min_shared_trigrams(grams.len());
        let mut matches = HashMap::new();

        for row in rows {
            let (contact_id, haystack) = row?;
            if exclude.contains(&contact_id) {
                continue;
            }

            let best = haystack
                .split_whitespace()
                .map(|word| similarity(word, term))
                .fold(0.0_f64, f64::max);

            let shared = trigrams(&haystack).iter().filter(|gram| grams.contains(gram)).count();

            if best >= FUZZY_MIN_SIMILARITY && shared >= needed {
                matches.insert(contact_id, (best, term.clone()));
            }
        }

        if matches.is_empty() {
            // One term nobody comes close to means the conjunction is empty.
            return Ok(Vec::new());
        }
        per_term.push(matches);
    }

    let Some((first, rest)) = per_term.split_first() else {
        return Ok(Vec::new());
    };

    let mut candidates = Vec::new();
    for (contact_id, (score, term)) in first {
        if !rest.iter().all(|matches| matches.contains_key(contact_id)) {
            continue;
        }

        // The weakest term governs: a result is only as good as its worst match.
        let weakest = rest
            .iter()
            .filter_map(|matches| matches.get(contact_id).map(|(s, _)| *s))
            .fold(*score, f64::min);

        let factor = yanuka_search::fuzzy_quality_factor(weakest);
        let total = score_match(MatchSource::Name, MatchQuality::Fuzzy, 1.0) * (factor / 0.45);

        candidates.push(Candidate {
            contact_id: contact_id.clone(),
            score: total,
            reasons: vec![MatchReason {
                source: "name".into(),
                quality: "fuzzy".into(),
                term: term.clone(),
                snippet: None,
                score: total,
            }],
        });
    }

    Ok(candidates)
}

/// No query: the home screen still wants something to show.
fn browse_candidates(connection: &Connection) -> Result<Vec<Candidate>> {
    let mut statement = connection.prepare(
        "SELECT id, is_favorite FROM contacts WHERE deleted_at IS NULL
          ORDER BY is_favorite DESC, last_viewed_at DESC, display_name LIMIT ?1",
    )?;
    let rows = statement.query_map(params![CANDIDATE_LIMIT], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;

    Ok(rows
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .map(|(contact_id, favorite)| Candidate {
            contact_id,
            score: if favorite != 0 { 1.0 } else { 0.0 },
            reasons: Vec::new(),
        })
        .collect())
}

fn passes_filters(
    connection: &Connection,
    candidate: &Candidate,
    query: &SearchQuery,
) -> Result<bool> {
    if query.favorites_only {
        let favorite: i64 = connection.query_row(
            "SELECT is_favorite FROM contacts WHERE id = ?1",
            params![candidate.contact_id],
            |row| row.get(0),
        )?;
        if favorite == 0 {
            return Ok(false);
        }
    }

    for (field, values) in &query.filters {
        if values.is_empty() {
            continue;
        }
        let actual = facet_values_for(connection, &candidate.contact_id, field)?;
        // Values within a field are OR-ed; different fields are AND-ed.
        if !values.iter().any(|value| actual.contains(value)) {
            return Ok(false);
        }
    }

    Ok(true)
}

fn facet_values_for(connection: &Connection, contact_id: &str, field: &str) -> Result<Vec<String>> {
    let sql = match field {
        "country" => "SELECT country FROM contacts WHERE id = ?1 AND country IS NOT NULL",
        "city" => "SELECT city FROM contacts WHERE id = ?1 AND city IS NOT NULL",
        "profession" => "SELECT profession FROM contacts WHERE id = ?1 AND profession IS NOT NULL",
        "specialty" => {
            "SELECT value FROM contact_specialties WHERE contact_id = ?1 AND deleted_at IS NULL"
        }
        "tag" => {
            "SELECT t.name FROM contact_tags ct JOIN tags t ON t.id = ct.tag_id
              WHERE ct.contact_id = ?1 AND ct.deleted_at IS NULL"
        }
        "category" => {
            "SELECT cat.name FROM category_members cm JOIN categories cat ON cat.id = cm.category_id
              WHERE cm.contact_id = ?1"
        }
        "organization" => {
            "SELECT o.name FROM contact_organizations co JOIN organizations o ON o.id = co.organization_id
              WHERE co.contact_id = ?1 AND co.deleted_at IS NULL"
        }
        "language" => {
            "SELECT language_code FROM contact_languages WHERE contact_id = ?1 AND deleted_at IS NULL"
        }
        _ => return Ok(Vec::new()),
    };

    let mut statement = connection.prepare(sql)?;
    let rows = statement.query_map(params![contact_id], |row| row.get::<_, String>(0))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

const FACET_FIELDS: [&str; 8] =
    ["country", "city", "profession", "specialty", "tag", "category", "organization", "language"];

/// Count facet values across the matched set.
///
/// Counts are computed *after* the active filters are applied, so selecting a
/// country shows the other countries at zero. Proper per-dimension exclusion
/// needs one pass per facet; the simplification is documented in
/// docs/SEARCH.md and the UI compensates by keeping selected chips visible.
fn compute_facets(
    connection: &Connection,
    candidates: &[Candidate],
) -> Result<HashMap<String, Vec<FacetValue>>> {
    let mut counters: HashMap<String, HashMap<String, i64>> = HashMap::new();

    for candidate in candidates {
        for field in FACET_FIELDS {
            for value in facet_values_for(connection, &candidate.contact_id, field)? {
                *counters.entry(field.to_string()).or_default().entry(value).or_insert(0) += 1;
            }
        }
    }

    let mut facets = HashMap::new();
    for (field, counts) in counters {
        let mut values: Vec<FacetValue> = counts
            .into_iter()
            .map(|(value, count)| FacetValue { label: value.clone(), value, count })
            .collect();
        values.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.label.cmp(&b.label)));
        values.truncate(12);
        if !values.is_empty() {
            facets.insert(field, values);
        }
    }

    Ok(facets)
}

/// Proclitic-stripped variants score lower than a hit on the literal query.
pub fn variant_penalty() -> f64 {
    PROCLITIC_PENALTY
}
