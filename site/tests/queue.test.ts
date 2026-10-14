import assert from 'node:assert/strict';
import { test } from 'node:test';
import { App } from '../src/lib/demo/app';
import { Agent } from '../src/lib/demo/programs';
import { inputKey } from '../src/lib/demo/client';
import { world } from '../src/lib/demo/world';

const key = (key: string) => ({ key, metaKey: false, isComposing: false, ctrlKey: false, altKey: false, shiftKey: false });

function appAt(cols: number): App {
  const app = new App(); app.virtual = true; app.resize(cols, 38); world(app, { scripted: true });
  return app;
}

for (const cols of [140, 70]) {
  test(`the queue form schedules, edits and cancels prompts at ${cols} columns`, () => {
    const app = appAt(cols);
    app.toggleQueue();
    app.render();
    assert.ok(app.textAt('+ New'));
    app.openQueueForm();
    const form = app.overlay;
    assert.equal(form?.kind, 'queue');
    if (form?.kind !== 'queue') throw new Error('queue form');
    form.prompt = 'Fix the login'; form.at = 'in 1m'; form.agent = 'claude';
    app.render();
    assert.ok(app.textAt('Queued agents always accept'));
    inputKey(app, key('Enter'));
    assert.equal(app.queued[0].state, 'pending');
    app.openQueueForm(app.queued[0]);
    const edit = app.overlay;
    if (edit?.kind !== 'queue') throw new Error('edit form');
    edit.at = 'in 2m';
    inputKey(app, key('Enter'));
    assert.equal(app.queued.length, 1);
    assert.equal(app.queued[0].at, 120000);
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
}

test('entries due together start separate agents without starting early or moving focus', () => {
  const app = appAt(140);
  app.config.trust = false;
  app.config.submit = false;
  const before = app.tab()?.id;
  for (const branch of ['first-queued', 'second-queued']) {
    app.openQueueForm();
    const form = app.overlay;
    if (form?.kind !== 'queue') throw new Error('form');
    form.project = app.project()!.id; form.workspace = null; form.worktree = true; form.branch = branch;
    form.prompt = 'Fix the login'; form.agent = 'claude'; form.at = 'in 1m';
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
  const form = app.overlay;
  assert.equal(form?.kind, 'queue');
  if (form?.kind !== 'queue') throw new Error('form');
  assert.ok(form.label?.includes('#'));
  assert.ok(form.branch);
  form.at = 'claude reset';
  app.submitQueue();
  assert.equal(app.queued[0].at, app.now() + 2 * 3600000);
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
  const form = app.overlay;
  if (form?.kind !== 'queue') throw new Error('form');
  form.prompt = 'Fix the login'; form.at = 'in 1h'; form.agent = 'claude';
  app.submitQueue();
  const before = [...app.queued];
  app.removeQueued((entry) => entry.id === before[0].id, `removed queue entry ${before[0].id}`);
  assert.equal(app.queued.length, 0);
  app.toast?.undo?.();
  assert.deepEqual(app.queued, before);
  app.destroy();
});
