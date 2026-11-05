import { BOLD, type Rect, type Style, rect } from '../term/grid';
import { type Combo, COMBO_ROWS, combo, comboKey, comboList, comboRows, drawComboField, drawComboList, openCombo, scrollCombo } from './combo';
import { EMPTY, bottom, intersect, isEmpty, right } from './layout';
import type { Key } from './programs';
import { paintRows, seg, wrapWords } from './text';
import { type Editable, type FieldGrip, drawSelection, editKey, fieldLines, fieldPosition, placeIn } from './todo';
import type { Painter } from './ui';

export interface QueueEntry {
  id: number;
  at: number;
  prompt: string;
  label: string;
  project: number;
  folder: string;
  workspace: number | null;
  workspaceName?: string;
  branch: string;
  agent: string;
  mode: string;
  command: string;
  state: 'pending' | 'started' | 'failed' | 'missed' | 'removed' | 'kept';
  repeat?: string;
  routine?: number;
  paused?: boolean;
  cleanup?: boolean;
  pane?: number;
  error?: string;
}

export type QueueText = 'prompt' | 'clock' | 'date' | 'amount' | 'branch';
export type QueueList = 'when' | 'unit' | 'day' | 'place' | 'openIn' | 'agent' | 'mode';
export type QueueControl = QueueText | QueueList | 'cleanup';
export type When = 'at' | 'tomorrow' | 'on' | 'in' | 'every' | 'daily' | 'weekdays' | 'weekly' | 'claude' | 'codex';

export const WHEN: [When, string][] = [
  ['at', 'at'],
  ['tomorrow', 'tomorrow at'],
  ['on', 'on'],
  ['in', 'in'],
  ['every', 'every'],
  ['daily', 'every day at'],
  ['weekdays', 'every weekday at'],
  ['weekly', 'every week on'],
  ['claude', 'Claude reset'],
  ['codex', 'Codex reset'],
];
export const UNITS: [string, string][] = [
  ['minutes', 'm'],
  ['hours', 'h'],
  ['days', 'd'],
  ['weeks', 'w'],
];
export const DAYS = ['monday', 'tuesday', 'wednesday', 'thursday', 'friday', 'saturday', 'sunday'];
export const OPEN_IN = ['new tab', 'new worktree'];

const LISTS: QueueList[] = ['when', 'unit', 'day', 'place', 'openIn', 'agent', 'mode'];
const LABEL = 9;
const HEIGHT = 23;
const WHERE = 8;
const OPEN_IN_ROW = 10;
const AGENT = 12;
const MODE = 14;
const NOTICE = 16;
const CLEANUP = 16;
const ROUTINE_NOTICE = 18;
const DAY = 12;
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
  day: Combo;
  places: QueueChoice[];
  shown: number[];
  cleanup: boolean;
  cleanupOff?: string;
  place: Combo;
  openIn: Combo;
  agent: Combo;
  mode: Combo;
  focus: QueueControl;
  cursor: number;
  anchor?: number | null;
  zone: string;
  error?: string;
}

export type QueueStep = 'none' | 'submit' | 'cancel' | QueueList;

export const isList = (control: QueueControl): control is QueueList => (LISTS as string[]).includes(control);
export const isText = (control: QueueControl): control is QueueText => control !== 'cleanup' && !isList(control);
export const queueRepeats = (o: QueueForm): boolean => (['every', 'daily', 'weekdays', 'weekly'] as When[]).includes(queueKind(o));

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
    day: combo([...DAYS], 0),
    places,
    shown: places.map((_, i) => i),
    cleanup: false,
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
export const queueChoice = (o: QueueForm): QueueChoice | undefined => (o.place.chosen === null ? undefined : o.places[o.shown[o.place.chosen]]);

export function queueRefit(o: QueueForm): void {
  const routine = queueRepeats(o);
  const label = queueChoice(o)?.label;
  o.shown = o.places.flatMap((choice, i) => (!routine || (choice.worktree && choice.workspace === null) ? [i] : []));
  const labels = o.shown.map((i) => o.places[i].label);
  const at = label === undefined ? -1 : labels.indexOf(label);
  o.place = combo(labels, at >= 0 ? at : labels.length ? 0 : null);
}
export const queueWorktree = (o: QueueForm): boolean => !!queueChoice(o)?.worktree && o.openIn.chosen === 1 && !queueRepeats(o);

