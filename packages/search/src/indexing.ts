import { trigrams } from '@yanuka/utils';
import { normalizeText, phoneticKeys } from './normalize.js';

/**
 * Everything that goes into a contact's searchable document.
 *
 * Assembled from eight tables, which is why the index is rebuilt by application
 * code inside the writing transaction rather than by SQL triggers: a trigger on
 * `contact_tags` cannot see the contact's aliases, and none of them can call
 * the Hebrew normalizer.
 */
export interface IndexableContact {
  id: string;
  displayName: string;
  prefix?: string | null;
  firstName?: string | null;
  lastName?: string | null;
  aliases?: string[];
  profession?: string | null;
  role?: string | null;
  specialties?: string[];
  organizations?: string[];
  city?: string | null;
  region?: string | null;
  country?: string | null;
  tags?: string[];
  categories?: string[];
  notes?: string | null;
  reasonForSaving?: string | null;
  introducedBy?: string | null;
  phoneDigits?: string[];
  emails?: string[];
}

/** One row of `contact_fts`, all fields pre-normalized. */
export interface ContactIndexDocument {
  contactId: string;
  name: string;
  aliases: string;
  profession: string;
  role: string;
  specialties: string;
  organization: string;
  city: string;
  country: string;
  tags: string;
  categories: string;
  notes: string;
  reasonForSaving: string;
}

function joinNormalized(values: Array<string | null | undefined>): string {
  const parts = values
    .map((value) => normalizeText(value))
    .filter((value): value is string => value.length > 0);
  return [...new Set(parts)].join(' ');
}

/**
 * Build the FTS5 row for a contact.
 *
 * The name column intentionally carries the display name, the first/last parts
 * and the phonetic keys together. Storing the phonetic skeleton alongside the
 * real spelling means a misspelled query can be answered by the same index
 * instead of a second lookup.
 */
export function buildIndexDocument(contact: IndexableContact): ContactIndexDocument {
  const nameParts = [contact.displayName, contact.firstName, contact.lastName, contact.prefix];
  const namePhonetics = phoneticKeys(
    [contact.displayName, ...(contact.aliases ?? [])].filter(Boolean).join(' '),
  );

  return {
    contactId: contact.id,
    name: joinNormalized([...nameParts, ...namePhonetics]),
    aliases: joinNormalized(contact.aliases ?? []),
    profession: joinNormalized([contact.profession]),
    role: joinNormalized([contact.role]),
    specialties: joinNormalized(contact.specialties ?? []),
    organization: joinNormalized(contact.organizations ?? []),
    city: joinNormalized([contact.city, contact.region]),
    country: joinNormalized([contact.country]),
    tags: joinNormalized(contact.tags ?? []),
    categories: joinNormalized(contact.categories ?? []),
    // `introducedBy` lives in the notes column because "who sent them to us" is
    // the same kind of recall cue as a remark, and users search it the same way.
    notes: joinNormalized([contact.notes, contact.introducedBy]),
    reasonForSaving: joinNormalized([contact.reasonForSaving]),
  };
}

/**
 * The single blob indexed by the trigram table, used for substring and fuzzy
 * candidate retrieval.
 *
 * Only names, aliases and phone digits are included. Notes are excluded on
 * purpose: trigram-indexing free text multiplies the index size by an order of
 * magnitude for a layer that exists solely to rescue misspelled *names*.
 */
export function buildTrigramHaystack(contact: IndexableContact): string {
  return joinNormalized([
    contact.displayName,
    contact.firstName,
    contact.lastName,
    ...(contact.aliases ?? []),
    ...(contact.phoneDigits ?? []),
  ]);
}

/** Trigrams of a query term, used to look up candidates in the trigram table. */
export function queryTrigrams(term: string): string[] {
  return [...new Set(trigrams(normalizeText(term)))];
}

/**
 * How many trigrams a candidate must share before it is worth scoring.
 *
 * Forty percent overlap tolerates roughly one edit per five characters, which
 * matches the `FUZZY_MIN_SIMILARITY` threshold the reranker applies afterwards.
 */
export function minSharedTrigrams(termTrigramCount: number): number {
  return Math.max(1, Math.ceil(termTrigramCount * 0.4));
}
