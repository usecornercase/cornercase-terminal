import { BOLD, type Rect, type Style, rect } from '../term/grid';
import { type Combo, COMBO_ROWS, combo, comboKey, comboList, comboRows, drawComboField, drawComboList, openCombo, scrollCombo } from './combo';
import { type Areas, EMPTY, Rows, bottom, intersect, isEmpty, right } from './layout';
import type { Key } from './programs';
import { paintRows, seg, wrapWords } from './text';
import { editKey, fieldLines, fieldPosition, placeIn } from './todo';
import type { Painter } from './ui';

export interface QueueEntry {
  id: number;
  at: number;
  prompt: string;
  label: string;
  project: number;
  workspace: number | null;
  branch: string;
  agent: string;
  mode: string;
  command: string;
  state: 'pending' | 'started' | 'failed' | 'missed';
  pane?: number;
  error?: string;
}

export type QueueText = 'prompt' | 'clock' | 'date' | 'amount' | 'branch';
export type QueueList = 'when' | 'unit' | 'place' | 'openIn' | 'agent' | 'mode';
export type QueueControl = QueueText | QueueList;
export type When = 'at' | 'tomorrow' | 'on' | 'in' | 'claude' | 'codex';

export const WHEN: [When, string][] = [
  ['at', 'at'],
  ['tomorrow', 'tomorrow at'],
  ['on', 'on'],
  ['in', 'in'],
  ['claude', 'Claude reset'],
  ['codex', 'Codex reset'],
];
export const UNITS: [string, string][] = [
  ['minutes', 'm'],
  ['hours', 'h'],
  ['days', 'd'],
];
export const OPEN_IN = ['new tab', 'new worktree'];

const LISTS: QueueList[] = ['when', 'unit', 'place', 'openIn', 'agent', 'mode'];
const LABEL = 9;
const HEIGHT = 23;
const WHERE = 8;
const OPEN_IN_ROW = 10;
const AGENT = 12;
const MODE = 14;
const NOTICE = 16;
const COMBO = 18;
const WIDE = 44;
const CLOCK = 7;
const DATE = 12;
const AMOUNT = 6;
const UNIT = 12;
const MUTED: Style = { fg: 8 };

export interface QueueChoice {
  label: string;
  project: number;
  workspace: number | null;
  worktree: boolean;
}

export interface QueueForm {
  kind: 'queue';
  id?: number;
  label?: string;
  prompt: string;
  clock: string;
  date: string;
  amount: string;
  branch: string;
  when: Combo;
  unit: Combo;
  places: QueueChoice[];
  place: Combo;
  openIn: Combo;
  agent: Combo;
  mode: Combo;
  focus: QueueControl;
  cursor: number;
  zone: string;
  error?: string;
}

export type QueueStep = 'none' | 'submit' | 'cancel' | QueueList;

export const isList = (control: QueueControl): control is QueueList => (LISTS as string[]).includes(control);

export const whenCombo = (kind: When): Combo => combo(WHEN.map(([, label]) => label), WHEN.findIndex(([k]) => k === kind));

export function queueForm(places: QueueChoice[], place: number | null, kinds: string[], agent: number, zone: string, today: string): QueueForm {
  return {
    kind: 'queue',
    prompt: '',
    clock: '09:00',
    date: today,
    amount: '2',
    branch: '',
    when: whenCombo('in'),
    unit: combo(UNITS.map(([name]) => name), 1),
    places,
    place: combo(places.map((choice) => choice.label), place),
    openIn: combo([...OPEN_IN], 0),
    agent: combo(kinds, agent),
    mode: combo(['default'], 0),
    focus: 'prompt',
    cursor: 0,
    zone,
  };
}

export const queueKind = (o: QueueForm): When => (o.when.chosen === null ? undefined : WHEN[o.when.chosen]?.[0]) ?? 'in';
export const queueChoice = (o: QueueForm): QueueChoice | undefined => (o.place.chosen === null ? undefined : o.places[o.place.chosen]);
export const queueWorktree = (o: QueueForm): boolean => !!queueChoice(o)?.worktree && o.openIn.chosen === 1;

