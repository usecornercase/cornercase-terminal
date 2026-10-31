import assert from 'node:assert/strict';
import { test } from 'node:test';
import { App } from '../src/lib/demo/app';
import { inputKey } from '../src/lib/demo/client';
import { world } from '../src/lib/demo/world';

const key = (k: string) => ({ key: k, metaKey: false, isComposing: false, ctrlKey: false, altKey: false, shiftKey: false });

test('the UI tab lists the icons choice and the icon bar, which needs a real cornercase', () => {
  const app = new App(); app.virtual = true; app.resize(110, 38); world(app, { scripted: true });
  app.openSettings();
  inputKey(app, key('ArrowLeft'));
  inputKey(app, key('ArrowLeft'));
  inputKey(app, key('ArrowLeft'));
  assert.equal(app.overlay?.kind === 'settings' && app.overlay.page, 5);
  const rows = app.settingsRows(5);
  assert.deepEqual(rows.slice(0, 4).map((row) => row.id), ['sidebar', 'tabs', 'iconBar', 'icons']);
  app.settingsClick(2);
  app.render();
  assert.ok(app.textAt('this needs cornercase in your terminal'));
  app.destroy();
});

test('the Worktrees tab lists pull requests, which need a real cornercase', () => {
  const app = new App(); app.virtual = true; app.resize(110, 38); world(app, { scripted: true });
  app.openSettings();
  assert.deepEqual(app.settingsRows(0).map((row) => row.id), ['folder', 'branches', 'fetch', 'pulls', 'merged']);
  app.settingsClick(3);
  app.render();
  assert.ok(app.textAt('the demo has no GitHub'));
  app.settingsClick(4);
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

test('the UI tab lists the sound rows after desktop notifications, which need a real cornercase', () => {
  const app = new App(); app.virtual = true; app.resize(110, 38); world(app, { scripted: true });
  app.openSettings();
  app.settingsPage(5);
  const ids = app.settingsRows(5).map((row) => row.id);
  const at = ids.indexOf('notify');
  assert.deepEqual(ids.slice(at, at + 5), ['notify', 'sound', 'soundWaiting', 'soundDone', 'soundSubagents']);
  const value = (id: string) => app.settingsRows(5).find((row) => row.id === id)?.value;
  assert.equal(value('sound'), '[x] on');
  assert.equal(value('soundSubagents'), '[ ] off');
  for (const id of ['sound', 'soundWaiting', 'soundDone', 'soundSubagents']) {
    app.settingsClick(ids.indexOf(id));
    app.render();
    assert.ok(app.textAt('the demo plays no sound'));
  }
  app.destroy();
});

test('the star prompt is a switch on the UI tab', () => {
  const app = new App(); app.virtual = true; app.resize(110, 38); world(app, { scripted: true });
  app.openSettings();
  inputKey(app, key('ArrowLeft'));
  inputKey(app, key('ArrowLeft'));
  inputKey(app, key('ArrowLeft'));
  const index = app.settingsRows(5).findIndex((row) => row.id === 'starPrompt');
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
  app.settingsPage(4);
  const row = () => app.settingsRows(4).find((r) => r.id === 'asana-workspace');
  const token = () => app.settingsRows(4).find((r) => r.id === 'token:asana');
  assert.deepEqual([row()?.value, row()?.note], ['1200000000000001', 'name or ID']);
  app.config.accounts.asana = true;
  app.config.asanaWorkspaceName = 'Acme';
  assert.deepEqual([row()?.value, row()?.note, token()?.value], ['Acme', 'ID 1200000000000001', '@you in Acme']);
  app.settingsClick(app.settingsRows(4).findIndex((r) => r.id === 'asana-workspace'));
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
    app.settingsPage(4);
    const row = app.settingsRows(4).find((r) => r.id === 'asana-workspace');
    app.destroy();
    return [row?.value, row?.note];
  };
  assert.deepEqual(connect(''), ['Acme', 'name or ID']);
  assert.deepEqual(connect('1200'), ['Acme', 'ID 1200']);
  assert.deepEqual(connect('acme'), ['Acme', 'name or ID']);
  assert.deepEqual(connect('1300'), ['1300', 'name or ID']);
});

test('the Accounts tab and the account switches need a real cornercase, and nothing else shows them', () => {
  const app = new App(); app.virtual = true; app.resize(140, 40); world(app, { scripted: true });
  app.openSettings();
  inputKey(app, key('ArrowRight'));
  inputKey(app, key('ArrowRight'));
  assert.equal(app.overlay?.kind === 'settings' && app.overlay.page, 2);
  app.render();
  assert.ok(app.textAt('Accounts'));
  const rows = app.settingsRows(2);
  const agents = ['Claude Code', 'Codex', 'Gemini CLI'];
  assert.deepEqual(rows.map((row) => row.section), agents.flatMap((agent) => [agent, agent, agent]));
  assert.deepEqual(rows.map((row) => row.label.trim()), agents.flatMap(() => ['● System', 'rename…', '+ add account']));
  assert.ok(!rows.some((row) => row.section === 'Profiles' || row.label.includes('profile')));
  for (let i = 0; i < rows.length; i++) {
    if (app.overlay?.kind === 'settings') app.overlay.notice = undefined;
    app.settingsClick(i);
    app.render();
    assert.ok(app.textAt('the demo has no agent accounts'));
  }
  const ui = app.settingsRows(5).map((row) => row.id);
  const at = ui.indexOf('memory');
  assert.deepEqual(ui.slice(at + 1, at + 4), ['account', 'accountSwitcher', 'accountPicker']);
  app.settingsPage(5);
  for (const id of ['account', 'accountSwitcher', 'accountPicker']) {
    if (app.overlay?.kind === 'settings') app.overlay.notice = undefined;
    app.settingsClick(app.settingsRows(5).findIndex((row) => row.id === id));
    app.render();
    assert.ok(app.textAt('the demo has no agent accounts'));
  }
  app.closeOverlay();
  app.render();
  assert.ok(!app.textAt('System ▾'));
  assert.ok(!app.textAt('Profile:'));
  assert.ok(!app.textAt('Accounts ▾'));
  assert.ok(!app.textAt('claude: System'));
  app.destroy();
});

test('share sits before restart on every settings tab and says it needs cornercase on your computer', () => {
  const app = new App(); app.virtual = true; app.resize(110, 38); world(app, { scripted: true });
  app.openSettings();
  for (const page of [0, 3, 5]) {
    app.settingsPage(page);
    app.render();
    const share = app.textAt(' share ');
    const restart = app.textAt(' restart ');
    assert.ok(share && restart);
    assert.equal(share.y, restart.y);
    assert.ok(share.x < restart.x);
  }
  const share = app.textAt(' share ');
  assert.ok(share);
  app.pointerDown(share.x + 1, share.y, 0);
  app.pointerUp();
  app.render();
  assert.equal(app.toast?.text, 'this needs cornercase on your computer: the demo has no settings to copy or import');
  assert.equal(app.overlay?.kind, 'settings');
  app.destroy();
});

test('typing when the settings open searches every tab by the start of each word', () => {
  const app = new App(); app.virtual = true; app.resize(110, 38); world(app, { scripted: true });
  app.openSettings();
  for (const ch of 'def ag') inputKey(app, key(ch));
  app.render();
  assert.ok(app.textAt('Agents › default agent'));
  assert.ok(app.textAt('Worktrees 0'));
  assert.equal(app.settingsHits('ose').length, 0);
  inputKey(app, key('Enter'));
  const o = app.overlay;
  assert.ok(o?.kind === 'settings');
  assert.equal(o.page, 1);
  assert.equal(app.settingsRows(1)[o.cursor].label, 'default agent');
  app.destroy();
});

test('esc clears the settings search first, then closes', () => {
  const app = new App(); app.virtual = true; app.resize(110, 38); world(app, { scripted: true });
  app.openSettings();
  inputKey(app, key('x'));
  inputKey(app, key('Escape'));
  assert.equal(app.overlay?.kind, 'settings');
  inputKey(app, key('Escape'));
  assert.equal(app.overlay, null);
  app.destroy();
});
