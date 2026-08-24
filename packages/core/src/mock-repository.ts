import { loadSeed, type LoadedSeed } from '@yanuka/database';
import {
  buildIndex,
  search as runSearch,
  suggest as runSuggest,
  normalizeName,
  normalizeText,
  type SearchableRecord,
} from '@yanuka/search';
import type {
  AuditLogEntry,
  Category,
  ContactSummary,
  ContactWithRelations,
  DeletedContact,
  FacetField,
  Note,
  Organization,
  Relationship,
  SearchResponse,
  SearchSuggestion,
  Tag,
  Ulid,
} from '@yanuka/types';
import { newId, nowIso, normalizePhone } from '@yanuka/utils';
import { RepositoryError } from './errors.js';
import {
  deriveDisplayName,
  scoreDuplicate,
  toDuplicateSubject,
  toSummary,
} from './contact-logic.js';
import type {
  CategoryInput,
  ContactInput,
  ContactsRepository,
  DatabaseStats,
  DuplicateCandidate,
  DuplicatePair,
  ListContactsInput,
  NoteInput,
  OrganizationInput,
  Page,
  QuickAddInput,
  RelationshipInput,
  SearchInput,
  TagInput,
} from './repository.js';

/**
 * In-memory implementation of the repository contract.
 *
 * This is not a stub. It runs the real search engine from @yanuka/search over
 * the real demo dataset, which means the browser build of the desktop app is a
 * working application — usable for design review and end-to-end UI tests in
 * environments where a Tauri shell cannot be built.
 *
 * It is also the reference the SQLite implementation is checked against: both
 * pass `runRepositoryContractTests`.
 */
export class MockRepository implements ContactsRepository {
  private contacts: ContactWithRelations[];
  private tags: Tag[];
  private categories: Category[];
  private organizations: Organization[];
  private relationships: Relationship[];
  private audit: AuditLogEntry[] = [];
  private pendingMutations = 0;

  /** Artificial latency so loading and empty states are exercised in the UI. */
  constructor(
    seed: LoadedSeed = loadSeed(),
    private readonly latencyMs = 0,
  ) {
    this.contacts = seed.contacts;
    this.tags = seed.tags;
    this.categories = seed.categories;
    this.organizations = seed.organizations;
    this.relationships = seed.relationships;
  }

  private async tick(): Promise<void> {
    if (this.latencyMs > 0) {
      await new Promise((resolve) => setTimeout(resolve, this.latencyMs));
    }
  }

  private live(): ContactWithRelations[] {
    return this.contacts.filter((contact) => contact.deletedAt == null);
  }

  private toRecord(contact: ContactWithRelations): SearchableRecord {
    const facetValues: Partial<Record<FacetField, string[]>> = {
      country: contact.country ? [contact.country] : [],
      city: contact.city ? [contact.city] : [],
      profession: contact.profession ? [contact.profession] : [],
      specialty: contact.specialties,
      tag: contact.tags.map((tag) => tag.name),
      category: contact.categories.map((category) => category.name),
      organization: contact.organizations.map((link) => link.organization.name),
      language: contact.languages,
    };

    return {
      summary: toSummary(contact),
      lastViewedAt: contact.lastViewedAt,
      facetValues,
      indexable: {
        id: contact.id,
        displayName: contact.displayName,
        prefix: contact.prefix,
        firstName: contact.firstName,
        lastName: contact.lastName,
        aliases: contact.aliases.map((alias) => alias.value),
        profession: contact.profession,
        role: contact.role,
        specialties: contact.specialties,
        organizations: contact.organizations.map((link) => link.organization.name),
        city: contact.city,
        region: contact.region,
        country: contact.country,
        tags: contact.tags.map((tag) => tag.name),
        categories: contact.categories.map((category) => category.name),
        notes: [contact.notes, ...contact.contactNotes.map((note) => note.body)]
          .filter(Boolean)
          .join('\n'),
        reasonForSaving: contact.reasonForSaving,
        introducedBy: contact.introducedBy,
        phoneDigits: contact.phones.map((phone) => phone.digits),
        emails: contact.emails.map((email) => email.address),
      },
    };
  }

