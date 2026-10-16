import assert from 'node:assert/strict';
import { test } from 'node:test';
import { App } from '../src/lib/demo/app';
import { inputKey } from '../src/lib/demo/client';
import { projectLabel } from '../src/lib/demo/model';
import { world } from '../src/lib/demo/world';

const tab = { key: 'Tab', metaKey: false, isComposing: false, ctrlKey: false, altKey: false, shiftKey: false };

function started(columns = 140): App {
  const app = new App(); app.virtual = true; app.resize(columns, 44); world(app);
  app.advance(3000); app.render();
  return app;
}

function type(app: App, text: string): void {
  for (const ch of text) app.key({ key: ch, ctrl: false, alt: false, shift: false });
}

test('the search opens in names and Tab or the button switch it to sessions', () => {
  const app = started();
  app.openSearch(); app.render();
  assert.ok(app.textAt('projects, workspaces'));
  assert.equal(inputKey(app, tab), true);
  app.render();
  assert.equal(app.overlay?.kind === 'search' && app.overlay.mode, 'sessions');
  assert.ok(app.textAt('what was said'));
  const button = app.textAt('❯'); assert.ok(button);
  app.pointerDown(button.x, button.y, 0); app.pointerUp(); app.render();
  assert.equal(app.overlay?.kind === 'search' && app.overlay.mode, 'names');
  app.destroy();
});

test('sessions find what a pane printed, ignoring case, in two-line results', () => {
  const app = started();
  app.openSearch('sessions'); type(app, 'LISTENING ON'); app.render();
  const results = app.searchResults('listening on', 'sessions');
  assert.ok(results.length > 0);
  assert.ok(results.every((r) => r.detail?.endsWith('· screen') && r.context.includes(' › ')));
  const first = results[0];
  assert.ok(first.hit && first.name.slice(first.hit[0], first.hit[1]).toLowerCase() === 'listening on');
  assert.ok(app.textAt('listening on'));
  app.destroy();
});

test('choosing a session result shows its pane', () => {
  const app = started();
  app.openSearch('sessions'); type(app, 'listening on'); app.render();
  const [pick] = app.searchResults('listening on', 'sessions');
  assert.ok(pick);
  app.key({ key: 'Enter', ctrl: false, alt: false, shift: false });
  assert.equal(app.overlay, null);
  assert.ok(pick.context.startsWith(projectLabel(app.project()!)));
  app.destroy();
});

test('f s with a prefix key opens the search in sessions', () => {
  const app = started();
  app.config.prefix = 'ctrl+]';
  app.key({ key: ']', ctrl: true, alt: false, shift: false });
  app.key({ key: 'f', ctrl: false, alt: false, shift: false });
  app.key({ key: 's', ctrl: false, alt: false, shift: false });
  assert.equal(app.overlay?.kind === 'search' && app.overlay.mode, 'sessions');
  app.destroy();
});

test('a search with nothing to find says so and a compact window keeps the button', () => {
  const app = started(60);
  app.openSearch('sessions'); type(app, 'zzz-not-anywhere'); app.render();
  assert.ok(app.textAt('no matches'));
  assert.ok(app.textAt('❯'));
  app.destroy();
});

test('every occurrence on a line is a result, and the per-pane cap counts occurrences', () => {
  const app = started();
  const pane = app.projects[0].workspaces[0].tabs[0].panes[0];
  pane.shell.screen = () => 'foo foo foo\nbar foo';
  assert.equal(app.searchResults('foo', 'sessions').filter((r) => r.name === 'foo foo foo').length, 3);
  assert.equal(app.searchResults('foo', 'sessions').filter((r) => r.name === 'bar foo').length, 1);
  pane.shell.screen = () => Array.from({ length: 30 }, () => 'foo').join(' ');
  const mine = app.searchResults('foo', 'sessions').filter((r) => r.name.startsWith('foo foo foo foo foo foo foo foo foo foo foo foo foo foo foo foo foo foo foo foo foo'));
  assert.equal(mine.length, 20);
  assert.equal(app.saidCapped('foo'), true);
  app.destroy();
});

test('the arrows keep the selected session result in view', () => {
  const app = started();
  const pane = app.projects[0].workspaces[0].tabs[0].panes[0];
  pane.shell.screen = () => Array.from({ length: 15 }, (_, i) => `row ${i} foo`).join('\n');
  app.resize(140, 16);
  app.openSearch('sessions'); type(app, 'row'); app.render();
  const o = app.overlay;
  assert.ok(o?.kind === 'search');
  for (let i = 0; i < 12; i++) app.key({ key: 'ArrowDown', ctrl: false, alt: false, shift: false });
  assert.equal(o.selected, 12);
  assert.ok(o.scroll > 0);
  app.render();
  assert.ok(app.textAt('row 2 foo'));
  assert.ok(!app.textAt('row 14 foo'));
  for (let i = 0; i < 12; i++) app.key({ key: 'ArrowUp', ctrl: false, alt: false, shift: false });
  assert.equal(o.scroll, 0);
  app.destroy();
});

test('a narrow row crops around the match so it stays visible, and the hint counts what is listed', () => {
  const app = started();
  const pane = app.projects[0].workspaces[0].tabs[0].panes[0];
  pane.shell.screen = () => `${'a'.repeat(80)}NEEDLE${'b'.repeat(80)}`;
  app.openSearch('sessions'); type(app, 'needle'); app.render();
  assert.ok(app.textAt('NEEDLE'));
  pane.shell.screen = () => Array.from({ length: 21 }, () => 'needle').join('\n');
  app.render();
  assert.ok(app.textAt('the first 20 matches'));
  app.destroy();
});
