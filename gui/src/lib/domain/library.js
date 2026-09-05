/** @typedef {import('../api/types.js').GameGroup} GameGroup */
/** @typedef {import('../api/types.js').GameVersionPackage} GameVersionPackage */
/** @typedef {import('../api/types.js').ManagedLibrarySnapshot} ManagedLibrarySnapshot */
/** @typedef {import('../api/types.js').ManagedPackageLibrary} ManagedPackageLibrary */
/** @typedef {import('../api/types.js').PackageMetadata} PackageMetadata */

/** @param {string} value */
export function normalizeTitleId(value) {
  return value.trim().toUpperCase();
}

/**
 * @param {ManagedPackageLibrary} packageLibrary
 * @param {string} observedTitleId
 */
export function findPresenceByObservedId(packageLibrary, observedTitleId) {
  if (packageLibrary.state !== 'ready') return null;
  const expected = normalizeTitleId(observedTitleId);
  return packageLibrary.correlation.edenEntries.find(
    (entry) => normalizeTitleId(entry.observedTitleId) === expected,
  ) ?? null;
}

/**
 * @typedef {{ source: 'library', package: GameVersionPackage }} LibraryCandidate
 * @typedef {{ source: 'fallback', metadata: PackageMetadata, label: string }} FallbackCandidate
 * @typedef {LibraryCandidate | FallbackCandidate} PackageCandidate
 */

/**
 * @param {ManagedPackageLibrary} packageLibrary
 * @param {string} observedTitleId
 * @returns {LibraryCandidate[]}
 */
export function libraryCandidatesForTitle(packageLibrary, observedTitleId) {
  const presence = findPresenceByObservedId(packageLibrary, observedTitleId);
  return presence?.packageCandidates.map((candidate) => ({
    source: /** @type {const} */ ('library'),
    package: candidate,
  })) ?? [];
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
 * @param {{ type: 'cacheLoaded', games: GameGroup[] }
 *   | { type: 'refreshSucceeded', snapshot: ManagedLibrarySnapshot }
 *   | { type: 'refreshFailed', error: string }} action
 * @returns {LibraryState}
 */
export function reduceLibraryState(state, action) {
  switch (action.type) {
    case 'cacheLoaded':
      return action.games.length > 0 ? { ...state, games: action.games } : state;
    case 'refreshSucceeded':
      return {
        games: action.snapshot.games,
        packageLibrary: action.snapshot.packageLibrary,
        refreshError: '',
      };
    case 'refreshFailed':
      return { ...state, refreshError: action.error };
    default:
      return state;
  }
}
