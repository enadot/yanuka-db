import { describe, expect, it } from 'vitest';
import {
  digitsOnly,
  looksLikePhoneQuery,
  normalizePhone,
  phoneSearchKey,
  telHref,
  whatsappHref,
} from './phone.js';

describe('normalizePhone', () => {
  it('parses a local Israeli number when the country is known', () => {
    const result = normalizePhone('054-555-0134', 'IL');
    expect(result.e164).toBe('+972545550134');
    expect(result.countryCode).toBe('IL');
    expect(result.digits).toBe('972545550134');
  });

  it('parses an international number without a country hint', () => {
    const result = normalizePhone('+44 7700 900123');
    expect(result.e164).toBe('+447700900123');
    // `countryCode` stays null here: 07700 900xxx is Ofcom's reserved drama
    // range and matches no assignable GB pattern, so libphonenumber declines to
    // name a country even though it produces a valid E.164. Callers must treat
    // the country as optional rather than assuming a parse implies one.
    expect(result.countryCode).toBeNull();
  });

  it('names the country for an assignable international number', () => {
    expect(normalizePhone('+972 54 555 0134').countryCode).toBe('IL');
  });

  it('never rewrites what the user typed', () => {
    const result = normalizePhone('054-555-0134', 'IL');
    expect(result.raw).toBe('054-555-0134');
  });

  it('keeps an unparseable number instead of throwing it away', () => {
    // Real notebook entries look like this. Rejecting them would lose the only
    // trace of that person.
    const result = normalizePhone('בבית של אדלר');
    expect(result.raw).toBe('בבית של אדלר');
    expect(result.e164).toBeNull();
    expect(result.isValid).toBe(false);
  });

  it('keeps a number with an extension annotation', () => {
    const result = normalizePhone('02-6521234 שלוחה 4', 'IL');
    expect(result.raw).toBe('02-6521234 שלוחה 4');
    expect(result.digits.length).toBeGreaterThan(0);
  });

  it('handles empty input', () => {
    const result = normalizePhone('   ');
    expect(result.raw).toBe('');
    expect(result.digits).toBe('');
  });
});

describe('phoneSearchKey', () => {
  it('reduces every format of one number to the same key', () => {
    const keys = ['054-555-0134', '+972 54 555 0134', '0545550134', '+972545550134'].map((input) =>
      phoneSearchKey(normalizePhone(input, 'IL').digits),
    );
    expect(new Set(keys).size).toBe(1);
  });

  it('leaves a short input as typed', () => {
    expect(phoneSearchKey('1234')).toBe('1234');
  });
});

describe('looksLikePhoneQuery', () => {
  it('accepts digit runs', () => {
    expect(looksLikePhoneQuery('0545550134')).toBe(true);
    expect(looksLikePhoneQuery('+972 54 555 0134')).toBe(true);
    expect(looksLikePhoneQuery('054-555')).toBe(true);
  });

  it('rejects anything containing letters', () => {
    expect(looksLikePhoneQuery('משה')).toBe(false);
    expect(looksLikePhoneQuery('רחוב 5')).toBe(false);
  });

  it('rejects digit runs too short to be selective', () => {
    expect(looksLikePhoneQuery('12')).toBe(false);
    expect(looksLikePhoneQuery('')).toBe(false);
  });
});

describe('link helpers', () => {
  it('builds a tel: link from E.164 when available', () => {
    expect(telHref(normalizePhone('054-555-0134', 'IL'))).toBe('tel:+972545550134');
  });

  it('falls back to the raw number for a tel: link', () => {
    expect(telHref({ e164: null, raw: '02 555 0187' })).toBe('tel:025550187');
  });

  it('returns no WhatsApp link for an unparseable number', () => {
    expect(whatsappHref({ e164: null })).toBeNull();
  });

  it('builds a WhatsApp link from E.164', () => {
    expect(whatsappHref(normalizePhone('054-555-0134', 'IL'))).toBe('https://wa.me/972545550134');
  });
});

describe('digitsOnly', () => {
  it('strips everything that is not a digit', () => {
    expect(digitsOnly('+972 (54) 555-0134')).toBe('972545550134');
    expect(digitsOnly('שלוחה 4')).toBe('4');
  });
});
