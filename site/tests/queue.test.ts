import assert from 'node:assert/strict';
import { test } from 'node:test';
import { App } from '../src/lib/demo/app';
import { type Combo, chooseCombo, comboLabel, comboList } from '../src/lib/demo/combo';
import { Agent } from '../src/lib/demo/programs';
import { inputKey } from '../src/lib/demo/client';
import { type QueueControl, type QueueForm, queueAt, queueControls, queueRect, queueWhen, whenCombo } from '../src/lib/demo/queue';
import { world } from '../src/lib/demo/world';
import { rect } from '../src/lib/term/grid';

const key = (key: string, shift = false) => ({ key, metaKey: false, isComposing: false, ctrlKey: false, altKey: false, shiftKey: shift });
const pick = (c: Combo, label: string) => chooseCombo(c, c.options.indexOf(label));

function appAt(cols: number): App {
  const app = new App(); app.virtual = true; app.resize(cols, 38); world(app, { scripted: true });
  return app;
}

function form(app: App): QueueForm {
  const o = app.overlay;
  if (o?.kind !== 'queue') throw new Error('queue form');
  return o;
}

function click(app: App, x: number, y: number): void {
  app.render();
  app.pointerDown(x, y, 0);
  app.pointerUp();
  app.render();
}

const screen = (app: App) => rect(0, 0, app.cols, app.rows);

function clickControl(app: App, control: QueueControl): void {
  const r = queueRect(screen(app), form(app), control);
  click(app, r.x + 2, r.y);
}

function clickOption(app: App, control: QueueControl, label: string): void {
  const o = form(app);
  const c = o[control] as Combo;
  const list = comboList(queueRect(screen(app), o, control), c, screen(app));
  const row = c.options.indexOf(label) - (c.open?.scroll ?? 0);
  assert.ok(row >= 0 && row < list.h - 2, `${label} is shown`);
  click(app, list.x + 3, list.y + 1 + row);
}

function inPanel(app: App, text: string): { x: number; y: number } {
  const panel = app.areas().changes;
  for (let nth = 0; ; nth++) {
    const at = app.textAt(text, nth);
    if (!at) throw new Error(`${text} in the panel`);
    if (at.x >= panel.x) return at;
  }
}

for (const cols of [140, 70]) {
  test(`the queue form schedules, edits and cancels prompts at ${cols} columns`, () => {
    const app = appAt(cols);
    app.toggleQueue();
    app.render();
    assert.ok(app.textAt('+ New'));
    app.openQueueForm();
    for (const ch of 'Fix the login') inputKey(app, key(ch));
    const o = form(app);
    assert.equal(o.prompt, 'Fix the login');
    assert.equal(queueAt(o), 'in 2h');
    o.amount = '1'; pick(o.unit, 'minutes'); pick(o.agent, 'claude');
    app.render();
    assert.ok(app.textAt('Queued agents always accept'));
    assert.ok(app.textAt('Schedule an agent'));
    inputKey(app, key('Enter'));
    assert.equal(app.queued[0].state, 'pending');
    assert.equal(app.queued[0].at, 60000);
    app.openQueueForm(app.queued[0]);
    const edit = form(app);
    assert.equal(comboLabel(edit.when), 'on');
    assert.equal(queueAt(edit), queueWhen(60000));
    assert.equal(edit.prompt, 'Fix the login');
    edit.when = whenCombo('in'); edit.amount = '2'; pick(edit.unit, 'minutes');
    inputKey(app, key('Enter'));
    assert.equal(app.queued.length, 1);
    assert.equal(app.queued[0].at, 120000);
    app.openQueueForm();
    inputKey(app, key('Escape'));
    assert.equal(app.overlay, null);
    app.queued = [];
    app.advance(180000);
    assert.equal(app.queued.length, 0);
    app.destroy();
  });

  test(`the queue shares the side column and Escape closes it at ${cols} columns`, () => {
    const app = appAt(cols);
    app.toggleQueue();
    assert.equal(app.todo.open, false);
    app.toggleTodo();
    assert.equal(app.queueOpen, false);
    app.toggleQueue();
    app.toggleFiles();
    assert.equal(app.queueOpen, false);
    app.toggleQueue();
    inputKey(app, key('Escape'));
    assert.equal(app.queueOpen, false);
    app.destroy();
  });

  test(`choosing a kind of time from its list changes the value fields at ${cols} columns`, () => {
    const app = appAt(cols);
    app.openQueueForm();
    app.render();
    assert.deepEqual(queueControls(form(app)).slice(0, 4), ['prompt', 'when', 'amount', 'unit']);
    assert.ok(app.textAt('hours'));
    clickControl(app, 'when');
    assert.ok(form(app).when.open);
    for (const label of ['tomorrow at', 'Claude reset', 'Codex reset']) assert.ok(app.textAt(label), label);
    clickOption(app, 'when', 'on');
    const o = form(app);
    assert.equal(o.when.open, null);
    assert.deepEqual(queueControls(o).slice(0, 4), ['prompt', 'when', 'date', 'clock']);
    assert.ok(app.textAt(`${o.date}   09:00`));
    assert.equal(app.textAt('hours'), null);
    inputKey(app, key('ArrowDown'));
    inputKey(app, key('ArrowDown'));
    inputKey(app, key('ArrowDown'));
    inputKey(app, key('Enter'));
    assert.equal(comboLabel(o.when), 'Claude reset');
    assert.equal(app.overlay, o);
    app.render();
    assert.ok(app.textAt('read from the plan when you save'));
    assert.deepEqual(queueControls(o).slice(0, 3), ['prompt', 'when', 'place']);
    for (const ch of 'tom') inputKey(app, key(ch));
    assert.equal(form(app).when.open?.query, 'tom');
    inputKey(app, key('Enter'));
    assert.equal(queueAt(o), 'tomorrow 09:00');
    app.render();
    assert.ok(app.textAt('today or tomorrow') === null);
    inputKey(app, key(' '));
    assert.ok(o.when.open);
    inputKey(app, key('Escape'));
    assert.equal(o.when.open, null);
    assert.equal(app.overlay, o);
    app.destroy();
  });
}

