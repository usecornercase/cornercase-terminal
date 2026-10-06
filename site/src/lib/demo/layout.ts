import { type Rect, contains, rect } from '../term/grid';

export const SIDEBAR_WIDTH = 32;
export const WORKSPACES_WIDTH = 26;
export const MIN_COLUMN_WIDTH = 16;
export const COMPACT_WIDTH = 90;
export const CHANGES_WIDTH = 64;
const MIN_CHANGES_WIDTH = 36;
const PANE_PADDING = 1;
const MIN_PANE_WIDTH = 20;
const COMPACT_PITCH = 3;
const COMPACT_BUTTON_WIDTH = 7;
const HEADER_HEIGHT = 2;
export const GAP = 1;
const FORM_WIDTH = 64;
const FORM_HEIGHT = 10;
const PICKER_WIDTH = 72;
const PICKER_HEIGHT = 24;
const ISSUES_WIDTH = 110;
const ISSUES_HEIGHT = 30;

export const GROUP_ICONS = ['●', '◉', '◐', '◆', '■', '▲', '▼', '★', '✦', '♥', '♣', '♠'];
export const GROUP_COLOURS = [1, 9, 208, 214, 3, 11, 2, 10, 6, 14, 4, 12, 99, 5, 13, 205];
export const DONE = 'done';
const ICONS_PER_ROW = 6;
const COLOURS_PER_ROW = 8;
const ICON_CELL = 3;
const COLOUR_CELL = 4;

export type Nav = 'projects' | 'workspaces' | null;

export type SidebarRow = { kind: 'gap' } | { kind: 'group'; g: number } | { kind: 'project'; p: number } | { kind: 'landing' };

export function sidebarRows(groups: (number | null)[], collapsed: boolean[]): SidebarRow[] {
  const inGroup = (g: number | null): SidebarRow[] =>
    groups.flatMap((group, p) => (group === g ? [{ kind: 'project', p } as SidebarRow] : []));
  const rows = inGroup(null);
  collapsed.forEach((folded, g) => {
    if (rows.length) rows.push({ kind: 'gap' });
    rows.push({ kind: 'group', g });
    if (!folded) rows.push(...inGroup(g));
  });
  return rows;
}

export const sidebarLayout = (list: Rect, pitch: number, rows: SidebarRow[], scroll: number) =>
  new Rows(list, rows.map((s) => (s.kind === 'gap' || s.kind === 'landing' ? GAP : pitch)), pitch, scroll);

export type WorkspaceRow = { kind: 'gap' } | { kind: 'ws'; w: number } | { kind: 'tab'; w: number; t: number } | { kind: 'new'; w: number } | { kind: 'landing' };

export function workspaceRows(tabs: number[][]): WorkspaceRow[] {
  return tabs.flatMap((lines, w) => [
    ...(w > 0 ? [{ kind: 'gap' } as WorkspaceRow] : []),
    { kind: 'ws', w } as WorkspaceRow,
    ...lines.map((_, t) => ({ kind: 'tab', w, t }) as WorkspaceRow),
    { kind: 'new', w } as WorkspaceRow,
  ]);
}

export function workspaceLayout(list: Rect, pitch: number, rows: WorkspaceRow[], tabs: number[][], scroll: number): Rows {
  const height = (r: WorkspaceRow): number => {
    if (r.kind === 'gap' || r.kind === 'landing') return GAP;
    if (r.kind === 'tab') return Math.max(pitch, tabs[r.w][r.t]);
    return pitch;
  };
  return new Rows(list, rows.map(height), pitch, scroll);
}

export function activeRow(rows: SidebarRow[], active: number, group: number | null): number {
  const own = rows.findIndex((r) => r.kind === 'project' && r.p === active);
  return own >= 0 ? own : rows.findIndex((r) => r.kind === 'group' && r.g === group);
}

export interface Widths {
  projects: number;
  workspaces: number;
  changes?: number | null;
  stack?: number | null;
}

export type Border = 'projects' | 'workspaces' | 'stack' | 'changes';
export type Sidebar = 'side_by_side' | 'projects_on_top' | 'workspaces_on_top';
export const SIDEBARS: [Sidebar, string][] = [
  ['side_by_side', 'projects and workspaces in two columns'],
  ['projects_on_top', 'one column, workspaces below projects'],
  ['workspaces_on_top', 'one column, projects below workspaces'],
];
export const MIN_STACK_SECTION = 5;
const STACK_FOOTER = 5;

