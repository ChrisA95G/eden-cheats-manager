import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash, createPublicKey, verify } from 'node:crypto';
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

const script = fileURLToPath(new URL('./sign-update-artifacts.mjs', import.meta.url));
const cli = fileURLToPath(new URL('../gui/node_modules/@tauri-apps/cli/tauri.js', import.meta.url));

function fixture(t, password = '') {
  const root = mkdtempSync(join(tmpdir(), 'codex-signing-test-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const key = join(root, 'test.key');
  const result = spawnSync(process.execPath, [cli, 'signer', 'generate', '--ci', '--password', password, '--write-keys', key]);
  assert.ifError(result.error);
  assert.equal(result.status, 0, 'Could not generate disposable signing key');
  const env = { ...process.env, TAURI_SIGNING_PRIVATE_KEY: readFileSync(key, 'utf8').trim(), TAURI_SIGNING_PRIVATE_KEY_PASSWORD: password };
  delete env.TAURI_SIGNING_PRIVATE_KEY_PATH;
  return { root, key, env };
}

function sign(files, env) {
  const result = spawnSync(process.execPath, [script, ...files], { env, encoding: 'utf8' });
  assert.ifError(result.error);
  return result;
}

function signatureValid(file, keyFile) {
  // Tauri wraps Minisign text in base64; its ED signatures cover a BLAKE2b digest.
  const keyText = Buffer.from(readFileSync(`${keyFile}.pub`, 'utf8').trim(), 'base64').toString();
  const rawKey = Buffer.from(keyText.split('\n')[1], 'base64').subarray(10);
  const key = createPublicKey({ format:'der', type:'spki', key:Buffer.concat([Buffer.from('302a300506032b6570032100', 'hex'), rawKey]) });
  const signatureText = Buffer.from(readFileSync(`${file}.sig`, 'utf8').trim(), 'base64').toString();
  const signature = Buffer.from(signatureText.split('\n')[1], 'base64');
  assert.equal(signature.subarray(0, 2).toString(), 'ED');
  const digest = createHash('blake2b512').update(readFileSync(file)).digest();
  return verify(null, digest, key, signature.subarray(10));
}

test('signs final bytes for all desktop formats and replaces stale signatures after modification', t => {
  const f = fixture(t);
  for (const name of ['Eden Cheats Manager.AppImage', 'EDC setup.exe', 'EDC.msi']) {
    const file = join(f.root, name);
    writeFileSync(file, 'original bundle');
    assert.equal(sign([file], f.env).status, 0);
    assert.ok(signatureValid(file, f.key));
    writeFileSync(file, 'repaired bundle');
    assert.equal(signatureValid(file, f.key), false);
    assert.equal(sign([file], f.env).status, 0);
    assert.ok(signatureValid(file, f.key));
    assert.equal(readFileSync(file, 'utf8'), 'repaired bundle');
  }
});

test('rejects missing configuration and invalid artifacts before signing anything', t => {
  const f = fixture(t);
  const valid = join(f.root, 'valid.exe');
  const empty = join(f.root, 'empty.msi');
  writeFileSync(valid, 'bundle');
  writeFileSync(empty, '');
  for (const files of [[], [valid, empty], [valid, f.root], [valid, join(f.root, '*.AppImage')]]) {
    assert.notEqual(sign(files, f.env).status, 0);
    assert.equal(existsSync(`${valid}.sig`), false);
  }
  const missing = sign([valid], { ...f.env, TAURI_SIGNING_PRIVATE_KEY: '' });
  assert.notEqual(missing.status, 0);
  assert.match(missing.stderr, /TAURI_SIGNING_PRIVATE_KEY is required/);
});

test('supports password-protected keys and removes stale signatures on signing failure without logging secrets', t => {
  const f = fixture(t, 'disposable-test-password');
  const file = join(f.root, 'EDC.exe');
  writeFileSync(file, 'bundle');
  assert.equal(sign([file], f.env).status, 0);
  assert.ok(signatureValid(file, f.key));
  for (const override of [{TAURI_SIGNING_PRIVATE_KEY:'invalid-secret-sentinel'}, {TAURI_SIGNING_PRIVATE_KEY_PASSWORD:'wrong-password-sentinel'}]) {
    writeFileSync(`${file}.sig`, 'stale signature');
    const result = sign([file], { ...f.env, ...override });
    assert.notEqual(result.status, 0);
    assert.equal(existsSync(`${file}.sig`), false);
    assert.match(result.stderr, /Could not sign/);
    for (const secret of [f.env.TAURI_SIGNING_PRIVATE_KEY, f.env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD, ...Object.values(override)]) {
      assert.ok(!`${result.stdout}${result.stderr}`.includes(secret));
    }
  }
});
