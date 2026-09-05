import test from 'node:test';
import assert from 'node:assert/strict';

import {
  candidateKey,
  cheatLibraryGroups,
  createLibraryState,
  fallbackCandidate,
  gameCheatTarget,
  findPresenceByObservedId,
  libraryCandidatesForTitle,
  reconcileCandidate,
  reduceLibraryState,
} from './library.js';

test('cheat library drops DLC-only groups and preserves base/update identities', () => {
  /** @param {string} titleId @param {import('../api/types.js').TitleEntry['category']} category */
  const entry = (titleId, category) => ({titleId, category, baseTitleId:'0100AAAA00000000', name:category, image:'', installed:true});
  const base = entry('0100AAAA00000000', 'base');
  const update = entry('0100AAAA00000800', 'update');
  const dlc = entry('0100AAAA00000001', 'dlc');
  const group = {baseTitleId:base.titleId, baseName:'Game', baseImage:'', baseInstalled:true, baseGame:base, updates:[update], dlcs:[dlc]};
  const updateOnly = {...group, baseGame:null, baseInstalled:false};
  const dlcOnly = {...updateOnly, updates:[]};
  const source = [group, updateOnly, dlcOnly, {...dlcOnly, dlcs:[]}];
  const before = structuredClone(source);
  const visible = cheatLibraryGroups(source);

  assert.equal(visible.length, 2);
  assert.equal(visible[0].baseGame, base);
  assert.equal(visible[0].updates[0], update);
  assert.equal(visible[1].updates[0], update);
  assert.ok(visible.every(item => item.dlcs.length === 0));
  assert.deepEqual(source, before, 'raw backend/cache data must remain intact');
  assert.deepEqual(cheatLibraryGroups([]), []);
  assert.equal(gameCheatTarget(group), base, 'updates must not redirect base-game cheat writes');
  assert.equal(gameCheatTarget(updateOnly), update, 'update-only groups retain the actual observed Title ID');
  assert.equal(gameCheatTarget(dlcOnly), null);
  assert.equal(gameCheatTarget(null), null);
});

/** @typedef {import('../api/types.js').EdenPackageCorrelationEntry} EdenPackageCorrelationEntry */
/** @typedef {import('../api/types.js').GameVersionGroup} GameVersionGroup */
/** @typedef {import('../api/types.js').GameVersionPackage} GameVersionPackage */
/** @typedef {import('../api/types.js').ManagedPackageLibrary} ManagedPackageLibrary */

/** @param {string} relativePath @param {string} [buildId] @returns {GameVersionPackage} */
function packageRecord(relativePath, buildId = 'BUILD') {
  return {
    contentKind: 'application',
    titleId: '0100000000000800',
    baseTitleId: '0100000000000000',
    version: 1,
    buildId,
    moduleId: buildId,
    packageFormat: 'NSP',
    filename: relativePath.split('/').at(-1) ?? relativePath,
    relativePath,
    size: 42,
  };
}

/**
 * @param {EdenPackageCorrelationEntry[]} entries
 * @param {GameVersionGroup[]} [unmatchedPackageGroups]
 * @returns {ManagedPackageLibrary}
 */
function readyLibrary(entries, unmatchedPackageGroups = []) {
  return {
    state: 'ready',
    correlation: {
      scannedPackages: 0,
      matchedPackages: 0,
      skippedPackages: 0,
      edenEntries: entries,
      unmatchedPackageGroups,
      packageScanErrors: [],
      correlationIssues: [],
    },
  };
}

test('candidate lookup uses only exact normalized observed Title ID', () => {
  const sameNameCandidate = packageRecord('wrong/game.nsp', 'WRONG');
  const exactCandidates = [
    packageRecord('first/game.nsp', 'SAME'),
    packageRecord('second/game.nsp', 'SAME'),
  ];
  const packageOnly = packageRecord('package-only/game.nsp', 'PACKAGE');
  const library = readyLibrary(
    [
      {
        observedTitleId: '0100AAAA00000800',
        resolvedBaseTitleId: '0100AAAA00000000',
        packageCandidates: [sameNameCandidate],
      },
      {
        observedTitleId: '0100BBBB00000800',
        resolvedBaseTitleId: '0100BBBB00000000',
        packageCandidates: exactCandidates,
      },
      {
        observedTitleId: '0100BBBB00000001',
        resolvedBaseTitleId: null,
        packageCandidates: [],
      },
    ],
    [{ baseTitleId: '0100BBBB00000000', versions: [packageOnly] }],
  );

  assert.equal(findPresenceByObservedId(library, ' 0100bbbb00000800 ')?.observedTitleId, '0100BBBB00000800');
  assert.deepEqual(
    libraryCandidatesForTitle(library, '0100BBBB00000800').map((item) => item.package.relativePath),
    ['first/game.nsp', 'second/game.nsp'],
  );
  assert.deepEqual(libraryCandidatesForTitle(library, '0100BBBB00000000'), []);
});

test('candidate reconciliation preserves duplicates and never auto-selects', () => {
  /** @type {{ source: 'library', package: GameVersionPackage }[]} */
  const candidates = [
    { source: 'library', package: packageRecord('a/game.nsp', 'SAME') },
    { source: 'library', package: packageRecord('b/game.nsp', 'SAME') },
  ];

  assert.equal(reconcileCandidate(null, candidates), null);
  assert.equal(reconcileCandidate(candidates[1], candidates), candidates[1]);
  assert.equal(reconcileCandidate(candidates[1], [candidates[0]]), null);
  assert.notEqual(candidateKey(candidates[0]), candidateKey(candidates[1]));

  const replacedInPlace = {
    source: /** @type {const} */ ('library'),
    package: packageRecord('b/game.nsp', 'REPLACED'),
  };
  assert.equal(reconcileCandidate(candidates[1], [replacedInPlace]), null);
});

test('fallback candidates remain tagged separately from library records', () => {
  const metadata = {
    packageFormat: 'XCI',
    contentKind: 'application',
    titleId: '0100AAAA00000000',
    baseTitleId: '0100AAAA00000000',
    programTitleId: '0100AAAA00000000',
    version: 7,
    buildId: 'ABCDEF',
    moduleId: 'ABCDEF',
    hasBktr: false,
    matchedProgramContentId: true,
  };
  const candidate = fallbackCandidate(metadata, 'owned-game.xci');

  assert.equal(candidate.source, 'fallback');
  assert.equal(candidate.label, 'owned-game.xci');
  assert.match(candidateKey(candidate), /^\["fallback"/);
});

test('library reducer paints cache, accepts nested setup errors, and keeps games on rejection', () => {
  const cachedGames = [{ baseTitleId: 'CACHE' }];
  const freshGames = [{ baseTitleId: 'FRESH' }];
  const initial = createLibraryState();
  assert.equal(initial.packageLibrary, null);

  let state = reduceLibraryState(initial, {
    type: 'cacheLoaded',
    games: /** @type {any} */ (cachedGames),
  });
  assert.equal(state.games, cachedGames);

  state = reduceLibraryState(state, {
    type: 'refreshSucceeded',
    snapshot: {
      games: /** @type {any} */ (freshGames),
      packageLibrary: { state: 'error', message: 'Keys could not be read' },
    },
  });
  assert.equal(state.games, freshGames);
  assert.equal(state.packageLibrary?.state, 'error');

  const failed = reduceLibraryState(state, {
    type: 'refreshFailed',
    error: 'Eden scan failed',
  });
  assert.equal(failed.games, freshGames);
  assert.equal(failed.refreshError, 'Eden scan failed');
});
