import type { ContactInput } from '@yanuka/validation';

/**
 * What each contact field is called, in the words the edit form uses.
 *
 * Needed because the conflict screen is the one place that shows a field the
 * user did not navigate to — it is handed a name like `reasonForSaving` by the
 * sync engine and has to ask a question about it. Every other screen knows
 * which field it is drawing and labels it inline.
 *
 * Typed as a total map over `ContactInput` on purpose: adding a column to the
 * contact without naming it here fails the build rather than surfacing as a
 * question about "postalCode" in the middle of Hebrew text.
 */
export const FIELD_LABELS: Record<keyof ContactInput, string> = {
  firstName: 'שם פרטי',
  lastName: 'שם משפחה',
  displayName: 'שם מלא',
  prefix: 'תואר לפני השם',
  title: 'תואר',
  country: 'מדינה',
  region: 'אזור',
  city: 'עיר',
  address: 'כתובת',
  postalCode: 'מיקוד',
  profession: 'מקצוע',
  role: 'תפקיד',
  notes: 'הערות',
  reasonForSaving: 'סיבת השמירה',
  source: 'מקור',
  introducedBy: 'הופנה על ידי',
  introducedByContactId: 'המפנה',
  isFavorite: 'מסומן כמועדף',
  phones: 'מספרי טלפון',
  emails: 'כתובות אימייל',
  aliases: 'שמות נוספים',
  specialties: 'תחומי התמחות',
  languages: 'שפות',
  tagIds: 'תגיות',
  categoryIds: 'קטגוריות',
  organizations: 'מוסדות',
};

/** The label, or the raw name when the engine reports something unmapped. */
export function fieldLabel(field: string): string {
  return FIELD_LABELS[field as keyof ContactInput] ?? field;
}

/**
 * A field value as a person would read it.
 *
 * The conflict screen receives whatever the field happens to hold — a string, a
 * flag, a list of phone objects — and has to put two of them side by side
 * legibly. An id is never shown alone: `["01J…"]` is not a thing anyone can
 * choose between, so lists of ids say how many rather than which.
 */
export function formatFieldValue(field: string, value: unknown): string {
  if (value === null || value === undefined) return '';
  if (typeof value === 'boolean') return value ? 'כן' : 'לא';
  if (typeof value === 'number') return String(value);
  if (typeof value === 'string') return value;

  if (Array.isArray(value)) {
    if (value.length === 0) return '';
    if (field === 'tagIds' || field === 'categoryIds') {
      return `${value.length} פריטים`;
    }
    return value.map(summarizeItem).filter(Boolean).join(', ');
  }

  return JSON.stringify(value);
}

/** The one readable field of a child record, or a count if it has none. */
function summarizeItem(item: unknown): string {
  if (typeof item === 'string') return item;
  if (item === null || typeof item !== 'object') return '';
  const record = item as Record<string, unknown>;
  for (const key of ['raw', 'address', 'value', 'name', 'label']) {
    const candidate = record[key];
    if (typeof candidate === 'string' && candidate.trim()) return candidate;
  }
  const role = record.role;
  return typeof role === 'string' && role.trim() ? role : 'רשומה מקושרת';
}
