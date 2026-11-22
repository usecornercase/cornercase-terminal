import { BOLD, type Rect, type Style, rect } from '../term/grid';
import { DEFAULT } from '../term/palette';
import { type Bot, EXAMPLES, type ChatLine, age, clock, details, shortClock, summary } from './bots';
import { type Areas, BACK_LABEL, BOTS_LABEL, Rows, bottom, buttonWidth, closeButton, isEmpty, marksWidth, menuButton, right } from './layout';
import { render as markdown } from './markdown';
import { type Line, seg, truncateRight, width } from './text';
import { drawSelection, fieldLines, fieldPosition, placeIn } from './todo';
import type { Painter } from './ui';

const DARK: Style = { fg: 8 };
const CYAN: Style = { fg: 6 };
const RED: Style = { fg: 1 };
const SPINNER = '⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏';
const SEND = 'send';
const NEW_CONVERSATION = 'new conversation';
const ROUTINES = 'routines';
const NAME_ROOM = 12;
const MAX_INPUT = 6;
const FOLD = 3;
const MENU_LEFT_OUT = 'this needs cornercase on your computer: the demo has no bot menu';
const NEW_BUTTON = '+ New';

interface Act {
  x: number;
  w: number;
  fn: () => void;
}

interface Row {
  x: number;
  w: number;
  line: Line;
  bg?: Style;
  acts?: Act[];
  run?: number;
}

export function wrapText(text: string, max: number): string[] {
  const w = Math.max(1, max);
  const out: string[] = [];
  for (const paragraph of text.split('\n')) {
    let row = '';
    for (const word of paragraph.split(' ')) {
      let rest = word;
      while ([...rest].length > w) {
        if (row) out.push(row);
        row = '';
        out.push([...rest].slice(0, w).join(''));
        rest = [...rest].slice(w).join('');
      }
      const joined = row ? `${row} ${rest}` : rest;
      if ([...joined].length <= w) row = joined;
      else {
        out.push(row);
        row = rest;
      }
    }
    out.push(row);
  }
  return out;
}

const spin = (p: Painter): string => SPINNER[Math.floor(p.app.now() / 100) % SPINNER.length];
const plural = (n: number, word: string): string => `${n} ${word}${n === 1 ? '' : 's'}`;
const mark = (shown: boolean): Line => [seg(shown ? '▌ ' : '  ', CYAN)];

function drawRow(p: Painter, r: Rect, bot: Bot, active: boolean): void {
  const app = p.app;
  const pitch = Math.max(1, r.h);
  const bg = p.rowBackground(r, active);
  const menu = menuButton(r, pitch);
  const close = closeButton(r, pitch);
  const flag: Line = bot.turn
    ? [seg(spin(p), { fg: 3 }), seg(' ')]
    : bot.unread
      ? [seg(`●${bot.unread > 9 ? '9+' : bot.unread}`, CYAN), seg(' ')]
      : [];
  const lead = [...mark(active), ...flag];
  const room = Math.max(0, r.w - width(lead) - close.w - menu.w - 1);
  const name = truncateRight(bot.name, room);
  const next = p.app.bots.nextRun(bot);
  const later = !bot.turn && next !== null ? `next ${shortClock(next)}` : '';
  const spare = room - [...name].length;
  const tag = later && spare - [...later].length >= 2 ? later : summary(bot);
  const gap = spare - [...tag].length;
  const line: Line = [...lead, seg(name, active ? { fg: DEFAULT, add: BOLD } : { fg: 7 })];
  if (gap >= 2) line.push(seg(' '.repeat(gap)), seg(tag, DARK));
  p.band(r, line, bg);
  if (active) p.rail(r);
  const open = (): void => app.bots.show(bot.id);
  const side = (): void => app.notify(MENU_LEFT_OUT);
  p.region({ r, click: open, right: side, cursor: 'pointer' });
  if (!p.sidebarHovered(r) && pitch === 1) return;
  const style: Style = p.sidebarHovered(r) && p.hovered(menu) ? { ...bg, fg: 6, add: BOLD } : { ...bg, fg: 8 };
  p.band(menu, [seg('⋯'.padStart(menu.w))], style);
  p.region({ r: menu, click: side, right: side, cursor: 'pointer' });
}

