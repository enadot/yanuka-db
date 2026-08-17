import type { CountryCode, LanguageCode } from '@yanuka/types';

/**
 * Hebrew display names for the countries and languages that appear in this
 * database.
 *
 * Codes are what get stored — they are stable, sortable and sync cleanly — but
 * a Hebrew-first interface must never show `IL` to a user. This table is the
 * single place the translation happens, so the search facets, the contact
 * subtitle and the detail screen cannot disagree.
 *
 * Unknown codes fall through to the code itself rather than being hidden: an
 * imported record with an unexpected country is still worth displaying.
 */
export const COUNTRY_NAMES_HE: Record<string, string> = {
  IL: 'ישראל',
  US: 'ארצות הברית',
  GB: 'אנגליה',
  FR: 'צרפת',
  BE: 'בלגיה',
  CA: 'קנדה',
  AU: 'אוסטרליה',
  AR: 'ארגנטינה',
  RU: 'רוסיה',
  UA: 'אוקראינה',
  CH: 'שווייץ',
  ZA: 'דרום אפריקה',
  BR: 'ברזיל',
  MX: 'מקסיקו',
  IT: 'איטליה',
  DE: 'גרמניה',
  NL: 'הולנד',
  AT: 'אוסטריה',
  ES: 'ספרד',
  HU: 'הונגריה',
  PA: 'פנמה',
  UY: 'אורוגוואי',
  CL: "צ'ילה",
  SE: 'שוודיה',
  PL: 'פולין',
};

export const LANGUAGE_NAMES_HE: Record<string, string> = {
  he: 'עברית',
  yi: 'אידיש',
  en: 'אנגלית',
  fr: 'צרפתית',
  ru: 'רוסית',
  es: 'ספרדית',
  de: 'גרמנית',
  nl: 'הולנדית',
  it: 'איטלקית',
  pt: 'פורטוגזית',
  ar: 'ערבית',
  uk: 'אוקראינית',
  hu: 'הונגרית',
  pl: 'פולנית',
};

export function countryName(code: CountryCode | null | undefined): string | null {
  if (!code) return null;
  return COUNTRY_NAMES_HE[code] ?? code;
}

export function languageName(code: LanguageCode | null | undefined): string | null {
  if (!code) return null;
  return LANGUAGE_NAMES_HE[code] ?? code;
}
