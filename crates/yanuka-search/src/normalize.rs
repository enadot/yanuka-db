//! Hebrew-aware text normalization.
//!
//! This is a deliberate mirror of `packages/search/src/normalize.ts`. The two
//! implementations must agree exactly: the desktop indexes with this one and a
//! future web client queries with the other, and a contact indexed under one
//! spelling would be unreachable under the other if they diverged.
//!
//! The contract between them is
//! `packages/search/fixtures/normalization.cases.json`, which both test suites
//! read. See `tests/conformance.rs` and docs/SEARCH.md.

use unicode_normalization::UnicodeNormalization;

/// Score multiplier for a hit that only matched after a proclitic was stripped.
pub const PROCLITIC_PENALTY: f64 = 0.5;

/// Shortest token left after stripping a proclitic.
///
/// Four, because at three the strip destroys ordinary words: `מלון` → `לון`,
/// `שלום` → `לום`, `בית` → `ית`, `לוי` → `וי`.
const MIN_STEM_LENGTH: usize = 4;

/// Hebrew proclitics: definite article, conjunction, inseparable prepositions.
const PROCLITIC_LETTERS: [char; 7] = ['ה', 'ו', 'ב', 'ל', 'מ', 'כ', 'ש'];

/// Common two-letter proclitic clusters.
const PROCLITIC_PAIRS: [&str; 11] =
    ["וה", "וב", "ול", "ומ", "וכ", "ומה", "מה", "לכ", "כש", "שה", "שב"];

/// Honorifics stripped from a name before comparison, longest first so that
/// `הרב` is consumed before the bare `ר`. Written already-normalized.
const HONORIFIC_PREFIXES: [&str; 22] = [
    "האדמור",
    "הגאונ",
    "הרהג",
    "הרהח",
    "הרהצ",
    "מוהר",
    "האדמו",
    "הרב",
    "רבי",
    "הרר",
    "הגר",
    "פרופ",
    "עוד",
    "דר",
    "רב",
    "מר",
    "גב",
    "ר",
    "rabbi",
    "rav",
    "reb",
    "prof",
];

/// Extra Latin honorifics that would otherwise be shadowed by prefix matching.
const HONORIFIC_LATIN: [&str; 4] = ["dr", "mr", "mrs", "ms"];

fn is_combining_mark(c: char) -> bool {
    // Hebrew niqqud and te'amim occupy U+0591-U+05C7; the generic ranges cover
    // Latin accents and everything else NFD decomposition produces.
    matches!(c as u32,
        0x0300..=0x036F | 0x0483..=0x0489 | 0x0591..=0x05BD | 0x05BF | 0x05C1..=0x05C2
        | 0x05C4..=0x05C5 | 0x05C7 | 0x0610..=0x061A | 0x064B..=0x065F | 0x0670
        | 0x1AB0..=0x1AFF | 0x1DC0..=0x1DFF | 0x20D0..=0x20F0 | 0xFE20..=0xFE2F)
}

fn fold_final_letter(c: char) -> char {
    match c {
        'ך' => 'כ',
        'ם' => 'מ',
        'ן' => 'נ',
        'ף' => 'פ',
        'ץ' => 'צ',
        other => other,
    }
}

fn is_geresh(c: char) -> bool {
    matches!(c, '\u{05F3}' | '\'' | '\u{2018}' | '\u{2019}' | '\u{02BC}')
}

fn is_gershayim(c: char) -> bool {
    matches!(c, '\u{05F4}' | '"' | '\u{201C}' | '\u{201D}')
}

fn is_dash(c: char) -> bool {
    // U+05BE HEBREW MAQAF, the U+2010-U+2015 dash block (which already contains
    // en and em dash), ASCII hyphen and underscore.
    matches!(c, '\u{05BE}' | '\u{2010}'..='\u{2015}' | '-' | '_')
}

fn is_punctuation(c: char) -> bool {
    matches!(
        c,
        '.' | ','
            | ';'
            | ':'
            | '!'
            | '?'
            | '('
            | ')'
            | '['
            | ']'
            | '{'
            | '}'
            | '<'
            | '>'
            | '/'
            | '\\'
            | '|'
            | '@'
            | '#'
            | '$'
            | '%'
            | '^'
            | '&'
            | '*'
            | '+'
            | '='
            | '~'
            | '`'
    )
}

/// Bidirectional controls and other invisible characters that ride along with
/// text copied out of an RTL document.
fn is_invisible(c: char) -> bool {
    matches!(c as u32,
        0x200B..=0x200F | 0x202A..=0x202E | 0x2066..=0x2069 | 0xFEFF)
}

/// The general normalization pipeline, applied to every indexed field and to
/// every query. Mirrors `normalizeText` in the TypeScript implementation.
pub fn normalize_text(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut pending_space = false;

    for c in input.nfd() {
        if is_combining_mark(c) || is_invisible(c) {
            continue;
        }
        if is_geresh(c) || is_gershayim(c) {
            // Removed outright, not turned into a space: `סת"ם` is one word.
            continue;
        }
        if c.is_whitespace() || is_dash(c) || is_punctuation(c) {
            pending_space = !out.is_empty();
            continue;
        }

        if pending_space {
            out.push(' ');
            pending_space = false;
        }

        for lowered in c.to_lowercase() {
            out.push(fold_final_letter(lowered));
        }
    }

    // Recompose so the output is comparable byte-for-byte with the JS side,
    // which returns NFC.
    out.nfc().collect()
}

