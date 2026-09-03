import type {
  CategoryCondition,
  CategoryRule,
  CategoryRuleField,
  CategoryRuleOperator,
  ContactWithRelations,
} from '@yanuka/types';
import { normalizeText } from '@yanuka/search';

/**
 * Rule evaluation for smart categories (ADR-038), in TypeScript.
 *
 * This is the reference semantics the in-memory repository runs and the
 * contract tests pin down; `crates/yanuka-db/src/categories.rs` implements the
 * same vocabulary in SQL. The two are held together by the contract suite —
 * change one, change both.
 *
 * `contains` is a word-start match on normalized text: `רב` finds `רב`,
 * `רבי` and `רבנים`, but not `ערב` or `קרב`. Substring matching on a
 * two-letter Hebrew needle would pull in half the archive.
 */

export interface RuleContext {
  /** ISO timestamp treated as "now" for `within_days`; injectable for tests. */
  now?: string;
  /**
   * Contact ids the semantic model deems similar to a sentence. Only the
   * desktop has the model; without it the browser falls back to word overlap.
   */
  similar?: (sentence: string) => ReadonlySet<string> | null;
}

/** The pieces of text a field condition reads, already normalized. */
function fieldValues(field: CategoryRuleField, contact: ContactWithRelations): string[] {
  const norm = (values: Array<string | null | undefined>) =>
    values.map((value) => normalizeText(value)).filter((value) => value.length > 0);

  switch (field) {
    case 'name':
      return norm([contact.displayName, ...contact.aliases.map((alias) => alias.value)]);
    case 'occupation':
      return norm([contact.profession, contact.role, contact.title, contact.prefix]);
    case 'city':
      return norm([contact.city, contact.region]);
    case 'country':
      return contact.country ? [contact.country.toUpperCase()] : [];
    case 'organization':
      return norm(contact.organizations.map((link) => link.organization.name));
    case 'tag':
      return norm(contact.tags.map((tag) => tag.name));
    case 'specialty':
      return norm(contact.specialties);
    case 'notes':
      return norm([
        contact.notes,
        contact.reasonForSaving,
        contact.introducedBy,
        ...contact.contactNotes.map((note) => note.body),
      ]);
    case 'anywhere':
      return [
        ...fieldValues('name', contact),
        ...fieldValues('occupation', contact),
        ...fieldValues('city', contact),
        ...fieldValues('organization', contact),
        ...fieldValues('tag', contact),
        ...fieldValues('specialty', contact),
        ...fieldValues('notes', contact),
      ];
    case 'relationship':
      return contact.relationships.map((edge) => edge.type);
    case 'phone':
      return contact.phones.map((phone) => phone.raw);
    case 'email':
      return contact.emails.map((email) => email.address);
    case 'created':
    case 'meaning':
      return [];
    default:
      return [];
  }
}

/** Word-start containment: `needle` begins a word somewhere in `haystack`. */
export function containsWord(haystack: string, needle: string): boolean {
  if (!needle) return false;
  return (` ${haystack}`).includes(` ${needle}`);
}

function textMatches(
  op: CategoryRuleOperator,
  values: string[],
  needles: string[],
  exact: boolean,
): boolean {
  const hit = needles.some((needle) =>
    values.some((value) => (exact ? value === needle : containsWord(value, needle))),
  );
  return op === 'contains' || op === 'is' ? hit : !hit;
}

const DAY_MS = 24 * 60 * 60 * 1000;

/** Approximation of the semantic model for the browser demo: enough shared
 * content words between the sentence and the contact's text. */
function wordOverlap(sentence: string, contact: ContactWithRelations): boolean {
  const words = normalizeText(sentence)
    .split(' ')
    .filter((word) => word.length >= 3);
  if (words.length === 0) return false;
  const text = ` ${fieldValues('anywhere', contact).join(' ')} `;
  const hits = words.filter((word) => text.includes(` ${word}`)).length;
  return hits >= Math.min(2, words.length);
}

export function evaluateCondition(
  condition: CategoryCondition,
  contact: ContactWithRelations,
  context: RuleContext = {},
): boolean {
  const { field, op } = condition;
  const values = fieldValues(field, contact);

  switch (op) {
    case 'is_empty':
      return values.length === 0;
    case 'is_not_empty':
      return values.length > 0;
    case 'within_days': {
      const days = Number(condition.values[0]);
      if (!Number.isFinite(days)) return false;
      const now = context.now ? Date.parse(context.now) : Date.now();
      return now - Date.parse(contact.createdAt) <= days * DAY_MS;
    }
    case 'similar': {
      const sentence = condition.values[0] ?? '';
      const similar = context.similar?.(sentence);
      if (similar) return similar.has(contact.id);
      return wordOverlap(sentence, contact);
    }
    case 'contains':
    case 'not_contains':
    case 'is':
    case 'is_not': {
      const raw = field === 'country' ? condition.values.map((v) => v.toUpperCase()) : null;
      const needles =
        raw ??
        (field === 'relationship'
          ? condition.values
          : condition.values.map((value) => normalizeText(value)).filter(Boolean));
      const exact = op === 'is' || op === 'is_not';
      return textMatches(op, values, needles, exact);
    }
    default:
      return false;
  }
}

export function evaluateRule(
  rule: CategoryRule,
  contact: ContactWithRelations,
  context: RuleContext = {},
): boolean {
  if (rule.conditions.length === 0) return false;
  const results = rule.conditions.map((condition) =>
    evaluateCondition(condition, contact, context),
  );
  return rule.match === 'any' ? results.some(Boolean) : results.every(Boolean);
}

/** Hebrew labels shared by the editor, the rule summary and the tooltips. */
export const RULE_FIELD_LABELS: Record<CategoryRuleField, string> = {
  name: 'שם',
  occupation: 'מקצוע / תפקיד / תואר',
  city: 'עיר',
  country: 'מדינה',
  organization: 'מוסד',
  tag: 'תגית',
  specialty: 'התמחות',
  notes: 'הערות',
  anywhere: 'בכל מקום',
  relationship: 'קשר',
  phone: 'טלפון',
  email: 'אימייל',
  created: 'נוסף למאגר',
  meaning: 'לפי משמעות',
};

export const RULE_OPERATOR_LABELS: Record<CategoryRuleOperator, string> = {
  contains: 'מכיל',
  not_contains: 'לא מכיל',
  is: 'הוא',
  is_not: 'אינו',
  is_empty: 'ריק',
  is_not_empty: 'לא ריק',
  within_days: 'בימים האחרונים',
  similar: 'דומה ל־',
};

/** One-line Hebrew rendering of a rule, e.g. for the dashboard cards. */
export function describeRule(
  rule: CategoryRule | null,
  labels: { country?: (code: string) => string; relationship?: (type: string) => string } = {},
): string {
  if (!rule || rule.conditions.length === 0) return 'קטגוריה ידנית';
  const parts = rule.conditions.map((condition) => {
    const field = RULE_FIELD_LABELS[condition.field];
    const op = RULE_OPERATOR_LABELS[condition.op];
    const values = condition.values.map((value) => {
      if (condition.field === 'country') return labels.country?.(value) ?? value;
      if (condition.field === 'relationship') return labels.relationship?.(value) ?? value;
      return value;
    });
    if (condition.op === 'within_days') return `${field} ב־${values[0]} הימים האחרונים`;
    if (condition.op === 'similar') return `${field}: "${values[0]}"`;
    if (values.length === 0) return `${field} ${op}`;
    return `${field} ${op} ${values.join(' / ')}`;
  });
  return parts.join(rule.match === 'any' ? ' · או · ' : ' · וגם · ');
}
