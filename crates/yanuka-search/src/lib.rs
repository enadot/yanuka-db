//! Hebrew-aware search primitives for the contacts database.
//!
//! Deliberately free of any database or Tauri dependency so it builds and tests
//! on any machine, including CI runners without a system webview.

pub mod normalize;
pub mod scoring;

pub use normalize::{
    expand_token, hebrew_phonetic_key, latin_phonetic_key, normalize_name, normalize_text,
    phonetic_key, strip_honorifics, tokenize, PROCLITIC_PENALTY,
};
pub use scoring::{
    bm25_to_quality_factor, edit_distance, field_weight, fuzzy_quality_factor, min_shared_trigrams,
    quality_multiplier, score_match, similarity, trigrams, MatchQuality, MatchSource,
    FUZZY_MAX_DISTANCE, FUZZY_MIN_SIMILARITY,
};
