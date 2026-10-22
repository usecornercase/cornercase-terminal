import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';

import { App } from '../src/lib/demo/app';
import { tableText } from '../src/lib/demo/icons';
import { world } from '../src/lib/demo/world';

function demo(icons: string): App {
  const app = new App();
  app.virtual = true;
  app.scripted = true;
  app.resize(140, 34);
  world(app, { scripted: true });
  app.config.icons = icons;
  app.files.open = true;
  app.render();
  return app;
}

test('the table is the one the app draws from', () => {
  assert.equal(tableText(), readFileSync('../tests/fixtures/icons/table.txt', 'utf8'));
});

test('the browser and the site ship the same symbols subset', () => {
  const served = readFileSync('../src/web/assets/symbols.woff2');
  assert.ok(served.equals(readFileSync('src/assets/symbols.woff2')));
  assert.ok(readFileSync('../src/web/assets/symbols-LICENSE.txt').equals(readFileSync('src/assets/symbols-LICENSE.txt')));
});

test('the tree draws a folder icon with icons on auto and none with off', () => {
  assert.ok(demo('auto').textAt('\u{e5ff}'));
  assert.equal(demo('off').textAt('\u{e5ff}'), null);
});

test('the settings pick offers auto, on and off', () => {
  const app = demo('auto');
  assert.deepEqual(
    app.settingsRows(5).filter((row) => row.id === 'icons').map((row) => row.value),
    ['auto'],
  );
});
