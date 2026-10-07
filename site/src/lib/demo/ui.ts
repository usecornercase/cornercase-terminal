import type { Cursor } from '../term/canvas';
import { BOLD, DIM, INVERSE, type Grid, type Rect, type Style, UNDERLINE, contains, rect } from '../term/grid';
import type { AgentRow, App } from './app';
import { changesLabel, drawChanges, hasChanges } from './changes';
import { drawFiles } from './files';
import { drawTodo } from './todo';
import {
  type Areas,
  type Border,
  activeRow,
  AGENTS_LABEL,
  agentLayout,
  GROUP_COLOURS,
  GROUP_ICONS,
  type Rows,
  type SidebarRow,
  type TreeRow,
  type WorkspaceRow,
  arrowIn,
  DONE,
  bottom,
  buttonWidth,
  closeButton,
  formArea,
  inner,
  intersect,
  isEmpty,
  issuesArea,
  landed,
  landingIndent,
  menuArea,
  moreAbove,
  pickerArea,
  right,
  rightAligned,
  sameRow,
  sidebarLayout,
  styleColour,
  styleDone,
  styleIcon,
  styleLabels,
  type Details,
  tabLines,
  tabsIn,
  treeActiveRow,
  treeDepth,
  treeIndent,
  treeLandingIndent,
  treeLayout,
  treeRows,
  usageArea,
  usageDone,
  workspaceLayout,
  workspaceRows,
  TODO_LABEL,
  FILES_LABEL,
} from './layout';
import { USAGE, type UsageWindow } from './data';
import { type ConfirmView, type Group, type IssuesOverlay, type Pane, type SettingsOverlay, type Status, type Tab, type Target, attention, projectAttention, projectLabel, tabLabel, tabStatus, workspaceLabel } from './model';
import { type Divider, type PanePlace, dividers, grab, hasRoom, visible } from './split';
import { type Line, type Seg, drawLine, seg, truncateLeft, truncateRight, width, wrapAll } from './text';

export type Drag = { kind: 'border'; border: Border } | { kind: 'divider'; tab: Tab; divider: Divider; area: Rect };

export interface Region {
  r: Rect;
  click?: (x: number, y: number) => void;
  right?: (x: number, y: number) => void;
  double?: () => void;
  drag?: Drag;
  grab?: Target;
  hold?: boolean;
  wheel?: (dy: number) => boolean;
  cursor?: string;
  pane?: { pane: Pane; rect: Rect; tab: Tab; alone: boolean };
}

export interface Frame {
  regions: Region[];
  cursor: Cursor | null;
  areas: Areas | null;
}

interface Band {
  r: Rect;
  pitch: number;
  lead: Seg[];
  bg: Style;
}

const DARK: Style = { fg: 8 };
const CYAN: Style = { fg: 6 };
const PRESSED: Style = { fg: 0, bg: 6, add: BOLD };
const STATUS_ICONS: Record<Status, Seg> = {
  idle: seg('○', { fg: 8 }),
  working: seg('◐', { fg: 3 }),
  done: seg('✓', { fg: 2, add: BOLD }),
  waiting: seg('!', { fg: 208, add: BOLD }),
};
const BRAND = 99;
const UNDO = 'undo';
const SEVERITY: Record<UsageWindow['severity'], number> = { normal: 2, warning: 208, critical: 1 };
const severityOf = (percent: number): UsageWindow['severity'] => (percent >= 90 ? 'critical' : percent >= 75 ? 'warning' : 'normal');
const USAGE_BAR = '━';
const SEARCH_PLACEHOLDER = 'search projects, workspaces, tabs';
const NO_TAB = 'no tab open';
const AGENTS_TITLE = 'agents';
const NO_AGENTS = 'no agents running';
const MIN_WORKSPACE_WIDTH = 4;
const MOVE_LABEL = 'move here';
const SWAP_LABEL = 'swap';
const NO_TAB_HINT = ' opens a shell here';
const TAGLINE = 'every agent in its own corner';
const WELCOME_HINT = ' opens a folder';
const LOGO: [string, string][] = [
  ['   █████████', ''],
  ['▄▄▄▀▀▀▀▀▀▀▀▀', ''],
  ['███', ''],
  ['███   ', '██'],
  ['███   ', '██'],
  ['███   ', '▀▀'],
];
const CONTEXT_SEPARATOR = ' · ';
const MIN_MODEL_WIDTH = 4;
const MB = 1 << 20;
const GB = 1 << 30;

const memorySize = (bytes: number): string => {
  const megabytes = Math.round(bytes / MB);
  return megabytes < 1024 ? `${megabytes} MB` : `${(Math.round((bytes * 10) / GB) / 10).toFixed(1)} GB`;
};

function updated(ms: number): string {
  const minutes = Math.floor(ms / 60_000);
  if (minutes < 1) return 'updated just now';
  if (minutes < 60) return `updated ${minutes}m ago`;
  return `updated ${Math.floor(minutes / 60)}h ago`;
}
const INPUT_PROMPT = '› ';

const middle = (r: Rect): Rect => rect(r.x, r.y + Math.floor(Math.max(0, r.h - 1) / 2), r.w, Math.min(r.h, 1));
const marker = (shown: boolean): Seg => seg(shown ? '▌ ' : '  ', CYAN);
const rail = (selected: boolean, bg: Style): Seg => (selected ? seg('▌', { ...bg, fg: 6 }) : seg(' ', bg));
const wordmark = (): Line => [seg('c', { fg: BRAND, add: BOLD }), seg('ornercase', { add: BOLD })];
type Guide = 'top' | 'tee' | 'end' | 'bar';
const arrow = (folded: boolean): Seg => seg(folded ? '▸ ' : '▾ ', DARK);

export class Painter {
  readonly regions: Region[] = [];
  cursor: Cursor | null = null;
  private surface: number;
  private hoverSurface: number;
  readonly lineColour: number;

  constructor(
    readonly app: App,
    readonly g: Grid,
  ) {
    this.surface = app.light ? 254 : 236;
    this.hoverSurface = app.light ? 255 : 235;
    this.lineColour = app.light ? 250 : 239;
  }

  hovered(r: Rect): boolean {
    const h = this.app.hover;
    return !!h && !isEmpty(r) && contains(r, h.x, h.y);
  }

  sidebarHovered(r: Rect): boolean {
    return !this.app.overlay && !this.app.rowDrag?.moved && this.hovered(r);
  }

  private landing(r: Rect, indent: number | null): void {
    if (indent === null || isEmpty(r)) return;
    this.g.clear(rect(r.x, r.y, r.w, 1));
    this.span(r.x + indent, r.y, '─'.repeat(Math.max(0, r.w - indent - 1)), CYAN, Math.max(0, r.w - indent));
  }

  region(region: Region): void {
    if (!isEmpty(region.r)) this.regions.push(region);
  }

  span(x: number, y: number, text: string, s: Style, max = Infinity): number {
    return this.g.text(x, y, text, s, max);
  }

  line(r: Rect, line: Line, base?: Style): void {
    if (base) this.g.fill(r, base);
    drawLine(this.g, r.x, r.y, line.map((s) => ({ t: s.t, s: { ...base, ...s.s, add: (base?.add ?? 0) | (s.s?.add ?? 0) } })), r.w);
  }

  band(r: Rect, line: Line, base: Style): void {
    this.g.fill(r, base);
    this.line(middle(r), line, base);
  }

  button(r: Rect, indent: string, label: string, style: Style): void {
    if (r.h > 1 && style.bg !== undefined) this.g.fill(r, { bg: style.bg });
    const m = middle(r);
    const x = this.span(m.x, m.y, indent, {}, m.w);
    this.span(x, m.y, ` ${label} `, style, Math.max(0, right(m) - x));
  }

  rowBackground(r: Rect, active: boolean): Style {
    if (active) return { bg: this.surface };
    return !this.app.dragging && this.sidebarHovered(r) ? { bg: this.hoverSurface } : {};
  }

  buttonStyle(r: Rect, idle: Style, hoverBg: number): Style {
    return this.sidebarHovered(r) ? { fg: 0, bg: hoverBg, add: BOLD } : idle;
  }

  private pickBackground(selected: boolean, r: Rect): Style {
    if (selected) return { bg: this.surface };
    return this.hovered(r) ? { bg: this.hoverSurface } : {};
  }

  private guide(x: number, r: Rect, kind: Guide): void {
    if (x >= right(r)) return;
    const mid = middle(r).y;
    for (let y = r.y; y < bottom(r); y++) {
      let ch = '';
      if (kind === 'bar' || (y < mid && (kind === 'tee' || kind === 'end')) || (y > mid && (kind === 'top' || kind === 'tee'))) ch = '│';
      else if (y === mid && kind === 'tee') ch = '├';
      else if (y === mid && kind === 'end') ch = '└';
      if (ch && this.g.at(x, y)?.ch === ' ') this.g.put(x, y, ch, { fg: this.lineColour });
    }
  }

