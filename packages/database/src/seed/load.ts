import type {
  Category,
  ContactAlias,
  ContactEmail,
  ContactOrganization,
  ContactPhone,
  ContactSummary,
  ContactWithRelations,
  Organization,
  Relationship,
  Tag,
  Ulid,
} from '@yanuka/types';
import { normalizePhone } from '@yanuka/utils';
import { SEED_DATASET } from './dataset.js';
import type { SeedContact, SeedDataset } from './types.js';

/** Crockford base32, as used by ULID. */
const CROCKFORD = '0123456789ABCDEFGHJKMNPQRSTVWXYZ';

/**
 * Derive a stable, ULID-shaped identifier from a seed key.
 *
 * The demo data must produce the same IDs on every load, or a contact's
 * bookmarked URL would break between restarts and the golden search fixtures
 * could not reference records by id. Real records use `newId()`; this exists
 * only so the fixture data is reproducible.
 */
export function deterministicId(key: string): Ulid {
  // FNV-1a, 64-bit, run over the key twice with different offsets to fill 26
  // characters without an external hash dependency.
  const hash = (input: string, offset: bigint): bigint => {
    let value = offset;
    const prime = 1099511628211n;
    const mask = (1n << 64n) - 1n;
    for (let i = 0; i < input.length; i += 1) {
      value = (value ^ BigInt(input.charCodeAt(i))) & mask;
      value = (value * prime) & mask;
    }
    return value;
  };

  const a = hash(key, 14695981039346656037n);
  const b = hash(`${key}:2`, 14695981039346656037n);

  let out = '';
  let bits = (a << 64n) | b;
  for (let i = 0; i < 26; i += 1) {
    out = CROCKFORD[Number(bits & 31n)] + out;
    bits >>= 5n;
  }

  // The first ULID character encodes the high bits of the timestamp and must
  // be 0-7 for the value to be a legal 48-bit time.
  return `${CROCKFORD[Number(out.charCodeAt(0)) % 8]}${out.slice(1)}`;
}

/** Fixed instant so the demo data has stable, sensible timestamps. */
const SEEDED_AT = '2026-01-15T09:00:00.000Z';

function envelope(id: Ulid, createdAt = SEEDED_AT) {
  return {
    id,
    createdAt,
    updatedAt: createdAt,
    createdBy: null,
    updatedBy: null,
    version: 1,
    deviceId: 'seed',
    deletedAt: null,
  };
}

export interface LoadedSeed {
  tags: Tag[];
  categories: Category[];
  organizations: Organization[];
  contacts: ContactWithRelations[];
  relationships: Relationship[];
}

/**
 * Expand the compact seed dataset into full entity records.
 *
 * Cross-references in the source file are written as human-readable keys; this
 * resolves them into IDs and materializes both directions of every
 * relationship so a contact detail screen can show incoming edges
 * ("הרב X → המליץ עליו") without a second query.
 */
