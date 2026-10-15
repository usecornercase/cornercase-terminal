import assert from 'node:assert/strict';
import { test } from 'node:test';
import { App } from '../src/lib/demo/app';
import { type Combo, chooseCombo, comboLabel, comboList } from '../src/lib/demo/combo';
import type { IssuesOverlay } from '../src/lib/demo/model';
import { Agent } from '../src/lib/demo/programs';
import { inputKey } from '../src/lib/demo/client';
import { type QueueControl, type QueueForm, queueAt, queueControls, queueRect, queueWhen, whenCombo } from '../src/lib/demo/queue';
import { world } from '../src/lib/demo/world';
import { rect } from '../src/lib/term/grid';

const key = (key: string, shift = false) => ({ key, metaKey: false, isComposing: false, ctrlKey: false, altKey: false, shiftKey: shift });
const pick = (c: Combo, label: string) => chooseCombo(c, c.options.indexOf(label));
const type = (app: App, text: string) => {
  for (const ch of text) inputKey(app, key(ch));
};

function appAt(cols: number): App {
  const app = new App(); app.virtual = true; app.resize(cols, 38); world(app, { scripted: true });
  return app;
}

function form(app: App): QueueForm {
  const o = app.overlay;
  if (o?.kind !== 'queue') throw new Error('queue form');
  return o;
}

function issues(app: App): IssuesOverlay {
  const o = app.overlay;
  if (o?.kind !== 'issues') throw new Error('issues modal');
  return o;
}

function queueTab(app: App): IssuesOverlay {
  app.openIssues();
  app.advance(1000);
  app.issuesTab(app.config.sources.length);
  return issues(app);
}

function schedule(app: App, prompt: string, minutes: number): void {
  app.openQueueForm();
  const o = form(app);
  o.prompt = prompt; pick(o.agent, 'claude'); o.amount = String(minutes); pick(o.unit, 'minutes');
  app.submitQueue();
}

function click(app: App, x: number, y: number): void {
  app.render();
  app.pointerDown(x, y, 0);
  app.pointerUp();
  app.render();
}

function lastText(app: App, text: string): { x: number; y: number } {
  app.render();
  let found = null;
  for (let nth = 0; ; nth++) {
    const at = app.textAt(text, nth);
    if (!at) break;
    found = at;
  }
  if (!found) throw new Error(`${text} on screen`);
  return found;
}

const pressButton = (app: App, label: string) => {
  const at = lastText(app, ` ${label} `);
  click(app, at.x + 1, at.y);
};

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

