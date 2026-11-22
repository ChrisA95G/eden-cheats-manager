import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';

const read = path => readFileSync(new URL(`../${path}`, import.meta.url), 'utf8').replace(/\r\n/g, '\n');
const config = JSON.parse(read('gui/src-tauri/tauri.conf.json'));
const version = config.version;
const releaseVersionPattern = /^(\d+)\.(\d+)\.(\d+)(?:-(?:beta|rc)\.[1-9]\d*)?$/;
const match = releaseVersionPattern.exec(version);
assert.ok(match, 'Expected x.y.z, x.y.z-beta.N, or x.y.z-rc.N with a positive prerelease number');
assert.equal(JSON.parse(read('gui/package.json')).version, version);
const lock = JSON.parse(read('gui/package-lock.json'));
assert.equal(lock.version, version);
assert.equal(lock.packages[''].version, version);
assert.equal(/^version = "([^"]+)"/m.exec(read('gui/src-tauri/Cargo.toml'))?.[1], version);
assert.equal(/name = "eden-cheats-manager-gui"\nversion = "([^"]+)"/.exec(read('gui/src-tauri/Cargo.lock'))?.[1], version);

// Native build numbers are maintained explicitly for upgrade continuity.
const [, major, minor, patch] = match;
const code = config.bundle.android.versionCode;
const lastAndroidCode = (version, code) => version.includes('-beta.') ? code + 1 : code;
assert.ok(Number.isInteger(code) && code > 1000 && lastAndroidCode(version, code) <= 2_100_000_000);
assert.match(config.bundle.windows.wix.version, new RegExp(`^${major}\\.${minor}\\.${patch}\\.\\d+$`));
assert.match(config.bundle.macOS.bundleVersion, /^\d+\.\d+\.\d+$/);
assert.match(read('gui/src-tauri/Info.plist'), new RegExp(`<string>${major}\\.${minor}\\.${patch}</string>`));
if (process.argv[2]) assert.equal(process.argv[2], `v${version}`, 'Tag must match application version');
if (process.argv.includes('--check-android-history')) {
  const git = (...args) => execFileSync('git', args, { encoding: 'utf8' });
  for (const tag of git('tag', '--list', 'v*').trim().split('\n')) {
    const previous = tag.slice(1);
    if (previous === version || !releaseVersionPattern.test(previous)) continue;
    const prior = JSON.parse(git('show', `refs/tags/${tag}:gui/src-tauri/tauri.conf.json`)).bundle?.android?.versionCode;
    if (prior != null) assert.ok(code > lastAndroidCode(previous, prior), `Android versionCode overlaps ${tag}`);
  }
}
console.log(`Release metadata consistent: ${version} (Android ${code})`);
