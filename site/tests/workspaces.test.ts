import assert from 'node:assert/strict';
import { test } from 'node:test';
import { App } from '../src/lib/demo/app';
import { workspaceLabel } from '../src/lib/demo/model';
import { world } from '../src/lib/demo/world';

function demo(): App {
  const app = new App();
  app.virtual = true;
  app.resize(177, 46);
  world(app, { scripted: true });
  app.render();
  return app;
}

test('the arrow beside a workspace hides its tabs, and the row shows its agents', () => {
  const app = demo();
  const project = app.project()!;
  const tabs = project.workspaces[0].tabs.length;
  assert.ok(tabs > 0);
  const name = app.textAt(`▾ ${workspaceLabel(project.workspaces[0])}`);
  assert.ok(name);
  const before = app.render().grid.cells.filter((c) => c.ch === '├').length;
  app.pointerDown(name.x, name.y, 0);
  app.pointerUp();
  assert.equal(project.workspaces[0].collapsed, true);
  const after = app.render().grid.cells.filter((c) => c.ch === '├').length;
  assert.ok(after < before);
  assert.equal(project.workspaces[0].tabs.length, tabs);
  assert.ok(app.textAt(`▸ ${workspaceLabel(project.workspaces[0])}`));
  app.destroy();
});

test('the arrow expands a collapsed workspace again', () => {
  const app = demo();
  const project = app.project()!;
  project.workspaces[0].collapsed = true;
  app.render();
  const arrow = app.textAt(`▸ ${workspaceLabel(project.workspaces[0])}`);
  assert.ok(arrow);
  app.pointerDown(arrow.x, arrow.y, 0);
  app.pointerUp();
  assert.equal(project.workspaces[0].collapsed, false);
  app.destroy();
});

test('A tab shows how many subagents its agent still runs on its second line', () => {
  const app = new App();
  app.virtual = true;
  app.resize(140, 40);
  world(app, { scripted: true });
  const pane = app.projects[0].workspaces[0].tabs[0].panes[0];
  const screen = () => app.render().grid.cells.map((c) => c.ch).join('');
  assert.ok(!screen().includes('subagent'));
  pane.subagents = 1;
  assert.ok(screen().includes('1 subagent'));
  assert.ok(!screen().includes('1 subagents'));
  pane.subagents = 2;
  assert.ok(screen().includes('2 subagents'));
  app.destroy();
});
