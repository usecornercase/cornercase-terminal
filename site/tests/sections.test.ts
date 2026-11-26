import assert from 'node:assert/strict';
import { test } from 'node:test';

import { App } from '../src/lib/demo/app';
import { bottom, type Section, sectionTitle, type Sidebar, workspaceLayout, workspaceRows } from '../src/lib/demo/layout';
import { world } from '../src/lib/demo/world';

function demo(sidebar: Sidebar, cols = 140, rows = 38): App {
  const app = new App();
  app.virtual = true;
  app.scripted = true;
  app.resize(cols, rows);
  world(app, { scripted: true });
  app.config.sidebar = sidebar;
  app.config.agentsSection = true;
  app.render();
  return app;
}

const screen = (app: App) => app.render().grid.plain();

function titleRow(app: App, section: Section): string {
  const r = sectionTitle(app.areas(), section);
  const y = r.y + Math.floor(Math.max(0, r.h - 1) / 2);
  return [...screen(app).split('\n')[y]].slice(r.x, r.x + r.w).join('');
}

function clickTitle(app: App, section: Section): void {
  const r = sectionTitle(app.areas(), section);
  assert.ok(r.w > 0 && r.h > 0, `no ${section} title`);
  app.pointerDown(r.x + 3, r.y, 0);
  app.pointerUp();
  app.render();
}

const FOLDS: [Sidebar, Section[]][] = [
  ['side_by_side', ['projects', 'workspaces', 'agents']],
  ['projects_on_top', ['projects', 'workspaces', 'agents']],
  ['workspaces_on_top', ['projects', 'workspaces', 'agents']],
  ['tree', ['projects', 'agents']],
];

for (const [sidebar, sections] of FOLDS) {
  for (const section of sections) {
    test(`a click on the ${section} title folds and opens it in ${sidebar}`, () => {
      const app = demo(sidebar);
      assert.ok(titleRow(app, section).startsWith(' ▾ '), titleRow(app, section));
      const rows = () => app.areas()[section === 'projects' ? 'list' : section === 'workspaces' ? 'workspacesList' : 'agentsList'];

      clickTitle(app, section);
      assert.equal(app.folds[section], true);
      assert.ok(titleRow(app, section).startsWith(' ▸ '), titleRow(app, section));
      assert.equal(rows().h, 0);

      clickTitle(app, section);
      assert.equal(app.folds[section], false);
      assert.ok(titleRow(app, section).startsWith(' ▾ '), titleRow(app, section));
      assert.ok(rows().h > 0);
      app.destroy();
    });
  }
}

for (const nav of ['projects', 'workspaces', 'agents'] as const) {
  test(`a tap on the ${nav} menu title folds its section in compact mode`, () => {
    const app = demo('side_by_side', 80, 30);
    app.navTo(nav);
    app.render();

    clickTitle(app, nav);

    assert.equal(app.folds[nav], true);
    assert.ok(titleRow(app, nav).includes('▸ '), titleRow(app, nav));
    assert.equal(app.nav, nav);
    app.destroy();
  });
}

test('a press that leaves the title folds nothing', () => {
  const app = demo('side_by_side');
  const r = sectionTitle(app.areas(), 'projects');

  app.pointerDown(r.x + 3, r.y, 0);
  app.pointerMove(r.x + 3, r.y + 4, 1);
  app.pointerUp();

  assert.equal(app.folds.projects, false);
  app.destroy();
});

test('stacked, the open list takes the room of the folded one down to the footer', () => {
  const app = demo('projects_on_top');
  const before = app.areas();

  clickTitle(app, 'projects');
  const a = app.areas();

  assert.equal(a.title.h, 1);
  assert.equal(a.stackLine.y, bottom(a.title));
  assert.ok(a.workspacesList.h > before.workspacesList.h);
  assert.equal(bottom(a.workspacesList), a.agentsLine.y);
  assert.deepEqual([a.stackBorder.w, a.agentsBorder.w > 0], [0, true]);
  app.destroy();
});

