import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import test from 'node:test';

const files = [
  'scripts/check-release-version.mjs',
  'gui/package.json',
  'gui/package-lock.json',
  'gui/src-tauri/Cargo.toml',
  'gui/src-tauri/Cargo.lock',
  'gui/src-tauri/tauri.conf.json',
  'gui/src-tauri/Info.plist',
];
const version = JSON.parse(readFileSync(new URL('../gui/package.json', import.meta.url), 'utf8')).version;

for (const [label, newline] of [['LF', '\n'], ['CRLF', '\r\n']]) {
  test(`release metadata accepts ${label} and rejects mismatched tags`, () => {
    const fixture = mkdtempSync(join(tmpdir(), 'ecm-version-test-'));
    try {
      for (const file of files) {
        const path = join(fixture, file);
        mkdirSync(dirname(path), { recursive: true });
        const text = readFileSync(new URL(`../${file}`, import.meta.url), 'utf8');
        writeFileSync(path, text.replace(/\r?\n/g, newline));
      }
      const script = join(fixture, files[0]);
      const valid = spawnSync(process.execPath, [script, `v${version}`], { encoding: 'utf8' });
      assert.ifError(valid.error);
      assert.equal(valid.status, 0, valid.stderr);
      const invalid = spawnSync(process.execPath, [script, 'v-not-the-app-version'], { encoding: 'utf8' });
      assert.ifError(invalid.error);
      assert.notEqual(invalid.status, 0);
      assert.match(invalid.stderr, /Tag must match application version/);
    } finally {
      rmSync(fixture, { recursive: true, force: true });
    }
  });
}
