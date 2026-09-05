import { invoke } from '@tauri-apps/api/core';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { revealItemInDir } from '@tauri-apps/plugin-opener';

/** @typedef {import('./types.js').AppSettings} AppSettings */
/** @typedef {import('./types.js').BootstrapResult} BootstrapResult */
/** @typedef {import('./types.js').EdenLoadAccessStatus} EdenLoadAccessStatus */
/** @typedef {import('./types.js').GameGroup} GameGroup */
/** @typedef {import('./types.js').GameInfo} GameInfo */
/** @typedef {import('./types.js').GameLibraryStatus} GameLibraryStatus */
/** @typedef {import('./types.js').InstalledCheat} InstalledCheat */
/** @typedef {import('./types.js').ManagedLibrarySnapshot} ManagedLibrarySnapshot */
/** @typedef {import('./types.js').PackageDiscoveryStatus} PackageDiscoveryStatus */
/** @typedef {import('./types.js').PackageMetadata} PackageMetadata */
/** @typedef {import('./types.js').Platform} Platform */

/** @param {unknown} value @returns {Platform} */
export function parsePlatform(value) {
  if (value === 'desktop' || value === 'android') return value;
  throw new Error(`Unsupported platform: ${String(value)}`);
}

/**
 * @param {Platform} platform
 * @param {string} androidCommand
 * @param {string} desktopCommand
 */
function platformCommand(platform, androidCommand, desktopCommand) {
  if (platform === 'android') return androidCommand;
  if (platform === 'desktop') return desktopCommand;
  throw new Error(`Unsupported platform: ${String(platform)}`);
}

/** @returns {Promise<BootstrapResult>} */
export async function loadBootstrap() {
  const [rawPlatform, settings] = await Promise.all([
    invoke('get_platform'),
    /** @type {Promise<AppSettings>} */ (invoke('get_settings')),
  ]);
  const platform = parsePlatform(rawPlatform);
  let edenAccess = /** @type {EdenLoadAccessStatus | null} */ (null);
  let edenAccessError = '';

  if (platform === 'android') {
    try {
      edenAccess = /** @type {EdenLoadAccessStatus} */ (
        await invoke('get_eden_load_access_status')
      );
    } catch (error) {
      edenAccessError = String(error);
    }
  }

  return { platform, settings, edenAccess, edenAccessError };
}

/** @param {AppSettings} settings */
export async function saveSettings(settings) {
  await invoke('save_settings', { settings });
}

/** @returns {Promise<string>} */
export async function detectPcLoadDir() {
  return /** @type {string} */ (await invoke('detect_pc_load_dir'));
}

/** @param {Platform} platform @returns {Promise<GameGroup[]>} */
export async function getCachedGames(platform) {
  const command = platformCommand(
    platform,
    'get_cached_games_android',
    'get_cached_games_pc',
  );
  return /** @type {GameGroup[]} */ (await invoke(command));
}

/** @param {Platform} platform @returns {Promise<ManagedLibrarySnapshot>} */
export async function scanManagedLibrary(platform) {
  const command = platformCommand(
    platform,
    'scan_managed_library_android_native',
    'scan_managed_library_pc',
  );
  return /** @type {ManagedLibrarySnapshot} */ (await invoke(command));
}

/** @returns {Promise<EdenLoadAccessStatus>} */
export async function getEdenLoadAccessStatus() {
  return /** @type {EdenLoadAccessStatus} */ (
    await invoke('get_eden_load_access_status')
  );
}

export async function selectEdenLoadDirectory() {
  await invoke('select_eden_load_directory');
}

/** @returns {Promise<string>} */
export async function testEdenLoadDirectory() {
  return /** @type {string} */ (await invoke('test_eden_load_directory'));
}

/** @returns {Promise<PackageDiscoveryStatus>} */
export async function getPackageDiscoveryStatus() {
  return /** @type {PackageDiscoveryStatus} */ (
    await invoke('get_package_discovery_status')
  );
}

/** @returns {Promise<GameLibraryStatus>} */
export async function getGameLibraryStatus() {
  return /** @type {GameLibraryStatus} */ (await invoke('get_game_library_status'));
}

export async function selectProdKeysDocument() {
  await invoke('select_prod_keys_document');
}

export async function selectGamePackageDocument() {
  await invoke('select_game_package_document');
}

export async function selectGameLibraryDirectory() {
  await invoke('select_game_library_directory');
}

/** @param {string} currentPath @returns {Promise<string | null>} */
export async function pickEdenLoadDirectory(currentPath = '') {
  return /** @type {string | null} */ (await openDialog({
    directory: true,
    multiple: false,
    title: 'Select Eden load directory',
    defaultPath: currentPath || undefined,
  }));
}

