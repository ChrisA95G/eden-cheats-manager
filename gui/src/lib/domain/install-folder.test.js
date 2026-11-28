import assert from 'node:assert/strict';
import test from 'node:test';
import { installFolderChanged, readSettledInstallFolder } from './install-folder.js';

/** @returns {import('../api/types.js').EdenLoadAccessStatus} */
function folder(destinationId = 'folder-a') {
  return {
    destinationId, folderLabel: 'Downloads/load', installMode: 'custom', selectionPending: false,
    selected: true, validLocation: true, readPermission: true, writePermission: true,
    readable: true, writable: true, ready: true, message: 'Ready',
  };
}

test('two same-named directories and a mode change both invalidate the workspace', () => {
  const before = folder();
  assert.equal(installFolderChanged(before, folder('folder-b')), true);
  assert.equal(installFolderChanged(before, { ...before, installMode: 'direct' }), true);
  assert.equal(installFolderChanged(before, { ...before, folderLabel: 'Renamed' }), false);
});

test('only the settled destination is returned when the new folder appears during saving', async () => {
  const before = folder();
  const after = folder('folder-b');
  const responses = [{ ...after, selectionPending: true }, after];
  const status = await readSettledInstallFolder(async () => responses.shift() ?? assert.fail('unexpected poll'), () => true);
  assert.deepEqual(status, after);
  assert.equal(installFolderChanged(before, after), true);
});

test('a cancelled pick or launch failure can finish without a focus event', async () => {
  const before = folder();
  for (const selectionError of ['', 'Android could not open the folder picker.']) {
    const next = { ...before, selectionError };
    const status = await readSettledInstallFolder(async () => next, () => true);
    assert.deepEqual(status, next);
    assert.equal(installFolderChanged(before, next), false);
  }
});

test('discard a status response that arrives after its caller has gone away', async () => {
  let active = true;
  const status = await readSettledInstallFolder(async () => {
    active = false;
    return folder();
  }, () => active);
  assert.equal(status, null);
});
