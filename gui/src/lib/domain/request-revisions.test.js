import test from 'node:test';
import assert from 'node:assert/strict';

import { createRequestRevisions } from './request-revisions.js';

test('selection changes reject all previous async completions', () => {
  const requests = createRequestRevisions(['catalog', 'installed']);
  requests.beginSelection();
  const catalog = requests.begin('catalog');
  const installed = requests.begin('installed');

  requests.beginSelection();
  assert.equal(requests.isCurrent(catalog), false);
  assert.equal(requests.isCurrent(installed), false);
});

test('newer same-selection request rejects the older completion only for that resource', () => {
  const requests = createRequestRevisions(['catalog', 'installed']);
  requests.beginSelection();
  const oldCatalog = requests.begin('catalog');
  const installed = requests.begin('installed');
  const newCatalog = requests.begin('catalog');

  assert.equal(requests.isCurrent(oldCatalog), false);
  assert.equal(requests.isCurrent(newCatalog), true);
  assert.equal(requests.isCurrent(installed), true);
});

test('unknown resources fail closed', () => {
  const requests = createRequestRevisions(['catalog']);
  assert.throws(() => requests.begin('candidate'), /Unknown request resource/);
});

test('invalidation rejects one resource without disturbing another', () => {
  const requests = createRequestRevisions(['catalog', 'installed']);
  requests.beginSelection();
  const catalog = requests.begin('catalog');
  const installed = requests.begin('installed');

  requests.invalidate('catalog');
  assert.equal(requests.isCurrent(catalog), false);
  assert.equal(requests.isCurrent(installed), true);
});
