import assert from 'node:assert/strict';
import { test } from 'node:test';
import { App } from '../src/lib/demo/app';
import { type Combo, chooseCombo } from '../src/lib/demo/combo';
import { MORE_LABEL, helpButton, moreButton, queueButton } from '../src/lib/demo/layout';
import { type QueueForm, WHEN, ordinal, queueAt, queueControls, whenCombo } from '../src/lib/demo/queue';
import { queueButtons } from '../src/lib/demo/queuelist';
import { world } from '../src/lib/demo/world';

const pick = (c: Combo, label: string) => chooseCombo(c, c.options.indexOf(label));

function demo(): App {
  const app = new App(); app.virtual = true; app.resize(110, 38); world(app, { scripted: true });
  return app;
}

function form(app: App): QueueForm {
  const o = app.overlay;
  if (o?.kind !== 'queue') throw new Error('queue form');
  return o;
}

function waitForRoom(app: App, prompt: string): number {
  app.openQueue();
  app.openQueueForm();
  const o = form(app);
  o.prompt = prompt;
  pick(o.agent, 'claude');
  app.queueChoose(o, 'when', WHEN.findIndex(([kind]) => kind === 'free'));
  app.submitQueue();
  assert.equal(app.overlay?.kind, 'queuelist', o.error);
  return Math.max(...app.queued.map((entry) => entry.id));
}

const rows = (app: App) => {
  app.openQueue();
  const o = app.overlay;
  if (o?.kind !== 'queuelist') throw new Error('queue list');
  return app.queueListView(o).rows.map((row) => [row.when, row.title, row.meta.split(' · claude')[0]]);
};

test('when there is room is the last kind of time, with no values and its own hint', () => {
  const app = demo();
  app.openQueueForm();
  const o = form(app);
  assert.deepEqual(WHEN.at(-1), ['free', 'when there is room']);
  o.when = whenCombo('free');
  assert.deepEqual(queueControls(o).slice(0, 3), ['prompt', 'when', 'place']);
  assert.equal(queueAt(o), '');
  app.render();
  assert.ok(app.textAt('starts on the machine with room, in turn'));
  assert.ok(app.textAt('Queued agents always accept'));
  app.destroy();
});

test('an entry that waits for room starts at once on this computer while it has room, and says where', () => {
  const app = demo();
  const id = waitForRoom(app, 'Fix the login form');
  const entry = app.queued.find((item) => item.id === id);
  assert.equal(entry?.state, 'started');
  app.render();
  assert.ok(app.textAt(`started queued agent ${id} on this computer`));
  assert.equal(app.queueWaiting(), 0);
  app.destroy();
});

test('the next one waits in line while the memory the first took is reserved, then starts', () => {
  const app = demo();
  waitForRoom(app, 'First task');
  const second = waitForRoom(app, 'Second task');
  const third = waitForRoom(app, 'Third task');
  assert.deepEqual(rows(app).slice(0, 2), [
    ['#1 in line', 'Second task', 'waits 0m · needs 4 GB · best: this computer 2 GB'],
    ['#2 in line', 'Third task', `waits 0m · behind queued agent ${second}`],
  ]);
  assert.equal(app.queueWaiting(), 2);

  app.advance(4 * 60000);
  assert.equal(rows(app)[0][2], 'waits 4m · needs 4 GB · best: this computer 2 GB');

  app.advance(90000);
  const started = (id: number) => app.queued.find((entry) => entry.id === id)?.state;
  assert.deepEqual([started(second), started(third)], ['started', 'pending']);
  assert.deepEqual(rows(app)[0], ['#1 in line', 'Third task', 'waits 5m · needs 4 GB · best: this computer 2 GB']);
  app.destroy();
});