export function changesWidth(w: Widths, total: number): number {
  const max = Math.max(0, total - (PANE_PADDING + MIN_PANE_WIDTH + 2 * MIN_COLUMN_WIDTH));
  const half = Math.floor(Math.max(0, total - w.projects - w.workspaces - PANE_PADDING) / 2);
  return Math.min(max, Math.max(MIN_CHANGES_WIDTH, w.changes ?? Math.min(half, CHANGES_WIDTH)));
}

export const mainWidth = (w: Widths, total: number, changes: boolean): number => (changes ? total - changesWidth(w, total) : total);

export const EMPTY: Rect = rect(0, 0, 0, 0);
export const isEmpty = (r: Rect) => r.w === 0 || r.h === 0;
export const bottom = (r: Rect) => r.y + r.h;
export const right = (r: Rect) => r.x + r.w;

export function intersect(a: Rect, b: Rect): Rect {
  const x = Math.max(a.x, b.x);
  const y = Math.max(a.y, b.y);
  const w = Math.min(right(a), right(b)) - x;
  const h = Math.min(bottom(a), bottom(b)) - y;
  return w > 0 && h > 0 ? rect(x, y, w, h) : EMPTY;
}

export function fit(w: Widths, total: number): Widths {
  const room = Math.max(0, total - (PANE_PADDING + MIN_PANE_WIDTH));
  const workspaces = Math.max(MIN_COLUMN_WIDTH, Math.min(w.workspaces, room - w.projects));
  const projects = Math.max(MIN_COLUMN_WIDTH, Math.min(w.projects, room - workspaces));
  return { ...w, projects, workspaces };
}

export function dragged(w: Widths, border: Border, x: number, total: number): Widths {
  if (border === 'stack') return w;
  if (border === 'changes') {
    const max = Math.max(0, total - (PANE_PADDING + MIN_PANE_WIDTH + 2 * MIN_COLUMN_WIDTH));
    return { ...w, changes: Math.max(Math.min(MIN_CHANGES_WIDTH, max), Math.min(max, total - x)) };
  }
  const fitted = fit(w, total);
  const room = Math.max(0, total - (PANE_PADDING + MIN_PANE_WIDTH));
  const edge = x + 1;
  if (border === 'projects') {
    const max = Math.max(MIN_COLUMN_WIDTH, room - fitted.workspaces);
    return { ...w, projects: Math.max(MIN_COLUMN_WIDTH, Math.min(max, edge)) };
  }
  const max = Math.max(MIN_COLUMN_WIDTH, room - fitted.projects);
  return { ...w, workspaces: Math.max(MIN_COLUMN_WIDTH, Math.min(max, edge - fitted.projects)) };
}

export interface Areas {
  compact: boolean;
  pitch: number;
  bar: Rect;
  search: Rect;
  searchButton: Rect;
  back: Rect;
  sidebar: Rect;
  title: Rect;
  list: Rect;
  separator: Rect;
  settings: Rect;
  usage: Rect;
  quit: Rect;
  workspaces: Rect;
  workspacesTitle: Rect;
  workspacesList: Rect;
  workspacesSeparator: Rect;
  issues: Rect;
  results: Rect;
  pane: Rect;
  projectsBorder: Rect;
  workspacesBorder: Rect;
  stackBorder: Rect;
  changes: Rect;
  changesBorder: Rect;
  changesButton: Rect;
}

function column(r: Rect, lead: number): [Rect, Rect, Rect, Rect, Rect, Rect] {
  const title = rect(r.x, r.y, r.w, Math.min(lead, r.h));
  const listY = r.y + lead + GAP;
  const listH = Math.max(1, r.h - lead - GAP - 4);
  const list = rect(r.x, listY, r.w, listH);
  const separator = rect(r.x, bottom(list), r.w, 1);
  const [a, b, c] = [1, 2, 3].map((dy) => rect(r.x, bottom(list) + dy, r.w, 1));
  return [title, list, separator, a, b, c];
}

