import { describe, expect, it } from 'vitest';
import {
  collapseWhitespace,
  editDistance,
  similarity,
  snippetAround,
  snippetAroundNormalized,
  stripDiacritics,
  tokenize,
  trigrams,
} from './text.js';

describe('editDistance', () => {
  it('is zero for identical strings', () => {
    expect(editDistance('משה', 'משה')).toBe(0);
  });

  it('counts a substitution as one', () => {
    expect(editDistance('כהן', 'כהל')).toBe(1);
  });

  it('counts a transposition as one, not two', () => {
    // This is the Damerau part, and it is what catches ordinary typing slips.
    expect(editDistance('friedman', 'firedman')).toBe(1);
  });

  it('bails out early once the ceiling is exceeded', () => {
    expect(editDistance('abcdefgh', 'zyxwvuts', 2)).toBeGreaterThan(2);
  });

  it('handles empty strings', () => {
    expect(editDistance('', 'abc')).toBe(3);
    expect(editDistance('abc', '')).toBe(3);
  });
});

describe('similarity', () => {
  it('is 1 for identical strings', () => {
    expect(similarity('כהן', 'כהן')).toBe(1);
  });

  it('rates a one-letter difference in a long name highly', () => {
    expect(similarity('פרידמן', 'פרידמאן')).toBeGreaterThan(0.8);
  });

  it('rates unrelated names low', () => {
    expect(similarity('כהן', 'רוזנברג')).toBeLessThan(0.4);
  });
});

describe('stripDiacritics', () => {
  it('removes Hebrew niqqud', () => {
    expect(stripDiacritics('שָׁלוֹם')).toBe('שלום');
  });

  it('removes Latin accents', () => {
    expect(stripDiacritics('José')).toBe('Jose');
  });
});

describe('tokenize', () => {
  it('splits on punctuation and whitespace', () => {
    expect(tokenize('משה, כהן')).toEqual(['משה', 'כהן']);
  });

  it('keeps digits', () => {
    expect(tokenize('רחוב 5')).toEqual(['רחוב', '5']);
  });
});

describe('collapseWhitespace', () => {
  it('collapses runs and trims', () => {
    expect(collapseWhitespace('  משה    כהן  ')).toBe('משה כהן');
  });
});

describe('trigrams', () => {
  it('pads so short strings still produce grams', () => {
    expect(trigrams('ab').length).toBeGreaterThan(0);
  });

  it('returns nothing for empty input', () => {
    expect(trigrams('')).toEqual([]);
  });
});

describe('snippetAround', () => {
  it('cuts a window around a literal match', () => {
    const text = 'א'.repeat(100) + ' תפילין ' + 'ב'.repeat(100);
    const snippet = snippetAround(text, ['תפילין']);
    expect(snippet).toContain('תפילין');
    expect(snippet!.length).toBeLessThan(text.length);
    expect(snippet).toContain('…');
  });

  it('returns null when nothing matches', () => {
    expect(snippetAround('שלום', ['מזוזה'])).toBeNull();
  });
});

describe('snippetAroundNormalized', () => {
  // The term reaching this function has been folded, so it no longer occurs
  // literally in the source text. A plain indexOf would always miss.
  const fold = (value: string) =>
    value.replace(/["'״׳]/g, '').replace(/[ךםןףץ]/g, (ch) => ({ ך: 'כ', ם: 'מ', ן: 'נ', ף: 'פ', ץ: 'צ' })[ch]!);

  it('finds a word whose stored form differs after normalization', () => {
    const text = 'הוא עוסק בעיקר בתפילין ומזוזות ובכתיבת ספרי תורה לבתי כנסת';
    // `תפילין` folds to `תפילינ`, which does not appear literally in the text.
    const snippet = snippetAroundNormalized(text, 'תפילינ', fold);
    expect(snippet).not.toBeNull();
    expect(snippet).toContain('תפילין');
  });

  it('returns text exactly as the user wrote it, gershayim included', () => {
    const text = 'סופר סת"ם מומלץ מאוד';
    // The term arrives already folded, exactly as the search engine produces it.
    const snippet = snippetAroundNormalized(text, fold('סתם'), fold);
    expect(snippet).toContain('סת"ם');
  });

  it('returns null when no word matches', () => {
    expect(snippetAroundNormalized('שלום עליכם', 'מזוזה', fold)).toBeNull();
  });

  it('returns null for empty input', () => {
    expect(snippetAroundNormalized('', 'משה', fold)).toBeNull();
    expect(snippetAroundNormalized('משה', '', fold)).toBeNull();
  });
});
