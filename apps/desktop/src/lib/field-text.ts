/**
 * Human wording for record fields and their values — shared by the card
 * history and the conflicts screen, so a phone list reads the same in both.
 */

/** The same wording the edit form uses, so history reads like the form. */
export const FIELD_LABELS: Record<string, string> = {
  displayName: 'שם מלא',
  firstName: 'שם פרטי',
  lastName: 'שם משפחה',
  prefix: 'תואר',
  title: 'תפקיד',
  country: 'מדינה',
  region: 'אזור',
  city: 'עיר',
  address: 'כתובת',
  postalCode: 'מיקוד',
  profession: 'מקצוע',
  role: 'תפקיד',
  notes: 'הערה חופשית',
  reasonForSaving: 'נשמר בגלל',
  source: 'מקור',
  introducedBy: 'מי הכיר',
  introducedByContactId: 'מי הכיר (קישור)',
  isFavorite: 'מועדף',
  deletedAt: 'בסל המחזור',
  phones: 'טלפונים',
  emails: 'אימיילים',
  aliases: 'כינויים',
  specialties: 'התמחויות',
  languages: 'שפות',
  tagIds: 'תגיות',
  categoryIds: 'קטגוריות',
  categories: 'קטגוריות',
  organizations: 'מוסדות',
  body: 'הערה',
  isSensitive: 'רגיש',
  contactId: 'איש קשר',
  name: 'שם',
  description: 'תיאור',
  color: 'צבע',
  icon: 'אייקון',
  rule: 'כלל',
  sortOrder: 'סדר',
  showOnHome: 'במסך הבית',
  kind: 'סוג',
  type: 'סוג קשר',
};

function itemText(item: unknown, lookup?: (id: string) => string | undefined): string {
  if (item === null || item === undefined) return '';
  if (typeof item === 'string') return lookup?.(item) ?? item;
  if (typeof item !== 'object') return String(item);
  const record = item as Record<string, unknown>;
  for (const key of ['raw', 'address', 'value', 'name', 'categoryId', 'organizationId', 'tagId']) {
    const candidate = record[key];
    if (typeof candidate === 'string' && candidate) {
      const text = lookup?.(candidate) ?? candidate;
      return record.mode === 'exclude' ? `לא ${text}` : text;
    }
  }
  return JSON.stringify(item);
}

/**
 * A field value as a person reads it: lists joined, booleans as words,
 * nothing as "ריק". `lookup` turns ids (tags, categories) into names.
 */
export function valueText(
  value: unknown,
  options: { maxLength?: number; lookup?: (id: string) => string | undefined } = {},
): string {
  const { maxLength = 120, lookup } = options;
  let text: string;
  if (value === null || value === undefined || value === '') {
    text = 'ריק';
  } else if (Array.isArray(value)) {
    text = value.length === 0 ? 'ריק' : value.map((item) => itemText(item, lookup)).join(' · ');
  } else if (typeof value === 'boolean') {
    text = value ? 'כן' : 'לא';
  } else if (typeof value === 'object') {
    text = JSON.stringify(value);
  } else {
    text = String(value);
  }
  return [...text].length > maxLength
    ? `${[...text].slice(0, maxLength).join('').trimEnd()}…`
    : text;
}
