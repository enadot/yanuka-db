//! Cross-implementation conformance.
//!
//! Reads the same fixture the TypeScript test suite reads. If this file and
//! `packages/search/src/normalize.conformance.test.ts` both pass, the two
//! normalizers agree — which is the only thing standing between a contact
//! indexed by the desktop and the same contact being findable from the web.

use serde::Deserialize;
use yanuka_search::{expand_token, normalize_name, normalize_text, phonetic_key};

const FIXTURE: &str = include_str!("../../../packages/search/fixtures/normalization.cases.json");

#[derive(Deserialize)]
struct TextCase {
    input: String,
    expected: String,
    why: String,
}

#[derive(Deserialize)]
struct ExpandCase {
    input: String,
    expected: Vec<String>,
    why: String,
}

#[derive(Deserialize)]
struct PhoneticCase {
    a: String,
    b: String,
    equal: bool,
    why: String,
}

#[derive(Deserialize)]
struct Fixture {
    #[serde(rename = "normalizeText")]
    normalize_text: Vec<TextCase>,
    #[serde(rename = "normalizeName")]
    normalize_name: Vec<TextCase>,
    #[serde(rename = "expandToken")]
    expand_token: Vec<ExpandCase>,
    #[serde(rename = "phoneticKey")]
    phonetic_key: Vec<PhoneticCase>,
}

fn fixture() -> Fixture {
    serde_json::from_str(FIXTURE).expect("normalization fixture must parse")
}

#[test]
fn normalize_text_matches_the_shared_fixture() {
    for case in fixture().normalize_text {
        assert_eq!(
            normalize_text(&case.input),
            case.expected,
            "normalize_text({:?}) — {}",
            case.input,
            case.why
        );
    }
}

#[test]
fn normalize_name_matches_the_shared_fixture() {
    for case in fixture().normalize_name {
        assert_eq!(
            normalize_name(&case.input),
            case.expected,
            "normalize_name({:?}) — {}",
            case.input,
            case.why
        );
    }
}

#[test]
fn expand_token_matches_the_shared_fixture() {
    for case in fixture().expand_token {
        assert_eq!(
            expand_token(&case.input),
            case.expected,
            "expand_token({:?}) — {}",
            case.input,
            case.why
        );
    }
}

#[test]
fn phonetic_keys_match_the_shared_fixture() {
    for case in fixture().phonetic_key {
        let key_a = phonetic_key(&case.a);
        let key_b = phonetic_key(&case.b);
        assert_eq!(
            key_a == key_b,
            case.equal,
            "phonetic_key({:?})={:?} vs phonetic_key({:?})={:?} — {}",
            case.a,
            key_a,
            case.b,
            key_b,
            case.why
        );
    }
}

#[test]
fn every_honorific_variant_of_one_person_converges() {
    let forms = ["הרב משה כהן", "ר' משה כהן", "רבי משה כהן", "משה כהן", "הרב מֹשֶׁה כֹּהֵן"];
    let normalized: Vec<String> = forms.iter().map(|f| normalize_name(f)).collect();
    assert!(
        normalized.windows(2).all(|w| w[0] == w[1]),
        "all honorific spellings must normalize alike, got {normalized:?}"
    );
}

#[test]
fn normalization_is_idempotent() {
    // A value read back out of the database and re-normalized must not change,
    // or reindexing would silently alter what is searchable.
    for case in fixture().normalize_text {
        let once = normalize_text(&case.input);
        assert_eq!(normalize_text(&once), once, "not idempotent for {:?}", case.input);
    }
}