export function queueAt(o: QueueForm): string {
  const clock = o.clock.trim();
  const kind = queueKind(o);
  if (kind === 'at') return clock;
  if (kind === 'tomorrow') return `tomorrow ${clock}`;
  if (kind === 'on') return `${o.date.trim()} ${clock}`;
  if (kind === 'in') return `in ${o.amount.trim()}${UNITS[o.unit.chosen ?? -1]?.[1] ?? 'h'}`;
  return `${kind} reset`;
}

export function queueControls(o: QueueForm): QueueControl[] {
  const values: Record<When, QueueControl[]> = { at: ['clock'], tomorrow: ['clock'], on: ['date', 'clock'], in: ['amount', 'unit'], claude: [], codex: [] };
  const controls: QueueControl[] = ['prompt', 'when', ...values[queueKind(o)], 'place'];
  if (queueChoice(o)?.worktree) controls.push('openIn');
  if (queueWorktree(o)) controls.push('branch');
  return [...controls, 'agent', 'mode'];
}

export const queueOpened = (o: QueueForm): QueueList | null => (queueControls(o).find((c) => isList(c) && !!o[c].open) as QueueList | undefined) ?? null;

export function closeQueueLists(o: QueueForm): void {
  for (const list of LISTS) o[list].open = null;
}

export function focusQueue(o: QueueForm, control: QueueControl): void {
  closeQueueLists(o);
  o.focus = control;
  if (!isList(control)) o.cursor = [...o[control]].length;
}

export function queueChose(o: QueueForm, control: QueueList): QueueList {
  if (!queueControls(o).includes(o.focus)) o.focus = control;
  return control;
}

export function queueKey(o: QueueForm, k: Key, width: number, rows: number): QueueStep {
  o.error = undefined;
  const open = queueOpened(o);
  if (open && k.key !== 'Tab') return comboKey(o[open], k, rows) ? queueChose(o, open) : 'none';
  if (k.key === 'Escape') return 'cancel';
  if (k.key === 'Enter') return 'submit';
  if (k.key === 'Tab') {
    const controls = queueControls(o);
    const at = Math.max(0, controls.indexOf(o.focus));
    focusQueue(o, controls[(at + (k.shift ? controls.length - 1 : 1)) % controls.length]);
  } else if (isList(o.focus)) comboKey(o[o.focus], k, rows);
  else {
    const field = { chars: [...o[o.focus]], cursor: o.cursor };
    editKey(field, k, width);
    o[o.focus] = field.chars.join('');
    o.cursor = field.cursor;
  }
  return 'none';
}

export function queueFormArea(cols: number, rows: number): Rect {
  const w = Math.min(72, Math.max(0, cols - 2));
  const h = Math.min(HEIGHT, rows);
  return rect(Math.floor((cols - w) / 2), Math.floor((rows - h) / 2), w, h);
}

function inner(screen: Rect): Rect {
  const a = queueFormArea(screen.w, screen.h);
  return rect(a.x + 2, a.y + 1, a.w - 4, a.h - 2);
}

function row(screen: Rect, at: number, h: number): Rect {
  const r = inner(screen);
  return intersect(rect(r.x, r.y + at, r.w, h), r);
}

function cell(screen: Rect, at: number, x: number, w: number): Rect {
  const r = row(screen, at, 1);
  return isEmpty(r) ? EMPTY : intersect(rect(r.x + x, r.y, w, r.h), r);
}

const rest = (screen: Rect, x: number): number => Math.max(0, inner(screen).w - x);

