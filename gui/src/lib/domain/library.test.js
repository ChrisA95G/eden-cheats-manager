import test from 'node:test';
import assert from 'node:assert/strict';

import {
  candidateKey,
  cheatLibraryGroups,
  createLibraryState,
  fallbackCandidate,
  gameCheatTarget,
  libraryCandidatesForGame,
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
  assert.equal(gameCheatTarget({...group, baseGame:{...base, installed:false}}), update);
});

/** @typedef {import('../api/types.js').EdenPackageCorrelationEntry} EdenPackageCorrelationEntry */
/** @typedef {import('../api/types.js').GameVersionGroup} GameVersionGroup */
/** @typedef {import('../api/types.js').GameVersionPackage} GameVersionPackage */
/** @typedef {import('../api/types.js').ManagedPackageLibrary} ManagedPackageLibrary */

/** @param {string} relativePath @param {string} [buildId] @param {string} [baseTitleId] @returns {GameVersionPackage} */
function packageRecord(relativePath, buildId = 'BUILD', baseTitleId = '0100000000000000') {
  return {
    contentKind: 'application',
    titleId: '0100000000000800',
    baseTitleId,
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

test('game candidates include package-only games and deduplicate Eden correlations by exact identity', () => {
  const sameNameCandidate = packageRecord('wrong/game.nsp', 'WRONG', '0100AAAA00000000');
  const exactCandidates = [
    packageRecord('first/game.nsp', 'SAME', '0100BBBB00000000'),
    packageRecord('second/game.nsp', 'SAME', '0100BBBB00000000'),
  ];
  const packageOnly = packageRecord('package-only/game.nsp', 'PACKAGE', '0100BBBB00000000');
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
        observedTitleId: '0100BBBB00000000',
        resolvedBaseTitleId: null,
        packageCandidates: exactCandidates,
      },
    ],
    [{ baseTitleId: '0100BBBB00000000', versions: [packageOnly] }],
  );

  assert.deepEqual(
    libraryCandidatesForGame(library, ' 0100bbbb00000000 ').map((item) => item.package.relativePath),
    ['first/game.nsp', 'second/game.nsp', 'package-only/game.nsp'],
  );
  assert.deepEqual(libraryCandidatesForGame(library, '0100CCCC00000000'), []);
  assert.equal(libraryCandidatesForGame(readyLibrary([], [{baseTitleId:packageOnly.baseTitleId, versions:[packageOnly]}]), packageOnly.baseTitleId).length, 1);
  assert.deepEqual(libraryCandidatesForGame({state:'notConfigured', message:''}, packageOnly.baseTitleId), []);
  const dlc = {...packageOnly, contentKind:'add_on_content'};
  assert.deepEqual(libraryCandidatesForGame(readyLibrary([], [{baseTitleId:dlc.baseTitleId, versions:[dlc]}]), dlc.baseTitleId), []);
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

test('library reducer never falls back to Eden-only or stale games after setup or scan errors', () => {
  const freshGames = [{ baseTitleId: 'FRESH' }];
  const initial = createLibraryState();
  assert.equal(initial.packageLibrary, null);

  let state = reduceLibraryState(initial, {
    type: 'refreshSucceeded',
    snapshot: {
      games: /** @type {any} */ (freshGames),
      packageLibrary: readyLibrary([]),
    },
  });
  assert.equal(state.games, freshGames);
  assert.equal(state.packageLibrary?.state, 'ready');

  for (const packageLibrary of [
    {state: /** @type {const} */ ('error'), message:'Keys could not be read'},
    {state: /** @type {const} */ ('notConfigured'), message:'Choose a package folder'},
  ]) {
    const unavailable = reduceLibraryState(state, {type:'refreshSucceeded', snapshot:{games:state.games, packageLibrary}});
    assert.deepEqual(unavailable.games, []);
    assert.equal(unavailable.packageLibrary, packageLibrary);
  }

  const failed = reduceLibraryState(state, {
    type: 'refreshFailed',
    error: 'Eden scan failed',
  });
  assert.deepEqual(failed.games, []);
  assert.equal(failed.packageLibrary, null);
  assert.equal(failed.refreshError, 'Eden scan failed');
});
