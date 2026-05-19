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
 * @param {Object} settings
 * @param {string} settings.targetMode
 * @param {string} [settings.adbPath]
 * @param {string} [settings.pcLoadDir]
 */
/**
 * Load games from the on-disk cache instantly, then kick off a fresh scan in
 * the background to pick up any changes. Works for all target modes.
 * @param {Object} settings
 */
export async function loadCachedGamesThenRescan(settings) {
  const mode = settings?.targetMode;
  if (!mode) return;

  try {
    const cacheCmd = (mode === 'pc') ? 'get_cached_games_pc' : 'get_cached_games_android';
    const cached = /** @type {GameGroup[]} */ (await invoke(cacheCmd));
    if (cached.length > 0) {
      games.set(cached);
    }
  } catch (_) {}

  // Rescan in background — may fail silently if no ADB device connected.
  scanGames(settings).catch(() => {});
}

export async function scanGames(settings) {
  debugLog('scanGames called', { targetMode: settings?.targetMode });
  gamesLoading.set(true);
  gamesError.set('');
  try {
    if (settings.targetMode === 'android') {
      const status = await invoke('get_adb_status', { adbPath: settings.adbPath });
      if (!status.connected) {
        games.set([]);
        selectedGame.set(null);
        gamesError.set(`No device connected. ${status.details}`);
        return;
      }
    }
    /** @type {GameGroup[]} */
    let list;
    if (settings.targetMode === 'androidNative') {
      debugLog('invoking scan_eden_games_android_native');
      list = await invoke('scan_eden_games_android_native');
      debugLog('scan result', { count: list.length });
    } else if (settings.targetMode === 'android') {
      list = await invoke('scan_eden_games_android', { adbPath: settings.adbPath });
    } else {
      list = await invoke('scan_eden_games_pc', { loadDir: settings.pcLoadDir });
      // Fire-and-forget ROM path cache update.
      const titleNames = list.flatMap(g =>
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
