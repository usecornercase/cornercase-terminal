export type NodeId = 'orchestrator' | 'planner' | 'coder' | 'reviewer' | 'security' | 'end';
export type EdgeId =
  | 'issue>orchestrator'
  | 'orchestrator>planner'
  | 'planner>coder'
  | 'coder>reviewer'
  | 'coder>security'
  | 'reviewer>coder'
  | 'reviewer>end'
  | 'security>end';
export type Target = NodeId | 'review';
export type Point = [number, number];
export interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}
export interface Curve {
  p0: Point;
  c1: Point;
  c2: Point;
  p3: Point;
}
export interface Fonts {
  role: number;
  agent: number;
  ticker: number;
  issue: number;
  endTitle: number;
  tag: number;
  loop: number;
}
export interface Layout {
  name: 'wide' | 'tall';
  width: number;
  height: number;
  fonts: Fonts;
  issue: Box;
  issueText: string;
  nodes: Record<NodeId, Box>;
  edges: Record<EdgeId, Curve>;
  loopLabel: Box;
  loopText: (rounds: number) => string;
  spot: number;
}

export const NODES: NodeId[] = ['orchestrator', 'planner', 'coder', 'reviewer', 'security', 'end'];
export const EDGES: EdgeId[] = [
  'issue>orchestrator',
  'orchestrator>planner',
  'planner>coder',
  'coder>reviewer',
  'coder>security',
  'reviewer>coder',
  'security>end',
  'reviewer>end',
];

export const CAST: Record<Exclude<NodeId, 'end'>, { agent: string; colour: string }> = {
  orchestrator: { agent: 'claude · opus', colour: '#a88dff' },
  planner: { agent: 'claude', colour: '#45d9e8' },
  coder: { agent: 'codex · gpt-5.4', colour: '#6aa8ff' },
  reviewer: { agent: 'claude', colour: '#58e6a0' },
  security: { agent: 'gemini', colour: '#ff7ac8' },
};

export const END_TITLE = 'open a pull request';
export const END_TAG = '#487';
export const ISSUE_KEY = '#482';
export const MAX_ROUNDS = 3;

const line = (a: Point, b: Point): Curve => ({ p0: a, c1: a, c2: b, p3: b });
const bez = (a: Point, c1: Point, c2: Point, b: Point): Curve => ({ p0: a, c1, c2, p3: b });

const wideFonts: Fonts = { role: 18, agent: 12.5, ticker: 12, issue: 12, endTitle: 15, tag: 13, loop: 12 };
const tallFonts: Fonts = { role: 16, agent: 12, ticker: 12, issue: 12, endTitle: 15, tag: 12.5, loop: 11 };

export const LAYOUTS: Record<'wide' | 'tall', Layout> = {
  wide: {
    name: 'wide',
    width: 1120,
    height: 460,
    fonts: wideFonts,
    issue: { x: 40, y: 40, w: 340, h: 44 },
    issueText: ' Returns page crashes on empty address',
    nodes: {
      orchestrator: { x: 440, y: 16, w: 300, h: 92 },
      planner: { x: 40, y: 236, w: 170, h: 66 },
      coder: { x: 300, y: 236, w: 200, h: 66 },
      security: { x: 600, y: 180, w: 180, h: 60 },
      reviewer: { x: 600, y: 290, w: 180, h: 60 },
      end: { x: 870, y: 236, w: 230, h: 66 },
    },
    edges: {
      'issue>orchestrator': line([380, 62], [440, 62]),
      'orchestrator>planner': bez([590, 108], [590, 172], [125, 172], [125, 236]),
      'planner>coder': line([210, 269], [300, 269]),
      'coder>security': bez([500, 269], [550, 269], [550, 210], [600, 210]),
      'coder>reviewer': bez([500, 269], [550, 269], [550, 320], [600, 320]),
      'reviewer>coder': bez([690, 350], [690, 440], [400, 440], [400, 302]),
      'security>end': bez([780, 210], [825, 210], [825, 269], [870, 269]),
      'reviewer>end': bez([780, 320], [825, 320], [825, 269], [870, 269]),
    },
    loopLabel: { x: 450, y: 398, w: 190, h: 28 },
    loopText: (rounds) => (rounds > 0 ? `findings · ${rounds}/${MAX_ROUNDS}` : `findings · at most ${MAX_ROUNDS}`),
    spot: 200,
  },
  tall: {
    name: 'tall',
    width: 340,
    height: 576,
    fonts: tallFonts,
    issue: { x: 20, y: 14, w: 300, h: 38 },
    issueText: ' Returns page crashes…',
    nodes: {
      orchestrator: { x: 20, y: 76, w: 300, h: 92 },
      planner: { x: 50, y: 206, w: 240, h: 58 },
      coder: { x: 50, y: 296, w: 240, h: 58 },
      security: { x: 37, y: 392, w: 128, h: 58 },
      reviewer: { x: 175, y: 392, w: 128, h: 58 },
      end: { x: 40, y: 492, w: 260, h: 62 },
    },
    edges: {
      'issue>orchestrator': line([170, 52], [170, 76]),
      'orchestrator>planner': line([170, 168], [170, 206]),
      'planner>coder': line([170, 264], [170, 296]),
      'coder>security': bez([170, 354], [170, 373], [101, 373], [101, 392]),
      'coder>reviewer': bez([170, 354], [170, 373], [239, 373], [239, 392]),
      'reviewer>coder': bez([303, 421], [336, 421], [336, 325], [290, 325]),
      'security>end': bez([101, 450], [101, 471], [170, 471], [170, 492]),
      'reviewer>end': bez([239, 450], [239, 471], [170, 471], [170, 492]),
    },
    loopLabel: { x: 236, y: 362, w: 98, h: 22 },
    loopText: (rounds) => (rounds > 0 ? `findings ${rounds}/${MAX_ROUNDS}` : `findings ≤${MAX_ROUNDS}`),
    spot: 150,
  },
};

