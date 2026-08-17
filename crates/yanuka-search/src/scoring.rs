//! Ranking weights, mirrored from `packages/search/src/scoring.ts`.
//!
//! The conformance tests assert result *ordering* rather than absolute scores:
//! two implementations agreeing on the order is what users experience, and
//! comparing floats across languages is brittle for no benefit.

use serde::{Deserialize, Serialize};

/// Which field produced a match.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchSource {
    Name,
    Phone,
    Alias,
    Email,
    Profession,
    Role,
    Tag,
    Category,
    Organization,
    Specialty,
    City,
    Country,
    Notes,
    ReasonForSaving,
}

/// How well the term matched, which sets the multiplier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchQuality {
    Exact,
    Prefix,
    Fulltext,
    Fuzzy,
}

/// Field weights.
///
/// A name or a phone number identifies a person; a city merely narrows them
/// down. Notes score lowest per hit because they are long and therefore easy to
/// match by accident — but they are never excluded, because they are what makes
/// this archive worth keeping.
pub fn field_weight(source: MatchSource) -> f64 {
    match source {
        MatchSource::Name | MatchSource::Phone => 100.0,
        MatchSource::Alias => 90.0,
        MatchSource::Email => 60.0,
        MatchSource::Profession => 50.0,
        MatchSource::Tag | MatchSource::Category => 40.0,
        MatchSource::Organization | MatchSource::Specialty => 35.0,
        MatchSource::City | MatchSource::Role => 30.0,
        MatchSource::ReasonForSaving => 25.0,
        MatchSource::Country => 20.0,
        MatchSource::Notes => 20.0,
    }
}

pub fn quality_multiplier(quality: MatchQuality) -> f64 {
    match quality {
        MatchQuality::Exact => 1.0,
        MatchQuality::Prefix => 0.8,
        MatchQuality::Fulltext => 0.55,
        MatchQuality::Fuzzy => 0.45,
    }
}

/// Score for one match, before per-contact bonuses.
pub fn score_match(source: MatchSource, quality: MatchQuality, penalty: f64) -> f64 {
    field_weight(source) * quality_multiplier(quality) * penalty
}

pub const BONUS_HAS_PHONE: f64 = 3.0;
pub const BONUS_HAS_ORGANIZATION: f64 = 2.0;
pub const BONUS_FAVORITE: f64 = 12.0;
pub const BONUS_RECENTLY_VIEWED: f64 = 8.0;

/// Minimum similarity for a fuzzy candidate to be shown at all.
pub const FUZZY_MIN_SIMILARITY: f64 = 0.72;

/// Maximum edit distance considered, regardless of term length.
pub const FUZZY_MAX_DISTANCE: usize = 2;

/// Map an FTS5 bm25 rank onto a `Fulltext` quality factor.
///
/// bm25 values are comparable only within one query, so they order results
/// *within* the full-text layer and are never compared against the exact or
/// fuzzy layers.
pub fn bm25_to_quality_factor(rank: f64, best_rank: f64, worst_rank: f64) -> f64 {
    if !rank.is_finite() {
        return 0.5;
    }
    let span = worst_rank - best_rank;
    if span <= 0.0 {
        return 1.0;
    }
    let position = (worst_rank - rank) / span;
    0.6 + 0.4 * position
}

/// Quality factor for a fuzzy hit, scaled by how close the match was.
pub fn fuzzy_quality_factor(similarity: f64) -> f64 {
    if similarity >= 1.0 {
        return 1.0;
    }
    if similarity < FUZZY_MIN_SIMILARITY {
        return 0.0;
    }
    let range = 1.0 - FUZZY_MIN_SIMILARITY;
    0.35 + 0.35 * ((similarity - FUZZY_MIN_SIMILARITY) / range)
}

/// Damerau-Levenshtein distance with an early-exit ceiling.
///
/// Only ever run over candidates the trigram index already narrowed down, never
/// over the whole table.
pub fn edit_distance(a: &str, b: &str, max_distance: usize) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();

    if a == b {
        return 0;
    }
    if a.len().abs_diff(b.len()) > max_distance {
        return max_distance + 1;
    }
    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }

    let mut prev_prev: Vec<usize> = vec![0; b.len() + 1];
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut current: Vec<usize> = vec![0; b.len() + 1];

    for i in 1..=a.len() {
        current[0] = i;
        let mut row_min = current[0];

        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            let mut value = (current[j - 1] + 1).min(prev[j] + 1).min(prev[j - 1] + cost);

            // Transposition: `ab` -> `ba` costs one, not two. This is what makes
            // it Damerau rather than plain Levenshtein, and it matters for typos.
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                value = value.min(prev_prev[j - 2] + cost);
            }

            current[j] = value;
            row_min = row_min.min(value);
        }

        if row_min > max_distance {
            return max_distance + 1;
        }

        std::mem::swap(&mut prev_prev, &mut prev);
        std::mem::swap(&mut prev, &mut current);
    }

    prev[b.len()]
}

/// Similarity in [0, 1], normalized by the longer string so a one-character
/// typo costs more in a short word than in a long one.
pub fn similarity(a: &str, b: &str) -> f64 {
    let longest = a.chars().count().max(b.chars().count());
    if longest == 0 {
        return 1.0;
    }
    let distance = edit_distance(a, b, longest);
    1.0 - (distance as f64 / longest as f64)
}

/// Character trigrams, padded so short strings still produce grams.
pub fn trigrams(value: &str) -> Vec<String> {
    if value.is_empty() {
        return Vec::new();
    }
    let padded: Vec<char> = format!(" {value} ").chars().collect();
    padded.windows(3).map(|w| w.iter().collect()).collect()
}

/// How many trigrams a candidate must share before it is worth scoring.
/// Forty percent overlap tolerates roughly one edit per five characters.
pub fn min_shared_trigrams(term_trigram_count: usize) -> usize {
    ((term_trigram_count as f64 * 0.4).ceil() as usize).max(1)
}
