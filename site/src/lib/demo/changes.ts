import { BOLD, ITALIC, type Rect, type Style, rect } from '../term/grid';
import { rgb } from '../term/palette';
import type { App } from './app';
import { ADDRESS_FIXED_RS, ADDRESS_RS, API, RETURNS_RS, THEME_RS, type Tree } from './data';
import { highlight, language } from './highlight';
import { type Areas, bottom, isEmpty, right } from './layout';
import type { Project, Workspace } from './model';
import { type Line, truncateLeft, truncateRight } from './text';
import type { Painter } from './ui';

export type ChangesMode = 'uncommitted' | 'commits' | 'all';
export const CHANGES_MODES: ChangesMode[] = ['uncommitted', 'commits', 'all'];
export const BASES = ['origin/main', 'main'];

type Status = 'M' | 'A' | 'D';

interface Change {
  path: string;
  status: Status;
  before: string;
  after: string;
  untracked?: boolean;
  lockfile?: boolean;
}

interface DiffLine {
  kind: ' ' | '-' | '+';
  old?: number;
  new?: number;
  text: string;
  syntax: Line;
  emph: [number, number][];
}

interface Hunk {
  newStart: number;
  newEnd: number;
  context: string;
  lines: DiffLine[];
}

export interface FileDiff {
  change: Change;
  hunks: Hunk[];
  added: number;
  removed: number;
}

const RETURNS_FIXED_RS = `mod address;

use axum::{Json, Router, http::StatusCode, routing::post};

pub use address::Address;

pub fn routes() -> Router {
    Router::new().route("/returns", post(create))
}

async fn create(Json(address): Json<Address>) -> Result<String, StatusCode> {
    let line = address.first_line().ok_or(StatusCode::UNPROCESSABLE_ENTITY)?;
    Ok(format!("label for {line}"))
}
`;

const RETURNS_TESTS_RS = `use super::Address;

#[test]
fn empty_address_is_rejected() {
    let address = Address { lines: vec![], city: "Lyon".into(), postcode: "69001".into() };
    assert_eq!(address.first_line(), None);
}
`;

const THEME_DARK_RS = THEME_RS.replace(
  'impl Theme {\n',
  'impl Theme {\n    pub fn from_system(prefers_dark: bool) -> Self {\n        if prefers_dark { Theme::Dark } else { Theme::Light }\n    }\n\n',
).replace('"#0e0d14"', '"#121018"');

const README_RS = `# shop

The storefront and returns service.

## Running

    cargo run
`;

const README_DARK = `${README_RS}
## Themes

The checkout follows \`prefers-color-scheme\`.
`;

const LOCK = 'version = 4\n\n[[package]]\nname = "shop"\nversion = "0.5.0"\n';

const ORDERS_TS = 'export const orders = new Map<string, number>();\n';
const PAGE_TS = `${ORDERS_TS}
export function page(offset: number, limit = 50) {
  return [...orders].slice(offset, offset + limit);
}
`;

function file(tree: Tree, path: string): string {
  let node: string | Tree | undefined = tree;
  for (const part of path.split('/')) node = typeof node === 'object' ? node[part] : undefined;
  return typeof node === 'string' ? node : '';
}

const SERVER_TS = file(API, 'src/server.ts');

