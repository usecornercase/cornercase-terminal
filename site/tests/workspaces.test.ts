import assert from 'node:assert/strict';
import { test } from 'node:test';
import { App } from '../src/lib/demo/app';
import { workspaceLabel } from '../src/lib/demo/model';
import type { Key } from '../src/lib/demo/programs';
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

for (const target of ['project', 'workspace'] as const) {
  test(`a ${target} menu offers open in…, which needs a real cornercase`, () => {
    const app = demo();
    const p = app.project()!;
    const workspace = p.workspaces[0].id;
    app.openMenu({ x: 2, y: 2 }, target === 'project' ? { kind: 'project', project: p.id } : { kind: 'workspace', project: p.id, workspace });
    if (app.overlay?.kind !== 'menu') return assert.fail('no menu');
    const at = app.overlay.actions.findIndex((a) => a.kind === 'openIn');
    assert.equal(app.menuLabel(app.overlay.actions[at]), 'open in…');
    app.chooseMenu(at);
    app.render();
    assert.ok(app.textAt('the demo opens no other apps'));
    app.destroy();
  });
}

test('a workspace menu ends with ports after open in…, which need a real cornercase', () => {
  const app = demo();
  const p = app.project()!;
  app.openMenu({ x: 2, y: 2 }, { kind: 'workspace', project: p.id, workspace: p.workspaces[0].id });
  if (app.overlay?.kind !== 'menu') return assert.fail('no menu');
  const labels = app.overlay.actions.map((a) => app.menuLabel(a));
  assert.deepEqual(labels.slice(-2), ['open in…', 'ports']);
  app.chooseMenu(labels.length - 1);
  app.render();
  assert.ok(app.textAt('the demo runs no servers'));
  app.destroy();
});

test('a tab menu has no open in…', () => {
  const app = demo();
  const p = app.project()!;
  const w = p.workspaces[0];
  app.openMenu({ x: 2, y: 2 }, { kind: 'tab', project: p.id, workspace: w.id, tab: w.tabs[0].id });
  if (app.overlay?.kind !== 'menu') return assert.fail('no menu');
  assert.ok(!app.overlay.actions.some((a) => a.kind === 'openIn'));
  app.destroy();
});

test('the new workspace name moves its cursor with the arrows, Ctrl+A and Ctrl+E', () => {
  const app = demo();
  app.openNewWorkspace(0);
  for (const ch of 'fix login') app.key({ key: ch } as Key);
  app.key({ key: 'ArrowLeft', alt: true } as Key);
  for (const ch of 'the ') app.key({ key: ch } as Key);
  app.key({ key: 'a', ctrl: true } as Key);
  app.key({ key: '-' } as Key);
  app.key({ key: 'e', ctrl: true } as Key);
  app.key({ key: '!' } as Key);
  const o = app.overlay;
  assert.ok(o?.kind === 'newWorkspace');
  assert.equal(o.input, '-fix the login!');
  app.paste('x');
  assert.equal(o.input, '-fix the login!x');
  app.key({ key: 'a', ctrl: true } as Key);
  app.paste('x');
  assert.equal(o.input, 'x-fix the login!x');
  app.destroy();
});

test('a long workspace name shows the wrapped line with the cursor, and the arrows move between them', () => {
  const app = demo();
  app.openNewWorkspace(0);
  app.paste(`start ${'mid '.repeat(30)}finish`);
  app.key({ key: 'a', ctrl: true } as Key);
  app.render();
  const field = app.textAt('› start');
  assert.ok(field);
  const row = () => app.render().grid.line(field.y);
  assert.ok(!row().includes('finish'));
  app.key({ key: 'ArrowDown' } as Key);
  app.render();
  assert.ok(row().includes('› mid'));
  const o = app.overlay;
  assert.ok(o?.kind === 'newWorkspace' && o.cursor! > 0 && o.cursor! < o.input.length);
  app.key({ key: 'e', ctrl: true } as Key);
  app.render();
  assert.ok(row().includes('finish'));
  app.destroy();
});