  private rail(r: Rect): void {
    for (let y = r.y; y < bottom(r); y++) if (this.g.at(r.x, y)?.ch === ' ') this.g.put(r.x, y, '▌', { fg: 6 });
  }

  private dim(): void {
    this.g.fill(rect(0, 0, this.app.cols, this.app.rows), { add: DIM });
  }

  private centered(area: Rect, lines: Line[]): void {
    const h = lines.length;
    const w = Math.max(0, ...lines.map(width));
    if (h > area.h || w > area.w) return;
    const y = area.y + Math.floor((area.h - h) / 2);
    lines.forEach((line, i) => drawLine(this.g, area.x + Math.floor((area.w - width(line)) / 2), y + i, line, area.w));
  }

  private welcome(area: Rect): void {
    const words: Line[] = [
      wordmark(),
      [seg(TAGLINE, DARK)],
      [],
      [seg('+ new project', CYAN), seg(WELCOME_HINT, DARK)],
    ];
    const size = Math.max(...LOGO.map(([c, k]) => [...c].length + [...k].length));
    const logo: Line[] = LOGO.map(([c, k]) => [seg(c, { fg: BRAND }), seg(k, CYAN), seg(' '.repeat(size - [...c].length - [...k].length))]);
    const all = [...logo, [], ...words];
    this.centered(area, all.length <= area.h ? all : words);
  }

  draw(): Frame {
    const app = this.app;
    if (app.detached) {
      this.outer();
      return { regions: this.regions, cursor: this.cursor, areas: null };
    }
    const areas = app.areas();
    this.pane(areas);
    if (areas.compact) {
      this.bar(areas);
      if (app.nav) this.g.clear(areas.pane);
    } else {
      this.borders(areas);
      this.searchBar(areas.search);
    }
    if (areas.tree) this.tree(areas);
    else if (!isEmpty(areas.sidebar)) this.sidebar(areas);
    if (app.config.agentsSection) this.agents(areas);
    if ([areas.separator, areas.settings, areas.usage, areas.quit].some((r) => !isEmpty(r))) this.footer(areas);
    if (!isEmpty(areas.workspaces)) this.workspaces(areas);
    if (app.project() && [areas.workspacesSeparator, areas.issues].some((r) => !isEmpty(r))) this.issuesRow(areas);
    if (app.changesShown() && !isEmpty(areas.changes)) drawChanges(this, areas);
    else if (app.todo.open && !isEmpty(areas.changes)) drawTodo(this, areas);
    else if (app.filesShown() && !isEmpty(areas.changes)) drawFiles(this, areas);
    this.overlay(areas);
    if (app.toast) this.toast(app.toast.text, app.toast.status, app.toast.undo);
    return { regions: this.regions, cursor: this.cursor, areas };
  }

  private outer(): void {
    const g = this.g;
    const lines: Line[] = [
      [seg('~', { fg: 6, add: BOLD }), seg(' '), seg('❯', { fg: 5 }), seg(' cornercase')],
      ...this.app.outerLines,
      [seg('~', { fg: 6, add: BOLD }), seg(' '), seg('❯', { fg: 5 }), seg(' '), seg(this.app.outerInput)],
    ];
    const rows = wrapAll(lines, g.cols);
    rows.forEach((row, y) => row.forEach((c, x) => g.put(x, y, c.ch, c.s ?? {})));
    const last = rows.length - 1;
    this.cursor = { x: rows[last].length, y: last, shape: 'block' };
    this.region({ r: rect(0, 0, g.cols, g.rows), click: () => this.app.reattach() , cursor: 'pointer' });
  }

  private pane(areas: Areas): void {
    const app = this.app;
    const tab = app.tab();
    if (tab) this.tab(tab, areas.pane);
    else if (app.project()) this.centered(areas.pane, [[seg(NO_TAB, { fg: 8, add: BOLD })], [seg('+ tab', CYAN), seg(NO_TAB_HINT, DARK)]]);
    else if (!app.projects.length) this.welcome(areas.pane);
  }

  private tab(tab: Tab, area: Rect): void {
    const app = this.app;
    const list = visible(tab.layout, area, tab.active);
    const split = list.length > 1;
    const alone = !hasRoom(tab.layout, area);
    for (const [id, r] of list) {
      const pane = tab.panes.find((p) => p.id === id);
      if (!pane) continue;
      const active = id === tab.active;
      const cursor = pane.shell.draw(this.g, r, app.focused && active && !app.overlay);
      if (active && !app.overlay && cursor) this.cursor = cursor;
      if (split && !active && app.config.dim) this.g.fill(r, { add: DIM });
      const link = active ? app.hoveredLink(pane, r, this.g) : null;
      if (link) for (let x = link.start; x < link.end; x++) this.g.style(x, link.y, { fg: 6, add: UNDERLINE });
      const sel = app.selection;
      if (sel && sel.pane === id) {
        for (const [x, y] of app.selectedCells(r)) this.g.style(x, y, { add: INVERSE });
      }
      this.region({ r, pane: { pane, rect: r, tab, alone }, cursor: pane.shell.mouse ? 'default' : 'text' });
      if (link) this.region({ r: rect(link.start, link.y, link.end - link.start, 1), cursor: 'pointer' });
    }
    const landing = app.paneLanding();
    if (landing) this.paneLanding(landing.area, landing.place);
    this.dividers(tab, area);
  }

  private paneLanding(r: Rect, place: PanePlace): void {
    this.g.fill(r, { bg: this.surface });
    const label = ` ${place === 'swap' ? SWAP_LABEL : MOVE_LABEL} `;
    if (label.length > r.w || r.h === 0) return;
    this.span(r.x + Math.floor((r.w - label.length) / 2), r.y + Math.floor(r.h / 2), label, { fg: 0, bg: 6, add: BOLD });
  }

  private dividers(tab: Tab, area: Rect): void {
    const UP = 1;
    const DOWN = 2;
    const LEFT = 4;
    const RIGHT = 8;
    const list = dividers(tab.layout, area);
    const cells = new Map<string, { x: number; y: number; links: number; lit: boolean }>();
    const key = (x: number, y: number) => `${x},${y}`;
    const dragging = this.app.dragging?.kind === 'divider' ? this.app.dragging.divider : null;
    for (const d of list) {
      const lit = (dragging && dragging.path.join() === d.path.join() && dragging.dir === d.dir) || (!dragging && this.sidebarHovered(grab(d)));
      const links = d.dir === 'right' ? UP | DOWN : LEFT | RIGHT;
      for (let y = d.line.y; y < bottom(d.line); y++) {
        for (let x = d.line.x; x < right(d.line); x++) {
          const c = cells.get(key(x, y)) ?? { x, y, links: 0, lit: false };
          c.links |= links;
          c.lit ||= !!lit;
          cells.set(key(x, y), c);
        }
      }
    }
    for (const d of list.filter((d) => d.dir === 'down')) {
      const pad = d.line.x - 1;
      const bar = pad - 1;
      if (pad < 0 || bar < 0) continue;
      const joins = !cells.has(key(pad, d.line.y)) && ((cells.get(key(bar, d.line.y))?.links ?? 0) & UP) !== 0;
      if (joins) {
        cells.set(key(pad, d.line.y), { x: pad, y: d.line.y, links: LEFT | RIGHT, lit: false });
        const b = cells.get(key(bar, d.line.y));
        if (b) b.links |= RIGHT;
      }
    }
    for (const d of list) {
      const ends: [number, number, number][] =
        d.dir === 'right'
          ? [
              [d.line.x, d.line.y - 1, DOWN],
              [d.line.x, bottom(d.line), UP],
            ]
          : [
              [d.line.x - 1, d.line.y, RIGHT],
              [right(d.line), d.line.y, LEFT],
            ];
      for (const [x, y, link] of ends) {
        const c = cells.get(key(x, y));
        if (c) c.links |= link;
      }
    }
    const symbol = (l: number) => {
      if (l === (UP | DOWN | LEFT | RIGHT)) return '┼';
      if (l === (UP | DOWN | RIGHT)) return '├';
      if (l === (UP | DOWN | LEFT)) return '┤';
      if (l === (LEFT | RIGHT | DOWN)) return '┬';
      if (l === (LEFT | RIGHT | UP)) return '┴';
      if (l & (UP | DOWN)) return '│';
      return '─';
    };
    for (const c of cells.values()) this.g.put(c.x, c.y, symbol(c.links), { fg: c.lit ? 6 : this.lineColour, bg: -1, sub: DIM | INVERSE });
    for (const d of list) {
      this.region({ r: grab(d), drag: { kind: 'divider', tab, divider: d, area: d.area }, cursor: d.dir === 'right' ? 'col-resize' : 'row-resize' });
    }
  }

