import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const read = path => readFileSync(new URL(`../${path}`, import.meta.url), 'utf8').replace(/\r\n/g, '\n');
const config = JSON.parse(read('gui/src-tauri/tauri.conf.json'));
const version = config.version;
const match = /^(\d+)\.(\d+)\.(\d+)(?:-rc\.([1-9]\d*))?$/.exec(version);
assert.ok(match, 'Expected x.y.z or x.y.z-rc.N');
assert.equal(JSON.parse(read('gui/package.json')).version, version);
const lock = JSON.parse(read('gui/package-lock.json'));
assert.equal(lock.version, version);
assert.equal(lock.packages[''].version, version);
assert.equal(/^version = "([^"]+)"/m.exec(read('gui/src-tauri/Cargo.toml'))?.[1], version);
assert.equal(/name = "eden-cheats-manager-gui"\nversion = "([^"]+)"/.exec(read('gui/src-tauri/Cargo.lock'))?.[1], version);

// Native build numbers are maintained explicitly for upgrade continuity.
const [, major, minor, patch] = match;
const code = config.bundle.android.versionCode;
assert.ok(Number.isInteger(code) && code > 1000 && code <= 2_100_000_000);
assert.match(config.bundle.windows.wix.version, new RegExp(`^${major}\\.${minor}\\.${patch}\\.\\d+$`));
assert.match(config.bundle.macOS.bundleVersion, /^\d+\.\d+\.\d+$/);
assert.match(read('gui/src-tauri/Info.plist'), new RegExp(`<string>${major}\\.${minor}\\.${patch}</string>`));
if (process.argv[2]) assert.equal(process.argv[2], `v${version}`, 'Tag must match application version');
console.log(`Release metadata consistent: ${version} (Android ${code})`);