function demoChanges(p: Project, w: Workspace, mode: ChangesMode): Change[] {
  const uncommitted: Change[] = [];
  const commits: Change[] = [];
  if (w.flags.fixed) {
    uncommitted.push(
      { path: 'src/returns/address.rs', status: 'M', before: ADDRESS_RS, after: ADDRESS_FIXED_RS },
      { path: 'src/returns/mod.rs', status: 'M', before: RETURNS_RS, after: RETURNS_FIXED_RS },
      { path: 'src/returns/tests.rs', status: 'A', before: '', after: RETURNS_TESTS_RS, untracked: true },
    );
  }
  if (w.branch === 'feat/dark-mode') {
    uncommitted.push(
      { path: 'Cargo.lock', status: 'M', before: LOCK, after: LOCK.replace('0.5.0', '0.6.0'), lockfile: true },
      { path: 'src/theme.rs', status: 'M', before: THEME_RS, after: THEME_DARK_RS },
    );
    commits.push({ path: 'README.md', status: 'M', before: README_RS, after: README_DARK });
  }
  if (p.folder === 'api' && w.branch === 'fix/pagination') {
    uncommitted.push({ path: 'src/orders.ts', status: 'M', before: ORDERS_TS, after: PAGE_TS });
    commits.push({ path: 'src/server.ts', status: 'M', before: SERVER_TS, after: SERVER_TS.replace('8080', 'Number(process.env.PORT ?? 8080)') });
  }
  const list = mode === 'uncommitted' ? uncommitted : mode === 'commits' ? commits : [...commits, ...uncommitted];
  return [...list].sort((a, b) => a.path.localeCompare(b.path));
}

const lines = (text: string): string[] => (text ? text.replace(/\n$/, '').split('\n') : []);

type Op = { kind: ' ' | '-' | '+'; a?: number; b?: number };

function ops(a: string[], b: string[]): Op[] {
  const n = a.length;
  const m = b.length;
  const lcs = Array.from({ length: n + 1 }, () => new Array<number>(m + 1).fill(0));
  for (let i = n - 1; i >= 0; i--) for (let j = m - 1; j >= 0; j--) lcs[i][j] = a[i] === b[j] ? lcs[i + 1][j + 1] + 1 : Math.max(lcs[i + 1][j], lcs[i][j + 1]);
  const out: Op[] = [];
  let i = 0;
  let j = 0;
  while (i < n || j < m) {
    if (i < n && j < m && a[i] === b[j]) out.push({ kind: ' ', a: i++, b: j++ });
    else if (j < m && (i >= n || lcs[i][j + 1] >= lcs[i + 1][j])) out.push({ kind: '+', b: j++ });
    else out.push({ kind: '-', a: i++ });
  }
  return out;
}

const tokens = (text: string): string[] => text.match(/[A-Za-z0-9_]+|\s+|./g) ?? [];

function words(oldText: string, newText: string): [[number, number][], [number, number][]] | null {
  const a = tokens(oldText);
  const b = tokens(newText);
  const ranges: [[number, number][], [number, number][]] = [[], []];
  const at = [0, 0];
  for (const op of ops(a, b)) {
    const side = op.kind === '+' ? 1 : 0;
    const token = op.kind === '+' ? b[op.b ?? 0] : a[op.a ?? 0];
    const len = [...token].length;
    if (op.kind === ' ') {
      at[0] += len;
      at[1] += len;
      continue;
    }
    const list = ranges[side];
    const last = list[list.length - 1];
    if (last && last[1] === at[side]) last[1] += len;
    else list.push([at[side], at[side] + len]);
    at[side] += len;
  }
  const changed = [...ranges[0], ...ranges[1]].reduce((n, [s, e]) => n + e - s, 0);
  const total = [...oldText].length + [...newText].length;
  return total > 0 && changed * 10 <= total * 6 ? ranges : null;
}

function emphasize(list: DiffLine[]): void {
  let i = 0;
  while (i < list.length) {
    if (list[i].kind !== '-') {
      i += 1;
      continue;
    }
    const removed = i;
    while (i < list.length && list[i].kind === '-') i += 1;
    const added = i;
    while (i < list.length && list[i].kind === '+') i += 1;
    let next = added;
    for (let r = removed; r < added; r++) {
      for (let a = next; a < i; a++) {
        const pair = words(list[r].text, list[a].text);
        if (!pair) continue;
        list[r].emph = pair[0];
        list[a].emph = pair[1];
        next = a + 1;
        break;
      }
    }
  }
}

