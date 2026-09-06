import { writable } from 'svelte/store';

/** @typedef {'system' | 'light' | 'dark'} Theme */
export const THEME_KEY = 'ecm-theme';
export const theme = writable(/** @type {Theme} */ ('system'));

/** @param {unknown} value @returns {Theme} */
export function normalizeTheme(value) {
  return value === 'light' || value === 'dark' ? value : 'system';
}

/** @param {Theme} preference @param {boolean} systemDark */
export function resolveTheme(preference, systemDark) {
  return preference === 'system' ? (systemDark ? 'dark' : 'light') : preference;
}

/** Apply a device-local preference independently of backend settings. */
export function initializeTheme() {
  const media = window.matchMedia('(prefers-color-scheme: dark)');
  let preference = /** @type {Theme} */ ('system');
  try { preference = normalizeTheme(localStorage.getItem(THEME_KEY)); } catch { /* Storage may be unavailable. */ }
  theme.set(preference);

  function apply() {
    document.documentElement.dataset.theme = resolveTheme(preference, media.matches);
    document.documentElement.style.colorScheme = resolveTheme(preference, media.matches);
    document.querySelector('meta[name="theme-color"]')?.setAttribute(
      'content', preference === 'dark' || (preference === 'system' && media.matches) ? '#141218' : '#fef7ff',
    );
  }
  const unsubscribe = theme.subscribe(value => {
    preference = value;
    apply();
    try { localStorage.setItem(THEME_KEY, value); } catch { /* Still usable for this session. */ }
  });
  media.addEventListener('change', apply);
  return () => { unsubscribe(); media.removeEventListener('change', apply); };
}