for (const cols of [140, 70]) {
  test(`the Queue tab schedules, edits and cancels prompts and the form comes back to it at ${cols} columns`, () => {
    const app = appAt(cols);
    const browser = queueTab(app);
    const start = app.now();
    app.render();
    assert.ok(app.textAt('nothing queued: new schedules a prompt'));
    assert.deepEqual(app.issuesView(browser).buttons, ['new', 'cancel']);
    pressButton(app, 'new');
    type(app, 'Fix the login');
    const o = form(app);
    assert.equal(o.prompt, 'Fix the login');
    assert.equal(queueAt(o), 'in 2h');
    o.amount = '1'; pick(o.unit, 'minutes'); pick(o.agent, 'claude');
    app.render();
    assert.ok(app.textAt('Queued agents always accept'));
    assert.ok(app.textAt('Schedule an agent'));
    inputKey(app, key('Enter'));
    assert.equal(app.queued[0].state, 'pending');
    assert.equal(app.queued[0].at, start + 60000);
    assert.equal(issues(app), browser);
    assert.equal(browser.tab, app.config.sources.length);
    app.render();
    assert.ok(app.textAt(`${queueWhen(start + 60000)}  Fix the login`));
    assert.ok(app.textAt('pending · '));
    inputKey(app, key('Enter'));
    const edit = form(app);
    assert.equal(comboLabel(edit.when), 'on');
    assert.equal(queueAt(edit), queueWhen(start + 60000));
    assert.equal(edit.prompt, 'Fix the login');
    edit.when = whenCombo('in'); edit.amount = '2'; pick(edit.unit, 'minutes');
    inputKey(app, key('Enter'));
    assert.equal(app.queued.length, 1);
    assert.equal(app.queued[0].at, start + 120000);
    assert.equal(issues(app).tab, app.config.sources.length);
    pressButton(app, 'new');
    inputKey(app, key('Escape'));
    assert.equal(issues(app), browser);
    inputKey(app, key('Escape'));
    assert.equal(app.overlay, null);
    app.queued = [];
    app.advance(180000);
    assert.equal(app.queued.length, 0);
    app.destroy();
  });

  test(`the sidebar has no queue button at ${cols} columns`, () => {
    const app = appAt(cols);
    app.render();
    assert.equal(app.textAt('Queue'), null);
    if (cols < 90) {
      app.nav = 'workspaces';
      app.render();
      assert.ok(app.textAt('Issues'));
      assert.equal(app.textAt('Queue'), null);
    }
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
    type(app, 'tom');
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

test('the Queue tab is the last one and lists every entry with its state in colour', () => {
  const app = appAt(140);
  for (const [prompt, minutes] of [['Fix the login', 1], ['Update the docs', 2], ['Fix logout', 3], ['Review the tests', 4]] as const) schedule(app, prompt, minutes);
  const [started, failed, missed] = app.queued;
  app.runQueue(started);
  failed.state = 'failed'; failed.error = 'codex is not installed';
  missed.state = 'missed';
  const browser = queueTab(app);
  const view = app.issuesView(browser);
  assert.equal(view.tabs[view.tabs.length - 1], 'Queue');
  assert.deepEqual(view.toggles, []);
  assert.deepEqual(view.items.map((item) => item.meta), [
    `started · pane ${started.pane} · claude · web-shop › feat-dark-mode`,
    'failed · codex is not installed · claude · web-shop › feat-dark-mode',
    'missed · claude · web-shop › feat-dark-mode',
    'pending · claude · web-shop › feat-dark-mode',
  ]);
  const grid = app.render().grid;
  assert.equal(app.textAt('[ ] closed'), null);
  for (const [text, colour] of [['started · pane', 2], ['failed · codex', 1], ['missed · claude', 3], ['pending · claude', 8]] as const) {
    const at = app.textAt(text);
    assert.ok(at, text);
    assert.equal(grid.at(at.x, at.y)?.fg, colour, text);
  }
  assert.ok(app.textAt(queueWhen(started.at)));
  const tab = app.textAt('Queue');
  assert.ok(tab && tab.y < (app.textAt('Fix the login')?.y ?? 0));
  app.destroy();
});

test('the buttons and the hint follow the selected entry', () => {
  const app = appAt(140);
  for (const [prompt, minutes] of [['Fix the login', 1], ['Update the docs', 2], ['Fix logout', 3]] as const) schedule(app, prompt, minutes);
  const [started, failed] = app.queued;
  app.runQueue(started);
  failed.state = 'failed'; failed.error = 'project closed';
  const browser = queueTab(app);
  const view = () => app.issuesView(browser);
  assert.deepEqual(view().buttons, ['new', 'remove', 'clear started', 'cancel']);
  assert.equal(view().hint, "enter opens its pane · every project's scheduled agents");
  inputKey(app, key('ArrowDown'));
  assert.deepEqual(view().buttons, ['new', 'start now', 'remove', 'clear started', 'cancel']);
  assert.equal(view().hint, 'enter edits it · start now runs it at once');
  inputKey(app, key('ArrowDown'));
  assert.deepEqual(view().buttons, ['new', 'start now', 'remove', 'clear started', 'cancel']);
  type(app, 'nothing like it');
  assert.deepEqual(view().buttons, ['new', 'clear started', 'cancel']);
  assert.equal(view().empty, 'nothing matches');
  assert.equal(view().hint, "every project's scheduled agents");
  app.destroy();
});

test('typing filters the entries and start now runs one at once', () => {
  const app = appAt(140);
  for (const [prompt, minutes] of [['Fix the login', 1], ['Fix logout', 2]] as const) schedule(app, prompt, minutes);
  const browser = queueTab(app);
  type(app, 'LOGOUT');
  assert.deepEqual(app.issuesView(browser).items.map((item) => item.title), ['Fix logout']);
  pressButton(app, 'start now');
  assert.equal(app.overlay, browser);
  assert.equal(app.queued[1].state, 'started');
  assert.equal(app.queued[0].state, 'pending');
  app.advance(4000);
  const pane = app.findPane(app.queued[1].pane ?? -1)?.pane;
  assert.ok(pane?.shell.fg instanceof Agent);
  app.destroy();
});

test('a started entry closes the modal and shows its pane', () => {
  const app = appAt(140);
  schedule(app, 'Fix the login', 1);
  app.runQueue(app.queued[0]);
  assert.notEqual(app.tab()?.panes[0].id, app.queued[0].pane);
  queueTab(app);
  app.render();
  const row = app.textAt('Fix the login');
  assert.ok(row);
  click(app, row.x, row.y);
  assert.equal(app.overlay, null);
  assert.ok(app.tab()?.panes.some((pane) => pane.id === app.queued[0].pane));
  app.destroy();
});

test('an issue schedule carries its title and branch and comes back to the Queue tab', () => {
  const app = appAt(140);
  app.openIssues(); app.advance(1000);
  const browser = issues(app);
  app.issuesButton('schedule');
  const cancelled = form(app);
  assert.ok(cancelled.label?.includes('#'));
  inputKey(app, key('Escape'));
  assert.equal(app.overlay, browser);
  assert.equal(browser.tab, 0);
  app.issuesClick(0);
  const detail = browser.detail;
  assert.ok(detail);
  app.issuesButton('schedule');
  click(app, 0, 0);
  assert.equal(app.overlay, browser);
  assert.equal(browser.detail, detail);
  app.issuesButton('schedule');
  const o = form(app);
  assert.ok(o.label?.includes(detail));
  assert.ok(o.branch);
  assert.equal(o.focus, 'when');
  assert.equal(o.places[o.place.chosen ?? -1]?.workspace, null);
  assert.equal(comboLabel(o.openIn), 'new worktree');
  o.when = whenCombo('claude');
  app.submitQueue();
  assert.equal(app.queued[0].at, app.now() + 2 * 3600000);
  assert.equal(app.queued[0].branch, o.branch);
  assert.equal(app.overlay, browser);
  assert.equal(browser.tab, app.config.sources.length);
  assert.equal(browser.detail, null);
  assert.equal(browser.filter, '');
  const [row] = app.issuesView(browser).items;
  assert.equal(row.title, o.label);
  assert.ok(row.meta.startsWith('pending · ') && row.meta.endsWith(` · web-shop › ${o.branch}`));
  app.destroy();
});

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
  type(app, 'fix-login');
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

test('remove and clear started on the Queue tab come back with undo', () => {
  const app = appAt(140);
  for (const [prompt, minutes] of [['Fix the login', 1], ['Fix logout', 2]] as const) schedule(app, prompt, minutes);
  const before = [...app.queued];
  const toast = () => app.toast;
  const browser = queueTab(app);
  pressButton(app, 'remove');
  assert.deepEqual(app.queued, [before[1]]);
  assert.equal(app.overlay, browser);
  assert.equal(toast()?.text, `removed queue entry ${before[0].id}`);
  const undo = lastText(app, 'undo');
  click(app, undo.x + 1, undo.y);
  assert.deepEqual(app.queued, before);
  assert.equal(toast(), null);
  app.runQueue(app.queued[0]);
  pressButton(app, 'clear started');
  assert.deepEqual(app.queued, [before[1]]);
  assert.equal(toast()?.text, 'cleared 1 started');
  assert.ok(!app.issuesView(browser).buttons.includes('clear started'));
  toast()?.undo?.();
  assert.deepEqual(app.queued, before);
  app.destroy();
});

test('an entry brought back by undo after its time still starts', () => {
  const app = appAt(140);
  app.openQueueForm();
  const form = app.overlay;
  if (form?.kind !== 'queue') throw new Error('form');
  form.prompt = 'Fix the login';
  chooseCombo(form.agent, form.agent.options.indexOf('claude'));
  chooseCombo(form.when, form.when.options.indexOf('in'));
  form.amount = '1';
  chooseCombo(form.unit, form.unit.options.indexOf('minutes'));
  app.submitQueue();
  const entry = app.queued[0];
  app.advance(58000);
  app.removeQueued((item) => item.id === entry.id, 'removed queue entry {n}');
  app.advance(3000);
  assert.equal(entry.state, 'pending');
  app.toast?.undo?.();
  app.advance(1);
  assert.equal(app.queued[0].state, 'started');
  app.destroy();
});
