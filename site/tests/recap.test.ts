import assert from 'node:assert/strict';
import { test } from 'node:test';

import { App } from '../src/lib/demo/app';
import { layout, recapButton, worktreesButton } from '../src/lib/demo/layout';
import { projectLabel } from '../src/lib/demo/model';
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

function press(app: App, label: string, index = 0): void {
  app.render();
  const at = app.textAt(label, index);
  assert.ok(at, `no ${label}`);
  app.pointerDown(at.x + 1, at.y, 0);
  app.pointerUp();
  app.render();
}

const widths = { projects: 32, workspaces: 26, changes: null, stack: null, agents: null };

for (const [columns, sidebar] of [[140, 'side_by_side'], [140, 'projects_on_top'], [140, 'tree'], [70, 'projects_on_top']] as const) {
  test(`Recap sits beside Worktrees on its row at ${columns} columns, ${sidebar}`, () => {
    const app = demo(columns, sidebar);
    const a = layout(columns, 38, widths, columns < 90 ? 'projects' : null, false, sidebar);
    const recap = recapButton(a.worktrees);
    const worktrees = worktreesButton(a.worktrees);
    assert.ok(recap.w > 0, 'recap shows');
    assert.equal(recap.y, a.worktrees.y);
    assert.equal(worktrees.x + worktrees.w, recap.x);
    press(app, ' Recap ');
    assert.equal(app.recap.open, true);
    app.toggleRecap();
    if (columns < 90) app.navTo('projects');
    press(app, ' Worktrees ');
    assert.match(app.toast?.text ?? '', /the demo has no worktrees on disk/);
    app.destroy();
  });
}

test('a row too narrow for both buttons keeps only Worktrees', () => {
  const row = { x: 0, y: 0, w: 20, h: 1 };
  assert.equal(recapButton(row).w, 0);
  assert.deepEqual(worktreesButton(row), row);
});

test('the three choices show their own recap and keep one panel open', () => {
  const app = demo(140);
  press(app, ' Recap ');
  assert.ok(app.textAt('On you'));
  assert.ok(app.textAt('Paginate') === null && app.textAt('In progress'));
  press(app, 'yesterday');
  assert.equal(app.recap.choice, 'yesterday');
  assert.ok(app.textAt('stand') === null && app.textAt('yesterday · mon 3 nov'));
  press(app, '7 days');
  assert.equal(app.recap.choice, 'week');
  assert.ok(app.textAt('blog'));
  app.destroy();
});

test('a place in the recap goes to its workspace', () => {
  const app = demo(140);
  press(app, ' Recap ');
  press(app, 'orders-api › fix/pagination › codex');
  const project = app.projects[app.active];
  assert.equal(projectLabel(project), 'orders-api');
  assert.equal(project.workspaces[project.active].name ?? project.workspaces[project.active].branch, 'fix/pagination');
  app.destroy();
});

test('the panel is exclusive with the other side panels and Esc closes it', () => {
  const app = demo(140);
  press(app, ' Recap ');
  app.toggleTodo();
  assert.equal(app.recap.open, false);
  press(app, ' Recap ');
  assert.equal(app.todo.open, false);
  app.key({ key: 'Escape' });
  assert.equal(app.recap.open, false);
  assert.equal(app.panelShown(), false);
  app.destroy();
});

test('s in the keys menu opens the recap', () => {
  const app = demo(140);
  app.overlay = { kind: 'keys', group: null };
  app.key({ key: 's' });
  assert.equal(app.recap.open, true);
  app.destroy();
});

test('the settings Agents page lists the Recap rows', () => {
  const app = demo(140);
  const labels = app.settingsRows(1).filter((r) => r.section === 'Recap').map((r) => `${r.label}=${r.value}`);
  assert.deepEqual(labels, [
    'recap agent=auto (claude)',
    'recap account=default',
    'recap model=default',
    'recap effort=low',
    'recap language=auto',
    'recap instructions=none',
  ]);
  app.destroy();
});
