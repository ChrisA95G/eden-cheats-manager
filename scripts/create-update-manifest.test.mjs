import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

function fixture(t, version = '0.3.0') {
  const root = mkdtempSync(join(tmpdir(), 'codex-manifest-test-'));
  t.after(() => rmSync(root, { recursive:true, force:true }));
  for (const dir of ['scripts', 'gui/src-tauri', 'artifacts']) mkdirSync(join(root, dir), { recursive:true });
  const script = join(root, 'scripts/create-update-manifest.mjs');
  copyFileSync(new URL('./create-update-manifest.mjs', import.meta.url), script);
  writeFileSync(join(root, 'gui/src-tauri/tauri.conf.json'), JSON.stringify({version}));
  const directory = join(root, 'artifacts');
  const releaseFile = join(root, 'release.json');
  const release = {tagName:`v${version}`, body:'Notes with "quotes"\nSecond line.', isDraft:true};
  const assets = {
    'windows-x86_64-nsis': `Eden.Cheats.Manager_${version}_x64-setup.exe`,
    'windows-x86_64-msi': `Eden.Cheats.Manager_${version}_x64_en-US.msi`,
    'linux-x86_64-appimage': `Eden.Cheats.Manager_${version}_amd64.AppImage`,
  };
  const files = {'eden-cheats-manager.apk':'APK'};
  for (const name of Object.values(assets)) {
    files[name] = `bundle: ${name}`;
    files[`${name}.sig`] = Buffer.from(`signature: ${name}`).toString('base64');
  }
  for (const [name, content] of Object.entries(files)) writeFileSync(join(directory, name), content);
  function run(metadata = release, repository = 'ChrisA95G/eden-cheats-manager') {
    writeFileSync(releaseFile, JSON.stringify(metadata));
    const result = spawnSync(process.execPath, [script, directory, releaseFile], {
      encoding:'utf8', env:{...process.env, GH_REPO:repository},
    });
    assert.ifError(result.error);
    return result;
  }
  const output = join(directory, 'latest.json');
  return {directory, release, assets, files, output, run};
}

for (const version of ['0.3.0', '0.3.0-rc.1']) {
  test(`maps each installer to its own signature and fixed release URL (${version})`, t => {
    const f = fixture(t, version);
    const result = f.run();
    assert.equal(result.status, 0, result.stderr);
    const manifest = JSON.parse(readFileSync(f.output, 'utf8'));
    assert.equal(manifest.version, version);
    assert.equal(manifest.notes, f.release.body);
    assert.deepEqual(Object.keys(manifest.platforms).sort(), Object.keys(f.assets).sort());
    for (const [target, name] of Object.entries(f.assets)) {
      assert.deepEqual(manifest.platforms[target], {
        url:`https://github.com/ChrisA95G/eden-cheats-manager/releases/download/v${version}/${name}`,
        signature:f.files[`${name}.sig`],
      });
    }
  });
}

test('requires all platform binaries and signatures before creating a manifest', t => {
  const f = fixture(t);
  for (const [name, content] of Object.entries(f.files)) {
    const path = join(f.directory, name);
    rmSync(path);
    assert.notEqual(f.run().status, 0, `Missing ${name}`);
    assert.equal(existsSync(f.output), false);
    writeFileSync(path, '');
    assert.notEqual(f.run().status, 0, `Empty ${name}`);
    assert.equal(existsSync(f.output), false);
    writeFileSync(path, content);
  }
  const signature = join(f.directory, `${Object.values(f.assets)[0]}.sig`);
  writeFileSync(signature, ' \r\n ');
  assert.notEqual(f.run().status, 0);
  assert.equal(existsSync(f.output), false);
});

test('rejects a different tag, published release, invalid notes, and invalid repository', t => {
  const f = fixture(t);
  for (const change of [{tagName:'v0.2.9'}, {isDraft:false}, {body:42}]) {
    assert.notEqual(f.run({...f.release, ...change}).status, 0);
    assert.equal(existsSync(f.output), false);
  }
  for (const repository of ['', '../repo', 'owner/repo?query', 'https://github.com/owner/repo']) {
    assert.notEqual(f.run(f.release, repository).status, 0);
    assert.equal(existsSync(f.output), false);
  }
});

test('replaces stale metadata on rerun and leaves it untouched if validation fails', t => {
  const f = fixture(t);
  writeFileSync(f.output, 'stale manifest');
  assert.equal(f.run({...f.release, body:null}).status, 0);
  const output = readFileSync(f.output, 'utf8');
  assert.equal(JSON.parse(output).notes, '');
  rmSync(join(f.directory, 'eden-cheats-manager.apk'));
  assert.notEqual(f.run().status, 0);
  assert.equal(readFileSync(f.output, 'utf8'), output);
});