export function layout(cols: number, rows: number, widths: Widths, nav: Nav, changes = false, sidebar: Sidebar = 'side_by_side'): Areas {
  const main = (c: number): Areas => (sidebar === 'side_by_side' ? wide(c, rows, widths) : stacked(c, rows, widths, sidebar));
  const areas = cols < COMPACT_WIDTH ? compact(cols, rows, changes) : changes ? withChanges(cols, rows, widths, main) : main(cols);
  if (!areas.compact) return areas;
  const projects = nav === 'projects' ? areas : { ...areas, sidebar: EMPTY, title: EMPTY, list: EMPTY, separator: EMPTY, settings: EMPTY, usage: EMPTY, quit: EMPTY };
  return nav === 'workspaces'
    ? projects
    : { ...projects, workspaces: EMPTY, workspacesTitle: EMPTY, workspacesList: EMPTY, workspacesSeparator: EMPTY, issues: EMPTY, back: EMPTY };
}

function withChanges(cols: number, rows: number, widths: Widths, main: (cols: number) => Areas): Areas {
  const w = changesWidth(widths, cols);
  const x = cols - w;
  return { ...main(x), changes: rect(x + 1, 0, w - 1, rows), changesBorder: rect(x, 0, Math.min(1, w), rows) };
}

function wide(cols: number, rows: number, widths: Widths): Areas {
  const { projects, workspaces } = fit(widths, cols);
  const columnsWidth = projects + workspaces;
  const pane = rect(columnsWidth + PANE_PADDING, 0, Math.max(1, cols - columnsWidth - PANE_PADDING), rows);
  const header = rect(0, 0, columnsWidth - 1, Math.min(HEADER_HEIGHT, rows));
  const sidebar = rect(0, HEADER_HEIGHT, projects, Math.max(0, rows - HEADER_HEIGHT));
  const [title, list, separator, settings, usage, quit] = column(rect(0, HEADER_HEIGHT, projects - 1, sidebar.h), 1);
  const wsColumn = rect(projects, 0, workspaces, rows);
  const [workspacesTitle, workspacesList, workspacesSeparator, issues] = column(rect(projects, HEADER_HEIGHT, workspaces - 1, sidebar.h), 1);
  return {
    compact: false,
    pitch: 1,
    bar: EMPTY,
    search: rect(1, 0, header.w - 2, 1),
    searchButton: rect(1, 0, header.w - 2, 1),
    back: EMPTY,
    sidebar,
    title,
    list,
    separator,
    settings,
    usage,
    quit,
    workspaces: wsColumn,
    workspacesTitle,
    workspacesList,
    workspacesSeparator,
    issues,
    results: rect(0, HEADER_HEIGHT, columnsWidth - 1, Math.max(0, rows - HEADER_HEIGHT)),
    pane,
    projectsBorder: rect(projects - 1, HEADER_HEIGHT, 1, sidebar.h),
    workspacesBorder: rect(projects + workspaces - 1, 0, 1, rows),
    stackBorder: EMPTY,
    changes: EMPTY,
    changesBorder: EMPTY,
    changesButton: EMPTY,
  };
}

export const stackedWidth = (w: Widths, total: number): number =>
  Math.max(MIN_COLUMN_WIDTH, Math.min(w.projects, Math.max(0, total - (PANE_PADDING + MIN_PANE_WIDTH))));

export function topRows(w: Widths, room: number): number {
  if (room < 2 * MIN_STACK_SECTION) return Math.floor(room / 2);
  return Math.max(MIN_STACK_SECTION, Math.min(room - MIN_STACK_SECTION, w.stack ?? Math.floor(room / 2)));
}

const stackRoom = (rows: number): number => Math.max(0, rows - HEADER_HEIGHT - STACK_FOOTER - 1);

export function draggedStacked(w: Widths, border: Border, x: number, y: number, total: number, rows: number): Widths {
  if (border === 'projects') {
    const max = Math.max(MIN_COLUMN_WIDTH, total - (PANE_PADDING + MIN_PANE_WIDTH));
    return { ...w, projects: Math.max(MIN_COLUMN_WIDTH, Math.min(max, x + 1)) };
  }
  if (border === 'stack') return { ...w, stack: topRows({ ...w, stack: Math.max(0, y - HEADER_HEIGHT) }, stackRoom(rows)) };
  return w;
}

