import assert from 'node:assert/strict';
import { test } from 'node:test';
import { App } from '../src/lib/demo/app';
import { AGENTS, MODES } from '../src/lib/demo/data';
import { followAgent, tabStatus, watchPane } from '../src/lib/demo/model';
import { AGENT_KINDS } from '../src/lib/demo/programs';
import { world } from '../src/lib/demo/world';

test('Grok is in the agent catalogue, its modes and the program list', () => {
  assert.deepEqual(AGENTS.find(([kind]) => kind === 'grok'), ['grok', 'grok']);
  assert.deepEqual(MODES.grok, [
    ['accept edits', '--permission-mode acceptEdits'],
    ['auto', '--permission-mode auto'],
    ['always approve (dangerous)', '--always-approve'],
  ]);
  assert.equal(AGENT_KINDS.grok, 'grok');
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