/// Normalize a personal name and drop any leading honorifics.
pub fn normalize_name(input: &str) -> String {
    strip_honorifics(&normalize_text(input))
}

/// Remove leading honorifics from already-normalized text.
///
/// Never consumes the final token: `הרב` on its own is somebody's whole name as
/// recorded, and returning an empty string would make the record unsearchable.
pub fn strip_honorifics(normalized: &str) -> String {
    let mut tokens: Vec<&str> = normalized.split(' ').filter(|t| !t.is_empty()).collect();

    loop {
        if tokens.len() <= 1 {
            break;
        }
        let head = tokens[0];
        let is_honorific = HONORIFIC_PREFIXES.contains(&head) || HONORIFIC_LATIN.contains(&head);
        if is_honorific {
            tokens.remove(0);
        } else {
            break;
        }
    }

    tokens.join(" ")
}

/// Search variants for a single query token.
///
/// Query-side only; the index stores the original spelling. The first element
/// is always the token itself.
pub fn expand_token(token: &str) -> Vec<String> {
    let mut variants = vec![token.to_string()];
    let chars: Vec<char> = token.chars().collect();

    if chars.len() >= MIN_STEM_LENGTH + 2 {
        let pair: String = chars[..2].iter().collect();
        if PROCLITIC_PAIRS.contains(&pair.as_str()) {
            let stem: String = chars[2..].iter().collect();
            if !variants.contains(&stem) {
                variants.push(stem);
            }
        }
    }

    // Expressed as "what is left after stripping one letter", which is the
    // condition that actually matters — see MIN_STEM_LENGTH.
    if chars.len() > MIN_STEM_LENGTH && PROCLITIC_LETTERS.contains(&chars[0]) {
        let stem: String = chars[1..].iter().collect();
        if !variants.contains(&stem) {
            variants.push(stem);
        }
    }

    variants
}

/// Split normalized text into tokens on any non-alphanumeric character.
pub fn tokenize(normalized: &str) -> Vec<String> {
    normalized
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(|t| t.to_string())
        .collect()
}

// ---------------------------------------------------------------------------
// Phonetic keys
// ---------------------------------------------------------------------------

const HEBREW_MATRES: [char; 5] = ['א', 'ה', 'ו', 'י', 'ע'];

fn hebrew_sound_group(c: char) -> char {
    match c {
        'ט' => 'ת',
        'כ' => 'ק',
        'ח' => 'כ',
        'ס' => 'ש',
        other => other,
    }
}

fn collapse_doubles(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut previous: Option<char> = None;
    for c in value.chars() {
        if Some(c) != previous {
            out.push(c);
        }
        previous = Some(c);
    }
    out
}

/// Consonantal skeleton of a Hebrew word, used only to retrieve fuzzy
/// candidates — never as a match on its own. `פרידמן` and `פרידמאן` collapse
/// to the same key.
pub fn hebrew_phonetic_key(word: &str) -> String {
    let chars: Vec<char> = word.chars().collect();
    if chars.len() <= 2 {
        return word.to_string();
    }

    let mut out = String::new();
    out.push(chars[0]);

    for &c in &chars[1..chars.len() - 1] {
        if HEBREW_MATRES.contains(&c) {
            continue;
        }
        out.push(hebrew_sound_group(c));
    }

    out.push(hebrew_sound_group(chars[chars.len() - 1]));
    collapse_doubles(&out)
}

/// Phonetic key for Latin-script names. `Friedman`, `Freidman` and `Fridman`
/// all reduce to the same skeleton.
pub fn latin_phonetic_key(word: &str) -> String {
    if word.is_empty() {
        return String::new();
    }

    let mut value = word.to_lowercase();
    // Longest digraphs first — `tsch` must not be eaten by `sch`.
    for (from, to) in [
        ("tzsch", "z"),
        ("tsch", "z"),
        ("sch", "s"),
        ("tz", "z"),
        ("ts", "z"),
        ("ph", "f"),
        ("ck", "k"),
        ("kh", "k"),
        ("ch", "k"),
        ("sh", "s"),
        ("th", "t"),
        ("gh", "g"),
        ("wh", "w"),
        ("qu", "k"),
    ] {
        value = value.replace(from, to);
    }

    value = value
        .chars()
        .map(|c| match c {
            'c' | 'q' => 'k',
            'v' | 'w' => 'v',
            'y' | 'j' => 'i',
            other => other,
        })
        .collect();

    let is_vowel = |c: char| matches!(c, 'a' | 'e' | 'i' | 'o' | 'u');
    let mut chars = value.chars();
    let first = chars.next().unwrap_or('\0');

    // A leading vowel carries real information; interior vowels are where
    // transliteration variance lives, so they go.
    let (head, rest): (String, String) = if is_vowel(first) {
        (first.to_string(), chars.collect())
    } else {
        (String::new(), value.clone())
    };

    let body: String = rest.chars().filter(|&c| !is_vowel(c)).collect();
    collapse_doubles(&format!("{head}{body}"))
}

fn is_hebrew(value: &str) -> bool {
    value.chars().any(|c| matches!(c as u32, 0x0590..=0x05FF | 0xFB1D..=0xFB4F))
}

/// Dispatch to the Hebrew or Latin phonetic key based on the script.
pub fn phonetic_key(word: &str) -> String {
    let normalized = normalize_text(word);
    if normalized.is_empty() {
        return String::new();
    }
    if is_hebrew(&normalized) {
        hebrew_phonetic_key(&normalized)
    } else {
        latin_phonetic_key(&normalized)
    }
}
