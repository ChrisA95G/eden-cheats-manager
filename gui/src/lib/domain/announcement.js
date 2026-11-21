import MarkdownIt from 'markdown-it';

const feed = 'https://raw.githubusercontent.com/ChrisA95G/eden-cheats-manager/announcements/';
const maxLength = 32 * 1024;
const dismissedPrefix = 'ecm-dismissed-announcement:';
const archivePrefix = 'ecm-announcement-history:';
const markdown = new MarkdownIt({ html: false }).disable('image');
markdown.validateLink = (/** @type {string} */ url) => /^https:\/\//i.test(url);
/** @type {import('markdown-it').RendererRule} */
function renderExternalLink(tokens, index, options, env, renderer) {
  tokens[index].attrSet('target', '_blank');
  tokens[index].attrSet('rel', 'noopener noreferrer');
  return renderer.renderToken(tokens, index, options);
}
markdown.renderer.rules.link_open = renderExternalLink;

/** @param {unknown} file @returns {file is string} */
function isAnnouncementFile(file) {
  return typeof file === 'string' && file.endsWith('.md') && !file.includes('\\')
    && file.split('/').every(part => part && part !== '.' && part !== '..');
}

/** @param {string} timestamp */
function validTimestamp(timestamp) {
  if (!/^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ$/.test(timestamp)) return false;
  const date = new Date(timestamp);
  return Number.isFinite(date.getTime()) && date.toISOString().replace('.000Z', 'Z') === timestamp;
}

/** @typedef {import('../api/types.js').Platform} Platform */
/** @typedef {{ file: string, timestamp: string }} AnnouncementId */

/** @param {string} source @param {string} file @param {Platform} platform */
function parseAnnouncement(source, file, platform) {
  if (source.length > maxLength) return null;
  const [firstLine, ...body] = source.replace(/^\uFEFF/, '').split(/\r?\n/);
  if (!validTimestamp(firstLine)) return null;

  const tokens = markdown.parse(body.join('\n'), {});
  let title = 'News from EDC';
  if (tokens[0]?.tag === 'h1') {
    title = tokens[1].content || title;
    tokens.splice(0, 3);
  }
  // A top-level "## Android" or "## Desktop" section is visible on that platform.
  let visible = true;
  const visibleTokens = tokens.filter((token, index) => {
    if (token.level === 0 && token.type === 'heading_open' && ['h1', 'h2'].includes(token.tag)) {
      const section = tokens[index + 1].content.toLowerCase();
      visible = token.tag !== 'h2' || !['android', 'desktop'].includes(section) || section === platform;
    }
    return visible;
  });
  const html = markdown.renderer.render(visibleTokens, markdown.options, {}).trim();
  return html ? { file, timestamp: firstLine, title, html, source } : null;
}

/** @param {string} source @param {string} file @param {Platform} platform @param {Pick<Storage, 'getItem'>} storage */
export function unreadAnnouncement(source, file, platform, storage) {
  const notice = parseAnnouncement(source, file, platform);
  return notice && notice.timestamp > (storage.getItem(dismissedPrefix + file) ?? '') ? notice : null;
}

/** @param {AnnouncementId & {source: string}} notice @param {Pick<Storage, 'getItem' | 'setItem'>} storage */
export function rememberDismissal({ file, timestamp, source }, storage) {
  const key = dismissedPrefix + file;
  if (timestamp > (storage.getItem(key) ?? '')) storage.setItem(key, timestamp);
  storage.setItem(archivePrefix + timestamp + '/' + file, source);
}

/** @param {Pick<Storage, 'length' | 'key' | 'getItem'>} storage */
export function listAnnouncementHistory(storage) {
  const ids = new Set();
  for (let i = 0; i < storage.length; i++) {
    const key = storage.key(i) ?? '';
    if (key.startsWith(archivePrefix)) ids.add(key.slice(archivePrefix.length));
    if (key.startsWith(dismissedPrefix)) ids.add(storage.getItem(key) + '/' + key.slice(dismissedPrefix.length));
  }
  // Canonical UTC timestamps sort chronologically, including revisions of the same file.
  return [...ids].sort().reverse().flatMap(id => {
    const [timestamp, ...path] = id.split('/');
    const file = path.join('/');
    return validTimestamp(timestamp) && isAnnouncementFile(file) ? [{ file, timestamp }] : [];
  });
}

/** @param {AnnouncementId} entry @param {Platform} platform @param {AbortSignal} signal @param {Pick<Storage, 'getItem' | 'setItem'>} storage */
export async function readPastAnnouncement({ file, timestamp }, platform, signal, storage) {
  if (!isAnnouncementFile(file) || !validTimestamp(timestamp)) return null;
  const key = archivePrefix + timestamp + '/' + file;
  const cached = storage.getItem(key);
  // Older installations recorded the dismissal date without keeping the Markdown.
  const source = cached ?? await fetchText(file, signal);
  const notice = source && parseAnnouncement(source, file, platform);
  if (!notice || notice.timestamp !== timestamp) return null;
  if (cached === null) {
    try { storage.setItem(key, source); } catch { /* It can still be read this time. */ }
  }
  return notice;
}

/** @param {string} path @param {AbortSignal} signal */
async function fetchText(path, signal) {
  if (signal.aborted) return null;
  const controller = new AbortController();
  const abort = () => controller.abort();
  signal.addEventListener('abort', abort, { once: true });
  const timeout = setTimeout(abort, 8000);
  try {
    const url = feed + path.split('/').map(encodeURIComponent).join('/');
    const response = await fetch(url, { signal: controller.signal, cache: 'no-cache', credentials: 'omit', referrerPolicy: 'no-referrer' });
    if (!response.ok || Number(response.headers.get('content-length')) > maxLength) return null;
    const text = await response.text();
    return text.length <= maxLength ? text : null;
  } catch {
    return null;
  } finally {
    clearTimeout(timeout);
    signal.removeEventListener('abort', abort);
  }
}

/** @param {import('../api/types.js').Platform} platform @param {AbortSignal} signal @param {Pick<Storage, 'getItem'>} [storage] */
export async function fetchAnnouncement(platform, signal, storage = localStorage) {
  const index = await fetchText('index.json', signal);
  if (!index) return null;
  const entries = JSON.parse(index);
  if (!Array.isArray(entries)) return null;
  for (const entry of entries) {
    if (signal.aborted) break;
    const file = entry?.file;
    if (!isAnnouncementFile(file)) continue;
    const source = await fetchText(file, signal);
    if (!source) continue;
    const notice = unreadAnnouncement(source, file, platform, storage);
    if (notice) return { ...notice, opensUpdateSettings: entry.action === 'app-updates' };
  }
  return null;
}
