import { describe, expect, it } from 'vitest';
import { parseCsv } from '@yanuka/utils';
import type { ContactWithRelations } from '@yanuka/types';
import { buildContactsCsv } from './export.js';
import { buildImportPlan, suggestMapping } from './import.js';

function contact(overrides: Partial<ContactWithRelations>): ContactWithRelations {
  const base = {
    id: '01TEST',
    firstName: null,
    lastName: null,
    displayName: '',
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
    lastViewedAt: null,
    createdAt: '2026-01-01T00:00:00Z',
    updatedAt: '2026-01-01T00:00:00Z',
    createdBy: null,
    updatedBy: null,
    version: 1,
    deviceId: null,
    deletedAt: null,
    phones: [],
    emails: [],
    aliases: [],
    tags: [],
    categories: [],
    specialties: [],
    languages: [],
    organizations: [],
    relationships: [],
    contactNotes: [],
  };
  return { ...base, ...overrides } as ContactWithRelations;
}

const syncable = {
  createdAt: '2026-01-01T00:00:00Z',
  updatedAt: '2026-01-01T00:00:00Z',
  createdBy: null,
  updatedBy: null,
  version: 1,
  deviceId: null,
  deletedAt: null,
};

const phone = (raw: string, isPrimary = false) => ({
  ...syncable,
  id: '01P',
  contactId: '01TEST',
  kind: 'mobile' as const,
  raw,
  e164: null,
  digits: raw.replaceAll(/\D/g, ''),
  countryCode: null,
  isPrimary,
  label: null,
});

describe('buildContactsCsv', () => {
  it('round-trips through the importer with the mapping auto-detected', () => {
    const source = [
      contact({
        displayName: 'הרב אברהם כהן',
        firstName: 'אברהם',
        lastName: 'כהן',
        prefix: 'הרב',
        city: 'ירושלים',
        profession: 'סופר סתם',
        notes: 'אמר: "נדבר אחרי החג", לחזור אליו',
        reasonForSaving: 'הומלץ ע"י אדלר',
        introducedBy: 'אדלר',
        phones: [phone('054-5550134', true), phone('02-6521234')],
        emails: [
          {
            ...syncable,
            id: '01E',
            contactId: '01TEST',
            kind: 'personal' as const,
            address: 'avraham@example.com',
            normalized: 'avraham@example.com',
            isPrimary: true,
          },
        ],
      }),
      contact({ displayName: 'החשמלאי מאנטוורפן' }),
    ] as ContactWithRelations[];

    const csv = buildContactsCsv(source);
    const parsed = parseCsv(csv);
    const mapping = suggestMapping(parsed.headers);

    // Every exported column is recognized — nothing lands on `ignore`.
    expect(mapping).not.toContain('ignore');

    const plan = buildImportPlan(parsed.rows, mapping);
    expect(plan.every((row) => row.input !== null)).toBe(true);

    const [first, second] = plan.map((row) => row.input!);
    expect(first?.displayName).toBe('הרב אברהם כהן');
    expect(first?.prefix).toBe('הרב');
    expect(first?.city).toBe('ירושלים');
    expect(first?.profession).toBe('סופר סתם');
    expect(first?.notes).toContain('נדבר אחרי החג');
    expect(first?.reasonForSaving).toBe('הומלץ ע"י אדלר');
    expect(first?.introducedBy).toBe('אדלר');
    expect(first?.phones.map((p) => p.raw)).toEqual(['054-5550134', '02-6521234']);
    expect(first?.phones[0]?.isPrimary).toBe(true);
    expect(first?.emails[0]?.address).toBe('avraham@example.com');
    // A name-only record — the product's most important kind — survives too.
    expect(second?.displayName).toBe('החשמלאי מאנטוורפן');
  });

  it('starts with a BOM so Excel decodes the Hebrew correctly', () => {
    const csv = buildContactsCsv([contact({ displayName: 'משה לוי' })]);
    expect(csv.charCodeAt(0)).toBe(0xfeff);
    expect(csv).toContain('\r\n');
  });
});