const middleOf = (r: Rect): Rect => rect(r.x, r.y + Math.floor(Math.max(0, r.h - 1) / 2), r.w, Math.min(r.h, 1));

function drawBotsButton(p: Painter, b: Rect): void {
  const app = p.app;
  const bots = app.bots;
  const style = p.buttonStyle(b, CYAN, 6);
  const lit = p.sidebarHovered(b);
  if (lit && b.h > 1) p.g.fill(b, style);
  const marked = (text: string, own: Style): Line => [seg(text, lit ? style : own), seg(' ', style)];
  const line: Line = [seg(' ', style)];
  if (b.w >= buttonWidth(BOTS_LABEL) + marksWidth(bots.thinking(), bots.unread())) {
    if (bots.thinking()) line.push(...marked(spin(p), { fg: 3 }));
    if (bots.unread()) line.push(...marked(`●${bots.unread() > 9 ? '9+' : bots.unread()}`, CYAN));
  }
  line.push(seg(`${BOTS_LABEL} `, style));
  p.line(middleOf(b), line);
  p.region({ r: b, click: () => app.openBots(), cursor: 'pointer' });
}

function pageTitle(p: Painter, areas: Areas): void {
  if (areas.compact) return p.back(areas, 'Bots');
  const back = areas.back;
  p.button(back, '', BACK_LABEL, p.buttonStyle(back, CYAN, 6));
  p.region({ r: back, click: () => p.app.pressBack(), cursor: 'pointer' });
  p.title(rect(right(back), back.y, Math.max(0, right(areas.botsTitle) - right(back)), back.h), 'Bots');
}

export function drawBots(p: Painter, areas: Areas): void {
  const app = p.app;
  const bots = app.bots;
  if (!isEmpty(areas.botsButton)) drawBotsButton(p, areas.botsButton);
  if (isEmpty(areas.bots)) return;
  pageTitle(p, areas);
  const list = areas.botsList;
  if (isEmpty(list)) return;
  const rows = new Rows(list, Array<number>(bots.list.length).fill(areas.pitch), areas.pitch, bots.scroll);
  p.region({
    r: list,
    wheel: (dy) => {
      const next = rows.scrolled(dy);
      if (next === bots.scroll) return false;
      bots.scroll = next;
      app.dirty();
      return true;
    },
  });
  bots.list.forEach((bot, i) => {
    const r = rows.item(i);
    if (!isEmpty(r)) drawRow(p, r, bot, bot.id === bots.shown);
  });
  p.more(list, rows, bots.list, () => true);
  const add = rows.buttonRect();
  p.g.clear(add);
  p.button(add, ' ', NEW_BUTTON, p.buttonStyle(add, CYAN, 6));
  p.region({ r: add, click: () => app.openBotForm(), cursor: 'pointer' });
}

function youRows(p: Painter, bot: Bot, l: ChatLine, w: number): Row[] {
  const max = Math.max(6, Math.floor(w * 0.8));
  const lines = wrapText(l.text, max - 2);
  const tag = bot.held.has(l.id) ? 'held' : clock(l.at);
  const last = [...lines[lines.length - 1]].length;
  const inline = last + 1 + tag.length <= max - 2;
  const longest = Math.max(...lines.map((t) => [...t].length), inline ? last + 1 + tag.length : tag.length);
  const bw = Math.min(max, longest + 2);
  const bg: Style = { bg: p.surface };
  const rows: Row[] = lines.map((text) => ({ x: w - bw, w: bw, bg, line: [seg(' '), seg(text), seg(' '.repeat(Math.max(0, bw - 2 - [...text].length))), seg(' ')] }));
  const end = rows[rows.length - 1];
  const dim: Style = { ...DARK, bg: p.surface };
  if (inline) end.line = [seg(' '), seg(lines[lines.length - 1]), seg(' '.repeat(bw - 2 - last - tag.length)), seg(tag, dim), seg(' ')];
  else rows.push({ x: w - bw, w: bw, bg, line: [seg(' '.repeat(Math.max(0, bw - 1 - tag.length))), seg(tag, dim), seg(' ')] });
  return rows;
}