  private borders(areas: Areas): void {
    const app = this.app;
    const lit = (border: Border, r: Rect) => (app.dragging?.kind === 'border' && app.dragging.border === border) || this.sidebarHovered(r);
    for (const [border, r] of [
      ['projects', areas.projectsBorder],
      ['workspaces', areas.workspacesBorder],
    ] as const) {
      for (let y = r.y; y < bottom(r); y++) this.g.put(r.x, y, '│', { fg: lit(border, r) ? 6 : this.lineColour });
      this.region({ r, drag: { kind: 'border', border }, double: () => app.resetBorder(border), cursor: 'col-resize' });
    }
    const line = areas.stackBorder;
    if (isEmpty(line)) return;
    this.line(line, [seg(` ${'─'.repeat(Math.max(0, line.w - 2))}`, { fg: lit('stack', line) ? 6 : this.lineColour })]);
    this.region({ r: line, drag: { kind: 'border', border: 'stack' }, double: () => app.resetBorder('stack'), cursor: 'row-resize' });
  }


  private searchBar(r: Rect): void {
    const app = this.app;
    const search = app.overlay?.kind === 'search' ? app.overlay : null;
    this.g.fill(r, { bg: this.surface });
    if (search) {
      const max = Math.max(0, r.w - 4);
      const query = truncateLeft(search.query, max);
      const x = this.span(r.x, r.y, ' ⌕ ', { fg: 6, bg: this.surface });
      this.span(x, r.y, query, { add: BOLD, bg: this.surface });
      if (x + [...query].length < right(r)) this.cursor = { x: x + [...query].length, y: r.y, shape: 'block' };
      return;
    }
    const icon = this.sidebarHovered(r) ? CYAN : DARK;
    const x = this.span(r.x, r.y, ' ⌕ ', { ...icon, bg: this.surface });
    this.span(x, r.y, truncateRight(SEARCH_PLACEHOLDER, Math.max(0, r.w - 4)), { fg: 8, bg: this.surface }, right(r) - x);
    this.region({ r, click: () => app.openSearch(), cursor: 'text' });
  }

  private bar(areas: Areas): void {
    const app = this.app;
    const r = areas.bar;
    this.g.fill(r, { bg: this.surface });
    if (app.overlay?.kind === 'search') {
      this.searchBar(middle(r));
      return;
    }
    const showChanges = hasChanges(app.workspace());
    const showFiles = !!app.project();
    const menu = rect(r.x, r.y, r.w - areas.searchButton.w - areas.todoButton.w - (showFiles ? areas.filesButton.w : 0) - (showChanges ? areas.changesButton.w : 0), r.h);
    const todo = areas.todoButton;
    this.band(todo, [seg(centered('☐', todo.w))], app.todo.open || this.sidebarHovered(todo) ? PRESSED : { fg: 8, bg: this.surface });
    this.region({ r: todo, click: () => app.toggleTodo(), cursor: 'pointer' });
    if (showFiles) {
      const f = areas.filesButton;
      this.band(f, [seg(centered('▤', f.w))], app.filesShown() || this.sidebarHovered(f) ? PRESSED : { fg: 8, bg: this.surface });
      this.region({ r: f, click: () => app.toggleFiles(), cursor: 'pointer' });
    }
    if (showChanges) {
      const c = areas.changesButton;
      this.band(c, [seg(centered('±', c.w))], app.changesOpen || this.sidebarHovered(c) ? PRESSED : { fg: 8, bg: this.surface });
      this.region({ r: c, click: () => app.toggleChanges(), cursor: 'pointer' });
    }
    const icon = rect(menu.x, menu.y, Math.min(7, menu.w), menu.h);
    const lit = !!app.nav || this.sidebarHovered(menu);
    const menuStyle = lit ? PRESSED : { fg: 6, bg: this.surface };
    this.band(icon, menuLabel(app.attentionElsewhere(), icon.w, lit), menuStyle);
    const crumbs = rect(right(icon) + 2, middle(menu).y, Math.max(0, menu.w - icon.w - 2), 1);
    this.line(crumbs, this.breadcrumb(crumbs.w), { bg: this.surface });
    this.region({ r: menu, click: () => app.toggleNav(), cursor: 'pointer' });
    const s = areas.searchButton;
    const sStyle = this.sidebarHovered(s) ? PRESSED : { fg: 8, bg: this.surface };
    this.band(s, [seg(centered('⌕', s.w))], sStyle);
    this.region({ r: s, click: () => app.openSearch(), cursor: 'pointer' });
  }

  private breadcrumb(room: number): Line {
    const app = this.app;
    const p = app.project();
    if (!p) return wordmark();
    const w = p.workspaces[p.active];
    const t = w?.tabs[w.active];
    const crumbs = [projectLabel(p), w ? workspaceLabel(w) : null, t ? tabLabel(t) : null].filter(Boolean) as string[];
    const text = truncateRight(crumbs.join(' › '), room);
    const head = Math.min([...projectLabel(p)].length, [...text].length);
    return [seg([...text].slice(0, head).join(''), { fg: 15, add: BOLD }), seg([...text].slice(head).join(''), { fg: 7 })];
  }

  private title(r: Rect, text: string): void {
    this.line(middle(r), [seg(` ${text}`, { fg: 8, add: BOLD })]);
  }

  private more<R>(list: Rect, rows: Rows, specs: R[], named: (r: R) => boolean): void {
    const [above, under] = rows.hidden();
    const count = (from: number, to: number) => specs.slice(from, to).filter(named).length;
    for (const [n, arrow, r] of [
      [above && count(0, above), '↑', moreAbove(list)],
      [under && count(specs.length - under, specs.length), '↓', rows.moreBelow()],
    ] as const) {
      if (!n || isEmpty(r)) continue;
      this.g.clear(rect(r.x, r.y, r.w, 1));
      this.span(r.x, r.y, `  ${arrow} ${n} more`, DARK, r.w);
    }
  }

  private closeX(row: Rect, pitch: number, bg: Style, act: () => void): void {
    const hover = this.sidebarHovered(row);
    if (!hover && pitch === 1) return;
    const r = closeButton(row, pitch);
    const style = hover && this.hovered(r) ? { ...bg, fg: 1, add: BOLD } : { ...bg, fg: 8 };
    this.band(r, [seg(centered('×', r.w))], style);
    this.region({ r, click: act, cursor: 'pointer' });
  }

  private details(row: Rect, pitch: number, d: Details, indent: number, agent: string | null = null): void {
    const r = intersect(rect(row.x, middle(row).y + 1, row.w, 1), row);
    const close = closeButton(row, pitch);
    const free = r.w - (bottom(close) > r.y ? close.w : 0) - 1;
    const lead: Seg[] = agent === null ? [] : [seg(agent, DARK)];
    const fixed: Seg[] = [];
    if (d.percent !== null) {
      const severity = severityOf(d.percent);
      fixed.push(seg(`${d.percent}%`, severity === 'normal' ? DARK : { fg: SEVERITY[severity] }));
    }
    if (d.memory !== null) fixed.push(seg(memorySize(d.memory), DARK));
    const fixedWidth = width([...lead, ...fixed]) + (lead.length + fixed.length) * CONTEXT_SEPARATOR.length;
    const shift = Math.max(0, Math.min(indent, free - Math.max(0, fixedWidth - CONTEXT_SEPARATOR.length)));
    const modelRoom = Math.max(0, free - shift - fixedWidth);
    const shown = (!fixed.length && !lead.length) || modelRoom >= MIN_MODEL_WIDTH;
    const model = d.model !== null && shown ? [seg(truncateRight(d.model, modelRoom), DARK)] : [];
    const parts = [...lead, ...model, ...fixed].flatMap((part) => [seg(CONTEXT_SEPARATOR, DARK), part]).slice(1);
    this.line(r, [seg(' '.repeat(shift)), ...parts]);
  }

  private groupHeader(group: Group, max: number): Line[number] {
    return seg(`${group.icon} ${truncateRight(group.name, max)}`, { fg: group.colour, add: BOLD });
  }

  private room(b: Band): number {
    return b.r.w - width(b.lead) - closeButton(b.r, b.pitch).w - 1;
  }

  private groupRow(b: Band, g: number): void {
    const app = this.app;
    const group = app.groups[g];
    const count = group.collapsed ? ` (${app.groupSize(group.id)})` : '';
    const badge = group.collapsed ? attention(app.projects.filter((p) => p.group === group.id).map(projectAttention)) : null;
    const room = this.room(b) - 2;
    const used = 2 + count.length;
    const marks = fitTags(badge ? [STATUS_ICONS[badge]] : [], room - used);
    const max = room - used - marks.reserved;
    const line = [...b.lead, arrow(group.collapsed), this.groupHeader(group, max), seg(count, DARK)];
    pushTags(line, marks, used + [...truncateRight(group.name, max)].length, room);
    this.band(b.r, line, b.bg);
    const grab: Target = { kind: 'group', group: group.id };
    this.region({ r: b.r, click: () => app.toggleGroup(g), right: (x, y) => app.openGroupMenu({ x, y }, g), grab, cursor: 'pointer' });
    this.closeX(b.r, b.pitch, b.bg, () => app.askDeleteGroup(group.id));
  }

