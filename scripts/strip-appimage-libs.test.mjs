import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { accessSync, constants, lstatSync, mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const script = fileURLToPath(new URL('./strip-appimage-libs.sh', import.meta.url));
const libraries = [
  'libwayland-client.so.0', 'libwayland-cursor.so.0', 'libwayland-egl.so.1',
  'libwayland-server.so.0', 'libxkbcommon.so.0', 'libxcb-randr.so.0',
  'libxcb-render.so.0', 'libxcb-shm.so.0', 'libXau.so.6', 'libXdmcp.so.6',
];

function fixture(t) {
  const root = mkdtempSync(join(tmpdir(), 'ecm-appimage-test-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const source = join(root, 'source');
  const packaged = join(root, 'packaged');
  const lib = join(source, 'usr/lib');
  mkdirSync(join(lib, 'x86_64-linux-gnu'), { recursive: true });
  writeFileSync(join(source, 'AppRun'), '#!/bin/sh\nexit 0\n', { mode: 0o755 });
  for (const name of libraries) {
    writeFileSync(join(lib, `${name}.123`), 'bundled library');
    symlinkSync(`${name}.123`, join(lib, name));
    writeFileSync(join(lib, 'x86_64-linux-gnu', name), 'multiarch copy');
  }
  symlinkSync('missing.so', join(lib, 'libwayland-client.so'));
  writeFileSync(join(lib, 'libwebkit2gtk-4.1.so.0'), 'keep WebKit');
  writeFileSync(join(source, 'cheats.db'), 'keep app resources');
  const image = join(root, 'Eden Cheats Manager.AppImage');
  const original = `#!/usr/bin/env bash
set -eu
case "$1" in
  --appimage-extract) cp -a "$SOURCE" squashfs-root ;;
  --appimage-offset) echo 16 ;;
  *) exit 2 ;;
esac
`;
  writeFileSync(image, original);
  const tool = join(root, 'appimagetool');
  writeFileSync(tool, `#!/usr/bin/env bash
set -eu
[[ "$ARCH" == x86_64 && "$1" == --appimage-extract-and-run ]]
[[ "$2" == --runtime-file && -s "$3" && "$4" == --comp && "$5" == zstd ]]
[[ "$(wc -c < "$3")" == 16 ]]
if [[ "$FAIL_PACKAGING" == 1 ]]; then exit 42; fi
rm -rf "$PACKAGED"
cp -a "$6" "$PACKAGED"
if [[ "$REINTRODUCE_LIBRARY" == 1 ]]; then
  touch "$PACKAGED/usr/lib/libwayland-client.so.0"
fi
cat > "$7" <<'IMAGE'
#!/usr/bin/env bash
set -eu
[[ "$1" == --appimage-extract ]]
cp -a "$PACKAGED" squashfs-root
IMAGE
`, { mode: 0o755 });
  const env = { ...process.env, SOURCE: source, PACKAGED: packaged, TMPDIR: root,
    FAIL_PACKAGING: '0', REINTRODUCE_LIBRARY: '0' };
  return { root, image, tool, packaged, original, env };
}

test('strips all reported libraries, symlinks and multiarch copies, preserving other content', (t) => {
  const f = fixture(t);
  // Multiple artifacts and spaces in names must survive argument handling.
  const second = join(f.root, 'Second AppImage.AppImage');
  writeFileSync(second, f.original);
  const result = spawnSync('bash', [script, f.tool, f.image, second], { env: f.env, encoding: 'utf8' });
  assert.equal(result.status, 0, result.stderr);
  for (const name of libraries) {
    for (const relative of [name, `${name}.123`, `x86_64-linux-gnu/${name}`]) {
      assert.throws(() => lstatSync(join(f.packaged, 'usr/lib', relative)), { code: 'ENOENT' });
    }
  }
  assert.throws(() => lstatSync(join(f.packaged, 'usr/lib/libwayland-client.so')), { code: 'ENOENT' });
  assert.equal(readFileSync(join(f.packaged, 'usr/lib/libwebkit2gtk-4.1.so.0'), 'utf8'), 'keep WebKit');
  assert.equal(readFileSync(join(f.packaged, 'cheats.db'), 'utf8'), 'keep app resources');
  for (const image of [f.image, second]) {
    accessSync(image, constants.X_OK);
    assert.notEqual(readFileSync(image, 'utf8'), f.original);
  }
});

for (const failure of ['FAIL_PACKAGING', 'REINTRODUCE_LIBRARY']) {
  test(`${failure}: fails without replacing the original artifact`, (t) => {
    const f = fixture(t);
    const result = spawnSync('bash', [script, f.tool, f.image], {
      env: { ...f.env, [failure]: '1' }, encoding: 'utf8',
    });
    assert.notEqual(result.status, 0);
    assert.equal(readFileSync(f.image, 'utf8'), f.original);
  });
}

test('fails on missing artifacts (including an unmatched shell glob)', (t) => {
  const f = fixture(t);
  const result = spawnSync('bash', [script, f.tool, join(f.root, '*.AppImage')], {
    env: f.env, encoding: 'utf8',
  });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /AppImage not found/);
});