export function queueRect(screen: Rect, o: QueueForm, control: QueueControl): Rect {
  const on = queueKind(o) === 'on';
  switch (control) {
    case 'prompt': return row(screen, 1, 3);
    case 'when': return cell(screen, 5, LABEL, COMBO);
    case 'date': return cell(screen, 6, LABEL, DATE);
    case 'clock': return cell(screen, 6, on ? LABEL + DATE + 1 : LABEL, CLOCK);
    case 'amount': return cell(screen, 6, LABEL, AMOUNT);
    case 'unit': return cell(screen, 6, LABEL + AMOUNT + 1, UNIT);
    case 'place': return cell(screen, WHERE, LABEL, Math.min(rest(screen, LABEL), WIDE));
    case 'openIn': return cell(screen, OPEN_IN_ROW, LABEL, COMBO);
    case 'branch': return cell(screen, OPEN_IN_ROW, LABEL + COMBO + 1, Math.min(rest(screen, LABEL + COMBO + 1), WIDE - COMBO - 1));
    case 'agent': return cell(screen, AGENT, LABEL, COMBO);
    case 'mode': return cell(screen, MODE, LABEL, COMBO);
  }
}

export function queueTextRect(screen: Rect, o: QueueForm, control: QueueControl): Rect {
  const r = queueRect(screen, o, control);
  return control === 'prompt' || isEmpty(r) ? r : rect(r.x + 1, r.y, r.w - 1, r.h);
}

export function queueList(screen: Rect, o: QueueForm): Rect | null {
  const open = queueOpened(o);
  return open ? comboList(queueRect(screen, o, open), o[open], screen) : null;
}

export const queueListRows = (screen: Rect, o: QueueForm): number => {
  const list = queueList(screen, o);
  return list ? comboRows(list) : COMBO_ROWS;
};

function union(a: Rect, b: Rect): Rect {
  if (isEmpty(b)) return a;
  const x = Math.min(a.x, b.x);
  const y = Math.min(a.y, b.y);
  return rect(x, y, Math.max(right(a), right(b)) - x, Math.max(bottom(a), bottom(b)) - y);
}

export function queueBounds(cols: number, rows: number, o: QueueForm): Rect {
  const area = queueFormArea(cols, rows);
  const list = queueList(rect(0, 0, cols, rows), o);
  return list ? union(area, list) : area;
}

function firstLine(chars: string[], cursor: number, field: Rect): number {
  const [line] = fieldPosition({ chars, cursor }, Math.max(1, field.w));
  return Math.max(0, line - Math.max(0, field.h - 1));
}

export function clickQueue(screen: Rect, o: QueueForm, control: QueueControl, x: number, y: number): void {
  const opening = isList(control) && !o[control].open;
  const t = queueTextRect(screen, o, control);
  const top = isList(control) ? 0 : firstLine([...o[control]], o.cursor, t);
  focusQueue(o, control);
  if (isList(control)) {
    if (opening) openCombo(o[control]);
    return;
  }
  const field = { chars: [...o[control]], cursor: o.cursor };
  placeIn(field, Math.max(1, t.w), top + Math.max(0, y - t.y), Math.max(0, x - t.x));
  o.cursor = field.cursor;
}

function drawText(p: Painter, field: Rect, t: Rect, value: string, cursor: number, focused: boolean, placeholder: string): void {
  const bg: Style = { bg: p.surface };
  p.g.fill(field, bg);
  const chars = [...value];
  const width = Math.max(1, t.w);
  const [line, col] = fieldPosition({ chars, cursor }, width);
  const start = firstLine(chars, cursor, t);
  if (!chars.length && placeholder) p.span(t.x, t.y, placeholder, { ...bg, ...MUTED }, t.w);
  else {
    const text: Style = { ...bg, fg: p.app.light ? 0 : 15, add: BOLD };
    const shown = chars.map((c) => (c === '\n' ? '↵' : /\p{Cc}/u.test(c) ? ' ' : c));
    fieldLines({ chars }, width)
      .slice(start, start + t.h)
      .forEach(([a, b], i) => p.span(t.x, t.y + i, shown.slice(a, b).join(''), text, t.w));
  }
  if (focused && !isEmpty(t)) p.cursor = { shape: 'block', x: t.x + Math.min(col, t.w - 1), y: t.y + Math.min(line - start, t.h - 1) };
}

function hint(o: QueueForm): string {
  const kind = queueKind(o);
  if (kind === 'at') return 'today or tomorrow';
  return kind === 'claude' || kind === 'codex' ? 'read from the plan when you save' : '';
}

