import assert from 'node:assert/strict';
import { test } from 'node:test';
import { App, PAGE, PAGE_NAMES } from '../src/lib/demo/app';
import { inputKey } from '../src/lib/demo/client';
import { tabsIn } from '../src/lib/demo/layout';
import { rect } from '../src/lib/term/grid';
import { world } from '../src/lib/demo/world';

const key = (k: string) => ({ key: k, metaKey: false, isComposing: false, ctrlKey: false, altKey: false, shiftKey: false });

test('the Machines tab is last and lists this computer, then + add machine, which needs a real cornercase', () => {
  const app = new App(); app.virtual = true; app.resize(110, 38); world(app, { scripted: true });
  app.openSettings();
  inputKey(app, key('ArrowLeft'));
  assert.equal(app.overlay?.kind === 'settings' && app.overlay.page, PAGE.machines);
  assert.equal(PAGE_NAMES[PAGE.machines], 'Machines');
  assert.deepEqual(app.settingsRows(PAGE.machines).map((row) => row.label), ['  ▭ this computer', '+ add machine']);
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
