// Fetch the semantic-search embedding model into the desktop's resources.
//
// The model (ADR-036) is 118MB and therefore not committed; it is downloaded
// from Hugging Face at a pinned revision and verified against pinned SHA-256
// digests, so every build embeds byte-identical files. Run before
// `tauri build`, and before `cargo test --features semantic`:
//
//   node scripts/fetch-semantic-model.mjs
//
// Idempotent: files that already exist with the right digest are kept.

import { createHash } from 'node:crypto';
import { mkdirSync, readFileSync, writeFileSync, existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const target = join(root, 'apps', 'desktop', 'src-tauri', 'resources', 'semantic');

const REVISION = '614241f622f53c4eeff9890bdc4f31cfecc418b3';
const BASE = `https://huggingface.co/intfloat/multilingual-e5-small/resolve/${REVISION}`;

const FILES = [
  {
    name: 'model.onnx',
    url: `${BASE}/onnx/model_qint8_avx512_vnni.onnx`,
    sha256: 'dd476dd0c2514e9b9be83aeb3853fac0763e0bdf4a71645407587d77c48a2d88',
  },
  {
    name: 'tokenizer.json',
    url: `${BASE}/tokenizer.json`,
    sha256: '0b44a9d7b51c3c62626640cda0e2c2f70fdacdc25bbbd68038369d14ebdf4c39',
  },
];

const digest = (buffer) => createHash('sha256').update(buffer).digest('hex');

mkdirSync(target, { recursive: true });

for (const file of FILES) {
  const path = join(target, file.name);
  if (existsSync(path) && digest(readFileSync(path)) === file.sha256) {
    console.log(`✔ ${file.name} (cached)`);
    continue;
  }
  console.log(`⇣ ${file.name} ...`);
  const response = await fetch(file.url);
  if (!response.ok) {
    console.error(`download failed: ${file.url} → ${response.status}`);
    process.exit(1);
  }
  const buffer = Buffer.from(await response.arrayBuffer());
  const actual = digest(buffer);
  if (actual !== file.sha256) {
    console.error(`checksum mismatch for ${file.name}: ${actual}`);
    process.exit(1);
  }
  writeFileSync(path, buffer);
  console.log(`✔ ${file.name} (${(buffer.length / 1024 / 1024).toFixed(1)} MB)`);
}
