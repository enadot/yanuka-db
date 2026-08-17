import { looksLikePhoneQuery, phoneSearchKey, tokenize } from '@yanuka/utils';
import { expandToken, normalizeText, phoneticKey, stripHonorifics } from './normalize.js';

/**
 * Field-scoped filters the user can type inline, e.g. `עיר:לונדון סופר סתם`.
 * Keys are accepted in Hebrew and English so neither keyboard layout is a
 * second-class citizen.
 */
const FIELD_ALIASES: Record<string, ParsedQuery['fields'][number]['field']> = {
  city: 'city',
  עיר: 'city',
  country: 'country',
  מדינה: 'country',
  tag: 'tag',
  תגית: 'tag',
  profession: 'profession',
  מקצוע: 'profession',
  org: 'organization',
  organization: 'organization',
  מוסד: 'organization',
  ארגון: 'organization',
  category: 'category',
  קטגוריה: 'category',
};

export interface ParsedTerm {
  /** The token as normalized, exactly as typed. */
  term: string;
  /** Proclitic-stripped variants; empty when none apply. */
  variants: string[];
  /** Fuzzy-retrieval key. */
  phonetic: string;
  /** Set when the user wrapped the term in quotes — no fuzzy, no expansion. */
  exact: boolean;
}

export interface ParsedQuery {
  /** Original input, untouched. */
  raw: string;
  /** Free-text terms after removing field filters and quoted phrases. */
  terms: ParsedTerm[];
  /** Quoted phrases, matched verbatim. */
  phrases: string[];
  /** Terms prefixed with `-`, excluded from results. */
  exclusions: string[];
  /** Inline `field:value` filters. */
  fields: Array<{
    field: 'city' | 'country' | 'tag' | 'profession' | 'organization' | 'category';
    value: string;
  }>;
  /** Set when the query is digits and long enough to be a phone lookup. */
  phone: { key: string; digits: string } | null;
  /** True when there is nothing to search for. */
  isEmpty: boolean;
}

const QUOTED = /"([^"]+)"|'([^']+)'/g;

/**
 * Parse a raw search box string into structured intent.
 *
 * Everything downstream — the FTS MATCH expression, the exact-match lookups,
 * the fuzzy candidate query — is derived from this, so the parse happens once
 * per keystroke rather than once per layer.
 */
export function parseQuery(raw: string): ParsedQuery {
  const input = (raw ?? '').trim();

  const empty: ParsedQuery = {
    raw: input,
    terms: [],
    phrases: [],
    exclusions: [],
    fields: [],
    phone: null,
    isEmpty: true,
  };
  if (!input) return empty;

  // A bare run of digits is a phone lookup and nothing else. Routing it through
  // the text pipeline would tokenize it into meaningless fragments.
  if (looksLikePhoneQuery(input)) {
    return {
      ...empty,
      isEmpty: false,
      phone: { key: phoneSearchKey(input), digits: input.replace(/\D+/g, '') },
    };
  }

  // Pull quoted phrases out first so their internal spaces survive tokenizing.
  const phrases: string[] = [];
  const withoutPhrases = input.replace(QUOTED, (_match, dq?: string, sq?: string) => {
    const phrase = normalizeText(dq ?? sq ?? '');
    if (phrase) phrases.push(phrase);
    return ' ';
  });

  const fields: ParsedQuery['fields'] = [];
  const exclusions: string[] = [];
  const freeText: string[] = [];

  for (const chunk of withoutPhrases.split(/\s+/)) {
    if (!chunk) continue;

    if (chunk.startsWith('-') && chunk.length > 1) {
      const term = normalizeText(chunk.slice(1));
      if (term) exclusions.push(term);
      continue;
    }

    const colon = chunk.indexOf(':');
    if (colon > 0) {
      const key = normalizeText(chunk.slice(0, colon));
      const field = FIELD_ALIASES[key];
      const value = normalizeText(chunk.slice(colon + 1));
      if (field && value) {
        fields.push({ field, value });
        continue;
      }
    }

    freeText.push(chunk);
  }

  // Honorifics are stripped from the query for the same reason they are
  // stripped from stored names: `הרב משה כהן` and `משה כהן` are one search.
  const normalizedFreeText = stripHonorifics(normalizeText(freeText.join(' ')));

  const terms: ParsedTerm[] = tokenize(normalizedFreeText).map((term) => {
    const variants = expandToken(term).filter((variant) => variant !== term);
    return { term, variants, phonetic: phoneticKey(term), exact: false };
  });

  for (const phrase of phrases) {
    for (const term of tokenize(phrase)) {
      terms.push({ term, variants: [], phonetic: phoneticKey(term), exact: true });
    }
  }

  return {
    raw: input,
    terms,
    phrases,
    exclusions,
    fields,
    phone: null,
    isEmpty: terms.length === 0 && phrases.length === 0 && fields.length === 0,
  };
}

/**
 * Build an FTS5 MATCH expression.
 *
 * Terms are AND-ed (every word must appear somewhere in the document), while
 * each term's proclitic variants are OR-ed together. The last term gets a `*`
 * so results appear while the user is still typing it.
 *
 * FTS5 treats `"` as a phrase delimiter and a bare `-`/`*`/`:`/`(` as syntax,
 * so every term is wrapped in double quotes and any embedded quote is doubled.
 */
export function buildFtsMatch(query: ParsedQuery, { prefixLastTerm = true } = {}): string {
  const clauses: string[] = [];

  for (const phrase of query.phrases) {
    clauses.push(quoteFts(phrase));
  }

  query.terms.forEach((term, index) => {
    if (term.exact) return; // already covered by the phrase clauses

    const isLast = index === query.terms.length - 1;
    const usePrefix = prefixLastTerm && isLast && !term.exact;

    const alternatives = [term.term, ...term.variants].map((value) =>
      usePrefix ? `${quoteFts(value)}*` : quoteFts(value),
    );

    clauses.push(alternatives.length > 1 ? `(${alternatives.join(' OR ')})` : alternatives[0]!);
  });

  for (const exclusion of query.exclusions) {
    clauses.push(`NOT ${quoteFts(exclusion)}`);
  }

  return clauses.join(' AND ');
}

/** Wrap a term as an FTS5 string literal, escaping embedded double quotes. */
export function quoteFts(value: string): string {
  return `"${value.replace(/"/g, '""')}"`;
}
