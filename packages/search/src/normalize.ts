import { collapseWhitespace, stripDiacritics, tokenize } from '@yanuka/utils';

/**
 * Hebrew-aware text normalization.
 *
 * The database records the same person as `ר' משה כהן`, `הרב משה כהן`,
 * `רבי משה` and `Moshe Cohen`. Normalization is what collapses those into a
 * single comparable key. Every `normalized_*` column in the schema is produced
 * here, and every query goes through the same pipeline, so the two sides can
 * never disagree about what a string "is".
 *
 * See docs/SEARCH.md for the rationale behind each step.
 */

/** U+05F3 HEBREW PUNCTUATION GERESH and the ASCII/curly apostrophes. */
const GERESH_CHARS = /[׳'‘’ʼ]/g;
/** U+05F4 HEBREW PUNCTUATION GERSHAYIM and the ASCII/curly double quotes. */
const GERSHAYIM_CHARS = /[״"“”]/g;
/** U+05BE HEBREW MAQAF plus the usual dash family. */
const DASH_CHARS = /[־‐-―\-–—_]/g;

/**
 * Final letter forms mapped to their medial equivalents.
 *
 * `סת"ם` and `סתם`, `בן` and `בנו` share a stem only once the final form is
 * folded. Users are also inconsistent about typing them mid-word.
 */
const FINAL_LETTERS: Record<string, string> = {
  'ך': 'כ',
  'ם': 'מ',
  'ן': 'נ',
  'ף': 'פ',
  'ץ': 'צ',
};

/**
 * Honorifics and titles stripped from names before comparison.
 *
 * These are prefixes, not part of the identity: a user who searches `משה כהן`
 * must find `הרב משה כהן`, and vice versa. Ordered longest-first so that
 * `הרב` is consumed before the bare `ר`.
 *
 * Written here in already-normalized form (no geresh, no gershayim, no final
 * letters) because stripping runs after `normalizeText`.
 */
const HONORIFIC_PREFIXES = [
  'הרהג',
  'האדמור',
  'הגאונ',
  'הרהח',
  'מוהר',
  'הרהצ',
  'האדמו',
  'הרב',
  'רבי',
  'הרר',
  'הגר',
  'פרופ',
  'עוד',
  'דר',
  'רב',
  'מר',
  'גב',
  'ר',
  'rabbi',
  'rav',
  'reb',
  'dr',
  'prof',
  'mr',
  'mrs',
  'ms',
].sort((a, b) => b.length - a.length);

/**
 * Single-letter Hebrew proclitics: definite article, conjunction and the
 * inseparable prepositions.
 *
 * These are never stripped destructively — `משה` would become `שה`. They are
 * used to generate *additional* search variants, so `מלונדון` can also match
 * `לונדון` without `משה` losing its own spelling. See `expandToken`.
 */
const PROCLITIC_LETTERS = new Set(['ה', 'ו', 'ב', 'ל', 'מ', 'כ', 'ש']);

/** Common two-letter proclitic clusters, e.g. `וב`ירושלים, `מה`ישיבה. */
const PROCLITIC_PAIRS = new Set(['וה', 'וב', 'ול', 'ומ', 'וכ', 'ומה', 'מה', 'לכ', 'כש', 'שה', 'שב']);

/**
 * Shortest token left after stripping a proclitic.
 *
 * Four is chosen from the false positives at three: `מלון` → `לון`,
 * `שלום` → `לום`, `בית` → `ית`, `לוי` → `וי`. Requiring a four-letter stem
 * keeps the cases that matter — `מלונדון` → `לונדון`, `בירושלים` → `ירושלים` —
 * while leaving every common short word intact. The negative list is asserted
 * in normalize.test.ts so this bound cannot be loosened by accident.
 */
const MIN_STEM_LENGTH = 4;

/**
 * The general normalization pipeline, applied to every indexed field and to
 * every query.
 *
 * Order matters: diacritics are removed before punctuation so that a niqqud
 * mark sitting under a letter adjacent to a geresh does not block the match.
 */
export function normalizeText(input: string | null | undefined): string {
  if (!input) return '';

  let value = input.normalize('NFC');
  value = stripDiacritics(value); // niqqud, te'amim, Latin accents
  value = value.replace(GERESH_CHARS, ''); // ר' → ר
  value = value.replace(GERSHAYIM_CHARS, ''); // סת"ם → סתם
  value = value.replace(DASH_CHARS, ' '); // בן־גוריון → בן גוריון
  value = value.replace(/[.,;:!?()[\]{}<>/\\|@#$%^&*+=~`]/g, ' ');
  value = value.toLocaleLowerCase('en-US'); // folds Latin, no-op for Hebrew
  value = value.replace(/[ךםןףץ]/g, (ch) => FINAL_LETTERS[ch] ?? ch);

  return collapseWhitespace(value);
}

/**
 * Normalize a personal name and drop leading honorifics.
 *
 * Applied to `contacts.normalized_name` and to alias values, so that the
 * honorific survives for display in `contacts.prefix` while never interfering
 * with matching.
 */
export function normalizeName(input: string | null | undefined): string {
  return stripHonorifics(normalizeText(input));
}

/** Remove any run of leading honorifics from already-normalized text. */
export function stripHonorifics(normalized: string): string {
  let tokens = normalized.split(' ').filter(Boolean);

  // A name may stack several: `הרב הגאון רבי משה`.
  let changed = true;
  while (changed && tokens.length > 1) {
    changed = false;
    const head = tokens[0]!;
    if (HONORIFIC_PREFIXES.includes(head)) {
      tokens = tokens.slice(1);
      changed = true;
    }
  }

  return tokens.join(' ');
}

/**
 * Search variants for a single query token.
 *
 * Query-side only. The index stores the original spelling and nothing else:
 * writing stripped forms into the index would make every document match more
 * broadly with no way to discount it afterwards. Expanding the query instead
 * keeps the stripped form attributable, so a hit on a variant can be scored
 * lower than a hit on what the user actually typed (see `PROCLITIC_PENALTY`).
 *
 * The first element is always the original token.
 */
export function expandToken(token: string): string[] {
  const variants = new Set<string>([token]);

  if (token.length >= MIN_STEM_LENGTH + 2) {
    const pair = token.slice(0, 2);
    if (PROCLITIC_PAIRS.has(pair)) {
      variants.add(token.slice(2));
    }
  }

  if (token.length >= MIN_STEM_LENGTH + 1) {
    const first = token[0]!;
    if (PROCLITIC_LETTERS.has(first)) {
      variants.add(token.slice(1));
    }
  }

  return [...variants];
}

/**
 * Score multiplier for a hit that only matched after a proclitic was stripped.
 * A match on the literal query always outranks a match on a grammatical guess.
 */
export const PROCLITIC_PENALTY = 0.5;

/** Normalize, tokenize and expand — the full query-term pipeline. */
export function normalizeAndExpand(input: string | null | undefined): string[] {
  const normalized = normalizeText(input);
  if (!normalized) return [];
  const expanded = new Set<string>();
  for (const token of tokenize(normalized)) {
    for (const variant of expandToken(token)) {
      expanded.add(variant);
    }
  }
  return [...expanded];
}

// ---------------------------------------------------------------------------
// Phonetic keys
// ---------------------------------------------------------------------------

/**
 * Hebrew matres lectionis. `פרידמן` and `פרידמאן`, `דויד` and `דוד` differ only
 * by these optional vowel letters, so a key with them removed matches across
 * both spellings.
 *
 * Word-initial and word-final positions are preserved: dropping them there
 * changes the consonantal skeleton rather than a spelling choice.
 */
const HEBREW_MATRES = new Set(['א', 'ה', 'ו', 'י', 'ע']);

/** Hebrew letters that collapse together because they sound alike. */
const HEBREW_SOUND_GROUPS: Record<string, string> = {
  'ט': 'ת',
  'כ': 'ק',
  'ח': 'כ',
  'ס': 'ש',
  'ב': 'ב',
  'צ': 'צ',
};

/**
 * Consonantal skeleton of a Hebrew word, used as a fuzzy-match key.
 *
 * `פרידמן` → `פרדמן`, `פרידמאן` → `פרדמן`. Deliberately lossy: it is only ever
 * used to *retrieve candidates*, which are then ranked by real edit distance.
 */
export function hebrewPhoneticKey(word: string): string {
  if (word.length <= 2) return word;

  const chars = [...word];
  const out: string[] = [chars[0]!];

  for (let i = 1; i < chars.length - 1; i += 1) {
    const ch = chars[i]!;
    if (HEBREW_MATRES.has(ch)) continue;
    out.push(HEBREW_SOUND_GROUPS[ch] ?? ch);
  }

  const last = chars[chars.length - 1]!;
  out.push(HEBREW_SOUND_GROUPS[last] ?? last);

  // Collapse doubled letters left behind by the removals (`וו` → `ו`).
  return out.join('').replace(/(.)\1+/g, '$1');
}

/** Latin digraphs folded before vowel removal. Order matters — longest first. */
const LATIN_DIGRAPHS: Array<[RegExp, string]> = [
  [/tsch|tzsch/g, 'z'],
  [/sch/g, 's'],
  [/tz|ts/g, 'z'],
  [/ph/g, 'f'],
  [/ck/g, 'k'],
  [/kh|ch/g, 'k'],
  [/sh/g, 's'],
  [/th/g, 't'],
  [/gh/g, 'g'],
  [/wh/g, 'w'],
  [/qu/g, 'k'],
];

/**
 * Phonetic key for Latin-script names.
 *
 * `Friedman`, `Freidman` and `Fridman` all reduce to `frdmn`; `Moshe` and
 * `Moishe` to `ms`. Same contract as the Hebrew key: candidate retrieval only.
 */
export function latinPhoneticKey(word: string): string {
  if (!word) return '';
  let value = word.toLowerCase();

  for (const [pattern, replacement] of LATIN_DIGRAPHS) {
    value = value.replace(pattern, replacement);
  }

  value = value.replace(/[cq]/g, 'k').replace(/[vw]/g, 'v').replace(/[yj]/g, 'i');

  // Keep a leading vowel — it carries real distinguishing information — and
  // drop the rest, which is where transliteration variance lives.
  const head = /^[aeiou]/.test(value) ? value[0]! : '';
  const body = value.slice(head ? 1 : 0).replace(/[aeiou]/g, '');

  return (head + body).replace(/(.)\1+/g, '$1');
}

const HEBREW_RANGE = /[֐-׿]/;

/** Dispatch to the Hebrew or Latin key based on the script of the word. */
export function phoneticKey(word: string): string {
  const normalized = normalizeText(word);
  if (!normalized) return '';
  return HEBREW_RANGE.test(normalized)
    ? hebrewPhoneticKey(normalized)
    : latinPhoneticKey(normalized);
}

/** Space-joined phonetic keys for every token in a phrase. */
export function phoneticKeys(text: string | null | undefined): string[] {
  return tokenize(normalizeText(text)).map(phoneticKey).filter(Boolean);
}
