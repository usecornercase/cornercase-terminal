import { BOLD, type Rect, type Style, contains, rect } from '../term/grid';
import { drawPanelBorder } from './changes';
import { type Areas, bottom, right } from './layout';
import type { Status } from './model';
import type { Painter } from './ui';
import { truncateRight } from './text';

export const TRAVEL = 1000;
export const LABEL = 'Flow';
const ORCHESTRATOR = 'orchestrator';
const ROLE_COLOURS = [12, 2, 99, 214, 205, 6];
const BOX_HEIGHT = 3;
const MIN_PREFERRED = 20;
const MAX_PREFERRED = 34;
const MIN_SIDE = 14;
const GAP_X = 2;
const HEADER = 3;
const N = 1;
const S = 2;
const E = 4;
const W = 8;
const BRAND = 99;
const WAITING = 208;
const DARK: Style = { fg: 8 };
const OWN_LONG = ' own worktree ';
const OWN_SHORT = ' own ';

export type Edge = { kind: 'forward'; stage: number } | { kind: 'loop'; stage: number } | { kind: 'outside'; from: number | null; to: number };

export const edgeKey = (e: Edge): string => (e.kind === 'outside' ? `outside:${e.from}:${e.to}` : `${e.kind}:${e.stage}`);
const sentKey = (edge: string, to: string): string => `${edge}|${to}`;

export interface FlowRole {
  name: string;
  agent: string;
  model?: string;
  own?: boolean;
}

export interface FlowStage {
  roles: string[];
  back?: { to: number; when: string; atMost: number };
}

export interface Flow {
  name: string;
  orchestrator: FlowRole;
  roles: FlowRole[];
  stages: FlowStage[];
  end: string;
}

export interface Message {
  from: string;
  to: string;
  text: string;
  at: number;
}

export interface Step {
  edge: Edge;
  from: string | null;
  to: string;
  at: number;
}

const stageOf = (flow: Flow, role: string): number => flow.stages.findIndex((s) => s.roles.includes(role));

export class Progress {
  active: number | null = null;
  holder: string | null = null;
  last: Step | null = null;
  outside: Edge | null = null;
  private entry: [Edge, string | null] | null = null;
  private counted: number[] = [];
  private along = new Map<string, Message>();

  rounds(stage: number): number {
    return this.counted[stage] ?? 0;
  }

  message(edge: Edge, to: string | null = null): Message | undefined {
    const key = `${edgeKey(edge)}|`;
    return [...this.along].reverse().find(([k, m]) => k.startsWith(key) && (to === null || m.to === to))?.[1];
  }

  sent(): string[] {
    return [...this.along.keys()];
  }

  travelled(now: number): number | null {
    if (!this.last || now < this.last.at || now - this.last.at >= TRAVEL) return null;
    return (now - this.last.at) / TRAVEL;
  }

  send(flow: Flow, m: Message): void {
    const target = stageOf(flow, m.to);
    if (target < 0) return;
    const active = this.active;
    let moved: [Edge, string | null] | null;
    if (active === target) moved = this.holder === m.to ? null : this.entry;
    else if (active !== null && target === active + 1) moved = [{ kind: 'forward', stage: active }, this.holder];
    else if (active !== null && flow.stages[active]?.back?.to === target + 1) {
      this.counted[active] = this.rounds(active) + 1;
      moved = [{ kind: 'loop', stage: active }, this.holder];
    } else if (active === null && target === 0) moved = null;
    else moved = [{ kind: 'outside', from: active, to: target }, this.holder];
    if (active !== target) this.entry = moved;
    if (moved) {
      const [edge, from] = moved;
      if (edge.kind === 'outside') this.outside = edge;
      const key = sentKey(edgeKey(edge), m.to);
      this.along.delete(key);
      this.along.set(key, m);
      this.last = { edge, from, to: m.to, at: m.at };
    }
    this.active = target;
    this.holder = m.to;
  }
}

export type NodeState = Status | 'closed' | 'none';

interface Node {
  role: string;
  text: string;
  colour: number;
  state: NodeState | null;
  own: boolean;
}

interface Loop {
  to: number;
  when: string;
  atMost: number;
  rounds: number;
}

