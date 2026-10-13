import assert from 'node:assert/strict';
import { test } from 'node:test';
import { App } from '../src/lib/demo/app';
import { AGENTS, MODES } from '../src/lib/demo/data';
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
