import assert from 'node:assert/strict';
import { test } from 'node:test';

import * as en from '../src/i18n/en';
import { EDGES, LAYOUTS, MAX_ROUNDS, NODES, CAST, END_TITLE, ISSUE_KEY, pathD, pointAt, source, target, textWidth, type Box, type EdgeId, type NodeId } from '../src/lib/flowshow/graph';
import { graphSvg } from '../src/lib/flowshow/svg';
import { CAPTIONS, CYCLE, FINAL, INTRO, LINES, PACKETS, RESET, frameAt, type Frame } from '../src/lib/flowshow/timeline';

const sample = (step = 50): Frame[] => Array.from({ length: Math.floor(CYCLE / step) }, (_, i) => frameAt(INTRO + i * step));
const firstTime = (pred: (f: Frame) => boolean): number => {
  for (let i = 0; i < CYCLE; i += 50) if (pred(frameAt(INTRO + i))) return i;
  return -1;
};
const layouts = Object.values(LAYOUTS);
const inside = (a: Box, b: Box, m: number): boolean => a.x < b.x + b.w + m && b.x < a.x + a.w + m && a.y < b.y + b.h + m && b.y < a.y + a.h + m;
const boxes = (l: (typeof layouts)[number]): Record<string, Box> => ({ issue: l.issue, ...l.nodes });

test('the timeline repeats and starts with its intro', () => {
  for (const x of [0, 950, 7400, 10900, 16800, 19400]) {
    const a = frameAt(INTRO + x);
    assert.deepEqual(frameAt(INTRO + CYCLE + x), a);
    assert.deepEqual(frameAt(INTRO + 3 * CYCLE + x), a);
  }
  assert.equal(frameAt(0).intro, true);
  assert.equal(frameAt(INTRO).intro, false);
});

test('the story happens in order', () => {
  const working = (id: NodeId) => firstTime((f) => f.nodes[id] === 'working');
  assert.ok(working('planner') < working('coder'));
  assert.ok(working('coder') < working('reviewer'));
  assert.equal(working('reviewer'), working('security'));

  const finding = firstTime((f) => f.nodes.reviewer === 'finding');
  const loop = firstTime((f) => f.packets.some((p) => p.edge === 'reviewer>coder'));
  assert.ok(finding >= 0 && finding < loop);
  for (const f of sample()) for (const p of f.packets) if (p.tone === 'finding') assert.equal(p.edge, 'reviewer>coder');

  const again = firstTime((f) => f.nodes.coder === 'working' && f.nodes.reviewer === 'finding' && f.lit.includes('reviewer>coder') && !f.hot.includes('reviewer>coder'));
  assert.ok(again > loop);
  const second = firstTime((f) => f.nodes.reviewer === 'working' && f.lit.includes('reviewer>coder') && f.nodes.coder === 'done');
  assert.ok(second > again);

  const bothDone = firstTime((f) => f.nodes.reviewer === 'done' && f.nodes.security === 'done');
  assert.ok(firstTime((f) => f.nodes.end === 'reached') > bothDone);

  const checks = sample().map((f) => f.checks);
  const order = checks.filter((c, i) => i === 0 || c !== checks[i - 1]);
  assert.deepEqual(order, ['none', 'running', 'passed', 'none']);

  const rounds = sample().map((f) => f.rounds);
  assert.deepEqual(rounds.filter((r, i) => i === 0 || r !== rounds[i - 1]), [0, 1, 0]);
  assert.ok(Math.max(...rounds) <= MAX_ROUNDS);

  const end = frameAt(INTRO + CYCLE - 1);
  for (const id of NODES) assert.equal(end.nodes[id], 'idle');
});

test('packets stay few, on real edges, and fork together', () => {
  for (const f of sample(20)) {
    assert.ok(f.packets.length <= PACKETS);
    for (const p of f.packets) {
      assert.ok(p.at >= 0 && p.at <= 1);
      for (const l of layouts) assert.ok(l.edges[p.edge]);
    }
  }
  const fork = frameAt(INTRO + 7200).packets.map((p) => p.edge).sort();
  assert.deepEqual(fork, ['coder>reviewer', 'coder>security']);
  const join = frameAt(INTRO + 15800).packets.map((p) => p.edge).sort();
  assert.deepEqual(join, ['reviewer>end', 'security>end']);
  const travelled = new Set<EdgeId>();
  for (let t = 0; t < RESET; t += 20) {
    const f = frameAt(INTRO + t);
    for (const e of f.hot) travelled.add(e);
    for (const e of f.lit) assert.ok(travelled.has(e));
  }
});

test('captions match the translated texts and the final frame is the finished run', () => {
  assert.deepEqual(new Set(CAPTIONS), new Set(Object.keys(en.page.flows.steps)));
  const seen = sample().map((f) => f.caption).filter((c, i, a) => i === 0 || c !== a[i - 1]);
  assert.deepEqual(seen.slice(0, CAPTIONS.length), CAPTIONS);
  assert.equal(FINAL.caption, 'pr');
  assert.deepEqual(FINAL, frameAt(INTRO + 18000));
  for (const id of ['planner', 'coder', 'reviewer', 'security'] as const) assert.equal(FINAL.nodes[id], 'done');
  assert.equal(FINAL.nodes.orchestrator, 'pending');
  assert.equal(FINAL.nodes.end, 'reached');
  assert.equal(FINAL.checks, 'passed');
  assert.equal(FINAL.rounds, 1);
  assert.deepEqual(FINAL.packets, []);
  assert.equal(FINAL.ticker, '✓ gh pr create');
});

