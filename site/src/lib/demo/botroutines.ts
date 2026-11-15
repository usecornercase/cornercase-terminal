import { BOLD, type Rect, type Style, rect } from '../term/grid';
import type { App } from './app';
import { type Bot, EXAMPLES, clock, shortClock } from './bots';
import { COMPACT_WIDTH, bottom, buttonWidth, centered, intersect, right } from './layout';
import type { Key } from './programs';
import { type QueueEntry, parseRepeat, queueWhen, repeatWalk } from './queue';
import { truncateRight } from './text';
import type { Painter } from './ui';
import { wrapText } from './botchat';

const MUTED: Style = { fg: 8 };
const PITCH = 2;
const WIDE = 100;
const HIGH = 24;
const NEW = '+ routine';
const DONE = 'done';
const LEAD = 4;
const RUN_LEFT_OUT = 'this needs cornercase on your computer: the demo runs no bot routines';
const FORM_LEFT_OUT = "this needs cornercase on your computer: the demo's schedule form lists no bots";
const HINT_OPEN = 'enter shows its last run in the chat';

export type Button = 'run now' | 'pause' | 'resume' | 'edit…' | 'remove';

export interface BotRoutinesDialog {
  kind: 'botroutines';
  bot: string;
  selected: number | null;
  scroll: number;
}

export const openBotRoutines = (bot: string, selected: number | null = null): BotRoutinesDialog => ({ kind: 'botroutines', bot, selected, scroll: 0 });

export const routinesOf = (app: App, bot: string): QueueEntry[] => app.botQueued.filter((entry) => entry.bot === bot && !entry.routine);

function lastRun(app: App, entry: QueueEntry): QueueEntry | undefined {
  if (entry.repeat) return app.botQueued.filter((run) => run.routine === entry.id).sort((a, b) => b.at - a.at || b.id - a.id)[0];
  return entry.state === 'pending' ? undefined : entry;
}

const leftBlock = (run: QueueEntry): boolean => run.state === 'answered' || run.state === 'failed' || run.state === 'started';

function buttonsOf(entry: QueueEntry): Button[] {
  const runnable = !!entry.repeat || entry.state === 'pending' || entry.state === 'failed' || entry.state === 'missed';
  return [...(runnable ? ['run now' as const] : []), ...(entry.repeat ? [entry.paused ? ('resume' as const) : ('pause' as const)] : []), ...(runnable ? ['edit…' as const] : []), 'remove'];
}

export const botRoutinesArea = (cols: number, rows: number): Rect =>
  cols < COMPACT_WIDTH ? rect(0, 0, cols, rows) : centered(cols, rows, Math.min(Math.max(0, cols - 2), WIDE), Math.min(rows, HIGH));

function parts(area: Rect): { list: Rect; note: Rect; buttons: Rect } {
  const inner = rect(area.x + 3, area.y + 1, Math.max(0, area.w - 6), Math.max(0, area.h - 2));
  return {
    list: rect(inner.x, inner.y, inner.w, Math.max(0, inner.h - 2)),
    note: rect(inner.x, bottom(inner) - 2, inner.w, 1),
    buttons: rect(inner.x, bottom(inner) - 1, inner.w, 1),
  };
}

const roomOf = (list: Rect): number => Math.max(1, Math.floor(list.h / PITCH));
const first = (o: BotRoutinesDialog, len: number, list: Rect): number => Math.min(o.scroll, Math.max(0, len - roomOf(list)));

function indexOf(o: BotRoutinesDialog, entries: QueueEntry[]): number | null {
  const at = entries.findIndex((entry) => entry.id === o.selected);
  return at >= 0 ? at : entries.length ? 0 : null;
}

function move(o: BotRoutinesDialog, entries: QueueEntry[], delta: number, rows: number): void {
  if (!entries.length) return;
  const at = indexOf(o, entries) ?? 0;
  const next = Math.max(0, Math.min(entries.length - 1, at + delta));
  o.selected = entries[next].id;
  if (next < o.scroll) o.scroll = next;
  else if (next >= o.scroll + rows) o.scroll = next + 1 - rows;
}

function pressRow(app: App, o: BotRoutinesDialog, entry: QueueEntry): void {
  o.selected = entry.id;
  const run = lastRun(app, entry);
  if (run && leftBlock(run)) {
    app.closeOverlay();
    app.bots.show(entry.bot ?? o.bot, run.id);
  } else app.dirty();
}

