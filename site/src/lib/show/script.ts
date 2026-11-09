import type { App } from '../demo/app';
import { activePane } from '../demo/model';
import { Agent } from '../demo/programs';
import { agentIn } from '../demo/world';

export const LEAD = 9000;
export const IDLE: [number, number] = [LEAD - 950, LEAD - 150];
export const MOVE = 560;
export const DWELL = 140;
export const SETTLE = 260;
export const KEY = 180;
export const CHAR = 75;

export type Mark = 'issues' | 'picked' | 'started' | 'working' | 'needs' | 'answered' | 'finished' | 'fixing' | 'review' | 'hunk' | 'quit' | 'away' | 'typing' | 'back';

export type Spot = 'sidebar' | 'pane' | 'changes' | 'left';

export type Step =
  | { wait: number; rate?: number }
  | { click: string; nth?: number; dx?: number; alt?: string; orKey?: string }
  | { point: string; nth?: number; dx?: number }
  | { key: string }
  | { type: string }
  | { run: (app: App) => void }
  | { expect: (app: App) => boolean; what: string }
  | { mark: Mark }
  | { spot: Spot | null }
  | { pointer: false };

export const CHAPTERS = ['start', 'glance', 'review', 'leave'] as const;

export type Chapter = (typeof CHAPTERS)[number];

export const COMPACT = 90;

const compact = (app: App): boolean => app.cols < COMPACT;

const open = (nav: 'projects' | 'workspaces'): Step => ({
  run: (app) => {
    if (!compact(app)) return;
    app.nav = nav;
    app.changesOpen = false;
    app.dirty();
  },
});

const close: Step = {
  run: (app) => {
    if (!compact(app)) return;
    app.nav = null;
    app.dirty();
  },
};

const reveal: Step = {
  run: (app) => {
    if (compact(app)) app.nav = 'workspaces';
    app.workspacesScroll = 0;
    app.dirty();
  },
};

const show = (text: string): Step => ({
  run: (app) => {
    app.render();
    for (let i = 0; i < 12 && !app.textAt(text); i++) {
      app.workspacesScroll += 1;
      app.dirty();
      app.render();
    }
  },
});

const activeAgent = (app: App): Agent | null => {
  const tab = app.tab();
  const fg = tab ? activePane(tab)?.shell.fg : null;
  return fg instanceof Agent ? fg : null;
};

export const trusting = (app: App): boolean => activeAgent(app)?.trusting ?? false;

export const prompted = (app: App): boolean => activeAgent(app)?.pending.includes('https://') ?? false;

export const fixed = (app: App): boolean => app.projects.some((p) => p.workspaces.some((w) => !!w.flags.fixed));

export const changesWidth = (cols: number): number => Math.max(40, Math.min(64, cols - 83));

const widen: Step = {
  run: (app) => {
    if (!compact(app)) app.widths = { ...app.widths, changes: changesWidth(app.cols) };
  },
};

const start: Step[] = [
  reveal,
  { wait: 900 },
  open('workspaces'),
  { mark: 'issues' },
  { click: 'Issues', nth: -1 },
  { wait: 1300 },
  { mark: 'picked' },
  { click: '#482' },
  { wait: 1500 },
  { click: 'start   schedule', dx: 1, orKey: 'Enter' },
  { wait: 900 },
  { key: 'Enter' },
  { mark: 'started' },
  { pointer: false },
  { wait: 2900 },
  { expect: trusting, what: 'the agent asks whether it can trust the folder' },
  { key: 'Enter' },
  { wait: 900 },
  { expect: prompted, what: 'the issue is in the prompt' },
  { key: 'Enter' },
  { mark: 'working' },
  { wait: 2400, rate: 2 },
];

const glance: Step[] = [
  reveal,
  { spot: 'left' },
  { wait: 1700 },
  { run: (app) => void agentIn(app, 'orders-api', 'fix/pagination')?.ask('src/orders.ts') },
  { mark: 'needs' },
  { wait: 2400 },
  open('projects'),
  { spot: null },
  { click: 'orders-api (' },
  close,
  { wait: 1500 },
  { key: 'Enter' },
  { mark: 'answered' },
  { wait: 1600 },
  open('projects'),
  { click: 'web-shop (' },
  open('workspaces'),
  show('feat/g'),
  { click: 'feat/g' },
  close,
  { mark: 'finished' },
  { pointer: false },
  { wait: 1500 },
];

const review: Step[] = [
  reveal,
  open('workspaces'),
  show('#482'),
  { click: '#482' },
  close,
  { mark: 'fixing' },
  { pointer: false },
  { wait: 4400, rate: 5 },
  { expect: fixed, what: 'the agent has changed the files' },
  widen,
  { click: 'Changes', nth: -1, alt: '±' },
  { mark: 'review' },
  { wait: 2600 },
  { mark: 'hunk' },
  { point: 'impl Address', nth: -1 },
  { wait: 2600 },
  { pointer: false },
];

const quiet: Step = {
  run: (app) => {
    app.toast = null;
    app.dirty();
  },
};

const leave: Step[] = [
  quiet,
  open('projects'),
  { wait: 700 },
  { click: 'More ▴', nth: -1 },
  { click: 'quit', nth: -1 },
  { mark: 'quit' },
  { run: (app) => void (app.hover = null) },
  { pointer: false },
  { wait: 1500 },
  { mark: 'away' },
  { wait: 4200, rate: 14 },
  { mark: 'typing' },
  { type: 'cornercase' },
  { wait: 500 },
  { key: 'Enter' },
  { mark: 'back' },
  reveal,
  { wait: 3200 },
];

export const STORY: Record<Chapter, Step[]> = { start, glance, review, leave };

export function lengthOf(step: Step): number {
  if ('wait' in step) return step.wait;
  if ('click' in step) return MOVE + DWELL + SETTLE;
  if ('point' in step) return MOVE;
  if ('key' in step) return KEY;
  if ('type' in step) return CHAR * [...step.type].length;
  return 0;
}

export interface Times {
  total: number;
  chapters: Record<Chapter, [number, number]>;
  marks: Record<Mark, number>;
}

export function timesOf(story: Record<Chapter, Step[]> = STORY): Times {
  const chapters = {} as Record<Chapter, [number, number]>;
  const marks = {} as Record<Mark, number>;
  let at = LEAD;
  for (const name of CHAPTERS) {
    const from = at;
    for (const step of story[name]) {
      if ('mark' in step) marks[step.mark] = at;
      at += lengthOf(step);
    }
    chapters[name] = [from, at];
  }
  return { total: at, chapters, marks };
}

export const TIMES = timesOf();
