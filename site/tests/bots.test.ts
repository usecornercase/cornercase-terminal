import assert from 'node:assert/strict';
import { test } from 'node:test';

import { App } from '../src/lib/demo/app';
import { inputKey } from '../src/lib/demo/client';
import { type BotForm } from '../src/lib/demo/botform';
import { EXAMPLES, draftOf } from '../src/lib/demo/bots';
import { plain } from '../src/lib/demo/markdown';
import { Rows, layout } from '../src/lib/demo/layout';
import { projectLabel } from '../src/lib/demo/model';
import { world } from '../src/lib/demo/world';

const key = (key: string, shift = false) => ({ key, metaKey: false, isComposing: false, ctrlKey: false, altKey: false, shiftKey: shift });

function panes(columns: number, rows = 38): App {
  const app = new App();
  app.virtual = true;
  app.scripted = true;
  app.resize(columns, rows);
  world(app, { scripted: true });
  if (columns < 90) app.navTo('projects');
  app.render();
  return app;
}

function demo(columns: number): App {
  const app = panes(columns);
  if (columns >= 90) app.bots.page = true;
  app.render();
  return app;
}

function newOnThePage(app: App): void {
  const a = app.areas();
  const add = new Rows(a.botsList, Array<number>(app.bots.list.length).fill(a.pitch), a.pitch, 0).buttonRect();
  app.pointerDown(add.x + 2, add.y, 0);
  app.pointerUp();
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

test('the page lists both seeded bots with their agent and an unread mark', () => {
  const text = screen(demo(140));
  assert.match(text, /‹ Projects +Bots/);
  assert.doesNotMatch(text, /Workspaces|web-shop/);
  assert.match(text, /prs +next /);
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
  assert.equal(app.bots.current()?.lines.length, 15, 'nothing was sent');
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

test('the new bot form creates a bot and shows its empty chat with the four examples', () => {
  const app = demo(140);
  newOnThePage(app);
  assert.equal(app.overlay?.kind, 'bot');
  assert.match(screen(app), /New bot/);
  type(app, 'nightly');
  inputKey(app, key('Enter'));
  assert.equal(app.overlay, null);
  const text = screen(app);
  assert.match(text, /Message nightly…/);
  for (const example of EXAMPLES) assert.ok(text.includes(example), example);
  assert.match(text, /‹ Projects +Bots/);
});

test('a bot made with For filled in sets itself up in its chat', () => {
  const app = demo(140);
  app.openBotForm();
  type(app, 'nightly');
  (app.overlay as BotForm).about = draftOf('Look for bugs every day');
  inputKey(app, key('Enter'));
  assert.equal(app.overlay, null);
  const text = screen(app);
  assert.match(text, /── setting up ──/);
  assert.match(text, /thinking/);
  for (const example of EXAMPLES) assert.ok(!text.includes(example), example);
  app.advance(6000);
  const bot = app.bots.current();
  assert.equal(bot?.lines[bot.lines.length - 1].kind, 'bot');
  assert.match(bot?.lines[bot.lines.length - 1].text ?? '', /Set up: every day at/);
  assert.equal(app.botQueued.filter((e) => e.bot === bot?.id && !e.routine).length, 1);
  assert.match(screen(app), /nightly +next /);
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
  for (const k of ['ArrowDown', 'ArrowDown', 'ArrowDown', 'ArrowDown', 'ArrowDown', 'Enter']) inputKey(app, key(k));
  const o = app.overlay;
  assert.equal(o?.kind, 'bot');
  if (o?.kind === 'bot') assert.equal(o.agent.chosen, 0);
});

test('opencode offers full access only, with its reason', () => {
  const app = demo(140);
  app.openBotForm();
  inputKey(app, key('Tab'));
  for (const k of ['ArrowDown', 'ArrowDown', 'ArrowDown', 'ArrowDown', 'Enter']) inputKey(app, key(k));
  const o = app.overlay;
  assert.equal(o?.kind, 'bot');
  if (o?.kind !== 'bot') return;
  assert.equal(o.agent.chosen, 3);
  assert.equal(o.access.options[0], 'through cornercase · a command can write outside its folder');
  assert.equal(o.access.chosen, null);
  assert.deepEqual(o.access.off, [0]);
});

test('gemini says it reads and writes only its folder, claude that it reads anything', () => {
  const app = demo(140);
  app.openBotForm();
  assert.match(screen(app), /through cornercase: reads anything, writes only in its folder/);
  inputKey(app, key('Tab'));
  for (const k of ['ArrowDown', 'ArrowDown', 'ArrowDown', 'Enter']) inputKey(app, key(k));
  assert.match(screen(app), /through cornercase: reads and writes only in its folder/);
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

test('‹ Projects goes back to the panes, and a click on a project then shows it', () => {
  const app = demo(140);
  press(app, 'prs');
  press(app, '‹ Projects');
  assert.equal(app.bots.current(), null);
  assert.equal(app.bots.page, false);
  assert.doesNotMatch(screen(app), /Message prs…/);
  press(app, 'blog');
  assert.equal(projectLabel(app.projects[app.active]), 'blog');
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
  for (const text of ['one', 'two', 'three', 'four', 'five', 'six']) app.bots.add(bot, Date.now(), 'step', text);
  press(app, 'prs');
  const folded = screen(app);
  assert.match(folded, /\+3 steps/);
  assert.doesNotMatch(folded, /▸ one/);
  press(app, '+3 steps');
  const open = screen(app);
  assert.match(open, /▸ one/);
  assert.doesNotMatch(open, /\+3 steps/);
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

test('the Bots button drops its marks before they cover the Projects title', () => {
  const app = panes(140);
  app.widths = { ...app.widths, projects: 22 };
  app.bots.list[0].unread = 2;
  app.bots.list[1].turn = { since: 0, steps: 0, stopping: false };
  app.render();
  const a = app.areas();
  assert.equal(a.botsButton.w, 8);
  const row = screen(app).split('\n')[a.title.y].slice(0, a.title.w);
  assert.equal(row, ' Projects     Bots › ');
  app.widths = { ...app.widths, projects: 32 };
  app.render();
  assert.match(screen(app).split('\n')[app.areas().title.y], / Projects +\S ●3 Bots › /);
});

test('at the landing’s 32 rows the Bots button opens a page with a bot to click', () => {
  const app = panes(120, 32);
  press(app, 'Bots ›');
  assert.ok(app.textAt('digest'), screen(app));
});

test('a bot with a routine shows when it runs next, in its row and on its card', () => {
  const app = demo(140);
  assert.match(screen(app), /prs +next /);
  press(app, 'prs');
  app.resize(140, 60);
  assert.match(screen(app), /routine +Review PRs[^\n]*\n[^\n]*next [^\n]*↻ 2h/);
});

test('a routine run is one block in the chat: its header, its steps folded and its answer', () => {
  const app = demo(140);
  press(app, 'prs');
  const bot = app.bots.current();
  assert.ok(bot);
  bot.scroll = 1000;
  const text = screen(app);
  assert.match(text, /↻ Review PRs · (.+ )?\d\d:\d\d/);
  assert.match(text, /\+2 steps/);
  assert.match(text, /Two are blocked/);
  const lines = app.bots.current()?.lines.filter((l) => l.routine !== undefined) ?? [];
  assert.deepEqual(lines.map((l) => l.kind), ['notice', 'step', 'step', 'bot']);
  press(app, '+2 steps');
  assert.match(screen(app), /▸ gh pr list/);
});

function opened(app: App): void {
  press(app, 'prs');
  press(app, 'routines');
}

test('the Queue window lists no bot routine and ends with a line saying where they are', () => {
  const app = new App();
  app.virtual = true;
  app.resize(140, 38);
  world(app);
  app.openQueue();
  app.render();
  const text = screen(app);
  assert.doesNotMatch(text, /Review PRs/);
  assert.doesNotMatch(text, /bot prs/);
  assert.match(text, /nothing queued/);
  assert.match(text, /1 bot routine: in its bot's chat/);
  assert.equal(app.queued.length, 0);
});

test('routines in the chat’s header lists the routine, and a click on it shows the chat at its run', () => {
  const app = demo(140);
  app.resize(140, 38);
  opened(app);
  const text = screen(app);
  assert.match(text, /Routines › prs/);
  assert.match(text, /Review PRs/);
  assert.match(text, /↻ 2h · next /);
  assert.match(text, /last run answered/);
  assert.match(text, /enter shows its last run in the chat/);
  const row = app.textAt('Review PRs');
  assert.ok(row);
  app.pointerDown(row.x + 1, row.y, 0);
  app.pointerUp();
  app.render();
  assert.equal(app.overlay, null);
  assert.equal(app.bots.shown, 'prs');
  assert.match(screen(app), /↻ Review PRs/);
});

test('pause, resume and remove in the routines dialog, and undo brings it back', () => {
  const app = demo(140);
  app.resize(140, 38);
  opened(app);
  press(app, 'pause');
  assert.match(screen(app), /↻ 2h · paused/);
  assert.match(app.toast?.text ?? '', /paused routine/);
  app.bots.show('prs');
  app.render();
  assert.doesNotMatch(screen(app), /prs +next /);
  app.openBotRoutines('prs');
  press(app, 'resume');
  assert.match(screen(app), /↻ 2h · next /);
  press(app, 'remove');
  assert.match(screen(app), /has no routines yet/);
  assert.equal(app.botQueued.length, 0);
  assert.match(app.toast?.text ?? '', /removed routine 1 and its 1 run/);
  app.toast?.undo?.();
  app.render();
  assert.match(screen(app), /Review PRs/);
  assert.equal(app.botQueued.length, 2);
  press(app, 'run now');
  assert.match(app.toast?.text ?? '', /needs cornercase on your computer/);
  press(app, 'done');
  assert.equal(app.overlay, null);
});

test('the dialog at phone width keeps how the last run ended clear of the buttons', () => {
  const app = demo(48);
  app.openBotRoutines('prs');
  const text = screen(app);
  assert.match(text, /last run answered/);
  assert.match(text, /run now {2}pause {2}edit… {2}remove/);
  assert.match(text, /│ ▌Review PRs/);
});

test('the empty routines dialog ends its example with one full stop', () => {
  const app = demo(140);
  app.botQueued = [];
  app.openBotRoutines('prs');
  assert.doesNotMatch(screen(app), /blocked\."\./);
  assert.match(screen(app), /prs has no routines yet\./);
});

test('the card never ends on a routine name without its time row', () => {
  let seen = 0;
  for (let rows = 38; rows < 70; rows++) {
    const app = demo(140);
    press(app, 'prs');
    app.resize(140, rows);
    const lines = screen(app).split('\n');
    lines.forEach((line, i) => {
      if (/^ routine +Review PRs/.test(line)) {
        seen++;
        assert.match(lines[i + 1] ?? '', /next |paused/, `rows ${rows}`);
      }
    });
  }
  assert.ok(seen > 0, 'no routine row found');
});

test('the dialog clips a button to its cell: the border survives a row narrower than its button', () => {
  const app = demo(10);
  app.botQueued = [{ ...app.botQueued[0], id: 90, routine: undefined, repeat: undefined, state: 'answered', at: Date.now() - 60000 }];
  app.openBotRoutines('prs');
  const lines = screen(app).split('\n');
  const at = lines.findIndex((l) => l.includes('rem'));
  assert.ok(at >= 0, 'the remove button is drawn');
  assert.match(lines[at], /│$/);
});

test('opening the dialog pins the first routine to its entry', () => {
  const app = demo(140);
  app.openBotRoutines('prs');
  assert.ok(app.overlay?.kind === 'botroutines');
  assert.equal(app.overlay.selected, app.botQueued.find((e) => !e.routine)?.id);
});

test('a routine on the card opens the routines dialog', () => {
  const app = demo(140);
  const first = app.botQueued.find((e) => !e.routine)!;
  app.botQueued.push({ ...first, id: 91, label: 'Early reminder', repeat: undefined, at: first.at - 3600000, state: 'pending' });
  app.botQueued.sort((a, b) => a.at - b.at || a.id - b.id);
  press(app, 'prs');
  app.resize(140, 60);
  press(app, 'routine', 1);
  assert.equal(app.overlay?.kind, 'botroutines');
  const shown = app.overlay;
  assert.ok(shown?.kind === 'botroutines');
  assert.equal(shown.selected, app.botQueued.find((e) => e.repeat)?.id);
  assert.match(screen(app), /Routines › prs/);
});

function cardRoutine(app: App): { x: number; y: number } {
  app.render();
  const list = app.areas().workspacesList;
  for (let n = 0; n < 8; n++) {
    const at = app.textAt('Review PRs', n);
    if (!at) break;
    if (at.x >= list.x && at.x < list.x + list.w) return at;
  }
  assert.fail('no routine on the card');
}

function cardButton(app: App, y: number): { x: number; y: number } | undefined {
  const list = app.areas().workspacesList;
  const { grid } = app.render();
  for (let x = list.x; x < list.x + list.w; x++) if (grid.at(x, y)?.ch === '⋯') return { x, y };
  return undefined;
}

test('the ⋯ on a routine of the card opens its menu: pause, resume and remove act, run now and edit… say so', () => {
  const app = demo(140);
  press(app, 'prs');
  app.resize(140, 60);
  const at = cardRoutine(app);
  app.pointerMove(120, 2, 0);
  app.render();
  assert.equal(cardButton(app, at.y), undefined);
  app.pointerMove(at.x, at.y, 0);
  app.render();
  const button = cardButton(app, at.y);
  assert.ok(button, 'the ⋯ shows while the routine is hovered');
  app.pointerDown(button.x, button.y, 0);
  app.pointerUp();
  app.render();
  assert.equal(app.overlay?.kind, 'menu');
  for (const label of ['run now', 'pause', 'edit…', 'remove']) assert.match(screen(app), new RegExp(label));
  press(app, 'pause');
  assert.match(app.toast?.text ?? '', /paused routine/);
  const list = app.areas().workspacesList;
  assert.match(screen(app).split('\n')[at.y + 1].slice(list.x, list.x + list.w), /paused/);
  app.pointerDown(at.x, at.y + 1, 2);
  app.pointerUp();
  app.render();
  assert.equal(app.overlay?.kind, 'menu');
  assert.match(screen(app), /resume/);
  press(app, 'resume');
  assert.match(screen(app).split('\n')[at.y + 1].slice(list.x, list.x + list.w), /next/);
  app.pointerDown(at.x, at.y, 2);
  app.pointerUp();
  app.render();
  const before = app.botQueued.length;
  press(app, 'remove');
  assert.match(app.toast?.text ?? '', /removed routine 1 and its 1 run/);
  assert.equal(app.botQueued.length, 0);
  app.toast?.undo?.();
  assert.equal(app.botQueued.length, before);
  app.render();
  app.pointerDown(at.x, at.y, 2);
  app.pointerUp();
  app.render();
  press(app, 'run now');
  assert.match(app.toast?.text ?? '', /needs cornercase on your computer/);
  app.pointerDown(at.x, at.y, 2);
  app.pointerUp();
  app.render();
  app.toast = null;
  press(app, 'edit…');
  assert.match((app.toast as App['toast'])?.text ?? '', /schedule form lists no bots/);
});

test('a card menu item that no longer applies does nothing', () => {
  const app = demo(140);
  press(app, 'prs');
  app.resize(140, 60);
  const at = cardRoutine(app);
  const entry = app.botQueued.find((e) => e.repeat)!;
  app.pauseQueued(entry, true, 'botQueued');
  app.pointerDown(at.x, at.y, 2);
  app.pointerUp();
  app.render();
  assert.match(screen(app), /resume/);
  app.pauseQueued(entry, false, 'botQueued');
  app.toast = null;
  assert.equal(app.overlay?.kind, 'menu');
  press(app, 'resume');
  assert.equal(app.overlay, null);
  assert.equal(app.toast, null);
  assert.equal(entry.paused, false);
});

test('the empty chat offers the scheduling example', () => {
  assert.equal(EXAMPLES.length, 4);
  assert.match(EXAMPLES[3], /^Every weekday at 9/);
});

test('the prs chat shows an agent it started and the report after it finished', () => {
  const app = demo(140);
  const bot = app.bots.list.find((b) => b.id === 'prs');
  assert.ok(bot);
  assert.deepEqual(
    bot.lines.filter((l) => l.kind === 'started' || l.kind === 'finished').map((l) => l.kind),
    ['started', 'finished'],
  );
  assert.equal(bot.lines.find((l) => l.kind === 'finished')?.agent?.kind, 'claude');
  press(app, 'prs');
  const text = screen(app);
  assert.match(text, /▶ started claude in web-shop › fix\/macos-test +show/);
  assert.match(text, /✓ claude finished in web-shop › fix\/macos-test +show/);
  assert.match(text, /Fixed the flaky path/);
  assert.match(text, /#490 passes `?test \(macos\)`? now/);
});

test('an answer that arrives while its chat is not shown toasts its first line without Markdown', () => {
  const app = demo(140);
  press(app, 'prs');
  type(app, 'and #501?');
  inputKey(app, key('Enter'));
  press(app, '‹ Projects');
  app.advance(5000);
  assert.equal(app.toast?.text, 'prs: Two are blocked:');
});

test('the plain text of Markdown leaves a code block as it is', () => {
  assert.equal(plain('```sh\n# rerun *all*\n```'), '# rerun *all*');
  assert.deepEqual(
    plain('Two are **blocked**:\n\n- #482 waits\n\n```sh\ngh run rerun\n```').split('\n').filter(Boolean),
    ['Two are blocked:', '#482 waits', 'gh run rerun'],
  );
});

test('the Bots button sits at the right of the Projects title with the unread count of every bot', () => {
  const app = panes(140);
  const a = app.areas();
  assert.equal(a.botsButton.y, a.title.y);
  assert.equal(a.botsButton.x + a.botsButton.w, a.title.x + a.title.w);
  const row = screen(app).split('\n')[a.title.y];
  assert.match(row, /Projects +●1 Bots › /);
  app.bots.list[0].unread = 11;
  assert.match(screen(app).split('\n')[a.title.y], /●9\+ Bots › /);
});

test('the Bots button opens the page on the first bot, and then on the bot read last', () => {
  const app = panes(140);
  press(app, 'Bots ›');
  assert.equal(app.bots.page, true);
  assert.equal(app.bots.current()?.id, 'prs');
  const text = screen(app);
  assert.match(text, /‹ Projects +Bots/);
  assert.match(text, /Message prs…/);
  press(app, 'digest');
  press(app, '‹ Projects');
  assert.equal(app.bots.page, false);
  press(app, 'Bots ›');
  assert.equal(app.bots.current()?.id, 'digest');
});

test('the page takes the place of the agents section', () => {
  const app = panes(140);
  app.config.agentsSection = true;
  assert.ok(app.areas().agents.h > 0);
  press(app, 'Bots ›');
  assert.equal(app.areas().agents.h, 0);
  assert.doesNotMatch(screen(app), /no agents running|Agents/);
});

test('with no bot the page says what a bot is, and an idea opens the form filled in', () => {
  const app = panes(140, 44);
  app.bots.list = [];
  press(app, 'Bots ›');
  assert.equal(app.bots.lobby(), 'empty');
  const text = screen(app);
  assert.match(text, /Meet your bots/);
  assert.match(text, /\+ New bot/);
  assert.match(text, /Start from an idea/);
  press(app, 'Releases');
  const form = app.overlay as BotForm;
  assert.equal(form.kind, 'bot');
  assert.equal(form.name.chars.join(''), 'Releases');
  assert.match(form.about.chars.join(''), /^Every Monday, check the new releases/);
  app.closeOverlay();
  press(app, '+ New bot');
  assert.equal((app.overlay as BotForm).name.chars.length, 0);
});

test('with bots and no chat chosen the page asks to pick one, and keys reach no pane', () => {
  const app = demo(140);
  assert.equal(app.bots.lobby(), 'pick');
  const text = screen(app);
  assert.match(text, /Your bots/);
  assert.match(text, /Pick a bot in the list/);
  assert.doesNotMatch(text, /Start from an idea/);
  app.toggleTodo();
  inputKey(app, key('Escape'));
  assert.equal(app.todo.open, false);
  assert.equal(app.bots.page, true);
  inputKey(app, key('Escape'));
  assert.equal(app.bots.page, true);
  assert.equal(inputKey(app, key('q')), true);
});

test('below 90 columns Bots › with no bot opens the lobby full screen with Bots as the breadcrumb', () => {
  const app = demo(80);
  app.bots.list = [];
  press(app, 'Bots ›');
  assert.equal(app.nav, null);
  const text = screen(app);
  assert.match(text, /Meet your bots/);
  assert.match(text.split('\n')[1], /Bots/);
  press(app, '≡');
  assert.equal(app.nav, 'bots');
  press(app, '‹ Projects');
  assert.equal(app.nav, 'projects');
  assert.equal(app.bots.page, false);
});
