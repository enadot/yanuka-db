#!/usr/bin/env node
/**
 * Print the CHANGELOG.md section for one version, for use as GitHub release
 * notes. Fails when the section is missing, so a tag cannot be released
 * without its changelog entry having been written first.
 *
 *   node scripts/release-notes.mjs 0.2.0
 */
import { readFileSync } from 'node:fs';

const version = process.argv[2];
if (!version) {
  console.error('שימוש: node scripts/release-notes.mjs <גרסה>');
  process.exit(1);
}

const changelog = readFileSync(new URL('../CHANGELOG.md', import.meta.url), 'utf8');
const lines = changelog.split('\n');

const start = lines.findIndex((line) => line.startsWith(`## ${version}`));
if (start === -1) {
  console.error(`אין ב־CHANGELOG.md סעיף לגרסה ${version} — יש לכתוב אותו לפני שמתייגים`);
  process.exit(1);
}

const end = lines.findIndex((line, index) => index > start && line.startsWith('## '));
const section = lines
  .slice(start + 1, end === -1 ? lines.length : end)
  .join('\n')
  .trim();

if (!section) {
  console.error(`הסעיף של גרסה ${version} ב־CHANGELOG.md ריק`);
  process.exit(1);
}

console.log(section);
