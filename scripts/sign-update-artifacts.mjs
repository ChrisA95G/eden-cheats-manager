import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { rmSync, statSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const cli = fileURLToPath(new URL('../gui/node_modules/@tauri-apps/cli/tauri.js', import.meta.url));
const files = process.argv.slice(2).map(file => resolve(file));
assert.ok(files.length, 'Expected at least one update artifact');
assert.ok(process.env.TAURI_SIGNING_PRIVATE_KEY?.trim(), 'TAURI_SIGNING_PRIVATE_KEY is required');

// Validate all paths before signing, including unmatched shell globs.
for (const file of files) {
  const stat = statSync(file);
  assert.ok(stat.isFile() && stat.size > 0, `Missing or empty update artifact: ${file}`);
}

for (const file of files) {
  const signature = `${file}.sig`;
  rmSync(signature, { force: true });
  const result = spawnSync(process.execPath, [cli, 'signer', 'sign', file], {
    env: { ...process.env, TAURI_SIGNING_PRIVATE_KEY_PASSWORD: process.env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD ?? '' },
    encoding: 'utf8',
    timeout: 60_000,
  });
  // Do not forward signer output: errors may contain key or password arguments.
  if (result.error || result.status !== 0) {
    rmSync(signature, { force: true });
    throw new Error(`Could not sign ${file}. Check the update signing key and password.`);
  }
  assert.ok(statSync(signature).size > 0, `Missing update signature: ${signature}`);
  console.log(`Signed ${file}`);
}
