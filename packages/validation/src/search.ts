import { z } from 'zod';
import { FACET_FIELDS, SEARCH_SORTS } from '@yanuka/types';
import { UlidSchema } from './common.js';

/**
 * Facet selections. `partialRecord` rather than `record` because every facet is
 * optional — `record` in Zod 4 requires each enum key to be present, which
 * would force the UI to send eight empty arrays to filter by nothing.
 */
export const FacetFiltersSchema = z
  .partialRecord(z.enum(FACET_FIELDS), z.array(z.string()))
  .default({});

export const SearchQuerySchema = z.object({
  /**
   * Capped rather than rejected: a paste of a whole paragraph into the search
   * box is a plausible way to look someone up by a remembered sentence.
   */
  text: z.string().max(500).default(''),
  filters: FacetFiltersSchema.optional(),
  sort: z.enum(SEARCH_SORTS).default('relevance'),
  limit: z.number().int().min(1).max(200).default(50),
  offset: z.number().int().min(0).default(0),
  favoritesOnly: z.boolean().default(false),
  includeDeleted: z.boolean().default(false),
});

export const SuggestQuerySchema = z.object({
  text: z.string().max(200),
  limit: z.number().int().min(1).max(20).default(8),
});

export const ListContactsQuerySchema = z.object({
  /**
   * Keyset cursor, opaque to the client. Offset pagination is not offered:
   * `OFFSET 99950` makes SQLite walk 99,950 rows, which breaks the performance
   * target at the exact scale the product promises to handle.
   */
  cursor: z.string().nullable().default(null),
  limit: z.number().int().min(1).max(200).default(50),
  sort: z.enum(['name', 'recently_updated', 'recently_added']).default('name'),
  /** Jump straight to a letter in the alphabet index. */
  startsWith: z.string().max(4).nullable().default(null),
  filters: FacetFiltersSchema.optional(),
  favoritesOnly: z.boolean().default(false),
  includeDeleted: z.boolean().default(false),
});

export const SavedSearchInputSchema = z.object({
  name: z.string().trim().min(1, 'יש להזין שם לחיפוש').max(120),
  query: SearchQuerySchema,
});

export const ContactIdSchema = z.object({ id: UlidSchema });
