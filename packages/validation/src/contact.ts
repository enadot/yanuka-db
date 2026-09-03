import { z } from 'zod';
import {
  ALIAS_KINDS,
  EMAIL_KINDS,
  ORGANIZATION_KINDS,
  PHONE_KINDS,
  RELATIONSHIP_TYPES,
} from '@yanuka/types';
import {
  CountryCodeSchema,
  IsoDateTimeSchema,
  LanguageCodeSchema,
  optionalText,
  requiredText,
  UlidSchema,
} from './common.js';

export const PhoneInputSchema = z.object({
  id: UlidSchema.optional(),
  kind: z.enum(PHONE_KINDS).default('mobile'),
  // Deliberately permissive. Numbers copied out of decades-old notebooks are
  // partial, mistyped, or annotated ("02-6521234 שלוחה 4"). Refusing to store
  // them would lose the only trace of that person, so anything non-empty is
  // accepted and normalization is best-effort — see @yanuka/utils normalizePhone.
  raw: requiredText(60, 'יש להזין מספר טלפון'),
  label: optionalText(60),
  isPrimary: z.boolean().default(false),
});

export const EmailInputSchema = z.object({
  id: UlidSchema.optional(),
  kind: z.enum(EMAIL_KINDS).default('personal'),
  address: z.string().trim().email('כתובת אימייל אינה תקינה').max(200),
  isPrimary: z.boolean().default(false),
});

export const AliasInputSchema = z.object({
  id: UlidSchema.optional(),
  kind: z.enum(ALIAS_KINDS).default('alias'),
  value: requiredText(200, 'יש להזין שם'),
  languageCode: LanguageCodeSchema.nullable().default(null),
});

export const OrganizationLinkInputSchema = z.object({
  organizationId: UlidSchema,
  role: optionalText(120),
  isPrimary: z.boolean().default(false),
  startedAt: IsoDateTimeSchema.nullable().default(null),
  endedAt: IsoDateTimeSchema.nullable().default(null),
});

/**
 * Fields a user can edit on a contact.
 *
 * Only `displayName` is required. The product exists to capture half-remembered
 * people; demanding a structured first/last name, a phone or a city at creation
 * time would push exactly the records that matter most out of the database.
 */
export const ContactInputSchema = z.object({
  firstName: optionalText(100),
  lastName: optionalText(100),
  displayName: requiredText(200, 'יש להזין שם'),
  prefix: optionalText(40),
  title: optionalText(40),

  country: CountryCodeSchema.nullable().default(null),
  region: optionalText(120),
  city: optionalText(120),
  address: optionalText(300),
  postalCode: optionalText(20),

  profession: optionalText(120),
  role: optionalText(120),

  notes: optionalText(10_000),
  reasonForSaving: optionalText(2_000),
  source: optionalText(200),
  introducedBy: optionalText(200),
  introducedByContactId: UlidSchema.nullable().default(null),

  isFavorite: z.boolean().default(false),

  phones: z.array(PhoneInputSchema).max(20).default([]),
  emails: z.array(EmailInputSchema).max(20).default([]),
  aliases: z.array(AliasInputSchema).max(30).default([]),
  specialties: z.array(requiredText(100)).max(30).default([]),
  languages: z.array(LanguageCodeSchema).max(15).default([]),
  tagIds: z.array(UlidSchema).max(50).default([]),
  categoryIds: z.array(UlidSchema).max(20).default([]),
  organizations: z.array(OrganizationLinkInputSchema).max(20).default([]),
});

export type ContactInput = z.infer<typeof ContactInputSchema>;

export const CreateContactSchema = z.object({
  /** Client-minted so a contact can be created offline and retried idempotently. */
  id: UlidSchema.optional(),
  data: ContactInputSchema,
});

export const UpdateContactSchema = z.object({
  id: UlidSchema,
  /** Partial patch: only the fields the form actually changed. */
  data: ContactInputSchema.partial(),
  /**
   * Version the edit was based on. The repository rejects a write whose base
   * version is stale, which is what turns a lost update into a visible conflict.
   */
  baseVersion: z.number().int().nonnegative().optional(),
});

/**
 * The minimal form behind "Quick Add" — name, one phone, one remark.
 * Everything else can be filled in later.
 */
export const QuickAddContactSchema = z.object({
  displayName: requiredText(200, 'יש להזין שם'),
  phone: z.string().trim().max(60).optional(),
  notes: z.string().trim().max(10_000).optional(),
});

export const TagInputSchema = z.object({
  name: requiredText(60, 'יש להזין שם תגית'),
  color: z
    .string()
    .regex(/^#[0-9a-fA-F]{6}$/, 'צבע חייב להיות בפורמט hex')
    .nullable()
    .default(null),
  description: optionalText(300),
});

export const OrganizationInputSchema = z.object({
  name: requiredText(200, 'יש להזין שם מוסד'),
  kind: z.enum(ORGANIZATION_KINDS).default('organization'),
  city: optionalText(120),
  region: optionalText(120),
  country: CountryCodeSchema.nullable().default(null),
  address: optionalText(300),
  notes: optionalText(2_000),
});

export const RelationshipInputSchema = z
  .object({
    fromContactId: UlidSchema,
    toContactId: UlidSchema,
    type: z.enum(RELATIONSHIP_TYPES),
    notes: optionalText(1_000),
  })
  .refine((value) => value.fromContactId !== value.toContactId, {
    message: 'לא ניתן לקשר איש קשר לעצמו',
    path: ['toContactId'],
  });

export const NoteInputSchema = z.object({
  contactId: UlidSchema,
  body: requiredText(10_000, 'יש להזין תוכן להערה'),
  isSensitive: z.boolean().default(false),
});