/** @param {string} currentPath @returns {Promise<string | null>} */
export async function pickPackageLibraryDirectory(currentPath = '') {
  return /** @type {string | null} */ (await openDialog({
    directory: true,
    multiple: false,
    title: 'Select game-package library',
    defaultPath: currentPath || undefined,
  }));
}

/** @param {string} currentPath @returns {Promise<string | null>} */
export async function pickProdKeysFile(currentPath = '') {
  return /** @type {string | null} */ (await openDialog({
    directory: false,
    multiple: false,
    title: 'Select prod.keys',
    filters: [{ name: 'prod.keys', extensions: ['keys'] }],
    defaultPath: currentPath || undefined,
  }));
}

/** @returns {Promise<string | null>} */
export async function pickGamePackageFile() {
  return /** @type {string | null} */ (await openDialog({
    directory: false,
    multiple: false,
    title: 'Select an NSP or XCI package',
    filters: [{ name: 'Nintendo Switch package', extensions: ['nsp', 'xci'] }],
  }));
}

/**
 * @param {Platform} platform
 * @param {AppSettings} settings
 * @param {string} expectedBaseTitleId
 * @param {string} [packagePath]
 * @returns {Promise<PackageMetadata>}
 */
export async function inspectPackageForTitle(
  platform,
  settings,
  expectedBaseTitleId,
  packagePath = '',
) {
  switch (platform) {
    case 'android':
      return /** @type {PackageMetadata} */ (
        await invoke('discover_package_metadata_for_title', { expectedBaseTitleId })
      );
    case 'desktop':
      return /** @type {PackageMetadata} */ (
        await invoke('discover_package_metadata_for_title_pc', {
          prodKeysPath: settings.prodKeysPath,
          packagePath,
          expectedBaseTitleId,
        })
      );
    default:
      throw new Error(`Unsupported platform: ${String(platform)}`);
  }
}

/** @param {Platform} platform @returns {Promise<boolean>} */
export async function revealAppLog(platform) {
  if (platform === 'android') return false;
  if (platform !== 'desktop') {
    throw new Error(`Unsupported platform: ${String(platform)}`);
  }
  const path = /** @type {string} */ (await invoke('get_app_log_path'));
  if (!path) return false;
  await revealItemInDir(path);
  return true;
}

/** @param {string} titleId @returns {Promise<GameInfo>} */
export async function searchCheats(titleId) {
  return /** @type {GameInfo} */ (await invoke('search_cheats', { titleId }));
}

/**
 * @param {Platform} platform
 * @param {AppSettings} settings
 * @param {string} titleId
 * @returns {Promise<InstalledCheat[]>}
 */
export async function listInstalledCheats(platform, settings, titleId) {
  const command = platformCommand(
    platform,
    'list_installed_cheats_android_native',
    'list_installed_cheats_pc',
  );
  const args = platform === 'android'
    ? { titleId }
    : { loadDir: settings.pcLoadDir, titleId };
  return /** @type {InstalledCheat[]} */ (await invoke(command, args));
}

/**
 * @param {Platform} platform
 * @param {AppSettings} settings
 * @param {{ titleId: string, cheatName: string, buildId: string, content: string }} cheat
 */
export async function installCheat(platform, settings, cheat) {
  const command = platformCommand(
    platform,
    'install_cheat_android_native',
    'install_cheat_pc',
  );
  const args = platform === 'android'
    ? cheat
    : { loadDir: settings.pcLoadDir, ...cheat };
  await invoke(command, args);
}

/**
 * @param {Platform} platform
 * @param {AppSettings} settings
 * @param {{ titleId: string, cheatName: string, buildId: string }} cheat
 */
export async function deleteInstalledCheat(platform, settings, cheat) {
  const command = platformCommand(
    platform,
    'delete_cheat_android_native',
    'delete_cheat_pc',
  );
  const args = platform === 'android'
    ? cheat
    : { loadDir: settings.pcLoadDir, ...cheat };
  await invoke(command, args);
}

/** @param {string} titleId @param {string} apiToken @returns {Promise<number>} */
export async function fetchCheatsOnline(titleId, apiToken) {
  return /** @type {number} */ (
    await invoke('fetch_cheats_online', { titleId, apiToken })
  );
}

/** @param {string} titleId @returns {Promise<number>} */
export async function clearFetchedCheats(titleId) {
  return /** @type {number} */ (await invoke('clear_api_cheats', { titleId }));
}

/**
 * @param {string} titleId
 * @param {string} buildId
 * @param {string} content
 * @returns {Promise<import('./types.js').CheatEntry>}
 */
export async function saveCustomCheat(titleId, buildId, content) {
  return /** @type {import('./types.js').CheatEntry} */ (
    await invoke('save_custom_cheat', { titleId, buildId, content })
  );
}

/** @param {number} cheatId */
export async function deleteCustomCheat(cheatId) {
  await invoke('delete_custom_cheat', { cheatId });
}