export function diffFile(change: Change): FileDiff {
  const a = lines(change.before);
  const b = lines(change.after);
  const all = ops(a, b);
  const lang = language(change.path);
  const changed = all.map((op, i) => (op.kind !== ' ' ? i : -1)).filter((i) => i >= 0);
  const hunks: Hunk[] = [];
  let start = -1;
  let end = -1;
  const flush = () => {
    if (start < 0) return;
    const slice = all.slice(Math.max(0, start - 3), Math.min(all.length, end + 4));
    let oldNo = (slice.find((op) => op.a !== undefined)?.a ?? a.length) + 1;
    let newNo = (slice.find((op) => op.b !== undefined)?.b ?? b.length) + 1;
    const firstOld = oldNo;
    const list: DiffLine[] = slice.map((op) => {
      const text = op.kind === '+' ? b[op.b ?? 0] : a[op.a ?? 0];
      const line: DiffLine = { kind: op.kind, text, syntax: highlight(text, lang), emph: [] };
      if (op.kind !== '+') line.old = oldNo++;
      if (op.kind !== '-') line.new = newNo++;
      return line;
    });
    emphasize(list);
    const context = a
      .slice(0, firstOld - 1)
      .reverse()
      .find((l) => /^[A-Za-z_$]/.test(l));
    const newStart = list.find((l) => l.new !== undefined)?.new ?? newNo;
    hunks.push({ newStart, newEnd: newNo, context: change.before ? (context ?? '') : '', lines: list });
  };
  for (const i of changed) {
    if (start >= 0 && i - end > 6) {
      flush();
      start = -1;
    }
    if (start < 0) start = i;
    end = i;
  }
  flush();
  const count = (kind: string) => all.filter((op) => op.kind === kind).length;
  return { change, hunks, added: count('+'), removed: count('-') };
}

export function workspaceDiff(p: Project, w: Workspace, mode: ChangesMode): FileDiff[] {
  return demoChanges(p, w, mode).map(diffFile);
}

function mix(a: string, b: string, t: number): number {
  const c = (hex: string, i: number) => Number.parseInt(hex.slice(1 + i * 2, 3 + i * 2), 16);
  const ch = (i: number) => Math.round(c(a, i) + (c(b, i) - c(a, i)) * t);
  return rgb(ch(0), ch(1), ch(2));
}

const TINTS = {
  dark: { removed: mix('#0e0d14', '#ff6b8b', 0.16), added: mix('#0e0d14', '#58e6a0', 0.14), removedWord: mix('#0e0d14', '#ff6b8b', 0.38), addedWord: mix('#0e0d14', '#58e6a0', 0.34) },
  light: { removed: mix('#fbfaf6', '#d6336c', 0.12), added: mix('#fbfaf6', '#2b8a3e', 0.13), removedWord: mix('#fbfaf6', '#d6336c', 0.28), addedWord: mix('#fbfaf6', '#2b8a3e', 0.3) },
};

type Row =
  | { kind: 'file'; i: number }
  | { kind: 'gap'; i: number; h: number; n: number }
  | { kind: 'hunk'; i: number; h: number }
  | { kind: 'line'; i: number; h: number; l: number }
  | { kind: 'spacer' };

const DARK: Style = { fg: 8 };

const chars = (text: string, fold: boolean) => [...text].map((c) => (fold ? c.toLowerCase()[0] : c));

function glob(p: string[], t: string[]): boolean {
  if (!p.length) return !t.length;
  const ends = (stop: number) => Array.from({ length: stop + 1 }, (_, i) => i);
  if (p[0] === '*' && p[1] === '*') {
    const rest = p.slice(2);
    const skip = rest[0] === '/' && ends(t.length).some((i) => (i === 0 || t[i - 1] === '/') && glob(rest.slice(1), t.slice(i)));
    return skip || ends(t.length).some((i) => glob(rest, t.slice(i)));
  }
  if (p[0] === '*') {
    const slash = t.indexOf('/');
    return ends(slash < 0 ? t.length : slash).some((i) => glob(p.slice(1), t.slice(i)));
  }
  if (p[0] === '?') return t.length > 0 && t[0] !== '/' && glob(p.slice(1), t.slice(1));
  return t[0] === p[0] && glob(p.slice(1), t.slice(1));
}

