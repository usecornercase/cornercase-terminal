import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';

import { fromFragment } from '../src/lib/stats/controller';
import { EXAMPLE, ROWS, parse, shareLink } from '../src/lib/stats/model';
import { type Pose, REST, STAGE, TICKET, aimed, css, multiply, project, rotateX, rotateY, ticketMatrix, translate } from '../src/lib/stats/pose';
import { FOILS, GLARE, SWEEP, foilAt, foilBox, foilCss, glareAt, phase } from '../src/lib/stats/sheen';
import { block, face } from '../src/lib/stats/ticket';
import { Grid } from '../src/lib/term/grid';
import { paint } from '../src/lib/term/paint';

const fixture = JSON.parse(readFileSync('../tests/fixtures/stats/share-v2.txt', 'utf8'));
const fragment = new URL(fixture.link).hash;
const older = JSON.parse(readFileSync('../tests/fixtures/stats/share-v1.txt', 'utf8'));
const v1 = new URL(older.link).hash;
const merged = JSON.parse(readFileSync('../tests/fixtures/stats/share-v3.txt', 'utf8'));
const v3 = new URL(merged.link).hash;

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
  ['v3 with v2 numbers', v3.replace('#v3', '#v2')],
  ['v1 with v2 numbers', fragment.replace('#v2', '#v1')],
  ['v2 with v1 numbers', v1.replace('#v1', '#v2')],
] as const) {
  test(`a ${name} link gives no card to draw`, () => { assert.equal(parse(hash), null); assert.equal(fromFragment(hash), null); });
}

const cells = (stats: NonNullable<ReturnType<typeof parse>>, window: '24h' | '7d' | 'all') => face(stats, window).lines.flat().map(([label, value]) => `${label} ${value}`);

test('the example opens on seven days and switching changes the ticket', () => {
  const card = fromFragment('');
  assert.ok(card); assert.equal(card.example, true); assert.equal(card.window, '7d');
  assert.deepEqual(card.face().lines[0][0], ['agents started', '31']);
  card.select('24h'); assert.deepEqual(card.face().lines[0][0], ['agents started', '4']);
  card.select('all'); assert.deepEqual(card.face().lines[0][0], ['agents started', '212']);
  assert.equal(card.face().example, true);
  assert.equal(fromFragment(fragment)?.example, false);
});

test('the ticket shows the rows of the Stats tab in its order, five to a line', () => {
  const stats = parse(fragment); assert.ok(stats);
  const shown = face(stats, '7d');
  assert.deepEqual(shown.lines.map((line) => line.length), [5, 4]);
  assert.deepEqual(cells(stats, '7d'), ['agents started 31', 'prompts 402', 'turns 377', 'agent time 41h 03m', 'most at once 7', 'waited for you 3h 40m', 'tokens out 3.1M', 'tokens in 11M', 'tokens from cache 310M']);
  assert.deepEqual(ROWS.map(([label]) => label), ['agents started', 'prompts', 'turns', 'agent time', 'most at once', 'waited for you', 'tokens out', 'tokens in', 'tokens from cache', 'merged PRs']);
  assert.deepEqual([shown.hero, shown.parallel, shown.work, shown.busy, shown.window], ['3.4×', true, '41h 03m', '12h 10m', 'last 7 days']);
  assert.equal(shown.days, 'counting for 110 days');
});

test('a v3 link carries the merged pull requests and round trips', () => {
  const stats = parse(v3); assert.ok(stats);
  assert.deepEqual([stats.windows['24h'][10], stats.windows['7d'][10], stats.windows.all[10]], [merged.merged['24h'], merged.merged['7d'], merged.merged.all]);
  assert.equal(shareLink(stats), merged.link);
  assert.ok(cells(stats, 'all').includes('merged PRs 61'));
  assert.deepEqual(face(stats, 'all').lines.map((line) => line.length), [5, 5]);
  assert.equal(cells(parse(fragment)!, 'all').some((cell) => cell.startsWith('merged PRs')), false);
});

test('a ticket from a link shared before v2 shows the five numbers it holds', () => {
  const stats = parse(v1); assert.ok(stats);
  const shown = cells(stats, '7d');
  assert.equal(shown.length, 5);
  assert.ok(shown.includes('agent time 41h 03m'));
  for (const text of ['waited', 'cache', 'turns']) assert.equal(shown.some((cell) => cell.includes(text)), false);
});

