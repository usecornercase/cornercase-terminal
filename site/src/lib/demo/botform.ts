import { BOLD, type Rect, type Style, rect } from '../term/grid';
import type { App } from './app';
import { BOT_AGENTS, MAX_BOTS, NO_ACCESS_WHY, type Access, draftOf, pasted } from './bots';
import { wrapText } from './botchat';
import { type Combo, COMBO_ROWS, chooseCombo, combo, comboKey, comboList, comboRows, comboShown, drawComboField, drawComboList, openCombo, scrollCombo } from './combo';
import { bottom, intersect, isEmpty, right } from './layout';
import type { Key } from './programs';
import { truncateRight } from './text';
import { type Editable, drawSelection, editKey, fieldLines, fieldPosition, placeIn } from './todo';
import type { Painter } from './ui';

const MUTED: Style = { fg: 8 };
const LABEL = 9;
const HEIGHT = 19;
const WIDTH = 76;
const COMBO = 44;
const ABOUT_ROWS = 4;
const CONTROLS = ['name', 'agent', 'model', 'access', 'about'] as const;
const LISTS = ['agent', 'model', 'access'] as const;
const ROWS: Record<Control, number> = { name: 0, agent: 1, model: 2, access: 3, about: 6 };
const ACCESS_LABELS: [Access, string][] = [['cornercase', 'through cornercase'], ['full', 'full access']];
const NOTES = ['through cornercase: reads anything, writes only in its folder', 'full access: can change any file and run anything, unattended'];

export type Control = (typeof CONTROLS)[number];
type List = (typeof LISTS)[number];

export interface BotForm {
  kind: 'bot';
  name: Editable;
  about: Editable;
  agent: Combo;
  model: Combo;
  access: Combo;
  focus: Control;
  error?: string;
}

const isList = (c: Control): c is List => (LISTS as readonly string[]).includes(c);

function agentCombo(): Combo {
  const c = combo(BOT_AGENTS.map((a) => (a.why ? `${a.title} · ${a.why}` : a.title)), 0);
  c.off = BOT_AGENTS.flatMap((a, i) => (a.why ? [i] : []));
  return c;
}

function refit(o: BotForm): void {
  const agent = BOT_AGENTS[o.agent.chosen ?? 0];
  o.model = combo(['default', ...agent.models], 0);
  const options = ACCESS_LABELS.map(([kind, label]) => (agent.accesses.includes(kind) ? label : `${label} · ${NO_ACCESS_WHY}`));
  const access = combo(options, null);
  access.off = ACCESS_LABELS.flatMap(([kind], i) => (agent.accesses.includes(kind) ? [] : [i]));
  if (agent.accesses.includes('cornercase')) chooseCombo(access, 0);
  o.access = access;
}

export function newBotForm(): BotForm {
  const o: BotForm = { kind: 'bot', name: draftOf(), about: draftOf(), agent: agentCombo(), model: combo([], 0), access: combo([], null), focus: 'name' };
  refit(o);
  return o;
}

export function botFormArea(cols: number, rows: number): Rect {
  const w = Math.min(WIDTH, Math.max(0, cols - 2));
  const h = Math.min(HEIGHT, rows);
  return rect(Math.floor((cols - w) / 2), Math.floor((rows - h) / 2), w, h);
}

const inner = (cols: number, rows: number): Rect => {
  const a = botFormArea(cols, rows);
  return rect(a.x + 2, a.y + 1, Math.max(0, a.w - 4), Math.max(0, a.h - 2));
};

function controlRect(screen: Rect, c: Control): Rect {
  const r = inner(screen.w, screen.h);
  const w = Math.max(0, r.w - LABEL);
  const h = c === 'about' ? ABOUT_ROWS : 1;
  return intersect(rect(r.x + LABEL, r.y + ROWS[c], isList(c) ? Math.min(w, COMBO) : w, h), r);
}

const textRect = (screen: Rect, c: Control): Rect => {
  const r = controlRect(screen, c);
  return rect(r.x + 1, r.y, Math.max(1, r.w - 2), r.h);
};

const fieldOf = (o: BotForm, c: Control): Editable | null => (c === 'name' ? o.name : c === 'about' ? o.about : null);