function pathMatches(word: string, path: string): boolean {
  const fold = word === word.toLowerCase();
  if (!/[*?]/.test(word)) return chars(path, fold).join('').includes(chars(word, fold).join(''));
  const dir = word.endsWith('/');
  const anchored = word.startsWith('/');
  const pattern = word.slice(anchored ? 1 : 0, dir ? -1 : undefined);
  if (anchored || pattern.includes('/')) {
    const t = chars(path, fold);
    return (!dir && glob(chars(pattern, fold), t)) || glob(chars(`${pattern}/**`, fold), t);
  }
  const parts = path.split('/');
  return parts.some((part, i) => (!dir || i < parts.length - 1) && glob(chars(pattern, fold), chars(part, fold)));
}

export function keptFiles(files: FileDiff[], query: string): Set<number> | null {
  const words = query.split(/\s+/).filter(Boolean);
  if (!words.length) return null;
  const exclude = words.filter((w) => w.length > 1 && w.startsWith('!')).map((w) => w.slice(1));
  const include = words.filter((w) => !(w.length > 1 && w.startsWith('!')));
  const kept = new Set<number>();
  files.forEach((f, i) => {
    const path = f.change.path;
    if ((!include.length || include.some((w) => pathMatches(w, path))) && !exclude.some((w) => pathMatches(w, path))) kept.add(i);
  });
  return kept;
}
const ACTIONS = ['open', 'ask agent', 'copy'] as const;
export type HunkAction = (typeof ACTIONS)[number];

function rows(app: App, files: FileDiff[], kept: Set<number> | null): Row[] {
  const out: Row[] = [];
  files.forEach((f, i) => {
    if (kept && !kept.has(i)) return;
    out.push({ kind: 'file', i });
    if (app.changesFolded(f)) return;
    f.hunks.forEach((hunk, h) => {
      const before = f.hunks[h - 1];
      if (before && hunk.newStart > before.newEnd) out.push({ kind: 'gap', i, h, n: hunk.newStart - before.newEnd });
      out.push({ kind: 'hunk', i, h });
      hunk.lines.forEach((_, l) => out.push({ kind: 'line', i, h, l }));
    });
    out.push({ kind: 'spacer' });
  });
  return out;
}

export function drawPanelBorder(p: Painter, areas: Areas): void {
  const app = p.app;
  const border = areas.changesBorder;
  if (isEmpty(border)) return;
  const lit = (app.dragging?.kind === 'border' && app.dragging.border === 'changes') || p.sidebarHovered(border);
  for (let y = border.y; y < bottom(border); y++) p.g.put(border.x, y, '│', { fg: lit ? 6 : p.lineColour });
  p.region({ r: border, drag: { kind: 'border', border: 'changes' }, double: () => app.resetBorder('changes'), cursor: 'col-resize' });
}

