import assert from 'node:assert/strict';
import { test } from 'node:test';
import { App } from '../src/lib/demo/app';
import { inputKey } from '../src/lib/demo/client';
import { world } from '../src/lib/demo/world';

const key = (k: string) => ({ key: k, metaKey: false, isComposing: false, ctrlKey: false, altKey: false, shiftKey: false });

test('the UI tab lists the icon bar, which needs a real cornercase', () => {
  const app = new App(); app.virtual = true; app.resize(110, 38); world(app, { scripted: true });
  app.openSettings();
  inputKey(app, key('ArrowLeft'));
  inputKey(app, key('ArrowLeft'));
  assert.equal(app.overlay?.kind === 'settings' && app.overlay.page, 3);
  const rows = app.settingsRows(3);
  assert.deepEqual(rows.slice(0, 3).map((row) => row.id), ['sidebar', 'tabs', 'iconBar']);
  app.settingsClick(2);
  app.render();
  assert.ok(app.textAt('the demo has no Nerd Font'));
  const sound = app.settingsRows(3).findIndex((row) => row.id === 'sound');
  assert.equal(app.settingsRows(3)[sound - 1]?.id, 'notify');
  assert.deepEqual(app.settingsRows(3).slice(sound, sound + 3).map((row) => row.label), ['sound', 'needs you', 'finished']);
  app.settingsClick(sound);
  app.render();
  inputKey(app, key('ArrowDown'));
  inputKey(app, key('Enter'));
  assert.equal(app.config.sound, 'on');
  app.destroy();
});