interface Lit {
  edge: Edge;
  from: number | null;
  to: number | null;
  colour: number;
  dot: number | null;
}

export interface Diagram {
  orchestrator: Node;
  stages: Node[][];
  loops: (Loop | null)[];
  end: string;
  live: { lit: Lit | null; outside: Edge | null; ended: boolean; messages: string[] } | null;
}

export const roleColour = (i: number): number => ROLE_COLOURS[i % ROLE_COLOURS.length];

const describe = (r: FlowRole): string => (r.model ? `${r.agent} · ${r.model}` : r.agent);

export function plan(flow: Flow): Diagram {
  const colour = (name: string) => roleColour(flow.roles.findIndex((r) => r.name === name) + 1);
  const node = (r: FlowRole, c: number): Node => ({ role: r.name, text: describe(r), colour: c, state: null, own: r.own === true });
  return {
    orchestrator: node(flow.orchestrator, roleColour(0)),
    stages: flow.stages.map((s) => s.roles.map((name) => node(flow.roles.find((r) => r.name === name) ?? { name, agent: 'not a role' }, colour(name)))),
    loops: flow.stages.map((s, n) => (s.back && s.back.to >= 1 && s.back.to - 1 < n ? { to: s.back.to - 1, when: s.back.when, atMost: s.back.atMost, rounds: 0 } : null)),
    end: flow.end,
    live: null,
  };
}

export function live(d: Diagram, progress: Progress, state: (role: string) => NodeState, now: number): Diagram {
  d.orchestrator.state = state(ORCHESTRATOR);
  for (const n of d.stages.flat()) n.state = state(n.role);
  d.loops.forEach((l, stage) => {
    if (l) l.rounds = progress.rounds(stage);
  });
  const step = progress.last;
  let lit: Lit | null = null;
  if (step) {
    const target = step.edge.kind === 'forward' ? step.edge.stage + 1 : step.edge.kind === 'loop' ? (d.loops[step.edge.stage]?.to ?? 0) : step.edge.to;
    const nodes = [...d.stages.flat(), d.orchestrator];
    const colour = nodes.find((n) => n.role === step.from)?.colour ?? d.orchestrator.colour;
    const from = step.edge.kind === 'forward' && step.from ? d.stages[step.edge.stage].findIndex((n) => n.role === step.from) : -1;
    const to = d.stages[target]?.findIndex((n) => n.role === step.to) ?? -1;
    lit = { edge: step.edge, from: from < 0 ? null : from, to: to < 0 ? null : to, colour, dot: progress.travelled(now) };
  }
  const edges: Edge[] = [
    ...d.stages.slice(1).map((_, stage): Edge => ({ kind: 'forward', stage })),
    ...d.loops.flatMap((l, stage): Edge[] => (l ? [{ kind: 'loop', stage }] : [])),
    ...(progress.outside ? [progress.outside] : []),
  ];
  const keys = new Set(edges.map(edgeKey));
  const messages = progress.sent().filter((key) => keys.has(key.split('|')[0]));
  d.live = { lit, outside: progress.outside, ended: d.orchestrator.state === 'pending', messages };
  return d;
}

type At = [number, number];

interface Cell {
  dirs: number;
  arrow: string | null;
  edges: string[];
  dashed: boolean;
}

interface Placed {
  node: Node;
  rect: Rect;
  exit: number | null;
}

interface Gap {
  merges: At[][];
  middle: At[];
  entries: At[][];
}

interface Geometry {
  height: number;
  boxes: Placed[];
  cells: Map<string, Cell & { at: At }>;
  labels: { stage: number; at: At; text: string }[];
  gaps: Gap[];
  loops: Map<number, At[]>;
  outside: At[];
  end: At;
  below: At | null;
}

type Shape = { kind: 'row' | 'stack'; width: number };

const len = (text: string): number => [...text].length;

function labels(l: Loop, isLive: boolean): string[] {
  return isLive
    ? [`${l.when} · ${l.rounds}/${l.atMost}`, `${l.when} ${l.rounds}/${l.atMost}`, `${l.rounds}/${l.atMost}`]
    : [`${l.when} · at most ${l.atMost}`, `${l.when} · ≤${l.atMost}`, l.when, `≤${l.atMost}`];
}