function stepRows(p: Painter, bot: Bot, run: ChatLine[], w: number): Row[] {
  const rows: Row[] = [];
  const hidden = run.length - FOLD;
  const folded = hidden > 0 && !bot.open.has(run[0].id);
  if (folded) {
    const label = `+${plural(hidden, 'step')}`;
    rows.push({ x: 0, w, line: [seg(label, DARK)], acts: [{ x: 0, w: label.length, fn: () => { bot.open.add(run[0].id); p.app.dirty(); } }] });
  }
  for (const l of folded ? run.slice(hidden) : run) rows.push({ x: 0, w, line: [seg(truncateRight(`▸ ${l.text}`, w), DARK)] });
  return rows;
}

function errorRows(p: Painter, bot: Bot, l: ChatLine, w: number, last: boolean): Row[] {
  const rows: Row[] = wrapText(l.text, w).map((t) => ({ x: 0, w, line: [seg(t, RED)] }));
  if (!last || bot.turn) return rows;
  const end = rows[rows.length - 1];
  const x = width(end.line) + 1;
  const label = ' retry ';
  end.line = [...end.line, seg(' '), seg(label, { ...CYAN, add: BOLD })];
  end.acts = [{ x, w: label.length, fn: () => p.app.bots.retry(bot) }];
  return rows;
}

function botRows(l: ChatLine, bot: Bot, w: number): Row[] {
  const head: Row = { x: 0, w, line: [seg(`${bot.name} · ${clock(l.at)}`, DARK)] };
  return [head, ...markdown(l.text, w).map((line): Row => ({ x: 0, w, line }))];
}

function thinkingRow(p: Painter, bot: Bot, w: number): Row {
  const turn = bot.turn;
  if (!turn) return { x: 0, w, line: [] };
  if (turn.stopping) return { x: 0, w, line: [seg(spin(p), { fg: 3 }), seg(' stopping…', DARK)] };
  const steps = turn.steps ? ` · ${plural(turn.steps, 'step')}` : '';
  const head = ` thinking · ${age(turn.since, p.app.now())}${steps}`;
  const label = ' stop ';
  return {
    x: 0,
    w,
    line: [seg(spin(p), { fg: 3 }), seg(head, DARK), seg('  '), seg(label, CYAN)],
    acts: [{ x: [...head].length + 3, w: label.length, fn: () => p.app.bots.stop(bot) }],
  };
}

const RUN_LEFT_OUT = 'this needs cornercase on your computer: the demo runs no bot routines';

function runRows(p: Painter, bot: Bot, lines: ChatLine[], w: number): Row[] {
  const head = lines[0].kind === 'notice' ? lines[0].text : '↻ a routine';
  const rows: Row[] = [{ x: 0, w, run: lines[0].routine, line: [seg(truncateRight(head, w), { ...DARK, add: BOLD })] }];
  const steps = lines.filter((l) => l.kind === 'step');
  if (steps.length) {
    const first = steps[0].id;
    if (bot.open.has(first)) for (const l of steps) rows.push({ x: 0, w, line: [seg(truncateRight(`▸ ${l.text}`, w), DARK)] });
    else {
      const label = `+${plural(steps.length, 'step')}`;
      rows.push({ x: 0, w, line: [seg(label, DARK)], acts: [{ x: 0, w: label.length, fn: () => { bot.open.add(first); p.app.dirty(); } }] });
    }
  }
  for (const l of lines) {
    if (l.kind === 'bot') rows.push(...botRows(l, bot, w));
    else if (l.kind === 'error') {
      const label = ' run again ';
      const rest = wrapText(l.text, w).map((t) => ({ x: 0, w, line: [seg(t, RED)] }));
      const end = rest[rest.length - 1];
      const x = width(end.line) + 1;
      end.line = [...end.line, seg(' '), seg(label, { ...CYAN, add: BOLD })];
      rows.push(...rest.slice(0, -1), { ...end, acts: [{ x, w: label.length, fn: () => p.app.notify(RUN_LEFT_OUT) }] });
    }
  }
  return rows;
}