function paragraph(p: Painter, r: Rect, text: string, style: Style): void {
  if (!isEmpty(r)) paintRows(p.g, r, wrapWords([seg(text, style)], r.w));
}

export function drawQueueForm(p: Painter, o: QueueForm): void {
  const app = p.app;
  const screen = rect(0, 0, app.cols, app.rows);
  const area = queueFormArea(app.cols, app.rows);
  const open = queueOpened(o);
  const scroll = (dy: number) => {
    const list = queueList(screen, o);
    if (open && list) scrollCombo(o[open], dy, comboRows(list));
    app.dirty();
    return true;
  };
  p.g.clear(area);
  p.g.box(area, MUTED, 'Schedule an agent', { add: BOLD, fg: -1 });
  p.region({ r: area, click: () => { closeQueueLists(o); app.dirty(); }, wheel: scroll });
  const controls = queueControls(o);
  for (const [at, label] of [[0, 'Prompt'], [5, 'When'], [WHERE, 'Where'], [OPEN_IN_ROW, 'Open in'], [AGENT, 'Agent'], [MODE, 'Mode']] as const) {
    const r = cell(screen, at, 0, LABEL);
    p.span(r.x, r.y, label, MUTED, r.w);
  }
  if (!controls.includes('openIn')) {
    const only = cell(screen, OPEN_IN_ROW, LABEL + 1, rest(screen, LABEL + 1));
    p.span(only.x, only.y, 'new tab', MUTED, only.w);
  }
  const placeholder = o.places.length ? 'choose an open place…' : 'no open project';
  for (const control of controls) {
    const r = queueRect(screen, o, control);
    if (isList(control)) drawComboField(p, r, o[control], control === 'place' ? placeholder : '', o.focus === control);
    else {
      const focused = !open && o.focus === control;
      const cursor = focused ? o.cursor : [...o[control]].length;
      drawText(p, r, queueTextRect(screen, o, control), o[control], cursor, focused, control === 'branch' ? 'branch name' : '');
    }
    p.region({ r, click: (x, y) => { clickQueue(screen, o, control, x, y); app.dirty(); }, cursor: isList(control) ? 'pointer' : 'text' });
  }
  const values = controls.filter((c) => c === 'clock' || c === 'date' || c === 'amount' || c === 'unit');
  const end = values.length ? Math.max(...values.map((c) => right(queueRect(screen, o, c)))) : inner(screen).x + LABEL;
  const tipRow = row(screen, 6, 1);
  if (!isEmpty(tipRow)) p.span(end + 1, tipRow.y, hint(o), MUTED, Math.max(0, right(tipRow) - end - 1));
  paragraph(p, row(screen, NOTICE, 2), `Times use ${o.zone}. Queued agents always accept their folder trust question.`, MUTED);
  const r = inner(screen);
  const buttons = bottom(r) - 1;
  if (o.error) {
    const y = Math.min(Math.max(buttons - 2, r.y + NOTICE), buttons);
    const note = rect(r.x, y, r.w, buttons - y);
    p.g.clear(note);
    paragraph(p, note, o.error, { fg: 1 });
  }
  p.dialogButtons(rect(r.x, buttons, r.w, isEmpty(r) ? 0 : 1), 'schedule', () => app.submitQueue(), () => app.closeOverlay());
  const list = queueList(screen, o);
  if (open && list) {
    p.region({ r: union(area, list), click: () => { closeQueueLists(o); app.dirty(); }, wheel: scroll });
    drawComboList(p, list, o[open], (i) => app.queueChoose(o, open, i));
  }
}