export function drawChanges(p: Painter, areas: Areas): void {
  const app = p.app;
  const g = p.g;
  drawPanelBorder(p, areas);
  const area = areas.changes;
  g.clear(area);
  const inner = rect(area.x + 1, area.y, Math.max(0, area.w - 2), area.h);
  const files = app.changesDiff();
  const tints = app.light ? TINTS.light : TINTS.dark;
  const surface = app.light ? 254 : 236;

  const filter = app.changesFilter;
  const kept = filter ? keptFiles(files, filter.query) : null;

  let x = inner.x;
  for (const mode of CHANGES_MODES) {
    const r = rect(x, inner.y, mode.length + 2, 1);
    const style: Style = mode === app.changesMode ? { fg: 6, bg: surface, add: BOLD } : p.hovered(r) ? { fg: 6 } : { fg: 7 };
    p.span(r.x, r.y, ` ${mode} `, style);
    p.region({ r, click: () => app.setChangesMode(mode), cursor: 'pointer' });
    x = right(r) + 1;
  }
  const close = rect(right(inner) - 3, inner.y, 3, 1);
  p.span(close.x + 1, close.y, '×', p.hovered(close) ? { fg: 1, add: BOLD } : DARK);
  p.region({ r: close, click: () => app.toggleChanges(), cursor: 'pointer' });
  const button = rect(close.x - 3, inner.y, 3, 1);
  p.span(button.x + 1, button.y, '⌕', filter || p.hovered(button) ? { fg: 6 } : DARK);
  p.region({ r: button, click: () => app.openChangesFilter(), cursor: 'pointer' });
  if (filter) filterField(p, rect(inner.x, inner.y + 1, inner.w, 1), filter, surface);

  const sy = inner.y + (filter ? 2 : 1);
  let selector = rect(0, 0, 0, 0);
  if (app.changesMode !== 'uncommitted') {
    const label = ` vs ${app.changesBase} ▾ `;
    selector = rect(right(inner) - [...label].length, sy, [...label].length, 1);
    p.span(selector.x, sy, label, p.hovered(selector) ? { fg: 6 } : { fg: 7 });
    p.region({ r: selector, click: (cx, cy) => app.openChangesBase({ x: cx, y: cy }), cursor: 'pointer' });
  }
  const added = files.reduce((n, f) => n + f.added, 0);
  const removed = files.reduce((n, f) => n + f.removed, 0);
  const noun = files.length === 1 ? 'file' : 'files';
  x = p.span(inner.x, sy, String(kept ? kept.size : files.length), { fg: 15, add: BOLD });
  x = p.span(x, sy, kept ? ` of ${files.length} ${noun}` : ` ${noun}`, { fg: 7 });
  if (added + removed > 0 && !kept) {
    x = p.span(x + 2, sy, `+${added}`, { fg: 2, add: BOLD });
    x = p.span(x + 1, sy, `−${removed}`, { fg: 1, add: BOLD });
    const green = Math.min(8, Math.ceil((added * 8) / (added + removed)));
    x = p.span(x + 2, sy, '▃'.repeat(green), { fg: 2 });
    p.span(x, sy, '▃'.repeat(8 - green), { fg: 1 });
  }
  if (isEmpty(selector)) {
    const lx = p.span(right(inner) - 6, sy, '●', { fg: 2 });
    p.span(lx, sy, ' live', DARK);
  }

  const top = filter ? 4 : 3;
  const body = rect(area.x, area.y + top, area.w, Math.max(0, area.h - top - 2));
  const list = rows(app, files, kept);
  const max = Math.max(0, list.length - body.h);
  const first = Math.min(app.changesScroll, max);
  p.region({ r: body, wheel: (dy) => app.scrollChanges(dy, max) });
  if (!files.length) {
    const text = app.changesMode === 'uncommitted' ? 'no changes' : app.changesMode === 'commits' ? `no commits since ${app.changesBase}` : `nothing changed since ${app.changesBase}`;
    p.span(body.x + 1, body.y, text, DARK);
  } else if (kept && !kept.size) p.span(body.x + 1, body.y, 'no file matches', DARK);
  const shown = list.slice(first, first + body.h).map((row, k) => ({ row, r: rect(body.x, body.y + k, body.w, 1) }));
  const hot = shown.find(({ r }) => p.hovered(r))?.row;
  const hotHunk = hot && (hot.kind === 'hunk' || hot.kind === 'line') ? `${hot.i}:${hot.h}` : null;
  for (const { row, r } of shown) {
    if (row.kind === 'file') fileRow(p, files[row.i], r, surface);
    else if (row.kind === 'gap') {
      const digits = numberWidth(files[row.i]);
      p.span(r.x + 3 + digits * 2, r.y, '↕', { fg: 6 });
      p.span(r.x + 5 + digits * 2, r.y, row.n === 1 ? '1 unchanged line' : `${row.n} unchanged lines`, p.hovered(r) ? { fg: 6 } : DARK);
    } else if (row.kind === 'hunk') hunkRow(p, files[row.i], row.h, r, hotHunk === `${row.i}:${row.h}`);
    else if (row.kind === 'line') codeRow(p, files[row.i], files[row.i].hunks[row.h].lines[row.l], r, tints);
  }

  const sep = rect(inner.x, bottom(area) - 2, inner.w, 1);
  p.span(sep.x, sep.y, '─'.repeat(sep.w), { fg: p.lineColour });
  const foot = bottom(area) - 1;
  if (files.length) {
    const label = files.every((f) => app.changesFolded(f)) ? 'unfold all' : 'fold all';
    const r = rect(inner.x, foot, label.length + 2, 1);
    p.span(r.x, foot, ` ${label} `, p.hovered(r) ? { fg: 0, bg: 6 } : DARK);
    p.region({ r, click: () => app.foldAllChanges(), cursor: 'pointer' });
    const viewed = files.filter((f) => app.changesViewed(f)).length;
    const text = `${viewed} of ${files.length} viewed`;
    p.span(right(inner) - text.length, foot, text, DARK);
  }
}