function preferred(d: Diagram): number {
  const widest = Math.max(...[d.orchestrator, ...d.stages.flat()].map((n) => Math.max(len(n.role) + (n.own ? len(OWN_SHORT) + 5 : 6), len(n.text) + 6)));
  return Math.min(MAX_PREFERRED, Math.max(MIN_PREFERRED, widest));
}

const rowWidth = (k: number, width: number): number => width * Math.max(1, k) + GAP_X * (Math.max(1, k) - 1);

function shapeOf(k: number, room: number, pref: number): Shape {
  if (k <= 1) return { kind: 'row', width: Math.min(pref, room) };
  const each = Math.floor((room - GAP_X * (k - 1)) / k);
  if (each >= MIN_SIDE) return { kind: 'row', width: Math.min(each, pref) };
  return { kind: 'stack', width: Math.min(pref, room - 2, 2 * Math.max(0, Math.floor(room / 2) - 2)) };
}

function naturalWidth(d: Diagram): number {
  const pref = preferred(d);
  const boxes = Math.max(pref, ...d.stages.map((s) => rowWidth(s.length, pref)));
  const lbl = Math.max(0, ...d.loops.flatMap((l) => (l ? [2 * (len(labels(l, !!d.live)[0]) + 3)] : [])));
  const end = 2 * (len(d.end) + 2);
  const lanes = 2 * d.loops.filter(Boolean).length;
  return Math.max(boxes, lbl, end) + lanes + (d.live?.outside ? 2 : 0);
}

const fit = (d: Diagram, width: number): number => Math.min(naturalWidth(d), width);

class Builder {
  g: Geometry = { height: 0, boxes: [], cells: new Map(), labels: [], gaps: [], loops: new Map(), outside: [], end: [0, 0], below: null };
  y = 0;

  constructor(
    readonly left: number,
    readonly room: number,
    readonly cx: number,
    readonly pref: number,
  ) {}

  connect([x, y]: At, dirs: number, edge: string | null, dashed = false): void {
    const key = `${x},${y}`;
    const cell = this.g.cells.get(key) ?? { dirs: 0, arrow: null, edges: [], dashed: false, at: [x, y] as At };
    cell.dirs |= dirs;
    cell.dashed ||= dashed;
    if (edge && !cell.edges.includes(edge)) cell.edges.push(edge);
    this.g.cells.set(key, cell);
  }

  arrow(at: At, glyph: string, edge: string | null, dashed = false): void {
    this.connect(at, 0, edge, dashed);
    const cell = this.g.cells.get(`${at[0]},${at[1]}`);
    if (cell) cell.arrow = glyph;
  }

  across(y: number, from: number, to: number, edge: string | null, dashed = false): At[] {
    const xs = from <= to ? range(from, to) : range(to, from).reverse();
    const [lo, hi] = [Math.min(from, to), Math.max(from, to)];
    for (const x of xs) this.connect([x, y], (x > lo ? W : 0) | (x < hi ? E : 0), edge, dashed);
    return xs.map((x): At => [x, y]);
  }

  down(x: number, from: number, to: number, edge: string | null, dashed = false): At[] {
    const ys = from <= to ? range(from, to) : range(to, from).reverse();
    const [lo, hi] = [Math.min(from, to), Math.max(from, to)];
    for (const y of ys) this.connect([x, y], (y > lo ? N : 0) | (y < hi ? S : 0), edge, dashed);
    return ys.map((y): At => [x, y]);
  }

  centred(width: number): number {
    return Math.max(this.left, this.cx - Math.floor(width / 2));
  }

  row(k: number, width: number): number[] {
    const x0 = this.centred(rowWidth(k, width));
    return Array.from({ length: Math.max(1, k) }, (_, i) => x0 + i * (width + GAP_X));
  }

  place(node: Node, x: number, width: number): Rect {
    const r = rect(x, this.y, Math.max(1, width), BOX_HEIGHT);
    this.g.boxes.push({ node, rect: r, exit: null });
    return r;
  }

  stage(nodes: Node[], shape: Shape): Rect[] {
    if (shape.kind === 'row') {
      const rects = this.row(nodes.length, shape.width).map((x, i) => this.place(nodes[i], x, shape.width));
      this.y += BOX_HEIGHT;
      return rects;
    }
    const x = Math.max(this.left + 2, this.centred(shape.width));
    return nodes.map((n) => {
      const r = this.place(n, x, shape.width);
      this.y += BOX_HEIGHT;
      return r;
    });
  }