test('the place, agent and mode lists open and choose with a click', () => {
  const app = appAt(140);
  app.openQueueForm();
  const o = form(app);
  o.prompt = 'Fix the login';
  app.render();
  assert.ok(app.textAt('Open in   new tab'));
  clickControl(app, 'place');
  assert.ok(o.place.open);
  const root = o.places.findIndex((choice) => choice.project === app.project()!.id && choice.workspace === null);
  clickOption(app, 'place', o.places[root].label);
  assert.equal(o.place.chosen, root);
  assert.ok(app.textAt('Open in'));
  assert.ok(app.textAt('new tab'));
  clickControl(app, 'openIn');
  clickOption(app, 'openIn', 'new worktree');
  assert.ok(app.textAt('branch name'));
  clickControl(app, 'branch');
  for (const ch of 'fix-login') inputKey(app, key(ch));
  assert.equal(o.branch, 'fix-login');
  clickControl(app, 'agent');
  assert.ok(o.agent.open);
  assert.ok(app.textAt(`↓ ${o.agent.options.length - 8} more`));
  clickOption(app, 'agent', 'codex');
  assert.equal(comboLabel(o.agent), 'codex');
  assert.deepEqual(o.mode.options, ['default', 'read only', 'workspace write', 'no sandbox, no approvals (dangerous)']);
  clickControl(app, 'mode');
  const submit = app.textAt(' schedule ');
  assert.ok(submit);
  click(app, submit.x + 2, submit.y);
  assert.equal(o.mode.open, null);
  assert.equal(app.overlay, o);
  clickControl(app, 'mode');
  clickOption(app, 'mode', 'read only');
  inputKey(app, key('Tab'));
  assert.equal(o.focus, 'prompt');
  inputKey(app, key('Tab', true));
  assert.equal(o.focus, 'mode');
  inputKey(app, key('Enter'));
  assert.equal(app.overlay, null);
  assert.equal(app.queued[0].agent, 'codex');
  assert.equal(app.queued[0].mode, 'read only');
  assert.equal(app.queued[0].branch, 'fix-login');
  assert.equal(app.queued[0].workspace, null);
  app.destroy();
});

