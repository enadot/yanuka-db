import type { ContactSummary } from './contact.js';
import type { SyncableEntity, Ulid } from './primitives.js';

/**
 * Smart categories (ADR-038).
 *
 * A category is a shelf in the archive: "רבנים בחו"ל", "סופרי סת"ם". It can be
 * filled by hand, by a rule, or both — a rule describes who belongs, contacts
 * join and leave as their records change, and any single person can still be
 * pinned in or kept out by hand. The rule vocabulary is deliberately small
 * and reads as a sentence in the editor: *field · operator · values*.
 */

/** What a condition looks at. Several fields are composites, named for how
 * the user thinks rather than for the schema (`occupation` is profession,
 * role, title and prefix together). */
export const CATEGORY_RULE_FIELDS = [
  'name',
  'occupation',
  'city',
  'country',
  'organization',
  'tag',
  'specialty',
  'notes',
  'anywhere',
  'relationship',
  'phone',
  'email',
  'created',
  'meaning',
] as const;
export type CategoryRuleField = (typeof CATEGORY_RULE_FIELDS)[number];

/**
 * How a condition compares.
 *
 * `contains` is a *word-start* match on normalized text: `רב` finds `רב`,
 * `רבי` and `רבנים` but not `ערב` or `קרב`. `is` is a whole-value match.
 * Several values are alternatives (OR) within one condition.
 */
export const CATEGORY_RULE_OPERATORS = [
  'contains',
  'not_contains',
  'is',
  'is_not',
  'is_empty',
  'is_not_empty',
  'within_days',
  'similar',
] as const;
export type CategoryRuleOperator = (typeof CATEGORY_RULE_OPERATORS)[number];

const TEXT_OPERATORS: readonly CategoryRuleOperator[] = [
  'contains',
  'not_contains',
  'is',
  'is_not',
  'is_empty',
  'is_not_empty',
];

/** Operators each field accepts; the editor and the validator both read this. */
export const CATEGORY_FIELD_OPERATORS: Record<CategoryRuleField, readonly CategoryRuleOperator[]> =
  {
    name: TEXT_OPERATORS,
    occupation: TEXT_OPERATORS,
    city: TEXT_OPERATORS,
    country: ['is', 'is_not', 'is_empty', 'is_not_empty'],
    organization: TEXT_OPERATORS,
    tag: TEXT_OPERATORS,
    specialty: TEXT_OPERATORS,
    notes: TEXT_OPERATORS,
    anywhere: ['contains', 'not_contains'],
    relationship: ['is', 'is_not', 'is_empty', 'is_not_empty'],
    phone: ['is_empty', 'is_not_empty'],
    email: ['is_empty', 'is_not_empty'],
    created: ['within_days'],
    meaning: ['similar'],
  };

/** Operators that take no values. */
export const VALUELESS_OPERATORS: readonly CategoryRuleOperator[] = ['is_empty', 'is_not_empty'];

export interface CategoryCondition {
  field: CategoryRuleField;
  op: CategoryRuleOperator;
  /**
   * Alternatives, OR-ed. For `within_days` the single value is a number of
   * days; for `similar` it is the sentence the contact should resemble; for
   * `country` the values are ISO codes; for `relationship` they are
   * relationship types.
   */
  values: string[];
}

export interface CategoryRule {
  /** `all` — every condition must hold (AND); `any` — at least one (OR). */
  match: 'all' | 'any';
  conditions: CategoryCondition[];
}

export interface Category extends SyncableEntity {
  name: string;
  normalized: string;
  description: string | null;
  /** Categories may nest one level (e.g. `מוסדות` › `ישיבות`). */
  parentId: Ulid | null;
  /** Icon key from the application's fixed icon set; null shows the default. */
  icon: string | null;
  /** Hex colour, `#rrggbb`. */
  color: string | null;
  /** Who belongs automatically; null for a hand-filled category. */
  rule: CategoryRule | null;
  /** Position on the home screen and in the dashboard; lower first. */
  sortOrder: number;
  /** Offer the category as a tile on the home screen. */
  showOnHome: boolean;
}

/** A category with its live size, as the dashboard and the home tiles show it. */
export interface CategorySummary extends Category {
  count: number;
}

/** How a contact came to be in a category. */
export const CATEGORY_MEMBERSHIPS = ['rule', 'manual'] as const;
export type CategoryMembershipKind = (typeof CATEGORY_MEMBERSHIPS)[number];

/** A category as it appears on a contact's card. */
export interface CategoryMembership extends Category {
  membership: CategoryMembershipKind;
}

/** One row of a category's member list. */
export interface CategoryMember {
  contact: ContactSummary;
  membership: CategoryMembershipKind;
}

export interface CategoryMembersPage {
  items: CategoryMember[];
  total: number;
}

/** What a rule would select right now, shown live while it is being edited. */
export interface CategoryPreview {
  count: number;
  sample: ContactSummary[];
}

/**
 * A category the data suggests: a profession, tag or place that recurs often
 * enough to deserve a shelf and does not have one yet.
 */
export interface CategorySuggestion {
  name: string;
  description: string | null;
  icon: string | null;
  rule: CategoryRule;
  count: number;
}

/**
 * A manual override on one contact. `include` pins the contact in, `exclude`
 * keeps them out even when the rule matches, `auto` removes the override and
 * lets the rule decide.
 */
export const CATEGORY_MEMBERSHIP_MODES = ['include', 'exclude', 'auto'] as const;
export type CategoryMembershipMode = (typeof CATEGORY_MEMBERSHIP_MODES)[number];
