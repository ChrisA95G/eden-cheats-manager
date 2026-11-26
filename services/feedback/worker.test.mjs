import assert from 'node:assert/strict';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { Miniflare, convertV4MiniflareOptions } from 'miniflare';

const url = 'http://localhost/v1/feedback';
const config = JSON.parse(await readFile(new URL('wrangler.jsonc', import.meta.url), 'utf8'));
const migration = await readFile(new URL('migrations/0001_feedback.sql', import.meta.url), 'utf8');

function startWorker(directory, limit = config.ratelimits[0].simple.limit) {
  const limiter = config.ratelimits[0];
  return new Miniflare(convertV4MiniflareOptions({
    modules: true,
    scriptPath: fileURLToPath(new URL(config.main, import.meta.url)),
    compatibilityDate: config.compatibility_date,
    d1Databases: { DB: config.d1_databases[0].database_name },
    ratelimits: {
      [limiter.name]: { namespace_id: limiter.namespace_id, simple: { ...limiter.simple, limit } },
    },
    resourcePersistencePath: directory,
  }));
}

test('a submitted message can be read after the Worker restarts', async t => {
  const directory = await mkdtemp(join(tmpdir(), 'edc-feedback-test-'));
  let worker = startWorker(directory);
  t.after(async () => {
    await worker.dispose();
    await rm(directory, { recursive: true, force: true });
  });
  const db = await worker.getD1Database('DB');
  await db.prepare(migration).run();
  const message = "I'd like to see the build ID.\nThanks 🎮";
  const response = await worker.dispatchFetch(url, {
    method: 'POST', headers: { 'Content-Type': 'text/plain; charset=utf-8', 'User-Agent': 'test-client' },
    body: message,
  });
  assert.equal(response.status, 201);
  await worker.dispose();
  worker = startWorker(directory);
  const reopened = await worker.getD1Database('DB');
  assert.deepEqual((await reopened.prepare('SELECT * FROM feedback').all()).results, [{ id: 1, message }]);
  assert.equal((await worker.dispatchFetch(url)).status, 405);
});

test('a bug report with an apostrophe, SQL and HTML round-trips as one message', async t => {
  const worker = startWorker();
  t.after(() => worker.dispose());
  const db = await worker.getD1Database('DB');
  await db.prepare(migration).run();

  const message = [
    "I can't see the game title.",
    "The log shows: SELECT title FROM games WHERE id = 'demo';",
    'The screen shows: <strong class="title">Unknown &amp; missing</strong>',
  ].join('\n');
  const response = await worker.dispatchFetch(url, {
    method: 'POST', headers: { 'Content-Type': 'text/plain' }, body: message,
  });

  assert.equal(response.status, 201);
  const { results } = await db.prepare('SELECT message FROM feedback').all();
  assert.deepEqual(results, [{ message }]);
});

test('validates input before writing and only acknowledges a successful insert', async t => {
  const worker = startWorker();
  t.after(() => worker.dispose());
  const send = (body, type = 'text/plain') => worker.dispatchFetch(url, {
    method: 'POST', headers: { 'Content-Type': type }, body,
  });
  assert.equal((await send('missing database table')).status, 503);
  const db = await worker.getD1Database('DB');
  await db.prepare(migration).run();
  assert.equal((await send(' \n\t')).status, 400);
  assert.equal((await send(new Uint8Array([0xff]))).status, 400);
  assert.equal((await send('{}', 'application/json')).status, 415);
  assert.equal((await worker.dispatchFetch(url + '/other')).status, 404);
  assert.equal((await send('é'.repeat(2049))).status, 413);
  const boundary = 'é'.repeat(2048);
  assert.equal((await send(boundary)).status, 201);
  assert.deepEqual((await db.prepare('SELECT message FROM feedback').all()).results, [{ message: boundary }]);
});

test('valid submissions share a route limit and a rejected message never reaches D1', async t => {
  const worker = startWorker(undefined, 1);
  t.after(() => worker.dispose());
  const db = await worker.getD1Database('DB');
  await db.prepare(migration).run();
  const submit = (address, body) => worker.dispatchFetch(address, {
    method: 'POST', headers: { 'Content-Type': 'text/plain' }, body,
  });

  assert.equal((await submit(url, '  ')).status, 400);
  assert.equal((await submit(url, 'first message')).status, 201);
  const limited = await submit(url + '?source=beta', 'second message');
  assert.equal(limited.status, 429);
  assert.equal(limited.headers.get('Retry-After'), '60');
  assert.deepEqual((await db.prepare('SELECT message FROM feedback').all()).results, [
    { message: 'first message' },
  ]);
});