  private index(includeDeleted = false) {
    const source = includeDeleted ? this.contacts : this.live();
    return buildIndex(source.map((contact) => this.toRecord(contact)));
  }

  private require(id: Ulid): ContactWithRelations {
    const contact = this.contacts.find((candidate) => candidate.id === id);
    if (!contact) throw RepositoryError.notFound('איש הקשר');
    return contact;
  }

  private record(action: AuditLogEntry['action'], contact: ContactWithRelations): void {
    this.audit.unshift({
      id: newId(),
      userId: null,
      userDisplayName: null,
      action,
      entityType: 'contact',
      entityId: contact.id,
      entityLabel: contact.displayName,
      changes: null,
      deviceId: 'browser',
      deviceName: 'דפדפן',
      createdAt: nowIso(),
    });
    this.pendingMutations += 1;
  }

  // -- reads ---------------------------------------------------------------

  async search(input: SearchInput): Promise<SearchResponse> {
    await this.tick();
    return runSearch(this.index(input.includeDeleted), input);
  }

  async suggest(text: string, limit = 8): Promise<SearchSuggestion[]> {
    await this.tick();
    return runSuggest(this.index(), text, limit);
  }

  async listContacts(input: ListContactsInput): Promise<Page<ContactSummary>> {
    await this.tick();

    let rows = input.includeDeleted ? this.contacts : this.live();
    if (input.favoritesOnly) rows = rows.filter((contact) => contact.isFavorite);

    if (input.startsWith) {
      const prefix = normalizeName(input.startsWith);
      rows = rows.filter((contact) => normalizeName(contact.displayName).startsWith(prefix));
    }

    if (input.filters) {
      for (const [rawField, values] of Object.entries(input.filters)) {
        if (!values || values.length === 0) continue;
        const facetField = rawField as FacetField;
        rows = rows.filter((contact) => {
          const record = this.toRecord(contact);
          const actual = record.facetValues[facetField] ?? [];
          return values.some((value) => actual.includes(value));
        });
      }
    }

    const sorted = [...rows].sort((a, b) => {
      switch (input.sort) {
        case 'recently_updated':
          return b.updatedAt.localeCompare(a.updatedAt);
        case 'recently_added':
          return b.createdAt.localeCompare(a.createdAt);
        case 'name':
        default:
          // Sorting by the normalized name puts `הרב משה` next to `משה`, which
          // is what a reader scanning an alphabetical list expects.
          return (
            normalizeName(a.displayName).localeCompare(normalizeName(b.displayName), 'he') ||
            a.id.localeCompare(b.id)
          );
      }
    });

    // Keyset pagination: the cursor is the last id of the previous page, so
    // page N+1 costs the same as page 1 regardless of how deep it is.
    const start = input.cursor ? sorted.findIndex((contact) => contact.id === input.cursor) + 1 : 0;
    const slice = sorted.slice(start, start + input.limit);
    const nextIndex = start + slice.length;

    return {
      items: slice.map(toSummary),
      nextCursor: nextIndex < sorted.length ? (slice[slice.length - 1]?.id ?? null) : null,
      total: sorted.length,
    };
  }

  async getContact(id: Ulid): Promise<ContactWithRelations | null> {
    await this.tick();
    return this.contacts.find((contact) => contact.id === id) ?? null;
  }

  async recentContacts(limit = 8): Promise<ContactSummary[]> {
    await this.tick();
    return this.live()
      .filter((contact) => contact.lastViewedAt != null)
      .sort((a, b) => (b.lastViewedAt ?? '').localeCompare(a.lastViewedAt ?? ''))
      .slice(0, limit)
      .map(toSummary);
  }

  async favoriteContacts(limit = 12): Promise<ContactSummary[]> {
    await this.tick();
    return this.live()
      .filter((contact) => contact.isFavorite)
      .slice(0, limit)
      .map(toSummary);
  }

