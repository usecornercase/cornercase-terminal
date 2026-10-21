import assert from 'node:assert/strict';
import { test } from 'node:test';
import { App } from '../src/lib/demo/app';
import { paneStatus, workspaceLabel } from '../src/lib/demo/model';
import { world } from '../src/lib/demo/world';

function demo(): App {
  const app = new App();
  app.virtual = true;
  app.resize(177, 46);
  world(app, { scripted: true });
  app.render();
  return app;
}

const agents = (app: App, w = 0): number => app.project()!.workspaces[w].tabs.filter((t) => t.panes.some((p) => paneStatus(p))).length;

function menuOf(app: App, workspace = 0): string[] {
  const p = app.project()!;
  app.openMenu({ x: 2, y: 2 }, { kind: 'workspace', project: p.id, workspace: p.workspaces[workspace].id });
  if (app.overlay?.kind !== 'menu') return assert.fail('no menu');
  return app.overlay.actions.map((a) => app.menuLabel(a));
}

test('a workspace menu offers close agents, and closing leaves the shells and a reopen tag on the row', () => {
  const app = demo();
  const ws = app.project()!.workspaces[0];
  const shells = ws.tabs.length - agents(app);
  assert.ok(agents(app) > 0);
  assert.ok(menuOf(app).includes('close agents'));
  const i = app.overlay?.kind === 'menu' ? app.overlay.actions.findIndex((a) => a.kind === 'closeAgents') : -1;
  app.chooseMenu(i);
  assert.equal(agents(app), 0);
  assert.equal(ws.tabs.length, shells);
  assert.equal(app.closedCount(ws.id), 1);
  assert.match(app.toast?.text ?? '', /1 agent closed · ▶ on the workspace reopens it/);
  app.render();
  assert.ok(app.textAt('▶'));
  app.destroy();
});

test('the tag on the row reopens the agents after the other tabs', () => {
  const app = demo();
  const ws = app.project()!.workspaces[0];
  app.closeAgents(ws.id);
  const tag = app.render() && app.textAt(`${workspaceLabel(ws).slice(0, 3)}`);
  assert.ok(tag);
  const at = app.textAt('▶');
  assert.ok(at);
  app.pointerDown(at.x, at.y, 0);
  app.pointerUp();
  assert.equal(app.closedCount(ws.id), 0);
  assert.equal(agents(app), 1);
  assert.match(app.toast?.text ?? '', /1 agent reopened in /);
  app.destroy();
});

test('an emptied workspace says how many agents are closed and its button brings them back', () => {
  const app = demo();
  const ws = app.project()!.workspaces[0];
  ws.tabs = ws.tabs.filter((t) => t.panes.some((p) => paneStatus(p)));
  app.closeAgents(ws.id);
  assert.equal(ws.tabs.length, 0);
  app.render();
  assert.ok(app.textAt('1 agent closed'));
  const button = app.textAt('▶ reopen agent');
  assert.ok(button);
  app.pointerDown(button.x + 1, button.y, 0);
  app.pointerUp();
  assert.equal(ws.tabs.length, 1);
  app.destroy();
});

test('the toast reopens them, and forgetting asks first', () => {
  const app = demo();
  const ws = app.project()!.workspaces[0];
  app.closeAgents(ws.id);
  app.toast?.undo?.();
  assert.equal(app.closedCount(ws.id), 0);
  app.closeAgents(ws.id);
  app.askForgetAgents(ws.id);
  assert.equal(app.overlay?.kind, 'forgetAgents');
  app.submitConfirm();
  assert.equal(app.closedCount(ws.id), 0);
  assert.equal(agents(app), 0);
  app.destroy();
});

test('the keys menu closes and reopens the agents of the shown workspace', () => {
  const app = demo();
  const shown = app.workspace()!;
  app.config.prefix = 'ctrl+]';
  const press = (key: string, ctrl = false) => app.key({ key, ctrl, alt: false, shift: false });
  for (const key of ['w', 'a']) (key === 'w' ? () => (press(']', true), press('w')) : () => press('a'))();
  assert.equal(app.closedCount(shown.id), 1);
  press(']', true);
  press('w');
  press('o');
  assert.equal(app.closedCount(shown.id), 0);
  press(']', true);
  press('w');
  press('o');
  assert.equal(app.toast?.text, 'this workspace has no closed agents');
  app.destroy();
});