test('the layouts fit their viewBox and do not overlap', () => {
  for (const l of layouts) {
    const all: Record<string, Box> = { ...boxes(l), loop: l.loopLabel };
    for (const [k, b] of Object.entries(all)) {
      assert.ok(b.x >= 4 && b.y >= 4 && b.x + b.w <= l.width - 4 && b.y + b.h <= l.height - 4, `${l.name} ${k}`);
    }
    const names = Object.keys(boxes(l));
    for (const a of names) {
      for (const b of names) if (a < b) assert.ok(!inside(boxes(l)[a], boxes(l)[b], 8), `${l.name} ${a}/${b}`);
      assert.ok(!inside(boxes(l)[a], l.loopLabel, 0), `${l.name} loop/${a}`);
    }
  }
});

test('edges join their boxes and move at an even speed', () => {
  const onBorder = (b: Box, [x, y]: [number, number]): boolean => {
    const near = (a: number, c: number) => Math.abs(a - c) <= 1;
    const within = x >= b.x - 1 && x <= b.x + b.w + 1 && y >= b.y - 1 && y <= b.y + b.h + 1;
    return within && (near(x, b.x) || near(x, b.x + b.w) || near(y, b.y) || near(y, b.y + b.h));
  };
  for (const l of layouts) {
    for (const e of EDGES) {
      const c = l.edges[e];
      const from = source(e) === 'issue' ? l.issue : l.nodes[source(e) as NodeId];
      assert.ok(onBorder(from, c.p0), `${l.name} ${e} start`);
      assert.ok(onBorder(l.nodes[target(e) as NodeId], c.p3), `${l.name} ${e} end`);
      assert.deepEqual(pointAt(c, 0), c.p0);
      assert.deepEqual(pointAt(c, 1), c.p3);
      const lens: number[] = [];
      let prev = pointAt(c, 0);
      for (let i = 1; i <= 20; i++) {
        const p = pointAt(c, i / 20);
        lens.push(Math.hypot(p[0] - prev[0], p[1] - prev[1]));
        prev = p;
      }
      assert.ok(Math.max(...lens) / Math.min(...lens) < 1.15, `${l.name} ${e} even`);
      assert.match(pathD(c), /^M /);
    }
  }
});

test('text fits its boxes', () => {
  for (const l of layouts) {
    const { fonts } = l;
    for (const id of ['orchestrator', 'planner', 'coder', 'reviewer', 'security'] as const) {
      const b = l.nodes[id];
      assert.ok(textWidth(id, fonts.role) <= b.w - 32, `${l.name} ${id}`);
      assert.ok(textWidth(CAST[id].agent, fonts.agent) <= b.w - 32, `${l.name} ${id} agent`);
    }
    const inset = l.nodes.orchestrator.w - 24 - 16;
    for (const [, line] of LINES) assert.ok(textWidth(`› ${line}▍`, fonts.ticker) <= inset, `${l.name} ${line}`);
    for (let r = 0; r <= MAX_ROUNDS; r++) assert.ok(textWidth(l.loopText(r), fonts.loop) <= l.loopLabel.w - 12, `${l.name} loop ${r}`);
    assert.ok(textWidth(ISSUE_KEY + l.issueText, fonts.issue) <= l.issue.w - 32, `${l.name} issue`);
    assert.ok(textWidth(END_TITLE, fonts.endTitle) + 42 <= l.nodes.end.w - 8, `${l.name} end`);
  }
});

test('the markup is complete, unique and starts as the finished run', () => {
  const svgs = { wide: graphSvg('wide'), tall: graphSvg('tall') };
  const ids: string[] = [];
  for (const [name, svg] of Object.entries(svgs)) {
    for (const id of NODES) assert.ok(svg.includes(`data-node="${id}"`), `${name} ${id}`);
    for (const e of EDGES) {
      assert.ok(svg.includes(`data-edge="${e}"`) && svg.includes(`data-head="${e}"`), `${name} ${e}`);
    }
    assert.equal(svg.match(/data-packet="/g)?.length, PACKETS);
    assert.equal(svg.match(/data-trail="/g)?.length, PACKETS);
    const own = [...svg.matchAll(/ id="([^"]+)"/g)].map((m) => m[1]);
    ids.push(...own);
    for (const id of own) assert.ok(id.startsWith(`fs-${name}-`), id);
    for (const [, ref] of svg.matchAll(/url\(#([^)]+)\)/g)) assert.ok(own.includes(ref), ref);
    assert.ok(!/style=/.test(svg));
    assert.match(svg, /^<svg[^>]*aria-hidden="true"/);
    for (const id of NODES) {
      const m = svg.match(new RegExp(`data-node="${id}" data-state="(\\w+)"`));
      assert.equal(m?.[1], FINAL.nodes[id]);
    }
    for (const [, tag] of svg.matchAll(/<(\w+)[^>]*pathLength/g)) assert.equal(tag, 'path');
  }
  assert.equal(new Set(ids).size, ids.length);
});
