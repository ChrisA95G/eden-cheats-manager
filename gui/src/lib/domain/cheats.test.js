import test from 'node:test';
import assert from 'node:assert/strict';

import {
  cheatFileName,
  createInstalledIndex,
  groupCheatEntries,
  installedTupleKey,
  parseCheatSections,
  sectionActionKey,
  toCheatName,
} from './cheats.js';

test('parseCheatSections preserves headers and order while trimming section ends', () => {
  const content = [
    'ignored preamble',
    '  [Infinite HP]  ',
    '04000000 00000000',
    '',
    '[Infinite HP]',
    'line with spaces   ',
    '',
  ].join('\n');

  assert.deepEqual(parseCheatSections(content), [
    {
      name: 'Infinite HP',
      content: '  [Infinite HP]  \n04000000 00000000',
    },
    {
      name: 'Infinite HP',
      content: '[Infinite HP]\nline with spaces',
    },
  ]);
  assert.deepEqual(parseCheatSections('no header\njust text'), []);
});

test('toCheatName crops before filtering and supplies a stable empty fallback', () => {
  assert.equal(toCheatName('HP+ Speed! (x2)'), 'HP Speed (x2)');
  assert.equal(toCheatName(`${'A'.repeat(59)}!B`), 'A'.repeat(59));
  assert.equal(cheatFileName('!!!', 'ABCDEF'), 'cheat_ABCDEF');
});

test('groupCheatEntries preserves build order, section provenance, and credits', () => {
  const groups = groupCheatEntries([
    {
      id: 7,
      buildId: 'abc',
      content: '[One]\n1\n[One]\n2',
      credits: 'First API author',
      description: '',
      custom: false,
    },
    {
      id: 8,
      buildId: 'DEF',
      content: '[Other]\n3',
      credits: 'Second build',
      description: '',
      custom: false,
    },
    {
      id: 9,
      buildId: 'AbC',
      content: 'custom without a section',
      credits: 'Ignored custom credit',
      description: '',
      custom: true,
    },
    {
      id: 10,
      buildId: 'ABC',
      content: '[Custom]\n4',
      credits: '',
      description: '',
      custom: true,
    },
  ]);

  assert.deepEqual(groups.map((group) => group.buildId), ['ABC', 'DEF']);
  assert.equal(groups[0].credits, 'First API author');
  assert.deepEqual(groups[0].sections.map((section) => section.name), ['One', 'One', 'Custom']);
  assert.deepEqual(
    groups[0].sections.map(({ entryId, sectionIndex, custom }) => ({ entryId, sectionIndex, custom })),
    [
      { entryId: 7, sectionIndex: 0, custom: false },
      { entryId: 7, sectionIndex: 1, custom: false },
      { entryId: 10, sectionIndex: 0, custom: true },
    ],
  );
  assert.deepEqual(groups[0].customEntries, [
    { entryId: 9, content: 'custom without a section' },
    { entryId: 10, content: '[Custom]\n4' },
  ]);
});

test('installed and pending keys are collision-safe and normalize only Build ID case', () => {
  assert.notEqual(installedTupleKey('AB_C', 'D'), installedTupleKey('AB', 'C_D'));
  assert.equal(installedTupleKey('abc', 'Exact Name'), installedTupleKey('ABC', 'Exact Name'));
  assert.notEqual(installedTupleKey('ABC', 'Exact Name'), installedTupleKey('ABC', 'exact name'));
  assert.notEqual(sectionActionKey(1, 23), sectionActionKey(12, 3));

  const index = createInstalledIndex([{ buildId: 'abc', cheatName: 'Exact Name' }]);
  assert.equal(index.has(installedTupleKey('ABC', 'Exact Name')), true);
  assert.equal(index.has(installedTupleKey('ABC', 'exact name')), false);
});