  async deletedContacts(limit = 100): Promise<DeletedContact[]> {
    await this.tick();
    return this.contacts
      .filter((contact) => contact.deletedAt != null)
      .sort((a, b) => (a.deletedAt! < b.deletedAt! ? 1 : -1))
      .slice(0, limit)
      .map((contact) => ({ contact: toSummary(contact), deletedAt: contact.deletedAt! }));
  }

  // -- writes --------------------------------------------------------------

  private materialize(
    id: Ulid,
    input: ContactInput,
    existing?: ContactWithRelations,
  ): ContactWithRelations {
    const now = nowIso();
    const base = existing ?? {
      id,
      createdAt: now,
      createdBy: null,
      version: 0,
      deviceId: 'browser',
      deletedAt: null,
      lastViewedAt: null,
      relationships: [],
      contactNotes: [],
    };

    const envelope = {
      id,
      createdAt: (base as ContactWithRelations).createdAt ?? now,
      updatedAt: now,
      createdBy: (base as ContactWithRelations).createdBy ?? null,
      updatedBy: null,
      version: ((base as ContactWithRelations).version ?? 0) + 1,
      deviceId: 'browser',
      deletedAt: null,
    };

    return {
      ...envelope,
      firstName: input.firstName,
      lastName: input.lastName,
      displayName: deriveDisplayName(input),
      prefix: input.prefix,
      title: input.title,
      country: input.country,
      region: input.region,
      city: input.city,
      address: input.address,
      postalCode: input.postalCode,
      profession: input.profession,
      role: input.role,
      notes: input.notes,
      reasonForSaving: input.reasonForSaving,
      source: input.source,
      introducedBy: input.introducedBy,
      introducedByContactId: input.introducedByContactId,
      isFavorite: input.isFavorite,
      lastViewedAt: existing?.lastViewedAt ?? null,

      phones: input.phones.map((phone, index) => {
        const normalized = normalizePhone(phone.raw, input.country);
        return {
          ...envelope,
          id: phone.id ?? newId(),
          contactId: id,
          kind: phone.kind,
          raw: normalized.raw,
          e164: normalized.e164,
          digits: normalized.digits,
          countryCode: normalized.countryCode ?? input.country,
          isPrimary: phone.isPrimary || index === 0,
          label: phone.label,
        };
      }),
      emails: input.emails.map((email, index) => ({
        ...envelope,
        id: email.id ?? newId(),
        contactId: id,
        kind: email.kind,
        address: email.address,
        normalized: email.address.toLowerCase(),
        isPrimary: email.isPrimary || index === 0,
      })),
      aliases: input.aliases.map((alias) => ({
        ...envelope,
        id: alias.id ?? newId(),
        contactId: id,
        kind: alias.kind,
        value: alias.value,
        normalized: normalizeName(alias.value),
        languageCode: alias.languageCode,
      })),
      tags: input.tagIds
        .map((tagId) => this.tags.find((tag) => tag.id === tagId))
        .filter((tag): tag is Tag => tag != null),
      categories: input.categoryIds
        .map((categoryId) => this.categories.find((category) => category.id === categoryId))
        .filter((category): category is Category => category != null),
      specialties: input.specialties,
      languages: input.languages,
      organizations: input.organizations
        .map((link) => {
          const organization = this.organizations.find((org) => org.id === link.organizationId);
          if (!organization) return null;
          return {
            ...envelope,
            id: newId(),
            contactId: id,
            organizationId: organization.id,
            role: link.role,
            isPrimary: link.isPrimary,
            startedAt: link.startedAt,
            endedAt: link.endedAt,
            organization,
          };
        })
        .filter((link): link is NonNullable<typeof link> => link != null),
      relationships: existing?.relationships ?? [],
      contactNotes: existing?.contactNotes ?? [],
    };
  }

  async createContact(input: ContactInput, id: Ulid = newId()): Promise<ContactWithRelations> {
    await this.tick();
    const contact = this.materialize(id, input);
    this.contacts.push(contact);
    this.record('create', contact);
    return contact;
  }

