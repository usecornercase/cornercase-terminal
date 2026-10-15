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
