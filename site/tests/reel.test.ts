import assert from 'node:assert/strict';
import { test } from 'node:test';

import { LAST, Reel, Screen, TICK } from '../src/lib/show/reel';
import { CHAPTERS, DWELL, IDLE, LEAD, MOVE, STORY, TIMES, lengthOf, timesOf, type Mark } from '../src/lib/show/script';
import { RUNS, idleAt, storyAt, whereIs } from '../src/lib/show/shots';
import { sizeFor } from '../src/lib/show/track';

const SIZES: [number, number][] = [
  [36, 34],
  [44, 34],
  [47, 34],
  [62, 34],
  [92, 24],
  [110, 28],
  [124, 30],
  [150, 40],
  [189, 48],
  [196, 48],
];

const reels = new Map<string, Reel>();

function whole(cols: number, rows: number): Reel {
  const id = `${cols}x${rows}`;
  let reel = reels.get(id);
  if (!reel) {
    reel = new Reel(cols, rows);
    reel.extend(LAST);
    reels.set(id, reel);
  }
  return reel;
}

function shown(reel: Reel, ms: number): string {
  const screen = new Screen(reel);
  screen.seek(Math.round(ms / TICK));
  return screen.grid.plain();
}

const after = (mark: Mark, ms: number): number => TIMES.marks[mark] + ms;

for (const [cols, rows] of SIZES) {
  test(`the story finds every click and meets every expectation at ${cols}×${rows}`, () => {
    const reel = whole(cols, rows);
    assert.ok(reel.done);
    assert.deepEqual(reel.missed, []);
    assert.deepEqual(reel.failed, []);
  });
}

test('every screen size the page picks runs the whole story', () => {
  const views: [number, number][] = [
    [320, 568],
    [360, 740],
    [390, 844],
    [430, 932],
    [699, 900],
    [700, 900],
    [820, 1180],
    [979, 700],
    [980, 600],
    [1024, 768],
    [1280, 720],
    [1366, 768],
    [1440, 900],
    [1920, 1080],
    [2560, 1440],
    [3440, 1440],
  ];
  for (const [width, height] of views) {
    const { cols, rows } = sizeFor({ width, height });
    const reel = whole(cols, rows);
    assert.deepEqual([reel.missed, reel.failed], [[], []], `${width}×${height} is ${cols}×${rows}`);
  }
});

test('the story is the same every time it is recorded', () => {
  const a = whole(150, 40);
  const b = new Reel(150, 40);
  b.extend(LAST);
  for (let ms = 0; ms <= TIMES.total; ms += 1750) {
    assert.equal(shown(b, ms), shown(a, ms), `at ${ms} ms`);
    assert.deepEqual(b.pointerAt(ms), a.pointerAt(ms), `pointer at ${ms} ms`);
    assert.deepEqual(b.spotAt(ms), a.spotAt(ms), `spot at ${ms} ms`);
  }
});

test('a reel recorded in slices equals one recorded at once', () => {
  const sliced = new Reel(124, 30);
  for (let tick = 0; !sliced.done; tick += 7) sliced.extend(tick);
  const once = whole(124, 30);
  for (let ms = 0; ms <= TIMES.total; ms += 2300) assert.equal(shown(sliced, ms), shown(once, ms), `at ${ms} ms`);
});

test('scrolling back shows what scrolling forward showed', () => {
  const reel = whole(150, 40);
  const forward = new Screen(reel);
  const seen = new Map<number, string>();
  for (let tick = 0; tick <= LAST; tick += 9) {
    forward.seek(tick);
    seen.set(tick, forward.grid.plain());
  }
  const back = new Screen(reel);
  back.seek(LAST);
  for (let tick = LAST - (LAST % 9); tick >= 0; tick -= 9) {
    back.seek(tick);
    assert.equal(back.grid.plain(), seen.get(tick), `tick ${tick}`);
  }
  const jumps = [400, 12, 733, 733, 90, LAST, 0, 512];
  for (const tick of jumps) {
    back.seek(tick);
    assert.equal(back.grid.plain(), shown(reel, tick * TICK), `jump to ${tick}`);
  }
});