const SHOW_LEFT_OUT = ' show ';
const BODY_SHOWN = 6;

function agentRows(p: Painter, bot: Bot, l: ChatLine, w: number): Row[] {
  const glyph = [...l.text][0] ?? '';
  const tone: Style = l.kind === 'started' ? DARK : l.text.startsWith('✗') ? { fg: 1, add: BOLD } : l.kind === 'finished' ? { fg: 2, add: BOLD } : { fg: 208, add: BOLD };
  const room = Math.max(0, w - SHOW_LEFT_OUT.length - 1);
  const text = truncateRight(l.text, room);
  const rest = [...text].slice(1).join('');
  const head: Line = [seg(glyph, tone), seg(rest, l.kind === 'started' ? DARK : {}), seg(' '), seg(SHOW_LEFT_OUT, CYAN)];
  const rows: Row[] = [{ x: 0, w, line: head }];
  if (l.body === undefined || l.kind === 'started') return rows;
  const body = l.kind === 'finished' ? markdown(l.body, Math.max(1, w - 2)) : l.body.split('\n').map((t): Line => [seg(t)]);
  const hidden = bot.open.has(l.id) ? 0 : Math.max(0, body.length - BODY_SHOWN);
  for (const line of body.slice(0, body.length - hidden)) rows.push({ x: 0, w, line: [seg('  '), ...line.map((part) => ({ ...part, s: { ...part.s, fg: part.s?.fg ?? 8 } }))] });
  if (hidden > 0) {
    const label = `  +${plural(hidden, 'line')}`;
    rows.push({ x: 0, w, line: [seg(label, DARK)], acts: [{ x: 0, w: label.length, fn: () => { bot.open.add(l.id); p.app.dirty(); } }] });
  }
  return rows;
}

function block(p: Painter, bot: Bot, l: ChatLine, w: number, last: boolean): Row[] {
  if (l.kind === 'started' || l.kind === 'finished' || l.kind === 'waiting') return agentRows(p, bot, l, w);
  if (l.kind === 'you') return youRows(p, bot, l, w);
  if (l.kind === 'bot') return botRows(l, bot, w);
  if (l.kind === 'error') return errorRows(p, bot, l, w, last);
  return wrapText(l.text, Math.max(1, w - 6)).map((row) => {
    const text = truncateRight(`── ${row} ──`, w);
    return { x: Math.max(0, Math.floor((w - [...text].length) / 2)), w, line: [seg(text, DARK)] };
  });
}

function chatRows(p: Painter, bot: Bot, w: number): Row[] {
  const rows: Row[] = [];
  let previous: ChatLine['kind'] | null = null;
  const lines = bot.lines;
  const seen = new Set<number>();
  for (let i = 0; i < lines.length; i++) {
    const l = lines[i];
    if (l.routine !== undefined) {
      if (seen.has(l.routine)) continue;
      seen.add(l.routine);
      if (previous !== null) rows.push({ x: 0, w, line: [] });
      rows.push(...runRows(p, bot, lines.filter((m) => m.routine === l.routine), w));
      previous = 'notice';
      continue;
    }
    if (previous !== null && l.kind !== 'step') rows.push({ x: 0, w, line: [] });
    if (l.kind === 'step') {
      const run = [l];
      while (lines[i + 1]?.kind === 'step') run.push(lines[++i]);
      rows.push(...stepRows(p, bot, run, w));
    } else rows.push(...block(p, bot, l, w, i === lines.length - 1));
    previous = l.kind;
  }
  if (bot.turn) {
    rows.push(thinkingRow(p, bot, w));
  }
  return rows;
}

