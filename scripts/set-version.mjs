#!/usr/bin/env node
/**
 * Set the product version everywhere it is declared, in one move.
 *
 *     node scripts/set-version.mjs 0.3.0
 *
 * The version lives in four files that must never disagree — the installer
 * takes its name from tauri.conf.json while the UI shows the desktop
 * package.json — so hand-editing one of them is how a "0.2.0" installer ends
 * up reporting 0.1.0 in the settings screen. A test in the desktop package
 * (src/version.test.ts) fails the pipeline if they drift.
 */

import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const version = process.argv[2];

if (!version || !/^\d+\.\d+\.\d+$/.test(version)) {
  console.error('שימוש: node scripts/set-version.mjs <major.minor.patch>  למשל 0.3.0');
  process.exit(1);
}

const replaceJsonVersion = (path) => {
  const source = readFileSync(path, 'utf-8');
  // A targeted replacement keeps the file's formatting byte-for-byte intact,
  // where JSON.parse/stringify would rewrite all of it.
  const next = source.replace(/"version":\s*"\d+\.\d+\.\d+"/, `"version": "${version}"`);
  if (next === source && !source.includes(`"version": "${version}"`)) {
    throw new Error(`לא נמצא שדה version בקובץ ${path}`);
  }
  writeFileSync(path, next);
  console.log(`✔ ${path}`);
};

replaceJsonVersion(join(root, 'package.json'));
replaceJsonVersion(join(root, 'apps/desktop/package.json'));
replaceJsonVersion(join(root, 'apps/desktop/src-tauri/tauri.conf.json'));

const cargoPath = join(root, 'Cargo.toml');
const cargo = readFileSync(cargoPath, 'utf-8');
const nextCargo = cargo.replace(/^version = "\d+\.\d+\.\d+"$/m, `version = "${version}"`);
if (nextCargo === cargo && !cargo.includes(`version = "${version}"`)) {
  throw new Error('לא נמצא version תחת [workspace.package] ב־Cargo.toml');
}
writeFileSync(cargoPath, nextCargo);
console.log(`✔ ${cargoPath}`);

console.log(`\nהגרסה עודכנה ל־${version}.`);
console.log('להשלמה: cargo update --workspace  (מרענן את Cargo.lock), ואז commit.');
