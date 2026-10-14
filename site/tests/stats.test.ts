import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';

import { fromFragment } from '../src/lib/stats/controller';
import { cardGrid } from '../src/lib/stats/card';
import { EXAMPLE, ROWS, parse, shareLink } from '../src/lib/stats/model';
import { Grid } from '../src/lib/term/grid';
import { paint } from '../src/lib/term/paint';

const fixture = JSON.parse(readFileSync('../tests/fixtures/stats/share-v2.txt', 'utf8'));
const fragment = new URL(fixture.link).hash;
const older = JSON.parse(readFileSync('../tests/fixtures/stats/share-v1.txt', 'utf8'));
const v1 = new URL(older.link).hash;

for (const [name, link] of [['Rust share link', fixture], ['link shared before v2', older]] as const) {
  test(`the ${name} is parsed without losing any values`, () => {
    const stats = parse(new URL(link.link).hash);
    assert.ok(stats);
    assert.deepEqual(stats.windows, { '24h': link['24h'], '7d': link['7d'], all: link.all });
    assert.equal(stats.days, link.days);
    assert.deepEqual(stats.levels, link.levels);
    assert.equal(shareLink(stats), link.link);
  });
}

for (const [name, hash] of [
  ['too long', fragment + 'a'.repeat(1024)],
  ['trailing newline', fragment + '\n'],
  ['missing field', fragment.replace(/&7d=[^&]+/, '')],
  ['extra field', fragment + '&name=x'],
  ['wrong map length', fragment.slice(0, -1)],
  ['unsafe integer', fragment.replace('days=110', 'days=9007199254740992')],
  ['markup', fragment.replace('24h=4.', '24h=<img>.')],
  ['invalid level', '#v1&24h=0.0.0.0.0.0&7d=0.0.0.0.0.0&all=0.0.0.0.0.0&days=1&map=4A'],
  ['nonzero spare bits', '#v1&24h=0.0.0.0.0.0&7d=0.0.0.0.0.0&all=0.0.0.0.0.0&days=1&map=AQ'],
  ['wrong version', fragment.replace('#v2', '#v3')],
  ['v1 with v2 numbers', fragment.replace('#v2', '#v1')],
  ['v2 with v1 numbers', v1.replace('#v1', '#v2')],
] as const) {
  test(`a ${name} link gives no card to draw`, () => { assert.equal(parse(hash), null); assert.equal(fromFragment(hash), null); });
}

test('the example opens on seven days and switching changes the card grid', () => {
  const card = fromFragment('');
  assert.ok(card); assert.equal(card.example, true); assert.equal(card.window, '7d');
  assert.match(card.grid().plain(), /agents started\s+31/);
  card.select('24h'); assert.match(card.grid().plain(), /agents started\s+4/);
  card.select('all'); assert.match(card.grid().plain(), /agents started\s+212/);
  assert.equal(fromFragment(fragment)?.example, false);
});

test('the card shows the rows of the Stats tab in its order', () => {
  const stats = parse(fragment); assert.ok(stats);
  const grid = cardGrid(stats, '7d');
  const shown = Array.from({ length: 9 }, (_, i) => grid.line(i + 2).slice(4, 31).replace(/\s+/g, ' ').trim());
  assert.deepEqual(shown, ['agents started 31', 'prompts 402', 'turns 377', 'agent time 41h 03m', 'most at once 7', 'waited for you 3h 40m', 'tokens out 3.1M', 'tokens in 11M', 'tokens from cache 310M']);
  assert.deepEqual(ROWS.map(([label]) => label), ['agents started', 'prompts', 'turns', 'agent time', 'most at once', 'waited for you', 'tokens out', 'tokens in', 'tokens from cache']);
  assert.equal(grid.at(30, 7)?.ch, 'm');
  for (const text of ['claude', 'codex']) assert.equal(grid.plain().includes(text), false);
});

test('a card from a link shared before v2 shows the five numbers it holds', () => {
  const stats = parse(v1); assert.ok(stats);
  const plain = cardGrid(stats, '7d').plain();
  assert.match(plain, /agent time\s+41h 03m/);
  for (const text of ['waited', 'cache', 'turns']) assert.equal(plain.includes(text), false);
});

test('every heatmap block is drawn, also those before counting began', () => {
  const grid = cardGrid({ ...EXAMPLE, days: 1, levels: [3, 1] }, '7d');
  const squares = Array.from({ length: 7 }, (_, row) => Array.from({ length: 26 }, (_, week) => grid.at(44 + week * 2, 3 + row)?.ch).join('')).join('');
  assert.equal(squares, '■'.repeat(182));
  assert.equal(grid.at(94, 9)?.fg, 141);
  assert.equal(grid.at(94, 8)?.fg, 60);
  assert.equal(grid.at(44, 3)?.fg, 239);
});

test('a heatmap square is a snapped shape with equal sides', () => {
  const grid = new Grid(1, 1); grid.put(0, 0, '■', { fg: 99 });
  const ops = paint(grid, { cw: 10, ch: 20, lw: 1, snap: Math.round });
  assert.equal(ops.length, 1); const square = ops[0]; assert.equal(square.k, 'rect');
  if (square.k === 'rect') { assert.equal(square.w, square.h); assert.equal(square.x, 1); assert.equal(square.y, 6); }
});

test('the first counting day shares its current heatmap square', () => {
  const stats = { ...EXAMPLE, days: 0, levels: [1] };
  const link = shareLink(stats);
  assert.ok(link.endsWith('&days=0&map=IA'));
  assert.deepEqual(parse(new URL(link).hash)?.levels, [1]);
});

test('the legend ends at the right edge of the heatmap', () => {
  const grid = cardGrid(EXAMPLE, '7d');
  assert.equal(grid.line(11).slice(77, 96), 'less · ■ ■ ■ ■ more');
  assert.equal(grid.line(11).slice(1, 77).trim(), '');
});