test('side by side, folded projects leave the column to the agents', () => {
  const app = demo('side_by_side');

  clickTitle(app, 'projects');
  const a = app.areas();

  assert.deepEqual([a.agentsLine.y, bottom(a.agents)], [bottom(a.title), a.separator.y]);
  assert.equal(a.agentsBorder.w, 0);
  app.destroy();
});

test('a folded title says how many rows it hides', () => {
  const app = demo('side_by_side');
  const projects = app.projects.length;

  clickTitle(app, 'projects');

  assert.ok(titleRow(app, 'projects').startsWith(` ▸ Projects (${projects})`), titleRow(app, 'projects'));
  app.destroy();
});

for (const [section, name] of [
  ['projects', 'Projects'],
  ['workspaces', 'Workspaces'],
] as const) {
  test(`a folded ${section} title is never cut to make room for its count`, () => {
    const app = demo('side_by_side');
    clickTitle(app, section);
    const counted = new Set<boolean>();

    for (let width = 16; width <= 32; width++) {
      app.widths = { ...app.widths, [section]: width };
      app.render();
      const row = titleRow(app, section);
      assert.ok(!row.includes('(') || row.startsWith(` ▸ ${name} (`), `${width}: ${row}`);
      counted.add(row.includes('('));
    }

    assert.equal(counted.size, 2);
    app.destroy();
  });
}

for (const section of ['projects', 'workspaces'] as const) {
  test(`a folded ${section} title with nothing waiting is never cut shorter than the open one`, () => {
    const app = demo('side_by_side');

    for (let width = 16; width <= 32; width++) {
      app.widths = { ...app.widths, [section]: width };
      app.render();
      const open = titleRow(app, section).trimEnd().replace('▾', '▸');
      app.toggleSection(section);
      app.render();
      assert.ok(titleRow(app, section).startsWith(open), `${width}: ${titleRow(app, section)} against ${open}`);
      app.toggleSection(section);
    }
    app.destroy();
  });
}

for (const nav of ['projects', 'workspaces', 'agents'] as const) {
  test(`a narrow compact ${nav} menu keeps what needs you wherever it fits`, () => {
    const app = demo('side_by_side', 60, 30);
    app.sectionTags = () => ({ count: 3, attention: 'waiting' });
    app.toggleSection(nav);

    for (let cols = 18; cols <= 60; cols++) {
      app.resize(cols, 30);
      app.navTo(nav);
      app.render();
      const row = titleRow(app, nav);
      assert.equal(row.includes('!'), sectionTitle(app.areas(), nav).w >= 5, `${cols}: ${row}`);
    }
    app.destroy();
  });
}

test('expanding Workspaces reveals the active workspace and tab scrolled out of view', () => {
  const app = demo('side_by_side', 140, 16);
  const shop = app.project()!;
  const active = () => {
    const tabs = app.tabLines();
    const rows = workspaceRows(tabs, app.tabsListed(), app.workspacesFolded());
    const { workspacesList, pitch } = app.areas();
    const shown = workspaceLayout(workspacesList, pitch, rows, tabs, app.workspacesScroll);
    const tab = shop.workspaces[shop.active].active;
    return rows.flatMap((r, i) => ((r.kind === 'ws' || (r.kind === 'tab' && r.t === tab)) && r.w === shop.active ? [shown.item(i).h > 0] : []));
  };
  const list = app.areas().workspacesList;
  for (let turn = 0; turn < 20; turn++) app.wheel(list.x + 2, list.y + 1, 1);
  app.render();
  assert.deepEqual(active(), [false, false]);

  clickTitle(app, 'workspaces');
  clickTitle(app, 'workspaces');

  assert.deepEqual(active(), [true, true]);
  app.destroy();
});

test('the tree folds whole under the Projects title and has no Workspaces title', () => {
  const app = demo('tree');

  clickTitle(app, 'projects');

  assert.equal(sectionTitle(app.areas(), 'workspaces').w, 0);
  assert.equal(app.areas().list.h, 0);
  app.destroy();
});
