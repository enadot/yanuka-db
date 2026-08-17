/**
 * Script-agnostic text helpers. Hebrew-specific normalization lives in
 * @yanuka/search, which builds on these primitives.
 */

/** Collapse runs of whitespace (including NBSP and RTL marks) to single spaces. */
export function collapseWhitespace(value: string): string {
  return value.replace(/[\s ‎‏‪-‮]+/g, ' ').trim();
}

/**
 * Strip combining marks via NFD decomposition.
 *
 * Covers Hebrew niqqud and te'amim (U+0591–U+05C7 are combining marks) as well
 * as Latin accents, so `José` and `Jose`, `שָׁלוֹם` and `שלום` compare equal.
 */
export function stripDiacritics(value: string): string {
  return value.normalize('NFD').replace(/\p{M}+/gu, '').normalize('NFC');
}

/** Case fold for scripts that have case; a no-op for Hebrew. */
export function foldCase(value: string): string {
  return value.toLocaleLowerCase('en-US');
}

/** Split on any non-letter, non-digit character. Unicode-aware. */
export function tokenize(value: string): string[] {
  return value.split(/[^\p{L}\p{N}]+/u).filter((token) => token.length > 0);
}

/**
 * Damerau-Levenshtein distance with an early-exit ceiling.
 *
 * Used to rank fuzzy candidates that the trigram index already narrowed down,
 * never to scan the whole table. `maxDistance` lets the common "obviously too
 * different" case bail out after a few rows of the matrix.
 */
export function editDistance(a: string, b: string, maxDistance = Infinity): number {
  if (a === b) return 0;
  if (Math.abs(a.length - b.length) > maxDistance) return maxDistance + 1;
  if (a.length === 0) return b.length;
  if (b.length === 0) return a.length;

  // Two rolling rows plus the row before them (needed for transpositions).
  let prevPrev: number[] = [];
  let prev: number[] = Array.from({ length: b.length + 1 }, (_, i) => i);
  let current: number[] = new Array<number>(b.length + 1);

  for (let i = 1; i <= a.length; i += 1) {
    current[0] = i;
    let rowMin = current[0]!;

    for (let j = 1; j <= b.length; j += 1) {
      const cost = a[i - 1] === b[j - 1] ? 0 : 1;
      let value = Math.min(
        current[j - 1]! + 1, // insertion
        prev[j]! + 1, // deletion
        prev[j - 1]! + cost, // substitution
      );

      // Transposition: `ab` -> `ba` costs 1, not 2. This is the difference
      // between plain Levenshtein and Damerau, and it matters for typos.
      if (i > 1 && j > 1 && a[i - 1] === b[j - 2] && a[i - 2] === b[j - 1]) {
        value = Math.min(value, prevPrev[j - 2]! + cost);
      }

      current[j] = value;
      if (value < rowMin) rowMin = value;
    }

    if (rowMin > maxDistance) return maxDistance + 1;

    prevPrev = prev;
    prev = current;
    current = new Array<number>(b.length + 1);
  }

  return prev[b.length]!;
}

/**
 * Similarity in [0, 1] derived from edit distance, normalized by the longer
 * string so that a one-character typo in a short word costs more than in a
 * long one.
 */
export function similarity(a: string, b: string): number {
  if (!a && !b) return 1;
  const longest = Math.max(a.length, b.length);
  if (longest === 0) return 1;
  return 1 - editDistance(a, b) / longest;
}

/** Character trigrams, padded so short strings still produce grams. */
export function trigrams(value: string): string[] {
  if (!value) return [];
  const padded = ` ${value} `;
  const grams: string[] = [];
  for (let i = 0; i + 3 <= padded.length; i += 1) {
    grams.push(padded.slice(i, i + 3));
  }
  return grams;
}

/**
 * Extract a window of text around the first occurrence of any term, for the
 * "matched because of this note" snippet in search results.
 */
export function snippetAround(text: string, terms: string[], radius = 60): string | null {
  if (!text) return null;
  const haystack = text.toLowerCase();

  let index = -1;
  let matchedLength = 0;
  for (const term of terms) {
    if (!term) continue;
    const found = haystack.indexOf(term.toLowerCase());
    if (found !== -1 && (index === -1 || found < index)) {
      index = found;
      matchedLength = term.length;
    }
  }
  if (index === -1) return null;

  const start = Math.max(0, index - radius);
  const end = Math.min(text.length, index + matchedLength + radius);
  const prefix = start > 0 ? '…' : '';
  const suffix = end < text.length ? '…' : '';
  return `${prefix}${text.slice(start, end).trim()}${suffix}`;
}