test('a window without an hour of busy time says there was no agent work', () => {
  const quiet = face({ ...EXAMPLE, windows: { ...EXAMPLE.windows, '24h': [1, 2, 30, 59, 1, 100, 1, 0, 10, 0] } }, '24h');
  assert.equal(quiet.parallel, false);
  assert.equal(quiet.hero, 'no agent work');
});

test('every heatmap block is on the ticket, the newest at the bottom right of the stub', () => {
  const shown = face({ ...EXAMPLE, days: 1, levels: [3, 1] }, '7d');
  assert.equal(shown.levels.length, 182);
  assert.deepEqual(shown.levels.slice(0, 3), [3, 1, 0]);
  assert.equal(block(25, 6), 0);
  assert.equal(block(25, 5), 1);
  assert.equal(block(0, 0), 181);
  assert.deepEqual(new Set(Array.from({ length: 26 * 7 }, (_, i) => block(Math.floor(i / 7), i % 7))).size, 182);
});

test('the matrix the page gives CSS is the one the PNG is drawn with', () => {
  const m = ticketMatrix(REST);
  const values = css(m).slice('matrix3d('.length, -1).split(',').map(Number);
  assert.equal(values.length, 16);
  for (let col = 0; col < 4; col++) for (let row = 0; row < 4; row++) assert.ok(Math.abs(values[col * 4 + row] - m[row * 4 + col]) < 1e-6);
  const [x, y] = project(m, TICKET.w / 2, TICKET.h / 2);
  assert.ok(x > 0 && x < STAGE.w && y > 0 && y < STAGE.h);
  const near = Math.hypot(...project(m, 0, 0).map((v, i) => v - project(m, 0, TICKET.h)[i]) as [number, number]);
  const far = Math.hypot(...project(m, TICKET.w, 0).map((v, i) => v - project(m, TICKET.w, TICKET.h)[i]) as [number, number]);
  assert.ok(near > far, 'the hero side of the ticket is the near side at rest');
});

test('rotations follow CSS: rotateY turns the right edge away, rotateX the top edge', () => {
  const away = (m: number[], x: number, y: number) => m[8] * x + m[9] * y + m[11];
  assert.ok(away(rotateY(20), 100, 0) < 0);
  assert.ok(away(rotateX(20), 0, -100) < 0);
  assert.deepEqual(multiply(translate(1, 2), translate(3, 4)), translate(4, 6));
});

test('the pointer tilts the ticket within its limits and moves the light with it', () => {
  const corner = aimed({ x: 5, y: -5 });
  assert.deepEqual([corner.pose.rx, corner.pose.ry], [13, 18]);
  assert.deepEqual(corner.light, { x: 1, y: 0 });
  const middle: Pose = aimed({ x: 0, y: 0 }).pose;
  assert.equal(Math.abs(middle.rx) + Math.abs(middle.ry) + Math.abs(middle.rz), 0);
});

test('the foil and the glare always cover the whole ticket', () => {
  for (const foil of FOILS) {
    const box = foilBox(foil);
    for (let i = 0; i <= 20; i++) for (let j = 0; j <= 20; j++) {
      const light = { x: i / 20, y: j / 20 };
      const [fx, fy] = foilAt(foil, light);
      assert.ok(fx <= 1e-9 && fy <= 1e-9 && fx + box.w >= TICKET.w - 1e-9 && fy + box.h >= TICKET.h - 1e-9, `${foil.key} at ${i},${j}`);
      const [gx, gy] = glareAt(light);
      assert.ok(gx <= 0 && gy <= 0 && gx + GLARE.size >= TICKET.w && gy + GLARE.size >= TICKET.h);
    }
  }
});

test('the foil repeats, so a sweep of one period lands on the same colours', () => {
  for (const foil of FOILS) {
    const step = foil.period / (SWEEP.x * Math.sin(foil.angle * Math.PI / 180));
    const a = phase(foil, { x: 0.2, y: 0.4 }), b = phase(foil, { x: 0.2 + step, y: 0.4 });
    assert.ok(a >= 0 && a < foil.period);
    assert.ok(Math.abs(a - b) < 1e-6 || Math.abs(Math.abs(a - b) - foil.period) < 1e-6);
    assert.match(foilCss(foil), new RegExp(`^repeating-linear-gradient\\(${foil.angle}deg, ${foil.colours[0]} 0px, .* ${foil.colours[0]} ${foil.period}px\\)$`));
  }
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
