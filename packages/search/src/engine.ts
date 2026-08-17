import { COUNTRY_NAMES_HE, LANGUAGE_NAMES_HE, similarity, snippetAroundNormalized, tokenize } from '@yanuka/utils';
import type {
  ContactSummary,
  FacetField,
  FacetFilters,
  Facets,
  FacetValue,
  MatchQuality,
  MatchReason,
  MatchSource,
  SearchQuery,
  SearchResponse,
  SearchResult,
  SearchSuggestion,
} from '@yanuka/types';
import { normalizeText, PROCLITIC_PENALTY } from './normalize.js';
import { parseQuery, type ParsedQuery, type ParsedTerm } from './query.js';
import {
  combineScore,
  FUZZY_MAX_DISTANCE,
  FUZZY_MIN_SIMILARITY,
  fuzzyQualityFactor,
  scoreMatch,
} from './scoring.js';
import type { IndexableContact } from './indexing.js';

/**
 * A contact as the engine sees it: the projection shown in results, plus every
 * field that can be searched, plus the signals that feed ranking bonuses.
 */
export interface SearchableRecord {
  summary: ContactSummary;
  indexable: IndexableContact;
  lastViewedAt: string | null;
  /** Values used for facet counting, kept separate from the searchable text. */
  facetValues: Partial<Record<FacetField, string[]>>;
}

/** One searchable field of one contact, pre-normalized at index build time. */
interface FieldEntry {
  source: MatchSource;
  /** Normalized whole-field text, for exact full-field comparison. */
  text: string;
  tokens: string[];
  /** Original text, used to cut a snippet the user can actually read. */
  original: string;
}

interface IndexedRecord {
  record: SearchableRecord;
  fields: FieldEntry[];
  /** Digit strings of every phone, for format-insensitive matching. */
  phoneDigits: string[];
  hasPhone: boolean;
  hasOrganization: boolean;
}

export interface InMemoryIndex {
  records: IndexedRecord[];
  byId: Map<string, IndexedRecord>;
}

function field(
  source: MatchSource,
  value: string | null | undefined,
  entries: FieldEntry[],
): void {
  if (!value) return;
  const text = normalizeText(value);
  if (!text) return;
  entries.push({ source, text, tokens: tokenize(text), original: value });
}

function fields(source: MatchSource, values: string[] | undefined, entries: FieldEntry[]): void {
  for (const value of values ?? []) field(source, value, entries);
}

/**
 * Build a searchable index over an in-memory collection.
 *
 * This is the engine behind `MockRepository`, so the browser build of the app
 * is a real application rather than a click-through — and it is the reference
 * the SQLite implementation's ranking is compared against in tests.
 *
 * Linear scan is intentional. This path serves the demo dataset and the test
 * suite; the desktop's hundred-thousand-row path is FTS5 in SQLite.
 */
export function buildIndex(records: SearchableRecord[]): InMemoryIndex {
  const indexed = records.map<IndexedRecord>((record) => {
    const c = record.indexable;
    const entries: FieldEntry[] = [];

    field('name', c.displayName, entries);
    field('name', [c.firstName, c.lastName].filter(Boolean).join(' '), entries);
    fields('alias', c.aliases, entries);
    field('profession', c.profession, entries);
    field('role', c.role, entries);
    fields('specialty', c.specialties, entries);
    fields('organization', c.organizations, entries);
    field('city', c.city, entries);
    field('city', c.region, entries);
    field('country', c.country, entries);
    fields('tag', c.tags, entries);
    fields('category', c.categories, entries);
    field('notes', c.notes, entries);
    field('notes', c.introducedBy, entries);
    field('reason_for_saving', c.reasonForSaving, entries);
    fields('email', c.emails, entries);

    return {
      record,
      fields: entries,
      phoneDigits: c.phoneDigits ?? [],
      hasPhone: (c.phoneDigits ?? []).length > 0,
      hasOrganization: (c.organizations ?? []).length > 0,
    };
  });

  return {
    records: indexed,
    byId: new Map(indexed.map((entry) => [entry.record.summary.id, entry])),
  };
}

