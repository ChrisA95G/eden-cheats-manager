/** @typedef {import('../api/types.js').GameGroup} GameGroup */
/** @typedef {import('../api/types.js').GameVersionPackage} GameVersionPackage */
/** @typedef {import('../api/types.js').ManagedLibrarySnapshot} ManagedLibrarySnapshot */
/** @typedef {import('../api/types.js').ManagedPackageLibrary} ManagedPackageLibrary */
/** @typedef {import('../api/types.js').PackageMetadata} PackageMetadata */

/** @param {string} value */
export function normalizeTitleId(value) {
  return value.trim().toUpperCase();
}

/** Keep only cheat-relevant titles in the frontend, without mutating scan/cache data.
 * @param {GameGroup[]} games
 * @returns {GameGroup[]}
 */
export function cheatLibraryGroups(games) {
  return games.filter(group => group.baseGame || group.updates.length > 0)
    .map(group => ({ ...group, dlcs: [] }));
}

/** Prefer an existing Eden target, otherwise browse the scanned game's base ID.
 * @param {GameGroup | null} game
 */
export function gameCheatTarget(game) {
  return (game?.baseGame?.installed ? game.baseGame : game?.updates.find(entry => entry.installed))
    ?? game?.baseGame ?? game?.updates[0] ?? null;
}

/**
 * @typedef {{ source: 'library', package: GameVersionPackage }} LibraryCandidate
 * @typedef {{ source: 'fallback', metadata: PackageMetadata, label: string }} FallbackCandidate
 * @typedef {LibraryCandidate | FallbackCandidate} PackageCandidate
 */

/**
 * @param {ManagedPackageLibrary} packageLibrary
 * @param {string} baseTitleId
 * @returns {LibraryCandidate[]}
 */
export function libraryCandidatesForGame(packageLibrary, baseTitleId) {
  if (packageLibrary.state !== 'ready') return [];
  const correlation = packageLibrary.correlation;
  const packages = [
    ...correlation.edenEntries.flatMap(entry => entry.packageCandidates),
    ...correlation.unmatchedPackageGroups.flatMap(group => group.versions),
  ];
  const candidates = new Map();
  for (const value of packages) {
    if (normalizeTitleId(value.baseTitleId) !== normalizeTitleId(baseTitleId)
      || !['application', 'patch'].includes(value.contentKind)) continue;
    const candidate = { source: /** @type {const} */ ('library'), package: value };
    candidates.set(candidateKey(candidate), candidate);
  }
  return [...candidates.values()];
}

/** @param {PackageMetadata} metadata @param {string} [label] @returns {FallbackCandidate} */
export function fallbackCandidate(metadata, label = 'Selected package') {
  return { source: 'fallback', metadata, label };
}

/** @param {PackageCandidate} candidate */
export function candidateKey(candidate) {
  if (candidate.source === 'library') {
    const value = candidate.package;
    return JSON.stringify([
      'library',
      value.relativePath,
      value.packageFormat,
      value.contentKind,
      value.titleId,
      value.baseTitleId,
      value.version,
      value.buildId,
      value.moduleId,
    ]);
  }
  const value = candidate.metadata;
  return JSON.stringify([
    'fallback',
    value.packageFormat,
    value.titleId,
    value.version,
    value.buildId,
    value.moduleId,
  ]);
}

/**
 * Retain a previously explicit choice only while that same candidate exists.
 * A newly available candidate is never selected implicitly.
 *
 * @param {PackageCandidate | null} selected
 * @param {PackageCandidate[]} candidates
 */
export function reconcileCandidate(selected, candidates) {
  if (!selected) return null;
  const selectedKey = candidateKey(selected);
  return candidates.find((candidate) => candidateKey(candidate) === selectedKey) ?? null;
}

/**
 * @typedef {Object} LibraryState
 * @property {GameGroup[]} games
 * @property {ManagedPackageLibrary | null} packageLibrary
 * @property {string} refreshError
 */

/** @returns {LibraryState} */
export function createLibraryState() {
  return {
    games: [],
    packageLibrary: null,
    refreshError: '',
  };
}

/**
 * @param {LibraryState} state
 * @param {{ type: 'refreshSucceeded', snapshot: ManagedLibrarySnapshot }
 *   | { type: 'refreshFailed', error: string }} action
 * @returns {LibraryState}
 */
export function reduceLibraryState(state, action) {
  switch (action.type) {
    case 'refreshSucceeded':
      return {
        games: action.snapshot.packageLibrary.state === 'ready' ? action.snapshot.games : [],
        packageLibrary: action.snapshot.packageLibrary,
        refreshError: '',
      };
    case 'refreshFailed':
      return { games: [], packageLibrary: null, refreshError: action.error };
    default:
      return state;
  }
}
