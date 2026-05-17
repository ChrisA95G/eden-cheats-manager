import { writable } from 'svelte/store';

/** @type {import('svelte/store').Writable<string[]>} */
export const debugLogs = writable([]);

/**
 * @param {string} msg
 * @param {any} [data]
 */
export function debugLog(msg, data) {
  const ts = new Date().toISOString().slice(11, 23);
  const line = data !== undefined
    ? `[${ts}] ${msg}: ${JSON.stringify(data)}`
    : `[${ts}] ${msg}`;
  console.log(line);
  debugLogs.update(logs => [...logs.slice(-99), line]);
}

export function clearDebugLogs() {
  debugLogs.set([]);
}
