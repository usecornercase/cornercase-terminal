import { BOLD, type Rect, type Style, contains, rect } from '../term/grid';
import { drawPanelBorder } from './changes';
import { BRAND, DARK, type Diagram, type DiagramHit, HEADER, WAITING, diagramHeight, drawDiagram, len } from './flow';
import { type Areas, bottom, right } from './layout';
import type { Painter } from './ui';
import { truncateRight } from './text';
import type { NodeState } from './flow';

const LEAD = 2;
const PREVIEW = 2;
const MIN_NAME = 4;
const MIN_ROLE = 4;
const MIN_TEXT = 4;
const MIN_BRANCH = 6;
const MIN_FILE_NAME = 8;
const GUTTER = '▎';
const INDENT = 2;
const LEAD_BUTTON = ' orchestrator ';
const STOP_BUTTON = ' stop ';
const FOLDER_BUTTON = ' open folder ';
const ACTION: Style = { fg: 6 };
const ACTION_LIT: Style = { fg: 0, bg: 6, add: BOLD };

export type PanelStep = { kind: 'forward'; stage: number } | { kind: 'loop'; when: string } | { kind: 'outside' };

export interface PanelHeading {
  place: string;
  run: number;
  messages: number;
  active: number | null;
  lead: boolean;
}

export interface PanelMember {
  role: string;
  agent: string;
  colour: number;
  state: NodeState;
  branch: string | null;
  pane: number | null;
}

export interface PanelEntry {
  seq: number;
  from: string;
  to: string;
  kind: string;
  step: PanelStep | null;
  colour: number;
  age: string;
  head: string | null;
}

export interface PanelHistory {
  entries: PanelEntry[];
  recorded: number;
}

export interface PanelFile {
  name: string;
  size: string;
  age: string;
}

export interface PanelView {
  name: string;
  task: string;
  heading: PanelHeading;
  team: PanelMember[];
  diagram: Diagram;
  history: PanelHistory;
  files: PanelFile[] | null;
  scroll: number;
}

export type PanelHit = DiagramHit & { stop?: boolean; message?: number; folder?: boolean };

interface Slot {
  area: Rect;
  skip: number;
}

const slot = (body: Rect, top: number, scroll: number): Slot | null => {
  const y = body.y + Math.max(0, top - scroll);
  return y < bottom(body) ? { area: rect(body.x, y, body.w, bottom(body) - y), skip: Math.max(0, scroll - top) } : null;
};

const rowY = (s: Slot, row: number): number | null => {
  const r = row - s.skip;
  const y = s.area.y + r;
  return r >= 0 && y < bottom(s.area) ? y : null;
};

const rowAt = (s: Slot, x: number, y: number): number | null => (contains(s.area, x, y) ? y - s.area.y + s.skip : null);

export function fileAge(ms: number): string {
  const minutes = Math.max(0, Math.floor(ms / 60000));
  const hours = Math.floor(minutes / 60);
  if (minutes < 60) return `${minutes}m`;
  return hours < 24 ? `${hours}h` : `${Math.floor(hours / 24)}d`;
}

const reached = (view: PanelView): boolean => !!view.diagram.live?.ended;

function summary(view: PanelView): [string, Style][] {
  const stages = view.diagram.stages.length;
  const active = view.heading.active;
  const out: [string, Style][] = [[active === null ? 'not started' : `stage ${active + 1} of ${stages}`, {}]];
  for (const back of view.diagram.loops) {
    if (back && back.rounds > 0) out.push([` · ${back.when} ${back.rounds}/${back.atMost}`, back.rounds > back.atMost ? { fg: WAITING } : DARK]);
  }
  return out;
}

const summaryWidth = (view: PanelView): number => summary(view).reduce((n, [t]) => n + len(t), 0);
const buttonsWidth = (view: PanelView): number => (view.heading.lead ? len(LEAD_BUTTON) + 1 : 0) + len(STOP_BUTTON);
const fits = (view: PanelView, width: number): boolean => summaryWidth(view) + 1 + buttonsWidth(view) <= width;
const headingHeight = (view: PanelView, width: number): number => (fits(view, width) ? 3 : 4);

const teamHeight = (team: PanelMember[]): number => (team.length ? LEAD + team.length : 0);

function entryPlan(entry: PanelEntry, width: number): { inline: boolean; lines: string[] } {
  const step = entry.step ? len(` · ${stepLabel(entry.step)}`) : 0;
  const left = INDENT + len(entry.from) + len(' → ') + len(entry.to) + len(` · ${entry.kind}`);
  const tail = len(`#${entry.seq} · ${entry.age}`);
  return { inline: left + step + tail + 1 <= width, lines: preview(entry.head, width) };
}