  exits(shape: Shape, rects: Rect[]): number[] {
    const first = this.g.boxes.length - rects.length;
    const exiting = shape.kind === 'row' ? rects.map((_, i) => i) : [rects.length - 1];
    return exiting.map((i) => {
      const x = rects[i].x + Math.floor(rects[i].w / 2);
      this.g.boxes[first + i].exit = x;
      return x;
    });
  }

  merge(exits: number[], edge: string | null): At[][] {
    if (exits.length === 1 && exits[0] === this.cx) return [[]];
    const paths = exits.map((x) => this.across(this.y, x, this.cx, edge));
    for (const x of exits) this.connect([x, this.y], N, edge);
    this.connect([this.cx, this.y], S, edge);
    this.y += 1;
    return paths;
  }

  middle(edge: string | null): At[] {
    const at: At = [this.cx, this.y];
    this.connect(at, N | S, edge);
    this.y += 1;
    return [at];
  }

  entries(shape: Shape, k: number, edge: string | null): At[][] {
    const y = this.y;
    if (shape.kind === 'row') {
      const centres = this.row(k, shape.width).map((x) => x + Math.floor(shape.width / 2));
      if (centres.length === 1 && centres[0] === this.cx) {
        this.arrow([this.cx, y], '▼', edge);
        this.y += 1;
        return [[[this.cx, y]]];
      }
      const paths = centres.map((x) => {
        const path = this.across(y, this.cx, x, edge);
        this.connect([x, y], S, edge);
        this.arrow([x, y + 1], '▼', edge);
        return [...path, [x, y + 1] as At];
      });
      this.connect([this.cx, y], N, edge);
      this.y += 2;
      return paths;
    }
    const bx = Math.max(this.left + 2, this.centred(shape.width)) - 2;
    const turn = this.across(y, this.cx, bx, edge);
    this.connect([this.cx, y], N, edge);
    this.y += 1;
    const top = this.y;
    const last = top + BOX_HEIGHT * (Math.max(1, k) - 1) + 1;
    const line = this.down(bx, y, last, edge);
    return Array.from({ length: k }, (_, j) => {
      const row = top + BOX_HEIGHT * j + 1;
      this.connect([bx, row], E, edge);
      this.arrow([bx + 1, row], '▶', edge);
      return [...turn, ...line.slice(1).filter(([, yy]) => yy <= row), [bx + 1, row] as At];
    });
  }
}

function range(a: number, b: number): number[] {
  return Array.from({ length: b - a + 1 }, (_, i) => a + i);
}