function filterField(p: Painter, row: Rect, filter: { query: string; focused: boolean }, surface: number): void {
  const app = p.app;
  p.g.fill(row, { bg: surface });
  const clear = rect(right(row) - 3, row.y, 3, 1);
  p.span(clear.x + 1, clear.y, '×', p.hovered(clear) ? { fg: 1, add: BOLD } : DARK);
  p.region({ r: clear, click: () => app.closeChangesFilter(), cursor: 'pointer' });
  const end = clear.x;
  const start = p.span(row.x + 1, row.y, '⌕', { fg: filter.focused ? 6 : 8 }) + 1;
  const room = Math.max(0, end - start - 1);
  if (!filter.query) p.span(start, row.y, truncateRight('path, *.test.js, !*.snap', room), DARK);
  const x = p.span(start, row.y, truncateLeft(filter.query, room), { add: BOLD });
  if (filter.focused && x < end) p.cursor = { x, y: row.y, shape: 'block' };
  p.region({ r: rect(row.x, row.y, end - row.x, 1), click: () => app.openChangesFilter(), cursor: 'text' });
}

function numberWidth(f: FileDiff): number {
  const last = Math.max(0, ...f.hunks.flatMap((h) => h.lines.map((l) => Math.max(l.old ?? 0, l.new ?? 0))));
  return Math.max(3, String(last).length);
}

function fileRow(p: Painter, f: FileDiff, r: Rect, surface: number): void {
  const app = p.app;
  const open = !app.changesFolded(f);
  const viewed = app.changesViewed(f);
  const muted = viewed || !!f.change.lockfile;
  if (open) {
    p.g.fill(r, { bg: surface });
    p.span(r.x + 1, r.y, '▌', { fg: 6 });
  }
  p.span(r.x + 2, r.y, open ? '▾' : '▸', DARK);
  const colour = f.change.status === 'M' ? 3 : f.change.status === 'A' ? 2 : 1;
  p.span(r.x + 4, r.y, f.change.status, muted ? DARK : { fg: colour, add: BOLD });
  const check = rect(right(r) - 3, r.y, 3, 1);
  if (viewed || p.hovered(r)) p.span(check.x + 1, r.y, '✓', viewed || p.hovered(check) ? { fg: 2 } : DARK);
  const tag = f.change.lockfile ? 'lockfile' : f.change.untracked ? 'new' : '';
  const parts: [string, Style][] = [];
  if (tag) parts.push([tag, { fg: 8, add: ITALIC }]);
  if (f.added) parts.push([`+${f.added}`, muted ? DARK : { fg: 2 }]);
  if (f.removed) parts.push([`−${f.removed}`, muted ? DARK : { fg: 1 }]);
  let x = check.x - parts.reduce((n, [t]) => n + [...t].length + 1, 0);
  const pathEnd = x - 1;
  for (const [t, s] of parts) x = p.span(x, r.y, t, s) + 1;
  const path = truncateLeft(f.change.path, Math.max(0, pathEnd - (r.x + 6)));
  const cut = path.lastIndexOf('/') + 1;
  const px = p.span(r.x + 6, r.y, path.slice(0, cut), DARK);
  p.span(px, r.y, path.slice(cut), muted ? DARK : { fg: 15, add: BOLD });
  p.region({ r: rect(r.x, r.y, r.w - 3, 1), click: () => app.toggleChangesFile(f), cursor: 'pointer' });
  p.region({ r: check, click: () => app.toggleChangesViewed(f), cursor: 'pointer' });
}

