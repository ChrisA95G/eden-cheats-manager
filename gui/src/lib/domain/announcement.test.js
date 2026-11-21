import assert from 'node:assert/strict';
import test from 'node:test';
import { fetchAnnouncement, listAnnouncementHistory, readPastAnnouncement, unreadAnnouncement, rememberDismissal } from './announcement.js';

const first = '2026-11-20T09:00:00Z';
const second = '2026-11-20T09:00:01Z';
const file = 'updates/beta-updates.md';
const empty = { getItem: () => null };

function memoryStorage() {
  const entries = new Map();
  return {
    get length() { return entries.size; },
    key: (index = 0) => [...entries.keys()][index] ?? null,
    getItem: (key = '') => entries.get(key) ?? null,
    setItem: (key = '', value = '') => entries.set(key, value),
  };
}

test('dismissing a notice suppresses edits and older notices across launches', () => {
  const storage = memoryStorage();
  const read = (stamp = first, body = 'Original') => unreadAnnouncement(stamp + '\r\n' + body, file, 'desktop', storage);
  const original = read();
  assert.ok(original);
  rememberDismissal(original, storage);
  assert.equal(read(first, 'Corrected'), null);
  assert.equal(read('2026-11-19T12:00:00Z'), null);
  const revised = read(second);
  assert.ok(revised);
  rememberDismissal(revised, storage);
  rememberDismissal(original, storage);
  assert.equal(read(second), null);
});

test('invalid timestamps, empty notices and oversized content are ignored', () => {
  for (const source of ['# No date', '2026-02-30T00:00:00Z\nHi', first + '\n# Title only', first + '\n', first + '\n' + 'x'.repeat(32768)]) {
    assert.equal(unreadAnnouncement(source, file, 'android', empty), null);
  }
});

test('remote Markdown renders formatting and HTTPS links, and escapes embedded HTML', () => {
  const source = first + '\n# News\n\n**Hello** [Details](https://example.org) <b>raw</b> ![image](https://example.org/i.png) [local](file:///tmp/example)';
  const notice = unreadAnnouncement(source, file, 'desktop', empty);
  assert.equal(notice?.title, 'News');
  assert.match(notice.html, /<strong>Hello<\/strong>/);
  assert.match(notice.html, /href="https:\/\/example.org" target="_blank" rel="noopener noreferrer"/);
  assert.match(notice.html, /&lt;b&gt;raw&lt;\/b&gt;/);
  assert.doesNotMatch(notice.html, /<img|href="file:/);
});

test('platform sections stop at the next second-level heading', () => {
  const source = first + '\n# News\n\nShared intro\n\n## Android\n\nPhone copy\n\n## Desktop\n\nPC copy\n\n## Feedback\n\nShared ending';
  for (const platform of /** @type {const} */ (['desktop', 'android'])) {
    const html = unreadAnnouncement(source, file, platform, empty)?.html ?? '';
    assert.match(html, /Shared intro/);
    assert.match(html, /Shared ending/);
    assert.equal(html.includes('Phone copy'), platform === 'android');
    assert.equal(html.includes('PC copy'), platform === 'desktop');
  }
});

test('the index orders independent notices and skips missing or invalid entries', async t => {
  const other = 'news/Community news.md';
  const base = 'https://raw.githubusercontent.com/ChrisA95G/eden-cheats-manager/announcements/';
  const files = new Map([
    ['index.json', JSON.stringify([null, { file: '../README.md' }, { file: 'gone.md' }, { file: 'invalid.md' },
      { file, action: 'app-updates' }, { file: other }])],
    ['invalid.md', 'no timestamp'],
    [file, first + '\n# Beta\nHow to join'],
    [other, first + '\n# News\nAnother topic'],
  ]);
  const requests = /** @type {string[]} */ ([]);
  t.mock.method(globalThis, 'fetch', async (url = '') => {
    assert.ok(url.startsWith(base));
    const path = url.slice(base.length);
    requests.push(path);
    const body = files.get(decodeURIComponent(path));
    return new Response(body, { status: body ? 200 : 404 });
  });
  const storage = memoryStorage();
  const read = () => fetchAnnouncement('desktop', new AbortController().signal, storage);
  const beta = await read();
  assert.ok(beta?.opensUpdateSettings);
  assert.deepEqual(requests, ['index.json', 'gone.md', 'invalid.md', file]);
  rememberDismissal(beta, storage);
  const news = await read();
  assert.equal(news?.file, other);
  assert.equal(news.opensUpdateSettings, false);
  assert.equal(requests.at(-1), 'news/Community%20news.md');
  rememberDismissal(news, storage);
  assert.equal(await read(), null);
  files.set(file, second + '\nRevised beta announcement');
  assert.equal((await read())?.file, file);
  files.set('index.json', '[]');
  assert.equal(await read(), null);
});

test('dismissed revisions and separate notices with the same date can be reread offline', async t => {
  const fetch = t.mock.method(globalThis, 'fetch', () => { throw new Error('offline'); });
  const storage = memoryStorage();
  const other = 'news/maintenance.md';
  const entries = [
    { file, timestamp: first, body: 'Original' },
    { file: other, timestamp: first, body: 'Maintenance' },
    { file, timestamp: second, body: 'New copy\n## Android\nMobile only' },
  ];
  for (const { file, timestamp, body } of entries) {
    const notice = unreadAnnouncement(timestamp + '\n' + body, file, 'desktop', storage);
    assert.ok(notice);
    rememberDismissal(notice, storage);
  }
  const history = listAnnouncementHistory(storage);
  assert.equal(history.length, 3);
  assert.deepEqual(history[0], { file, timestamp: second });
  const signal = new AbortController().signal;
  for (const entry of history) {
    const notice = await readPastAnnouncement(entry, 'desktop', signal, storage);
    const expected = entries.find(e => e.file === entry.file && e.timestamp === entry.timestamp);
    assert.ok(expected && notice);
    assert.ok(notice.html.includes(expected.body.split('\n')[0]));
    assert.doesNotMatch(notice.html, /Mobile only/);
    assert.equal(unreadAnnouncement(notice.source, entry.file, 'desktop', storage), null);
  }
  assert.equal(fetch.mock.callCount(), 0);
});

test('a date-only dismissal can recover its original revision when still available', async t => {
  const storage = memoryStorage();
  storage.setItem('ecm-dismissed-announcement:' + file, first);
  const [entry] = listAnnouncementHistory(storage);
  const signal = new AbortController().signal;
  let remote = second + '\nNewer revision';
  const fetch = t.mock.method(globalThis, 'fetch', () => new Response(remote));
  assert.equal(await readPastAnnouncement(entry, 'desktop', signal, storage), null);
  remote = first + '\nOriginal revision';
  assert.match((await readPastAnnouncement(entry, 'desktop', signal, storage))?.html ?? '', /Original/);
  remote = '';
  assert.match((await readPastAnnouncement(entry, 'desktop', signal, storage))?.html ?? '', /Original/);
  assert.equal(fetch.mock.callCount(), 2);
  assert.ok(unreadAnnouncement(second + '\nNewer revision', file, 'desktop', storage));
});
