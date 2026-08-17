import { describe, expect, it } from 'vitest';
import { loadSeed } from '@yanuka/database';
import { MockRepository } from './mock-repository.js';
import { runRepositoryContractTests } from './contract-tests.js';

// A fresh seed per repository — the contract suite mutates its store.
runRepositoryContractTests('MockRepository', () => new MockRepository(loadSeed()));

/**
 * Search behaviour against the demo dataset. These are the scenarios from the
 * product brief, expressed as assertions: they are what "the search must be
 * exceptionally good" means in practice.
 */
describe('search over the demo dataset', () => {
  const repo = () => new MockRepository(loadSeed());

  const find = async (text: string) => {
    const response = await repo().search({
      text,
      sort: 'relevance',
      limit: 50,
      offset: 0,
      favoritesOnly: false,
      includeDeleted: false,
    });
    return response;
  };

  const names = (response: Awaited<ReturnType<typeof find>>) =>
    response.results.map((result) => result.contact.displayName);

  it('seeds at least 50 contacts, as the brief requires', async () => {
    const stats = await repo().stats();
    expect(stats.contacts).toBeGreaterThanOrEqual(50);
  });

  it('finds a scribe in Jerusalem who works on tefillin', async () => {
    const response = await find('סופר סתם ירושלים');
    expect(names(response)).toContain('ישראל סופר');
  });

  it('finds someone by a phrase from a note when the name is forgotten', async () => {
    const response = await find('מלונדון נדלן');
    expect(names(response)).toContain('מיכאל גולדשטיין');
  });

  it('finds a person through a Hebrew grammatical prefix', async () => {
    // The note says `יהודי מלונדון`; the user types the bare city name.
    const response = await find('לונדון');
    expect(names(response).length).toBeGreaterThan(0);
  });

  it('reaches a contact through a Latin transliteration alias', async () => {
    const response = await find('Friedman');
    expect(names(response)).toContain('יעקב פרידמן');
  });

  it('tolerates a misspelling', async () => {
    // `פרידמאן` with an extra alef must still reach `פרידמן`.
    const response = await find('פרידמאן');
    expect(names(response)).toContain('יעקב פרידמן');
  });

  it('ignores honorifics on both sides of the comparison', async () => {
    const withTitle = await find('הרב אברהם כהן');
    const without = await find('אברהם כהן');
    expect(names(withTitle)).toContain('אברהם כהן');
    expect(names(without)).toContain('אברהם כהן');
  });

  it('matches a phone number typed in a different format', async () => {
    // Stored as `054-555-0134`; typed here in international form.
    const response = await find('+972 54 555 0134');
    expect(names(response)).toContain('אברהם כהן');
  });

  it('matches a phone number by its last digits alone', async () => {
    const response = await find('5550134');
    expect(names(response)).toContain('אברהם כהן');
  });

  it('narrows rather than widens as words are added', async () => {
    const broad = await find('סת"ם');
    const narrow = await find('סת"ם לונדון');
    expect(narrow.total).toBeLessThan(broad.total);
    expect(names(narrow)).toContain('אהרן ברנר');
  });

  it('produces facet counts that can drive the filter panel', async () => {
    const response = await find('סת"ם');
    expect(response.facets.country?.length).toBeGreaterThan(0);
    const israel = response.facets.country?.find((facet) => facet.value === 'IL');
    expect(israel?.label).toBe('ישראל');
    expect(israel?.count).toBeGreaterThan(0);
  });

  it('explains why each result matched', async () => {
    const response = await find('תפילין');
    expect(response.results.length).toBeGreaterThan(0);
    for (const result of response.results) {
      expect(result.reasons.length).toBeGreaterThan(0);
    }
  });

  it('returns a readable snippet when the match came from a note', async () => {
    const response = await find('בורו פארק');
    const withSnippet = response.results.find((result) =>
      result.reasons.some((reason) => reason.source === 'notes' && reason.snippet),
    );
    expect(withSnippet).toBeDefined();
  });

  it('ranks an exact name match above a passing mention in a note', async () => {
    const response = await find('אברהם כהן');
    expect(names(response)[0]).toBe('אברהם כהן');
  });

  it('keeps two similarly-named people distinct', async () => {
    const response = await find('ישראל מאיר');
    const found = names(response);
    expect(found).toContain('ישראל מאיר הכהן');
    expect(found).toContain('ישראל מאיר כהן');
  });

  it('supports an inline field filter', async () => {
    const response = await find('עיר:ירושלים סופר');
    expect(names(response)).toContain('ישראל סופר');
    expect(names(response)).not.toContain('בנימין גוטמן'); // Brooklyn
  });

  it('excludes terms prefixed with a minus', async () => {
    const withAll = await find('סת"ם');
    const excluded = await find('סת"ם -לונדון');
    expect(excluded.total).toBeLessThan(withAll.total);
  });

  it('answers an empty query with a browse rather than an error', async () => {
    const response = await find('');
    expect(response.results.length).toBeGreaterThan(0);
  });

  it('suggests contacts while typing', async () => {
    const suggestions = await repo().suggest('משה');
    expect(suggestions.length).toBeGreaterThan(0);
    expect(suggestions[0]!.kind).toBe('contact');
  });
});

describe('relationship graph', () => {
  it('materializes incoming edges so the detail screen can show who recommended whom', async () => {
    const seed = loadSeed();
    const repo = new MockRepository(seed);
    const scribe = seed.contacts.find((contact) => contact.displayName === 'ישראל סופר')!;

    const loaded = await repo.getContact(scribe.id);
    const incoming = loaded!.relationships.filter((edge) => edge.direction === 'in');

    expect(incoming.length).toBeGreaterThan(0);
    expect(incoming.some((edge) => edge.otherContact.displayName === 'אברהם כהן')).toBe(true);
  });

  it('links free-text "introduced by" to the real record where it matches', async () => {
    const seed = loadSeed();
    const weiss = seed.contacts.find((contact) => contact.displayName === 'שמואל וייס')!;
    expect(weiss.introducedByContactId).not.toBeNull();
  });
});