const entryRows = (entry: PanelEntry, width: number): number => {
  const plan = entryPlan(entry, width);
  return 1 + (!plan.inline && entry.step ? 1 : 0) + plan.lines.length;
};

function preview(head: string | null, width: number): string[] {
  const room = Math.max(1, width - INDENT);
  const rows: string[] = [];
  for (const line of (head ?? '').split('\n')) {
    let current = '';
    for (const word of line.split(' ')) {
      const next = current ? `${current} ${word}` : word;
      if (len(next) > room && current) {
        rows.push(current);
        current = word;
      } else current = next;
    }
    rows.push(current);
    if (rows.length > PREVIEW) break;
  }
  const more = rows.length > PREVIEW;
  const kept = rows.slice(0, PREVIEW);
  if (more) kept[PREVIEW - 1] = truncateRight(`${kept[PREVIEW - 1]}…`, room);
  return head === null ? [] : kept;
}

const stepLabel = (step: PanelStep): string => (step.kind === 'forward' ? `stage ${step.stage + 1} → ${step.stage + 2}` : step.kind === 'loop' ? step.when : 'outside the plan');

const historyHeight = (h: PanelHistory, width: number): number => LEAD + (h.entries.length ? h.entries.reduce((n, e) => n + entryRows(e, width), 0) : 1);
const filesHeight = (files: PanelFile[] | null): number => (files ? LEAD + Math.max(1, files.length) : 0);

interface Tops {
  team: number;
  diagram: number;
  history: number;
  files: number;
  total: number;
}

function tops(view: PanelView, width: number): Tops {
  const team = headingHeight(view, width);
  const t = teamHeight(view.team);
  const diagram = team + t + (t > 0 ? 1 : 0);
  const history = diagram + diagramHeight(view.diagram, width);
  const files = history + historyHeight(view.history, width);
  return { team, diagram, history, files, total: files + filesHeight(view.files) };
}

export function panelBody(area: Rect): Rect {
  const y = Math.min(area.y + HEADER, bottom(area));
  return rect(area.x + 1, y, Math.max(0, area.w - 2), bottom(area) - y);
}

export function scrolled(area: Rect, view: PanelView, delta: number): number {
  const body = panelBody(area);
  const most = Math.max(0, tops(view, body.w).total - body.h);
  return Math.max(0, Math.min(most, view.scroll + delta));
}

function title(p: Painter, s: Slot, row: number, name: string, n: string): void {
  const y = rowY(s, row);
  if (y === null) return;
  p.span(s.area.x, y, name, { add: BOLD });
  p.span(s.area.x + len(name), y, ` · ${n}`, DARK);
}

function shade(p: Painter, s: Slot, from: number, to: number): void {
  const hover = p.app.overlay ? null : p.app.hover;
  const at = hover ? rowAt(s, hover.x, hover.y) : null;
  if (at === null || at < from || at >= to) return;
  for (let row = from; row < to; row++) {
    const y = rowY(s, row);
    if (y !== null) p.span(s.area.x, y, ' '.repeat(s.area.w), { bg: p.app.light ? 255 : 235 });
  }
}

function drawTitle(p: Painter, header: Rect, view: PanelView): void {
  const reachedEnd = reached(view);
  const word = reachedEnd ? 'reached its end' : 'running';
  const style: Style = reachedEnd ? { fg: BRAND } : { fg: 3 };
  const room = Math.max(0, header.w - 4);
  const tail = len(word) + 3;
  if (room < tail + MIN_NAME) return void p.span(header.x, header.y, truncateRight(word, room), style);
  const name = truncateRight(view.name, room - tail);
  p.span(header.x, header.y, name, { add: BOLD });
  p.span(header.x + len(name), header.y, ' · ', DARK);
  p.span(header.x + len(name) + 3, header.y, word, style);
}

interface Button {
  r: Rect;
  label: string;
  hit: PanelHit;
}

function buttons(s: Slot, view: PanelView): Button[] {
  const y = rowY(s, fits(view, s.area.w) ? 2 : 3);
  if (y === null) return [];
  const stop = len(STOP_BUTTON);
  const x = s.area.x + s.area.w - stop;
  const out: Button[] = [{ r: rect(x, y, stop, 1), label: STOP_BUTTON, hit: { stop: true } }];
  if (view.heading.lead) out.push({ r: rect(x - len(LEAD_BUTTON) - 1, y, len(LEAD_BUTTON), 1), label: LEAD_BUTTON, hit: { role: 'orchestrator' } });
  return out;
}

function segments(width: number, n: number): [number, number][] {
  const count = Math.max(1, n);
  const room = Math.max(0, width - (count - 1));
  const each = Math.floor(room / count);
  const extra = room % count;
  let x = 0;
  return Array.from({ length: count }, (_, i) => {
    const seg: [number, number] = [x, each + (i < extra ? 1 : 0)];
    x += seg[1] + 1;
    return seg;
  });
}

