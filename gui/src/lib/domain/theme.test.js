import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { initializeTheme, normalizeTheme, resolveTheme, theme, THEME_KEY } from './theme.js';

test('theme defaults to system and explicit preferences override system appearance', () => {
  for (const value of [null, undefined, '', 'unknown']) assert.equal(normalizeTheme(value), 'system');
  for (const dark of [true, false]) {
    assert.equal(resolveTheme('light', dark), 'light');
    assert.equal(resolveTheme('dark', dark), 'dark');
    assert.equal(resolveTheme('system', dark), dark ? 'dark' : 'light');
  }
});

test('theme persists, follows system changes only in system mode, and cleans up listeners', () => {
  const root = { dataset: { theme: '' }, style: { colorScheme: '' } };
  const meta = { content: '', setAttribute(_name = '', value = '') { this.content = value; } };
  const media = Object.assign(new EventTarget(), { matches: true });
  let stored = 'light';
  const mocks = {
    window: { matchMedia: () => media },
    document: { documentElement: root, querySelector: () => meta },
    localStorage: { getItem: () => stored, setItem: (key = '', value = '') => { assert.equal(key, THEME_KEY); stored = value; } },
  };
  const original = Object.getOwnPropertyDescriptors(globalThis);
  let stop = () => {};
  try {
    for (const [key, value] of Object.entries(mocks)) Object.defineProperty(globalThis, key, { configurable: true, value });
    stop = initializeTheme();
    assert.equal(root.dataset.theme, 'light');
    theme.set('dark');
    assert.equal(stored, 'dark');
    assert.equal(root.style.colorScheme, 'dark');
    assert.equal(meta.content, '#141218');
    media.matches = false;
    media.dispatchEvent(new Event('change'));
    assert.equal(root.dataset.theme, 'dark');
    theme.set('system');
    assert.equal(stored, 'system');
    assert.equal(root.dataset.theme, 'light');
    media.matches = true;
    media.dispatchEvent(new Event('change'));
    assert.equal(root.dataset.theme, 'dark');
    stop();
    media.matches = false;
    media.dispatchEvent(new Event('change'));
    assert.equal(root.dataset.theme, 'dark', 'listener removed');
    mocks.localStorage.getItem = () => { throw Error('blocked'); };
    mocks.localStorage.setItem = () => { throw Error('blocked'); };
    stop = initializeTheme();
    assert.equal(root.dataset.theme, 'light');
    theme.set('dark');
    assert.equal(root.dataset.theme, 'dark', 'storage failure must not prevent switching');
  } finally {
    stop();
    for (const key of Object.keys(mocks)) {
      if (original[key]) Object.defineProperty(globalThis, key, original[key]);
      else Reflect.deleteProperty(globalThis, key);
    }
  }
});

test('pre-paint restoration agrees with runtime for light, dark, system and unavailable storage', () => {
  const html = readFileSync(new URL('../../app.html', import.meta.url), 'utf8');
  const script = html.match(/<script>([\s\S]*?)<\/script>/)?.[1];
  assert.ok(script);
  for (const preference of [null, 'system', 'light', 'dark', 'invalid', 'blocked']) {
    for (const matches of [false, true]) {
      const root = { dataset: { theme: '' }, style: { colorScheme: '' } };
      const meta = { content: '' };
      runInNewContext(script, {
        localStorage: { getItem: () => { if (preference === 'blocked') throw Error('blocked'); return preference; } },
        matchMedia: () => ({ matches }),
        document: { documentElement: root, querySelector: () => meta },
      });
      const expected = resolveTheme(normalizeTheme(preference), matches);
      assert.equal(root.dataset.theme, expected);
      assert.equal(root.style.colorScheme, expected);
      assert.equal(meta.content, expected === 'dark' ? '#141218' : '#fef7ff');
    }
  }
});
