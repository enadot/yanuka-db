import { expect, it, describe } from 'vitest';
import type { ContactsRepository } from './repository.js';

/**
 * The behavioural contract every repository implementation must satisfy.
 *
 * Exported as a function rather than a test file so the same suite can be run
 * against the in-memory implementation here and against the SQLite-backed one
 * from the desktop app's test job. Without this, the mock is free to drift into
 * fiction and stop predicting how the real database behaves.
 */
export function runRepositoryContractTests(
  name: string,
  makeRepository: () => Promise<ContactsRepository> | ContactsRepository,
): void {
  describe(`ContactsRepository contract: ${name}`, () => {
    const blankContact = {
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
      notes: null,
      reasonForSaving: null,
      source: null,
      introducedBy: null,
      introducedByContactId: null,
      isFavorite: false,
      phones: [],
      emails: [],
      aliases: [],
      specialties: [],
      languages: [],
      tagIds: [],
      categoryIds: [],
      organizations: [],
    };

    it('creates a contact and reads it back', async () => {
      const repo = await makeRepository();
      const created = await repo.createContact({ ...blankContact, displayName: 'ניסיון אחד' });

      expect(created.id).toBeTruthy();
      expect(created.version).toBeGreaterThanOrEqual(1);

      const fetched = await repo.getContact(created.id);
      expect(fetched?.displayName).toBe('ניסיון אחד');
    });

    it('normalizes a phone number on write while preserving what was typed', async () => {
      const repo = await makeRepository();
      const created = await repo.createContact({
        ...blankContact,
        displayName: 'בעל טלפון',
        country: 'IL',
        phones: [{ kind: 'mobile', raw: '054-555-0134', isPrimary: true, label: null }],
      });

      const phone = created.phones[0]!;
      expect(phone.raw).toBe('054-555-0134');
      expect(phone.digits).toContain('5550134');
    });

    it('keeps an unparseable phone number rather than rejecting it', async () => {
      // Historical notebook entries are frequently not valid numbers. Losing
      // them would defeat the purpose of the archive.
      const repo = await makeRepository();
      const created = await repo.createContact({
        ...blankContact,
        displayName: 'מספר חלקי',
        phones: [{ kind: 'other', raw: 'בבית של אדלר', isPrimary: true, label: null }],
      });

      expect(created.phones).toHaveLength(1);
      expect(created.phones[0]!.raw).toBe('בבית של אדלר');
      expect(created.phones[0]!.e164).toBeNull();
    });

    it('bumps the version on update', async () => {
      const repo = await makeRepository();
      const created = await repo.createContact({ ...blankContact, displayName: 'לפני' });
      const updated = await repo.updateContact(created.id, { displayName: 'אחרי' });

      expect(updated.displayName).toBe('אחרי');
      expect(updated.version).toBeGreaterThan(created.version);
    });

    it('rejects an update based on a stale version', async () => {
      const repo = await makeRepository();
      const created = await repo.createContact({ ...blankContact, displayName: 'גרסאות' });
      await repo.updateContact(created.id, { city: 'ירושלים' });

      await expect(
        repo.updateContact(created.id, { city: 'בני ברק' }, created.version),
      ).rejects.toMatchObject({ code: 'stale_version' });
    });

    it('soft-deletes so the record can still be restored', async () => {
      const repo = await makeRepository();
      const created = await repo.createContact({ ...blankContact, displayName: 'למחיקה' });

      await repo.deleteContact(created.id);
      const afterDelete = await repo.getContact(created.id);
      expect(afterDelete?.deletedAt).not.toBeNull();

      const results = await repo.search({
        text: 'למחיקה',
        sort: 'relevance',
        limit: 50,
        offset: 0,
        favoritesOnly: false,
        includeDeleted: false,
      });
      expect(results.results).toHaveLength(0);

      const restored = await repo.restoreContact(created.id);
      expect(restored.deletedAt).toBeNull();
    });

    it('lists a deleted contact in the recycle bin until it is restored', async () => {
      // A soft delete that nothing can list is a hard delete with extra steps:
      // the row survives on disk but the user can never reach it again.
      const repo = await makeRepository();
      const created = await repo.createContact({ ...blankContact, displayName: 'לסל המחזור' });

      expect((await repo.deletedContacts()).some((row) => row.contact.id === created.id)).toBe(
        false,
      );

      await repo.deleteContact(created.id);
      const binned = (await repo.deletedContacts()).find((row) => row.contact.id === created.id);
      expect(binned).toBeDefined();
      expect(binned!.deletedAt).toBeTruthy();
      expect(binned!.contact.displayName).toBe('לסל המחזור');

      await repo.restoreContact(created.id);
      expect((await repo.deletedContacts()).some((row) => row.contact.id === created.id)).toBe(
        false,
      );

      // And it is searchable again, which is the point of restoring it.
      const results = await repo.search({
        text: 'לסל המחזור',
        sort: 'relevance',
        limit: 50,
        offset: 0,
        favoritesOnly: false,
        includeDeleted: false,
      });
      expect(results.results.some((result) => result.contact.id === created.id)).toBe(true);
    });

    it('finds a contact by a word from its notes', async () => {
      const repo = await makeRepository();
      const created = await repo.createContact({
        ...blankContact,
        displayName: 'שם שלא זוכרים',
        notes: 'יהודי מלונדון שעוסק בנדלן והומלץ על ידי הרב',
      });

      const results = await repo.search({
        text: 'נדלן',
        sort: 'relevance',
        limit: 50,
        offset: 0,
        favoritesOnly: false,
        includeDeleted: false,
      });

      // Not necessarily first: an implementation seeded with real data may hold
      // contacts tagged `נדל"ן`, and a tag match legitimately outranks a note.
      // What matters is that the note-only contact is found, and that the
      // engine attributes it to the note.
      const found = results.results.find((result) => result.contact.id === created.id);
      expect(found).toBeDefined();
      expect(found!.reasons.some((reason) => reason.source === 'notes')).toBe(true);
    });

    it('finds a contact through an honorific variant of its name', async () => {
      const repo = await makeRepository();
      await repo.createContact({ ...blankContact, displayName: 'הרב יצחק בדיקה' });

      const results = await repo.search({
        text: 'יצחק בדיקה',
        sort: 'relevance',
        limit: 50,
        offset: 0,
        favoritesOnly: false,
        includeDeleted: false,
      });

      expect(results.results.length).toBeGreaterThan(0);
    });

    it('returns facet counts alongside results', async () => {
      const repo = await makeRepository();
      const results = await repo.search({
        text: '',
        sort: 'relevance',
        limit: 50,
        offset: 0,
        favoritesOnly: false,
        includeDeleted: false,
      });

      expect(results.facets).toBeDefined();
      expect(results.total).toBeGreaterThanOrEqual(0);
    });

    it('warns about a duplicate when the phone number matches', async () => {
      const repo = await makeRepository();
      await repo.createContact({
        ...blankContact,
        displayName: 'מקורי',
        country: 'IL',
        phones: [{ kind: 'mobile', raw: '054-555-9876', isPrimary: true, label: null }],
      });

      const candidates = await repo.findDuplicates({
        displayName: 'שם אחר לגמרי',
        phones: [{ kind: 'mobile', raw: '+972 54 555 9876', isPrimary: true, label: null }],
      });

      expect(candidates.length).toBeGreaterThan(0);
      expect(candidates[0]!.reasons).toContain('אותו מספר טלפון');
    });

    it('paginates without repeating or skipping rows', async () => {
      const repo = await makeRepository();
      const listArgs = {
        cursor: null,
        limit: 10,
        sort: 'name' as const,
        startsWith: null,
        favoritesOnly: false,
        includeDeleted: false,
      };

      const first = await repo.listContacts(listArgs);
      expect(first.items.length).toBeGreaterThan(0);

      if (first.nextCursor) {
        const second = await repo.listContacts({ ...listArgs, cursor: first.nextCursor });
        const firstIds = new Set(first.items.map((item) => item.id));
        for (const item of second.items) {
          expect(firstIds.has(item.id)).toBe(false);
        }
      }
    });

    it('links two contacts and shows the edge from both sides', async () => {
      const repo = await makeRepository();
      const a = await repo.createContact({ ...blankContact, displayName: 'ממליץ' });
      const b = await repo.createContact({ ...blankContact, displayName: 'מומלץ' });

      await repo.createRelationship({
        fromContactId: a.id,
        toContactId: b.id,
        type: 'recommended',
        notes: null,
      });

      const fromA = await repo.getContact(a.id);
      const fromB = await repo.getContact(b.id);

      expect(fromA?.relationships.some((edge) => edge.otherContact.id === b.id)).toBe(true);
      expect(fromB?.relationships.some((edge) => edge.otherContact.id === a.id)).toBe(true);
    });

    it('leaves a collection the patch never mentions alone', async () => {
      // A write replaces the child collections wholesale, so a patch that
      // omits a collection has to mean "untouched" and not "empty". Otherwise
      // every save from a screen that renders phones but not e-mail addresses
      // deletes the addresses — priority 1, מידע לא הולך לאיבוד.
      const repo = await makeRepository();
      const created = await repo.createContact({
        ...blankContact,
        displayName: 'שומר על מה שלא נגעו בו',
        emails: [{ kind: 'personal', address: 'a@example.com', isPrimary: true }],
        aliases: [{ kind: 'alias', value: 'אברהמ׳ל', languageCode: null }],
        languages: ['he'],
        specialties: ['סת"ם'],
      });

      const updated = await repo.updateContact(created.id, {
        phones: [{ kind: 'mobile', raw: '054-5550134', label: null, isPrimary: true }],
      });

      expect(updated.phones).toHaveLength(1);
      expect(updated.emails).toHaveLength(1);
      expect(updated.aliases).toHaveLength(1);
      expect(updated.languages).toEqual(['he']);
      expect(updated.specialties).toHaveLength(1);
    });

    it('clears a scalar the patch explicitly nulls', async () => {
      // `null` means "the user emptied this box"; an absent key means "the
      // screen never showed it". Conflating the two makes a cleared field
      // impossible to save.
      const repo = await makeRepository();
      const created = await repo.createContact({
        ...blankContact,
        displayName: 'עיר שהוסרה',
        city: 'ירושלים',
        profession: 'סופר',
      });

      const updated = await repo.updateContact(created.id, { city: null });
      expect(updated.city).toBeNull();
      expect(updated.profession).toBe('סופר');
    });

    it('clears a collection the patch explicitly empties', async () => {
      const repo = await makeRepository();
      const created = await repo.createContact({
        ...blankContact,
        displayName: 'ריקון מכוון',
        emails: [{ kind: 'personal', address: 'a@example.com', isPrimary: true }],
      });

      const updated = await repo.updateContact(created.id, { emails: [] });
      expect(updated.emails).toHaveLength(0);
    });

    it('links a contact to an organization and keeps the link across an edit', async () => {
      const repo = await makeRepository();
      const organization = await repo.createOrganization({
        name: 'ישיבת מיר',
        kind: 'yeshiva',
        city: 'ירושלים',
        region: null,
        country: 'IL',
        address: null,
        notes: null,
      });

      const created = await repo.createContact({
        ...blankContact,
        displayName: 'ראש הישיבה',
        organizations: [
          {
            organizationId: organization.id,
            role: 'ראש ישיבה',
            isPrimary: true,
            startedAt: null,
            endedAt: null,
          },
        ],
      });

      expect(created.organizations).toHaveLength(1);
      expect(created.organizations[0]!.organization.name).toBe('ישיבת מיר');
      expect(created.organizations[0]!.role).toBe('ראש ישיבה');

      const updated = await repo.updateContact(created.id, { city: 'ירושלים' });
      expect(updated.organizations).toHaveLength(1);
    });

    it('adds, edits and removes a timestamped note', async () => {
      const repo = await makeRepository();
      const created = await repo.createContact({ ...blankContact, displayName: 'בעל הערות' });

      const note = await repo.addNote({
        contactId: created.id,
        body: 'הכיר לנו את הרב מלונדון',
        isSensitive: false,
      });
      expect((await repo.getContact(created.id))?.contactNotes).toHaveLength(1);

      await repo.updateNote(note.id, 'הכיר לנו את הרב ממנצ׳סטר');
      const afterEdit = await repo.getContact(created.id);
      expect(afterEdit?.contactNotes[0]!.body).toContain('מנצ׳סטר');

      await repo.deleteNote(note.id);
      expect((await repo.getContact(created.id))?.contactNotes).toHaveLength(0);
    });

    it('removes a relationship from both endpoints', async () => {
      const repo = await makeRepository();
      const a = await repo.createContact({ ...blankContact, displayName: 'צד א' });
      const b = await repo.createContact({ ...blankContact, displayName: 'צד ב' });

      const edge = await repo.createRelationship({
        fromContactId: a.id,
        toContactId: b.id,
        type: 'knows',
        notes: null,
      });
      await repo.deleteRelationship(edge.id);

      expect((await repo.getContact(a.id))?.relationships).toHaveLength(0);
      expect((await repo.getContact(b.id))?.relationships).toHaveLength(0);
    });

    it('finds duplicate pairs and merges without losing data', async () => {
      const repo = await makeRepository();
      const keep = await repo.createContact({
        ...blankContact,
        displayName: 'זלמן דוקטור',
        city: 'ירושלים',
        notes: 'הערה על הנשמר',
        phones: [{ kind: 'mobile', raw: '054-8880001', label: null, isPrimary: true }],
      });
      const merged = await repo.createContact({
        ...blankContact,
        displayName: 'ר זלמן דוקטור',
        city: 'צפת',
        profession: 'רופא',
        notes: 'הערה על הממוזג',
        // The first number matches the kept contact's exactly; the second is
        // the same number in another format. Only the exact match is deduped:
        // suffix-based dedupe could silently drop a genuinely different
        // number that shares a local suffix, and losing a number is the one
        // outcome this product forbids.
        phones: [
          { kind: 'mobile', raw: '054-8880001', label: null, isPrimary: true },
          { kind: 'home', raw: '+972548880001', label: null, isPrimary: false },
        ],
      });

      const pairs = await repo.listDuplicatePairs();
      const pair = pairs.find(
        (candidate) =>
          [candidate.first.id, candidate.second.id].includes(keep.id) &&
          [candidate.first.id, candidate.second.id].includes(merged.id),
      );
      expect(pair).toBeDefined();
      expect(pair?.reasons).toContain('אותו מספר טלפון');

      const out = await repo.mergeContacts(keep.id, merged.id);
      // The exact duplicate is not doubled; the variant format moved over.
      expect(out.phones).toHaveLength(2);
      expect(out.phones.map((phone) => phone.raw)).toContain('+972548880001');
      expect(out.phones.filter((phone) => phone.isPrimary)).toHaveLength(1);
      // Blank filled, conflict and the merged notes preserved in notes.
      expect(out.profession).toBe('רופא');
      expect(out.city).toBe('ירושלים');
      expect(out.notes).toContain('הערה על הנשמר');
      expect(out.notes).toContain('צפת');
      expect(out.notes).toContain('הערה על הממוזג');
      // The merged contact no longer appears anywhere live.
      expect(await repo.getContact(merged.id).then((c) => c?.deletedAt)).toBeTruthy();
      expect(await repo.mergeContacts(keep.id, keep.id).catch(() => 'rejected')).toBe('rejected');
    });

    it('reports stats', async () => {
      const repo = await makeRepository();
      const stats = await repo.stats();
      expect(stats.contacts).toBeGreaterThanOrEqual(0);
      expect(stats.sync).toBeDefined();
    });
  });
}