  private projectRow(b: Band, p: number, summary: boolean, click: (x: number, y: number) => void): void {
    const app = this.app;
    const project = app.projects[p];
    const count = summary ? ` (${project.workspaces.length})` : '';
    const room = this.room(b);
    const badge = summary ? projectAttention(project) : null;
    const marks = fitTags(badge ? [STATUS_ICONS[badge]] : [], room - count.length);
    const name = truncateRight(projectLabel(project), room - count.length - marks.reserved);
    const line = [...b.lead, seg(name, p === app.active ? { fg: 15, add: BOLD } : { fg: 7 }), seg(count, DARK)];
    pushTags(line, marks, [...name].length + count.length, room);
    this.band(b.r, line, b.bg);
    const grab: Target = { kind: 'project', project: project.id };
    this.region({ r: b.r, click, right: (x, y) => app.openMenu({ x, y }, grab), grab, cursor: 'pointer' });
    this.closeX(b.r, b.pitch, b.bg, () => app.askCloseProject(project.id));
  }

  private workspaceRow(b: Band, p: number, w: number, badge: boolean, click: (x: number, y: number) => void): void {
    const app = this.app;
    const project = app.projects[p];
    const ws = project.workspaces[w];
    const active = p === app.active && w === project.active;
    const style: Style = ws.removing ? { fg: 8, add: BOLD } : active ? { fg: 15, add: BOLD } : { fg: 7, add: BOLD };
    const room = this.room(b);
    const status = badge ? attention(ws.tabs.map(tabStatus)) : null;
    const behind = app.config.fetchMinutes && ws.behind ? [seg(`↓${ws.behind}`, { fg: 3 })] : [];
    const tags = ws.removing ? [seg('removing…', DARK)] : [...(status ? [STATUS_ICONS[status]] : []), ...behind];
    const marks = fitTags(tags, room);
    const name = truncateRight(workspaceLabel(ws), room - marks.reserved);
    const line = [...b.lead, seg(name, style)];
    pushTags(line, marks, [...name].length, room);
    this.band(b.r, line, b.bg);
    if (ws.removing) return;
    const grab: Target = { kind: 'workspace', project: project.id, workspace: ws.id };
    this.region({ r: b.r, click, right: (x, y) => app.openMenu({ x, y }, grab), grab, cursor: 'pointer' });
    this.closeX(b.r, b.pitch, b.bg, () => (b.pitch > 1 ? app.askCloseWorkspace(p, w) : app.closeWorkspace(p, w)));
  }

  private tabRow(b: Band, p: number, w: number, t: number, active: boolean): void {
    const app = this.app;
    const project = app.projects[p];
    const ws = project.workspaces[w];
    const tab = ws.tabs[t];
    const status = tabStatus(tab);
    const icon = status ? 2 : 0;
    const name = truncateRight(tabLabel(tab), this.room(b) - icon);
    const line = [...b.lead];
    if (status) line.push(STATUS_ICONS[status], seg(' '));
    line.push(seg(name, active ? { fg: 15 } : { fg: 7 }));
    this.band(b.r, line, b.bg);
    const details = app.tabDetails(tab);
    if (tabLines(details) > 1) this.details(b.r, b.pitch, details, width(b.lead) + icon);
    const grab: Target = { kind: 'tab', project: project.id, workspace: ws.id, tab: tab.id };
    this.region({ r: b.r, click: () => app.selectTab(p, w, t), right: (x, y) => app.openMenu({ x, y }, grab), grab, cursor: 'pointer' });
    this.closeX(b.r, b.pitch, b.bg, () => (b.pitch > 1 ? app.askCloseTab(p, w, t) : app.closeTab(p, w, t)));
  }

  private newTab(r: Rect, indent: string, p: number, w: number): void {
    if (this.app.projects[p]?.workspaces[w]?.removing) return;
    this.button(r, indent, '+ tab', this.buttonStyle(r, DARK, 6));
    this.region({ r, click: () => this.app.addTab(p, w), cursor: 'pointer' });
  }

  private newWorkspace(r: Rect, indent: string, p: number): void {
    this.button(r, indent, '+ new workspace', this.buttonStyle(r, CYAN, 6));
    this.region({ r, click: () => this.app.openNewWorkspace(p), cursor: 'pointer' });
  }

  private newProject(b: Rect): void {
    this.button(b, ' ', '+ new project', this.buttonStyle(b, CYAN, 6));
    this.region({ r: b, click: (x, y) => this.app.openNewMenu({ x, y }), cursor: 'pointer' });
  }

  private sidebar(areas: Areas): void {
    const app = this.app;
    this.title(areas.title, 'projects');
    const all = app.sidebarRows();
    const base = sidebarLayout(areas.list, areas.pitch, all, app.projectsScroll);
    const drag = app.rowDragView();
    const landing = drag?.list === 'sidebar' ? drag.landing : null;
    const indent = landing && landingIndent(landing.spot);
    const dragged = (row: SidebarRow) => drag?.list === 'sidebar' && sameRow(drag.row, row);
    const [sidebar, line] = landed(all, base, landing, { kind: 'landing' } as SidebarRow);
    const rows = sidebarLayout(areas.list, areas.pitch, sidebar, base.first());
    const marked = activeRow(sidebar, app.active, app.groupIndex(app.project()?.group));
    const pitch = areas.pitch;
    this.region({ r: areas.list, wheel: (dy) => app.scrollProjects(base, dy) });
    sidebar.forEach((spec, i) => {
      const r = rows.item(i);
      if (isEmpty(r) || spec.kind === 'gap') return;
      if (spec.kind === 'landing') return this.landing(r, indent);
      if (spec.kind === 'group') {
        this.groupRow({ r, pitch, lead: [marker(i === marked)], bg: this.rowBackground(r, dragged(spec)) }, spec.g);
        if (i === marked) this.rail(r);
        return;
      }
      const active = spec.p === app.active;
      const lead = [marker(active), seg(app.projects[spec.p].group !== undefined ? '  ' : '')];
      this.projectRow({ r, pitch, lead, bg: this.rowBackground(r, active || dragged(spec)) }, spec.p, true, () => app.selectProject(spec.p));
      if (active) this.rail(r);
    });
    this.more(areas.list, rows, sidebar, (s) => s.kind === 'group' || s.kind === 'project');
    this.newProject(rows.buttonRect());
    this.landing(line, indent);
  }

  private tree(areas: Areas): void {
    const app = this.app;
    this.title(areas.title, 'projects');
    const shape = app.treeShape();
    const base = treeLayout(areas.list, shape, treeRows(shape), app.projectsScroll);
    const drag = app.rowDragView();
    const landing = drag?.list === 'tree' ? drag.landing : null;
    const indent = landing && drag?.list === 'tree' ? treeLandingIndent(shape, drag.row, landing.spot) : null;
    const dragged = (row: TreeRow) => drag?.list === 'tree' && sameRow(drag.row, row);
    const [all, line] = landed(treeRows(shape), base, landing, { kind: 'landing' } as TreeRow);
    const rows = treeLayout(areas.list, shape, all, base.first());
    const p = app.project();
    const w = p?.workspaces[p.active];
    const marked = p ? treeActiveRow(all, shape, app.active, p.active, w?.tabs.length ? w.active : null) : -1;
    const folded = (row: TreeRow) => (row.kind === 'project' ? shape.projects[row.p].collapsed : row.kind === 'ws' && !!shape.projects[row.p].workspaces[row.w]?.collapsed);
    const pitch = 1;
    this.region({ r: areas.list, wheel: (dy) => app.scrollProjects(base, dy) });
    all.forEach((row, i) => {
      const r = rows.item(i);
      if (isEmpty(r) || row.kind === 'gap') return;
      if (row.kind === 'landing') {
        this.landing(r, indent);
        const tab = treeAmongTabs(all, i);
        if (tab) this.guide(r.x + treeIndent(shape, tab) - 2, r, 'bar');
        return;
      }
      const mark = i === marked;
      const bg = this.rowBackground(r, mark || dragged(row));
      const spaces = seg(' '.repeat(2 * treeDepth(shape, row)));
      const lead = [marker(mark), spaces, arrow(folded(row))];
      const fold = arrowIn(r, shape, row);
      const button = ' '.repeat(treeIndent(shape, row) - 1);
      const guide = r.x + treeIndent(shape, row) - 2;
      if (row.kind === 'group') this.groupRow({ r, pitch, lead: [marker(mark)], bg }, row.g);
      else if (row.kind === 'project') {
        this.projectRow({ r, pitch, lead, bg }, row.p, folded(row), (x, y) => (contains(fold, x, y) ? app.toggleFold(row.p) : app.selectProject(row.p)));
      } else if (row.kind === 'ws') {
        this.workspaceRow({ r, pitch, lead, bg }, row.p, row.w, folded(row), (x, y) => (contains(fold, x, y) ? app.toggleFold(row.p, row.w) : app.selectWorkspace(row.p, row.w)));
      } else if (row.kind === 'tab') {
        this.tabRow({ r, pitch, lead: [marker(mark), spaces], bg }, row.p, row.w, row.t, mark);
        this.guide(guide, r, 'tee');
      } else if (row.kind === 'newTab') {
        this.newTab(r, button, row.p, row.w);
        this.guide(guide, r, 'end');
      } else this.newWorkspace(r, button, row.p);
      if (mark) this.rail(r);
    });
    this.more(areas.list, rows, all, (r) => r.kind === 'group' || r.kind === 'project' || r.kind === 'ws' || r.kind === 'tab');
    this.newProject(rows.buttonRect());
    this.landing(line, indent);
  }