function layout(d: Diagram, width: number): Geometry {
  const left = d.live?.outside ? 2 : 0;
  const lanes = d.loops
    .flatMap((l, stage) => (l && l.to < stage ? [{ stage, to: l.to, x: 0 }] : []))
    .sort((a, b) => a.stage - a.to - (b.stage - b.to) || a.stage - b.stage);
  const room = Math.max(1, width - left - 2 * lanes.length);
  lanes.forEach((lane, k) => (lane.x = left + room + 1 + 2 * k));
  const pref = Math.min(preferred(d), room);
  const b = new Builder(left, room, left + Math.floor(room / 2), pref);
  b.place(d.orchestrator, b.centred(pref), pref);
  b.y += BOX_HEIGHT + 1;
  const shapes: [Shape, Rect[]][] = [];
  d.stages.forEach((nodes, stage) => {
    const shape = shapeOf(nodes.length, room, pref);
    const previous = shapes[stage - 1];
    if (previous) {
      const edge = edgeKey({ kind: 'forward', stage: stage - 1 });
      const merges = b.merge(b.exits(previous[0], previous[1]), edge);
      const middle = b.middle(edge);
      const entries = b.entries(shape, nodes.length, edge);
      b.g.gaps.push({ merges, middle, entries });
    }
    shapes.push([shape, b.stage(nodes, shape)]);
  });
  const lastShape = shapes[shapes.length - 1];
  if (lastShape) {
    b.merge(b.exits(lastShape[0], lastShape[1]), null);
    b.arrow([b.cx, b.y], '▼', null);
    b.y += 1;
  }
  b.g.end = [b.cx, b.y];
  b.y += 1;
  if (b.cx + 2 + len(d.end) > width) {
    b.g.below = [Math.max(left, b.cx - Math.floor(len(d.end) / 2)), b.y];
    b.y += 1;
  }
  const port = (stage: number, incoming: boolean): Rect | undefined => {
    const s = shapes[stage];
    if (!s) return undefined;
    return s[0].kind === 'stack' && incoming ? s[1][0] : s[1][s[1].length - 1];
  };
  const targets = new Map<number, number>();
  const entering = new Map<number, number[]>();
  lanes.forEach((lane, i) => entering.set(lane.to, [...(entering.get(lane.to) ?? []), i]));
  for (const [to, ins] of entering) {
    const into = port(to, true);
    if (!into) continue;
    const shared = lanes.some((l) => l.stage === to) && port(to, false) === into;
    const free = (shared ? [2, 0] : [1, 0, 2]).slice(0, ins.length).sort((a, c) => a - c);
    [...ins].sort((a, c) => lanes[c].x - lanes[a].x).forEach((i, n) => targets.set(i, into.y + (free[n] ?? free[free.length - 1] ?? 1)));
  }
  lanes.forEach((lane, i) => {
    const from = port(lane.stage, false);
    const into = port(lane.to, true);
    if (!from || !into) return;
    const edge = edgeKey({ kind: 'loop', stage: lane.stage });
    const ys = from.y + 1;
    const yt = targets.get(i) ?? into.y + 1;
    const path = b.across(ys, right(from), lane.x, edge);
    path.push(...b.down(lane.x, ys, yt, edge).slice(1));
    path.push(...b.across(yt, lane.x, right(into), edge).slice(1));
    b.arrow([right(into), yt], '◀', edge);
    b.g.loops.set(lane.stage, path);
  });
  const slots = lanes.flatMap((lane) => {
    const y = b.g.gaps[lane.stage - 1]?.middle[0]?.[1];
    const l = d.loops[lane.stage];
    if (y === undefined || !l) return [];
    const crossing = lanes.filter((o) => o.x < lane.x && (b.g.cells.get(`${o.x},${y}`)?.dirs ?? 0) & N && (b.g.cells.get(`${o.x},${y}`)?.dirs ?? 0) & S);
    const end = Math.min(lane.x, ...crossing.map((o) => o.x)) - 1;
    return [{ stage: lane.stage, y, end, room: end - (b.cx + 2), texts: labels(l, !!d.live) }];
  });
  const shared = [0, 1, 2, 3].find((form) => slots.every((s) => s.texts[form] !== undefined && len(s.texts[form]) <= s.room));
  for (const s of slots) {
    const text = (shared !== undefined ? s.texts[shared] : undefined) ?? s.texts.find((t) => len(t) <= s.room);
    if (text) b.g.labels.push({ stage: s.stage, at: [s.end - len(text), s.y], text });
  }
  const out = d.live?.outside;
  if (out?.kind === 'outside') {
    const source = out.from === null ? b.g.boxes[0]?.rect : shapes[out.from]?.[1][0];
    const target = shapes[out.to]?.[1][0];
    if (source && target) {
      const edge = edgeKey(out);
      const [ys, yt] = [source.y + 1, target.y + 1];
      const path = b.across(ys, source.x - 1, 0, edge, true);
      path.push(...b.down(0, ys, yt, edge, true).slice(1));
      path.push(...b.across(yt, 0, target.x - 1, edge, true).slice(1));
      b.arrow([target.x - 1, yt], '▶', edge, true);
      b.g.outside = path;
    }
  }
  b.g.height = b.y;
  return b.g;
}

function glyph(dirs: number, dashed: boolean): string {
  if (dirs === N || dirs === S || dirs === (N | S)) return dashed ? '┆' : '│';
  if (dirs === E || dirs === W || dirs === (E | W)) return dashed ? '┄' : '─';
  const glyphs: Record<number, string> = {
    [S | E]: '╭',
    [S | W]: '╮',
    [N | E]: '╰',
    [N | W]: '╯',
    [N | S | E]: '├',
    [N | S | W]: '┤',
    [S | E | W]: '┬',
    [N | E | W]: '┴',
    [N | S | E | W]: '┼',
  };
  return glyphs[dirs] ?? ' ';
}

