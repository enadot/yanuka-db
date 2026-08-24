import type { CountryCode, IsoDateTime, LanguageCode, SyncableEntity, Ulid } from './primitives.js';

/** How a phone number is used. Drives the call/WhatsApp affordances in the UI. */
export const PHONE_KINDS = [
  'mobile',
  'office',
  'home',
  'whatsapp',
  'fax',
  'assistant',
  'other',
] as const;
export type PhoneKind = (typeof PHONE_KINDS)[number];

export const EMAIL_KINDS = ['personal', 'work', 'other'] as const;
export type EmailKind = (typeof EMAIL_KINDS)[number];

/**
 * Why an alternate name exists. A single person may be recorded as
 * `ר' משה כהן`, `הרב משה כהן`, `Moshe Cohen` and `Moishe Cohen` — searching any
 * of them must reach the same contact. See docs/SEARCH.md.
 */
export const ALIAS_KINDS = [
  'alias',
  'nickname',
  'maiden',
  'transliteration',
  'formal',
  'former',
  'other',
] as const;
export type AliasKind = (typeof ALIAS_KINDS)[number];

export const ORGANIZATION_KINDS = [
  'organization',
  'institution',
  'community',
  'synagogue',
  'business',
  'yeshiva',
  'kollel',
  'charity',
  'other',
] as const;
export type OrganizationKind = (typeof ORGANIZATION_KINDS)[number];

/**
 * Directed relationship types. Every type has an inverse so the graph can be
 * traversed and rendered from either endpoint — see `RELATIONSHIP_INVERSES`.
 */
export const RELATIONSHIP_TYPES = [
  'recommended',
  'knows',
  'related_to',
  'works_with',
  'family_of',
  'referred_us_to',
  'member_of',
  'student_of',
  'teacher_of',
] as const;
export type RelationshipType = (typeof RELATIONSHIP_TYPES)[number];

/**
 * Inverse of each relationship type, used to present an edge from the far end.
 * `knows`, `related_to`, `works_with` and `family_of` are symmetric.
 */
export const RELATIONSHIP_INVERSES: Record<RelationshipType, RelationshipType> = {
  recommended: 'recommended',
  knows: 'knows',
  related_to: 'related_to',
  works_with: 'works_with',
  family_of: 'family_of',
  referred_us_to: 'referred_us_to',
  member_of: 'member_of',
  student_of: 'teacher_of',
  teacher_of: 'student_of',
};

export interface ContactPhone extends SyncableEntity {
  contactId: Ulid;
  kind: PhoneKind;
  /** Exactly as the user typed it. Never rewritten — the original is evidence. */
  raw: string;
  /** E.164 when parseable (`+972541234567`), otherwise null. */
  e164: string | null;
  /** Digits only, used for format-insensitive suffix matching. */
  digits: string;
  countryCode: CountryCode | null;
  isPrimary: boolean;
  label: string | null;
}

export interface ContactEmail extends SyncableEntity {
  contactId: Ulid;
  kind: EmailKind;
  address: string;
  /** Lower-cased address used for exact-match lookup. */
  normalized: string;
  isPrimary: boolean;
}

export interface ContactAlias extends SyncableEntity {
  contactId: Ulid;
  kind: AliasKind;
  value: string;
  /** Output of the Hebrew-aware normalizer — see @yanuka/search. */
  normalized: string;
  languageCode: LanguageCode | null;
}

export interface Tag extends SyncableEntity {
  name: string;
  normalized: string;
  color: string | null;
  description: string | null;
}

export interface Category extends SyncableEntity {
  name: string;
  normalized: string;
  description: string | null;
  /** Categories may nest one level (e.g. `מוסדות` › `ישיבות`). */
  parentId: Ulid | null;
}

export interface Organization extends SyncableEntity {
  name: string;
  normalized: string;
  kind: OrganizationKind;
  city: string | null;
  region: string | null;
  country: CountryCode | null;
  address: string | null;
  notes: string | null;
}

/** Membership edge between a contact and an organization. */
export interface ContactOrganization extends SyncableEntity {
  contactId: Ulid;
  organizationId: Ulid;
  /** Position held, e.g. `ראש ישיבה`, `גבאי`, `מנכ"ל`. */
  role: string | null;
  isPrimary: boolean;
  startedAt: IsoDateTime | null;
  endedAt: IsoDateTime | null;
}

export interface Relationship extends SyncableEntity {
  fromContactId: Ulid;
  toContactId: Ulid;
  type: RelationshipType;
  notes: string | null;
}

export interface Note extends SyncableEntity {
  contactId: Ulid;
  body: string;
  /**
   * Sensitive notes are hidden unless the viewer holds `contacts:view_sensitive`.
   * See docs/SECURITY.md.
   */
  isSensitive: boolean;
  authorId: Ulid | null;
}

/**
 * The central entity. Deliberately richer than a phone book row: most of the
 * value of this database lives in `notes`, `reasonForSaving`, `specialties` and
 * `introducedBy`, because those are what the user actually remembers when the
 * name is gone.
 */
export interface Contact extends SyncableEntity {
  firstName: string | null;
  lastName: string | null;
  /** Rendered name. Derived from first/last when not explicitly set. */
  displayName: string;
  /** Honorific placed before the name, e.g. `הרב`, `ר'`, `ד"ר`. */
  prefix: string | null;
  /** Title placed after the name, e.g. `שליט"א`, `ז"ל`. */
  title: string | null;

  country: CountryCode | null;
  region: string | null;
  city: string | null;
  address: string | null;
  postalCode: string | null;

  profession: string | null;
  role: string | null;

  /** Free text the user wrote about this person. The highest-value field. */
  notes: string | null;
  /** Why this contact was kept — often the only thing the user recalls. */
  reasonForSaving: string | null;
  /** Where the record came from, e.g. `מחברת 1998`, `ייבוא CSV`. */
  source: string | null;
  /** Free-text name of whoever made the introduction. */
  introducedBy: string | null;
  /** Structured counterpart of `introducedBy` when that person is in the DB. */
  introducedByContactId: Ulid | null;

  isFavorite: boolean;
  lastViewedAt: IsoDateTime | null;
}

/** A contact joined with every child collection — what the detail screen renders. */
export interface ContactWithRelations extends Contact {
  phones: ContactPhone[];
  emails: ContactEmail[];
  aliases: ContactAlias[];
  tags: Tag[];
  categories: Category[];
  specialties: string[];
  languages: LanguageCode[];
  organizations: Array<ContactOrganization & { organization: Organization }>;
  relationships: Array<Relationship & { otherContact: ContactSummary; direction: 'out' | 'in' }>;
  contactNotes: Note[];
}

/** Lightweight projection used in lists, search results and relationship edges. */
/**
 * A soft-deleted contact, as the recycle bin lists it.
 *
 * `deletedAt` is carried explicitly rather than read off `updatedAt`: the two
 * happen to coincide today because a delete touches both, and a screen that
 * quietly depends on that would start showing wrong dates the first time
 * anything else writes to a deleted row.
 */
export interface DeletedContact {
  contact: ContactSummary;
  deletedAt: IsoDateTime;
}

export interface ContactSummary {
  id: Ulid;
  displayName: string;
  prefix: string | null;
  profession: string | null;
  role: string | null;
  city: string | null;
  country: CountryCode | null;
  primaryPhone: string | null;
  tags: string[];
  isFavorite: boolean;
  updatedAt: IsoDateTime;
}