  private agents(areas: Areas): void {
    const app = this.app;
    if (isEmpty(areas.agents)) {
      const b = areas.agentsButton;
      if (isEmpty(b)) return;
      this.button(b, '', AGENTS_LABEL, this.buttonStyle(b, CYAN, 6));
      this.region({ r: b, click: () => app.navTo('agents'), cursor: 'pointer' });
      return;
    }
    const line = areas.agentsBorder;
    if (!isEmpty(line)) {
      const lit = (app.dragging?.kind === 'border' && app.dragging.border === 'agents') || this.sidebarHovered(line);
      this.line(line, [seg(` ${'─'.repeat(Math.max(0, line.w - 2))}`, { fg: lit ? 6 : this.lineColour })]);
      this.region({ r: line, drag: { kind: 'border', border: 'agents' }, double: () => app.resetBorder('agents'), cursor: 'row-resize' });
    }
    if (areas.compact) this.back(areas, AGENTS_TITLE);
    else this.title(areas.agentsTitle, AGENTS_TITLE);
    const list = areas.agentsList;
    const entries = app.agentRows();
    const rows = agentLayout(list, areas.pitch, entries.length, app.agentsScroll);
    if (!entries.length) {
      const first = middle(rect(list.x, list.y, list.w, Math.min(areas.pitch, list.h)));
      this.span(first.x, first.y, `  ${NO_AGENTS}`, DARK, first.w);
    }
    this.region({ r: list, wheel: (dy) => app.scrollAgents(rows, dy) });
    entries.forEach((entry, i) => {
      const r = rows.item(i);
      if (isEmpty(r)) return;
      this.agentRow(r, areas.pitch, entry);
      this.region({ r, click: () => app.jumpToPane(entry.pane), hold: true, cursor: 'pointer' });
    });
    this.more(list, rows, entries, () => true);
  }

  private agentRow(r: Rect, pitch: number, entry: AgentRow): void {
    const bg = this.rowBackground(r, entry.active);
    const line: Line = [marker(entry.active), STATUS_ICONS[entry.status], seg(' ')];
    const indent = width(line);
    const room = Math.max(0, r.w - indent - 1);
    line.push(...agentPlace(entry, room, { fg: entry.active ? 15 : 7 }));
    this.band(r, line, bg);
    this.details(r, pitch, entry.details, indent, entry.agent);
    if (entry.active) this.rail(r);
  }

  private back(areas: Areas, name: string): void {
    const style = this.buttonStyle(areas.back, CYAN, 6);
    this.button(areas.back, '', '‹ projects', style);
    this.region({ r: areas.back, click: () => this.app.navTo('projects'), cursor: 'pointer' });
    const restX = right(areas.back);
    const m = middle(areas.back);
    this.span(restX, m.y, ` ${truncateRight(name, Math.max(0, right(areas.bar) - restX - 1))}`, { fg: 8, add: BOLD });
  }

  private footer(areas: Areas): void {
    const app = this.app;
    this.line(areas.separator, [seg(` ${'─'.repeat(Math.max(0, areas.separator.w - 2))}${areas.compact ? ' ' : ''}`, { fg: this.lineColour })]);
    this.button(areas.settings, ' ', 'settings', this.buttonStyle(areas.settings, DARK, 6));
    this.region({ r: areas.settings, click: () => app.openSettings(), cursor: 'pointer' });
    this.button(areas.usage, ' ', 'usage', this.buttonStyle(areas.usage, DARK, 6));
    this.region({ r: areas.usage, click: () => app.openUsage(), cursor: 'pointer' });
    this.button(areas.quit, ' ', 'quit', this.buttonStyle(areas.quit, DARK, 1));
    this.region({ r: areas.quit, click: () => app.quit(), cursor: 'pointer' });
  }

  private workspaces(areas: Areas): void {
    const app = this.app;
    if (isEmpty(areas.back)) this.title(areas.workspacesTitle, 'workspaces');
    else this.back(areas, app.project() ? projectLabel(app.project()!) : '');
    const p = app.project();
    if (!p) return;
    const list = areas.workspacesList;
    const tabs = app.tabLines();
    const all = workspaceRows(tabs);
    const base = workspaceLayout(list, areas.pitch, all, tabs, app.workspacesScroll);
    const drag = app.rowDragView();
    const landing = drag?.list === 'workspaces' ? drag.landing : null;
    const indent = landing && landingIndent(landing.spot);
    const dragged = (row: WorkspaceRow) => drag?.list === 'workspaces' && sameRow(drag.row, row);
    const [rowsSpec, line] = landed(all, base, landing, { kind: 'landing' } as WorkspaceRow);
    const rows = workspaceLayout(list, areas.pitch, rowsSpec, tabs, base.first());
    const pitch = areas.pitch;
    this.region({ r: list, wheel: (dy) => app.scrollWorkspaces(base, dy) });
    rowsSpec.forEach((spec, i) => {
      const r = rows.item(i);
      if (isEmpty(r) || spec.kind === 'gap') return;
      const guide = r.x + 2;
      if (spec.kind === 'landing') {
        this.landing(r, indent);
        if (amongTabs(rowsSpec, i)) this.guide(guide, r, 'bar');
        return;
      }
      if (spec.kind === 'ws') {
        this.workspaceRow({ r, pitch, lead: [seg('  ')], bg: this.rowBackground(r, dragged(spec)) }, app.active, spec.w, true, () => app.selectWorkspace(app.active, spec.w));
        this.guide(guide, r, 'top');
      } else if (spec.kind === 'tab') {
        const active = spec.w === p.active && spec.t === p.workspaces[spec.w].active;
        this.tabRow({ r, pitch, lead: [marker(active), seg('  ')], bg: this.rowBackground(r, active || dragged(spec)) }, app.active, spec.w, spec.t, active);
        this.guide(guide, r, 'tee');
        if (active) this.rail(r);
      } else {
        this.newTab(r, '   ', app.active, spec.w);
        this.guide(guide, r, 'end');
      }
    });
    this.more(list, rows, rowsSpec, (r) => r.kind === 'ws' || r.kind === 'tab');
    this.newWorkspace(rows.buttonRect(), ' ', app.active);
    this.landing(line, indent);
  }

  private issuesRow(areas: Areas): void {
    const app = this.app;
    this.line(areas.workspacesSeparator, [seg(` ${'─'.repeat(Math.max(0, areas.workspacesSeparator.w - 2))}`, { fg: this.lineColour })]);
    this.button(areas.issues, ' ', 'issues', this.buttonStyle(areas.issues, DARK, 6));
    this.region({ r: areas.issues, click: () => app.openIssues(), cursor: 'pointer' });
    if (!areas.compact && hasChanges(app.workspace())) {
      const label = changesLabel(app.changesDiff());
      const w = label.length + 2;
      const r = intersect(rect(right(areas.issues) - w - 1, areas.issues.y, w, 1), areas.issues);
      const idle: Style = app.changesOpen ? { fg: 6, add: BOLD } : DARK;
      this.button(r, '', label, this.buttonStyle(r, idle, 6));
      this.region({ r, click: () => app.toggleChanges(), cursor: 'pointer' });
    }
    if (!areas.compact) {
      const r = areas.todoButton;
      this.button(r, '', TODO_LABEL, this.buttonStyle(r, app.todo.open ? { fg: 6, add: BOLD } : DARK, 6));
      this.region({ r, click: () => app.toggleTodo(), cursor: 'pointer' });
      const f = areas.filesButton;
      this.button(f, '', FILES_LABEL, this.buttonStyle(f, app.filesShown() ? { fg: 6, add: BOLD } : DARK, 6));
      this.region({ r: f, click: () => app.toggleFiles(), cursor: 'pointer' });
    }
  }