test('a screen reports the rows that changed, once', () => {
  const reel = whole(150, 40);
  const screen = new Screen(reel);
  screen.seek(0);
  assert.deepEqual(screen.dirty(), [0, 39]);
  assert.equal(screen.dirty(), null);
  screen.seek(Math.round(after('picked', -200) / TICK));
  const rows = screen.dirty();
  assert.ok(rows && rows[0] <= 6 && rows[1] >= 30, `the issues dialog redraws its rows, got ${rows}`);
  screen.seek(Math.round(after('picked', -200) / TICK));
  assert.equal(screen.dirty(), null);
});

test('the four chapters tell the story of issue #482', () => {
  const reel = whole(150, 40);
  const line = (text: string, has: string) => text.split('\n').find((l) => l.includes(has)) ?? '';
  assert.ok(shown(reel, after('picked', -150)).includes('Issues · web-shop'));
  assert.ok(shown(reel, after('started', 2700)).includes('Do you trust the files in this folder?'));
  const working = shown(reel, after('working', 1400));
  assert.ok(working.includes('> https://github.com/acme/web-shop/issues/482') && working.includes('Churning'));
  assert.ok(!line(shown(reel, after('needs', -200)), 'orders-api (').includes('!'));
  assert.ok(line(shown(reel, after('needs', 1500)), 'orders-api (').includes('!'));
  assert.ok(shown(reel, after('answered', -400)).includes('Do you want to make this edit?'));
  assert.ok(!line(shown(reel, after('finished', 600)), 'orders-api (').includes('!'));
  assert.ok(line(shown(reel, after('finished', 600)), 'feat/gift-cards').includes('⋯'));
  const review = shown(reel, after('review', 1200));
  assert.ok(review.includes('Uncommitted') && review.includes('src/returns/address.rs'));
  assert.ok(shown(reel, after('hunk', 1200)).includes('open   ask agent   copy'));
  const away = shown(reel, after('quit', 700));
  assert.ok(away.startsWith('~ ❯ cornercase') && !away.includes('Projects'));
  assert.ok(shown(reel, after('typing', 760)).includes('~ ❯ cornercase\n~ ❯ cornercase'));
  const back = shown(reel, after('back', 1500));
  assert.ok(back.includes('Projects') && line(back, 'orders-api (').includes('✓'));
});

test('the marks sit inside their chapters, in order', () => {
  assert.deepEqual(timesOf(), TIMES);
  const marks = Object.entries(TIMES.marks).sort((a, b) => a[1] - b[1]);
  assert.deepEqual(
    marks.map(([name]) => name),
    ['issues', 'picked', 'started', 'working', 'needs', 'answered', 'finished', 'fixing', 'review', 'hunk', 'quit', 'away', 'typing', 'back'],
  );
  const inside = (mark: Mark, chapter: (typeof CHAPTERS)[number]) => {
    const [from, to] = TIMES.chapters[chapter];
    assert.ok(TIMES.marks[mark] >= from && TIMES.marks[mark] < to, `${mark} in ${chapter}`);
  };
  for (const mark of ['issues', 'picked', 'started', 'working'] as const) inside(mark, 'start');
  for (const mark of ['needs', 'answered', 'finished'] as const) inside(mark, 'glance');
  for (const mark of ['fixing', 'review', 'hunk'] as const) inside(mark, 'review');
  for (const mark of ['quit', 'away', 'typing', 'back'] as const) inside(mark, 'leave');
  let at = LEAD;
  for (const name of CHAPTERS) {
    assert.equal(TIMES.chapters[name][0], at);
    at += STORY[name].reduce((sum, step) => sum + lengthOf(step), 0);
    assert.equal(TIMES.chapters[name][1], at);
  }
  assert.equal(TIMES.total, at);
  assert.equal(LAST, Math.ceil(at / TICK));
});

test('the scroll maps onto the story and back', () => {
  const at = (now: Partial<Record<(typeof CHAPTERS)[number], number>>) => storyAt((id) => now[id] ?? 0);
  assert.equal(at({}), LEAD);
  assert.equal(at({ start: RUNS.start[0] }), LEAD);
  assert.equal(at({ start: 1 }), TIMES.chapters.glance[0]);
  assert.equal(at({ start: 1, glance: 1, review: 1, leave: 1 }), TIMES.total);
  let before = LEAD;
  for (const name of CHAPTERS) {
    const done = Object.fromEntries(CHAPTERS.slice(0, CHAPTERS.indexOf(name)).map((c) => [c, 1]));
    for (let p = 0; p <= 1.0001; p += 0.05) {
      const ms = at({ ...done, [name]: p });
      assert.ok(ms >= before, `${name} at ${p.toFixed(2)} goes back`);
      before = ms;
    }
  }
  for (const [mark, ms] of Object.entries(TIMES.marks)) {
    const where = whereIs(ms);
    const done = Object.fromEntries(CHAPTERS.slice(0, CHAPTERS.indexOf(where.chapter)).map((c) => [c, 1]));
    assert.ok(Math.abs(at({ ...done, [where.chapter]: where.progress }) - ms) < 1, mark);
  }
});