export const textWidth = (text: string, size: number): number => [...text].length * 0.6 * size;

const f = (n: number): string => String(Math.round(n * 100) / 100);

export const pathD = (c: Curve): string =>
  `M ${f(c.p0[0])} ${f(c.p0[1])} C ${f(c.c1[0])} ${f(c.c1[1])} ${f(c.c2[0])} ${f(c.c2[1])} ${f(c.p3[0])} ${f(c.p3[1])}`;

export function bezier(c: Curve, t: number): Point {
  const u = 1 - t;
  const w = [u * u * u, 3 * u * u * t, 3 * u * t * t, t * t * t];
  const at = (i: 0 | 1): number => w[0] * c.p0[i] + w[1] * c.c1[i] + w[2] * c.c2[i] + w[3] * c.p3[i];
  return [at(0), at(1)];
}

const SAMPLES = 64;

function table(c: Curve): { pts: Point[]; len: number[] } {
  const pts: Point[] = [];
  const len = [0];
  for (let i = 0; i <= SAMPLES; i++) {
    pts.push(bezier(c, i / SAMPLES));
    if (i > 0) len.push(len[i - 1] + Math.hypot(pts[i][0] - pts[i - 1][0], pts[i][1] - pts[i - 1][1]));
  }
  return { pts, len };
}

export function pointAt(c: Curve, p: number): Point {
  const { pts, len } = table(c);
  const want = Math.min(1, Math.max(0, p)) * len[SAMPLES];
  let i = 1;
  while (i < SAMPLES && len[i] < want) i++;
  const span = len[i] - len[i - 1] || 1;
  const k = (want - len[i - 1]) / span;
  return [pts[i - 1][0] + (pts[i][0] - pts[i - 1][0]) * k, pts[i - 1][1] + (pts[i][1] - pts[i - 1][1]) * k];
}

export function arrowHead(c: Curve, size = 9): string {
  const [x, y] = c.p3;
  const [px, py] = bezier(c, 0.97);
  const a = Math.atan2(y - py, x - px);
  const pt = (angle: number, r: number): string => `${f(x + Math.cos(angle) * r)} ${f(y + Math.sin(angle) * r)}`;
  return `M ${f(x)} ${f(y)} L ${pt(a + Math.PI - 0.5, size)} L ${pt(a + Math.PI + 0.5, size)} Z`;
}

export const centre = (b: Box): Point => [b.x + b.w / 2, b.y + b.h / 2];

export function focusPoint(layout: Layout, target: Target): Point {
  if (target !== 'review') return centre(layout.nodes[target]);
  const a = centre(layout.nodes.reviewer);
  const b = centre(layout.nodes.security);
  return [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2];
}

export const roundedRect = (w: number, h: number, r: number): string =>
  `M ${r} 0 H ${w - r} A ${r} ${r} 0 0 1 ${w} ${r} V ${h - r} A ${r} ${r} 0 0 1 ${w - r} ${h} H ${r} A ${r} ${r} 0 0 1 0 ${h - r} V ${r} A ${r} ${r} 0 0 1 ${r} 0 Z`;

export const source = (edge: EdgeId): string => edge.split('>')[0];
export const target = (edge: EdgeId): string => edge.split('>')[1];
