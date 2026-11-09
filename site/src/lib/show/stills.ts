import { App } from '../demo/app';
import { issuesArea } from '../demo/layout';
import { fixedState } from '../demo/scenes';
import { world } from '../demo/world';
import { type Grid, type Rect, rect } from '../term/grid';
import { seeded } from './math';
import { changesWidth } from './script';

export const STILLS = ['issues', 'changes', 'files', 'flow', 'phone', 'menu'] as const;

export type Still = (typeof STILLS)[number];

export const PHONE: [number, number] = [46, 30];

const FILE = 'src/theme.rs';

function fresh(cols: number, rows: number): App {
  const app = new App();
  app.virtual = true;
  app.random = seeded(7);
  app.resize(cols, rows);
  return app;
}

function panel(app: App): Rect {
  app.widths = { ...app.widths, changes: changesWidth(app.cols) };
  app.dirty();
  app.render();
  return app.areas().changes;
}

const BUILD: Record<Still, (app: App) => Rect> = {
  issues: (app) => {
    world(app);
    app.openIssues();
    app.advance(1500);
    return issuesArea(app.cols, app.rows);
  },
  changes: (app) => {
    fixedState(app);
    return panel(app);
  },
  files: (app) => {
    world(app);
    app.toggleFiles();
    app.showFile(FILE);
    app.advance(400);
    return panel(app);
  },
  flow: (app) => {
    world(app);
    const run = app.runs[0];
    if (run) app.showFlowOf(run.workspace);
    app.render();
    app.advance(14000);
    return panel(app);
  },
  phone: (app) => {
    world(app);
    app.advance(20000);
    return rect(0, 0, app.cols, app.rows);
  },
  menu: (app) => {
    world(app);
    app.nav = 'projects';
    app.dirty();
    return rect(0, 0, app.cols, app.rows);
  },
};

export function still(name: Still, cols: number, rows: number): Grid {
  const app = name === 'phone' ? fresh(...PHONE) : fresh(cols, rows);
  const crop = BUILD[name](app);
  const { grid } = app.render();
  const cut = grid.crop(crop);
  app.destroy();
  return cut;
}