function pressButton(app: App, o: BotRoutinesDialog, entry: QueueEntry, button: Button): void {
  o.selected = entry.id;
  if (button === 'run now') app.notify(RUN_LEFT_OUT);
  else if (button === 'edit…') app.notify(FORM_LEFT_OUT);
  else if (button === 'remove') removeEntry(app, entry);
  else pause(app, entry, button === 'pause');
  app.dirty();
}

function pause(app: App, entry: QueueEntry, paused: boolean): void {
  const repeat = parseRepeat(entry.repeat ?? '');
  if (typeof repeat === 'string') return;
  entry.paused = paused;
  const now = app.now();
  if (!paused && entry.at <= now) entry.at = repeatWalk(repeat, entry.at, now).next;
  app.botQueued.sort((a, b) => a.at - b.at || a.id - b.id);
  app.notify(paused ? `paused routine ${entry.id}` : `resumed routine ${entry.id}, next at ${queueWhen(entry.at)}`);
}

function removeEntry(app: App, entry: QueueEntry): void {
  const gone = app.botQueued.filter((item) => item.id === entry.id || (!!entry.repeat && item.routine === entry.id));
  const runs = gone.length - 1;
  app.botQueued = app.botQueued.filter((item) => !gone.includes(item));
  const text = entry.repeat ? `removed routine ${entry.id} and its ${runs === 1 ? '1 run' : `${runs} runs`}` : `removed queue entry ${entry.id}`;
  app.undoToast(text, () => {
    app.botQueued = [...app.botQueued, ...gone].sort((a, b) => a.at - b.at || a.id - b.id);
    app.toast = null;
    app.dirty();
  });
}

export function botRoutinesKey(app: App, o: BotRoutinesDialog, k: Key): void {
  const entries = routinesOf(app, o.bot);
  const rows = roomOf(parts(botRoutinesArea(app.cols, app.rows)).list);
  const at = indexOf(o, entries);
  if (k.key === 'Escape') app.closeOverlay();
  else if (k.key === 'Enter' && at !== null) pressRow(app, o, entries[at]);
  else if (k.key === 'ArrowDown') move(o, entries, 1, rows);
  else if (k.key === 'ArrowUp') move(o, entries, -1, rows);
  else if (k.key === 'PageDown') move(o, entries, rows, rows);
  else if (k.key === 'PageUp') move(o, entries, -rows, rows);
  else if (k.key === 'Home') move(o, entries, -entries.length, rows);
  else if (k.key === 'End') move(o, entries, entries.length, rows);
}

function stateText(app: App, entry: QueueEntry): { text: string; colour?: number; at: string } | null {
  const run = lastRun(app, entry);
  if (!run) return null;
  const at = clock(run.at);
  if (run.state === 'answered') return { text: 'answered', colour: 2, at };
  if (run.state === 'failed') return run.error === 'stopped' ? { text: 'stopped', at } : { text: 'failed', colour: 1, at };
  if (run.state === 'missed') return { text: 'missed', colour: 3, at };
  return { text: 'answering…', at: '' };
}

function hintOf(app: App, bot: Bot, entry: QueueEntry | undefined): string {
  const none = `each run is a new conversation of ${bot.name}, which answers in its chat`;
  const run = entry ? lastRun(app, entry) : undefined;
  if (!run) return none;
  if (run.state === 'missed') return `missed its run at ${clock(run.at)} · run now runs it once`;
  if (run.state === 'failed' && run.error !== 'stopped') return `last run failed: ${run.error ?? 'unknown'} · enter shows it in the chat`;
  return leftBlock(run) ? HINT_OPEN : none;
}

function whenText(entry: QueueEntry): string {
  if (entry.repeat) return entry.paused ? `↻ ${entry.repeat} · paused` : `↻ ${entry.repeat} · next ${shortClock(entry.at)}`;
  return entry.state === 'pending' ? `once · ${shortClock(entry.at)}` : 'once';
}