export function loadSeed(dataset: SeedDataset = SEED_DATASET): LoadedSeed {
  const normalizeKey = (value: string) => value.trim();

  const tags: Tag[] = dataset.tags.map((tag) => ({
    ...envelope(deterministicId(`tag:${tag.name}`)),
    name: tag.name,
    normalized: tag.name,
    color: tag.color ?? null,
    description: null,
  }));
  const tagByName = new Map(tags.map((tag) => [tag.name, tag]));

  const categories: Category[] = dataset.categories.map((category) => ({
    ...envelope(deterministicId(`category:${category.name}`)),
    name: category.name,
    normalized: category.name,
    description: category.description ?? null,
    parentId: null,
  }));
  const categoryByName = new Map(categories.map((category) => [category.name, category]));

  const organizations: Organization[] = dataset.organizations.map((org) => ({
    ...envelope(deterministicId(`org:${org.key}`)),
    name: org.name,
    normalized: org.name,
    kind: org.kind,
    city: org.city ?? null,
    region: null,
    country: org.country ?? null,
    address: null,
    notes: org.notes ?? null,
  }));
  const orgByKey = new Map(
    dataset.organizations.map((org, index) => [org.key, organizations[index]!]),
  );

  const contactIdByKey = new Map(
    dataset.contacts.map((contact) => [contact.key, deterministicId(`contact:${contact.key}`)]),
  );

  const summaries = new Map<Ulid, ContactSummary>();

  const buildBase = (seed: SeedContact): ContactWithRelations => {
    const id = contactIdByKey.get(seed.key)!;
    const base = envelope(id);

    const phones: ContactPhone[] = (seed.phones ?? []).map((phone, index) => {
      // The seed stores what was written down, then runs it through the very
      // same normalizer a typed-in number goes through. Two of the fixtures are
      // deliberately unparseable, and they must degrade here exactly as they
      // would in the form — otherwise the demo data would be easier than
      // reality and would hide bugs.
      const normalized = normalizePhone(phone.raw, seed.country ?? null);
      return {
        ...envelope(deterministicId(`phone:${seed.key}:${index}`)),
        contactId: id,
        kind: phone.kind,
        raw: normalized.raw,
        e164: normalized.e164,
        digits: normalized.digits,
        countryCode: normalized.countryCode ?? seed.country ?? null,
        isPrimary: phone.isPrimary ?? index === 0,
        label: phone.label ?? null,
      };
    });

    const emails: ContactEmail[] = (seed.emails ?? []).map((email, index) => ({
      ...envelope(deterministicId(`email:${seed.key}:${index}`)),
      contactId: id,
      kind: email.kind,
      address: email.address,
      normalized: email.address.toLowerCase(),
      isPrimary: email.isPrimary ?? index === 0,
    }));

    const aliases: ContactAlias[] = (seed.aliases ?? []).map((alias, index) => ({
      ...envelope(deterministicId(`alias:${seed.key}:${index}`)),
      contactId: id,
      kind: alias.kind,
      value: alias.value,
      normalized: alias.value,
      languageCode: alias.languageCode ?? null,
    }));

    const contactTags = (seed.tags ?? [])
      .map((name) => tagByName.get(normalizeKey(name)))
      .filter((tag): tag is Tag => tag != null);

    const contactCategories = (seed.categories ?? [])
      .map((name) => categoryByName.get(normalizeKey(name)))
      .filter((category): category is Category => category != null);

    const organizationLinks: Array<ContactOrganization & { organization: Organization }> = [];
    (seed.organizations ?? []).forEach((link, index) => {
      const organization = orgByKey.get(link.key);
      // A dangling organization key means the dataset references one that was
      // renamed. The seed test asserts none exist; skipping keeps it loadable.
      if (!organization) return;
      organizationLinks.push({
        ...envelope(deterministicId(`contactorg:${seed.key}:${index}`)),
        contactId: id,
        organizationId: organization.id,
        role: link.role ?? null,
        isPrimary: link.isPrimary ?? index === 0,
        startedAt: null,
        endedAt: null,
        organization,
      });
    });

    const contact: ContactWithRelations = {
      ...base,
      firstName: seed.firstName ?? null,
      lastName: seed.lastName ?? null,
      displayName: seed.displayName,
      prefix: seed.prefix ?? null,
      title: seed.title ?? null,
      country: seed.country ?? null,
      region: seed.region ?? null,
      city: seed.city ?? null,
      address: seed.address ?? null,
      postalCode: null,
      profession: seed.profession ?? null,
      role: seed.role ?? null,
      notes: seed.notes ?? null,
      reasonForSaving: seed.reasonForSaving ?? null,
      source: seed.source ?? null,
      introducedBy: seed.introducedBy ?? null,
      introducedByContactId: null,
      isFavorite: seed.isFavorite ?? false,
      lastViewedAt: null,
      phones,
      emails,
      aliases,
      tags: contactTags,
      categories: contactCategories,
      specialties: seed.specialties ?? [],
      languages: seed.languages ?? [],
      organizations: organizationLinks,
      relationships: [],
      contactNotes: [],
    };

    summaries.set(id, {
      id,
      displayName: contact.displayName,
      prefix: contact.prefix,
      profession: contact.profession,
      role: contact.role,
      city: contact.city,
      country: contact.country,
      primaryPhone: phones.find((phone) => phone.isPrimary)?.raw ?? phones[0]?.raw ?? null,
      tags: contactTags.map((tag) => tag.name),
      isFavorite: contact.isFavorite,
      updatedAt: contact.updatedAt,
    });

    return contact;
  };

  const contacts = dataset.contacts.map(buildBase);
  const contactById = new Map(contacts.map((contact) => [contact.id, contact]));

  const relationships: Relationship[] = [];
  dataset.relationships.forEach((seed, index) => {
    const fromId = contactIdByKey.get(seed.from);
    const toId = contactIdByKey.get(seed.to);
    // A dangling key means the dataset references a contact that was renamed or
    // removed. Skipping keeps the demo loadable; the seed test asserts none exist.
    if (!fromId || !toId || fromId === toId) return;

    const relationship: Relationship = {
      ...envelope(deterministicId(`rel:${seed.from}:${seed.to}:${seed.type}:${index}`)),
      fromContactId: fromId,
      toContactId: toId,
      type: seed.type,
      notes: seed.notes ?? null,
    };
    relationships.push(relationship);

    const from = contactById.get(fromId);
    const to = contactById.get(toId);
    if (from && summaries.has(toId)) {
      from.relationships.push({ ...relationship, otherContact: summaries.get(toId)!, direction: 'out' });
    }
    if (to && summaries.has(fromId)) {
      to.relationships.push({ ...relationship, otherContact: summaries.get(fromId)!, direction: 'in' });
    }
  });

  // Link the free-text `introducedBy` to a real record where the name matches,
  // so the detail screen can offer a link rather than plain text.
  for (const contact of contacts) {
    if (!contact.introducedBy) continue;
    const match = contacts.find(
      (candidate) =>
        candidate.id !== contact.id &&
        (candidate.displayName === contact.introducedBy ||
          `${candidate.prefix ?? ''} ${candidate.displayName}`.trim() === contact.introducedBy),
    );
    if (match) contact.introducedByContactId = match.id;
  }

  return { tags, categories, organizations, contacts, relationships };
}

/** Convenience accessor for the summary projection of every seeded contact. */
export function seedSummaries(seed: LoadedSeed = loadSeed()): ContactSummary[] {
  return seed.contacts.map((contact) => ({
    id: contact.id,
    displayName: contact.displayName,
    prefix: contact.prefix,
    profession: contact.profession,
    role: contact.role,
    city: contact.city,
    country: contact.country,
    primaryPhone:
      contact.phones.find((phone) => phone.isPrimary)?.raw ?? contact.phones[0]?.raw ?? null,
    tags: contact.tags.map((tag) => tag.name),
    isFavorite: contact.isFavorite,
    updatedAt: contact.updatedAt,
  }));
}
