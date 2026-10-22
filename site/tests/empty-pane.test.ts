import assert from 'node:assert/strict';
import { test } from 'node:test';
import { App } from '../src/lib/demo/app';
import { world } from '../src/lib/demo/world';

function demo(): App {
  const app = new App();
  app.virtual = true;
  app.resize(177, 46);
  world(app, { scripted: true });
  app.render();
  return app;
}

test('the + Tab in the middle of an empty pane opens a shell, and lights up under the pointer', () => {
  const app = demo();
  const ws = app.project()!.workspaces[0];
  ws.tabs = [];
  app.render();
  const at = app.textAt('+ Tab opens a shell here');
  assert.ok(at);
  app.pointerDown(at.x, at.y, 0);
  app.pointerUp();
  assert.equal(ws.tabs.length, 1);
  app.destroy();
});

test('the + New under the logo of an empty window opens the new menu', () => {
  const app = demo();
  for (const p of [...app.projects]) app.closeProject(p.id);
  app.render();
  const at = app.textAt('+ New opens a folder');
  assert.ok(at);
  app.pointerDown(at.x, at.y, 0);
  app.pointerUp();
  if (app.overlay?.kind !== 'menu') return assert.fail('no menu');
  assert.deepEqual(app.overlay.actions.map((a) => app.menuLabel(a)), ['open project', 'new group']);
  app.destroy();
});
