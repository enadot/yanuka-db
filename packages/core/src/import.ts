/**
 * CSV import: header auto-detection and row → ContactInput conversion.
 *
 * Pure logic, no I/O — the desktop screen feeds it a parsed file and a
 * (possibly user-corrected) column mapping, and imports the results through
 * `ContactsRepository.createContact` like any other create. That boundary is
 * what makes the whole flow testable here and identical against the in-memory
 * repository and SQLite.
 *
 * The product priority "מידע לא הולך לאיבוד" decides every ambiguity:
 * - only a missing name fails a row, because nothing can be stored without it;
 * - a phone is stored exactly as written (the schema is permissive by design);
 * - a malformed email is demoted to a notes line rather than failing the row;
 * - unmapped ambiguity is the user's call in the mapping UI, not silent.
 */

import type { ContactInput } from '@yanuka/validation';
import { ContactInputSchema } from '@yanuka/validation';
import { collapseWhitespace } from '@yanuka/utils';

/** What a CSV column can be imported as. */
export const IMPORT_TARGETS = [
  'ignore',
  'displayName',
  'firstName',
  'lastName',
  'prefix',
  'phone',
  'email',
  'city',
  'address',
  'profession',
  'role',
  'organization',
  'notes',
  'reasonForSaving',
  'source',
  'introducedBy',
] as const;
export type ImportTarget = (typeof IMPORT_TARGETS)[number];

export const IMPORT_TARGET_LABELS: Record<ImportTarget, string> = {
  ignore: 'התעלם',
  displayName: 'שם מלא',
  firstName: 'שם פרטי',
  lastName: 'שם משפחה',
  prefix: 'תואר (לפני השם)',
  phone: 'טלפון',
  email: 'אימייל',
  city: 'עיר',
  address: 'כתובת',
  profession: 'מקצוע',
  role: 'תפקיד',
  organization: 'ארגון / מוסד',
  notes: 'הערות',
  reasonForSaving: 'נשמר בגלל',
  source: 'מקור',
  introducedBy: 'הכיר בינינו',
};

/**
 * Keyword table for header auto-detection, most specific first.
 *
 * Covers the headers Google Contacts, Outlook (English and Hebrew) and
 * hand-made Hebrew Excel sheets actually produce. Matching is
 * case-insensitive substring — "Phone 1 - Value" and "Mobile Phone" both hit
 * `phone`. Detection is a convenience: whatever it gets wrong, the user
 * corrects in the mapping UI before anything is written.
 */
const EXACT_HEADERS: Array<[ImportTarget, string[]]> = [
  ['displayName', ['name', 'full name', 'display name', 'שם', 'שם מלא', 'שם תצוגה']],
];

const HEADER_KEYWORDS: Array<[ImportTarget, string[]]> = [
  ['prefix', ['name prefix', 'קידומת', 'תואר']],
  ['firstName', ['first name', 'given name', 'שם פרטי']],
  ['lastName', ['last name', 'family name', 'surname', 'שם משפחה']],
  ['email', ['e-mail', 'email', 'אימייל', 'דוא"ל', 'דואל', 'מייל']],
  ['phone', ['phone', 'mobile', 'טלפון', 'נייד', 'פלאפון', 'סלולרי', 'פקס']],
  ['city', ['city', 'עיר', 'ישוב', 'יישוב']],
  ['address', ['address', 'street', 'כתובת', 'רחוב']],
  ['profession', ['profession', 'occupation', 'מקצוע', 'עיסוק']],
  ['role', ['job title', 'תפקיד']],
  ['organization', ['organization', 'company', 'ארגון', 'מוסד', 'חברה', 'ישיבה', 'קהילה']],
  ['reasonForSaving', ['reason', 'סיבה', 'נשמר בגלל']],
  ['notes', ['notes', 'note', 'comments', 'הערות', 'הערה']],
  ['source', ['source', 'מקור']],
  ['introducedBy', ['introduced', 'referred', 'ממליץ', 'המליץ', 'הכיר']],
];

