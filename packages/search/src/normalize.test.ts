import { describe, expect, it } from 'vitest';
import {
  expandToken,
  hebrewPhoneticKey,
  latinPhoneticKey,
  normalizeAndExpand,
  normalizeName,
  normalizeText,
  phoneticKey,
  stripHonorifics,
} from './normalize.js';

describe('normalizeText', () => {
  it('strips niqqud and te‘amim', () => {
    expect(normalizeText('שָׁלוֹם')).toBe('שלומ');
    expect(normalizeText('מֹשֶׁה')).toBe('משה');
    expect(normalizeText('אַבְרָהָם')).toBe('אברהמ');
  });

  it('removes geresh and gershayim in every encoding', () => {
    // U+05F3/U+05F4, ASCII quotes and curly quotes must all behave identically.
    expect(normalizeText('ר׳ משה')).toBe('ר משה');
    expect(normalizeText("ר' משה")).toBe('ר משה');
    // Final mem also folds to a medial mem, hence `סתמ` rather than `סתם`.
    expect(normalizeText('סת״ם')).toBe('סתמ');
    expect(normalizeText('סת"ם')).toBe('סתמ');
    expect(normalizeText('סת”ם')).toBe('סתמ');
  });

  it('folds final letters to their medial form', () => {
    expect(normalizeText('סתם')).toBe('סתמ');
    expect(normalizeText('כהן')).toBe('כהנ');
    expect(normalizeText('ירושלים')).toBe('ירושלימ');
  });

  it('turns maqaf and hyphens into spaces', () => {
    expect(normalizeText('בן־גוריון')).toBe('בנ גוריונ');
    expect(normalizeText('בני-ברק')).toBe('בני ברק');
  });

  it('folds Latin case and accents', () => {
    expect(normalizeText('José Cohen')).toBe('jose cohen');
    expect(normalizeText('MOSHE')).toBe('moshe');
  });

  it('collapses whitespace', () => {
    expect(normalizeText('  משה    כהן  ')).toBe('משה כהנ');
  });

  it('is stable when applied twice', () => {
    const once = normalizeText('הרב מֹשֶׁה כֹּהֵן שליט״א');
    expect(normalizeText(once)).toBe(once);
  });

  it('handles null and empty input', () => {
    expect(normalizeText(null)).toBe('');
    expect(normalizeText(undefined)).toBe('');
    expect(normalizeText('   ')).toBe('');
  });
});

describe('stripHonorifics', () => {
  it('removes a single honorific', () => {
    expect(stripHonorifics(normalizeText('הרב משה כהן'))).toBe('משה כהנ');
    expect(stripHonorifics(normalizeText("ר' משה כהן"))).toBe('משה כהנ');
    expect(stripHonorifics(normalizeText('רבי משה כהן'))).toBe('משה כהנ');
  });

  it('removes stacked honorifics', () => {
    expect(stripHonorifics(normalizeText('הרב הגאון רבי משה כהן'))).toBe('משה כהנ');
  });

  it('removes Latin honorifics', () => {
    expect(stripHonorifics(normalizeText('Rabbi Moshe Cohen'))).toBe('moshe cohen');
    expect(stripHonorifics(normalizeText('Dr. Sarah Levy'))).toBe('sarah levy');
  });

  it('never consumes the entire name', () => {
    // `רב` alone is the whole name here; stripping it would leave nothing.
    expect(stripHonorifics(normalizeText('הרב'))).toBe('הרב');
    expect(stripHonorifics(normalizeText('רבי'))).toBe('רבי');
  });

  it('makes the honorific variants of one person converge', () => {
    const forms = ['הרב משה כהן', "ר' משה כהן", 'רבי משה כהן', 'משה כהן', 'הרב מֹשֶׁה כֹּהֵן'];
    const normalized = new Set(forms.map(normalizeName));
    expect(normalized.size).toBe(1);
  });
});

describe('expandToken', () => {
  it('strips a proclitic when a real stem remains', () => {
    expect(expandToken('מלונדון')).toContain('לונדון');
    expect(expandToken('בירושלים')).toContain('ירושלים');
    expect(expandToken('ולונדון')).toContain('לונדון');
  });

  it('always keeps the original token first', () => {
    expect(expandToken('מלונדון')[0]).toBe('מלונדון');
  });

  // These are the words that a looser bound would destroy. Each one begins with
  // a proclitic letter but is not a prefixed form of anything.
  it.each([
    ['מלון', 'לון'],
    ['שלום', 'לום'],
    ['בית', 'ית'],
    ['לוי', 'וי'],
    ['משה', 'שה'],
    ['הרב', 'רב'],
    ['בני', 'ני'],
    ['כהן', 'הן'],
  ])('does not strip %s into %s', (token, forbidden) => {
    expect(expandToken(normalizeText(token))).not.toContain(normalizeText(forbidden));
  });

  it('handles two-letter proclitic clusters', () => {
    expect(expandToken('ובירושלים')).toContain('ירושלים');
  });
});

describe('normalizeAndExpand', () => {
  it('produces every searchable variant of a phrase', () => {
    const variants = normalizeAndExpand('יהודי מלונדון');
    expect(variants).toContain('יהודי');
    expect(variants).toContain('מלונדונ');
    expect(variants).toContain('לונדונ');
  });

  it('returns an empty list for empty input', () => {
    expect(normalizeAndExpand('')).toEqual([]);
  });
});

describe('phonetic keys', () => {
  it('collapses Hebrew matres lectionis spelling variants', () => {
    // The spec's example: פרידמן and פרידמאן must land on the same key.
    expect(hebrewPhoneticKey(normalizeText('פרידמן'))).toBe(
      hebrewPhoneticKey(normalizeText('פרידמאן')),
    );
  });

  it('collapses Latin transliteration variants', () => {
    expect(latinPhoneticKey('Friedman')).toBe(latinPhoneticKey('Freidman'));
    expect(latinPhoneticKey('Moshe')).toBe(latinPhoneticKey('Moishe'));
    expect(latinPhoneticKey('Cohen')).toBe(latinPhoneticKey('Kohen'));
  });

  it('keeps genuinely different names apart', () => {
    expect(latinPhoneticKey('Friedman')).not.toBe(latinPhoneticKey('Goldman'));
    expect(hebrewPhoneticKey(normalizeText('כהן'))).not.toBe(
      hebrewPhoneticKey(normalizeText('לוי')),
    );
  });

  it('dispatches on script', () => {
    expect(phoneticKey('משה')).toBe(hebrewPhoneticKey(normalizeText('משה')));
    expect(phoneticKey('Moshe')).toBe(latinPhoneticKey('moshe'));
  });

  it('returns empty for empty input', () => {
    expect(phoneticKey('')).toBe('');
  });
});
