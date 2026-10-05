import { type Rect, rect } from '../term/grid';
import { App } from './app';
import { agentIn, agentTab, world } from './world';

export interface Scene {
  cols: number;
  rows: number;
  build: (app: App) => Rect | void;
}

export const SCENES: Record<string, Scene> = {
  hero: {
    cols: 140,
    rows: 32,
    build: (app) => {
      world(app);
      app.advance(300);
    },
  },
  status: {
    cols: 104,
    rows: 24,
    build: (app) => {
      app.widths = { projects: 22, workspaces: 26 };
      world(app);
      const shop = app.projects[0];
      shop.active = 0;
      agentTab(app, shop, app.addWorkspace(shop, 'fix/return-labels', true), 'Print return labels as PDF', 600000);
      app.render();
      agentIn(app, 'shop', 'feat/gift-cards')?.ask('src/checkout.rs');
      app.advance(20000);
      app.workspacesScroll = 5;
      return rect(0, 2, 48, 17);
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