/**
 * Suggest a target for every header. Unrecognized headers map to `ignore`.
 *
 * Exact names are tried before keywords so Google's "Name Prefix" and
 * "Organization 1 - Name" do not land on `displayName` via the substring
 * "name". Google's "… - Type" columns describe the value beside them
 * ("Mobile", "Home") and are never data, so they are ignored outright.
 */
export function suggestMapping(headers: string[]): ImportTarget[] {
  return headers.map((header) => {
    const normalized = header.trim().toLowerCase();
    if (normalized === '' || normalized === 'type' || normalized.includes(' - type')) {
      return 'ignore';
    }
    for (const [target, names] of EXACT_HEADERS) {
      if (names.includes(normalized)) {
        return target;
      }
    }
    for (const [target, keywords] of HEADER_KEYWORDS) {
      if (keywords.some((keyword) => normalized.includes(keyword))) {
        return target;
      }
    }
    return 'ignore';
  });
}

export interface ImportRowResult {
  /** 1-based data row number, as a person counting rows in Excel would. */
  row: number;
  input: ContactInput | null;
  /** Human-readable reason when `input` is null. */
  error: string | null;
}

const EMAIL_SHAPE = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;

/**
 * Convert one CSV row into a validated ContactInput.
 *
 * `fallbackSource` fills the `source` field when the file has no mapped source
 * column — the screen passes the file name, so every imported contact records
 * where it came from.
 */
export function buildContactInput(
  cells: string[],
  mapping: ImportTarget[],
  fallbackSource?: string,
): { input: ContactInput | null; error: string | null } {
  const single: Partial<Record<ImportTarget, string>> = {};
  const phones: string[] = [];
  const emails: string[] = [];
  const noteLines: string[] = [];

  mapping.forEach((target, index) => {
    const value = collapseWhitespace(cells[index] ?? '');
    if (value === '' || target === 'ignore') {
      return;
    }
    if (target === 'phone') {
      phones.push(value);
    } else if (target === 'email') {
      if (EMAIL_SHAPE.test(value)) {
        emails.push(value);
      } else {
        noteLines.push(`אימייל (כפי שנכתב): ${value}`);
      }
    } else if (target === 'notes') {
      noteLines.push(value);
    } else if (target === 'organization') {
      noteLines.push(`ארגון: ${value}`);
    } else if (single[target] === undefined) {
      single[target] = value;
    } else {
      // Two columns mapped to one scalar field: keep the first, save the rest.
      noteLines.push(`${IMPORT_TARGET_LABELS[target]}: ${value}`);
    }
  });

  const displayName =
    single.displayName ??
    collapseWhitespace([single.prefix, single.firstName, single.lastName].filter(Boolean).join(' '));
  if (displayName === '') {
    return { input: null, error: 'אין שם — שום עמודה של שם אינה מלאה בשורה זו' };
  }

  const candidate = {
    displayName,
    firstName: single.firstName ?? null,
    lastName: single.lastName ?? null,
    prefix: single.prefix ?? null,
    city: single.city ?? null,
    address: single.address ?? null,
    profession: single.profession ?? null,
    role: single.role ?? null,
    notes: noteLines.length > 0 ? noteLines.join('\n') : null,
    reasonForSaving: single.reasonForSaving ?? null,
    source: single.source ?? fallbackSource ?? null,
    introducedBy: single.introducedBy ?? null,
    phones: phones.map((raw, index) => ({ raw, isPrimary: index === 0 })),
    emails: emails.map((address, index) => ({ address, isPrimary: index === 0 })),
  };

  const parsed = ContactInputSchema.safeParse(candidate);
  if (!parsed.success) {
    const first = parsed.error.issues[0];
    return { input: null, error: `${first?.path.join('.')}: ${first?.message}` };
  }
  return { input: parsed.data, error: null };
}

/** Convert every row, keeping failures alongside successes for the summary. */
export function buildImportPlan(
  rows: string[][],
  mapping: ImportTarget[],
  fallbackSource?: string,
): ImportRowResult[] {
  return rows.map((cells, index) => {
    const { input, error } = buildContactInput(cells, mapping, fallbackSource);
    return { row: index + 1, input, error };
  });
}
