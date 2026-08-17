import { z } from 'zod';

/**
 * ULID: 26 characters of Crockford base32, first character bounded so the
 * 48-bit timestamp cannot overflow.
 */
export const UlidSchema = z
  .string()
  .regex(/^[0-7][0-9A-HJKMNP-TV-Z]{25}$/, 'מזהה אינו תקין');

export const IsoDateTimeSchema = z.iso.datetime({ message: 'תאריך אינו תקין' });

export const CountryCodeSchema = z
  .string()
  .length(2, 'קוד מדינה חייב להיות שתי אותיות')
  .regex(/^[A-Z]{2}$/, 'קוד מדינה חייב להיות באותיות גדולות');

export const LanguageCodeSchema = z.string().regex(/^[a-z]{2}$/, 'קוד שפה אינו תקין');

/**
 * Free text that must not be blank once trimmed.
 *
 * Whitespace-only input is the most common way a required field ends up
 * "filled" but empty, so it is rejected here rather than at every call site.
 */
export function requiredText(max: number, message = 'שדה חובה') {
  return z.string().trim().min(1, message).max(max, `עד ${max} תווים`);
}

/** Optional free text; blank strings collapse to null rather than `''`. */
export function optionalText(max: number) {
  return z
    .string()
    .trim()
    .max(max, `עד ${max} תווים`)
    .transform((value) => (value.length === 0 ? null : value))
    .nullable()
    .default(null);
}

/** Sync envelope fields, shared by every persisted entity. */
export const SyncableEntitySchema = z.object({
  id: UlidSchema,
  createdAt: IsoDateTimeSchema,
  updatedAt: IsoDateTimeSchema,
  createdBy: UlidSchema.nullable(),
  updatedBy: UlidSchema.nullable(),
  version: z.number().int().nonnegative(),
  deviceId: z.string().nullable(),
  deletedAt: IsoDateTimeSchema.nullable(),
});

/** Cursor-based page envelope used by every list endpoint. */
export function pageOf<T extends z.ZodTypeAny>(item: T) {
  return z.object({
    items: z.array(item),
    /** Opaque keyset cursor for the next page; null when the list is exhausted. */
    nextCursor: z.string().nullable(),
    total: z.number().int().nonnegative(),
  });
}
