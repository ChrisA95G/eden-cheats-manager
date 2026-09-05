import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';

test('baseline state-layer RGB channels agree with both theme palettes', () => {
  const css = readFileSync(new URL('../styles/tokens.css', import.meta.url), 'utf8');
  const schemes = [css.split(':root {')[1], css.split(':root[data-theme="dark"] {')[1]];
  for (const scheme of schemes) {
    const block = scheme.split('}')[0];
    for (const role of ['on-surface', 'primary']) {
      const hex = block.match(new RegExp(`--md-sys-color-${role}: #([a-f0-9]{6});`))?.[1];
      const channels = block.match(new RegExp(`--md-sys-color-${role}-rgb: ([\\d ]+);`))?.[1];
      assert.ok(hex && channels);
      assert.deepEqual(channels.split(' ').map(Number), hex.match(/../g)?.map(value => parseInt(value, 16)));
    }
  }
});

test('shipped styles do not require color-mix unsupported by Thor WebView 109', () => {
  /** @param {URL} dir */
  function check(dir) {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const url = new URL(entry.name + (entry.isDirectory() ? '/' : ''), dir);
      if (entry.isDirectory()) check(url);
      else if (/\.(svelte|css)$/.test(entry.name)) {
        assert.doesNotMatch(readFileSync(url, 'utf8'), /color-mix\(/, url.pathname);
      }
    }
  }
  check(new URL('../../', import.meta.url));
});
