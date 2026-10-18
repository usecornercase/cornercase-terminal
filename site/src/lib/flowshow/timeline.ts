import type { Page } from '../../i18n/en';
import type { EdgeId, NodeId, Target } from './graph';

export type Caption = keyof Page['flows']['steps'];
export type State = 'idle' | 'working' | 'done' | 'finding' | 'pending' | 'reached';
export type Tone = 'issue' | NodeId | 'finding' | 'done';
export interface Packet {
  edge: EdgeId;
  at: number;
  tone: Tone;
}
export interface Frame {
  intro: boolean;
  nodes: Record<NodeId, State>;
  checks: 'none' | 'running' | 'passed';
  packets: Packet[];
  lit: EdgeId[];
  hot: EdgeId[];
  ripple: number | null;
  ticker: string;
  rounds: number;
  commits: number;
  focus: { from: Target; to: Target; mix: number };
  caption: Caption;
  mood: 'working' | 'waiting' | 'done';
}

export const INTRO = 1200;
export const CYCLE = 19500;
export const PACKETS = 2;
export const RESET = 19000;
export const RIPPLE_FOR = 600;
export const TYPE_MS = 20;
export const EASE_MS = 600;

export const CAPTIONS: Caption[] = ['issue', 'plan', 'code', 'review', 'back', 'again', 'pr'];

const ALL: NodeId[] = ['orchestrator', 'planner', 'coder', 'reviewer', 'security', 'end'];

export const STATES: [number, NodeId, State][] = [
  [300, 'orchestrator', 'working'],
  [1700, 'planner', 'working'],
  [3500, 'planner', 'done'],
  [4400, 'coder', 'working'],
  [6800, 'coder', 'done'],
  [7800, 'reviewer', 'working'],
  [7800, 'security', 'working'],
  [9600, 'security', 'done'],
  [9900, 'reviewer', 'finding'],
  [11400, 'coder', 'working'],
  [13000, 'coder', 'done'],
  [14000, 'reviewer', 'working'],
  [14000, 'security', 'working'],
  [15400, 'reviewer', 'done'],
  [15400, 'security', 'done'],
  [16400, 'end', 'reached'],
  [17200, 'orchestrator', 'pending'],
  ...ALL.map((n): [number, NodeId, State] => [RESET, n, 'idle']),
];

export const MOVES: [number, number, EdgeId, Tone][] = [
  [0, 700, 'issue>orchestrator', 'issue'],
  [900, 800, 'orchestrator>planner', 'orchestrator'],
  [3700, 700, 'planner>coder', 'planner'],
  [7000, 800, 'coder>reviewer', 'coder'],
  [7000, 800, 'coder>security', 'coder'],
  [10200, 1200, 'reviewer>coder', 'finding'],
  [13200, 800, 'coder>reviewer', 'coder'],
  [13200, 800, 'coder>security', 'coder'],
  [15600, 800, 'reviewer>end', 'done'],
  [15600, 800, 'security>end', 'done'],
];

export const SENDS = [900, 3700, 7000, 10200, 13200, 15600];

export const LINES: [number, string][] = [
  [0, '← issue #482'],
  [900, '→ planner: plan #482'],
  [3700, '→ coder: implement the plan'],
  [7000, '→ reviewer, security: review it'],
  [9900, '← reviewer: 1 finding'],
  [10200, '→ coder: fix the finding'],
  [13200, '→ reviewer, security: again'],
  [15600, '✓ gh pr create'],
  [RESET, ''],
];

export const PIPS = [5000, 5600, 6200, 12600];
const ROUND_AT = 10900;
const CHECKS: [number, 'running' | 'passed'][] = [
  [16400, 'running'],
  [17200, 'passed'],
];
const CAPTION_AT: [number, Caption, Frame['mood']][] = [
  [0, 'issue', 'working'],
  [1700, 'plan', 'working'],
  [4400, 'code', 'working'],
  [7800, 'review', 'working'],
  [9900, 'back', 'waiting'],
  [13200, 'again', 'working'],
  [16400, 'pr', 'done'],
];
const FOCUS: [number, Target][] = [
  [0, 'orchestrator'],
  [1700, 'planner'],
  [4400, 'coder'],
  [7800, 'review'],
  [10200, 'coder'],
  [13200, 'review'],
  [15600, 'end'],
  [RESET, 'orchestrator'],
];

const idleNodes = (): Record<NodeId, State> => ({
  orchestrator: 'idle',
  planner: 'idle',
  coder: 'idle',
  reviewer: 'idle',
  security: 'idle',
  end: 'idle',
});

const INTRO_FRAME: Frame = {
  intro: true,
  nodes: idleNodes(),
  checks: 'none',
  packets: [],
  lit: [],
  hot: [],
  ripple: null,
  ticker: '',
  rounds: 0,
  commits: 0,
  focus: { from: 'orchestrator', to: 'orchestrator', mix: 1 },
  caption: 'issue',
  mood: 'working',
};

const last = <T extends [number, ...unknown[]]>(rows: T[], c: number): T | undefined => {
  let found: T | undefined;
  for (const r of rows) if (r[0] <= c) found = r;
  return found;
};

function cycle(c: number): Frame {
  const nodes = idleNodes();
  for (const [at, id, state] of STATES) if (at <= c) nodes[id] = state;

  const lit: EdgeId[] = [];
  const hot: EdgeId[] = [];
  const packets: Packet[] = [];
  if (c < RESET) {
    for (const [start, dur, edge, tone] of MOVES) {
      if (start > c) continue;
      if (!lit.includes(edge)) lit.push(edge);
      if (c < start + dur) {
        hot.push(edge);
        packets.push({ edge, at: (c - start) / dur, tone });
      }
    }
  }

  const send = SENDS.filter((s) => s <= c).pop();
  const ripple = c < RESET && send !== undefined && c - send < RIPPLE_FOR ? (c - send) / RIPPLE_FOR : null;

  const row = last(LINES, c);
  const ticker = row ? [...row[1]].slice(0, Math.floor((c - row[0]) / TYPE_MS)).join('') : '';

  const checks = last(CHECKS, c)?.[1] ?? 'none';
  const cap = last(CAPTION_AT, c) ?? CAPTION_AT[0];

  const i = FOCUS.findIndex((_, k) => k === FOCUS.length - 1 || FOCUS[k + 1][0] > c);
  const to = FOCUS[i][1];
  const from = FOCUS[Math.max(0, i - 1)][1];
  const mix = i === 0 ? 1 : Math.min(1, (c - FOCUS[i][0]) / EASE_MS);

  return {
    intro: false,
    nodes,
    checks: c >= RESET ? 'none' : checks,
    packets,
    lit,
    hot,
    ripple,
    ticker: c >= RESET ? '' : ticker,
    rounds: c >= ROUND_AT && c < RESET ? 1 : 0,
    commits: c >= RESET ? 0 : PIPS.filter((p) => p <= c).length,
    focus: { from, to, mix },
    caption: cap[1],
    mood: cap[2],
  };
}

export function frameAt(t: number): Frame {
  if (t < INTRO) return INTRO_FRAME;
  return cycle((t - INTRO) % CYCLE);
}

export const FINAL: Frame = frameAt(INTRO + 18000);
