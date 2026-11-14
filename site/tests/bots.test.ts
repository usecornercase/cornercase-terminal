import assert from 'node:assert/strict';
import { test } from 'node:test';

import { App } from '../src/lib/demo/app';
import { inputKey } from '../src/lib/demo/client';
import { EXAMPLES } from '../src/lib/demo/bots';
import { layout } from '../src/lib/demo/layout';
import { world } from '../src/lib/demo/world';

const key = (key: string, shift = false) => ({ key, metaKey: false, isComposing: false, ctrlKey: false, altKey: false, shiftKey: shift });

function demo(columns: number): App {
  const app = new App();
  app.virtual = true;
  app.scripted = true;
  app.resize(columns, 38);
  world(app, { scripted: true });
  if (columns < 90) app.navTo('projects');
  app.render();
  return app;
}

function press(app: App, label: string, index = 0): void {
  app.render();
  const at = app.textAt(label, index);
  assert.ok(at, `no ${label}`);
  app.pointerDown(at.x + 1, at.y, 0);
  app.pointerUp();
  app.render();
}

function type(app: App, text: string): void {
  for (const ch of text) inputKey(app, key(ch));
}

function screen(app: App): string {
  const { grid } = app.render();
  const rows: string[] = [];
  for (let y = 0; y < app.rows; y++) {
    let row = '';
    for (let x = 0; x < app.cols; x++) row += grid.at(x, y)?.ch ?? ' ';
    rows.push(row.trimEnd());
  }
  return rows.join('\n');
}

test('the section lists both seeded bots with their agent and an unread mark', () => {
  const text = screen(demo(140));
  assert.match(text, /▾ Bots/);
  assert.match(text, /prs +claude · opus/);
  assert.match(text, /●1 digest +codex/);
});

test('a click on a bot shows its chat in place of the panes, with its card', () => {
  const app = demo(140);
  press(app, 'digest');
  const text = screen(app);
  assert.match(text, /digest {2}codex/);
  assert.match(text, /what merged yesterday\?/);
  assert.match(text, /Yesterday in web-shop:/);
  assert.match(text, /access +full access/);
  assert.doesNotMatch(text, /●1 digest/, 'reading it clears the unread mark');
  assert.equal(app.bots.current()?.id, 'digest');
});