  async quickAddContact(input: QuickAddInput, id: Ulid = newId()): Promise<ContactWithRelations> {
    return this.createContact(
      {
        displayName: input.displayName,
        firstName: null,
        lastName: null,
        prefix: null,
        title: null,
        country: null,
        region: null,
        city: null,
        address: null,
        postalCode: null,
        profession: null,
        role: null,
        notes: input.notes ?? null,
        reasonForSaving: null,
        source: null,
        introducedBy: null,
        introducedByContactId: null,
        isFavorite: false,
        phones: input.phone
          ? [{ kind: 'mobile', raw: input.phone, isPrimary: true, label: null }]
          : [],
        emails: [],
        aliases: [],
        specialties: [],
        languages: [],
        tagIds: [],
        categoryIds: [],
        organizations: [],
      },
      id,
    );
  }

  async updateContact(
    id: Ulid,
    patch: Partial<ContactInput>,
    baseVersion?: number,
  ): Promise<ContactWithRelations> {
    await this.tick();
    const existing = this.require(id);

    // The same optimistic-concurrency check the SQLite implementation performs.
    // Without it a stale form silently overwrites a newer edit.
    if (baseVersion != null && baseVersion !== existing.version) {
      throw RepositoryError.staleVersion(baseVersion, existing.version);
    }

    // Key presence, not `??`: a patch sends `null` to *clear* a field, and
    // `??` would read that as "not supplied" and quietly keep the old value —
    // making a cleared city impossible to save. The SQLite implementation
    // draws the same distinction, which is why `ContactPatch` on the Rust side
    // deserializes its nullable scalars through a double-option.
    const take = <K extends keyof ContactInput>(key: K, fallback: ContactInput[K]) =>
      key in patch ? (patch[key] as ContactInput[K]) : fallback;

    const merged: ContactInput = {
      firstName: take('firstName', existing.firstName),
      lastName: take('lastName', existing.lastName),
      displayName: take('displayName', existing.displayName),
      prefix: take('prefix', existing.prefix),
      title: take('title', existing.title),
      country: take('country', existing.country),
      region: take('region', existing.region),
      city: take('city', existing.city),
      address: take('address', existing.address),
      postalCode: take('postalCode', existing.postalCode),
      profession: take('profession', existing.profession),
      role: take('role', existing.role),
      notes: take('notes', existing.notes),
      reasonForSaving: take('reasonForSaving', existing.reasonForSaving),
      source: take('source', existing.source),
      introducedBy: take('introducedBy', existing.introducedBy),
      introducedByContactId: take('introducedByContactId', existing.introducedByContactId),
      isFavorite: take('isFavorite', existing.isFavorite),
      phones: take(
        'phones',
        existing.phones.map((phone) => ({
          id: phone.id,
          kind: phone.kind,
          raw: phone.raw,
          label: phone.label,
          isPrimary: phone.isPrimary,
        })),
      ),
      emails: take(
        'emails',
        existing.emails.map((email) => ({
          id: email.id,
          kind: email.kind,
          address: email.address,
          isPrimary: email.isPrimary,
        })),
      ),
      aliases: take(
        'aliases',
        existing.aliases.map((alias) => ({
          id: alias.id,
          kind: alias.kind,
          value: alias.value,
          languageCode: alias.languageCode,
        })),
      ),
      specialties: take('specialties', existing.specialties),
      languages: take('languages', existing.languages),
      tagIds: take(
        'tagIds',
        existing.tags.map((tag) => tag.id),
      ),
      categoryIds: take(
        'categoryIds',
        existing.categories.map((category) => category.id),
      ),
      organizations: take(
        'organizations',
        existing.organizations.map((link) => ({
          organizationId: link.organizationId,
          role: link.role,
          isPrimary: link.isPrimary,
          startedAt: link.startedAt,
          endedAt: link.endedAt,
        })),
      ),
    };

    const updated = this.materialize(id, merged, existing);
    this.contacts = this.contacts.map((contact) => (contact.id === id ? updated : contact));
    this.record('update', updated);
    return updated;
  }

