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
  app.destroy();
});

test('the Worktrees tab lists pull requests, which need a real cornercase', () => {
  const app = new App(); app.virtual = true; app.resize(110, 38); world(app, { scripted: true });
  app.openSettings();
  assert.deepEqual(app.settingsRows(0).map((row) => row.id), ['folder', 'fetch', 'pulls', 'merged']);
  app.settingsClick(2);
  app.render();
  assert.ok(app.textAt('the demo has no GitHub'));
  app.settingsClick(3);
  app.render();
  assert.ok(app.textAt('the demo has no GitHub or GitLab'));
  app.destroy();
});

test('g p in the keys menu says pull requests need a real cornercase', () => {
  const app = new App(); app.virtual = true; app.resize(110, 38); world(app, { scripted: true });
  app.config.prefix = 'ctrl+]';
  inputKey(app, { ...key(']'), ctrlKey: true });
  inputKey(app, key('g'));
  app.render();
  assert.ok(app.textAt('pull request and its checks'));
  inputKey(app, key('p'));
  app.render();
  assert.equal(app.overlay, null);
  assert.ok(app.textAt('the demo has no GitHub'));
  app.destroy();
});

test('the star prompt is a switch on the UI tab', () => {
  const app = new App(); app.virtual = true; app.resize(110, 38); world(app, { scripted: true });
  app.openSettings();
  inputKey(app, key('ArrowLeft'));
  inputKey(app, key('ArrowLeft'));
  const index = app.settingsRows(3).findIndex((row) => row.id === 'starPrompt');
  assert.ok(index > 0);
  assert.equal(app.config.starPrompt, true);
  app.settingsClick(index);
  app.render();
  assert.equal(app.config.starPrompt, false);
  assert.ok(app.textAt('cornercase never asks for a star'));
  app.destroy();
});
