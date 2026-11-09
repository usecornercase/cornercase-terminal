import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';

import * as en from '../src/i18n/en';
import { EACH, FIRST, HEIGHT, LAND, LAYERS, lift, startOf, turnOf } from '../src/lib/show/layers';
import { backOut, beat, crossed, damp, easeExpo, easeInOut, easeOut, seeded, span, veil } from '../src/lib/show/math';
import { CHAPTERS, COMPACT, changesWidth } from '../src/lib/show/script';
import { cameraKeys } from '../src/lib/show/shots';
import { PHONE, STILLS, still } from '../src/lib/show/stills';
import { type Box, NAV, type Shot, type View, WIDE, captionWidth, cells, closest, freeArea, lens, place, pose, poseAt, sheetHeight, sizeFor, transformOf, windowOf } from '../src/lib/show/track';

const VIEWS: View[] = [
  { width: 320, height: 568 },
  { width: 360, height: 740 },
  { width: 390, height: 844 },
  { width: 820, height: 1180 },
  { width: 1024, height: 768 },
  { width: 1280, height: 720 },
  { width: 1440, height: 900 },
  { width: 1920, height: 1080 },
  { width: 2560, height: 1440 },
];

const SECTIONS = ['hero', 'features', 'why', ...CHAPTERS];
const fakePlace = (id: string, progress: number): number => SECTIONS.indexOf(id) * 3000 + progress * 3000;

test('the easings start at zero, end at one and never leave their range', () => {
  for (const ease of [easeOut, easeExpo, easeInOut]) {
    assert.equal(ease(0), 0);
    assert.equal(ease(1), 1);
    assert.equal(ease(-3), 0);
    assert.equal(ease(4), 1);
    let before = 0;
    for (let t = 0; t <= 1; t += 0.05) {
      assert.ok(ease(t) >= before - 1e-9);
      before = ease(t);
    }
  }
  assert.deepEqual([backOut(-1), backOut(0), backOut(1), backOut(2)], [0, 0, 1, 1]);
  assert.ok(Math.max(...Array.from({ length: 21 }, (_, i) => backOut(i / 20))) > 1);
  assert.equal(span(5, 5, 5), 1);
  assert.equal(span(4, 5, 5), 0);
  assert.equal(span(3, 4, 2), 0.5);
  assert.deepEqual([veil(0.1, 0.2, 0.3, 0.8, 0.9), veil(0.5, 0.2, 0.3, 0.8, 0.9), veil(0.95, 0.2, 0.3, 0.8, 0.9)], [0, 1, 0]);
  assert.equal(beat(0.5, 0, 1), 0.5);
  assert.deepEqual([crossed(1, 2, 2), crossed(2, 3, 2), crossed(3, 1, 2)], [true, false, false]);
  assert.ok(Math.abs(damp(0, 10, 5, 100) - 10) < 1e-9);
});

test('a seeded generator repeats and stays between zero and one', () => {
  const a = seeded(482);
  const b = seeded(482);
  const c = seeded(483);
  const first = Array.from({ length: 200 }, () => a());
  assert.deepEqual(first, Array.from({ length: 200 }, () => b()));
  assert.notDeepEqual(first.slice(0, 5), Array.from({ length: 5 }, () => c()));
  assert.ok(first.every((v) => v >= 0 && v < 1));
  assert.ok(new Set(first.map((v) => Math.floor(v * 10))).size === 10);
});

test('the terminal takes the phone, tablet and desktop sizes its layouts were made for', () => {
  for (const view of VIEWS) {
    const size = sizeFor(view);
    const [, , width, height] = windowOf(size);
    assert.equal(size.cols < COMPACT, view.width < 700, `${view.width}`);
    if (view.width < 700) {
      assert.equal(size.rows, 34);
      assert.ok(width <= view.width - 20 + 0.01);
    } else {
      const [, , aw, ah] = freeArea('all', view);
      assert.ok(width <= aw + 0.01 && height <= ah + 0.01, `${view.width}×${view.height} fits ${size.cols}×${size.rows}`);
    }
    assert.ok(size.cols >= 36 && size.cols <= 196 && size.rows >= 24 && size.rows <= 48);
  }
  assert.deepEqual(cells({ cols: 100, rows: 30, cell: 8 }, 2, 3, 10, 4), [10 + 16, 34 + 10 + 3 * 8 * 2.08, 80, 4 * 8 * 2.08]);
});

test('a shot fits its focus in the room the caption leaves', () => {
  for (const view of VIEWS) {
    const size = sizeFor(view);
    const whole = windowOf(size);
    for (const free of ['all', 'left', 'right'] as const) {
      const [ax, ay, aw, ah] = freeArea(free, view);
      assert.ok(aw > 0 && ah > 0 && ay >= NAV);
      if (view.width >= WIDE && free !== 'all') assert.ok(Math.abs(aw - (view.width - captionWidth(view) - 28)) < 0.01);
      if (view.width < WIDE) assert.ok(ay + ah <= view.height - sheetHeight(view));
      const focus: Box = [whole[2] * 0.2, whole[3] * 0.1, whole[2] * 0.5, whole[3] * 0.6];
      const p = pose({ focus, free }, view);
      const left = p.x + focus[0] * p.scale;
      const top = p.y + focus[1] * p.scale;
      assert.ok(left >= ax - 0.01 && left + focus[2] * p.scale <= ax + aw + 0.01, `${view.width} ${free} across`);
      assert.ok(top >= ay - 0.01 && top + focus[3] * p.scale <= ay + ah + 0.01, `${view.width} ${free} down`);
      assert.ok(Math.abs(left + (focus[2] * p.scale) / 2 - (ax + aw / 2)) < 0.01);
    }
  }
});

