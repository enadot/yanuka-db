import { describe, expect, it } from 'vitest';
import { buildContactInput, buildImportPlan, suggestMapping } from './import.js';

describe('suggestMapping', () => {
  it('recognizes a Google Contacts export header row', () => {
    const headers = [
      'Name',
      'Given Name',
      'Family Name',
      'Name Prefix',
      'E-mail 1 - Type',
      'E-mail 1 - Value',
      'Phone 1 - Type',
      'Phone 1 - Value',
      'Phone 2 - Type',
      'Phone 2 - Value',
      'Address 1 - City',
      'Address 1 - Formatted',
      'Organization 1 - Name',
      'Notes',
    ];
    expect(suggestMapping(headers)).toEqual([
      'displayName',
      'firstName',
      'lastName',
      'prefix',
      'ignore',
      'email',
      'ignore',
      'phone',
      'ignore',
      'phone',
      'city',
      'address',
      'organization',
      'notes',
    ]);
  });

  it('recognizes a hand-made Hebrew sheet', () => {
    expect(suggestMapping(['שם', 'טלפון נייד', 'עיר', 'מקצוע', 'הערות', 'מי הכיר'])).toEqual([
      'displayName',
      'phone',
      'city',
      'profession',
      'notes',
      'introducedBy',
    ]);
  });

  it('maps unknown headers to ignore rather than guessing', () => {
    expect(suggestMapping(['Birthday', 'Photo', ''])).toEqual(['ignore', 'ignore', 'ignore']);
  });
});

describe('buildContactInput', () => {
  it('builds a contact from mapped cells', () => {
    const { input, error } = buildContactInput(
      ['הרב אברהם כהן', '054-5550134', 'ירושלים', 'סופר סתם'],
      ['displayName', 'phone', 'city', 'profession'],
      'ייבוא CSV — מחברת.csv',
    );
    expect(error).toBeNull();
    expect(input?.displayName).toBe('הרב אברהם כהן');
    expect(input?.phones).toEqual([
      expect.objectContaining({ raw: '054-5550134', isPrimary: true }),
    ]);
    expect(input?.city).toBe('ירושלים');
    expect(input?.source).toBe('ייבוא CSV — מחברת.csv');
  });

  it('derives the display name from prefix, first and last name', () => {
    const { input } = buildContactInput(
      ["ר'", 'יעקב', 'פרידמן'],
      ['prefix', 'firstName', 'lastName'],
    );
    expect(input?.displayName).toBe("ר' יעקב פרידמן");
  });

  it('fails only a row with no name anywhere', () => {
    const { input, error } = buildContactInput(
      ['', '054-5550134'],
      ['displayName', 'phone'],
    );
    expect(input).toBeNull();
    expect(error).toContain('אין שם');
  });

  it('keeps a mangled phone exactly as written', () => {
    const { input } = buildContactInput(
      ['משה לוי', '02-6521234 שלוחה 4'],
      ['displayName', 'phone'],
    );
    expect(input?.phones[0]?.raw).toBe('02-6521234 שלוחה 4');
  });

  it('collects multiple phone columns, first one primary', () => {
    const { input } = buildContactInput(
      ['משה לוי', '054-5550134', '02-6521234'],
      ['displayName', 'phone', 'phone'],
    );
    expect(input?.phones.map((p) => p.raw)).toEqual(['054-5550134', '02-6521234']);
    expect(input?.phones.map((p) => p.isPrimary)).toEqual([true, false]);
  });

  it('demotes a malformed email to a notes line instead of failing the row', () => {
    const { input, error } = buildContactInput(
      ['משה לוי', 'לשאול את אדלר'],
      ['displayName', 'email'],
    );
    expect(error).toBeNull();
    expect(input?.emails).toEqual([]);
    expect(input?.notes).toContain('לשאול את אדלר');
  });

  it('routes an organization column into the notes', () => {
    const { input } = buildContactInput(
      ['משה לוי', 'ישיבת מיר'],
      ['displayName', 'organization'],
    );
    expect(input?.notes).toBe('ארגון: ישיבת מיר');
  });

  it('a mapped source column wins over the fallback', () => {
    const { input } = buildContactInput(
      ['משה לוי', 'מחברת 1998'],
      ['displayName', 'source'],
      'ייבוא CSV',
    );
    expect(input?.source).toBe('מחברת 1998');
  });
});

describe('buildImportPlan', () => {
  it('keeps failures alongside successes with 1-based row numbers', () => {
    const plan = buildImportPlan(
      [
        ['אברהם כהן', '054-5550134'],
        ['', '02-6521234'],
        ['יעקב פרידמן', ''],
      ],
      ['displayName', 'phone'],
    );
    expect(plan.map((r) => [r.row, r.input !== null])).toEqual([
      [1, true],
      [2, false],
      [3, true],
    ]);
    expect(plan[1]?.error).toContain('אין שם');
  });
});
