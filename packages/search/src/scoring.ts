import type { MatchQuality, MatchReason, MatchSource } from '@yanuka/types';

/**
 * Field weights, taken from the product spec.
 *
 * The ordering encodes an editorial judgement about identity: a name or a phone
 * number identifies a person, a city merely narrows them down. Notes score
 * lowest per hit because they are long and therefore easy to match by accident
 * — but they are the field that makes this database worth having, so they are
 * never excluded, only ranked below the precise signals.
 */
export const FIELD_WEIGHTS: Record<MatchSource, number> = {
  name: 100,
  phone: 100,
  alias: 90,
  email: 60,
  profession: 50,
  tag: 40,
  category: 40,
  organization: 35,
  specialty: 35,
  city: 30,
  country: 20,
  role: 30,
  notes: 20,
  reason_for_saving: 25,
};

/**
 * Multiplier for how well the term matched.
 *
 * `exact` is the whole field equalling the term; `prefix` is a completion while
 * the user is still typing; `fulltext` is a token hit somewhere inside a longer
 * field; `fuzzy` is an edit-distance rescue and is deliberately expensive.
 */
export const QUALITY_MULTIPLIERS: Record<MatchQuality, number> = {
  exact: 1,
  prefix: 0.8,
  fulltext: 0.55,
  fuzzy: 0.45,
};

/** Small additive nudges applied once per contact, not per match. */
export const BONUSES = {
  /** The record is complete enough to act on. */
  hasPhone: 3,
  hasOrganization: 2,
  favorite: 12,
  /** Recently opened records are usually the ones being worked on. */
  recentlyViewed: 8,
} as const;

/** Score for a single match, before per-contact bonuses. */
export function scoreMatch(
  source: MatchSource,
  quality: MatchQuality,
  /** Extra discount, e.g. PROCLITIC_PENALTY for a grammatically-guessed stem. */
  penalty = 1,
): number {
  return FIELD_WEIGHTS[source] * QUALITY_MULTIPLIERS[quality] * penalty;
}

export interface ScoreInputs {
  reasons: MatchReason[];
  hasPhone: boolean;
  hasOrganization: boolean;
  isFavorite: boolean;
  /** Milliseconds since the contact was last opened, or null if never. */
  msSinceViewed: number | null;
}

const RECENT_VIEW_WINDOW_MS = 7 * 24 * 60 * 60 * 1000;

/**
 * Combine per-match scores into a single ranking score.
 *
 * Matches on the *same* field do not simply add up: five hits of the same query
 * word inside one long note should not outrank a single exact name match. Each
 * field therefore contributes its best match at full value and any additional
 * matches at a steep discount, which rewards a contact matching across several
 * different fields — the real signal that a result is what the user meant.
 */
export function combineScore(inputs: ScoreInputs): number {
  const bestPerField = new Map<MatchSource, number[]>();

  for (const reason of inputs.reasons) {
    const bucket = bestPerField.get(reason.source);
    if (bucket) bucket.push(reason.score);
    else bestPerField.set(reason.source, [reason.score]);
  }

  let total = 0;
  for (const scores of bestPerField.values()) {
    scores.sort((a, b) => b - a);
    total += scores[0]!;
    for (let i = 1; i < scores.length; i += 1) {
      total += scores[i]! * 0.15;
    }
  }

  if (inputs.hasPhone) total += BONUSES.hasPhone;
  if (inputs.hasOrganization) total += BONUSES.hasOrganization;
  if (inputs.isFavorite) total += BONUSES.favorite;
  if (inputs.msSinceViewed != null && inputs.msSinceViewed < RECENT_VIEW_WINDOW_MS) {
    // Linear decay across the window rather than a cliff at day seven.
    total += BONUSES.recentlyViewed * (1 - inputs.msSinceViewed / RECENT_VIEW_WINDOW_MS);
  }

  return Math.round(total * 100) / 100;
}

/**
 * Map an FTS5 bm25 rank onto a `fulltext` quality multiplier.
 *
 * bm25 values are only comparable within a single query, so they are used to
 * order *within* the full-text layer and never compared against the exact or
 * fuzzy layers. `rank` is bm25's output (more negative is better) and
 * `worstRank` the least relevant value in the same result set.
 */
export function bm25ToQualityFactor(rank: number, bestRank: number, worstRank: number): number {
  if (!Number.isFinite(rank)) return 0.5;
  const span = worstRank - bestRank;
  if (span <= 0) return 1;
  const position = (worstRank - rank) / span; // 1 at the best rank, 0 at the worst
  return 0.6 + 0.4 * position;
}

/** Minimum similarity for a fuzzy candidate to be shown at all. */
export const FUZZY_MIN_SIMILARITY = 0.72;

/** Maximum edit distance considered, regardless of term length. */
export const FUZZY_MAX_DISTANCE = 2;

/**
 * Quality factor for a fuzzy hit, scaled by how close the match actually was.
 * A distance-1 typo ranks well above a distance-2 guess.
 */
export function fuzzyQualityFactor(similarityScore: number): number {
  if (similarityScore >= 1) return 1;
  if (similarityScore < FUZZY_MIN_SIMILARITY) return 0;
  const range = 1 - FUZZY_MIN_SIMILARITY;
  return 0.35 + 0.35 * ((similarityScore - FUZZY_MIN_SIMILARITY) / range);
}
