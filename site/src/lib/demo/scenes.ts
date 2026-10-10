import { type Rect, rect } from '../term/grid';
import { App } from './app';
import { ISSUES } from './data';
import { issuesArea } from './layout';
import type { Pos } from './model';
import { truncateRight } from './text';
import { POSTER_PACE, agentTab, world } from './world';

export interface Scene {
  cols: number;
  rows: number;
  build: (app: App) => Rect | void;
}

export const WINDOW_COLS = 177;
export const WINDOW_ROWS = 46;
const SHOT_COLS = 87;
const SHOT_ROWS = 30;
const CHANGES_FROM = 120;
const STATUS_COLS = 96;
const STATUS_ROWS = 43;
const LINK = 'src/theme.rs:7';

const shot = (x: number, y: number): Rect => rect(x, y, SHOT_COLS, SHOT_ROWS);

const panelWidth = (cols: number): number => Math.max(40, Math.min(52, cols - 125));

export function posterState(app: App): void {
  world(app);
  app.render();
  app.advance(20000);
  if (app.cols >= CHANGES_FROM) {
    app.widths = { ...app.widths, changes: panelWidth(app.cols) };
    if (!app.todo.open) app.toggleTodo();
  }
  app.render();
  app.hover = app.textAt(LINK);
  app.dirty();
}

export function fixedState(app: App): void {
  world(app);
  const shop = app.projects[0];
  const issue = ISSUES.find((i) => i.source === 'github' && i.number === 482);
  if (!issue) throw new Error('issue 482 is missing from data.ts');
  const w = app.addWorkspace(shop, app.branchFor(issue), true, truncateRight(`${issue.key} ${issue.title}`, 40));
  agentTab(app, shop, w, issue.url, POSTER_PACE, { shown: 4 });
  w.flags.fixed = true;
  shop.active = shop.workspaces.indexOf(w);
  app.render();
  app.advance(20000);
  app.widths = { ...app.widths, changes: SHOT_COLS };
  if (!app.changesOpen) app.toggleChanges();
  app.dirty();
}

function clickText(app: App, text: string, nth = 0): void {
  if (app.stale) app.render();
  let p: Pos | null = null;
  if (nth < 0) {
    for (let i = 0; ; i++) {
      const q = app.textAt(text, i);
      if (!q) break;
      p = q;
    }
  } else p = app.textAt(text, nth);
  if (!p) throw new Error(`not on screen: ${text}`);
  app.pointerDown(p.x, p.y, 0);
  app.pointerUp();
}

const poster = (cols: number, rows: number): Scene => ({ cols, rows, build: (app) => posterState(app) });

export const SCENES: Record<string, Scene> = {
  poster: poster(WINDOW_COLS, WINDOW_ROWS),
  'poster-190': poster(190, WINDOW_ROWS),
  'poster-202': poster(202, WINDOW_ROWS),
  'poster-220': poster(220, WINDOW_ROWS),
  'poster-102': poster(102, 32),
  'poster-compact': poster(48, 34),
  issue: {
    cols: 90,
    rows: WINDOW_ROWS,
    build: (app) => {
      world(app);
      app.advance(300);
      clickText(app, 'Issues', -1);
      app.advance(1200);
      clickText(app, '#482');
      app.advance(300);
      return issuesArea(app.cols, app.rows);
    },
  },
  status: {
    cols: STATUS_COLS,
    rows: STATUS_ROWS,
    build: (app) => {
      posterState(app);
    },
  },
  changes: {
    cols: WINDOW_COLS,
    rows: WINDOW_ROWS,
    build: (app) => {
      fixedState(app);
      return shot(app.cols - SHOT_COLS, 0);
    },
  },
  back: {
    cols: 60,
    rows: 6,
    build: (app) => {
      world(app);
      app.advance(300);
      app.quit();
      app.outerInput = 'cornercase';
      return rect(0, 0, 60, 2);
    },
  },
  og: {
    cols: 100,
    rows: WINDOW_ROWS,
    build: (app) => {
      posterState(app);
      return rect(0, 0, 56, 32);
    },
  },
  readme: {
    cols: 110,
    rows: STATUS_ROWS,
    build: (app) => {
      posterState(app);
    },
  },
};

export function run(name: string): { app: App; crop: Rect | null } {
  const scene = SCENES[name];
  if (!scene) throw new Error(`unknown scene: ${name}`);
  const app = new App();
  app.virtual = true;
  app.resize(scene.cols, scene.rows);
  const crop = scene.build(app) ?? null;
  return { app, crop };
}
