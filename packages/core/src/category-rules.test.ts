import { describe, expect, it } from 'vitest';
import { loadSeed } from '@yanuka/database';
import type { CategoryRule } from '@yanuka/types';
import { containsWord, describeRule, evaluateRule } from './category-rules.js';

const seed = loadSeed();
const byName = (name: string) => {
  const contact = seed.contacts.find((candidate) => candidate.displayName === name);
  if (!contact) throw new Error(`no seed contact ${name}`);
  return contact;
};

describe('category rules', () => {
  it('contains is a word-start match, not a substring', () => {
    expect(containsWord('רב קהילה', 'רב')).toBe(true);
    expect(containsWord('רבנים', 'רב')).toBe(true);
    expect(containsWord('סוחר ערבים', 'רב')).toBe(false);
    expect(containsWord('ראש ישיבה', 'ראש ישיבה')).toBe(true);
  });

  it('selects rabbis abroad and nobody else', () => {
    const rule: CategoryRule = {
      match: 'all',
      conditions: [
        { field: 'occupation', op: 'contains', values: ['רב', 'הרב', 'דיין'] },
        { field: 'country', op: 'is_not', values: ['IL'] },
      ],
    };
    const abroad = seed.contacts.filter((contact) => evaluateRule(rule, contact));
    expect(abroad.length).toBeGreaterThan(0);
    for (const contact of abroad) {
      expect(contact.country).not.toBe('IL');
    }
    // A scribe in Jerusalem is not a rabbi abroad.
    expect(evaluateRule(rule, byName('ישראל סופר'))).toBe(false);
  });

  it('normalizes gershayim so סת"ם and סתם agree', () => {
    const rule: CategoryRule = {
      match: 'any',
      conditions: [{ field: 'occupation', op: 'contains', values: ['סת"ם'] }],
    };
    expect(evaluateRule(rule, byName('ישראל סופר'))).toBe(true);
  });

  it('empty and non-empty checks read the child collections', () => {
    const noPhone: CategoryRule = {
      match: 'all',
      conditions: [{ field: 'phone', op: 'is_empty', values: [] }],
    };
    const withPhone = seed.contacts.find((contact) => contact.phones.length > 0)!;
    expect(evaluateRule(noPhone, withPhone)).toBe(false);
    expect(
      evaluateRule(noPhone, { ...withPhone, phones: [] }),
    ).toBe(true);
  });

  it('within_days uses the injected clock', () => {
    const rule: CategoryRule = {
      match: 'all',
      conditions: [{ field: 'created', op: 'within_days', values: ['30'] }],
    };
    const contact = { ...byName('ישראל סופר'), createdAt: '2026-08-01T00:00:00Z' };
    expect(evaluateRule(rule, contact, { now: '2026-08-15T00:00:00Z' })).toBe(true);
    expect(evaluateRule(rule, contact, { now: '2026-10-01T00:00:00Z' })).toBe(false);
  });

  it('describes a rule in Hebrew', () => {
    const text = describeRule({
      match: 'all',
      conditions: [
        { field: 'occupation', op: 'contains', values: ['רב', 'דיין'] },
        { field: 'country', op: 'is_not', values: ['IL'] },
      ],
    });
    expect(text).toContain('מכיל רב / דיין');
    expect(text).toContain('וגם');
    expect(describeRule(null)).toBe('קטגוריה ידנית');
  });
});