function drawHeading(p: Painter, s: Slot, view: PanelView, click: (hit: PanelHit) => void): void {
  const h = view.heading;
  const width = s.area.w;
  const right0 = `run ${h.run} · ${h.messages} ${h.messages === 1 ? 'message' : 'messages'}`;
  const y0 = rowY(s, 0);
  if (y0 !== null) {
    const tail = len(right0) + 9 <= width ? len(right0) : 0;
    if (tail) p.span(s.area.x + width - tail, y0, right0, DARK);
    p.span(s.area.x, y0, truncateRight(h.place, Math.max(0, width - tail - 1)), {});
  }
  const bar = rowY(s, 1);
  if (bar !== null) {
    segments(width, view.diagram.stages.length).forEach(([x, n], i) => {
      const on = reached(view) || (h.active !== null && i <= h.active);
      p.span(s.area.x + x, bar, (on ? '━' : '─').repeat(n), on ? { fg: BRAND } : { fg: p.lineColour });
    });
  }
  const y2 = rowY(s, 2);
  if (y2 !== null) {
    let x = s.area.x;
    for (const [text, style] of summary(view)) x = p.span(x, y2, text, style);
  }
  for (const b of buttons(s, view)) {
    const lit = p.hovered(b.r);
    p.span(b.r.x, b.r.y, b.label, lit ? ACTION_LIT : ACTION);
    p.region({ r: b.r, click: () => click(b.hit), cursor: 'pointer' });
  }
}

function teamRow(p: Painter, s: Slot, member: PanelMember, longest: number, row: number): { role: string; closed: boolean } | null {
  const y = rowY(s, row);
  if (y === null) return null;
  const closed = member.state === 'closed';
  const pane = closed || member.pane === null ? '' : `pane ${member.pane}`;
  const after = Math.max(0, s.area.w - 2 - (pane ? len(pane) + 1 : 0));
  const roleW = Math.max(Math.min(MIN_ROLE, longest), Math.min(longest, after));
  const role = truncateRight(member.role, roleW);
  const icon = member.state === 'closed' ? null : p.statusSeg(member.state === 'none' ? 'idle' : member.state);
  if (icon) p.span(s.area.x, y, icon.t, icon.s ?? {});
  else p.span(s.area.x, y, '·', DARK);
  p.span(s.area.x + 2, y, role, closed ? DARK : { fg: member.colour, add: BOLD });
  let left = Math.max(0, after - roleW - 2);
  let x = s.area.x + 2 + roleW + 2;
  const said = closed ? 'closed' : member.agent;
  if (left >= MIN_TEXT) {
    const text = truncateRight(said, left);
    p.span(x, y, text, DARK);
    x += len(text) + 2;
    left -= len(text) + 2;
  }
  if (member.branch && !closed && len(member.branch) + 2 <= left && len(member.branch) >= MIN_BRANCH) p.span(x, y, member.branch, DARK);
  if (pane) p.span(s.area.x + s.area.w - len(pane), y, pane, DARK);
  return { role: member.role, closed };
}

function drawTeam(p: Painter, s: Slot, team: PanelMember[], click: (hit: PanelHit) => void): void {
  if (!team.length) return;
  title(p, s, 1, 'Team', String(team.length));
  const longest = Math.max(...team.map((m) => len(m.role)));
  team.forEach((member, i) => {
    const row = LEAD + i;
    if (member.state !== 'closed') shade(p, s, row, row + 1);
    const drawn = teamRow(p, s, member, longest, row);
    const y = rowY(s, row);
    if (drawn && !drawn.closed && y !== null) p.region({ r: rect(s.area.x, y, s.area.w, 1), click: () => click({ role: drawn.role }), cursor: 'pointer' });
  });
}