  private box(r: Rect, title: string): void {
    this.g.clear(r);
    this.g.box(r, { fg: 8 }, title || undefined, { add: BOLD, fg: -1 });
  }

  private backdrop(close: boolean): void {
    const app = this.app;
    this.region({ r: rect(0, 0, app.cols, app.rows), click: close ? () => app.closeOverlay() : () => {}, right: close ? () => app.closeOverlay() : () => {} });
  }

  private overlay(areas: Areas): void {
    const o = this.app.overlay;
    if (!o) return;
    if (o.kind !== 'menu' && o.kind !== 'search') this.dim();
    if (o.kind === 'menu') return this.menu();
    if (o.kind === 'newWorkspace' || o.kind === 'rename' || o.kind === 'newGroup') return this.form();
    if (o.kind === 'groupStyle') return this.groupStyle(o.group);
    const confirm = this.app.confirmView();
    if (confirm) return this.confirm(confirm);
    if (o.kind === 'picker') return this.picker();
    if (o.kind === 'settings') return this.settings(o);
    if (o.kind === 'usage') return this.usage();
    if (o.kind === 'issues') return this.issues(o);
    if (o.kind === 'search') return this.results(areas);
  }

  private menu(): void {
    const app = this.app;
    const o = app.overlay;
    if (o?.kind !== 'menu') return;
    const items = o.actions.map((a) => app.menuLabel(a));
    const r = menuArea(app.cols, app.rows, o.at, items);
    this.backdrop(true);
    this.box(r, '');
    items.forEach((item, i) => {
      const row = intersect(rect(r.x + 1, r.y + 1 + i, r.w - 2, 1), r);
      const lit = this.hovered(row);
      const bg: Style = lit ? { bg: this.surface } : {};
      this.line(row, [rail(lit, bg), seg(`${item} `, lit ? { add: BOLD } : {})], bg);
      this.region({ r: row, click: () => app.chooseMenu(i), cursor: 'pointer' });
    });
  }

  private dialogButtons(row: Rect, submit: string, onSubmit: () => void, onCancel: () => void): void {
    const cancel = intersect(rect(right(row) - buttonWidth('cancel'), row.y, buttonWidth('cancel'), 1), row);
    const ok = intersect(rect(cancel.x - buttonWidth(submit) - 1, row.y, buttonWidth(submit), 1), row);
    this.submitButton(ok, submit, onSubmit);
    this.span(cancel.x, cancel.y, ' cancel ', this.pill(cancel, false));
    this.region({ r: cancel, click: onCancel, cursor: 'pointer' });
  }

  private pill(r: Rect, primary: boolean): Style {
    if (primary) return this.hovered(r) ? PRESSED : { fg: 6, bg: this.surface, add: BOLD };
    return this.hovered(r) ? { fg: 0, bg: 7 } : { fg: 7, bg: this.surface };
  }

  private submitButton(r: Rect, label: string, onClick: () => void): void {
    this.span(r.x, r.y, ` ${label} `, this.pill(r, true), r.w);
    this.region({ r, click: onClick, cursor: 'pointer' });
  }

  private input(row: Rect, label: string, value: string): void {
    const lbl = label ? `${label} ` : '';
    const max = Math.max(0, row.w - INPUT_PROMPT.length - [...lbl].length - 1);
    const v = truncateLeft(value, max);
    let x = this.span(row.x, row.y, INPUT_PROMPT, CYAN);
    x = this.span(x, row.y, lbl, DARK);
    x = this.span(x, row.y, v, { add: BOLD });
    if (x < right(row)) this.cursor = { x, y: row.y, shape: 'block' };
  }

  private form(): void {
    const app = this.app;
    const o = app.overlay;
    if (!o || (o.kind !== 'newWorkspace' && o.kind !== 'rename' && o.kind !== 'newGroup')) return;
    const r = formArea(app.cols, app.rows);
    this.backdrop(false);
    const title = o.kind === 'rename' ? app.renameLabel(o.target) : o.kind === 'newGroup' ? 'new group' : 'new workspace';
    this.box(r, title);
    const c = rect(r.x + 2, r.y + 1, r.w - 4, r.h - 2);
    this.span(c.x, c.y, 'name', DARK);
    this.input(rect(c.x, c.y + 1, c.w, 1), '', o.input);
    if (o.kind === 'newWorkspace') {
      this.span(c.x, c.y + 2, truncateLeft(app.newWorkspaceHint(o), c.w), DARK);
      if (o.worktree !== null) {
        const t = rect(c.x, c.y + 3, c.w, 1);
        this.span(t.x, t.y, `${o.worktree ? '[x]' : '[ ]'} with its own worktree`, this.hovered(t) ? CYAN : {});
        this.region({ r: t, click: () => app.toggleWorktree(), cursor: 'pointer' });
      }
      if (o.creating) {
        this.span(c.x, c.y + 4, 'creating…', DARK);
        this.cursor = null;
      } else if (o.error) this.span(c.x, c.y + 4, truncateRight(o.error, c.w), { fg: 1 });
    } else if (o.kind === 'newGroup') this.span(c.x, c.y + 2, truncateLeft('right-click a project to move it into the group', c.w), DARK);
    else this.span(c.x, c.y + 2, truncateLeft(app.renameHint(o.target), c.w), DARK);
    this.dialogButtons(rect(c.x, bottom(c) - 1, c.w, 1), o.kind === 'rename' ? 'rename' : 'create', () => app.submitForm(), () => app.closeOverlay());
  }

  private groupStyle(id: number): void {
    const app = this.app;
    const group = app.group(id);
    if (!group) return;
    const { cols, rows } = app;
    this.backdrop(false);
    this.box(formArea(cols, rows), group.name);
    const [iconLabel, colourLabel, last] = styleLabels(cols, rows);
    this.span(iconLabel.x, iconLabel.y, 'icon', DARK, iconLabel.w);
    this.span(colourLabel.x, colourLabel.y, 'colour', DARK, colourLabel.w);
    GROUP_ICONS.forEach((icon, i) => {
      const cell = styleIcon(cols, rows, i);
      if (isEmpty(cell)) return;
      const style: Style = icon === group.icon ? PRESSED : this.hovered(cell) ? CYAN : { fg: group.colour };
      this.span(cell.x, cell.y, ` ${icon} `, style, cell.w);
      this.region({ r: cell, click: () => app.setGroupStyle(icon), cursor: 'pointer' });
    });
    GROUP_COLOURS.forEach((colour, i) => {
      const cell = styleColour(cols, rows, i);
      if (isEmpty(cell)) return;
      const chosen = colour === group.colour;
      const [open, close, edge]: [string, string, Style] = chosen ? ['[', ']', { fg: 15, add: BOLD }] : this.hovered(cell) ? ['[', ']', DARK] : [' ', ' ', DARK];
      drawLine(this.g, cell.x, cell.y, [seg(open, edge), seg('██', { fg: colour }), seg(close, edge)], cell.w);
      this.region({ r: cell, click: () => app.setGroupStyle(undefined, colour), cursor: 'pointer' });
    });
    const done = styleDone(cols, rows);
    const max = Math.max(0, last.w - done.w - 5);
    this.line(last, [seg('▾ ', DARK), this.groupHeader(group, max)]);
    this.submitButton(done, DONE, () => app.closeOverlay());
  }

  private usage(): void {
    const app = this.app;
    const { loading, at } = app.usage;
    const r0 = usageArea(app.cols, app.rows, 0);
    const width = inner(r0).w;
    const status = loading ? 'loading…' : at === null ? '' : updated(app.now() - at);
    const lines: Line[] = [];
    for (const section of USAGE) {
      const title = at === null ? section.agent : `${section.agent} · ${section.plan} plan`;
      const room = width - [...title].length - [...status].length;
      lines.push([seg(title, { add: BOLD }), seg(' '.repeat(Math.max(0, room))), seg(status, DARK)], []);
      for (const w of at === null ? [] : section.windows) {
        const colour: Style = { fg: SEVERITY[w.severity] };
        const percent = `${w.percent}%`;
        const resets = ` · ${w.resets}`;
        const filled = Math.ceil((width * Math.min(w.percent, 100)) / 100);
        const room = width - [...w.label].length - percent.length - [...resets].length;
        lines.push([seg(w.label), seg(' '.repeat(Math.max(0, room))), seg(percent, { ...colour, add: BOLD }), seg(resets, DARK)]);
        lines.push([seg(USAGE_BAR.repeat(filled), colour), seg(USAGE_BAR.repeat(width - filled), { fg: this.lineColour })]);
        lines.push([]);
      }
    }
    const r = usageArea(app.cols, app.rows, lines.length);
    this.backdrop(false);
    this.box(r, 'usage');
    const c = inner(r);
    lines.slice(0, Math.max(0, c.h - 1)).forEach((line, i) => this.line(rect(c.x, c.y + i, c.w, 1), line));
    this.submitButton(usageDone(r), DONE, () => app.closeOverlay());
  }

