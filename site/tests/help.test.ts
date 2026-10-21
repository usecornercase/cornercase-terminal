import assert from 'node:assert/strict';
import { test } from 'node:test';
import { App, BUG_URL, DOCS_URL } from '../src/lib/demo/app';
import { world } from '../src/lib/demo/world';

function demo(columns: number): App {
  const app = new App(); app.virtual = true; app.resize(columns, 38); world(app, { scripted: true });
  if (columns < 90) app.navTo('projects');
  app.render();
  return app;
}

function openHelp(app: App, columns: number): void {
  app.toast = null;
  if (columns < 90) app.navTo('projects');
  app.render();
  press(app, ' Help ');
}

function press(app: App, label: string): void {
  const at = app.textAt(label);
  assert.ok(at, `no ${label}`);
  app.pointerDown(at.x + 1, at.y, 0);
  app.pointerUp();
  app.render();
}

for (const columns of [140, 70]) test(`Help sits beside Quit and opens its menu at ${columns} columns`, () => {
  const app = demo(columns);
  const help = app.textAt(' Help ');
  const quit = app.textAt(' Quit ');
  assert.ok(help && quit);
  assert.equal(help.y, quit.y);
  assert.ok(help.x > quit.x);
  press(app, ' Help ');
  for (const item of ['tour', 'catalogue…', 'docs', 'report a bug']) assert.ok(app.textAt(item), item);
  const copied: string[] = [];
  app.on((event, detail) => { if (event === 'copy' && detail) copied.push(detail); });
  press(app, 'docs ');
  assert.deepEqual(copied, [DOCS_URL]);
  openHelp(app, columns);
  press(app, 'report a bug');
  assert.deepEqual(copied, [DOCS_URL, BUG_URL]);
  openHelp(app, columns);
  press(app, 'tour ');
  assert.ok(app.textAt('guided tour needs a real cornercase'));
  app.destroy();
});

test('with a prefix key, Help lists the keys menu and opens it', () => {
  const app = demo(140);
  app.config.prefix = 'ctrl+]';
  openHelp(app, 140);
  press(app, 'keys menu');
  assert.equal(app.overlay?.kind, 'keys');
  app.destroy();
});
