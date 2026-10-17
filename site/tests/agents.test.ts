import assert from 'node:assert/strict';
import { test } from 'node:test';
import { App } from '../src/lib/demo/app';
import { AGENTS, MODES } from '../src/lib/demo/data';
import { attention, followAgent, tabStatus, watchPane } from '../src/lib/demo/model';
import { AGENT_KINDS } from '../src/lib/demo/programs';
import { world } from '../src/lib/demo/world';
import { Painter } from '../src/lib/demo/ui';

test('Grok is in the agent catalogue, its modes and the program list', () => {
  assert.deepEqual(AGENTS.find(([kind]) => kind === 'grok'), ['grok', 'grok']);
  assert.deepEqual(MODES.grok, [
    ['accept edits', '--permission-mode acceptEdits'],
    ['auto', '--permission-mode auto'],
    ['always approve (dangerous)', '--always-approve'],
  ]);
  assert.equal(AGENT_KINDS.grok, 'grok');
});

test('Cursor is in the agent catalogue, its modes and the program list', () => {
  assert.deepEqual(AGENTS.find(([kind]) => kind === 'cursor'), ['cursor', 'cursor-agent']);
  assert.deepEqual(MODES.cursor, [
    ['plan', '--mode plan'],
    ['ask', '--mode ask'],
    ['auto review', '--auto-review'],
    ['run everything (dangerous)', '--force'],
  ]);
  assert.equal(AGENT_KINDS['cursor-agent'], 'cursor');
});

test('Settings lists Cursor with its modes', () => {
  const app = new App();
  app.virtual = true;
  app.resize(110, 40);
  world(app, { scripted: true });
  app.openSettings();
  app.settingsPage(1);
  const cursor = app.settingsRows(1).findIndex((row) => row.id === 'kind:cursor');
  assert.ok(cursor >= 0);
  app.settingsClick(cursor);
  const modes = app.overlay?.kind === 'settings' ? app.overlay.pick : undefined;
  assert.deepEqual(
    modes?.items.map((item) => item.value),
    ['default', 'plan', 'ask', 'auto review', 'run everything (dangerous)', 'extra arguments…'],
  );
  app.destroy();
});

test('Settings lists Grok with its modes and the agent picker offers it', () => {
  const app = new App();
  app.virtual = true;
  app.resize(110, 40);
  world(app, { scripted: true });
  app.openSettings();
  app.settingsPage(1);
  const rows = app.settingsRows(1);
  const grok = rows.findIndex((row) => row.id === 'kind:grok');
  assert.ok(grok >= 0);
  assert.equal(rows[grok].label, 'grok');
  app.settingsClick(grok);
  const modes = app.overlay?.kind === 'settings' ? app.overlay.pick : undefined;
  assert.ok(modes);
  assert.deepEqual(
    modes.items.map((item) => item.value),
    ['default', 'accept edits', 'auto', 'always approve (dangerous)', 'extra arguments…'],
  );
  app.closeOverlay();
  app.openSettings();
  app.settingsPage(1);
  const agent = app.settingsRows(1).findIndex((row) => row.id === 'agent');
  app.settingsClick(agent);
  const picker = app.overlay?.kind === 'settings' ? app.overlay.pick : undefined;
  assert.ok(picker?.items.some((item) => item.value === 'grok' && item.note === 'grok'));
  app.destroy();
});

test('A working agent shows the moon, and turns once the spinner is picked in settings', () => {
  const app = new App();
  app.virtual = true;
  app.resize(140, 40);
  world(app);
  const icon = (): string | undefined => {
    app.render();
    return [...'◐⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏'].find((glyph) => app.textAt(`├ ${glyph} claude`) !== null);
  };
  const moon = [icon()];
  app.advance(100);
  moon.push(icon());
  app.openSettings();
  app.settingsPage(3);
  app.settingsClick(app.settingsRows(3).findIndex((row) => row.id === 'working'));
  const pick = app.overlay?.kind === 'settings' ? app.overlay.pick : undefined;
  assert.deepEqual(pick?.items.map((item) => item.value), ['moon', 'spinner']);
  app.choosePick(1);
  assert.equal(app.config.workingIcon, 'spinner');
  app.closeOverlay();
  const frames = [icon()];
  app.advance(100);
  frames.push(icon());
  assert.deepEqual(moon, ['◐', '◐']);
  assert.ok(frames.every((frame) => frame && frame !== '◐') && frames[0] !== frames[1], frames.join(' '));
  app.destroy();
});

