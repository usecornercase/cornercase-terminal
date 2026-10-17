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
  assert.equal(app.overlay?.kind === 'settings' && app.overlay.page, 6);
  app.render();
  assert.ok(app.textAt('Plugins'));
  assert.ok(app.textAt('no plugins yet'));
  assert.ok(app.textAt('plugins run as you'));
  inputKey(app, key('Enter'));
  app.render();
  assert.ok(app.textAt('`cornercase plugin install <owner/repo>` adds one'));
  inputKey(app, key('Tab'));
  assert.equal(app.overlay?.kind === 'settings' && app.overlay.page, 0);
  app.destroy();
});