  async deleteContact(id: Ulid): Promise<void> {
    await this.tick();
    const contact = this.require(id);
    // Soft delete: the row survives so the deletion can be synced to other
    // devices and undone. See docs/SYNC.md.
    contact.deletedAt = nowIso();
    contact.updatedAt = contact.deletedAt;
    contact.version += 1;
    this.record('delete', contact);
  }

  async restoreContact(id: Ulid): Promise<ContactWithRelations> {
    await this.tick();
    const contact = this.require(id);
    contact.deletedAt = null;
    contact.updatedAt = nowIso();
    contact.version += 1;
    this.record('restore', contact);
    return contact;
  }

  async setFavorite(id: Ulid, isFavorite: boolean): Promise<void> {
    await this.tick();
    const contact = this.require(id);
    contact.isFavorite = isFavorite;
    contact.updatedAt = nowIso();
    contact.version += 1;
  }

  async touchContact(id: Ulid): Promise<void> {
    const contact = this.require(id);
    // Not a versioned change: opening a record is not an edit and must not
    // create a mutation that other devices have to reconcile.
    contact.lastViewedAt = nowIso();
  }

  async findDuplicates(
    input: Partial<ContactInput>,
    excludeId?: Ulid,
  ): Promise<DuplicateCandidate[]> {
    await this.tick();
    return this.live()
      .filter((contact) => contact.id !== excludeId)
      .map((contact) => scoreDuplicate(input, toDuplicateSubject(contact)))
      .filter((candidate): candidate is DuplicateCandidate => candidate != null)
      .sort((a, b) => b.confidence - a.confidence)
      .slice(0, 5);
  }

  async listDuplicatePairs(limit = 100): Promise<DuplicatePair[]> {
    await this.tick();
    const live = this.live();
    const pairs = new Map<string, DuplicatePair>();
    const signal = (
      a: ContactWithRelations,
      b: ContactWithRelations,
      confidence: number,
      reason: string,
    ) => {
      const [first, second] = a.id < b.id ? [a, b] : [b, a];
      const key = `${first.id}:${second.id}`;
      const existing = pairs.get(key);
      if (existing) {
        existing.confidence = Math.max(existing.confidence, confidence);
        if (!existing.reasons.includes(reason)) existing.reasons.push(reason);
      } else {
        pairs.set(key, {
          first: toSummary(first),
          second: toSummary(second),
          confidence,
          reasons: [reason],
        });
      }
    };

    for (let i = 0; i < live.length; i += 1) {
      for (let j = i + 1; j < live.length; j += 1) {
        const a = live[i]!;
        const b = live[j]!;
        const aDigits = a.phones.map((p) => p.digits).filter((d) => d.length >= 7);
        const bDigits = new Set(
          b.phones
            .map((p) => p.digits)
            .filter((d) => d.length >= 7)
            .map((d) => d.slice(-7)),
        );
        if (aDigits.some((d) => bDigits.has(d.slice(-7)))) {
          signal(a, b, 0.9, 'אותו מספר טלפון');
        }
        const aEmails = a.emails.map((e) => e.normalized).filter(Boolean);
        const bEmails = new Set(b.emails.map((e) => e.normalized).filter(Boolean));
        if (aEmails.some((e) => bEmails.has(e))) {
          signal(a, b, 0.85, 'אותה כתובת אימייל');
        }
        if (
          normalizeName(a.displayName) !== '' &&
          normalizeName(a.displayName) === normalizeName(b.displayName)
        ) {
          signal(a, b, 0.5, 'שם זהה');
        }
      }
    }
    return [...pairs.values()].sort((a, b) => b.confidence - a.confidence).slice(0, limit);
  }