function emptyRows(p: Painter, bot: Bot, w: number): Row[] {
  const intro = ['A bot is an agent of your own. It belongs to no project and keeps a memory.', 'It answers here when you write to it, and on a schedule it sets itself. Try:'];
  const rows: Row[] = intro.flatMap((line) => wrapText(line, Math.max(1, w))).map((t) => ({ x: Math.floor((w - [...t].length) / 2), w, line: [seg(t, DARK)] }));
  rows.push({ x: 0, w, line: [] });
  for (const text of EXAMPLES) {
    const shown = truncateRight(text, w);
    const x = Math.floor((w - [...shown].length) / 2);
    rows.push({ x, w: w - x, line: [seg(shown, CYAN)], acts: [{ x: 0, w: [...shown].length, fn: () => p.app.bots.example(bot, text) }] });
  }
  return rows;
}

interface Layout {
  header: Rect;
  rule: Rect;
  lines: Rect;
  input: Rect;
  text: Rect;
  send: Rect;
}

function chatLayout(area: Rect, pitch: number, bot: Bot): Layout {
  const sendW = buttonWidth(SEND);
  const tw = Math.max(1, area.w - 2 - sendW - 1);
  const rowsNeeded = Math.max(1, Math.min(MAX_INPUT, fieldLines(bot.draft, tw).length));
  const h = Math.max(rowsNeeded, pitch);
  const input = rect(area.x, bottom(area) - h, area.w, h);
  const header = rect(area.x, area.y, area.w, pitch);
  const rule = rect(area.x, area.y + pitch, area.w, 1);
  const top = rule.y + 1;
  return {
    header,
    rule,
    lines: rect(area.x + 1, top, Math.max(0, area.w - 2), Math.max(0, input.y - 1 - top)),
    input,
    text: rect(area.x + 2, input.y, tw, rowsNeeded),
    send: rect(right(area) - sendW - 1, input.y, sendW, pitch > 1 ? h : 1),
  };
}

function drawHeader(p: Painter, l: Layout, bot: Bot): void {
  const app = p.app;
  const h = l.header;
  const menu = rect(right(h) - 4, h.y, 3, h.h);
  const wide = buttonWidth(NEW_CONVERSATION);
  const fresh = rect(menu.x - 1 - wide, h.y, wide, h.h);
  const routinesW = buttonWidth(ROUTINES);
  const routinesAt = fresh.x - 1 - routinesW;
  const routines = routinesAt < h.x + NAME_ROOM ? rect(0, 0, 0, 0) : rect(routinesAt, h.y, routinesW, h.h);
  const room = Math.max(0, (isEmpty(routines) ? fresh.x : routines.x) - h.x - 2);
  const name = truncateRight(bot.name, room);
  const line: Line = [seg(' '), seg(name, { add: BOLD })];
  const rest = room - [...name].length - 2;
  if (rest > 3) line.push(seg('  '), seg(truncateRight(details(bot), rest), DARK));
  p.band(h, line, {});
  p.button(routines, '', ROUTINES, p.buttonStyle(routines, CYAN, 6));
  p.region({ r: routines, click: () => app.openBotRoutines(bot.id), cursor: 'pointer' });
  p.button(fresh, '', NEW_CONVERSATION, bot.turn ? DARK : p.buttonStyle(fresh, CYAN, 6));
  p.region({ r: fresh, click: () => app.bots.newConversation(bot), cursor: 'pointer' });
  p.button(menu, '', '⋯', p.buttonStyle(menu, CYAN, 6));
  p.region({ r: menu, click: () => app.notify(MENU_LEFT_OUT), cursor: 'pointer' });
  p.span(l.rule.x, l.rule.y, '─'.repeat(l.rule.w), { fg: p.lineColour });
}

