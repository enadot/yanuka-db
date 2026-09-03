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

    it('removes a deleted relationship from both sides', async () => {
      const repo = await makeRepository();
      const a = await repo.createContact({ ...blankContact, displayName: 'צד ראשון' });
      const b = await repo.createContact({ ...blankContact, displayName: 'צד שני' });
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

    it('a saved note is findable at once, and its old wording stops matching after an edit', async () => {
      const searchInput = {
        sort: 'relevance',
        limit: 50,
        offset: 0,
        favoritesOnly: false,
        includeDeleted: false,
      } as const;
      const hits = async (repo: ContactsRepository, text: string, id: string) => {
        const results = await repo.search({ ...searchInput, text });
        return results.results.some((result) => result.contact.id === id);
      };

      const repo = await makeRepository();
      const created = await repo.createContact({ ...blankContact, displayName: 'בעל הערות' });

      const note = await repo.addNote({
        contactId: created.id,
        body: 'פגשנו אותו בכנס באנטוורפן',
        isSensitive: false,
      });
      const detail = await repo.getContact(created.id);
      expect(detail?.contactNotes.map((candidate) => candidate.body)).toContain(
        'פגשנו אותו בכנס באנטוורפן',
      );
      expect(await hits(repo, 'אנטוורפן', created.id)).toBe(true);

      await repo.updateNote(note.id, 'עבר לגור בעיר צפת');
      expect(await hits(repo, 'אנטוורפן', created.id)).toBe(false);
      expect(await hits(repo, 'צפת', created.id)).toBe(true);

      await repo.deleteNote(note.id);
      expect(await hits(repo, 'צפת', created.id)).toBe(false);
      expect((await repo.getContact(created.id))?.contactNotes).toHaveLength(0);
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

    it('a deleted contact waits in the trash and leaves it on restore', async () => {
      const repo = await makeRepository();
      const contact = await repo.createContact({
        ...blankContact,
        displayName: 'נחום הנמחק',
      });

      await repo.deleteContact(contact.id);
      const trash = await repo.listDeletedContacts();
      const entry = trash.find((row) => row.id === contact.id);
      expect(entry).toBeDefined();
      expect(entry?.deletedAt).toBeTruthy();
      // Gone from the living list…
      const living = await repo.listContacts({
        cursor: null,
        limit: 200,
        sort: 'name',
        startsWith: null,
        favoritesOnly: false,
        includeDeleted: false,
      });
      expect(living.items.some((row) => row.id === contact.id)).toBe(false);

      // …and back in it after a restore, out of the trash.
      await repo.restoreContact(contact.id);
      expect((await repo.listDeletedContacts()).some((row) => row.id === contact.id)).toBe(false);
      const restored = await repo.getContact(contact.id);
      expect(restored?.deletedAt).toBeNull();
    });

    it('history remembers what changed and what it was before', async () => {
      const repo = await makeRepository();
      const contact = await repo.createContact({
        ...blankContact,
        displayName: 'זכריה הזכור',
        city: 'ירושלים',
      });

      await repo.updateContact(contact.id, { city: 'צפת' });
      await repo.deleteContact(contact.id);
      await repo.restoreContact(contact.id);

      const history = await repo.auditLog(contact.id);
      const actions = history.map((entry) => entry.action);
      // Newest first: restore, delete, update, create.
      expect(actions.slice(0, 4)).toEqual(['restore', 'delete', 'update', 'create']);

      const update = history.find((entry) => entry.action === 'update');
      expect(update?.changes?.city).toEqual({ from: 'ירושלים', to: 'צפת' });
      // Fields that did not change do not clutter the entry.
      expect(update?.changes?.displayName).toBeUndefined();
    });

    it('note edits are on the record, with the wording they replaced', async () => {
      const repo = await makeRepository();
      const contact = await repo.createContact({
        ...blankContact,
        displayName: 'נתן הרושם',
      });

      const note = await repo.addNote({
        contactId: contact.id,
        body: 'ביקר בבני ברק אצל הרב',
        isSensitive: false,
      });
      await repo.updateNote(note.id, 'עבר לגור בירושלים');
      await repo.deleteNote(note.id);

      const history = await repo.auditLog(contact.id);
      const notes = history.filter((entry) => entry.entityType === 'note');
      // Newest first: delete, update, create — all reachable from the card.
      expect(notes.map((entry) => entry.action)).toEqual(['delete', 'update', 'create']);
      expect(notes[1]?.changes?.body).toEqual({
        from: 'ביקר בבני ברק אצל הרב',
        to: 'עבר לגור בירושלים',
      });
      // The entry is labeled with the note's wording, so a deleted note stays
      // readable from the history.
      expect(notes[0]?.entityLabel).toContain('עבר לגור בירושלים');
    });

    it('a relationship appears in the history of both endpoints', async () => {
      const repo = await makeRepository();
      const a = await repo.createContact({ ...blankContact, displayName: 'איתן הראשון' });
      const b = await repo.createContact({ ...blankContact, displayName: 'בועז השני' });

      const edge = await repo.createRelationship({
        fromContactId: a.id,
        toContactId: b.id,
        type: 'recommended',
        notes: null,
      });

      for (const id of [a.id, b.id]) {
        const created = (await repo.auditLog(id)).filter(
          (entry) => entry.entityType === 'relationship',
        );
        expect(created.map((entry) => entry.action)).toEqual(['create']);
      }

      await repo.deleteRelationship(edge.id);
      for (const id of [a.id, b.id]) {
        const actions = (await repo.auditLog(id))
          .filter((entry) => entry.entityType === 'relationship')
          .map((entry) => entry.action);
        expect(actions).toEqual(['delete', 'create']);
      }
    });

    it('reports stats', async () => {
      const repo = await makeRepository();
      const stats = await repo.stats();
      expect(stats.contacts).toBeGreaterThanOrEqual(0);
      expect(stats.sync).toBeDefined();
    });

    // -- smart categories (ADR-038) -------------------------------------------

    const scribeRule = {
      match: 'all' as const,
      conditions: [{ field: 'occupation' as const, op: 'contains' as const, values: ['סופר'] }],
    };

    it('a rule fills a category, and the contact card explains why', async () => {
      const repo = await makeRepository();
      const scribe = await repo.createContact({
        ...blankContact,
        displayName: 'סופר לבדיקה',
        profession: 'סופר סת"ם',
      });
      await repo.createContact({ ...blankContact, displayName: 'לא סופר', profession: 'חשמלאי' });

      const category = await repo.createCategory({
        name: 'סופרים לבדיקה',
        description: null,
        parentId: null,
        icon: 'scroll',
        color: '#b45309',
        rule: scribeRule,
        showOnHome: true,
      });

      const summaries = await repo.listCategories();
      const summary = summaries.find((candidate) => candidate.id === category.id);
      expect(summary?.count).toBeGreaterThanOrEqual(1);

      const members = await repo.categoryMembers(category.id);
      const names = members.items.map((member) => member.contact.displayName);
      expect(names).toContain('סופר לבדיקה');
      expect(names).not.toContain('לא סופר');
      expect(members.items.find((m) => m.contact.id === scribe.id)?.membership).toBe('rule');

      const card = await repo.getContact(scribe.id);
      const onCard = card?.categories.find((c) => c.id === category.id);
      expect(onCard?.membership).toBe('rule');
    });

    it('membership follows the record: change the profession and the shelf updates', async () => {
      const repo = await makeRepository();
      const category = await repo.createCategory({
        name: 'קליגרפים',
        description: null,
        parentId: null,
        icon: null,
        color: null,
        rule: {
          match: 'all',
          conditions: [{ field: 'occupation', op: 'contains', values: ['קליגרף'] }],
        },
        showOnHome: true,
      });
      const created = await repo.createContact({
        ...blankContact,
        displayName: 'עוד מעט קליגרף',
        profession: 'מלמד',
      });
      const names = async () =>
        (await repo.categoryMembers(category.id)).items.map((m) => m.contact.displayName);
      expect(await names()).not.toContain('עוד מעט קליגרף');

      await repo.updateContact(created.id, { profession: 'קליגרף' }, created.version);
      expect(await names()).toContain('עוד מעט קליגרף');
      expect((await repo.getCategory(category.id))?.count).toBe(1);

      const updated = await repo.getContact(created.id);
      await repo.updateContact(created.id, { profession: 'מלמד' }, updated!.version);
      expect(await names()).not.toContain('עוד מעט קליגרף');
    });

    it('a person can be pinned in, kept out, and handed back to the rule', async () => {
      const repo = await makeRepository();
      const category = await repo.createCategory({
        name: 'סופרים עם חריגים',
        description: null,
        parentId: null,
        icon: null,
        color: null,
        rule: scribeRule,
        showOnHome: true,
      });
      const scribe = await repo.createContact({
        ...blankContact,
        displayName: 'סופר מודר',
        profession: 'סופר',
      });
      const outsider = await repo.createContact({
        ...blankContact,
        displayName: 'מצורף ביד',
        profession: 'נגר',
      });

      await repo.setCategoryMembership(category.id, scribe.id, 'exclude');
      await repo.setCategoryMembership(category.id, outsider.id, 'include');

      let members = await repo.categoryMembers(category.id);
      expect(members.items.map((m) => m.contact.displayName)).not.toContain('סופר מודר');
      expect(members.items.find((m) => m.contact.id === outsider.id)?.membership).toBe('manual');

      await repo.setCategoryMembership(category.id, scribe.id, 'auto');
      await repo.setCategoryMembership(category.id, outsider.id, 'auto');
      members = await repo.categoryMembers(category.id);
      expect(members.items.map((m) => m.contact.displayName)).toContain('סופר מודר');
      expect(members.items.map((m) => m.contact.displayName)).not.toContain('מצורף ביד');
    });

    it('editing the rule re-selects the members, and a preview never writes', async () => {
      const repo = await makeRepository();
      await repo.createContact({ ...blankContact, displayName: 'נגר לבדיקה', profession: 'נגר' });
      const category = await repo.createCategory({
        name: 'כלל שמתחלף',
        description: null,
        parentId: null,
        icon: null,
        color: null,
        rule: scribeRule,
        showOnHome: true,
      });

      const carpenters = {
        match: 'all' as const,
        conditions: [{ field: 'occupation' as const, op: 'contains' as const, values: ['נגר'] }],
      };
      const before = (await repo.getCategory(category.id))?.count;
      const preview = await repo.previewCategoryRule(carpenters);
      expect(preview.count).toBeGreaterThanOrEqual(1);
      expect(preview.sample.map((c) => c.displayName)).toContain('נגר לבדיקה');
      // Previewing a different rule changed nothing on the stored category.
      expect((await repo.getCategory(category.id))?.count).toBe(before);

      await repo.updateCategory(category.id, {
        name: 'נגרים',
        description: null,
        parentId: null,
        icon: 'wrench',
        color: null,
        rule: carpenters,
        showOnHome: false,
      });
      const after = await repo.getCategory(category.id);
      expect(after?.name).toBe('נגרים');
      expect(after?.showOnHome).toBe(false);
      const members = await repo.categoryMembers(category.id);
      expect(members.items.map((m) => m.contact.displayName)).toContain('נגר לבדיקה');
      expect(members.items.map((m) => m.contact.displayName)).not.toContain('סופר לבדיקה');
    });

    it('a rule category is searchable by its name and filterable as a facet', async () => {
      const repo = await makeRepository();
      await repo.createContact({
        ...blankContact,
        displayName: 'שרברב מהכלל',
        profession: 'שרברב',
      });
      await repo.createCategory({
        name: 'אינסטלציה',
        description: null,
        parentId: null,
        icon: null,
        color: null,
        rule: {
          match: 'all',
          conditions: [{ field: 'occupation', op: 'contains', values: ['שרברב'] }],
        },
        showOnHome: true,
      });

      const byName = await repo.search({
        text: 'אינסטלציה',
        sort: 'relevance',
        limit: 20,
        offset: 0,
        favoritesOnly: false,
        includeDeleted: false,
      });
      expect(byName.results.map((r) => r.contact.displayName)).toContain('שרברב מהכלל');

      const filtered = await repo.search({
        text: '',
        filters: { category: ['אינסטלציה'] },
        sort: 'name',
        limit: 20,
        offset: 0,
        favoritesOnly: false,
        includeDeleted: false,
      });
      expect(filtered.results.map((r) => r.contact.displayName)).toContain('שרברב מהכלל');
    });

    it('orders categories as asked and deletes cleanly', async () => {
      const repo = await makeRepository();
      const blank = { description: null, parentId: null, icon: null, color: null, rule: null, showOnHome: true };
      const first = await repo.createCategory({ ...blank, name: 'סדר א' });
      const second = await repo.createCategory({ ...blank, name: 'סדר ב' });

      await repo.reorderCategories([second.id, first.id]);
      const ordered = (await repo.listCategories())
        .filter((c) => c.id === first.id || c.id === second.id)
        .map((c) => c.id);
      expect(ordered).toEqual([second.id, first.id]);

      await repo.deleteCategory(first.id);
      expect(await repo.getCategory(first.id)).toBeNull();
      expect((await repo.listCategories()).some((c) => c.id === first.id)).toBe(false);
    });

    it('suggests shelves from recurring professions', async () => {
      const repo = await makeRepository();
      for (let index = 0; index < 4; index += 1) {
        await repo.createContact({
          ...blankContact,
          displayName: `שדכן ${index}`,
          profession: 'שדכן',
        });
      }
      const suggestions = await repo.suggestCategories();
      const match = suggestions.find((suggestion) => suggestion.name.includes('שדכן'));
      expect(match).toBeDefined();
      expect(match!.count).toBeGreaterThanOrEqual(4);
      expect(match!.rule.conditions[0]?.values).toContain('שדכן');
    });
  });
}
