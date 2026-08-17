import { createRequire } from 'node:module';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { describe, expect, it } from 'vitest';

/**
 * Guards the application bundle against test-only code.
 *
 * `@yanuka/core` used to re-export `contract-tests`, which imports `vitest`.
 * A production build tree-shook it away and looked perfectly healthy, so every
 * check passed — but the dev server serves modules as written, and the browser
 * died on the vitest import with a blank page. The failure only appeared on a
 * developer's machine, which is the worst place to discover it.
 *
 * This walks the real module graph of every workspace package the app imports
 * and fails if anything reachable from a package entry point pulls in a test
 * framework.
 */

const require = createRequire(import.meta.url);

const TEST_ONLY_IMPORTS = ['vitest', '@playwright/test', 'node:test'];

const APP_PACKAGES = [
  '@yanuka/core',
  '@yanuka/types',
  '@yanuka/utils',
  '@yanuka/search',
  '@yanuka/validation',
  '@yanuka/database',
];

/** Follow relative re-exports out of an entry point and collect every file. */
function reachableFiles(entry: string, seen = new Set<string>()): string[] {
  if (seen.has(entry)) return [];
  seen.add(entry);

  let source: string;
  try {
    source = readFileSync(entry, 'utf8');
  } catch {
    return [];
  }

  const files = [entry];
  const importPattern = /(?:from|import)\s+['"](\.[^'"]+)['"]/g;
  for (const match of source.matchAll(importPattern)) {
    const specifier = match[1]!;
    // Emitted JavaScript refers to `./x.js`; resolve against the entry's dir.
    const resolved = join(dirname(entry), specifier);
    files.push(...reachableFiles(resolved, seen));
  }
  return files;
}

describe('application bundle safety', () => {
  it.each(APP_PACKAGES)('%s does not reach a test framework from its entry point', (name) => {
    const entry = require.resolve(name);
    const offenders: string[] = [];

    for (const file of reachableFiles(entry)) {
      const source = readFileSync(file, 'utf8');
      for (const testImport of TEST_ONLY_IMPORTS) {
        if (new RegExp(`from\\s+['"]${testImport}['"]`).test(source)) {
          offenders.push(`${file} imports ${testImport}`);
        }
      }
    }

    expect(offenders, `${name} would drag a test framework into the browser bundle`).toEqual([]);
  });
});
