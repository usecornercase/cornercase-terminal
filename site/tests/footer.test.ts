import assert from 'node:assert/strict';
import { test } from 'node:test';

import { App } from '../src/lib/demo/app';
import { CHAPTERS } from '../src/lib/demo/boot';
import { changesButton, helpButton, layout, moreButton } from '../src/lib/demo/layout';
import type { Mounted } from '../src/lib/demo/client';
import { play } from '../src/lib/demo/tour';
import { world } from '../src/lib/demo/world';

function demo(columns: number, sidebar = 'projects_on_top'): App {
  const app = new App();
  app.virtual = true;
  app.scripted = true;
  app.resize(columns, 38);
  world(app, { scripted: true });
  app.config.sidebar = sidebar;
  if (columns < 90) app.navTo('projects');
  app.render();
  return app;
}

function last(app: App, label: string): { x: number; y: number } | null {
  app.render();
  let at = null;
  for (let i = 0; ; i++) {
    const q = app.textAt(label, i);
    if (!q) return at;
    at = q;
  }
}

function press(app: App, label: string): void {
  const at = last(app, label);
  assert.ok(at, `no ${label}`);
  app.pointerDown(at.x + 1, at.y, 0);
  app.pointerUp();
  app.render();
}

function menu(app: App): string[] {
  const o = app.overlay;
  assert.equal(o?.kind, 'menu');
  return o.kind === 'menu' ? o.actions.map((a) => (a.kind === 'more' ? a.item : a.kind)) : [];
}

for (const [columns, sidebar] of [[140, 'projects_on_top'], [140, 'side_by_side'], [140, 'tree'], [70, 'projects_on_top']] as const) {
  test(`More replaces Settings, Usage and Quit in the footer at ${columns} columns, ${sidebar}`, () => {
    const app = demo(columns, sidebar);
    for (const gone of [' Settings ', ' Usage ', ' Quit ']) assert.equal(app.textAt(gone), null, gone);
    press(app, ' More ▴ ');
    assert.deepEqual(menu(app), ['settings', 'usage', 'quit']);
    press(app, 'settings');
    assert.equal(app.overlay?.kind, 'settings');
    app.closeOverlay();
    if (columns < 90) app.navTo('projects');
    press(app, ' More ▴ ');
    press(app, 'usage');
    assert.equal(app.overlay?.kind, 'usage');
    app.destroy();
  });
}

for (const [columns, sidebar] of [[140, 'projects_on_top'], [140, 'side_by_side'], [140, 'tree'], [70, 'projects_on_top']] as const) {
  test(`Worktrees in the footer says the demo has none at ${columns} columns, ${sidebar}`, () => {
    const app = demo(columns, sidebar);
    press(app, ' Worktrees ');
    assert.match(app.toast?.text ?? '', /the demo has no worktrees on disk/);
    app.destroy();
  });
}

test('the compact menu gives Worktrees a band of its own above More, in the projects menu only', () => {
  const widths = { projects: 32, workspaces: 26, changes: null, stack: null, agents: null };
  const projects = layout(70, 38, widths, 'projects', false, 'projects_on_top');
  const workspaces = layout(70, 38, widths, 'workspaces', false, 'projects_on_top');
  assert.deepEqual([projects.worktrees.h, projects.worktrees.y + projects.worktrees.h, workspaces.worktrees.w], [3, projects.more.y, 0]);
});

test('quit in the More menu detaches', () => {
  const app = demo(140);
  let detached = false;
  app.on((event) => { if (event === 'detached') detached = true; });
  press(app, ' More ▴ ');
  press(app, 'quit');
  assert.ok(detached);
  app.destroy();
});

test('the tools sit in two columns: Changes and TODO start where the row is halved', () => {
  const a = layout(140, 38, { projects: 32, workspaces: 26, changes: null, stack: null, agents: null }, null, false, 'projects_on_top');
  const issues = a.issues;
  const column = issues.x + 1 + Math.floor(issues.w / 2);
  assert.equal(changesButton(issues, 'Changes 3').x, column);
  assert.equal(a.todoButton.x, column);
  assert.equal(a.filesButton.x, issues.x + 1);
  assert.deepEqual([a.issues.y + 1, a.worktrees.y, a.moreSeparator.y, a.more.y], [a.todoButton.y, a.todoButton.y + 1, a.todoButton.y + 2, 37]);
});

test('a label that does not fit the second column is right-aligned instead', () => {
  const row = { x: 0, y: 0, w: 15, h: 1 };
  const r = changesButton(row, 'Changes 9+');
  assert.equal(r.x + r.w, 14);
});

test('side by side, Worktrees sits above More in the projects column so both lines stay level', () => {
  const a = layout(140, 38, { projects: 32, workspaces: 26, changes: null, stack: null, agents: null }, null, false, 'side_by_side');
  assert.deepEqual([a.separator.y, a.workspacesSeparator.y, a.issues.y, a.worktrees.y, a.more.y], [35, 35, 36, 36, 37]);
  assert.equal(a.list.y + a.list.h, a.separator.y);
});

test('More keeps the rest of its row left of Help, and a narrow row has no Help', () => {
  const row = { x: 0, y: 0, w: 31, h: 1 };
  const help = helpButton(row);
  assert.deepEqual(moreButton(row), { ...row, w: help.x });
  const narrow = { x: 0, y: 0, w: 15, h: 1 };
  assert.deepEqual([helpButton(narrow).w, moreButton(narrow)], [0, narrow]);
});

for (const columns of [140, 80]) {
  test(`the hero tour detaches through the More menu at ${columns} columns`, async () => {
    const app = new App();
    app.virtual = true;
    app.scripted = true;
    app.resize(columns, 34);
    world(app, { scripted: true });
    let detached = false;
    app.on((event) => { if (event === 'detached') detached = true; });
    await play({ app } as Mounted, null, CHAPTERS.flat(), { instant: true }).done;
    assert.ok(detached);
    app.destroy();
  });
}
