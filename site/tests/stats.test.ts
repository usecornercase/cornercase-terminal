import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';

import { fromFragment } from '../src/lib/stats/controller';
import { cardGrid } from '../src/lib/stats/card';
import { EXAMPLE, parse, shareLink } from '../src/lib/stats/model';
import { Grid } from '../src/lib/term/grid';
import { paint } from '../src/lib/term/paint';

const fixture = JSON.parse(readFileSync('../tests/fixtures/stats/share-v1.txt', 'utf8'));
const fragment = new URL(fixture.link).hash;

test('the Rust share link is parsed without losing any values', () => {
  const stats = parse(fragment);
  assert.ok(stats);
  assert.deepEqual(stats.windows, { '24h': fixture['24h'], '7d': fixture['7d'], all: fixture.all });
  assert.equal(stats.days, fixture.days);
  assert.deepEqual(stats.levels, fixture.levels);
  assert.equal(shareLink(stats), fixture.link);
});

for (const [name, hash] of [
  ['too long', fragment + 'a'.repeat(512)],
  ['trailing newline', fragment + '\n'],
  ['missing field', fragment.replace(/&7d=[^&]+/, '')],
  ['extra field', fragment + '&name=x'],
  ['wrong map length', fragment.slice(0, -1)],
  ['unsafe integer', fragment.replace('days=110', 'days=9007199254740992')],
  ['markup', fragment.replace('24h=4.', '24h=<img>.')],
  ['invalid level', '#v1&24h=0.0.0.0.0.0&7d=0.0.0.0.0.0&all=0.0.0.0.0.0&days=1&map=4A'],
  ['nonzero spare bits', '#v1&24h=0.0.0.0.0.0&7d=0.0.0.0.0.0&all=0.0.0.0.0.0&days=1&map=AQ'],
  ['wrong version', fragment.replace('#v1', '#v2')],
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

test('the fixture card keeps private counters out of its terminal grid', () => {
  const stats = parse(fragment); assert.ok(stats);
  const grid = cardGrid(stats, '7d');
  assert.equal(grid.line(2).trim(), '│   agents started        31                                                                       │');
  assert.match(grid.plain(), /agent time\s+41h 03m/);
  for (const text of ['waited', 'cache', 'turns', 'claude', 'codex']) assert.equal(grid.plain().includes(text), false);
  assert.equal(cardGrid(EXAMPLE, '7d').at(94, 9)?.ch, '■');
});

test('a heatmap square is a snapped shape with equal sides', () => {
  const grid = new Grid(1, 1); grid.put(0, 0, '■', { fg: 99 });
  const ops = paint(grid, { cw: 10, ch: 20, lw: 1, snap: Math.round });
  assert.equal(ops.length, 1); const square = ops[0]; assert.equal(square.k, 'rect');
  if (square.k === 'rect') { assert.equal(square.w, square.h); assert.equal(square.x, 1); assert.equal(square.y, 6); }
});