function path(g: Geometry, lit: Lit): At[] {
  const e = lit.edge;
  if (e.kind === 'loop') return g.loops.get(e.stage) ?? [];
  if (e.kind === 'outside') return g.outside;
  const gap = g.gaps[e.stage];
  if (!gap) return [];
  return [...(lit.from !== null ? (gap.merges[lit.from] ?? []) : []), ...gap.middle, ...(gap.entries[lit.to ?? 0] ?? [])];
}

export function diagramHeight(d: Diagram, width: number): number {
  return layout(d, fit(d, width)).height;
}

export interface DiagramHit {
  role?: string;
  edge?: string;
  to?: string | null;
  file?: number;
}

export function drawDiagram(p: Painter, area: Rect, d: Diagram, scroll: number, click: (hit: DiagramHit) => void): void {
  const width = fit(d, area.w);
  const ox = area.x + Math.floor((area.w - width) / 2);
  const g = layout(d, width);
  const spot = ([x, y]: At): At | null => {
    const sy = y - scroll;
    return x < width && sy >= 0 && sy < area.h ? [ox + x, area.y + sy] : null;
  };
  const put = (at: At, text: string, s: Style) => {
    let x = at[0];
    for (const ch of text) {
      const where = spot([x, at[1]]);
      if (where) p.g.put(where[0], where[1], ch, s);
      x += 1;
    }
  };
  const live = d.live;
  const clickable = new Set(live?.messages ?? []);
  const hover = p.app.hover;
  const local: At | null = hover && contains(area, hover.x, hover.y) ? [hover.x - ox, hover.y - area.y + scroll] : null;
  const onLabel = (at: At) => g.labels.find((l) => l.at[1] === at[1] && at[0] >= l.at[0] && at[0] < l.at[0] + len(l.text));
  const branchAt = (edge: string, at: At): number | null => {
    const [kind, stage] = edge.split(':');
    const entries = kind === 'forward' ? (g.gaps[Number(stage)]?.entries ?? []) : [];
    const on = entries.flatMap((path, j) => (path.some(([x, y]) => x === at[0] && y === at[1]) ? [j] : []));
    return on.length === 1 ? on[0] : null;
  };
  const roleOf = (edge: string, branch: number | null): string | null =>
    branch === null ? null : (d.stages[Number(edge.split(':')[1]) + 1]?.[branch]?.role ?? null);
  const sent = (edge: string, to: string | null) => [...clickable].some((k) => k.startsWith(`${edge}|`) && (to === null || k === `${edge}|${to}`));
  const arrowAt = (at: At | null): { edge: string; branch: number | null } | undefined => {
    if (!at) return undefined;
    const cell = g.cells.get(`${at[0]},${at[1]}`);
    const label = onLabel(at);
    const onCell = (cell?.edges ?? []).map((edge) => ({ edge, branch: branchAt(edge, at) }));
    const onText = label ? [{ edge: edgeKey({ kind: 'loop', stage: label.stage }), branch: null }] : [];
    return [...onCell, ...onText].find((a) => sent(a.edge, roleOf(a.edge, a.branch)));
  };
  const hovered = arrowAt(local);
  const way = (() => {
    if (!hovered || hovered.branch === null) return null;
    const gap = g.gaps[Number(hovered.edge.split(':')[1])];
    const cells = gap ? [...gap.merges.flat(), ...gap.middle, ...(gap.entries[hovered.branch] ?? [])] : [];
    return new Set(cells.map(([x, y]) => `${x},${y}`));
  })();
  const litKey = live?.lit ? edgeKey(live.lit.edge) : null;
  const lineColour = p.lineColour;
  const colour = (edges: string[], at: At | null = null): Style => {
    if (hovered && edges.includes(hovered.edge) && (!way || !at || way.has(`${at[0]},${at[1]}`))) return { fg: 6, add: BOLD };
    if (litKey && live?.lit && edges.includes(litKey)) return { fg: live.lit.colour };
    return { fg: lineColour };
  };
  for (const cell of g.cells.values()) {
    put(cell.at, cell.arrow ?? glyph(cell.dirs, cell.dashed), colour(cell.edges, cell.at));
    const where = spot(cell.at);
    const arrow = arrowAt(cell.at);
    const hit = arrow && { edge: arrow.edge, to: roleOf(arrow.edge, arrow.branch) };
    if (where && hit) p.region({ r: rect(where[0], where[1], 1, 1), click: () => click(hit), cursor: 'pointer' });
  }
  for (const l of g.labels) {
    const loop = d.loops[l.stage];
    const style = live && loop && loop.rounds > loop.atMost ? { fg: WAITING } : colour([edgeKey({ kind: 'loop', stage: l.stage })]);
    put(l.at, l.text, style.fg === lineColour ? DARK : style);
    const key = edgeKey({ kind: 'loop', stage: l.stage });
    const where = spot(l.at);
    if (where && sent(key, null)) p.region({ r: rect(where[0], where[1], len(l.text), 1), click: () => click({ edge: key, to: null }), cursor: 'pointer' });
  }
  for (const b of g.boxes) drawBox(p, put, spot, b, local, click);
  const ended = !!live?.ended;
  put(g.end, '◆', ended ? { fg: BRAND, add: BOLD } : DARK);
  const at = g.below ?? ([g.end[0] + 2, g.end[1]] as At);
  put(at, truncateRight(d.end, Math.max(0, width - at[0])), ended ? {} : DARK);
  const lit = live?.lit;
  if (lit && lit.dot !== null) {
    const cells = path(g, lit);
    const dot = cells[Math.min(cells.length - 1, Math.floor(lit.dot * cells.length))];
    if (dot) put(dot, '●', { fg: lit.colour, add: BOLD });
  }
}

