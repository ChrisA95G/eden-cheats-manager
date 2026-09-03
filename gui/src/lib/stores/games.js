import { writable } from 'svelte/store';
import { invoke } from '@tauri-apps/api/core';
import { debugLog } from './debug.js';

/**
 * @typedef {Object} TitleEntry
 * @property {string} titleId
 * @property {string} baseTitleId
 * @property {string} name
 * @property {string} image
 * @property {"base"|"update"|"dlc"} category
 * @property {boolean} installed
 */

/**
 * @typedef {Object} GameGroup
 * @property {string} baseTitleId
 * @property {string} baseName
 * @property {string} baseImage
 * @property {boolean} baseInstalled
 * @property {TitleEntry|null} baseGame
 * @property {TitleEntry[]} updates
 * @property {TitleEntry[]} dlcs
 */

/** @type {import('svelte/store').Writable<GameGroup[]>} */
export const games = writable([]);

/** @type {import('svelte/store').Writable<TitleEntry|null>} */
export const selectedGame = writable(null);

export const gamesLoading = writable(false);
export const gamesError = writable('');

/**
 * Load games from the on-disk cache instantly, then kick off a fresh scan in
 * the background to pick up any changes. The runtime platform selects the
 * local desktop or native Android backend.
 * @param {{ pcLoadDir?: string }} settings
 * @param {'android' | 'desktop'} platform
 */
export async function loadCachedGamesThenRescan(settings, platform) {
  const cacheCmd = platform === 'android'
    ? 'get_cached_games_android'
    : 'get_cached_games_pc';

  try {
    const cached = /** @type {GameGroup[]} */ (await invoke(cacheCmd));
    if (cached.length > 0) {
      games.set(cached);
    }
  } catch (_) {}

  scanGames(settings, platform).catch(() => {});
}

/**
 * @param {{ pcLoadDir?: string }} settings
 * @param {'android' | 'desktop'} platform
 */
export async function scanGames(settings, platform) {
  debugLog('scanGames called', { platform });
  gamesLoading.set(true);
  gamesError.set('');
  try {
    /** @type {GameGroup[]} */
    let list;
    if (platform === 'android') {
      debugLog('invoking scan_eden_games_android_native');
      list = await invoke('scan_eden_games_android_native');
      debugLog('scan result', { count: list.length });
    } else {
      list = await invoke('scan_eden_games_pc', { loadDir: settings.pcLoadDir ?? '' });
      // Fire-and-forget ROM path cache update.
      const titleNames = list.flatMap((/** @type {GameGroup} */ g) =>
        g.baseGame ? [[g.baseGame.titleId, g.baseName]] : []
      );
      if (titleNames.length > 0) {
        invoke('scan_and_update_rom_cache', { titleNames }).catch(() => {});
      }
    }
    games.set(list);
  } catch (e) {
    debugLog('scanGames ERROR', String(e));
    gamesError.set(String(e));
  } finally {
    gamesLoading.set(false);
  }
}
