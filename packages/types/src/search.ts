import type { ContactSummary } from './contact.js';

/** Which layer produced a match. Drives the "why this result" explanation. */
export const MATCH_SOURCES = [
  'name',
  'alias',
  'phone',
  'email',
  'profession',
  'role',
  'specialty',
  'organization',
  'city',
  'country',
  'tag',
  'category',
  'notes',
  'reason_for_saving',
] as const;
export type MatchSource = (typeof MATCH_SOURCES)[number];

/** How the term was matched, which determines the score multiplier. */
export const MATCH_QUALITIES = ['exact', 'prefix', 'fuzzy', 'fulltext'] as const;
export type MatchQuality = (typeof MATCH_QUALITIES)[number];

/**
 * One reason a contact appeared in the result set. Surfaced in the UI as
 * "נמצא בגלל: …" so the user understands the ranking — see docs/SEARCH.md §17.
 */
export interface MatchReason {
  source: MatchSource;
  quality: MatchQuality;
  /** The query term that hit. */
  term: string;
  /** Text around the hit, for a highlighted snippet. Present for note matches. */
  snippet: string | null;
  score: number;
}

export interface SearchResult {
  contact: ContactSummary;
  score: number;
  reasons: MatchReason[];
}

/** Dimensions the result set can be sliced by, shown as counts next to results. */
export const FACET_FIELDS = [
  'country',
  'city',
  'profession',
  'specialty',
  'tag',
  'category',
  'organization',
  'language',
] as const;
export type FacetField = (typeof FACET_FIELDS)[number];

export interface FacetValue {
  value: string;
  /** Human-readable label; differs from `value` for coded fields like country. */
  label: string;
  count: number;
}

export type Facets = Partial<Record<FacetField, FacetValue[]>>;

/** Active facet restrictions. Values within a field are OR-ed, fields are AND-ed. */
export type FacetFilters = Partial<Record<FacetField, string[]>>;

export const SEARCH_SORTS = ['relevance', 'name', 'recently_updated', 'recently_viewed'] as const;
export type SearchSort = (typeof SEARCH_SORTS)[number];

export interface SearchQuery {
  /** Raw text as typed. Normalization happens inside the search engine. */
  text: string;
  filters?: FacetFilters;
  sort?: SearchSort;
  limit?: number;
  offset?: number;
  /** Restrict to favourites. */
  favoritesOnly?: boolean;
  /** Include soft-deleted rows. Only ever true in the trash view. */
  includeDeleted?: boolean;
}

export interface SearchResponse {
  results: SearchResult[];
  /** Total matches before `limit`/`offset`, used for pagination. */
  total: number;
  facets: Facets;
  /** Wall-clock time spent in the engine, surfaced in dev tooling. */
  tookMs: number;
  /** Terms the engine actually searched after normalization and expansion. */
  normalizedTerms: string[];
}

/** Typeahead entry shown while the user is still typing. */
export interface SearchSuggestion {
  kind: 'contact' | 'tag' | 'organization' | 'category' | 'profession' | 'city';
  id: string | null;
  label: string;
  sublabel: string | null;
  count: number | null;
}