function section(r: Rect): [Rect, Rect] {
  return [rect(r.x, r.y, r.w, Math.min(1, r.h)), rect(r.x, r.y + 1 + GAP, r.w, Math.max(0, r.h - 1 - GAP))];
}

function stacked(cols: number, rows: number, widths: Widths, sidebar: Sidebar): Areas {
  const width = stackedWidth(widths, cols);
  const inner = width - 1;
  const room = stackRoom(rows);
  const topH = topRows(widths, room);
  const top = rect(0, HEADER_HEIGHT, inner, topH);
  const line = rect(0, HEADER_HEIGHT + topH, inner, 1);
  const under = rect(0, bottom(line), inner, room - topH);
  const [projects, workspaces] = sidebar === 'workspaces_on_top' ? [under, top] : [top, under];
  const [title, list] = section(projects);
  const [workspacesTitle, workspacesList] = section(workspaces);
  const footer = bottom(under);
  return {
    compact: false,
    pitch: 1,
    bar: EMPTY,
    search: rect(1, 0, inner - 2, 1),
    searchButton: rect(1, 0, inner - 2, 1),
    back: EMPTY,
    sidebar: { ...projects, w: width },
    title,
    list,
    separator: rect(0, footer, inner, 1),
    settings: rect(0, footer + 2, inner, 1),
    usage: rect(0, footer + 3, inner, 1),
    quit: rect(0, footer + 4, inner, 1),
    workspaces: { ...workspaces, w: width },
    workspacesTitle,
    workspacesList,
    workspacesSeparator: EMPTY,
    issues: rect(0, footer + 1, inner, 1),
    results: rect(0, HEADER_HEIGHT, inner, Math.max(0, rows - HEADER_HEIGHT)),
    pane: rect(width + PANE_PADDING, 0, Math.max(1, cols - width - PANE_PADDING), rows),
    projectsBorder: rect(width - 1, 0, 1, rows),
    workspacesBorder: EMPTY,
    stackBorder: line,
    changes: EMPTY,
    changesBorder: EMPTY,
    changesButton: EMPTY,
  };
}

function compact(cols: number, rows: number, changes: boolean): Areas {
  const pitch = COMPACT_PITCH;
  const bar = rect(0, 0, cols, pitch);
  const below = rect(0, pitch, cols, Math.max(0, rows - pitch));
  const searchWidth = Math.min(COMPACT_BUTTON_WIDTH, cols);
  const menu = rect(0, pitch + GAP, cols, Math.max(0, rows - pitch - GAP));
  const title = rect(0, menu.y, cols, pitch);
  const listY = menu.y + pitch + GAP;
  const list = rect(0, listY, cols, Math.max(1, bottom(menu) - listY - 1 - pitch));
  const separator = rect(0, bottom(list), cols, 1);
  const footer = rect(0, bottom(list) + 1, cols, pitch);
  const [first, second] = [Math.round(cols / 3), Math.round((2 * cols) / 3)];
  return {
    compact: true,
    pitch,
    bar,
    search: bar,
    searchButton: rect(cols - searchWidth, 0, searchWidth, pitch),
    back: intersect(rect(0, title.y, '‹ projects'.length + 2 + 2, pitch), title),
    sidebar: below,
    title,
    list,
    separator,
    settings: rect(0, footer.y, first, pitch),
    usage: rect(first, footer.y, second - first, pitch),
    quit: rect(second, footer.y, cols - second, pitch),
    workspaces: below,
    workspacesTitle: title,
    workspacesList: list,
    workspacesSeparator: separator,
    issues: footer,
    results: below,
    pane: below,
    projectsBorder: EMPTY,
    workspacesBorder: EMPTY,
    stackBorder: EMPTY,
    changes: changes ? below : EMPTY,
    changesBorder: EMPTY,
    changesButton: rect(Math.max(0, cols - searchWidth - COMPACT_BUTTON_WIDTH), 0, Math.min(COMPACT_BUTTON_WIDTH, Math.max(0, cols - searchWidth)), pitch),
  };
}