  private confirm({ title, message, submit, note }: ConfirmView): void {
    const app = this.app;
    const r = formArea(app.cols, app.rows);
    this.backdrop(false);
    this.box(r, title);
    const c = rect(r.x + 2, r.y + 1, r.w - 4, r.h - 2);
    const rows = wrapAll([[seg(message)]], c.w);
    rows.slice(0, 4).forEach((row, i) => row.forEach((cell, x) => this.g.put(c.x + x, c.y + i, cell.ch, {})));
    if (note) this.span(c.x, c.y + 4, note, DARK);
    this.dialogButtons(rect(c.x, bottom(c) - 1, c.w, 1), submit, () => app.submitConfirm(), () => app.closeOverlay());
  }

  private picker(): void {
    const app = this.app;
    const o = app.overlay;
    if (!o || o.kind !== 'picker') return;
    const r = pickerArea(app.cols, app.rows);
    this.backdrop(false);
    this.box(r, 'new project');
    const c = rect(r.x + 2, r.y + 1, r.w - 4, r.h - 2);
    const path = app.pickerPath(o);
    const max = Math.max(0, c.w - INPUT_PROMPT.length - 1);
    const filter = truncateLeft(o.filter, max);
    const shown = truncateLeft(path, Math.max(0, max - [...filter].length));
    let x = this.span(c.x, c.y, INPUT_PROMPT, CYAN);
    x = this.span(x, c.y, shown, {});
    x = this.span(x, c.y, filter, { add: BOLD });
    this.cursor = { x, y: c.y, shape: 'block' };
    const list = rect(c.x, c.y + 2, c.w, Math.max(0, c.h - 4));
    const items = app.pickerItems(o);
    if (!items.length) this.span(list.x, list.y, o.filter ? ' no matches' : ' no folders here', DARK);
    const first = Math.min(o.scroll, Math.max(0, items.length - list.h));
    items.slice(first, first + list.h).forEach((item, k) => {
      const i = first + k;
      const row = rect(list.x, list.y + k, list.w, 1);
      const selected = o.selected === i;
      const bg = this.pickBackground(selected, row);
      const branch = item.branch ?? '';
      const name = truncateLeft(item.name, row.w - branch.length - 3);
      this.g.fill(row, bg);
      this.line(rect(row.x, row.y, 1, 1), [rail(selected, bg)]);
      this.span(row.x + 1, row.y, name, { ...bg, ...(selected ? { add: BOLD } : {}) });
      this.span(right(row) - branch.length - 1, row.y, branch, { ...bg, fg: 8 });
      this.region({ r: row, click: () => app.pickerClick(i), cursor: 'pointer' });
    });
    this.region({ r: list, wheel: (dy) => app.pickerScroll(dy) });
    this.span(c.x, bottom(c) - 2, truncateLeft(app.pickerHint(o), c.w), DARK);
    this.dialogButtons(rect(c.x, bottom(c) - 1, c.w, 1), 'open', () => app.pickerOpen(), () => app.closeOverlay());
  }

  private tabs(row: Rect, names: string[], active: number, onClick: (i: number) => void): void {
    tabsIn(row, names).forEach((r, i) => {
      const style: Style = i === active ? { fg: 6, bg: this.surface, add: BOLD } : this.hovered(r) ? CYAN : { fg: 7 };
      this.span(r.x, r.y, ` ${names[i]} `, style, r.w);
      this.region({ r, click: () => onClick(i), cursor: 'pointer' });
    });
  }

  private settings(o: SettingsOverlay): void {
    const app = this.app;
    const r = issuesArea(app.cols, app.rows);
    this.backdrop(false);
    this.box(r, 'settings');
    const c = inner(r);
    const tabsRow = rect(c.x, c.y, c.w, 1);
    const body = rect(c.x, c.y + 2, c.w, Math.max(0, c.h - 5));
    const editRow = rect(c.x, bottom(body), c.w, 1);
    const noteRow = rect(c.x, bottom(body) + 1, c.w, 1);
    const buttonsRow = rect(c.x, bottom(body) + 2, c.w, 1);
    this.tabs(tabsRow, ['Worktrees', 'Agents', 'Issues', 'TUI'], o.page, (i) => app.settingsPage(i));
    if (o.pick) {
      this.input(rect(body.x, body.y, body.w, 1), o.pick.title, o.pick.filter);
      const list = rect(body.x, body.y + 2, body.w, Math.max(0, body.h - 2));
      const items = app.pickChoices(o);
      if (!items.length) this.span(list.x, list.y, ' nothing matches', DARK);
      items.slice(0, list.h).forEach((item, i) => {
        const row = rect(list.x, list.y + i, list.w, 1);
        const selected = Math.min(o.pick!.selected, items.length - 1) === i;
        const base = this.pickBackground(selected, row);
        this.g.fill(row, base);
        this.line(rect(row.x, row.y, 1, 1), [rail(selected, base)]);
        const value: Style = { ...base, ...(item.dangerous ? { fg: 1 } : {}), add: BOLD };
        const x = this.span(row.x + 1, row.y, `${item.value.padEnd(32)} `, value);
        this.span(x, row.y, item.note, { ...base, fg: 8 }, right(row) - x);
        this.region({ r: row, click: () => app.choosePick(i), cursor: 'pointer' });
      });
    } else {
      const rows = app.settingsRows(o.page);
      const lines: ({ kind: 'blank' } | { kind: 'header'; text: string } | { kind: 'row'; i: number })[] = [];
      rows.forEach((row, i) => {
        if (row.section && (i === 0 || rows[i - 1].section !== row.section)) {
          if (i > 0) lines.push({ kind: 'blank' });
          lines.push({ kind: 'header', text: row.section });
        }
        lines.push({ kind: 'row', i });
      });
      lines.slice(0, body.h).forEach((line, n) => {
        const y = body.y + n;
        if (line.kind === 'header') this.span(body.x, y, line.text, { fg: 8, add: BOLD });
        if (line.kind !== 'row') return;
        const row = rows[line.i];
        const selected = line.i === o.cursor;
        const rr = rect(body.x, y, body.w, 1);
        const bg: Style = selected ? { bg: this.surface } : {};
        this.g.fill(rr, bg);
        let x = this.span(rr.x, y, selected ? '▌ ' : '  ', { ...bg, fg: 6 });
        x = this.span(x, y, truncateRight(row.label, 24).padEnd(26), { ...bg, ...(selected ? { add: BOLD } : {}) });
        const value = truncateRight(row.value, Math.floor((rr.w - 30) / 2) + 10);
        x = this.span(x, y, value, { ...bg, ...(row.dangerous ? { fg: 1 } : {}) });
        if (value && row.note) x = this.span(x, y, '  ', bg);
        this.span(x, y, row.note, { ...bg, fg: 8 }, right(rr) - x);
        this.region({ r: rr, click: () => app.settingsClick(line.i), cursor: 'pointer' });
      });
    }
    if (o.edit) this.input(editRow, `${o.edit.label}:`, o.edit.token ? '•'.repeat(o.edit.input.length) : o.edit.input);
    const note = o.busy ?? o.edit?.error ?? o.notice ?? app.settingsHint(o);
    this.span(noteRow.x, noteRow.y, truncateRight(note, noteRow.w), o.edit?.error && !o.busy ? { fg: 1 } : DARK);
    if (o.busy) this.cursor = null;
    this.submitButton(rightAligned(buttonsRow, [DONE], buttonWidth, 1)[0], DONE, () => app.closeOverlay());
  }

