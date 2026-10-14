import assert from 'node:assert/strict';
import { test } from 'node:test';
import { App } from '../src/lib/demo/app';
import { inputKey } from '../src/lib/demo/client';
import { world } from '../src/lib/demo/world';

const key = (k: string) => ({ key: k, metaKey: false, isComposing: false, ctrlKey: false, altKey: false, shiftKey: false });

test('the Plugins tab is last and says how to add one and that plugins run as you', () => {
  const app = new App(); app.virtual = true; app.resize(110, 38); world(app, { scripted: true });
  app.openSettings();
  inputKey(app, key('ArrowLeft'));
  assert.equal(app.overlay?.kind === 'settings' && app.overlay.page, 5);
  app.render();
  assert.ok(app.textAt('Plugins'));
  assert.ok(app.textAt('no plugins yet'));
  assert.ok(app.textAt('plugins run as you'));
  inputKey(app, key('Enter'));
  app.render();
  assert.ok(app.textAt('install one from the catalogue below'));
  inputKey(app, key('Tab'));
  assert.equal(app.overlay?.kind === 'settings' && app.overlay.page, 0);
  app.destroy();
});

test('Available shows the catalogue row, which lists nothing until it is opened', () => {
  const app = new App(); app.virtual = true; app.resize(110, 38); world(app, { scripted: true });
  app.openSettings();
  inputKey(app, key('ArrowLeft'));
  app.render();
  assert.ok(app.textAt('Available'));
  inputKey(app, key('ArrowDown'));
  app.render();
  assert.ok(app.textAt('plugins reviewed for cornercase'));
  inputKey(app, key('Enter'));
  app.render();
  assert.ok(app.textAt('no plugins listed yet'));
  assert.ok(app.textAt('refresh'));
  app.destroy();
});