export class Rows {
  constructor(
    readonly list: Rect,
    readonly heights: number[],
    readonly button: number,
    readonly scroll: number,
  ) {}

  private room(): number {
    return Math.max(0, this.list.h - GAP - this.button);
  }

  private height(a: number, b: number): number {
    let n = 0;
    for (let i = a; i < b; i++) n += this.heights[i];
    return n;
  }

  private fittingBefore(end: number): number {
    const room = this.room();
    let start = end;
    while (start > 0 && this.height(start - 1, end) <= room) start -= 1;
    return start;
  }

  first(): number {
    return Math.min(this.scroll, this.fittingBefore(this.heights.length));
  }

  end(): number {
    const first = this.first();
    const room = this.room();
    let end = first;
    for (let i = first; i < this.heights.length; i++) {
      if (this.height(first, i + 1) <= room) end = i + 1;
      else break;
    }
    return end;
  }

  item(i: number): Rect {
    const first = this.first();
    if (i < first || i >= this.end()) return EMPTY;
    return rect(this.list.x, this.list.y + this.height(first, i), this.list.w, this.heights[i]);
  }

  hidden(): [number, number] {
    return [this.first(), this.heights.length - this.end()];
  }

  scrolled(delta: number): number {
    const max = this.fittingBefore(this.heights.length);
    return Math.max(0, Math.min(max, Math.min(this.scroll, max) + delta));
  }

  reveal(i: number): number {
    const first = this.first();
    if (i >= this.heights.length || (i >= first && i < this.end())) return first;
    if (i < first) return i;
    return Math.min(this.fittingBefore(i + 1), i);
  }

  buttonRect(): Rect {
    const gap = this.heights.length ? GAP : 0;
    const below = this.list.y + this.height(0, this.heights.length) + gap;
    const last = bottom(this.list) - this.button;
    return rect(this.list.x, Math.min(below, last), this.list.w, this.button);
  }

  moreBelow(): Rect {
    return intersect(rect(this.list.x, this.list.y + this.room(), this.list.w, 1), this.list);
  }

  at(x: number, y: number): number | null {
    for (let i = this.first(); i < this.end(); i++) if (contains(this.item(i), x, y)) return i;
    return null;
  }

  edge(x: number, y: number): number {
    const [above, below] = this.hidden();
    const top = this.list.y + this.room();
    if (above && contains(moreAbove(this.list), x, y)) return -1;
    if (below && contains(rect(this.list.x, top, this.list.w, bottom(this.list) - top), x, y)) return 1;
    return 0;
  }

  boundary(x: number, y: number, onRow: (i: number) => number | null): number | null {
    if (contains(moreAbove(this.list), x, y)) return this.first();
    if (!contains(this.list, x, y)) return null;
    const i = this.at(x, y);
    return i === null ? this.end() : onRow(i);
  }
}

export type Spot =
  | { kind: 'group'; before: number }
  | { kind: 'project'; group: number | null; before: number | null }
  | { kind: 'workspace'; before: number }
  | { kind: 'tab'; before: number };

export interface Landing {
  at: number;
  spot: Spot;
}

export const landingIndent = (spot: Spot): number => ((spot.kind === 'project' && spot.group !== null) || spot.kind === 'tab' ? 4 : 2);

function groupOf(rows: SidebarRow[], i: number): number | null {
  for (let j = i; j >= 0; j--) {
    const r = rows[j];
    if (r.kind === 'group') return r.g;
  }
  return null;
}

function projectSpot(rows: SidebarRow[], dragged: number, at: number): Spot {
  const below = at === dragged ? at + 1 : at;
  const next = rows[below];
  if (next?.kind === 'project') return { kind: 'project', group: groupOf(rows, below), before: next.p };
  const k = at - 1 === dragged ? at - 2 : at - 1;
  const group = k < 0 ? null : rows[k].kind === 'gap' ? groupOf(rows, k - 1) : groupOf(rows, k);
  return { kind: 'project', group, before: null };
}