function drawBox(p: Painter, put: (at: At, text: string, s: Style) => void, spot: (at: At) => At | null, b: Placed, local: At | null, click: (hit: DiagramHit) => void): void {
  const r = b.rect;
  const { node } = b;
  const working = node.state === 'working';
  const closed = node.state === 'closed';
  const lit = !!local && contains(r, local[0], local[1]);
  const bg: Style = lit ? { bg: p.app.light ? 255 : 235 } : {};
  const border: Style = { ...bg, fg: working ? node.colour : p.lineColour };
  const [tl, tr, bl, br, h, v, x] = working ? ['┏', '┓', '┗', '┛', '━', '┃', '┰'] : ['╭', '╮', '╰', '╯', '─', '│', '┬'];
  const inner = Math.max(0, r.w - 2);
  for (let y = r.y; y < bottom(r); y++) put([r.x, y], ' '.repeat(r.w), bg);
  put([r.x, r.y], `${tl}${h.repeat(inner)}${tr}`, border);
  put([r.x, bottom(r) - 1], `${bl}${h.repeat(inner)}${br}`, border);
  for (let y = r.y + 1; y < bottom(r) - 1; y++) {
    put([r.x, y], v, border);
    put([right(r) - 1, y], v, border);
  }
  if (b.exit !== null) put([b.exit, bottom(r) - 1], x, border);
  if (r.w >= 6) put([r.x + 2, r.y], ` ${truncateRight(node.role, r.w - 6)} `, closed ? { ...bg, fg: 8 } : { ...bg, fg: node.colour, add: BOLD });
  if (node.own && r.w >= 6) {
    const used = len(truncateRight(node.role, r.w - 6)) + 4;
    const tag = [OWN_LONG, OWN_SHORT].find((t) => len(t) + 1 + used <= r.w);
    if (tag) put([right(r) - 1 - len(tag), r.y], tag, { ...bg, fg: 8 });
  }
  const icon = node.state && node.state !== 'closed' ? p.statusSeg(node.state === 'none' ? 'idle' : node.state) : null;
  const words = closed ? 'closed' : node.text;
  put([r.x + 2, r.y + 1], truncateRight(words, Math.max(0, r.w - (icon ? 6 : 4))), { ...bg, fg: 8 });
  if (icon && r.w >= 5) put([right(r) - 3, r.y + 1], icon.t, { ...bg, ...icon.s });
  const top = spot([r.x, r.y]);
  if (top) p.region({ r: rect(top[0], top[1], r.w, Math.max(0, Math.min(BOX_HEIGHT, bottom(p.app.areas().changes) - top[1]))), click: () => click({ role: node.role }), cursor: 'pointer' });
}