export function queueAt(o: QueueForm): string {
  const clock = o.clock.trim();
  const kind = queueKind(o);
  if (kind === 'at') return clock;
  if (kind === 'tomorrow') return `tomorrow ${clock}`;
  if (kind === 'on') return `${o.date.trim()} ${clock}`;
  if (kind === 'in') return `in ${o.amount.trim()}${UNITS[o.unit.chosen ?? -1]?.[1] ?? 'h'}`;
  if (queueRepeats(o)) return '';
  return `${kind} reset`;
}

export function queueRepeat(o: QueueForm, now: number): string {
  const clock = o.clock.trim();
  const kind = queueKind(o);
  const unit = UNITS[o.unit.chosen ?? -1]?.[1] ?? 'h';
  if (kind === 'daily') return `day ${clock}`;
  if (kind === 'weekdays') return `weekday ${clock}`;
  if (kind === 'weekly') return `${DAYS[o.day.chosen ?? 0]} ${clock}`;
  const amount = `${o.amount.trim()}${unit}`;
  return unit === 'd' || unit === 'w' ? `${amount} ${queueWhen(now).split(' ')[1]}` : amount;
}

export function queueControls(o: QueueForm): QueueControl[] {
  const values: Record<When, QueueControl[]> = { at: ['clock'], tomorrow: ['clock'], on: ['date', 'clock'], in: ['amount', 'unit'], every: ['amount', 'unit'], daily: ['clock'], weekdays: ['clock'], weekly: ['day', 'clock'], claude: [], codex: [] };
  const routine = queueRepeats(o);
  const controls: QueueControl[] = ['prompt', 'when', ...values[queueKind(o)], 'place'];
  if (queueChoice(o)?.worktree && !routine) controls.push('openIn');
  if (queueWorktree(o)) controls.push('branch');
  controls.push('agent', 'mode');
  if (routine && !o.cleanupOff) controls.push('cleanup');
  return controls;
}

export const queueOpened = (o: QueueForm): QueueList | null => (queueControls(o).find((c) => isList(c) && !!o[c].open) as QueueList | undefined) ?? null;

export function closeQueueLists(o: QueueForm): void {
  for (const list of LISTS) o[list].open = null;
}

export function focusQueue(o: QueueForm, control: QueueControl): void {
  closeQueueLists(o);
  o.focus = control;
  o.anchor = null;
  if (isText(control)) o.cursor = [...o[control]].length;
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
  if (k.key === ' ' && o.focus === 'cleanup') o.cleanup = !o.cleanup;
  else if (k.key === 'Tab') {
    const controls = queueControls(o);
    const at = Math.max(0, controls.indexOf(o.focus));
    focusQueue(o, controls[(at + (k.shift ? controls.length - 1 : 1)) % controls.length]);
  } else if (isList(o.focus)) comboKey(o[o.focus], k, rows);
  else if (isText(o.focus)) {
    queueEdit(o, o.focus, (field) => editKey(field, k, width));
  }
  return 'none';
}

function queueEdit(o: QueueForm, control: QueueText, change: (f: Editable, width: number) => void, width = 1): void {
  const field: Editable = { chars: [...o[control]], cursor: o.cursor, anchor: o.anchor };
  change(field, width);
  o[control] = field.chars.join('');
  o.cursor = field.cursor;
  o.anchor = field.anchor ?? null;
}