test('the camera rests on its keys and moves between them without a jump', () => {
  const view = { width: 1440, height: 900 };
  const whole = windowOf(sizeFor(view));
  const a: Shot = { focus: whole, free: 'all' };
  const b: Shot = { focus: [0, 0, 400, 300], free: 'right', ry: -6, zoom: 0.9, fade: 0 };
  const keys = place(
    [
      { at: 1000, shot: b },
      { at: 0, shot: a },
    ],
    view,
  );
  assert.deepEqual(poseAt(keys, -50), pose(a, view));
  assert.deepEqual(poseAt(keys, 0), pose(a, view));
  assert.deepEqual(poseAt(keys, 2000), pose(b, view));
  const half = poseAt(keys, 500);
  assert.ok(Math.abs(half.scale - Math.sqrt(pose(a, view).scale * pose(b, view).scale)) < 1e-9);
  assert.equal(half.ry, -3);
  assert.equal(half.fade, 0.5);
  let before = poseAt(keys, 0);
  for (let at = 10; at <= 1000; at += 10) {
    const now = poseAt(keys, at);
    assert.ok(Math.abs(now.x - before.x) < 40 && Math.abs(now.scale - before.scale) < 0.05);
    before = now;
  }
  assert.equal(closest(keys), pose(a, view).scale);
  assert.equal(transformOf(pose(a, view)).includes('rotate'), false);
  assert.equal(lens(view), 'translate3d(720.0px,414.0px,0) perspective(1500px) translate3d(-720.0px,-414.0px,0) ');
  assert.match(transformOf(pose(b, view)), /^translate3d\(.+\) scale\(.+\) translate\(200\.0px,150\.0px\) rotateX\(0\.00deg\) rotateY\(-6\.00deg\) rotateZ\(0\.00deg\) translate\(-200\.0px,-150\.0px\)$/);
});

test('every camera key frames a real part of the window, at every size', () => {
  for (const view of VIEWS) {
    const size = sizeFor(view);
    const [, , width, height] = windowOf(size);
    const keys = cameraKeys(fakePlace, size, view);
    assert.ok(keys.length > 20);
    for (const { at, shot } of keys) {
      assert.ok(Number.isFinite(at));
      const [x, y, w, h] = shot.focus;
      assert.ok(w > 40 && h > 40, `${view.width}: a focus of ${w}×${h}`);
      assert.ok(x >= -30 && y >= -60 && x + w <= width + 30 && y + h <= height + 30, `${view.width}: focus ${shot.focus} outside ${width}×${height}`);
      if (view.width < WIDE) assert.equal(shot.free, 'all');
    }
    const placed = place(keys, view);
    for (let i = 1; i < placed.length; i++) assert.ok(placed[i].at >= placed[i - 1].at);
    assert.ok(closest(placed) <= 2.4);
  }
});

test('the nine layers rise one at a time, in the order of the nine tiles, and land together', () => {
  assert.equal(LAYERS.length, en.page.features.items.length);
  assert.equal(new Set(Object.values(HEIGHT)).size, LAYERS.length);
  assert.ok(HEIGHT.server < 0 && Object.entries(HEIGHT).every(([name, z]) => name === 'server' || z > 0));
  assert.equal(turnOf(0), -1);
  assert.equal(turnOf(FIRST - 0.001), -1);
  LAYERS.forEach((_, i) => {
    assert.equal(turnOf(startOf(i) + EACH / 2), i);
    assert.equal(lift(startOf(i) - 0.001, i), 0);
    assert.ok(lift(startOf(i) + EACH, i) > 0.98);
    assert.ok(lift(LAND[0], i) > 0.98);
    assert.equal(lift(LAND[1], i), 0);
  });
  assert.ok(startOf(LAYERS.length) <= LAND[0]);
  assert.equal(turnOf(0.99), LAYERS.length);
  const features = readFileSync('src/components/landing/Stage.astro', 'utf8');
  for (const name of LAYERS) assert.ok(features.includes(`'${name}'`) || features.includes(`data-layer="${name}"`), name);
});

test('each layer has a picture of its own, on a desktop and on a phone', () => {
  const pictures = new Set<string>();
  for (const name of STILLS) {
    const wide = still(name, 150, 40);
    const text = wide.plain();
    assert.ok(text.trim().length > 200, name);
    assert.ok(!pictures.has(text), `${name} repeats another layer`);
    pictures.add(text);
    const narrow = still(name, 47, 34);
    assert.ok(narrow.plain().trim().length > 100, `${name} on a phone`);
    assert.ok(narrow.cols <= 47 && narrow.rows <= 34);
  }
  assert.deepEqual([still('phone', 150, 40).cols, still('phone', 150, 40).rows], PHONE);
  assert.ok(still('issues', 150, 40).plain().includes('#482  Returns page crashes on empty address'));
  assert.ok(still('changes', 150, 40).plain().includes('src/returns/address.rs'));
  assert.ok(still('files', 150, 40).plain().includes('src/theme.rs'));
  assert.ok(still('flow', 150, 40).plain().includes('orchestrator'));
  assert.ok(still('menu', 47, 34).plain().includes('Projects'));
  for (const name of ['changes', 'files', 'flow'] as const) assert.ok(Math.abs(still(name, 150, 40).cols - changesWidth(150)) <= 1, name);
});