function opened(o: BotForm): List | null {
  return LISTS.find((l) => !!o[l].open) ?? null;
}

function closeLists(o: BotForm): void {
  for (const l of LISTS) o[l].open = null;
}

function focus(o: BotForm, c: Control): void {
  closeLists(o);
  o.focus = c;
}

function chose(o: BotForm, list: List, i: number): void {
  chooseCombo(o[list], i);
  if (list === 'agent') refit(o);
}

function listArea(screen: Rect, o: BotForm): Rect | null {
  const open = opened(o);
  return open ? comboList(controlRect(screen, open), o[open], screen) : null;
}

export function botFormBounds(cols: number, rows: number, o: BotForm): Rect {
  const area = botFormArea(cols, rows);
  const list = listArea(rect(0, 0, cols, rows), o);
  if (!list) return area;
  const x = Math.min(area.x, list.x);
  const y = Math.min(area.y, list.y);
  return rect(x, y, Math.max(right(area), right(list)) - x, Math.max(bottom(area), bottom(list)) - y);
}

export function submitBotForm(app: App, o: BotForm): void {
  const name = o.name.chars.join('').trim();
  const kind = BOT_AGENTS[o.agent.chosen ?? 0].kind;
  const fail = (text: string): void => {
    o.error = text;
    app.dirty();
  };
  if (!name) return fail('name the bot');
  if ([...name].length > 40) return fail('a name takes 1 to 40 characters');
  if (app.bots.named(name)) return fail(`there is a bot called ${name} already`);
  if (app.bots.list.length >= MAX_BOTS) return fail(`there can be at most ${MAX_BOTS} bots`);
  if (o.access.chosen === null) return fail('choose its access');
  const model = o.model.chosen === null ? '' : o.model.options[o.model.chosen];
  const bot = app.bots.create({
    name,
    agent: kind,
    model: model === 'default' ? '' : model,
    access: ACCESS_LABELS[o.access.chosen][0],
    about: o.about.chars.join(''),
  });
  app.closeOverlay();
  app.bots.show(bot.id);
}

function step(o: BotForm, back: boolean): void {
  const at = CONTROLS.indexOf(o.focus);
  focus(o, CONTROLS[(at + (back ? CONTROLS.length - 1 : 1)) % CONTROLS.length]);
}

function listKey(o: BotForm, list: List, k: Key, rows: number): void {
  const c = o[list];
  if (list === 'model' && c.open && k.key === 'Enter' && !comboShown(c).length) {
    c.options.push(c.open.query);
    chooseCombo(c, c.options.length - 1);
    return;
  }
  if (comboKey(c, k, rows) && list === 'agent') refit(o);
}

export function botFormKey(app: App, o: BotForm, k: Key): void {
  o.error = undefined;
  const screen = rect(0, 0, app.cols, app.rows);
  const open = opened(o);
  const list = listArea(screen, o);
  if (open) return listKey(o, open, k, list ? comboRows(list) : COMBO_ROWS);
  if (k.key === 'Escape') return app.closeOverlay();
  if (k.key === 'Tab') return step(o, !!k.shift);
  const field = fieldOf(o, o.focus);
  if (k.key === 'Enter' && !(field && o.focus === 'about')) return submitBotForm(app, o);
  if (field && k.key === 'Enter') pasted(field, '\n');
  else if (field) editKey(field, k, textRect(screen, o.focus).w);
  else if (isList(o.focus)) listKey(o, o.focus, k, COMBO_ROWS);
}

export function botFormPaste(o: BotForm, text: string): void {
  const field = fieldOf(o, o.focus);
  if (!field || opened(o)) return;
  pasted(field, o.focus === 'name' ? text.replace(/\s+/g, ' ') : text);
}

function press(app: App, o: BotForm, c: Control, x: number, y: number): void {
  const screen = rect(0, 0, app.cols, app.rows);
  const opening = isList(c) && !o[c].open;
  focus(o, c);
  const field = fieldOf(o, c);
  const t = textRect(screen, c);
  if (field) placeIn(field, t.w, Math.max(0, y - t.y), Math.max(0, x - t.x));
  else if (opening && isList(c)) openCombo(o[c]);
  app.dirty();
}