function queueGrip(screen: Rect, o: QueueForm, control: QueueText): FieldGrip {
  const t = queueTextRect(screen, o, control);
  return {
    edit: (change) => queueEdit(o, control, change, Math.max(1, t.w)),
    focused: () => o.focus === control,
    spot: (x, y) => [firstLine([...o[control]], o.cursor, t) + (y - t.y), Math.max(0, x - t.x)],
  };
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
    case 'day': return cell(screen, 6, LABEL, DAY);
    case 'clock': return cell(screen, 6, queueKind(o) === 'weekly' ? LABEL + DAY + 1 : on ? LABEL + DATE + 1 : LABEL, CLOCK);
    case 'cleanup': return cell(screen, CLEANUP, LABEL, rest(screen, LABEL));
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
  return control === 'prompt' || control === 'cleanup' || isEmpty(r) ? r : rect(r.x + 1, r.y, r.w - 1, r.h);
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
  if (control === 'cleanup') {
    focusQueue(o, control);
    o.cleanup = !o.cleanup;
    return;
  }
  const opening = isList(control) && !o[control].open;
  const t = queueTextRect(screen, o, control);
  const top = isText(control) ? firstLine([...o[control]], o.cursor, t) : 0;
  focusQueue(o, control);
  if (!isText(control)) {
    if (opening && isList(control)) openCombo(o[control]);
    return;
  }
  queueEdit(o, control, (field, width) => placeIn(field, width, top + Math.max(0, y - t.y), Math.max(0, x - t.x)), Math.max(1, t.w));
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
  if (kind === 'every') return 'at least 30 minutes apart';
  return kind === 'claude' || kind === 'codex' ? 'read from the plan when you save' : '';
}

function paragraph(p: Painter, r: Rect, text: string, style: Style): void {
  if (!isEmpty(r)) paintRows(p.g, r, wrapWords([seg(text, style)], r.w));
}

