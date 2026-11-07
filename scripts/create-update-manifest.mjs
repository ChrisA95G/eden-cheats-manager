import assert from 'node:assert/strict';
import { readFileSync, statSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const [directory, releaseFile] = process.argv.slice(2);
assert.ok(directory && releaseFile, 'Expected artifact directory and release metadata file');
const { version } = JSON.parse(readFileSync(new URL('../gui/src-tauri/tauri.conf.json', import.meta.url), 'utf8'));
const release = JSON.parse(readFileSync(releaseFile, 'utf8'));
const repository = process.env.GH_REPO;
assert.match(repository ?? '', /^[A-Za-z0-9][A-Za-z0-9_.-]*\/[A-Za-z0-9][A-Za-z0-9_.-]*$/, 'GH_REPO must be owner/repository');
assert.match(version, /^\d+\.\d+\.\d+(?:-rc\.[1-9]\d*)?$/);
assert.equal(release.tagName, `v${version}`, 'Release tag must match the application version');
assert.equal(release.isDraft, true, 'Update metadata must be generated for a draft release');
assert.ok(release.body == null || typeof release.body === 'string', 'Release notes must be text');

function requireFile(name) {
  const path = join(directory, name);
  const stat = statSync(path);
  assert.ok(stat.isFile() && stat.size > 0, `Missing or empty release file: ${name}`);
  return path;
}

// The release must include Android too, even though Tauri's updater handles desktop only.
requireFile('eden-cheats-manager.apk');
const platforms = {};
for (const [target, suffix] of [
  ['windows-x86_64-nsis', 'x64-setup.exe'],
  ['windows-x86_64-msi', 'x64_en-US.msi'],
  ['linux-x86_64-appimage', 'amd64.AppImage'],
]) {
  const name = `Eden.Cheats.Manager_${version}_${suffix}`;
  requireFile(name);
  const signature = readFileSync(requireFile(`${name}.sig`), 'utf8').trim();
  assert.ok(signature, `Empty update signature: ${name}.sig`);
  platforms[target] = {
    url: `https://github.com/${repository}/releases/download/${release.tagName}/${name}`,
    signature,
  };
}

writeFileSync(join(directory, 'latest.json'), `${JSON.stringify({ version, notes: release.body ?? '', platforms }, null, 2)}\n`);
console.log(`Created update metadata for ${release.tagName}`);