function blockDrop<R>(rows: R[], layout: Rows, x: number, y: number, dragged: number, gap: (r: R) => boolean, header: (r: R) => number | null): [number, number] | null {
  const len = rows.length;
  const block = (i: number): number | null => {
    for (let j = i; j >= 0 && !gap(rows[j]); j--) if (header(rows[j]) !== null) return j;
    return null;
  };
  const after = (from: number, want: (r: R) => boolean): number => {
    for (let j = from; j < len; j++) if (want(rows[j])) return j;
    return len;
  };
  const at = layout.boundary(x, y, (i) => {
    const h = block(i);
    if (h === dragged) return null;
    if (h === null) return after(i, (r) => header(r) !== null);
    return h < dragged ? h : after(h, gap);
  });
  if (at === null) return null;
  const next = rows.slice(at).find((r) => header(r) !== null);
  return [at, next === undefined ? rows.filter((r) => header(r) !== null).length : (header(next) ?? 0)];
}

export function sidebarDrop(list: Rect, pitch: number, rows: SidebarRow[], scroll: number, dragged: SidebarRow, x: number, y: number): Landing | null {
  const layout = sidebarLayout(list, pitch, rows, scroll);
  const d = rows.findIndex((r) => sameRow(r, dragged));
  if (d < 0) return null;
  if (dragged.kind === 'group') {
    const found = blockDrop(rows, layout, x, y, d, (r) => r.kind === 'gap', (r) => (r.kind === 'group' ? r.g : null));
    return found && { at: found[0], spot: { kind: 'group', before: found[1] } };
  }
  const at = layout.boundary(x, y, (i) => {
    const r = rows[i];
    if (i === d) return null;
    if (r.kind === 'project') return i < d ? i : i + 1;
    return r.kind === 'group' ? i + 1 : i;
  });
  return at === null ? null : { at, spot: projectSpot(rows, d, at) };
}

export function workspaceDrop(list: Rect, pitch: number, tabs: number[][], scroll: number, dragged: WorkspaceRow, x: number, y: number): Landing | null {
  const rows = workspaceRows(tabs);
  const layout = workspaceLayout(list, pitch, rows, tabs, scroll);
  if (dragged.kind === 'ws') {
    const d = rows.findIndex((r) => sameRow(r, dragged));
    if (d < 0) return null;
    const found = blockDrop(rows, layout, x, y, d, (r) => r.kind === 'gap', (r) => (r.kind === 'ws' ? r.w : null));
    return found && { at: found[0], spot: { kind: 'workspace', before: found[1] } };
  }
  if (dragged.kind !== 'tab') return null;
  const header = rows.findIndex((r) => r.kind === 'ws' && r.w === dragged.w);
  const newTab = rows.findIndex((r) => r.kind === 'new' && r.w === dragged.w);
  const at = layout.boundary(x, y, (i) => {
    const r = rows[i];
    if (r.kind === 'tab' && r.w === dragged.w) return r.t === dragged.t ? null : r.t < dragged.t ? i : i + 1;
    return i <= header ? header + 1 : newTab;
  });
  if (at === null) return null;
  const clamped = Math.max(header + 1, Math.min(newTab, at));
  return { at: clamped, spot: { kind: 'tab', before: clamped - header - 1 } };
}

export const sameRow = <R extends { kind: string }>(a: R, b: R): boolean => JSON.stringify(a) === JSON.stringify(b);

export function landed<R extends { kind: string }>(rows: R[], layout: Rows, landing: Landing | null, mark: R): [R[], Rect] {
  const first = layout.first();
  const end = layout.end();
  const at = landing?.at;
  if (at === undefined || at < first || at > end) return [rows, EMPTY];
  const drawn = [...rows];
  const gap = [at - 1 >= first ? at - 1 : -1, at < end ? at : -1].find((g) => g >= 0 && rows[g].kind === 'gap');
  if (gap !== undefined) {
    drawn[gap] = mark;
    return [drawn, EMPTY];
  }
  if (at === first) return [drawn, moreAbove(layout.list)];
  if (at === end) {
    const last = layout.item(end - 1);
    return [drawn, rect(last.x, bottom(last), last.w, 1)];
  }
  drawn.splice(at, 0, mark);
  return [drawn, EMPTY];
}

export const moreAbove = (list: Rect): Rect => (list.y < GAP ? EMPTY : rect(list.x, list.y - GAP, list.w, 1));