test('the terminal lives for a few seconds before the story takes over', () => {
  const reel = whole(150, 40);
  const idle = (ms: number) => shown(reel, ms);
  assert.ok(idle(0).includes('Churning… (esc to interrupt · 0s)'));
  assert.ok(idle(3000).includes('Churning… (esc to interrupt · 3s)'));
  assert.ok(!idle(1000).includes('claude finished in blog'));
  assert.ok(idle(3000).includes('✓ claude finished in blog › main'));
  assert.ok(!idle(LEAD).includes('claude finished in blog'));
  assert.equal(reel.pointerAt(LEAD).shown, false);
  assert.equal(TIMES.chapters.start[0], LEAD);
});

test('while nobody scrolls, only the spinner keeps turning', () => {
  const [from, to] = IDLE;
  assert.ok(from > 0 && to < LEAD);
  for (let ms = 0; ms < to; ms += 137) assert.equal(idleAt(ms), ms);
  assert.equal(idleAt(-40), 0);
  let before = idleAt(to);
  for (let ms = to; ms < to + 20000; ms += 16) {
    const at = idleAt(ms);
    assert.ok(at >= from && at <= to, `${at} at ${ms}`);
    assert.ok(Math.abs(at - before) <= 16, `a jump of ${at - before} at ${ms}`);
    before = at;
  }
  assert.equal(idleAt(to + (to - from)), from);
  assert.equal(idleAt(to + 2 * (to - from)), to);
  for (const [cols, rows] of [[150, 40], [47, 34], [110, 28]] as const) {
    const reel = whole(cols, rows);
    const spinner = /[·✢✳✶✻✽] Churning…/;
    const still = (ms: number) => shown(reel, ms).replace(spinner, '* Churning…');
    const frames = new Set<string>();
    const turns = new Set<string>();
    for (let ms = from; ms <= to; ms += TICK) {
      frames.add(still(ms));
      turns.add(shown(reel, ms).match(spinner)?.[0] ?? '');
    }
    assert.equal(frames.size, 1, `${cols}×${rows}: something but the spinner moves`);
    assert.ok(turns.size >= 5, `${cols}×${rows}: the spinner shows ${[...turns]}`);
    assert.ok(!still(from).includes('finished in'), `${cols}×${rows}: a toast would stay for good`);
  }
});

test('the pointer glides to each click, taps there and hides between chapters', () => {
  const reel = whole(150, 40);
  const issues = TIMES.marks.issues;
  assert.equal(reel.pointerAt(issues - 1).shown, false);
  const start = reel.pointerAt(issues);
  const middle = reel.pointerAt(issues + MOVE / 2);
  const end = reel.pointerAt(issues + MOVE);
  assert.ok(start.shown && middle.shown && end.shown);
  assert.ok(Math.hypot(middle.x - start.x, middle.y - start.y) > 2 && Math.hypot(end.x - middle.x, end.y - middle.y) > 2);
  assert.deepEqual([end.x % 1, end.y % 1], [0.5, 0.5]);
  assert.equal(end.tap, -1);
  const tapped = reel.pointerAt(issues + MOVE + DWELL + 100);
  assert.ok(tapped.tap > 0 && tapped.tap < 1);
  assert.deepEqual([tapped.x, tapped.y], [end.x, end.y]);
  assert.equal(reel.pointerAt(after('started', 400)).shown, false);
  assert.equal(reel.pointerAt(after('quit', 400)).shown, false);
});

test('the spotlight covers the columns while the agents report', () => {
  const reel = whole(150, 40);
  assert.equal(reel.spotAt(TIMES.chapters.glance[0] - 1), null);
  const lit = reel.spotAt(after('needs', 500));
  assert.deepEqual(lit, { x: 0, y: 0, w: 32, h: 40 });
  assert.equal(reel.spotAt(after('answered', 0)), null);
});