test('typing and Enter send a you bubble, then the thinking line, then the answer', () => {
  const app = demo(140);
  press(app, 'prs');
  type(app, 'and #501?');
  inputKey(app, key('Enter'));
  const sent = screen(app);
  assert.match(sent, /and #501\?/);
  assert.match(sent, /thinking · 0s/);
  app.advance(2000);
  assert.match(screen(app), /thinking · 2s · 1 step /);
  app.advance(5000);
  const done = screen(app);
  assert.doesNotMatch(done, /thinking/);
  assert.equal(app.bots.current()?.lines.at(-1)?.kind, 'bot');
  assert.equal(app.bots.current()?.draft.chars.length, 0);
});

test('keys never reach a pane while a chat shows, and Esc clears the field', () => {
  const app = demo(140);
  press(app, 'prs');
  type(app, 'abc');
  assert.equal(app.bots.current()?.draft.chars.join(''), 'abc');
  inputKey(app, key('Escape'));
  assert.equal(app.bots.current()?.draft.chars.length, 0);
  app.paste('one\ntwo');
  assert.equal(app.bots.current()?.draft.chars.join(''), 'one\ntwo');
  assert.equal(app.bots.current()?.lines.length, 5, 'nothing was sent');
});

test('a message sent while a turn runs is held, and answered after it', () => {
  const app = demo(140);
  press(app, 'prs');
  type(app, 'first');
  inputKey(app, key('Enter'));
  type(app, 'second');
  inputKey(app, key('Enter'));
  assert.match(screen(app), /second.*held/);
  app.advance(4300);
  assert.equal(app.bots.current()?.turn?.steps, 0, 'the held message starts the next turn');
  app.advance(5000);
  assert.equal(app.bots.current()?.turn, null);
});

test('the new bot form creates a bot and shows its empty chat with the three examples', () => {
  const app = demo(140);
  const title = app.areas().botsTitle;
  app.pointerDown(title.x + title.w - 2, title.y, 0);
  app.pointerUp();
  assert.equal(app.overlay?.kind, 'bot');
  assert.match(screen(app), /New bot/);
  type(app, 'nightly');
  inputKey(app, key('Enter'));
  assert.equal(app.overlay, null);
  const text = screen(app);
  assert.match(text, /Message nightly…/);
  for (const example of EXAMPLES) assert.ok(text.includes(example), example);
  assert.match(text, /▾ Bots/);
});

test('the form refuses a name in use and a missing name', () => {
  const app = demo(140);
  app.openBotForm();
  inputKey(app, key('Enter'));
  assert.match(screen(app), /name the bot/);
  type(app, 'PRS');
  inputKey(app, key('Enter'));
  assert.match(screen(app), /there is a bot called PRS already/);
  assert.equal(app.overlay?.kind, 'bot');
});

test('a not-yet agent is greyed and cannot be chosen', () => {
  const app = demo(140);
  app.openBotForm();
  inputKey(app, key('Tab'));
  for (const k of ['ArrowDown', 'ArrowDown', 'ArrowDown', 'Enter']) inputKey(app, key(k));
  const o = app.overlay;
  assert.equal(o?.kind, 'bot');
  if (o?.kind === 'bot') assert.equal(o.agent.chosen, 0);
});

test('an example fills the field and sends nothing', () => {
  const app = demo(140);
  app.openBotForm();
  type(app, 'nightly');
  inputKey(app, key('Enter'));
  press(app, EXAMPLES[1].slice(0, 20));
  const bot = app.bots.current();
  assert.equal(bot?.draft.chars.join(''), EXAMPLES[1]);
  assert.equal(bot?.lines.length, 0);
});

test('a click on a project goes back to the panes', () => {
  const app = demo(140);
  press(app, 'prs');
  press(app, 'blog');
  assert.equal(app.bots.current(), null);
  assert.doesNotMatch(screen(app), /Message prs…/);
});

test('the keys menu opens the form with b, and More lists new bot…', () => {
  const app = demo(140);
  app.config.prefix = 'ctrl+]';
  app.openBotForm();
  assert.equal(app.overlay?.kind, 'bot');
  app.closeOverlay();
  press(app, 'More');
  assert.match(screen(app), /new bot…/);
  press(app, 'new bot…');
  assert.equal(app.overlay?.kind, 'bot');
});

test('below 90 columns Bots › opens the list and a tap shows the chat full screen', () => {
  const app = demo(80);
  assert.ok(layout(80, 38, app.widths, 'projects', false, 'side_by_side', false, false, app.bots.shape()).botsButton.w > 0);
  press(app, 'Bots ›');
  const list = screen(app);
  assert.match(list, /‹ Projects/);
  assert.match(list, /digest/);
  press(app, 'prs');
  assert.equal(app.nav, null);
  const text = screen(app);
  assert.match(text, /Bots › prs/);
  assert.match(text, /Message prs…/);
  assert.match(text, /\bsend\b/);
});

test('a run of more than three steps folds under +N steps, and a click unfolds it', () => {
  const app = demo(140);
  const bot = app.bots.list[0];
  for (const text of ['one', 'two', 'three', 'four', 'five']) app.bots.add(bot, Date.now(), 'step', text);
  press(app, 'prs');
  const folded = screen(app);
  assert.match(folded, /\+2 steps/);
  assert.doesNotMatch(folded, /▸ one/);
  press(app, '+2 steps');
  const open = screen(app);
  assert.match(open, /▸ one/);
  assert.doesNotMatch(open, /\+2 steps/);
});

test('scrolling up shows ↓ new when a line arrives, and a click goes back down', () => {
  const app = demo(140);
  press(app, 'prs');
  const bot = app.bots.list[0];
  for (let i = 0; i < 30; i++) app.bots.add(bot, Date.now(), 'notice', `line ${i}`);
  app.render();
  app.bots.scrollTo(bot, 5, 100);
  app.bots.add(bot, Date.now(), 'notice', 'late');
  assert.match(screen(app), /↓ new/);
  press(app, '↓ new');
  assert.equal(bot.scroll, 0);
  assert.doesNotMatch(screen(app), /↓ new/);
});
