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

test('the Asana workspace row names the workspace with its saved ID in the note, and forgets the name on an edit', () => {
  const app = new App(); app.virtual = true; app.resize(110, 38); world(app, { scripted: true });
  app.config.asanaWorkspace = '1200000000000001';
  app.openSettings();
  app.settingsPage(2);
  const row = () => app.settingsRows(2).find((r) => r.id === 'asana-workspace');
  const token = () => app.settingsRows(2).find((r) => r.id === 'token:asana');
  assert.deepEqual([row()?.value, row()?.note], ['1200000000000001', 'name or ID']);
  app.config.accounts.asana = true;
  app.config.asanaWorkspaceName = 'Acme';
  assert.deepEqual([row()?.value, row()?.note, token()?.value], ['Acme', 'ID 1200000000000001', '@you in Acme']);
  app.settingsClick(app.settingsRows(2).findIndex((r) => r.id === 'asana-workspace'));
  for (let i = 0; i < 16; i++) inputKey(app, key('Backspace'));
  for (const ch of '1200000000000002') inputKey(app, key(ch));
  inputKey(app, key('Enter'));
  assert.deepEqual([row()?.value, row()?.note, token()?.value], ['1200000000000002', 'name or ID', 'connected']);
  app.destroy();
});

test('connecting Asana names only the demo workspace, whatever ID is saved', () => {
  const connect = (saved: string) => {
    const app = new App(); app.virtual = true; app.resize(110, 38); world(app, { scripted: true });
    app.config.asanaWorkspace = saved;
    app.openIssues();
    app.advance(500);
    app.issuesTab(app.config.sources.indexOf('asana'));
    for (const ch of 'tok') inputKey(app, key(ch));
    inputKey(app, key('Enter'));
    app.advance(1500);
    app.closeOverlay();
    app.openSettings();
    app.settingsPage(2);
    const row = app.settingsRows(2).find((r) => r.id === 'asana-workspace');
    app.destroy();
    return [row?.value, row?.note];
  };
  assert.deepEqual(connect(''), ['Acme', 'name or ID']);
  assert.deepEqual(connect('1200'), ['Acme', 'ID 1200']);
  assert.deepEqual(connect('acme'), ['Acme', 'name or ID']);
  assert.deepEqual(connect('1300'), ['1300', 'name or ID']);
});