function drawRow(p: Painter, o: BotRoutinesDialog, entry: QueueEntry, r: Rect, selected: boolean, compact: boolean): void {
  const app = p.app;
  const bg: Style = selected ? { bg: p.surface } : p.hovered(r) ? { bg: p.hoverSurface } : {};
  p.g.fill(r, bg);
  const when = truncateRight(whenText(entry), Math.floor(r.w / 2));
  const name = truncateRight(entry.label, Math.max(0, r.w - LEAD - [...when].length - 1));
  const nameStyle: Style = entry.paused ? { ...bg, ...MUTED } : selected ? { ...bg, add: BOLD } : bg;
  p.span(r.x, r.y, selected ? '▌' : ' ', { ...bg, fg: 6 });
  p.span(r.x + 1, r.y, name, nameStyle, r.w - 1);
  p.span(right(r) - [...when].length - 1, r.y, when, { ...bg, ...MUTED });
  p.span(r.x, r.y + 1, selected ? '▌' : ' ', { ...bg, fg: 6 });
  const buttons = buttonsOf(entry);
  const shown = compact || selected || p.hovered(r);
  const taken = shown ? buttons.reduce((sum, b) => sum + buttonWidth(b), 0) + 1 : 0;
  const state = stateText(app, entry);
  const room = Math.max(0, r.w - LEAD - taken);
  if (!state) p.span(r.x + 3, r.y + 1, 'not run yet', { ...bg, ...MUTED }, room);
  else if (!state.at) p.span(r.x + 3, r.y + 1, state.text, { ...bg, ...MUTED }, room);
  else {
    let x = p.span(r.x + 3, r.y + 1, 'last run ', { ...bg, ...MUTED }, room);
    x = p.span(x, r.y + 1, state.text, { ...bg, fg: state.colour ?? 8 }, Math.max(0, r.x + 3 + room - x));
    p.span(x, r.y + 1, ` · ${truncateRight(state.at, Math.max(0, r.x + 3 + room - x - 3))}`, { ...bg, ...MUTED }, Math.max(0, r.x + 3 + room - x));
  }
  p.region({ r, click: () => pressRow(app, o, entry), cursor: 'pointer' });
  if (!shown) return;
  let x = right(r) - taken + 1;
  for (const b of buttons) {
    const rb = intersect(rect(x, r.y + 1, buttonWidth(b), 1), r);
    p.span(rb.x, rb.y, ` ${b} `, p.hovered(rb) ? { ...bg, fg: 6, add: BOLD } : { ...bg, ...MUTED });
    p.region({ r: rb, click: () => pressButton(app, o, entry, b), cursor: 'pointer' });
    x += buttonWidth(b);
  }
}

export function drawBotRoutines(p: Painter, o: BotRoutinesDialog): void {
  const app = p.app;
  const bot = app.bots.list.find((b) => b.id === o.bot);
  if (!bot) return;
  const area = botRoutinesArea(app.cols, app.rows);
  const compact = app.cols < COMPACT_WIDTH;
  const { list, note, buttons } = parts(area);
  p.g.clear(area);
  p.g.box(area, MUTED, `Routines › ${bot.name}`, { add: BOLD, fg: -1 });
  p.region({ r: area, click: () => {}, wheel: (dy) => scroll(app, o, dy) });
  const entries = routinesOf(app, o.bot);
  const selected = indexOf(o, entries);
  if (!entries.length) {
    const text = `${bot.name} has no routines yet. + routine schedules one, or ask ${bot.name} in its chat, such as "${EXAMPLES[3]}".`;
    wrapText(text, Math.max(1, list.w - 2)).slice(0, list.h).forEach((t, i) => p.span(list.x + 1, list.y + i, t, MUTED, list.w - 1));
  }
  const top = first(o, entries.length, list);
  entries.slice(top, top + roomOf(list)).forEach((entry, i) => {
    const r = rect(list.x, list.y + i * PITCH, list.w, PITCH);
    drawRow(p, o, entry, r, selected === top + i, compact);
  });
  p.span(note.x, note.y, truncateRight(hintOf(app, bot, selected === null ? undefined : entries[selected]), note.w), MUTED, note.w);
  p.dialogButtons(buttons, NEW, () => app.notify(FORM_LEFT_OUT), () => app.closeOverlay(), DONE);
}

function scroll(app: App, o: BotRoutinesDialog, dy: number): boolean {
  const { list } = parts(botRoutinesArea(app.cols, app.rows));
  const max = Math.max(0, routinesOf(app, o.bot).length - roomOf(list));
  o.scroll = Math.max(0, Math.min(max, first(o, max + roomOf(list), list) + dy));
  app.dirty();
  return true;
}

export const botRoutinesBounds = (cols: number, rows: number): Rect => botRoutinesArea(cols, rows);