function hunkRow(p: Painter, f: FileDiff, h: number, r: Rect, hot: boolean): void {
  const hunk = f.hunks[h];
  let end = right(r);
  const buttons: Rect[] = [];
  if (hot) {
    let bx = right(r) - 1;
    for (const label of [...ACTIONS].reverse()) {
      const w = label.length + 2;
      buttons.unshift(rect(bx - w, r.y, w, 1));
      bx -= w + 1;
    }
    end = buttons[0].x;
  }
  let x = r.x + 1;
  if (hunk.context) {
    x = p.span(x, r.y, '┄┄ ', { fg: p.lineColour });
    x = p.span(x, r.y, truncateRight(hunk.context, Math.max(0, end - x - 2)), { fg: 7, add: ITALIC }) + 1;
  }
  p.span(x, r.y, '┄'.repeat(Math.max(0, end - x - 1)), { fg: p.lineColour });
  buttons.forEach((b, k) => {
    const action = ACTIONS[k];
    p.span(b.x, b.y, ` ${action} `, p.hovered(b) ? { fg: 0, bg: 6, add: BOLD } : { fg: 6 });
    p.region({ r: b, click: () => p.app.hunkAction(action, f, h), cursor: 'pointer' });
  });
}

function codeRow(p: Painter, f: FileDiff, line: DiffLine, r: Rect, tints: (typeof TINTS)['dark']): void {
  const tint = line.kind === '-' ? tints.removed : line.kind === '+' ? tints.added : undefined;
  const word = line.kind === '-' ? tints.removedWord : tints.addedWord;
  if (tint !== undefined) p.g.fill(r, { bg: tint });
  const digits = numberWidth(f);
  const num = (n?: number) => (n === undefined ? ' '.repeat(digits) : String(n).padStart(digits));
  let x = p.span(r.x + 1, r.y, num(line.old), DARK);
  x = p.span(x + 1, r.y, num(line.new), DARK);
  if (line.kind !== ' ') p.span(x + 1, r.y, '▎', { fg: line.kind === '-' ? 1 : 2 });
  const start = x + 3;
  const end = right(r) - 1;
  const room = end - start;
  const total = [...line.text].length;
  const limit = total > room ? room - 1 : room;
  let col = start;
  let index = 0;
  for (const s of line.syntax) {
    for (const ch of s.t) {
      if (index >= limit) break;
      const strong = line.emph.some(([a, b]) => index >= a && index < b);
      const bg = strong ? word : tint;
      p.g.put(col, r.y, ch, { ...s.s, ...(bg !== undefined ? { bg } : {}) });
      col += 1;
      index += 1;
    }
  }
  if (total > room) p.span(col, r.y, '…', DARK);
}

export function changesLabel(files: FileDiff[]): string {
  return files.length ? `changes ${files.length}` : 'changes';
}

export const hasChanges = (w: Workspace | undefined): boolean => !!w?.branch;
