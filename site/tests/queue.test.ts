import assert from 'node:assert/strict';
import { test } from 'node:test';
import { App } from '../src/lib/demo/app';
import { type Combo, chooseCombo, combo, comboLabel, comboList } from '../src/lib/demo/combo';
import type { IssuesOverlay } from '../src/lib/demo/model';
import type { QueueListDialog } from '../src/lib/demo/queuelist';
import { Agent } from '../src/lib/demo/programs';
import { inputKey } from '../src/lib/demo/client';
import { type QueueControl, type QueueForm, focusQueue, queueAt, queueControls, queueRect, queueWhen, whenCombo } from '../src/lib/demo/queue';
import { world } from '../src/lib/demo/world';
import { inner, issuesArea, menuArea } from '../src/lib/demo/layout';
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

function list(app: App): QueueListDialog {
  const o = app.overlay;
  if (o?.kind !== 'queuelist') throw new Error('queue modal');
  return o;
}

function queueModal(app: App): QueueListDialog {
  app.openQueue();
  return list(app);
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
  test(`the Queue window schedules, edits and cancels prompts and the form comes back to it at ${cols} columns`, () => {
    const app = appAt(cols);
    const browser = queueModal(app);
    const start = app.now();
    app.render();
    assert.ok(app.textAt('nothing queued: new schedules a prompt'));
    assert.deepEqual(app.queueListView(browser).buttons, ['new', 'done']);
    pressButton(app, 'new');
    type(app, 'Fix the login');
    const o = form(app);
    assert.equal(o.prompt, 'Fix the login');
    assert.equal(queueAt(o), 'in 2h');
    o.amount = '1'; pick(o.unit, 'minutes'); pick(o.agent, 'claude');
    app.render();
    assert.ok(app.textAt('Queued agents always accept'));
    assert.ok(app.textAt('Schedule an agent'));
    pressButton(app, 'schedule');
    assert.equal(app.queued[0].state, 'pending');
    assert.equal(app.queued[0].at, start + 60000);
    assert.equal(list(app), browser);
    assert.equal(browser.selected, app.queued[0].id);
    app.render();
    assert.ok(app.textAt(`${queueWhen(start + 60000)}  Fix the login`));
    assert.ok(app.textAt('pending · '));
    inputKey(app, key('Enter'));
    const edit = form(app);
    assert.equal(comboLabel(edit.when), 'on');
    assert.equal(queueAt(edit), queueWhen(start + 60000));
    assert.equal(edit.prompt, 'Fix the login');
    edit.when = whenCombo('in'); edit.amount = '2'; pick(edit.unit, 'minutes');
    pressButton(app, 'schedule');
    assert.equal(app.queued.length, 1);
    assert.equal(app.queued[0].at, start + 120000);
    assert.equal(list(app), browser);
    pressButton(app, 'new');
    inputKey(app, key('Escape'));
    assert.equal(list(app), browser);
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
    for (const label of ['tomorrow at', 'every', 'every day at', 'every week on']) assert.ok(app.textAt(label), label);
    clickOption(app, 'when', 'on');
    const o = form(app);
    assert.equal(o.when.open, null);
    assert.deepEqual(queueControls(o).slice(0, 4), ['prompt', 'when', 'date', 'clock']);
    assert.ok(app.textAt(`${o.date}   09:00`));
    assert.equal(app.textAt('hours'), null);
    for (let i = 0; i < 7; i++) inputKey(app, key('ArrowDown'));
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

test('the Queue window lists every entry with its state in colour', () => {
  const app = appAt(140);
  for (const [prompt, minutes] of [['Fix the login', 1], ['Update the docs', 2], ['Fix logout', 3], ['Review the tests', 4]] as const) schedule(app, prompt, minutes);
  const [started, failed, missed] = app.queued;
  app.runQueue(started);
  failed.state = 'failed'; failed.error = 'codex is not installed';
  missed.state = 'missed';
  const browser = queueModal(app);
  const view = app.queueListView(browser);
  assert.deepEqual(view.rows.map((item) => item.meta), [
    'pending · claude · web-shop › feat-dark-mode',
    'missed · claude · web-shop › feat-dark-mode',
    'failed · codex is not installed · claude · web-shop › feat-dark-mode',
    `started · pane ${started.pane} · claude · web-shop › feat-dark-mode`,
  ]);
  const grid = app.render().grid;
  assert.equal(app.textAt('[ ] closed'), null);
  for (const [text, colour] of [['started · pane', 2], ['failed · codex', 1], ['missed · claude', 3], ['pending · claude', 8]] as const) {
    const at = app.textAt(text);
    assert.ok(at, text);
    assert.equal(grid.at(at.x, at.y)?.fg, colour, text);
  }
  assert.ok(app.textAt(queueWhen(started.at)));
  const title = app.textAt('Queue');
  assert.ok(title && title.y < (app.textAt('Fix the login')?.y ?? 0));
  app.destroy();
});

test('the buttons and the hint follow the selected entry', () => {
  const app = appAt(140);
  for (const [prompt, minutes] of [['Fix the login', 1], ['Update the docs', 2], ['Fix logout', 3]] as const) schedule(app, prompt, minutes);
  const [started, failed] = app.queued;
  app.runQueue(started);
  failed.state = 'failed'; failed.error = 'project closed';
  const browser = queueModal(app);
  const view = () => app.queueListView(browser);
  assert.deepEqual(view().buttons, ['new', 'start now', 'remove', 'clear started', 'done']);
  assert.equal(view().hint, 'enter edits it · start now runs it at once');
  inputKey(app, key('End'));
  assert.deepEqual(view().buttons, ['new', 'remove', 'clear started', 'done']);
  assert.equal(view().hint, "enter opens its pane · every project's scheduled agents");
  inputKey(app, key('ArrowUp'));
  assert.deepEqual(view().buttons, ['new', 'start now', 'remove', 'clear started', 'done']);
  type(app, 'nothing like it');
  assert.deepEqual(view().buttons, ['new', 'clear started', 'done']);
  assert.equal(view().empty, 'nothing matches');
  assert.equal(view().hint, "every project's scheduled agents");
  app.destroy();
});

test('typing filters the entries and start now runs one at once', () => {
  const app = appAt(140);
  for (const [prompt, minutes] of [['Fix the login', 1], ['Fix logout', 2]] as const) schedule(app, prompt, minutes);
  const browser = queueModal(app);
  type(app, 'LOGOUT');
  assert.deepEqual(app.queueListView(browser).rows.map((item) => item.title), ['Fix logout']);
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
  queueModal(app);
  app.render();
  const row = app.textAt('Fix the login');
  assert.ok(row);
  click(app, row.x, row.y);
  assert.equal(app.overlay, null);
  assert.ok(app.tab()?.panes.some((pane) => pane.id === app.queued[0].pane));
  app.destroy();
});

test('a started entry whose pane is closed says it shows its result and a click leaves the list open', () => {
  const app = appAt(140);
  schedule(app, 'Fix the login', 1);
  app.runQueue(app.queued[0]);
  app.queued[0].pane = 987654;
  const browser = queueModal(app);
  assert.equal(app.queueListView(browser).hint, "enter shows its result · every project's scheduled agents");
  app.render();
  const row = app.textAt('Fix the login');
  assert.ok(row);
  click(app, row.x, row.y);
  assert.equal(app.overlay?.kind, 'queuelist');
  app.destroy();
});

test('an issue schedule carries its title and branch and a save opens the Queue window with it selected', () => {
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
  const queue = list(app);
  assert.equal(queue.selected, app.queued[0].id);
  const [row] = app.queueListView(queue).rows;
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

test('the mode list opens on the mode settings chose, and default schedules none', () => {
  const app = appAt(140);
  app.openQueueForm();
  const o = form(app);
  assert.equal(comboLabel(o.mode), 'plan');
  o.prompt = 'Fix the login';
  app.render();
  clickControl(app, 'mode');
  clickOption(app, 'mode', 'default');
  inputKey(app, key('Enter'));
  assert.equal(app.queued[0].mode, '');
  assert.ok(!app.queued[0].command.includes('--permission-mode'), app.queued[0].command);
  app.openQueueForm(app.queued[0]);
  assert.equal(comboLabel(form(app).mode), 'default');
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

test('remove and clear started in the Queue window come back with undo', () => {
  const app = appAt(140);
  for (const [prompt, minutes] of [['Fix the login', 1], ['Fix logout', 2]] as const) schedule(app, prompt, minutes);
  const before = [...app.queued];
  const toast = () => app.toast;
  const browser = queueModal(app);
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
  assert.ok(!app.queueListView(browser).buttons.includes('clear started'));
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

test('right click on the prompt opens a field menu and keeps the queue form', () => {
  const app = appAt(140);
  app.openQueueForm();
  type(app, 'Fix the login');
  const r = queueRect(screen(app), form(app), 'prompt');
  app.render();
  app.pointerDown(r.x + 2, r.y, 2);
  app.pointerUp();
  assert.ok(app.fieldMenu);
  assert.equal(app.overlay?.kind, 'queue');
  app.render();
  const menu = menuArea(app.cols, app.rows, { x: r.x + 2, y: r.y }, ['copy', 'cut', 'select all']);
  app.pointerDown(menu.x + 2, menu.y + 3, 0);
  type(app, 'Y');
  assert.equal(form(app).prompt, 'Y');
  assert.equal(app.overlay?.kind, 'queue');
  app.destroy();
});

test('right click on another text control focuses it instead of using the prompt selection', () => {
  const app = appAt(140);
  app.openQueueForm();
  type(app, 'abcd');
  const o = form(app);
  o.amount = 'abcd';
  pick(o.when, 'in');
  app.render();
  const prompt = queueRect(screen(app), o, 'prompt');
  app.pointerDown(prompt.x + 1, prompt.y, 0);
  o.anchor = 0;
  o.cursor = 4;
  const amount = queueRect(screen(app), form(app), 'amount');
  app.render();
  app.pointerDown(amount.x + 1, amount.y, 2);
  assert.equal(form(app).focus, 'amount');
  app.destroy();
});

function routineForm(app: App, kind: string, prompt = 'Check the dependencies'): QueueForm {
  app.openQueueForm();
  const o = form(app);
  o.prompt = prompt;
  pick(o.agent, 'claude');
  app.queueChoose(o, 'when', o.when.options.indexOf(kind));
  return o;
}

function routine(app: App, kind = 'every', hours = 2): void {
  const o = routineForm(app, kind);
  if (kind === 'every') { o.amount = String(hours); pick(o.unit, 'hours'); }
  app.submitQueue();
}

const runs = (app: App) => app.queued.filter((entry) => entry.routine !== undefined);
const worktrees = (app: App) => app.projects.flatMap((project) => project.workspaces).filter((workspace) => !!workspace.branch?.startsWith('routine/'));

test('a routine scheduled from the form fires on its grid and each run gets a new workspace', () => {
  const app = appAt(140);
  routine(app);
  const [entry] = app.queued;
  assert.equal(entry.repeat, '2h');
  assert.equal(app.toast?.text, `scheduled routine ${entry.id}, next at ${queueWhen(entry.at)}`);
  assert.equal(runs(app).length, 0);
  app.advance(2 * 3600000 + 1000);
  assert.equal(runs(app).length, 1);
  assert.equal(worktrees(app).length, 1);
  app.advance(2 * 3600000);
  assert.equal(runs(app).length, 2);
  assert.equal(worktrees(app).length, 2);
  assert.ok(runs(app).every((run) => run.state === 'started' && run.repeat === undefined && run.routine === entry.id));
  assert.match(worktrees(app)[0].branch ?? '', /^routine\/check-the-dependencies-\d{8}-\d{4}$/);
  const browser = queueModal(app);
  const view = app.queueListView(browser);
  assert.deepEqual(view.rows.map((item) => [item.run, item.fold]), [[false, false], [true, null], [true, null]]);
  assert.equal(view.rows.filter((item) => item.title.startsWith('started · pane')).length, 2);
  assert.ok(view.rows[0].meta.startsWith('↻ every 2h'));
  app.destroy();
});

test('an interval under 30 minutes shows the error in the form', () => {
  const app = appAt(140);
  const o = routineForm(app, 'every');
  o.amount = '20';
  pick(o.unit, 'minutes');
  app.submitQueue();
  assert.equal(o.error, 'a routine repeats at most every 30 minutes, since each run starts an agent');
  assert.equal(app.overlay, o);
  assert.equal(app.queued.length, 0);
  app.destroy();
});

test('pause stops a routine and resume restarts it on the grid', () => {
  const app = appAt(140);
  routine(app);
  const [entry] = app.queued;
  const browser = queueModal(app);
  assert.deepEqual(app.queueListView(browser).buttons, ['new', 'start now', 'pause', 'remove', 'done']);
  assert.equal(app.queueListView(browser).hint, 'enter edits it · start now runs it once more');
  pressButton(app, 'pause');
  assert.equal(app.toast?.text, `paused routine ${entry.id}`);
  assert.equal(app.queueListView(browser).rows[0].when, 'paused');
  assert.equal(app.queueListView(browser).rows[0].colour, 8);
  app.advance(5 * 3600000);
  assert.equal(runs(app).length, 0);
  assert.deepEqual(app.queueListView(browser).buttons, ['new', 'start now', 'resume', 'remove', 'done']);
  pressButton(app, 'resume');
  assert.match(app.toast?.text ?? '', new RegExp(`^resumed routine ${entry.id}, next at \\d{4}-`));
  assert.ok(entry.at > app.now());
  assert.equal(runs(app).length, 0);
  app.advance(entry.at - app.now() + 1000);
  assert.equal(runs(app).length, 1);
  app.destroy();
});

test('an every day at routine shows its repeat in the row and start now adds a run without moving it', () => {
  const app = appAt(140);
  routine(app, 'every day at');
  const [entry] = app.queued;
  assert.equal(entry.repeat, 'day 09:00');
  const at = entry.at;
  const browser = queueModal(app);
  assert.ok(app.queueListView(browser).rows[0].meta.includes('↻ every day 09:00'));
  pressButton(app, 'start now');
  assert.equal(entry.at, at);
  assert.equal(runs(app).length, 1);
  assert.equal(app.toast?.text, 'started routine Check the dependencies');
  assert.equal(app.queueListView(browser).rows[1].title.startsWith('started'), true);
  inputKey(app, key('ArrowDown'));
  assert.deepEqual(app.queueListView(browser).buttons, ['new', 'remove', 'clear started', 'done']);
  assert.equal(app.queueListView(browser).hint, 'enter opens its pane · a run of a routine');
  app.destroy();
});

test('removing a routine removes its runs and undo brings them back', () => {
  const app = appAt(140);
  routine(app);
  const [entry] = app.queued;
  app.advance(4 * 3600000 + 1000);
  const before = [...app.queued];
  assert.equal(before.length, 3);
  queueModal(app);
  pressButton(app, 'remove');
  assert.equal(app.queued.length, 0);
  assert.equal(app.toast?.text, `removed routine ${entry.id} and its 2 runs`);
  app.toast?.undo?.();
  assert.equal(app.queued.length, 3);
  app.advance(2 * 3600000);
  assert.equal(runs(app).length, 3);
  app.destroy();
});

test('the cleanup checkbox toggles with Space and a click, and only shows for repeat kinds', () => {
  const app = appAt(140);
  const o = routineForm(app, 'every day at');
  app.render();
  assert.ok(app.textAt('[ ] remove the workspace when it finishes'));
  assert.ok(app.textAt('Each run gets a new worktree'));
  assert.ok(app.textAt('new worktree'));
  focusQueue(o, 'cleanup');
  inputKey(app, key(' '));
  assert.equal(o.cleanup, true);
  app.render();
  assert.ok(app.textAt('[x] remove the workspace when it finishes'));
  clickControl(app, 'cleanup');
  assert.equal(o.cleanup, false);
  app.queueChoose(o, 'when', o.when.options.indexOf('in'));
  app.render();
  assert.equal(app.textAt('remove the workspace'), null);
  app.destroy();
});

test('a custom agent greys the cleanup checkbox with its reason', () => {
  const app = appAt(140);
  const o = routineForm(app, 'every week on');
  assert.deepEqual(queueControls(o).slice(-3), ['agent', 'mode', 'cleanup']);
  o.agent = combo([...o.agent.options, 'aider-fake'], o.agent.chosen);
  o.cleanup = true;
  app.queueChoose(o, 'agent', o.agent.options.indexOf('aider-fake'));
  assert.equal(o.cleanup, false);
  assert.ok(!queueControls(o).includes('cleanup'));
  app.render();
  assert.ok(app.textAt('cornercase cannot tell when aider-fake finishes'));
  app.destroy();
});

test('a schedule with extra words is refused like the app does', () => {
  const app = appAt(140);
  const o = routineForm(app, 'every day at');
  o.clock = '09:00 junk';
  app.submitQueue();
  assert.ok(o.error);
  assert.equal(app.queued.length, 0);
  app.destroy();
});

test('a routine far away does not fire early', () => {
  const app = appAt(140);
  const o = routineForm(app, 'every');
  o.amount = '30';
  pick(o.unit, 'days');
  app.submitQueue();
  app.advance(26 * 86400000);
  assert.equal(runs(app).length, 0);
  app.destroy();
});

test('editing only the prompt of a routine keeps its next time', () => {
  const app = appAt(140);
  routine(app);
  const [entry] = app.queued;
  const next = entry.at;
  app.advance(3600000);
  app.openQueueForm(entry);
  form(app).prompt = 'Check again';
  app.submitQueue();
  assert.equal(app.queued[0].at, next);
  assert.equal(app.queued[0].prompt, 'Check again');
  app.destroy();
});

test('a routine cannot turn into a one-off entry', () => {
  const app = appAt(140);
  routine(app);
  app.openQueueForm(app.queued[0]);
  const o = form(app);
  app.queueChoose(o, 'when', o.when.options.indexOf('in'));
  o.amount = '2';
  app.submitQueue();
  assert.match(o.error ?? '', /cannot turn into each other/);
  app.destroy();
});

test('a long prompt names its entry and its tab with its first words', () => {
  const app = appAt(140);
  const name = 'Review the open pull requests and fix…';
  schedule(app, 'Review the open pull requests and fix the failing checks on main, then write a short summary of what changed and what is left for tomorrow.', 1);
  assert.equal(app.queued[0].label, name);
  app.advance(60000);
  assert.equal(app.findPane(app.queued[0].pane ?? -1)?.t.name, name);
  app.destroy();
});

test('typing in the Queue window finds an entry by a word its cut name lost', () => {
  const app = appAt(140);
  schedule(app, 'Review the open pull requests and fix the failing checks on main, then write a short summary', 1);
  queueModal(app);
  type(app, 'failing');
  app.render();
  assert.ok(app.textAt('Review the open pull requests and fix…'));
  app.destroy();
});

test('More lists queue after new bot and opens the Queue window', () => {
  const app = appAt(140);
  app.render();
  const more = app.textAt('More ▴');
  assert.ok(more);
  click(app, more.x + 1, more.y);
  const item = app.textAt('queue');
  assert.ok(item);
  click(app, item.x + 1, item.y);
  assert.equal(list(app).filter, '');
  assert.ok(app.textAt('nothing queued'));
  app.destroy();
});

test('upcoming entries come first, then paused routines, then what ran newest first, with each routine\'s runs under it', () => {
  const app = appAt(140);
  routine(app);
  const [entry] = app.queued;
  app.advance(4 * 3600000 + 1000);
  schedule(app, 'Fix the login', 30);
  routine(app, 'every day at');
  const daily = app.queued.find((item) => item.repeat === 'day 09:00');
  assert.ok(daily);
  app.pauseQueued(daily, true);
  const view = app.queueListView(queueModal(app));
  const titles = view.rows.map((item) => `${item.run ? '  ' : ''}${item.title.split(' · ')[0]}`);
  assert.deepEqual(titles, ['Fix the login', 'Check the dependencies', '  started', '  started', 'Check the dependencies']);
  const ran = runs(app).filter((run) => run.routine === entry.id).map((run) => run.at);
  assert.ok(ran[0] !== ran[1]);
  const shown = app.queueListShown(list(app)).filter((item) => item.run).map((item) => item.entry.at);
  assert.deepEqual(shown, [...shown].sort((a, b) => b - a));
  assert.equal(view.rows[4].dim, true);
  app.destroy();
});

test('a click on the mark or the arrow keys fold and unfold a routine\'s runs, and the fold stays', () => {
  const app = appAt(140);
  routine(app);
  app.advance(4 * 3600000 + 1000);
  const o = queueModal(app);
  const box = inner(issuesArea(app.cols, app.rows));
  click(app, box.x + 1, box.y + 2);
  assert.equal(app.queueListView(o).rows.length, 1);
  assert.equal(app.queueListView(o).rows[0].hidden, ' · 2 runs');
  app.render();
  assert.ok(app.textAt('▸ '));
  assert.ok(app.textAt(' · 2 runs'));
  inputKey(app, key('Escape'));
  const again = queueModal(app);
  assert.equal(app.queueListView(again).rows.length, 1);
  inputKey(app, key('ArrowRight'));
  assert.equal(app.queueListView(again).rows.length, 3);
  inputKey(app, key('ArrowDown'));
  assert.ok(app.queueListView(again).hint.startsWith('enter opens its pane'));
  inputKey(app, key('ArrowLeft'));
  assert.equal(app.queueListView(again).rows.length, 1);
  assert.equal(again.selected, app.queued.find((item) => item.repeat)?.id);
  type(app, 'started');
  assert.deepEqual(app.queueListView(again).rows.map((item) => [item.run, item.fold]), [[false, null], [true, null], [true, null]]);
  app.destroy();
});

test('the selection stays on its entry when a routine fires above it', () => {
  const app = appAt(140);
  routine(app);
  app.advance(2 * 3600000 + 1000);
  const o = queueModal(app);
  inputKey(app, key('ArrowDown'));
  const first = o.selected;
  app.advance(2 * 3600000);
  assert.equal(app.queueListShown(o)[1].entry.id !== first, true);
  assert.equal(o.selected, first);
  app.destroy();
});

test('Enter in the prompt adds a line, a paste keeps its line breaks and the schedule button saves', () => {
  const app = appAt(140);
  queueModal(app);
  pressButton(app, 'new');
  type(app, 'first');
  inputKey(app, key('Enter'));
  type(app, 'second');
  assert.equal(form(app).prompt, 'first\nsecond');
  assert.equal(app.overlay?.kind, 'queue');
  app.paste('\nthird\r\nfourth');
  assert.equal(form(app).prompt, 'first\nsecond\nthird\nfourth');
  app.render();
  assert.ok(app.textAt('enter breaks the line · tab moves on'));
  assert.equal(app.textAt('↵'), null);
  pick(form(app).agent, 'claude');
  pressButton(app, 'schedule');
  assert.equal(app.queued[0].prompt, 'first\nsecond\nthird\nfourth');
  assert.equal(list(app).selected, app.queued[0].id);
  app.destroy();
});

test('the prompt is taller in a tall window, and the wheel scrolls it with marks for the hidden lines', () => {
  const app = appAt(140);
  app.openQueueForm();
  const o = form(app);
  const tall = queueRect(screen(app), o, 'prompt');
  assert.equal(tall.h, 8);
  o.prompt = Array.from({ length: 12 }, (_, i) => `line ${i + 1}`).join('\n');
  o.cursor = 0;
  app.render();
  assert.ok(app.textAt(' ↓ 4 more '));
  const field = queueRect(screen(app), o, 'prompt');
  app.wheel(field.x + 2, field.y, 3);
  app.render();
  assert.ok(app.textAt(' ↑ 3 more '));
  assert.ok(app.textAt(' ↓ 1 more '));
  assert.ok(app.textAt('line 4'));
  click(app, field.x, field.y);
  assert.equal(o.cursor, [...'line 1\nline 2\nline 3\n'].length);
  const short = new App(); short.virtual = true; short.resize(100, 24); world(short, { scripted: true });
  short.openQueueForm();
  assert.equal(queueRect(screen(short), form(short), 'prompt').h, 3);
  short.destroy();
  app.destroy();
});

const behind = (app: App, o: QueueListDialog, add: () => void) => {
  app.overlay = null;
  add();
  app.overlay = o;
};

test('the row shown first stays selected when an earlier entry arrives above it', () => {
  const app = appAt(140);
  schedule(app, 'Fix the login', 60);
  const first = app.queued[0].id;
  const o = queueModal(app);
  app.render();
  behind(app, o, () => schedule(app, 'Update the docs', 1));
  app.render();
  assert.equal(app.queueListShown(o)[0].entry.label, 'Update the docs');
  assert.equal(o.selected, first);
  app.destroy();
});

test('a save the filter would hide clears the filter and selects the saved entry', () => {
  const app = appAt(140);
  schedule(app, 'Fix the login', 60);
  const o = queueModal(app);
  type(app, 'login');
  pressButton(app, 'new');
  type(app, 'Update the docs');
  pick(form(app).agent, 'claude');
  pressButton(app, 'schedule');
  assert.equal(list(app), o);
  assert.equal(o.filter, '');
  assert.equal(o.selected, app.queued.find((entry) => entry.label === 'Update the docs')?.id);
  app.destroy();
});

test('a click acts on the entry drawn under it even when a row arrives before the click', () => {
  const app = appAt(140);
  schedule(app, 'Fix the login', 60);
  const o = queueModal(app);
  app.render();
  const row = app.textAt('Fix the login');
  assert.ok(row);
  behind(app, o, () => schedule(app, 'Update the docs', 1));
  app.pointerDown(row.x, row.y, 0);
  app.pointerUp();
  assert.equal(o.selected, app.queued.find((entry) => entry.label === 'Fix the login')?.id);
  app.destroy();
});