/** Best match of one term against one field, or null. */
function matchTerm(entry: FieldEntry, term: ParsedTerm): MatchReason | null {
  const candidates: Array<{ value: string; penalty: number }> = [
    { value: term.term, penalty: 1 },
    ...term.variants.map((variant) => ({ value: variant, penalty: PROCLITIC_PENALTY })),
  ];

  let best: MatchReason | null = null;

  const consider = (quality: MatchQuality, penalty: number, value: string): void => {
    const score = scoreMatch(entry.source, quality, penalty);
    if (!best || score > best.score) {
      best = {
        source: entry.source,
        quality,
        term: value,
        // Only long free-text fields get a snippet; for a name or a tag the
        // field itself is already the answer.
        snippet:
          entry.source === 'notes' || entry.source === 'reason_for_saving'
            ? snippetAroundNormalized(entry.original, value, normalizeText)
            : null,
        score,
      };
    }
  };

  for (const { value, penalty } of candidates) {
    if (!value) continue;

    // Whole field equals the term — the strongest possible signal.
    if (entry.text === value) {
      consider('exact', penalty, value);
      continue;
    }

    let sawToken = false;
    let sawPrefix = false;
    for (const token of entry.tokens) {
      if (token === value) {
        sawToken = true;
        break;
      }
      if (!sawPrefix && token.startsWith(value)) sawPrefix = true;
    }

    if (sawToken) {
      // A token hit inside a short field (a name) is worth more than the same
      // hit buried in a long note, which `fulltext` already reflects via the
      // quality multiplier.
      consider(entry.tokens.length <= 3 ? 'exact' : 'fulltext', penalty, value);
      continue;
    }
    if (sawPrefix) {
      consider('prefix', penalty, value);
      continue;
    }
    if (entry.text.includes(value) && value.length >= 3) {
      consider('fulltext', penalty, value);
      continue;
    }

    // Fuzzy rescue, names and aliases only. Running edit distance over note
    // bodies produces noise, not recall.
    if (
      (entry.source === 'name' || entry.source === 'alias') &&
      value.length >= 3 &&
      !term.exact
    ) {
      for (const token of entry.tokens) {
        if (Math.abs(token.length - value.length) > FUZZY_MAX_DISTANCE) continue;
        const score = similarity(token, value);
        if (score >= FUZZY_MIN_SIMILARITY) {
          const factor = fuzzyQualityFactor(score);
          const fuzzyScore = scoreMatch(entry.source, 'fuzzy', penalty) * (factor / 0.45);
          if (!best || fuzzyScore > best.score) {
            best = {
              source: entry.source,
              quality: 'fuzzy',
              term: value,
              snippet: null,
              score: Math.round(fuzzyScore * 100) / 100,
            };
          }
        }
      }
    }
  }

  return best;
}

function matchesFilters(record: SearchableRecord, filters: FacetFilters | undefined): boolean {
  if (!filters) return true;
  for (const [rawField, selected] of Object.entries(filters)) {
    if (!selected || selected.length === 0) continue;
    const values = record.facetValues[rawField as FacetField] ?? [];
    // Values within a field are OR-ed; different fields are AND-ed.
    if (!selected.some((value) => values.includes(value))) return false;
  }
  return true;
}

function matchesExclusions(entry: IndexedRecord, exclusions: string[]): boolean {
  if (exclusions.length === 0) return true;
  for (const excluded of exclusions) {
    for (const f of entry.fields) {
      if (f.text.includes(excluded)) return false;
    }
  }
  return true;
}