  private issues(o: IssuesOverlay): void {
    const app = this.app;
    const r = issuesArea(app.cols, app.rows);
    this.backdrop(false);
    this.box(r, `issues · ${app.issuesProjectName(o)}`);
    const c = inner(r);
    const tabsRow = rect(c.x, c.y, c.w, 1);
    const inputRow = rect(c.x, c.y + 2, c.w, 1);
    const list = rect(c.x, c.y + 4, c.w, Math.max(0, c.h - 6));
    const noteRow = rect(c.x, bottom(c) - 2, c.w, 1);
    const buttonsRow = rect(c.x, bottom(c) - 1, c.w, 1);
    const view = app.issuesView(o);
    if (view.detail) {
      const area = rect(c.x, c.y, c.w, noteRow.y - c.y);
      const lines = view.detail;
      lines.slice(o.detailScroll, o.detailScroll + area.h).forEach((line, i) => drawLine(this.g, area.x, area.y + i, line, area.w));
      this.region({ r: area, wheel: (dy) => app.issuesScrollDetail(dy, lines.length - area.h) });
    } else {
      this.tabs(tabsRow, view.tabs, o.tab, (i) => app.issuesTab(i));
      const toggles = view.toggles;
      rightAligned(tabsRow, toggles.map((t) => t.label), (l) => buttonWidth(l) + 4, 2).forEach((tr, i) => {
        const t = toggles[i];
        this.span(tr.x, tr.y, ` ${t.on ? '[x]' : '[ ]'} ${t.label} `, this.hovered(tr) ? CYAN : {}, tr.w);
        this.region({ r: tr, click: () => app.issuesToggle(i), cursor: 'pointer' });
      });
      if (view.token) {
        this.input(inputRow, view.token.label, view.token.input);
        const help = wrapAll(view.token.help.map((h) => [seg(h)]), list.w - 2);
        help.slice(0, list.h).forEach((row, i) => row.forEach((cell, x) => this.g.put(list.x + 1 + x, list.y + i, cell.ch, {})));
      } else {
        this.input(inputRow, '', view.picking ? o.agentPick?.filter ?? '' : o.filter);
        const items = view.items;
        if (!items.length) this.span(list.x, list.y, ` ${view.empty}`, DARK);
        const keyWidth = Math.max(0, ...items.map((i) => [...i.key].length));
        const first = Math.min(view.scroll, Math.max(0, items.length - list.h));
        items.slice(first, first + list.h).forEach((item, k) => {
          const i = first + k;
          const row = rect(list.x, list.y + k, list.w, 1);
          const chosen = view.selected === i;
          const bg = this.pickBackground(chosen, row);
          const style: Style = chosen ? { ...bg, add: BOLD } : bg;
          this.g.fill(row, bg);
          const meta = truncateRight(item.meta, Math.floor(row.w / 3));
          const title = truncateRight(item.title, row.w - keyWidth - meta.length - 6);
          this.line(rect(row.x, row.y, 1, 1), [rail(chosen, bg)]);
          let x = this.span(row.x + 1, row.y, `${item.key.padEnd(keyWidth)}  `, { ...style, fg: item.danger ? 1 : 6 });
          x = this.span(x, row.y, title, style);
          this.span(right(row) - meta.length - 1, row.y, meta, { ...bg, fg: 8 });
          this.region({ r: row, click: () => app.issuesClick(i), cursor: 'pointer' });
        });
        this.region({ r: list, wheel: (dy) => app.issuesScroll(dy) });
      }
    }
    const busy = o.busy;
    const note = busy ?? o.notice ?? view.hint;
    this.span(noteRow.x, noteRow.y, truncateRight(note, noteRow.w), view.error && !busy ? { fg: 1 } : DARK);
    if (busy) this.cursor = null;
    const labels = view.buttons;
    rightAligned(buttonsRow, labels, buttonWidth, 1).forEach((br, i) => {
      this.span(br.x, br.y, ` ${labels[i]} `, this.pill(br, i === 0));
      this.region({ r: br, click: () => app.issuesButton(labels[i]), cursor: 'pointer' });
    });
  }

  private results(areas: Areas): void {
    const app = this.app;
    const o = app.overlay;
    if (!o || o.kind !== 'search') return;
    this.backdrop(true);
    this.region({ r: areas.compact ? areas.bar : areas.search, click: () => {} });
    if (!o.query) return;
    const r = areas.results;
    this.g.clear(r);
    const list = rect(r.x, r.y, r.w, Math.max(0, r.h - 2));
    const hint = rect(r.x, bottom(r) - 1, r.w, 1);
    const results = app.searchResults(o.query);
    if (!results.length) this.span(list.x, list.y, '  no matches', DARK);
    const first = Math.min(o.scroll, Math.max(0, results.length - list.h));
    results.slice(first, first + list.h).forEach((res, k) => {
      const i = first + k;
      const row = rect(list.x, list.y + k, list.w, 1);
      const selected = o.selected === i;
      const bg = this.pickBackground(selected, row);
      const base: Style = selected ? { ...bg, add: BOLD } : bg;
      const matched: Style = { ...bg, fg: 6, add: BOLD };
      const ctxStyle: Style = { ...bg, fg: 8 };
      this.g.fill(row, bg);
      const context = truncateLeft(res.context, Math.floor(row.w / 2));
      const name = truncateRight(res.name, row.w - context.length - 5);
      this.line(rect(row.x, row.y, 1, 1), [rail(selected, bg)]);
      let x = row.x + 2;
      const at = name.toLowerCase().indexOf(o.query.toLowerCase());
      if (at >= 0) {
        x = this.span(x, row.y, name.slice(0, at), base);
        x = this.span(x, row.y, name.slice(at, at + o.query.length), matched);
        x = this.span(x, row.y, name.slice(at + o.query.length), base);
      } else x = this.span(x, row.y, name, base);
      this.span(right(row) - context.length - 1, row.y, context, ctxStyle);
      this.region({ r: row, click: () => app.searchGo(i), cursor: 'pointer' });
    });
    const selected = results[Math.min(o.selected, results.length - 1)];
    if (selected) this.span(hint.x, hint.y, `  ${truncateLeft(`enter goes to ${selected.name}`, hint.w - 2)}`, DARK);
  }

  private toast(message: string, status?: Status, undo?: () => void): void {
    const app = this.app;
    const w = Math.min(3 + [...message].length + 1 + (undo ? UNDO.length + 3 : 0) + 2, app.cols);
    const h = Math.min(3, app.rows);
    const r = rect(Math.max(0, app.cols - w - 1), Math.max(0, app.rows - h - 1), w, h);
    const icon = status ? STATUS_ICONS[status] : seg('✓', { fg: 2 });
    this.g.clear(r);
    this.g.box(r, { fg: icon.s?.fg ?? 2 });
    const x = this.span(r.x + 1, r.y + 1, ` ${icon.t} `, icon.s ?? {});
    this.span(x, r.y + 1, message, {}, right(r) - 1 - x);
    if (!undo) return;
    const b = intersect(rect(right(r) - UNDO.length - 4, r.y + 1, UNDO.length + 2, 1), r);
    this.span(b.x, b.y, ` ${UNDO} `, this.hovered(b) ? PRESSED : { fg: 6, add: BOLD });
    this.region({ r: b, click: undo, cursor: 'pointer' });
  }
}

function agentPlace(entry: AgentRow, room: number, name: Style): Line {
  const project = [...entry.project].length;
  const rest = room - (project + 3);
  if (entry.workspace !== null && rest >= MIN_WORKSPACE_WIDTH) return [seg(entry.project, name), seg(' › ', DARK), seg(truncateRight(entry.workspace, rest), name)];
  return [seg(truncateRight(entry.project, room), name)];
}

function amongTabs(rows: WorkspaceRow[], i: number): boolean {
  const above = rows[i - 1];
  const below = rows[i + 1];
  const owner = above && (above.kind === 'ws' || above.kind === 'tab') ? above.w : null;
  const next = below && (below.kind === 'tab' || below.kind === 'new') ? below.w : null;
  return owner !== null && owner === next;
}

function treeAmongTabs(rows: TreeRow[], i: number): TreeRow | null {
  const above = rows[i - 1];
  const below = rows[i + 1];
  if (!above || !below || (above.kind !== 'ws' && above.kind !== 'tab') || (below.kind !== 'tab' && below.kind !== 'newTab')) return null;
  return above.p === below.p && above.w === below.w ? { kind: 'tab', p: above.p, w: above.w, t: 0 } : null;
}

function menuLabel(status: Status | null, w: number, lit: boolean): Line {
  if (!status) return [seg(centered('≡', w))];
  const left = Math.floor((w - 1) / 2);
  return [seg(`${' '.repeat(left)}≡ `), lit ? seg(STATUS_ICONS[status].t) : STATUS_ICONS[status], seg(' '.repeat(Math.max(0, w - left - 3)))];
}

interface Tags {
  segs: Seg[];
  width: number;
  reserved: number;
}

function fitTags(tags: Seg[], room: number): Tags {
  const segs: Seg[] = [];
  let width = 0;
  for (const tag of tags) {
    const next = segs.length ? width + 1 + [...tag.t].length : [...tag.t].length;
    if (next + 1 >= room) break;
    if (segs.length) segs.push(seg(' '));
    segs.push(tag);
    width = next;
  }
  return { segs, width, reserved: segs.length ? width + 1 : 0 };
}

function pushTags(line: Line, tags: Tags, used: number, room: number): void {
  if (!tags.segs.length) return;
  line.push(seg(' '.repeat(Math.max(0, room - used - tags.width))), ...tags.segs);
}

export function centered(text: string, w: number): string {
  const n = [...text].length;
  const left = Math.max(0, Math.floor((w - n) / 2));
  return ' '.repeat(left) + text + ' '.repeat(Math.max(0, w - n - left));
}
