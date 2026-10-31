import assert from 'node:assert/strict';
import { test } from 'node:test';
import { App } from '../src/lib/demo/app';
import { inputKey } from '../src/lib/demo/client';
import { world } from '../src/lib/demo/world';

const key = (k: string, ctrl = false) => ({ key: k, metaKey: false, isComposing: false, ctrlKey: ctrl, altKey: false, shiftKey: false });

test('o in the keys menu, away on chat apps, opens the Telegram bot row in Settings › Remote', () => {
  const app = new App(); app.virtual = true; app.resize(110, 38); world(app, { scripted: true });
  app.config.prefix = 'ctrl+]';
  inputKey(app, key(']', true));
  app.render();
  assert.ok(app.textAt('away on chat apps'));
  inputKey(app, key('o'));
  assert.equal(app.overlay?.kind, 'settings');
  if (app.overlay?.kind !== 'settings') return;
  assert.equal(app.settingsRows(app.overlay.page)[app.overlay.cursor].id, 'telegram');
  app.render();
  assert.ok(app.textAt('make a bot with @BotFather'));
  app.destroy();
});

test('the Remote tab holds the browser, Telegram and Discord, which need a real cornercase', () => {
  const app = new App(); app.virtual = true; app.resize(110, 38); world(app, { scripted: true });
  app.openSettings();
  inputKey(app, key('ArrowLeft'));
  inputKey(app, key('ArrowLeft'));
  assert.equal(app.overlay?.kind === 'settings' && app.overlay.page, 6);
  const rows = app.settingsRows(6);
  assert.deepEqual(rows.map((row) => row.section), ['Browser', 'Telegram', 'Discord']);
  assert.equal(app.settingsRows(1).some((row) => row.section === 'Telegram'), false);
  app.settingsClick(1);
  app.render();
  assert.ok(app.textAt('the demo has neither Tailscale nor a chat app'));
  if (app.overlay?.kind === 'settings') app.overlay.notice = undefined;
  app.settingsClick(2);
  app.render();
  assert.ok(app.textAt('the demo has neither Tailscale nor a chat app'));
  assert.ok(app.textAt('make a Discord app with User Install'));
  app.destroy();
});
