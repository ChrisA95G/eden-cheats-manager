import { writable } from 'svelte/store';
import { invoke } from '@tauri-apps/api/core';

export const settings = writable(null);

export async function loadSettings() {
  const s = await invoke('get_settings');
  settings.set(s);
  return s;
}

export async function saveSettings(updated) {
  await invoke('save_settings', { settings: updated });
  settings.set(updated);
}
