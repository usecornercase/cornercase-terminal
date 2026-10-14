import assert from 'node:assert/strict';
import { test } from 'node:test';
import { App } from '../src/lib/demo/app';
import { inputKey } from '../src/lib/demo/client';
import { type Flow, Progress } from '../src/lib/demo/flow';
import { FULL_ISSUE, RUN_BRANCH } from '../src/lib/demo/flowrun';
import { keysItems } from '../src/lib/demo/keys';
import { workspaceLabel } from '../src/lib/demo/model';
import { world } from '../src/lib/demo/world';

const key = (k: string, ctrl = false) => ({ key: k, metaKey: false, isComposing: false, ctrlKey: ctrl, altKey: false, shiftKey: false });

function demo(columns = 140, rows = 46): App {
  const app = new App();
  app.virtual = true;
  app.resize(columns, rows);
  world(app, { scripted: true });
  return app;
}

function toRun(app: App): void {
  const p = app.projects.findIndex((x) => x.folder === 'payments');
  app.selectProject(p);
  app.selectWorkspace(p, app.projects[p].workspaces.findIndex((w) => workspaceLabel(w) === RUN_BRANCH));
  app.render();
}

function shown(app: App, needle: string): boolean {
  app.render();
  return app.textAt(needle) !== null;
}

function playUntil(app: App, needle: string, limit = 240000): void {
  for (let t = 0; t < limit && !shown(app, needle); t += 250) app.advance(250);
  assert.ok(shown(app, needle), `never showed ${needle}`);
}

const send = (p: Progress, flow: Flow, to: string, at: number) => p.send(flow, { from: 'orchestrator', to, text: `to ${to}`, at });

test('progress follows the plan: next stage, a loop round, a step outside the plan', () => {
  const p = new Progress();
  const last = () => p.last;
  send(p, FULL_ISSUE, 'coder', 0);
  assert.equal(last(), null);
  send(p, FULL_ISSUE, 'reviewer', 1000);
  assert.deepEqual(last()?.edge, { kind: 'forward', stage: 0 });
  send(p, FULL_ISSUE, 'security', 2000);
  assert.equal(last()?.to, 'security');
  send(p, FULL_ISSUE, 'coder', 3000);
  assert.deepEqual([last()?.edge, p.rounds(1)], [{ kind: 'loop', stage: 1 }, 1]);
  send(p, FULL_ISSUE, 'tester', 4000);
  assert.deepEqual(p.outside, { kind: 'outside', from: 0, to: 2 });
  assert.equal(p.message({ kind: 'forward', stage: 0 })?.text, 'to security');
  assert.equal(p.travelled(4500), 0.5);
  assert.equal(p.travelled(5000), null);
});

test('the run workspace offers show flow in its menu, and the panel draws the plan', () => {
  const app = demo();
  const said: string[] = [];
  app.on((event, detail) => event === 'narrate' && detail && said.push(detail));
  toRun(app);
  assert.equal(said[0], app.narration.flowHint);
  const p = app.projects.find((x) => x.folder === 'payments');
  const w = p?.workspaces.find((x) => workspaceLabel(x) === RUN_BRANCH);
  assert.ok(p && w);
  app.openMenu({ x: 4, y: 4 }, { kind: 'workspace', project: p.id, workspace: w.id });
  app.render();
  assert.ok(app.textAt('show flow'));
  if (app.overlay?.kind !== 'menu') return;
  app.chooseMenu(app.overlay.actions.findIndex((a) => a.kind === 'showFlow'));
  assert.equal(app.flowShown(), true);
  assert.ok(shown(app, 'Full issue'));
  for (const text of ['orchestrator', 'codex · gpt-5.4', 'findings · 0/3', 'failures · 0/2', '◆ open a pull request']) assert.ok(shown(app, text), text);
  inputKey(app, key('Escape'));
  assert.equal(app.flowShown(), false);
  app.destroy();
});

test('the run plays while its panel shows: working boxes, a round, then the end', () => {
  const app = demo();
  toRun(app);
  app.toggleFlow();
  playUntil(app, '┏━ coder');
  playUntil(app, 'findings · 1/3');
  playUntil(app, '┏━ tester');
  playUntil(app, 'marked as pending review');
  app.render();
  const end = app.textAt('◆');
  assert.ok(end);
  const run = app.runs[0];
  const done = () => run.members.slice(1).filter((m) => app.findPane(m.pane)?.pane.unseen).length;
  assert.ok(done() > 0);
  playUntil(app, '┏━ coder', 60000);
  assert.deepEqual([done(), run.messages.length], [0, 1]);
  app.destroy();
});

test('the run waits while its panel is closed', () => {
  const app = demo();
  toRun(app);
  app.advance(60000);
  assert.equal(app.runs[0].messages.length, 0);
  app.destroy();
});

test('a box goes to its tab, and an arrow shows the last message sent along it', () => {
  const app = demo();
  toRun(app);
  app.toggleFlow();
  playUntil(app, '┏━ reviewer');
  app.advance(1500);
  app.render();
  const arrow = app.textAt('▼');
  assert.ok(arrow);
  app.pointerDown(arrow.x, arrow.y, 0);
  app.pointerUp();
  assert.equal(app.overlay?.kind, 'flowMessage');
  assert.ok(shown(app, 'orchestrator → '));
  inputKey(app, key('Enter'));
  assert.equal(app.overlay, null);
  assert.ok(shown(app, 'copied to clipboard'));
  const coder = app.textAt('─ coder ─');
  assert.ok(coder);
  app.pointerDown(coder.x + 2, coder.y, 0);
  app.pointerUp();
  assert.equal(app.tab()?.name, 'coder');
  app.destroy();
});

test('the keys menu lists the flow keys, and a workspace without a run says so', () => {
  const app = demo(110, 38);
  assert.deepEqual(
    keysItems('workspace').map((item) => item.key),
    ['n', 's', 'r', 'f', 'x'],
  );
  app.config.prefix = 'ctrl+]';
  for (const k of [key(']', true), key('w'), key('f')]) inputKey(app, k);
  assert.equal(app.flowShown(), false);
  assert.ok(shown(app, 'this workspace runs no flow'));
  app.destroy();
});

test('starting and editing flows need a real cornercase', () => {
  const app = demo(110, 38);
  app.openNewMenu({ x: 2, y: 2 });
  if (app.overlay?.kind !== 'menu') return assert.fail('no menu');
  app.chooseMenu(app.overlay.actions.findIndex((a) => a.kind === 'startFlow'));
  assert.ok(shown(app, 'the demo only plays the flow in payments'));
  app.openSettings();
  inputKey(app, key('ArrowRight'));
  const rows = app.settingsRows(1);
  const at = rows.findIndex((row) => row.id === 'flows');
  assert.equal(rows[at].section, 'Flows');
  app.settingsClick(at);
  assert.ok(shown(app, 'the demo only plays the flow in payments'));
  app.destroy();
});
