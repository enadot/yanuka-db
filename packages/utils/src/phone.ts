import { parsePhoneNumberFromString, type CountryCode as LibCountryCode } from 'libphonenumber-js';
import type { CountryCode } from '@yanuka/types';

export interface NormalizedPhone {
  /** Exactly what was typed, untouched. */
  raw: string;
  /** E.164 when the number parsed successfully, otherwise null. */
  e164: string | null;
  /**
   * Digits only, with any international prefix included when known. Search
   * matches on a suffix of this, which is what makes `4567`, `054-123-4567`
   * and `+972 54 123 4567` all find the same row.
   */
  digits: string;
  countryCode: CountryCode | null;
  /** Pretty form for display; falls back to `raw` when unparseable. */
  formatted: string;
  isValid: boolean;
}

/** Strip everything that is not a digit. Keeps no leading `+`. */
export function digitsOnly(value: string): string {
  return value.replace(/\D+/g, '');
}

/**
 * Parse a phone number without ever discarding the original.
 *
 * The database is full of numbers copied from decades-old notebooks: partial,
 * mis-typed, with local prefixes for countries the user no longer remembers.
 * An unparseable number is still worth storing and still worth finding, so
 * failure here degrades to `raw` + `digits` rather than rejecting the input.
 */
export function normalizePhone(
  raw: string,
  defaultCountry: CountryCode | null = null,
): NormalizedPhone {
  const trimmed = raw.trim();
  const fallback: NormalizedPhone = {
    raw: trimmed,
    e164: null,
    digits: digitsOnly(trimmed),
    countryCode: null,
    formatted: trimmed,
    isValid: false,
  };

  if (!trimmed) return fallback;

  try {
    const parsed = parsePhoneNumberFromString(
      trimmed,
      defaultCountry ? (defaultCountry as LibCountryCode) : undefined,
    );
    if (!parsed) return fallback;

    return {
      raw: trimmed,
      e164: parsed.number,
      digits: digitsOnly(parsed.number),
      countryCode: (parsed.country as CountryCode | undefined) ?? null,
      formatted: parsed.formatInternational(),
      isValid: parsed.isValid(),
    };
  } catch {
    // libphonenumber throws on some malformed inputs rather than returning
    // undefined. A bad number must never block saving a contact.
    return fallback;
  }
}

/**
 * Digit suffix used to look a number up.
 *
 * Country and trunk prefixes are the part users get wrong or omit, so matching
 * on the last 7 digits finds the record whichever way it was entered. Shorter
 * inputs are matched as typed.
 */
export const PHONE_SUFFIX_LENGTH = 7;

export function phoneSearchKey(input: string): string {
  const digits = digitsOnly(input);
  return digits.length > PHONE_SUFFIX_LENGTH ? digits.slice(-PHONE_SUFFIX_LENGTH) : digits;
}

/** Minimum digits before a query is treated as a phone-number search at all. */
export const PHONE_QUERY_MIN_DIGITS = 4;

/**
 * Whether a query string should be routed to the phone matcher.
 * Requires enough digits to be selective and no letters.
 */
export function looksLikePhoneQuery(query: string): boolean {
  const trimmed = query.trim();
  if (!trimmed) return false;
  if (/\p{L}/u.test(trimmed)) return false;
  return digitsOnly(trimmed).length >= PHONE_QUERY_MIN_DIGITS;
}

/** `tel:` URI for the click-to-call affordance. */
export function telHref(phone: Pick<NormalizedPhone, 'e164' | 'raw'>): string {
  return `tel:${phone.e164 ?? phone.raw.replace(/\s+/g, '')}`;
}

/** WhatsApp deep link. Requires E.164; returns null when the number is unparsed. */
export function whatsappHref(phone: Pick<NormalizedPhone, 'e164'>): string | null {
  if (!phone.e164) return null;
  return `https://wa.me/${digitsOnly(phone.e164)}`;
}