function matchesFieldFilters(entry: IndexedRecord, parsed: ParsedQuery): boolean {
  const FIELD_TO_SOURCE: Record<ParsedQuery['fields'][number]['field'], MatchSource> = {
    city: 'city',
    country: 'country',
    tag: 'tag',
    profession: 'profession',
    organization: 'organization',
    category: 'category',
  };

  return parsed.fields.every(({ field: name, value }) => {
    const source = FIELD_TO_SOURCE[name];
    return entry.fields.some((f) => f.source === source && f.text.includes(value));
  });
}

/**
 * Display names for coded facet values. Shared with the UI via @yanuka/utils so
 * `IL` reads as `ישראל` everywhere it appears, not just in the filter panel.
 */
const FACET_LABELS: Partial<Record<FacetField, Record<string, string>>> = {
  country: COUNTRY_NAMES_HE,
  language: LANGUAGE_NAMES_HE,
};

function computeFacets(records: SearchableRecord[], limitPerField = 12): Facets {
  const counters = new Map<FacetField, Map<string, number>>();

  for (const record of records) {
    for (const [rawField, values] of Object.entries(record.facetValues)) {
      const facetField = rawField as FacetField;
      let counter = counters.get(facetField);
      if (!counter) {
        counter = new Map<string, number>();
        counters.set(facetField, counter);
      }
      for (const value of values ?? []) {
        counter.set(value, (counter.get(value) ?? 0) + 1);
      }
    }
  }

  const facets: Facets = {};
  for (const [facetField, counter] of counters) {
    const values: FacetValue[] = [...counter.entries()]
      .map(([value, count]) => ({
        value,
        label: FACET_LABELS[facetField]?.[value] ?? value,
        count,
      }))
      .sort((a, b) => b.count - a.count || a.label.localeCompare(b.label, 'he'))
      .slice(0, limitPerField);
    if (values.length > 0) facets[facetField] = values;
  }

  return facets;
}

const DEFAULT_LIMIT = 50;

/**
 * High-resolution clock when one is available.
 *
 * Reached through `globalThis` rather than the ambient `performance` binding so
 * this package keeps compiling without the DOM lib — it has to run in Node
 * (tests), a browser (the web build) and Hermes (React Native).
 */
function monotonicNow(): number {
  const perf = (globalThis as { performance?: { now(): number } }).performance;
  return perf ? perf.now() : Date.now();
}

/**
 * Run a query against an in-memory index.
 *
 * An empty query is a browse, not an error: the home screen shows favourites
 * and recently viewed contacts before a single character is typed.
 */