function drawField(p: Painter, r: Rect, t: Rect, f: Editable, focused: boolean, placeholder: string): void {
  const bg: Style = { bg: p.surface };
  p.g.fill(r, bg);
  const [line, col] = fieldPosition(f, t.w);
  const start = Math.max(0, line - (t.h - 1));
  if (!f.chars.length) p.span(t.x, t.y, placeholder, { ...bg, ...MUTED }, t.w);
  else {
    fieldLines(f, t.w)
      .slice(start, start + t.h)
      .forEach(([a, b], i) => p.span(t.x, t.y + i, f.chars.slice(a, b).filter((c) => c !== '\n').join(''), { ...bg, fg: p.app.light ? 0 : 15, add: BOLD }, t.w));
    if (focused) drawSelection(p, f, t, start, bg);
  }
  if (focused && !isEmpty(t)) p.cursor = { shape: 'block', x: t.x + Math.min(col, t.w - 1), y: t.y + Math.min(line - start, t.h - 1) };
}

function drawNotes(p: Painter, screen: Rect): void {
  const r = inner(screen.w, screen.h);
  NOTES.forEach((text, i) => {
    const y = r.y + ROWS.access + 1 + i;
    p.span(r.x + LABEL + 1, y, truncateRight(text, Math.max(0, r.w - LABEL - 1)), i ? { fg: 1 } : MUTED, Math.max(0, r.w - LABEL - 1));
  });
}

function drawControls(p: Painter, o: BotForm, screen: Rect, open: List | null): void {
  const app = p.app;
  for (const c of CONTROLS) {
    const r = controlRect(screen, c);
    const field = fieldOf(o, c);
    if (field) drawField(p, r, textRect(screen, c), field, !open && o.focus === c, c === 'name' ? '' : 'optional: what the bot is for');
    else if (isList(c)) drawComboField(p, r, o[c], c === 'access' ? 'choose its access' : '', o.focus === c);
    p.region({
      r,
      click: (x, y) => press(app, o, c, x, y),
      cursor: field ? 'text' : 'pointer',
      field: field
        ? { edit: (change) => change(field, textRect(screen, c).w), focused: () => o.focus === c, spot: (x, y) => [Math.max(0, y - r.y), Math.max(0, x - textRect(screen, c).x)] }
        : undefined,
    });
  }
}

export function drawBotForm(p: Painter, o: BotForm): void {
  const app = p.app;
  const screen = rect(0, 0, app.cols, app.rows);
  const area = botFormArea(app.cols, app.rows);
  const open = opened(o);
  const scroll = (dy: number): boolean => {
    const list = listArea(screen, o);
    if (open && list) scrollCombo(o[open], dy, comboRows(list));
    app.dirty();
    return true;
  };
  p.g.clear(area);
  p.g.box(area, MUTED, 'New bot', { add: BOLD, fg: -1 });
  p.region({ r: area, click: () => { closeLists(o); app.dirty(); }, wheel: scroll });
  const r = inner(screen.w, screen.h);
  for (const [c, label] of [['name', 'Name'], ['agent', 'Agent'], ['model', 'Model'], ['access', 'Access'], ['about', 'For']] as const) p.span(r.x, r.y + ROWS[c], label, MUTED, LABEL);
  drawControls(p, o, screen, open);
  drawNotes(p, screen);
  const buttons = bottom(r) - 1;
  if (o.error) {
    const y = buttons - 2;
    wrapText(o.error, r.w).slice(0, buttons - y).forEach((t, i) => p.span(r.x, y + i, t, { fg: 1 }, r.w));
  }
  p.dialogButtons(rect(r.x, buttons, r.w, isEmpty(r) ? 0 : 1), 'create', () => submitBotForm(app, o), () => app.closeOverlay());
  const list = listArea(screen, o);
  if (!open || !list) return;
  p.region({ r: botFormBounds(app.cols, app.rows, o), click: () => { closeLists(o); app.dirty(); }, wheel: scroll });
  drawComboList(p, list, o[open], (i) => { chose(o, open, i); app.dirty(); });
}