test('entries due together start separate agents without starting early or moving focus', () => {
  const app = appAt(140);
  app.config.trust = false;
  app.config.submit = false;
  const before = app.tab()?.id;
  for (const branch of ['first-queued', 'second-queued']) {
    app.openQueueForm();
    const o = form(app);
    chooseCombo(o.place, o.places.findIndex((choice) => choice.project === app.project()!.id && choice.workspace === null));
    pick(o.openIn, 'new worktree'); o.branch = branch;
    o.prompt = 'Fix the login'; pick(o.agent, 'claude'); o.amount = '1'; pick(o.unit, 'minutes');
    app.submitQueue();
  }
  app.advance(59999);
  assert.ok(app.queued.every((entry) => entry.state === 'pending'));
  app.advance(1);
  assert.ok(app.queued.every((entry) => entry.state === 'started'));
  assert.notEqual(app.queued[0].pane, app.queued[1].pane);
  assert.equal(app.tab()?.id, before);
  app.advance(4000);
  const panes = app.projects.flatMap((project) => project.workspaces.flatMap((workspace) => workspace.tabs.flatMap((tab) => tab.panes)));
  for (const entry of app.queued) {
    const agent = panes.find((pane) => pane.id === entry.pane)?.shell.fg;
    assert.ok(agent instanceof Agent && agent.working);
  }
  app.destroy();
});

test('an issue schedule carries its title and branch into the queue form', () => {
  const app = appAt(140);
  app.openIssues(); app.advance(1000);
  app.issuesButton('schedule');
  const o = form(app);
  assert.ok(o.label?.includes('#'));
  assert.ok(o.branch);
  assert.equal(o.focus, 'when');
  assert.equal(o.places[o.place.chosen ?? -1]?.workspace, null);
  assert.equal(comboLabel(o.openIn), 'new worktree');
  o.when = whenCombo('claude');
  app.submitQueue();
  assert.equal(app.queued[0].at, app.now() + 2 * 3600000);
  assert.equal(app.queued[0].branch, o.branch);
  app.destroy();
});

test('in compact mode the queue button sits beside Issues and the bar keeps its room', () => {
  const app = appAt(60);
  app.render();
  assert.equal(app.textAt('Q 0'), null);
  app.nav = 'workspaces';
  app.render();
  const issues = app.textAt('Issues');
  const queue = app.textAt('Queue 0');
  assert.ok(issues && queue);
  assert.equal(queue.y, issues.y);
  assert.ok(queue.x > issues.x);
  app.pointerDown(queue.x + 1, queue.y, 0);
  app.pointerUp();
  assert.equal(app.queueOpen, true);
  app.destroy();
});

test('a removed queue entry comes back with undo', () => {
  const app = appAt(140);
  app.openQueueForm();
  const o = form(app);
  o.prompt = 'Fix the login'; pick(o.agent, 'claude');
  app.submitQueue();
  const before = [...app.queued];
  app.removeQueued((entry) => entry.id === before[0].id, `removed queue entry ${before[0].id}`);
  assert.equal(app.queued.length, 0);
  app.toast?.undo?.();
  assert.deepEqual(app.queued, before);
  app.destroy();
});

test('the panel buttons light up under the pointer', () => {
  const app = appAt(140);
  app.openQueueForm();
  form(app).prompt = 'Fix the login';
  app.submitQueue();
  app.render();
  for (const text of ['+ New', 'start now']) {
    const at = inPanel(app, text);
    app.pointerMove(at.x + 1, at.y, 0);
    const cell = app.render().grid.at(at.x + 1, at.y);
    assert.equal(cell?.bg, 6, text);
    assert.equal(cell?.fg, 0, text);
  }
  const panel = app.areas().changes;
  const start = inPanel(app, 'start now');
  const remove = { x: panel.x + panel.w - 2, y: start.y - 2 };
  app.pointerMove(remove.x, remove.y, 0);
  const grid = app.render().grid;
  assert.equal(grid.at(remove.x, remove.y)?.ch, '×');
  assert.equal(grid.at(remove.x, remove.y)?.fg, 1);
  assert.equal(grid.at(panel.x, start.y - 3)?.bg, 235);
  app.pointerMove(panel.x + panel.w - 2, panel.y, 0);
  assert.equal(app.render().grid.at(panel.x + panel.w - 2, panel.y)?.fg, 1);
  app.destroy();
});