function drawEntry(p: Painter, s: Slot, entry: PanelEntry, top: number, click: (hit: PanelHit) => void): number {
  const plan = entryPlan(entry, s.area.w);
  const extra = !plan.inline && entry.step ? 1 : 0;
  const rows = 1 + extra + plan.lines.length;
  shade(p, s, top, top + rows);
  const stepStyle = (step: PanelStep): Style => (step.kind === 'outside' ? { fg: WAITING } : DARK);
  for (let row = top; row < top + rows; row++) {
    const y = rowY(s, row);
    if (y === null) continue;
    p.span(s.area.x, y, GUTTER, { fg: entry.colour });
    p.region({ r: rect(s.area.x, y, s.area.w, 1), click: () => click({ message: entry.seq }), cursor: 'pointer' });
  }
  const y = rowY(s, top);
  if (y !== null) {
    const tail = `#${entry.seq} · ${entry.age}`;
    p.span(s.area.x + s.area.w - len(tail), y, tail, DARK);
    const limit = s.area.w - len(tail) - 1;
    let x = INDENT;
    const put = (text: string, style: Style) => {
      const room = limit - x;
      if (room >= 2) p.span(s.area.x + x, y, truncateRight(text, room), style);
      x += len(text);
    };
    put(entry.from, {});
    put(' → ', DARK);
    put(entry.to, { fg: entry.colour, add: BOLD });
    put(` · ${entry.kind}`, DARK);
    if (entry.step && plan.inline) put(` · ${stepLabel(entry.step)}`, stepStyle(entry.step));
  }
  let next = top + 1;
  if (extra && entry.step) {
    const sy = rowY(s, next++);
    if (sy !== null) p.span(s.area.x + INDENT, sy, stepLabel(entry.step), stepStyle(entry.step));
  }
  plan.lines.forEach((line, i) => {
    const ly = rowY(s, next + i);
    if (ly !== null) p.span(s.area.x + INDENT, ly, line, DARK);
  });
  return rows;
}

function drawHistory(p: Painter, s: Slot, history: PanelHistory, click: (hit: PanelHit) => void): void {
  title(p, s, 1, 'Messages', String(history.recorded));
  if (!history.entries.length) {
    const y = rowY(s, LEAD);
    if (y !== null) p.span(s.area.x, y, 'nothing yet', DARK);
    return;
  }
  let top = LEAD;
  for (const entry of history.entries) top += drawEntry(p, s, entry, top, click);
}

function drawFiles(p: Painter, s: Slot, files: PanelFile[], click: (hit: PanelHit) => void): void {
  title(p, s, 1, 'Files', String(files.length));
  const y1 = rowY(s, 1);
  if (y1 !== null) {
    const r = rect(s.area.x + s.area.w - len(FOLDER_BUTTON), y1, len(FOLDER_BUTTON), 1);
    p.span(r.x, r.y, FOLDER_BUTTON, p.hovered(r) ? ACTION_LIT : ACTION);
    p.region({ r, click: () => click({ folder: true }), cursor: 'pointer' });
  }
  if (!files.length) {
    const y = rowY(s, LEAD);
    if (y !== null) p.span(s.area.x, y, 'nothing yet', DARK);
    return;
  }
  files.forEach((file, i) => {
    const y = rowY(s, LEAD + i);
    if (y === null) return;
    const r = rect(s.area.x, y, s.area.w, 1);
    shade(p, s, LEAD + i, LEAD + i + 1);
    const both = `${file.size} · ${file.age}`;
    const tail = s.area.w - len(both) - 1 >= MIN_FILE_NAME ? both : file.age;
    p.span(s.area.x + s.area.w - len(tail), y, tail, DARK);
    p.span(s.area.x, y, truncateRight(file.name, Math.max(0, s.area.w - len(tail) - 1)), {});
    p.region({ r, click: () => click({ file: i }), cursor: 'pointer' });
  });
}

export function drawFlow(p: Painter, areas: Areas, view: PanelView): void {
  const app = p.app;
  const area = areas.changes;
  drawPanelBorder(p, areas);
  p.g.clear(area);
  const header = rect(area.x + 1, area.y, Math.max(0, area.w - 2), 1);
  drawTitle(p, header, view);
  const close = rect(right(header) - 3, header.y, 3, 1);
  p.span(close.x + 1, close.y, '×', p.hovered(close) ? { fg: 1, add: BOLD } : DARK);
  p.region({ r: close, click: () => app.toggleFlow(), cursor: 'pointer' });
  p.span(header.x, header.y + 1, truncateRight(view.task.split('\n')[0], header.w), DARK, header.w);
  const body = panelBody(area);
  const scroll = scrolled(area, view, 0);
  const t = tops(view, body.w);
  const click = (hit: PanelHit) => app.flowClick(hit);
  const at = (top: number) => slot(body, top, scroll);
  const first = at(0);
  if (first) drawHeading(p, first, view, click);
  const team = at(t.team);
  if (team) drawTeam(p, team, view.team, click);
  const diagram = at(t.diagram);
  if (diagram) drawDiagram(p, diagram.area, view.diagram, diagram.skip, click);
  const history = at(t.history);
  if (history) drawHistory(p, history, view.history, click);
  const files = at(t.files);
  if (files && view.files) drawFiles(p, files, view.files, click);
  p.region({ r: body, wheel: (dy) => app.scrollFlow(Math.sign(dy) * 3) });
  const more = (y: number, text: string) => p.span(right(body) - len(text), y, text, DARK, len(text));
  if (scroll > 0) more(body.y, ' ↑ more ');
  if (t.total - scroll > body.h && body.h > 0) more(bottom(body) - 1, ' ↓ more ');
}