  async mergeContacts(keepId: Ulid, mergeId: Ulid): Promise<ContactWithRelations> {
    await this.tick();
    if (keepId === mergeId) {
      throw new RepositoryError('validation', 'לא ניתן למזג איש קשר עם עצמו');
    }
    const keep = this.contacts.find((c) => c.id === keepId && c.deletedAt == null);
    const merge = this.contacts.find((c) => c.id === mergeId && c.deletedAt == null);
    if (!keep || !merge) {
      throw new RepositoryError('not_found', 'איש הקשר לא נמצא');
    }
    const now = nowIso();

    // Blank scalars fill from the merged side; conflicts are preserved in notes.
    const scalarFields = [
      ['firstName', 'שם פרטי'],
      ['lastName', 'שם משפחה'],
      ['prefix', 'תואר'],
      ['title', 'תואר אחרי השם'],
      ['country', 'מדינה'],
      ['region', 'אזור'],
      ['city', 'עיר'],
      ['address', 'כתובת'],
      ['postalCode', 'מיקוד'],
      ['profession', 'מקצוע'],
      ['role', 'תפקיד'],
      ['reasonForSaving', 'נשמר בגלל'],
      ['source', 'מקור'],
      ['introducedBy', 'הכיר בינינו'],
    ] as const;
    const extraNotes: string[] = [];
    for (const [field, label] of scalarFields) {
      const kept = keep[field];
      const other = merge[field];
      if (kept == null && other != null) {
        (keep as Record<typeof field, string | null>)[field] = other;
      } else if (kept != null && other != null && kept !== other) {
        extraNotes.push(`${label}: ${other}`);
      }
    }
    if (merge.notes && merge.notes.trim() !== '' && merge.notes !== keep.notes) {
      extraNotes.push(merge.notes);
    }
    if (merge.isFavorite) keep.isFavorite = true;
    if (extraNotes.length > 0) {
      const separator = '\n';
      const addition = `— מוזג מ״${merge.displayName}״ (${now}) —${separator}${extraNotes.join(separator)}`;
      keep.notes =
        keep.notes && keep.notes !== ''
          ? `${keep.notes}${separator}${separator}${addition}`
          : addition;
    }

    // Children move unless the kept side already holds the same value.
    const keepPhoneDigits = new Set(keep.phones.map((p) => p.digits));
    keep.phones.push(
      ...merge.phones
        .filter((p) => !keepPhoneDigits.has(p.digits))
        .map((p) => ({
          ...p,
          contactId: keep.id,
          isPrimary: keep.phones.length === 0 && p.isPrimary,
        })),
    );
    const keepEmails = new Set(keep.emails.map((e) => e.normalized));
    keep.emails.push(
      ...merge.emails
        .filter((e) => !keepEmails.has(e.normalized))
        .map((e) => ({
          ...e,
          contactId: keep.id,
          isPrimary: keep.emails.length === 0 && e.isPrimary,
        })),
    );
    const keepAliases = new Set(keep.aliases.map((a) => a.normalized));
    keep.aliases.push(
      ...merge.aliases
        .filter((a) => !keepAliases.has(a.normalized))
        .map((a) => ({ ...a, contactId: keep.id })),
    );
    keep.specialties = [...new Set([...keep.specialties, ...merge.specialties])];
    keep.languages = [...new Set([...keep.languages, ...merge.languages])];
    const keepTagIds = new Set(keep.tags.map((t) => t.id));
    keep.tags.push(...merge.tags.filter((t) => !keepTagIds.has(t.id)));
    const keepCategoryIds = new Set(keep.categories.map((c) => c.id));
    keep.categories.push(...merge.categories.filter((c) => !keepCategoryIds.has(c.id)));
    const keepOrgIds = new Set(keep.organizations.map((o) => o.organizationId));
    keep.organizations.push(
      ...merge.organizations
        .filter((o) => !keepOrgIds.has(o.organizationId))
        .map((o) => ({ ...o, contactId: keep.id })),
    );
    keep.contactNotes.push(...merge.contactNotes.map((note) => ({ ...note, contactId: keep.id })));
    merge.contactNotes = [];

    // Edges re-point unless they would duplicate an existing one or point home.
    for (const contact of this.contacts) {
      const seen = new Set(
        contact.relationships.map(
          (edge) => `${edge.direction}:${edge.type}:${edge.otherContact.id}`,
        ),
      );
      contact.relationships = contact.relationships.flatMap((edge) => {
        if (edge.otherContact.id !== mergeId) return [edge];
        if (contact.id === keepId) return [];
        const moved = { ...edge, otherContact: toSummary(keep) };
        const key = `${moved.direction}:${moved.type}:${keepId}`;
        if (seen.has(key)) return [];
        seen.add(key);
        return [moved];
      });
    }
    const keepEdgeSeen = new Set(
      keep.relationships.map((edge) => `${edge.direction}:${edge.type}:${edge.otherContact.id}`),
    );
    for (const edge of merge.relationships) {
      if (edge.otherContact.id === keepId) continue;
      const key = `${edge.direction}:${edge.type}:${edge.otherContact.id}`;
      if (keepEdgeSeen.has(key)) continue;
      keepEdgeSeen.add(key);
      keep.relationships.push(edge);
    }
    merge.relationships = [];
    for (const contact of this.contacts) {
      if (contact.introducedByContactId === mergeId) {
        contact.introducedByContactId = keepId;
      }
    }

    merge.deletedAt = now;
    merge.updatedAt = now;
    merge.version += 1;
    keep.updatedAt = now;
    keep.version += 1;
    this.pendingMutations += 2;
    return keep;
  }