test('A tab marked as pending review keeps its ● until it is marked as reviewed', () => {
  const app = new App();
  app.virtual = true;
  app.resize(177, 46);
  world(app);
  const places = app.projects.flatMap((p) => p.workspaces.flatMap((w) => w.tabs.map((t) => ({ p, w, t }))));
  const place = places[0];
  assert.ok(place);
  followAgent(place.t.panes[0], 'claude');
  watchPane(place.t.panes[0], 'idle', true, 0);
  const target = { kind: 'tab', project: place.p.id, workspace: place.w.id, tab: place.t.id } as const;
  const labels = () => (app.overlay?.kind === 'menu' ? app.overlay.actions.map((a) => app.menuLabel(a)) : []);
  const pick = (label: string) => app.chooseMenu(labels().indexOf(label));

  app.openMenu({ x: 1, y: 1 }, target);
  assert.deepEqual(labels(), ['rename tab', 'mark as pending review']);
  pick('mark as pending review');
  assert.equal(tabStatus(place.t), 'pending');
  watchPane(place.t.panes[0], 'idle', true, 1);
  assert.equal(tabStatus(place.t), 'pending');

  app.openMenu({ x: 1, y: 1 }, target);
  pick('mark as reviewed');
  assert.equal(tabStatus(place.t), 'idle');
  app.destroy();
});

test('An agent row in the agents section offers marking its pane', () => {
  const app = new App();
  app.virtual = true;
  app.resize(177, 46);
  world(app);
  const pane = app.projects[0].workspaces[0].tabs[0].panes[0];
  followAgent(pane, 'claude');
  watchPane(pane, 'idle', true, 0);
  app.openAgentMenu({ x: 1, y: 1 }, pane.id);
  const labels = app.overlay?.kind === 'menu' ? app.overlay.actions.map((a) => app.menuLabel(a)) : [];
  assert.deepEqual(labels, ['mark as pending review']);
  app.chooseMenu(0);
  assert.equal(tabStatus(app.projects[0].workspaces[0].tabs[0]), 'pending');
  app.destroy();
});

test('Reported idle does not guess a finish, and explicit done stays acknowledged', () => {
  const app = new App();
  app.virtual = true;
  world(app, { scripted: true });
  const tab = app.projects[0].workspaces[0].tabs[0];
  const pane = tab.panes[0];
  followAgent(pane, 'claude');
  watchPane(pane, 'working', false, 0);
  watchPane(pane, 'ready', false, 1);
  assert.equal(tabStatus(tab), 'idle');
  watchPane(pane, 'done', false, 2);
  assert.equal(tabStatus(tab), 'done');
  watchPane(pane, 'done', true, 3);
  watchPane(pane, 'done', false, 4);
  assert.equal(tabStatus(tab), 'idle');
  app.destroy();
});

test('A failed agent has a red mark and reaches the attention rollup', () => {
  const app = new App();
  app.virtual = true;
  app.resize(140, 40);
  world(app, { scripted: true });
  const pane = app.projects[0].workspaces[0].tabs[0].panes[0];
  followAgent(pane, 'claude');
  const { grid } = app.render();
  followAgent(pane, 'claude');
  watchPane(pane, 'failed', false, 0);
  grid.reset();
  new Painter(app, grid).draw();
  assert.equal(attention(['done', 'pending', 'failed']), 'failed');
  assert.equal(attention(['failed', 'waiting']), 'waiting');
  assert.ok(grid.cells.some((cell) => cell.ch === '!' && cell.fg === 1));
  assert.equal(watchPane(pane, 'failed', false, 1000), 'failed');
  app.destroy();
});

test('An agent toast has a go that shows its tab, and lasts six seconds', () => {
  const app = new App();
  app.virtual = true;
  app.resize(177, 46);
  world(app);
  let toast = app.toast;
  for (let t = 0; t < 120000 && !toast; t += 250) {
    app.advance(250);
    app.render();
    toast = app.toast;
  }
  assert.ok(toast?.go, `a toast with a go: ${JSON.stringify(toast)}`);
  assert.ok(toast.until - app.now() > 2000);
  app.render();
  const before = app.tab();
  const at = app.textAt(' go ');
  assert.ok(at, 'the go button');
  app.pointerDown(at.x + 1, at.y, 0);
  app.pointerUp();
  assert.equal(app.toast, null);
  assert.notEqual(app.tab(), before);
  app.destroy();
});