function drawCleanup(p: Painter, o: QueueForm, screen: Rect): void {
  const r = queueRect(screen, o, 'cleanup');
  const text = `${o.cleanup && !o.cleanupOff ? '[x]' : '[ ]'} remove the workspace when it finishes`;
  const style: Style = o.cleanupOff ? MUTED : o.focus === 'cleanup' ? { add: BOLD } : {};
  p.span(r.x, r.y, text, style, r.w);
  if (!o.cleanupOff) return;
  const why = cell(screen, CLEANUP + 1, LABEL + 4, rest(screen, LABEL + 4));
  p.span(why.x, why.y, o.cleanupOff, MUTED, why.w);
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
  const routine = queueRepeats(o);
  if (!controls.includes('openIn')) {
    const only = cell(screen, OPEN_IN_ROW, LABEL + 1, rest(screen, LABEL + 1));
    p.span(only.x, only.y, OPEN_IN[routine ? 1 : 0], MUTED, only.w);
  }
  if (routine) drawCleanup(p, o, screen);
  const placeholder = o.places.length ? 'choose an open place…' : 'no open project';
  for (const control of controls) {
    const r = queueRect(screen, o, control);
    if (control === 'cleanup') {
      p.region({ r, click: (x, y) => { clickQueue(screen, o, control, x, y); app.dirty(); }, cursor: 'pointer' });
      continue;
    }
    if (isList(control)) drawComboField(p, r, o[control], control === 'place' ? placeholder : '', o.focus === control);
    else {
      if (!isText(control)) continue;
      const focused = !open && o.focus === control;
      const cursor = focused ? o.cursor : [...o[control]].length;
      const t = queueTextRect(screen, o, control);
      drawText(p, r, t, o[control], cursor, focused, control === 'branch' ? 'branch name' : '');
      if (focused) drawSelection(p, { chars: [...o[control]], cursor: o.cursor, anchor: o.anchor }, t, firstLine([...o[control]], o.cursor, t), { bg: p.surface });
    }
    const field = isText(control) ? queueGrip(screen, o, control) : undefined;
    p.region({ r, click: (x, y) => { clickQueue(screen, o, control, x, y); app.dirty(); }, cursor: isList(control) ? 'pointer' : 'text', field });
  }
  const values = controls.filter((c) => c === 'clock' || c === 'date' || c === 'amount' || c === 'unit' || c === 'day');
  const end = values.length ? Math.max(...values.map((c) => right(queueRect(screen, o, c)))) : inner(screen).x + LABEL;
  const tipRow = row(screen, 6, 1);
  if (!isEmpty(tipRow)) p.span(end + 1, tipRow.y, hint(o), MUTED, Math.max(0, right(tipRow) - end - 1));
  const notice = routine ? `Times use ${o.zone}. Each run gets a new worktree from the up-to-date default branch and accepts its folder trust question.` : `Times use ${o.zone}. Queued agents always accept their folder trust question.`;
  paragraph(p, row(screen, routine ? ROUTINE_NOTICE : NOTICE, 2), notice, MUTED);
  const r = inner(screen);
  const buttons = bottom(r) - 1;
  if (o.error) {
    const y = Math.min(Math.max(buttons - 2, r.y + NOTICE), buttons);
    const note = rect(r.x, y, r.w, buttons - y);
    p.g.clear(note);
    paragraph(p, note, o.error, { fg: 1 });
  }
  p.dialogButtons(rect(r.x, buttons, r.w, isEmpty(r) ? 0 : 1), 'schedule', () => app.submitQueue(), () => app.closeQueueForm(false));
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

export const ROUTINE_GRACE = 600000;
export const ROUTINE_RUNS = 10;
export const ROUTINE_USE = 'use every 2h, every 3d, every day 09:00, every weekday 09:00 or every monday 09:00';
const TOO_OFTEN = 'a routine repeats at most every 30 minutes, since each run makes a worktree';
const TOO_RARE = 'a routine repeats at least once a year';
const STEP: Record<string, number> = { m: 60000, h: 3600000, d: 86400000, w: 604800000 };

interface Repeat {
  amount: number;
  unit: string;
  weekdays: boolean;
  day: number | null;
  clock: [number, number] | null;
}

function parseClock(text: string | undefined): [number, number] | null {
  const m = /^(\d{1,2}):(\d{2})$/.exec(text ?? '');
  return m && Number(m[1]) < 24 && Number(m[2]) < 60 ? [Number(m[1]), Number(m[2])] : null;
}

export function parseRepeat(text: string): Repeat | string {
  const words = text.trim().toLowerCase().split(/\s+/);
  if (words.length > 2) return ROUTINE_USE;
  const [head, at] = words;
  const clock = parseClock(at);
  if (at !== undefined && !clock) return ROUTINE_USE;
  const day = DAYS.indexOf(head);
  if (head === 'day' || head === 'weekday' || day >= 0) {
    return clock ? { amount: 0, unit: '', weekdays: head === 'weekday', day: day >= 0 ? day : null, clock } : ROUTINE_USE;
  }
  const m = /^(\d+)([mhdw])$/.exec(head);
  if (!m || (clock && 'mh'.includes(m[2]))) return ROUTINE_USE;
  const gap = Number(m[1]) * STEP[m[2]];
  if (gap < 1800000) return TOO_OFTEN;
  return gap > 366 * 86400000 ? TOO_RARE : { amount: Number(m[1]), unit: m[2], weekdays: false, day: null, clock };
}

function onDate(r: Repeat, date: Date): number {
  const d = new Date(date);
  d.setHours(r.clock?.[0] ?? 0, r.clock?.[1] ?? 0, 0, 0);
  return d.getTime();
}

function matches(r: Repeat, date: Date): boolean {
  if (r.unit) return true;
  const weekday = (date.getDay() + 6) % 7;
  return r.day !== null ? weekday === r.day : !r.weekdays || weekday < 5;
}

function nextDate(r: Repeat, date: Date): Date {
  const next = new Date(date);
  if (r.unit) {
    next.setDate(next.getDate() + (r.amount * STEP[r.unit]) / STEP.d);
    return next;
  }
  do next.setDate(next.getDate() + 1);
  while (!matches(r, next));
  return next;
}

const absolute = (r: Repeat): number => (r.unit === 'm' || r.unit === 'h' ? r.amount * STEP[r.unit] : 0);

export function repeatFirst(r: Repeat, now: number): number {
  if (absolute(r)) return now + absolute(r);
  if (r.unit) return onDate(r, nextDate(r, new Date(now)));
  const date = new Date(now);
  while (!matches(r, date) || onDate(r, date) <= now) date.setDate(date.getDate() + 1);
  return onDate(r, date);
}

export const repeatAfter = (r: Repeat, at: number): number => (absolute(r) ? at + absolute(r) : onDate(r, nextDate(r, new Date(at))));

export function repeatWalk(r: Repeat, at: number, now: number): { last: number; skipped: boolean; next: number } {
  let last = at;
  let next = repeatAfter(r, last);
  while (next <= now) {
    last = next;
    next = repeatAfter(r, last);
  }
  return { last, skipped: last > at, next };
}

export const routineSlug = (label: string): string => label.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '').slice(0, 30).replace(/-+$/, '') || 'routine';