function drawLines(p: Painter, area: Rect, bot: Bot): void {
  const app = p.app;
  const empty = !bot.lines.length && !bot.turn;
  const rows = empty ? emptyRows(p, bot, area.w) : chatRows(p, bot, area.w);
  const max = Math.max(0, rows.length - area.h);
  if (bot.anchor !== null) {
    const at = rows.findIndex((r) => r.run === bot.anchor);
    if (at >= 0) bot.scroll = Math.max(0, Math.min(max, rows.length - area.h - at));
    bot.anchor = null;
  }
  bot.scroll = Math.min(bot.scroll, max);
  p.region({ r: area, wheel: (dy) => app.bots.scrollTo(bot, -dy, max) });
  const lift = empty ? Math.floor(Math.max(0, area.h - rows.length) / 2) : 0;
  const start = rows.length - area.h - bot.scroll;
  for (let y = 0; y < area.h; y++) {
    const row = rows[empty ? y - lift : start + y];
    if (!row) continue;
    const r = rect(area.x + row.x, area.y + y, row.w, 1);
    p.line(r, row.line, row.bg);
    for (const a of row.acts ?? []) p.region({ r: rect(r.x + a.x, r.y, a.w, 1), click: a.fn, cursor: 'pointer' });
  }
  if (bot.scroll > 0 && bot.below) {
    const label = ' ↓ new ';
    const r = rect(right(area) - label.length, bottom(area) - 1, label.length, 1);
    p.span(r.x, r.y, label, { fg: 0, bg: 6, add: BOLD });
    p.region({ r, click: () => app.bots.scrollTo(bot, -bot.scroll, max), cursor: 'pointer' });
  }
}

function drawInput(p: Painter, l: Layout, bot: Bot): void {
  const app = p.app;
  const bg: Style = { bg: p.surface };
  const t = l.text;
  p.g.fill(l.input, bg);
  const draft = bot.draft;
  const lines = fieldLines(draft, t.w);
  const [line, col] = fieldPosition(draft, t.w);
  const start = Math.max(0, line - (t.h - 1));
  if (!draft.chars.length) p.span(t.x, t.y, truncateRight(`Message ${bot.name}…`, t.w), { ...bg, ...DARK }, t.w);
  else {
    lines.slice(start, start + t.h).forEach(([a, b], i) => {
      const text = draft.chars.slice(a, b).filter((c) => c !== '\n').join('');
      p.span(t.x, t.y + i, text, { ...bg, fg: p.app.light ? 0 : 15 }, t.w);
    });
    drawSelection(p, draft, t, start, bg);
  }
  if (!app.overlay) p.cursor = { shape: 'block', x: t.x + Math.min(col, t.w - 1), y: t.y + Math.min(line - start, t.h - 1) };
  p.region({
    r: t,
    cursor: 'text',
    click: (x, y) => placeIn(draft, t.w, start + (y - t.y), x - t.x),
    field: {
      edit: (change) => change(bot.draft, t.w),
      focused: () => true,
      spot: (x, y) => [start + (y - t.y), Math.max(0, x - t.x)],
    },
  });
  p.button(l.send, '', SEND, p.buttonStyle(l.send, CYAN, 6));
  p.region({ r: l.send, click: () => app.bots.send(bot, draft.chars.join('')), cursor: 'pointer' });
}

export function drawChat(p: Painter, area: Rect, bot: Bot, pitch: number): void {
  if (area.h < pitch + 4 || area.w < 12) return;
  p.g.clear(area);
  const l = chatLayout(area, pitch, bot);
  drawHeader(p, l, bot);
  drawLines(p, l.lines, bot);
  drawInput(p, l, bot);
}

const LABEL = 8;
const ACCESS_NOTE: Record<string, string> = { cornercase: 'through cornercase', full: 'full access' };