  // -- taxonomy ------------------------------------------------------------

  async listTags(): Promise<Tag[]> {
    await this.tick();
    return this.tags.filter((tag) => tag.deletedAt == null);
  }

  async createTag(input: TagInput): Promise<Tag> {
    await this.tick();
    const normalized = normalizeText(input.name);
    const existing = this.tags.find(
      (tag) => tag.deletedAt == null && normalizeText(tag.name) === normalized,
    );
    if (existing) return existing;

    const now = nowIso();
    const tag: Tag = {
      id: newId(),
      createdAt: now,
      updatedAt: now,
      createdBy: null,
      updatedBy: null,
      version: 1,
      deviceId: 'browser',
      deletedAt: null,
      name: input.name,
      normalized,
      color: input.color,
      description: input.description,
    };
    this.tags.push(tag);
    return tag;
  }

  async deleteTag(id: Ulid): Promise<void> {
    await this.tick();
    const tag = this.tags.find((candidate) => candidate.id === id);
    if (!tag) throw RepositoryError.notFound('התגית');
    tag.deletedAt = nowIso();
    for (const contact of this.contacts) {
      contact.tags = contact.tags.filter((candidate) => candidate.id !== id);
    }
  }

  async listCategories(): Promise<Category[]> {
    await this.tick();
    return this.categories.filter((category) => category.deletedAt == null);
  }

  async createCategory(input: CategoryInput): Promise<Category> {
    await this.tick();
    const now = nowIso();
    const category: Category = {
      id: newId(),
      createdAt: now,
      updatedAt: now,
      createdBy: null,
      updatedBy: null,
      version: 1,
      deviceId: 'browser',
      deletedAt: null,
      name: input.name,
      normalized: normalizeText(input.name),
      description: input.description,
      parentId: input.parentId,
    };
    this.categories.push(category);
    return category;
  }

  async deleteCategory(id: Ulid): Promise<void> {
    await this.tick();
    const category = this.categories.find((candidate) => candidate.id === id);
    if (!category) throw RepositoryError.notFound('הקטגוריה');
    category.deletedAt = nowIso();
    for (const contact of this.contacts) {
      contact.categories = contact.categories.filter((candidate) => candidate.id !== id);
    }
  }

  async listOrganizations(query?: string, limit = 50): Promise<Organization[]> {
    await this.tick();
    const live = this.organizations.filter((org) => org.deletedAt == null);
    if (!query) return live.slice(0, limit);
    const needle = normalizeText(query);
    return live.filter((org) => normalizeText(org.name).includes(needle)).slice(0, limit);
  }

  async createOrganization(input: OrganizationInput): Promise<Organization> {
    await this.tick();
    const now = nowIso();
    const organization: Organization = {
      id: newId(),
      createdAt: now,
      updatedAt: now,
      createdBy: null,
      updatedBy: null,
      version: 1,
      deviceId: 'browser',
      deletedAt: null,
      name: input.name,
      normalized: normalizeText(input.name),
      kind: input.kind,
      city: input.city,
      region: input.region,
      country: input.country,
      address: input.address,
      notes: input.notes,
    };
    this.organizations.push(organization);
    return organization;
  }

