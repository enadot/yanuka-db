import { describe, expect, it } from 'vitest';
import {
  ContactInputSchema,
  PhoneInputSchema,
  QuickAddContactSchema,
  RelationshipInputSchema,
  UlidSchema,
} from './index.js';

describe('ContactInputSchema', () => {
  it('accepts a contact with nothing but a name', () => {
    // The central product decision: a half-remembered person must be storable.
    const result = ContactInputSchema.safeParse({ displayName: 'החשמלאי מאנטוורפן' });
    expect(result.success).toBe(true);
    expect(result.data?.phones).toEqual([]);
    expect(result.data?.isFavorite).toBe(false);
  });

  it('rejects a blank or whitespace-only name', () => {
    expect(ContactInputSchema.safeParse({ displayName: '' }).success).toBe(false);
    expect(ContactInputSchema.safeParse({ displayName: '   ' }).success).toBe(false);
  });

  it('turns a blank optional field into null rather than an empty string', () => {
    const result = ContactInputSchema.parse({ displayName: 'משה', city: '  ' });
    expect(result.city).toBeNull();
  });

  it('requires an uppercase two-letter country code', () => {
    expect(ContactInputSchema.safeParse({ displayName: 'משה', country: 'IL' }).success).toBe(true);
    expect(ContactInputSchema.safeParse({ displayName: 'משה', country: 'il' }).success).toBe(false);
    expect(ContactInputSchema.safeParse({ displayName: 'משה', country: 'ISR' }).success).toBe(false);
  });

  it('accepts long free-text notes', () => {
    const result = ContactInputSchema.safeParse({
      displayName: 'משה',
      notes: 'א'.repeat(5000),
    });
    expect(result.success).toBe(true);
  });

  it('rejects notes beyond the storage limit', () => {
    const result = ContactInputSchema.safeParse({
      displayName: 'משה',
      notes: 'א'.repeat(10_001),
    });
    expect(result.success).toBe(false);
  });

  it('validates an email address', () => {
    expect(
      ContactInputSchema.safeParse({
        displayName: 'משה',
        emails: [{ address: 'not-an-email' }],
      }).success,
    ).toBe(false);

    expect(
      ContactInputSchema.safeParse({
        displayName: 'משה',
        emails: [{ address: 'moshe@example.org' }],
      }).success,
    ).toBe(true);
  });
});

describe('PhoneInputSchema', () => {
  it('accepts a number that no parser could understand', () => {
    // Deliberately permissive; normalization is best-effort, not a gate.
    const result = PhoneInputSchema.safeParse({ raw: 'בבית של אדלר' });
    expect(result.success).toBe(true);
  });

  it('rejects an empty number', () => {
    expect(PhoneInputSchema.safeParse({ raw: '' }).success).toBe(false);
    expect(PhoneInputSchema.safeParse({ raw: '   ' }).success).toBe(false);
  });

  it('defaults the kind to mobile', () => {
    expect(PhoneInputSchema.parse({ raw: '054-555-0134' }).kind).toBe('mobile');
  });
});

describe('QuickAddContactSchema', () => {
  it('needs only a name', () => {
    expect(QuickAddContactSchema.safeParse({ displayName: 'מישהו' }).success).toBe(true);
  });
});

describe('RelationshipInputSchema', () => {
  const a = '01J0000000000000000000000A';
  const b = '01J0000000000000000000000B';

  it('accepts an edge between two contacts', () => {
    expect(
      RelationshipInputSchema.safeParse({ fromContactId: a, toContactId: b, type: 'recommended' })
        .success,
    ).toBe(true);
  });

  it('refuses to link a contact to itself', () => {
    const result = RelationshipInputSchema.safeParse({
      fromContactId: a,
      toContactId: a,
      type: 'knows',
    });
    expect(result.success).toBe(false);
  });
});

describe('UlidSchema', () => {
  it('accepts a well-formed ULID', () => {
    expect(UlidSchema.safeParse('01J0000000000000000000000A').success).toBe(true);
  });

  it('rejects a UUID or a plain integer', () => {
    expect(UlidSchema.safeParse('550e8400-e29b-41d4-a716-446655440000').success).toBe(false);
    expect(UlidSchema.safeParse('42').success).toBe(false);
  });

  it('rejects the ambiguous Crockford letters', () => {
    // I, L, O and U are excluded from the alphabet to avoid transcription errors.
    expect(UlidSchema.safeParse('01JIIIIIIIIIIIIIIIIIIIIIII').success).toBe(false);
  });
});
