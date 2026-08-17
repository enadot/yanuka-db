import { normalizeName, normalizeText, phoneticKey } from '@yanuka/search';
import { digitsOnly, phoneSearchKey, similarity } from '@yanuka/utils';
import type { ContactSummary, ContactWithRelations } from '@yanuka/types';
import type { ContactInput, DuplicateCandidate } from './repository.js';

/**
 * Derive the name shown everywhere from whatever parts were filled in.
 *
 * Falls back through first+last, then either alone, because the product accepts
 * a contact with almost nothing recorded — a person remembered only as
 * "the electrician from Antwerp" is still worth keeping.
 */
export function deriveDisplayName(input: {
  displayName?: string | null;
  firstName?: string | null;
  lastName?: string | null;
}): string {
  const explicit = input.displayName?.trim();
  if (explicit) return explicit;

  const parts = [input.firstName?.trim(), input.lastName?.trim()].filter(Boolean);
  if (parts.length > 0) return parts.join(' ');

  return 'ללא שם';
}

/** Name with its honorific, as displayed in headers. */
export function formatFullName(contact: {
  prefix?: string | null;
  displayName: string;
  title?: string | null;
}): string {
  return [contact.prefix, contact.displayName, contact.title].filter(Boolean).join(' ');
}

/** One-line subtitle: profession, role and place, whichever exist. */
export function formatSubtitle(contact: {
  profession?: string | null;
  role?: string | null;
  city?: string | null;
  country?: string | null;
}): string {
  const occupation = [contact.profession, contact.role].filter(Boolean).join(' | ');
  const place = [contact.city, contact.country].filter(Boolean).join(', ');
  return [occupation, place].filter(Boolean).join(' · ');
}

/** Initials for the avatar fallback, script-aware. */
export function initials(displayName: string): string {
  const tokens = normalizeText(displayName).split(' ').filter(Boolean);
  if (tokens.length === 0) return '?';
  if (tokens.length === 1) return tokens[0]!.slice(0, 2);
  return `${tokens[0]![0] ?? ''}${tokens[1]![0] ?? ''}`;
}

/**
 * Weights for the duplicate heuristic.
 *
 * A shared phone number is close to proof; a shared name is barely a hint,
 * because `כהן` and `Cohen` are everywhere in this dataset. The thresholds are
 * tuned to warn rather than block — the user always decides.
 */
const DUPLICATE_WEIGHTS = {
  samePhone: 0.75,
  sameEmail: 0.7,
  sameNormalizedName: 0.35,
  samePhoneticName: 0.2,
  sameCity: 0.15,
  sameProfession: 0.1,
} as const;

/** Below this a candidate is not worth interrupting the user about. */
export const DUPLICATE_THRESHOLD = 0.5;

export interface DuplicateSubject {
  summary: ContactSummary;
  normalizedName: string;
  phoneticName: string;
  phoneKeys: string[];
  emails: string[];
  city: string | null;
  profession: string | null;
}

/** Project a stored contact into the shape the duplicate check compares. */
export function toDuplicateSubject(contact: ContactWithRelations): DuplicateSubject {
  return {
    summary: toSummary(contact),
    normalizedName: normalizeName(contact.displayName),
    phoneticName: phoneticKey(contact.displayName),
    phoneKeys: contact.phones.map((phone) => phoneSearchKey(phone.digits || phone.raw)),
    emails: contact.emails.map((email) => email.normalized),
    city: contact.city ? normalizeText(contact.city) : null,
    profession: contact.profession ? normalizeText(contact.profession) : null,
  };
}

export function toSummary(contact: ContactWithRelations): ContactSummary {
  const primaryPhone =
    contact.phones.find((phone) => phone.isPrimary) ?? contact.phones[0] ?? null;

  return {
    id: contact.id,
    displayName: contact.displayName,
    prefix: contact.prefix,
    profession: contact.profession,
    role: contact.role,
    city: contact.city,
    country: contact.country,
    // The raw form is shown, not E.164: it is how the user wrote the number
    // down, and for historical entries it often carries information the
    // normalized form drops ("02-6521234 שלוחה 4").
    primaryPhone: primaryPhone ? primaryPhone.raw : null,
    tags: contact.tags.map((tag) => tag.name),
    isFavorite: contact.isFavorite,
    updatedAt: contact.updatedAt,
  };
}

/**
 * Score how likely two records describe the same person.
 *
 * Deliberately additive and capped rather than probabilistic: the output is
 * shown to a human as "this may already exist", so it needs to be explainable,
 * not calibrated.
 */
export function scoreDuplicate(
  input: Partial<ContactInput>,
  existing: DuplicateSubject,
): DuplicateCandidate | null {
  const reasons: string[] = [];
  let score = 0;

  const inputPhoneKeys = (input.phones ?? [])
    .map((phone) => phoneSearchKey(digitsOnly(phone.raw)))
    .filter((key) => key.length >= 6);
  if (inputPhoneKeys.some((key) => existing.phoneKeys.includes(key))) {
    score += DUPLICATE_WEIGHTS.samePhone;
    reasons.push('אותו מספר טלפון');
  }

  const inputEmails = (input.emails ?? []).map((email) => email.address.trim().toLowerCase());
  if (inputEmails.some((email) => existing.emails.includes(email))) {
    score += DUPLICATE_WEIGHTS.sameEmail;
    reasons.push('אותה כתובת אימייל');
  }

  const inputName = normalizeName(deriveDisplayName(input));
  if (inputName && inputName === existing.normalizedName) {
    score += DUPLICATE_WEIGHTS.sameNormalizedName;
    reasons.push('שם זהה');
  } else if (inputName && similarity(inputName, existing.normalizedName) >= 0.85) {
    score += DUPLICATE_WEIGHTS.sameNormalizedName * 0.7;
    reasons.push('שם דומה מאוד');
  } else if (inputName && phoneticKey(inputName) === existing.phoneticName) {
    score += DUPLICATE_WEIGHTS.samePhoneticName;
    reasons.push('שם בהגייה זהה');
  }

  // Place and occupation are corroborating signals only. On their own they say
  // nothing — half the database is a rabbi in Jerusalem.
  if (reasons.length > 0) {
    if (input.city && existing.city === normalizeText(input.city)) {
      score += DUPLICATE_WEIGHTS.sameCity;
      reasons.push('אותה עיר');
    }
    if (input.profession && existing.profession === normalizeText(input.profession)) {
      score += DUPLICATE_WEIGHTS.sameProfession;
      reasons.push('אותו מקצוע');
    }
  }

  const confidence = Math.min(1, score);
  if (confidence < DUPLICATE_THRESHOLD) return null;

  return { contact: existing.summary, confidence, reasons };
}