export function search(index: InMemoryIndex, query: SearchQuery, now = Date.now()): SearchResponse {
  const startedAt = monotonicNow();
  const parsed = parseQuery(query.text);
  const limit = query.limit ?? DEFAULT_LIMIT;
  const offset = query.offset ?? 0;

  const scored: SearchResult[] = [];
  const matchedRecords: SearchableRecord[] = [];

  for (const entry of index.records) {
    const { record } = entry;

    if (!query.includeDeleted && record.facetValues == null) continue;
    if (query.favoritesOnly && !record.summary.isFavorite) continue;
    if (!matchesFilters(record, query.filters)) continue;
    if (!matchesFieldFilters(entry, parsed)) continue;
    if (!matchesExclusions(entry, parsed.exclusions)) continue;

    const reasons: MatchReason[] = [];

    if (parsed.phone) {
      const hit = entry.phoneDigits.find((digits) => digits.endsWith(parsed.phone!.key));
      if (!hit) continue;
      reasons.push({
        source: 'phone',
        quality: hit === parsed.phone.digits ? 'exact' : 'prefix',
        term: parsed.phone.digits,
        snippet: null,
        score: scoreMatch('phone', hit === parsed.phone.digits ? 'exact' : 'prefix'),
      });
    } else if (parsed.terms.length > 0) {
      // Every term must match somewhere; that is what makes multi-word queries
      // like "סופר סתם ירושלים" narrow rather than widen the result set.
      let allTermsMatched = true;
      for (const term of parsed.terms) {
        let bestForTerm: MatchReason | null = null;
        let bestSnippet: MatchReason | null = null;

        for (const f of entry.fields) {
          const reason = matchTerm(f, term);
          if (!reason) continue;
          if (!bestForTerm || reason.score > bestForTerm.score) bestForTerm = reason;
          // A free-text hit is kept even when a stronger field also matched.
          // Its score is not what makes it worth reporting — the snippet is:
          // "found because of this sentence you wrote" is the explanation that
          // actually helps someone who has forgotten the name.
          if (reason.snippet && (!bestSnippet || reason.score > bestSnippet.score)) {
            bestSnippet = reason;
          }
        }

        if (!bestForTerm) {
          allTermsMatched = false;
          break;
        }
        reasons.push(bestForTerm);
        if (bestSnippet && bestSnippet !== bestForTerm) reasons.push(bestSnippet);
      }
      if (!allTermsMatched) continue;
    } else if (parsed.fields.length === 0 && !query.favoritesOnly && !query.filters) {
      // Nothing to search and nothing to filter by: browse mode.
      reasons.push({
        source: 'name',
        quality: 'exact',
        term: '',
        snippet: null,
        score: 0,
      });
    }

    const msSinceViewed = record.lastViewedAt
      ? now - new Date(record.lastViewedAt).getTime()
      : null;

    scored.push({
      contact: record.summary,
      score: combineScore({
        reasons,
        hasPhone: entry.hasPhone,
        hasOrganization: entry.hasOrganization,
        isFavorite: record.summary.isFavorite,
        msSinceViewed,
      }),
      reasons: reasons.filter((reason) => reason.term !== ''),
    });
    matchedRecords.push(record);
  }

  const sort = query.sort ?? 'relevance';
  scored.sort((a, b) => {
    switch (sort) {
      case 'name':
        return a.contact.displayName.localeCompare(b.contact.displayName, 'he');
      case 'recently_updated':
        return b.contact.updatedAt.localeCompare(a.contact.updatedAt);
      case 'recently_viewed':
        return b.contact.updatedAt.localeCompare(a.contact.updatedAt);
      case 'relevance':
      default:
        return (
          b.score - a.score || a.contact.displayName.localeCompare(b.contact.displayName, 'he')
        );
    }
  });

  const endedAt = monotonicNow();

  return {
    results: scored.slice(offset, offset + limit),
    total: scored.length,
    facets: computeFacets(matchedRecords),
    tookMs: Math.round((endedAt - startedAt) * 100) / 100,
    normalizedTerms: parsed.terms.map((term) => term.term),
  };
}

/**
 * Typeahead entries for the command palette.
 *
 * Contacts come first because they are what the user is usually after; tags,
 * organizations and cities follow as ways to *widen* into a browse when the
 * exact person is not remembered.
 */
export function suggest(index: InMemoryIndex, text: string, limit = 8): SearchSuggestion[] {
  const normalized = normalizeText(text);
  if (!normalized) return [];

  const suggestions: SearchSuggestion[] = [];
  const response = search(index, { text, limit });

  for (const result of response.results.slice(0, limit)) {
    suggestions.push({
      kind: 'contact',
      id: result.contact.id,
      label: result.contact.displayName,
      sublabel:
        [result.contact.profession, result.contact.city].filter(Boolean).join(' · ') || null,
      count: null,
    });
  }

  const addFacetSuggestions = (
    facetField: FacetField,
    kind: SearchSuggestion['kind'],
    max: number,
  ): void => {
    for (const value of response.facets[facetField] ?? []) {
      if (suggestions.filter((s) => s.kind === kind).length >= max) break;
      if (!normalizeText(value.label).includes(normalized)) continue;
      suggestions.push({
        kind,
        id: value.value,
        label: value.label,
        sublabel: null,
        count: value.count,
      });
    }
  };

  addFacetSuggestions('tag', 'tag', 3);
  addFacetSuggestions('organization', 'organization', 3);
  addFacetSuggestions('city', 'city', 2);

  return suggestions;
}