export interface PanelFile {
  name: string;
  age: string;
}

export interface PanelView {
  name: string;
  task: string;
  diagram: Diagram;
  files: PanelFile[] | null;
  scroll: number;
}

const FILES_LEAD = 2;

function filesHeight(view: PanelView): number {
  return view.files ? FILES_LEAD + Math.max(1, view.files.length) : 0;
}

function contentHeight(view: PanelView, width: number): number {
  return diagramHeight(view.diagram, width) + filesHeight(view);
}

export function fileAge(ms: number): string {
  const minutes = Math.max(0, Math.floor(ms / 60000));
  const hours = Math.floor(minutes / 60);
  if (minutes < 60) return `${minutes}m`;
  return hours < 24 ? `${hours}h` : `${Math.floor(hours / 24)}d`;
}

export function panelBody(area: Rect): Rect {
  const y = Math.min(area.y + HEADER, bottom(area));
  return rect(area.x + 1, y, Math.max(0, area.w - 2), bottom(area) - y);
}

export function scrolled(area: Rect, view: PanelView, delta: number): number {
  const body = panelBody(area);
  const most = Math.max(0, contentHeight(view, body.w) - body.h);
  return Math.max(0, Math.min(most, view.scroll + delta));
}

export function drawFlow(p: Painter, areas: Areas, view: PanelView): void {
  const app = p.app;
  const area = areas.changes;
  drawPanelBorder(p, areas);
  p.g.clear(area);
  const header = rect(area.x + 1, area.y, Math.max(0, area.w - 2), 1);
  p.span(header.x, header.y, truncateRight(view.name, Math.max(0, header.w - 4)), { add: BOLD });
  const close = rect(right(header) - 3, header.y, 3, 1);
  p.span(close.x + 1, close.y, '×', p.hovered(close) ? { fg: 1, add: BOLD } : DARK);
  p.region({ r: close, click: () => app.toggleFlow(), cursor: 'pointer' });
  p.span(header.x, header.y + 1, truncateRight(view.task.split('\n')[0], header.w), DARK, header.w);
  const body = panelBody(area);
  const scroll = scrolled(area, view, 0);
  drawDiagram(p, body, view.diagram, scroll, (hit) => app.flowClick(hit));
  if (view.files) drawFiles(p, body, view.files, diagramHeight(view.diagram, body.w) - scroll, (i) => app.flowClick({ file: i }));
  p.region({ r: body, wheel: (dy) => app.scrollFlow(Math.sign(dy) * 3) });
  const more = (y: number, text: string) => p.span(right(body) - len(text), y, text, DARK, len(text));
  if (scroll > 0) more(body.y, ' ↑ more ');
  if (contentHeight(view, body.w) - scroll > body.h && body.h > 0) more(bottom(body) - 1, ' ↓ more ');
}

function drawFiles(p: Painter, body: Rect, files: PanelFile[], top: number, click: (i: number) => void): void {
  const at = (row: number) => {
    const y = top + row;
    return y >= 0 && y < body.h ? body.y + y : null;
  };
  const head = at(1);
  if (head !== null) {
    p.span(body.x, head, 'Files', { add: BOLD });
    p.span(body.x + 5, head, ` · ${files.length}`, DARK);
  }
  if (!files.length) {
    const y = at(FILES_LEAD);
    if (y !== null) p.span(body.x, y, 'nothing yet', DARK);
    return;
  }
  files.forEach((file, i) => {
    const y = at(FILES_LEAD + i);
    if (y === null) return;
    const r = rect(body.x, y, body.w, 1);
    if (p.hovered(r)) p.span(r.x, y, ' '.repeat(r.w), { bg: p.app.light ? 255 : 235 });
    const age = len(file.age);
    p.span(body.x, y, truncateRight(file.name, Math.max(0, body.w - age - 1)), {}, body.w);
    p.span(right(body) - age, y, file.age, DARK, age);
    p.region({ r, click: () => click(i), cursor: 'pointer' });
  });
}
