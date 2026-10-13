import assert from 'node:assert/strict';
import { test } from 'node:test';
import { App } from '../src/lib/demo/app';
import { inputKey } from '../src/lib/demo/client';
import { world } from '../src/lib/demo/world';

const key = (k: string, ctrl = false) => ({ key: k, metaKey: false, isComposing: false, ctrlKey: ctrl, altKey: false, shiftKey: false });

test('o in the keys menu says Telegram is not set up', () => {
  const app = new App(); app.virtual = true; app.resize(110, 38); world(app, { scripted: true });
  app.config.prefix = 'ctrl+]';
  inputKey(app, key(']', true));
  app.render();
  assert.ok(app.textAt('away on Telegram'));
  inputKey(app, key('o'));
  assert.equal(app.toast?.text, 'Telegram is not set up: pair a phone in Settings › Agents › Telegram');
  assert.equal(app.toast?.bug, true);
  app.destroy();
});

test('a bot token in Settings › Agents shows the phone row', () => {
  const app = new App(); app.virtual = true; app.resize(110, 38); world(app, { scripted: true });
  app.openSettings();
  app.settingsPage(1);
  const rows = () => app.settingsRows(1).map((row) => row.id);
  assert.equal(rows().at(-1), 'telegram');
  app.settingsClick(rows().indexOf('telegram'));
  for (const c of '1:t') inputKey(app, key(c));
  inputKey(app, key('Enter'));
  app.advance(1000);
  assert.deepEqual(rows().slice(-2), ['telegram', 'telegram-phone']);
  app.render();
  assert.ok(app.textAt('@cornercase_bot'));
  app.destroy();
});
