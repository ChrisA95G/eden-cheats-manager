import { writable } from 'svelte/store';
import { invoke } from '@tauri-apps/api/core';
import { debugLog } from './debug.js';

/**
 * @typedef {Object} TitleEntry
 * @property {string} titleId
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
    }
    games.set(list);
  } catch (e) {
    debugLog('scanGames ERROR', String(e));
    gamesError.set(String(e));
  } finally {
    gamesLoading.set(false);
  }
}