test('a start needs that does not fit keeps the line waiting and says so once', () => {
  const app = demo();
  app.config.defaultMemory = '8GB';
  const id = waitForRoom(app, 'Big build');
  app.render();
  assert.ok(app.textAt(`queued agent ${id} waits for room: 1st in line, needs 8 GB · best: this computer 6 GB`));
  assert.deepEqual(rows(app)[0], ['#1 in line', 'Big build', 'waits 0m · needs 8 GB · best: this computer 6 GB']);
  assert.deepEqual(['1st', '2nd', '3rd', '4th', '11th', '12th', '21st'], [1, 2, 3, 4, 11, 12, 21].map(ordinal));
  app.destroy();
});

test('first puts an entry behind the head at the head, and start now starts it without counting what it needs', () => {
  const app = demo();
  app.config.defaultMemory = '8GB';
  const head = waitForRoom(app, 'Head');
  const behind = waitForRoom(app, 'Behind');
  const entry = (id: number) => app.queued.find((item) => item.id === id)!;
  assert.deepEqual(queueButtons(entry(head), false, 100, 1), ['new', 'start now', 'remove', 'done']);
  assert.deepEqual(queueButtons(entry(behind), false, 100, 2), ['new', 'start now', 'first', 'remove', 'done']);

  app.openQueue();
  const o = app.overlay;
  if (o?.kind !== 'queuelist') throw new Error('queue list');
  o.selected = behind;
  app.queueListButton('first');
  assert.deepEqual(rows(app).slice(0, 2).map(([when, title]) => [when, title]), [['#1 in line', 'Behind'], ['#2 in line', 'Head']]);

  app.openQueue();
  const again = app.overlay;
  if (again?.kind !== 'queuelist') throw new Error('queue list');
  again.selected = behind;
  app.queueListButton('start now');
  assert.equal(entry(behind).state, 'started');
  assert.deepEqual(rows(app)[0], ['#1 in line', 'Head', 'waits 0m · needs 8 GB · best: this computer 6 GB']);
  app.destroy();
});

test('an entry that waits cannot take a time, and one with a time cannot start waiting', () => {
  const app = demo();
  app.config.defaultMemory = '8GB';
  const id = waitForRoom(app, 'Waits');
  app.openQueueForm(app.queued.find((entry) => entry.id === id));
  assert.equal(form(app).when.options[form(app).when.chosen ?? -1], 'when there is room');
  form(app).when = whenCombo('in');
  app.submitQueue();
  assert.equal(form(app).error, 'an entry that waits for room has no time: it starts as soon as a machine has room');
  app.closeQueueForm();

  app.openQueueForm();
  const o = form(app);
  o.prompt = 'Timed'; o.amount = '30'; pick(o.agent, 'claude'); pick(o.unit, 'minutes');
  app.submitQueue();
  const timed = Math.max(...app.queued.map((entry) => entry.id));
  app.openQueueForm(app.queued.find((entry) => entry.id === timed));
  form(app).when = whenCombo('free');
  app.submitQueue();
  assert.equal(form(app).error, 'an entry with a time cannot start waiting for room; add a new entry');
  app.destroy();
});

test('the footer shows queue and the count between More and Help while entries wait, and opens the Queue window', () => {
  const app = demo();
  const areas = app.areas();
  assert.equal(queueButton(areas.more, 0).w, 0);
  const shown = queueButton(areas.more, 3);
  const help = helpButton(areas.more);
  assert.equal(shown.x + shown.w, help.x);
  assert.ok(moreButton(areas.more).w >= shown.w + MORE_LABEL.length + 2);

  app.config.defaultMemory = '8GB';
  waitForRoom(app, 'Waits');
  waitForRoom(app, 'Waits too');
  app.closeOverlay();
  app.render();
  const button = app.textAt(' queue 2 ');
  assert.ok(button);
  assert.equal(app.render().grid.at(button.x + 1, button.y)?.fg, 3);
  app.pointerDown(button.x + 1, button.y, 0);
  app.pointerUp();
  assert.equal(app.overlay?.kind, 'queuelist');

  app.closeOverlay();
  app.render();
  const more = app.textAt(MORE_LABEL);
  assert.ok(more);
  app.pointerDown(more.x, more.y, 0);
  app.pointerUp();
  app.render();
  assert.ok(app.textAt('queue · 2 waiting'));
  app.destroy();
});