export function drawCard(p: Painter, areas: Areas): void {
  const app = p.app;
  const bot = app.bots.current();
  const list = areas.workspacesList;
  if (!bot) return;
  p.title(areas.workspacesTitle, 'Bot');
  const room = Math.max(1, list.w - LABEL - 2);
  const entries: [string, string, Style?][] = [
    ['agent', bot.agent === 'codex' ? 'Codex' : 'Claude Code'],
    ['model', bot.model || "the agent's own"],
    ...(bot.effort ? [['effort', bot.effort] as [string, string]] : []),
    ...(bot.account ? [['account', bot.account] as [string, string]] : []),
    ['access', ACCESS_NOTE[bot.access], bot.access === 'full' ? RED : undefined],
    ['folder', app.bots.folder(bot)],
    ['made', new Date(bot.made).toISOString().slice(0, 10)],
  ];
  const routines = app.botQueued.filter((e) => e.bot === bot.id && !e.routine && e.repeat);
  const ids: (number | undefined)[] = entries.map(() => undefined);
  for (const e of routines) {
    const next = e.paused ? 'paused' : `next ${shortClock(e.at)}`;
    entries.push(['routine', e.label], ['', `${next} · ↻ ${e.repeat}`]);
    ids.push(e.id, e.id);
  }
  interface CardRow {
    label: string;
    text: string;
    value: string;
    style?: Style;
    y: number;
    id?: number;
    head: boolean;
  }
  const laid: CardRow[] = [];
  let y = list.y + 2;
  let cut = false;
  entries.forEach(([label, value, style], at) => {
    if (label === 'routine' && y + 2 > bottom(list)) cut = true;
    const rows = label === 'folder' || label === 'routine' || label === '' ? [truncateRight(value, room)] : wrapText(value, room);
    rows.forEach((text, i) => {
      if (!cut && y < bottom(list)) laid.push({ label, text, value, style, y, id: ids[at], head: i === 0 });
      y += 1;
    });
  });
  const over = (r: CardRow): Rect => rect(list.x + 1, r.y, Math.max(1, list.w - 1), 1);
  const hot = laid.find((r) => r.id !== undefined && p.hovered(over(r)))?.id;
  const head = laid.find((r) => r.id !== undefined && r.id === hot);
  const span = head && rect(list.x + 1, head.y, Math.max(1, list.w - 1), laid.filter((r) => r.id === hot).length);
  const menu = head && span && p.sidebarHovered(span) ? menuButton(over(head), 1) : undefined;
  p.span(list.x + 1, list.y, bot.name, { fg: DEFAULT, add: BOLD }, list.w - 1);
  for (const r of laid) {
    if (r.head) p.span(list.x + 1, r.y, r.label, DARK, LABEL);
    const shown = menu && menu.w > 0 && r === head ? truncateRight(r.value, Math.max(0, menu.x - 1 - (list.x + 1 + LABEL))) : r.text;
    p.span(list.x + 1 + LABEL, r.y, shown, r.id !== undefined && r.id === hot ? CYAN : r.style ?? {}, room);
    const id = r.id;
    if (id !== undefined) {
      const menuAt = (x: number, y: number): void => app.openRoutineMenu({ x, y }, bot.id, id);
      p.region({ r: over(r), click: () => app.openBotRoutines(bot.id, id), right: menuAt, cursor: 'pointer' });
    }
    if (r.label === 'folder') p.region({ r: rect(list.x + 1 + LABEL, r.y, room, 1), click: () => app.notify('copied to clipboard'), cursor: 'pointer' });
  }
  if (menu && menu.w > 0) {
    const open = (x: number, y: number): void => app.openRoutineMenu({ x, y }, bot.id, hot as number);
    p.band(menu, [seg('⋯'.padStart(menu.w))], p.hovered(menu) ? { fg: 6, add: BOLD } : { fg: 8 });
    p.region({ r: menu, click: open, right: open, cursor: 'pointer' });
  }
}