export const closeButton = (row: Rect, pitch: number): Rect => {
  const w = pitch > 1 ? 5 : 3;
  return rect(right(row) - w, row.y, Math.min(w, row.w), Math.min(pitch, row.h));
};

export interface Details {
  model: string | null;
  percent: number | null;
  memory: number | null;
}

export const tabLines = (d: Details): number => (d.model !== null || d.percent !== null || d.memory !== null ? 2 : 1);

export function centered(cols: number, rows: number, w: number, h: number): Rect {
  return rect(Math.floor((cols - w) / 2), Math.floor((rows - h) / 2), w, h);
}

export const formArea = (cols: number, rows: number) => centered(cols, rows, Math.min(Math.max(0, cols - 4), FORM_WIDTH), Math.min(FORM_HEIGHT, rows));
export const pickerArea = (cols: number, rows: number) =>
  centered(cols, rows, Math.min(Math.max(0, cols - 4), PICKER_WIDTH), Math.min(Math.max(0, rows - 2), PICKER_HEIGHT));
export const issuesArea = (cols: number, rows: number) =>
  centered(cols, rows, Math.min(Math.max(0, cols - 4), ISSUES_WIDTH), Math.min(Math.max(0, rows - 2), ISSUES_HEIGHT));

export const usageArea = (cols: number, rows: number, body: number) =>
  centered(cols, rows, Math.min(Math.max(0, cols - 4), FORM_WIDTH), Math.min(body + 3, rows));

export const usageDone = (r: Rect): Rect => {
  const row = rect(r.x + 2, bottom(r) - 2, Math.max(0, r.w - 4), 1);
  const w = Math.min(buttonWidth(DONE), row.w);
  return rect(right(row) - w, row.y, w, 1);
};

export const inner = (r: Rect): Rect => rect(r.x + 2, r.y + 1, Math.max(0, r.w - 4), Math.max(0, r.h - 2));

export function rightAligned(row: Rect, labels: string[], width: (l: string) => number, gap: number): Rect[] {
  let x = right(row);
  const rects = [...labels].reverse().map((label) => {
    x -= width(label);
    const r = intersect(rect(x, row.y, width(label), 1), row);
    x -= gap;
    return r;
  });
  return rects.reverse();
}

export const buttonWidth = (label: string) => [...label].length + 2;

export function tabsIn(row: Rect, names: string[]): Rect[] {
  let x = row.x;
  return names.map((name) => {
    const r = intersect(rect(x, row.y, buttonWidth(name), 1), row);
    x += buttonWidth(name) + 1;
    return r;
  });
}

export function menuArea(cols: number, rows: number, at: { x: number; y: number }, items: string[]): Rect {
  const longest = Math.max(0, ...items.map((i) => [...i].length));
  const w = Math.min(longest + 4, cols);
  const h = Math.min(items.length + 2, rows);
  return rect(Math.min(at.x, cols - w), Math.min(at.y + 1, rows - h), w, h);
}

function styleRows(cols: number, rows: number): [Rect, Rect, Rect, Rect, Rect] {
  const c = inner(formArea(cols, rows));
  const at = (dy: number, h: number) => intersect(rect(c.x, c.y + dy, c.w, h), c);
  return [at(0, 1), at(1, 2), at(3, 1), at(4, 2), intersect(rect(c.x, bottom(c) - 1, c.w, 1), c)];
}

function gridCell(grid: Rect, perRow: number, width: number, i: number): Rect {
  return intersect(rect(grid.x + (i % perRow) * width, grid.y + Math.floor(i / perRow), width, 1), grid);
}

export const styleLabels = (cols: number, rows: number): [Rect, Rect, Rect] => {
  const [icon, , colour, , last] = styleRows(cols, rows);
  return [icon, colour, last];
};
export const styleIcon = (cols: number, rows: number, i: number) => gridCell(styleRows(cols, rows)[1], ICONS_PER_ROW, ICON_CELL, i);
export const styleColour = (cols: number, rows: number, i: number) => gridCell(styleRows(cols, rows)[3], COLOURS_PER_ROW, COLOUR_CELL, i);

export const styleDone = (cols: number, rows: number): Rect => rightAligned(styleRows(cols, rows)[4], [DONE], buttonWidth, 1)[0];