export function queueTime(text: string, now: number): number | null {
  const value = text.trim();
  if (value === 'claude reset' || value === 'codex reset') return now + 2 * 3600000;
  const delay = /^in ([1-9][0-9]*)(m|h|d)$/.exec(value);
  if (delay) return now + Number(delay[1]) * { m: 60000, h: 3600000, d: 86400000 }[delay[2] as 'm' | 'h' | 'd'];
  const base = new Date(now);
  const clock = /^(?:tomorrow )?(\d{1,2}):(\d{2})$/.exec(value);
  if (clock && Number(clock[1]) < 24 && Number(clock[2]) < 60) {
    base.setHours(Number(clock[1]), Number(clock[2]), 0, 0);
    if (value.startsWith('tomorrow ') || base.getTime() <= now) base.setDate(base.getDate() + 1);
    return base.getTime();
  }
  if (!/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}$/.test(value)) return null;
  const date = new Date(value.replace(' ', 'T'));
  const at = date.getTime();
  return Number.isFinite(at) && queueWhen(at) === value && at > now ? at : null;
}

export function queueWhen(at: number): string {
  const date = new Date(at);
  const pad = (number: number) => String(number).padStart(2, '0');
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

export function drawQueue(p: Painter, areas: Areas): void {
  const app = p.app;
  const area = areas.changes;
  p.g.clear(area);
  p.span(area.x + 1, area.y, `Queue ${app.queued.filter((e) => e.state === 'pending').length}`, { ...MUTED, add: BOLD });
  const close = rect(right(area) - 3, area.y, 3, 1);
  p.span(close.x + 1, close.y, '×', p.hovered(close) ? { fg: 1, add: BOLD } : MUTED);
  p.region({ r: close, click: () => app.toggleQueue(), cursor: 'pointer' });
  if (app.queued.some((entry) => entry.state === 'started')) {
    const clear = intersect(rect(close.x - 16, area.y + (area.w < 28 ? 1 : 0), 15, 1), area);
    p.button(clear, '', 'clear started', p.hovered(clear) ? { fg: 0, bg: 6 } : MUTED);
    p.region({ r: clear, click: () => app.removeQueued((entry) => entry.state === 'started', 'cleared {n} started'), cursor: 'pointer' });
  }
  const list = rect(area.x, area.y + 2, area.w, Math.max(0, area.h - 2));
  const rows = new Rows(list, app.queued.map(() => 5), 1, app.queueScroll);
  app.queued.forEach((entry, i) => {
    const r = rows.item(i);
    if (!r.h) return;
    const lit = p.hovered(r);
    const bg: Style = lit ? { bg: p.hoverSurface } : {};
    if (lit) p.g.fill(r, bg);
    const when = queueWhen(entry.at);
    p.span(r.x + 1, r.y, when, MUTED, r.w - 2);
    p.span(r.x + 1, r.y + 1, entry.label, {}, r.w - 5);
    p.span(r.x + 1, r.y + 2, `${entry.agent} · ${entry.branch ? 'worktree' : 'tab'}`, MUTED, r.w - 2);
    p.span(r.x + 1, r.y + 3, entry.error ? `failed · ${entry.error}` : entry.state, { fg: entry.state === 'failed' ? 1 : entry.state === 'started' ? 2 : 8 }, r.w - 14);
    p.region({ r, click: () => app.openQueueForm(entry), cursor: 'pointer' });
    const remove = rect(right(r) - 3, r.y + 1, 3, 1);
    if (lit || areas.compact) p.button(remove, '', '×', p.hovered(remove) ? { ...bg, fg: 1, add: BOLD } : { ...bg, ...MUTED });
    p.region({ r: remove, click: () => app.removeQueued((item) => item.id === entry.id, `removed queue entry ${entry.id}`), cursor: 'pointer' });
    if (entry.state !== 'started') {
      const run = rect(right(r) - 12, r.y + 3, 11, 1);
      p.button(run, '', 'start now', p.hovered(run) ? { fg: 0, bg: 6, add: BOLD } : { ...bg, fg: 6 });
      p.region({ r: run, click: () => app.runQueue(entry), cursor: 'pointer' });
    }
  });
  const add = rows.buttonRect();
  p.button(add, '', '+ New', p.hovered(add) ? { fg: 0, bg: 6, add: BOLD } : { fg: 6 });
  p.region({ r: add, click: () => app.openQueueForm(), cursor: 'pointer' });
  p.region({ r: list, wheel: (dy) => { app.queueScroll = rows.scrolled(Math.sign(dy) * 3); app.dirty(); return true; } });
}
