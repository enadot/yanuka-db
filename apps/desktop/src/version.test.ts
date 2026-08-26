import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

/**
 * The product version is declared in four files. The installer takes its name
 * from tauri.conf.json while the settings screen shows package.json — if they
 * drift, a "0.2.0" installer reports a different number on screen and the
 * offline-update story ("which build is installed?") falls apart.
 *
 * scripts/set-version.mjs updates all four in one move; this test makes any
 * hand edit that forgets one of them fail the pipeline.
 */

const read = (relative: string) =>
  readFileSync(fileURLToPath(new URL(relative, import.meta.url)), 'utf-8');

describe('version declarations', () => {
  it('agree across package.json, tauri.conf.json and Cargo.toml', () => {
    const desktop = (JSON.parse(read('../package.json')) as { version: string }).version;
    const root = (JSON.parse(read('../../../package.json')) as { version: string }).version;
    const tauri = (JSON.parse(read('../src-tauri/tauri.conf.json')) as { version: string })
      .version;
    const cargo = /^version = "(\d+\.\d+\.\d+)"$/m.exec(read('../../../Cargo.toml'))?.[1];

    expect(desktop).toMatch(/^\d+\.\d+\.\d+$/);
    expect(root).toBe(desktop);
    expect(tauri).toBe(desktop);
    expect(cargo).toBe(desktop);
  });

  it('is the version the UI shows', () => {
    const desktop = (JSON.parse(read('../package.json')) as { version: string }).version;
    expect(__APP_VERSION__).toBe(desktop);
  });
});
