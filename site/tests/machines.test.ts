import assert from 'node:assert/strict';
import { test } from 'node:test';
import { App, PAGE, PAGE_NAMES } from '../src/lib/demo/app';
import { inputKey } from '../src/lib/demo/client';
import { tabsIn } from '../src/lib/demo/layout';
import { rect } from '../src/lib/term/grid';
import { memoryShown, parseSize } from '../src/lib/demo/text';
import { world } from '../src/lib/demo/world';

const key = (k: string) => ({ key: k, metaKey: false, isComposing: false, ctrlKey: false, altKey: false, shiftKey: false });

test('the Machines tab is last and lists this computer, then + add machine, which needs a real cornercase, then the free memory a start needs', () => {
  const app = new App(); app.virtual = true; app.resize(110, 38); world(app, { scripted: true });
  app.openSettings();
  inputKey(app, key('ArrowLeft'));
  assert.equal(app.overlay?.kind === 'settings' && app.overlay.page, PAGE.machines);
  assert.equal(PAGE_NAMES[PAGE.machines], 'Machines');
  assert.deepEqual(app.settingsRows(PAGE.machines).map((row) => [row.section, row.label]), [['', '  ▭ this computer'], ['', '+ add machine'], ['Free memory', 'a start needs']]);
  app.render();
  assert.ok(app.textAt('this computer runs the server you are using'));

  app.settingsClick(0);
  app.render();
  assert.equal(app.overlay?.kind === 'settings' && app.overlay.notice, undefined);

  app.settingsClick(1);
  app.render();
  assert.ok(app.textAt('the demo has no other machines to reach'));
  app.destroy();
});

test('a click on the Machines tab opens it, as the other tabs do', () => {
  const app = new App(); app.virtual = true; app.resize(110, 38); world(app, { scripted: true });
  app.openSettings();
  app.render();
  const tab = app.textAt(' Machines ');
  assert.ok(tab);
  app.pointerDown(tab.x + 1, tab.y, 0);
  app.pointerUp();
  assert.equal(app.overlay?.kind === 'settings' && app.overlay.page, PAGE.machines);
  app.destroy();
});

test('tabs that do not fit drop their gaps, then their padding, as the app does', () => {
  const last = (width: number) => {
    const r = tabsIn(rect(0, 0, width, 1), PAGE_NAMES).at(-1);
    return r && [r.x, r.w];
  };

  assert.deepEqual([last(90), last(80), last(72)], [[73, 10], [65, 10], [57, 8]]);
});

test('this computer says its free memory after the demo, as the app says it after its version', () => {
  const app = new App(); app.virtual = true; app.resize(110, 38); world(app, { scripted: true });
  app.openSettings();
  app.settingsPage(PAGE.machines);
  app.render();
  assert.ok(app.textAt('the demo, in your browser · 6 GB free of 16 GB'));
  app.destroy();
});

test('a start needs shows 4GB and its note, as the app draws the Free memory section', () => {
  const app = new App(); app.virtual = true; app.resize(110, 38); world(app, { scripted: true });
  app.openSettings();
  app.settingsPage(PAGE.machines);
  app.render();
  const header = app.textAt('Free memory');
  const row = app.textAt('a start needs');
  assert.ok(header && row && row.y === header.y + 1);
  assert.ok(app.textAt('4GB  what auto and the queue count on, unless a flow says'));
  app.destroy();
});

test('a start needs takes an amount, refuses what is not one and keeps it as typed', () => {
  const app = new App(); app.virtual = true; app.resize(110, 38); world(app, { scripted: true });
  app.openSettings();
  app.settingsPage(PAGE.machines);
  const at = app.settingsRows(PAGE.machines).findIndex((row) => row.id === 'defaultMemory');
  const type = (text: string) => {
    app.settingsClick(at);
    for (let i = 0; i < 12; i++) inputKey(app, key('Backspace'));
    for (const ch of text) inputKey(app, key(ch));
    inputKey(app, key('Enter'));
    app.render();
  };

  type('3 gigs');
  assert.ok(app.textAt('write an amount such as 4GB or 512MB'));
  assert.equal(app.config.defaultMemory, '4GB');

  inputKey(app, key('Escape'));
  type('1.5GB');
  assert.equal(app.config.defaultMemory, '1.5GB');
  assert.ok(app.textAt('a start that names no memory needs 1.5 GB'));
  assert.equal(app.settingsRows(PAGE.machines)[at].value, '1.5GB');
  app.destroy();
});

test('amounts of memory are read and shown as the app reads and shows them', () => {
  const GB = 1024 ** 3;
  const read = ['3GB', '512 MB', '4gb', '2GiB', '8G', '1.5GB', '1024', '1TB', '0GB', 'GB', '1.2.3GB', 'three GB', '3 gigs', '-3GB'].map(parseSize);
  assert.deepEqual(read, [3 * GB, 512 * 1024 ** 2, 4 * GB, 2 * GB, 8 * GB, 1.5 * GB, 1024, 1024 * GB, null, null, null, null, null, null]);
  assert.deepEqual([32 * GB, 9.2 * GB, 6 * GB, 512 * 1024 ** 2].map(memoryShown), ['32 GB', '9.2 GB', '6 GB', '512 MB']);
});