  async deleteOrganization(id: Ulid): Promise<void> {
    await this.tick();
    const organization = this.organizations.find((candidate) => candidate.id === id);
    if (!organization) throw RepositoryError.notFound('המוסד');
    organization.deletedAt = nowIso();
    for (const contact of this.contacts) {
      contact.organizations = contact.organizations.filter((link) => link.organizationId !== id);
    }
  }

  // -- graph ---------------------------------------------------------------

  async createRelationship(input: RelationshipInput): Promise<Relationship> {
    await this.tick();
    const from = this.require(input.fromContactId);
    const to = this.require(input.toContactId);

    const now = nowIso();
    const relationship: Relationship = {
      id: newId(),
      createdAt: now,
      updatedAt: now,
      createdBy: null,
      updatedBy: null,
      version: 1,
      deviceId: 'browser',
      deletedAt: null,
      fromContactId: input.fromContactId,
      toContactId: input.toContactId,
      type: input.type,
      notes: input.notes,
    };

    this.relationships.push(relationship);
    // Materialize both directions so either detail screen can render the edge.
    from.relationships.push({ ...relationship, otherContact: toSummary(to), direction: 'out' });
    to.relationships.push({ ...relationship, otherContact: toSummary(from), direction: 'in' });
    return relationship;
  }

  async deleteRelationship(id: Ulid): Promise<void> {
    await this.tick();
    this.relationships = this.relationships.filter((relationship) => relationship.id !== id);
    for (const contact of this.contacts) {
      contact.relationships = contact.relationships.filter(
        (relationship) => relationship.id !== id,
      );
    }
  }

  // -- notes ---------------------------------------------------------------

  async addNote(input: NoteInput): Promise<Note> {
    await this.tick();
    const contact = this.require(input.contactId);
    const now = nowIso();
    const note: Note = {
      id: newId(),
      createdAt: now,
      updatedAt: now,
      createdBy: null,
      updatedBy: null,
      version: 1,
      deviceId: 'browser',
      deletedAt: null,
      contactId: input.contactId,
      body: input.body,
      isSensitive: input.isSensitive,
      authorId: null,
    };
    contact.contactNotes.unshift(note);
    return note;
  }

  async updateNote(id: Ulid, body: string, isSensitive?: boolean): Promise<Note> {
    await this.tick();
    for (const contact of this.contacts) {
      const note = contact.contactNotes.find((candidate) => candidate.id === id);
      if (note) {
        note.body = body;
        if (isSensitive != null) note.isSensitive = isSensitive;
        note.updatedAt = nowIso();
        note.version += 1;
        return note;
      }
    }
    throw RepositoryError.notFound('ההערה');
  }

  async deleteNote(id: Ulid): Promise<void> {
    await this.tick();
    for (const contact of this.contacts) {
      contact.contactNotes = contact.contactNotes.filter((note) => note.id !== id);
    }
  }

  // -- meta ----------------------------------------------------------------

  async stats(): Promise<DatabaseStats> {
    await this.tick();
    return {
      contacts: this.live().length,
      organizations: this.organizations.filter((org) => org.deletedAt == null).length,
      tags: this.tags.filter((tag) => tag.deletedAt == null).length,
      relationships: this.relationships.length,
      notes: this.live().reduce((total, contact) => total + contact.contactNotes.length, 0),
      sync: {
        online: false,
        lastSyncAt: null,
        pendingMutations: this.pendingMutations,
        failedMutations: 0,
        openConflicts: 0,
        syncing: false,
      },
    };
  }

  async auditLog(entityId?: Ulid, limit = 50): Promise<AuditLogEntry[]> {
    await this.tick();
    const rows = entityId ? this.audit.filter((entry) => entry.entityId === entityId) : this.audit;
    return rows.slice(0, limit);
  }
}
