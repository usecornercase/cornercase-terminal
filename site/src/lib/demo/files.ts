import { ASPECT } from '../term/canvas';
import { BOLD, IMAGE_CELL, type Rect, type Style, rect } from '../term/grid';
import { DEFAULT } from '../term/palette';
import type { App } from './app';
import { type FileDiff, drawPanelBorder, tintsOf } from './changes';
import { type Picture, type Tree, picture } from './data';
import { type FindState, findAll, nearest, occurrences, start } from './find';
import { SLOT, fileIcon, fileIconAt, folderIcon, iconStyle } from './icons';
import { highlight, language } from './highlight';
import { type Areas, FILES_LABEL, bottom, right } from './layout';
import { type Project, type Workspace, isModal } from './model';
import { type Stretch, type WrapRow, charsAt, endsBefore, on, reverse, textOf, within, wrapRows } from './stretch';
import { type Line, truncateLeft, truncateRight } from './text';
import type { Painter } from './ui';

export type FilesMode = 'text' | 'name';
export type FileAction = 'open' | 'open in…' | 'ask agent' | 'copy';
type Mark = 'added' | 'modified' | 'deleted' | 'above';
type Lines = [number, number];

const ACTIONS: FileAction[] = ['open', 'open in…', 'ask agent', 'copy'];
const IMAGE_ACTIONS: FileAction[] = ['open in…', 'ask agent', 'copy'];
const MIN_INFO = 11;
const CELL = { w: 9, h: 9 * ASPECT };
const DARK: Style = { fg: 8 };
const LIT: Style = { fg: 0, bg: 3 };
const CURRENT: Style = { fg: 0, bg: 6, add: BOLD };
const HEADER_ROWS = 3;
const MAX_NAMES = 200;
const MAX_MATCHES = 1000;
const PATH_CHAR = /^[\p{L}\p{N}_\-./~@+,%:#]$/u;
const TRAILING = '.,:;#';
const LANGUAGES: [RegExp, string][] = [
  [/\.rs$/, 'rust'],
  [/\.[mc]?ts$/, 'typescript'],
  [/\.(tsx|jsx|[mc]?js)$/, 'tsx'],
  [/(\.toml|(^|\/)Cargo\.lock)$/, 'toml'],
  [/\.md$/, 'markdown'],
  [/\.tf$/, 'hcl'],
  [/\.json$/, 'json'],
  [/\.html$/, 'html'],
  [/(^|\/)\.env$/, 'bash'],
  [/(^|\/)\.zshrc$/, 'zsh'],
];

export interface Top {
  line: number;
  row: number;
}

export interface FilesViewer {
  path: string;
  top: Top;
  jump: number | null;
  selection: Lines | null;
  stretch: Stretch<number> | null;
  unfolded: Set<number>;
  find: FindState | null;
}

export interface FilesPlace {
  mode: FilesMode;
  query: string;
  focused: boolean;
  selected: number;
  scroll: number;
  treeScroll: number;
  expanded: Set<string>;
  viewer: FilesViewer | null;
  last: string | null;
}

export class FilesPanel {
  open = false;
  selecting: number | null = null;
  anchor: Stretch<number> | null = null;
  scrolled = -Infinity;
  full = false;
  private places = new Map<number, FilesPlace>();

  place(workspace: number): FilesPlace {
    const known = this.places.get(workspace);
    if (known) return known;
    const place: FilesPlace = { mode: 'text', query: '', focused: false, selected: 0, scroll: 0, treeScroll: 0, expanded: new Set(), viewer: null, last: null };
    this.places.set(workspace, place);
    return place;
  }

  close(): void {
    this.open = false;
    this.full = false;
    this.selecting = null;
    for (const place of this.places.values()) if (place.viewer) place.viewer.find = null;
    this.unfocus();
  }

  unfocus(): void {
    for (const place of this.places.values()) {
      place.focused = false;
      if (place.viewer?.find) place.viewer.find.focused = false;
    }
  }
}

export const wanted = (place: FilesPlace): string => place.query.trim();

export const typing = (place: FilesPlace): boolean => (place.viewer ? !!place.viewer.find?.focused : place.focused);

export function viewFile(place: FilesPlace, path: string, lines: Lines | null = null, find: FindState | null = null): void {
  place.last = path;
  place.viewer = { path, top: { line: 0, row: 0 }, jump: lines ? lines[0] : null, selection: lines, stretch: null, unfolded: new Set(), find };
}

export const ordered = ([a, b]: Lines): Lines => [Math.min(a, b), Math.max(a, b)];
export const compare = (a: number, b: number): number => a - b;

export function coveredLines(viewer: FilesViewer): Lines | null {
  const s = viewer.stretch;
  if (s) return [s.from.line, s.to.line - (endsBefore(compare, s) ? 1 : 0)];
  return viewer.selection && ordered(viewer.selection);
}

export function workspaceFiles(p: Project | undefined, w: Workspace | undefined, all: FileDiff[]): Map<string, string> {
  const out = new Map<string, string>();
  if (!p || !w) return out;
  const walk = (tree: Tree, prefix: string) => {
    for (const [name, node] of Object.entries(tree)) {
      const path = prefix ? `${prefix}/${name}` : name;
      if (typeof node === 'string') out.set(path, node);
      else walk(node, path);
    }
  };
  walk(p.tree, '');
  for (const f of all) {
    if (f.change.status === 'D') out.delete(f.change.path);
    else out.set(f.change.path, f.change.after);
  }
  return out;
}

interface TreeRow {
  path: string;
  name: string;
  depth: number;
  dir: boolean;
  open: boolean;
}

function children(paths: string[], folder: string): { name: string; dir: boolean }[] {
  const prefix = folder ? `${folder}/` : '';
  const seen = new Map<string, boolean>();
  for (const path of paths) {
    if (!path.startsWith(prefix)) continue;
    const rest = path.slice(prefix.length);
    const cut = rest.indexOf('/');
    const name = cut < 0 ? rest : rest.slice(0, cut);
    seen.set(name, (seen.get(name) ?? false) || cut >= 0);
  }
  return [...seen]
    .map(([name, dir]) => ({ name, dir }))
    .sort((a, b) => Number(b.dir) - Number(a.dir) || a.name.toLowerCase().localeCompare(b.name.toLowerCase()));
}

function treeRows(files: Map<string, string>, expanded: Set<string>): TreeRow[] {
  const paths = [...files.keys()];
  const out: TreeRow[] = [];
  const walk = (folder: string, depth: number) => {
    for (const { name, dir } of children(paths, folder)) {
      const path = folder ? `${folder}/${name}` : name;
      const open = dir && expanded.has(path);
      out.push({ path, name, depth, dir, open });
      if (open) walk(path, depth + 1);
    }
  };
  walk('', 0);
  return out;
}

function statuses(diff: FileDiff[]): Map<string, string> {
  const out = new Map<string, string>();
  for (const f of diff) {
    out.set(f.change.path, f.change.status);
    const added = f.change.status === 'A';
    let folder = f.change.path;
    while (folder.includes('/')) {
      folder = folder.slice(0, folder.lastIndexOf('/'));
      out.set(folder, !added || out.get(folder) === 'M' ? 'M' : 'A');
    }
  }
  return out;
}

interface Gutter {
  marks: ([Mark, number] | null)[];
  removed: Map<number, string[]>;
}

function gutter(f: FileDiff | undefined, count: number): Gutter | null {
  if (!f) return null;
  const g: Gutter = { marks: new Array(count).fill(null), removed: new Map() };
  const set = (n: number, mark: Mark, key: number) => {
    if (n >= 1 && n <= count) g.marks[n - 1] = [mark, key];
  };
  if (f.change.status === 'A') {
    for (let n = 1; n <= count; n++) set(n, 'added', n);
    return g;
  }
  for (const hunk of f.hunks) {
    const lines = hunk.lines;
    let next = Math.max(1, hunk.newStart);
    let i = 0;
    while (i < lines.length) {
      if (lines[i].kind === ' ') {
        next = (lines[i].new ?? next - 1) + 1;
        i += 1;
        continue;
      }
      const gone: string[] = [];
      while (i < lines.length && lines[i].kind === '-') gone.push(lines[i++].text);
      const added: number[] = [];
      while (i < lines.length && lines[i].kind === '+') added.push(lines[i++].new ?? 0);
      const key = added[0] ?? next;
      if (!gone.length) for (const n of added) set(n, 'added', key);
      else if (added.length) for (const n of added) set(n, 'modified', key);
      else if (next > 1) set(next - 1, 'deleted', key);
      else set(1, 'above', key);
      if (gone.length) g.removed.set(key, gone);
      if (added.length) next = added[added.length - 1] + 1;
    }
  }
  return g;
}

function block(g: Gutter | null, n: number): number | null {
  const key = g?.marks[n - 1]?.[1];
  return key !== undefined && g?.removed.has(key) ? key : null;
}

type CodeRow = { kind: 'code'; n: number } | { kind: 'gone'; key: number; text: string };

function codeRows(count: number, g: Gutter | null, unfolded: Set<number>): CodeRow[] {
  const out: CodeRow[] = [];
  for (let n = 1; n <= count + 1; n++) {
    const gone = unfolded.has(n) ? g?.removed.get(n) : undefined;
    for (const text of gone ?? []) out.push({ kind: 'gone', key: n, text });
    if (n <= count) out.push({ kind: 'code', n });
  }
  return out;
}

const textLines = (text: string): string[] => (text ? text.replace(/\n$/, '').split('\n') : []);

const binary = (text: string): boolean => text.slice(0, 8000).includes('\u0000');

export { occurrences };

export type Found =
  | { kind: 'name'; path: string; indices: number[] }
  | { kind: 'file'; path: string; count: number }
  | { kind: 'line'; path: string; n: number; text: string; ranges: Lines[] };

function fuzzy(path: string, query: string): { score: number; indices: number[] } | null {
  const fold = !/\p{Lu}/u.test(query);
  const hay = [...(fold ? path.toLowerCase() : path)];
  const letters = [...(fold ? query.toLowerCase() : query).replace(/\s+/g, '')];
  const indices: number[] = [];
  for (let i = 0; i < hay.length && indices.length < letters.length; i++) if (hay[i] === letters[indices.length]) indices.push(i);
  if (indices.length < letters.length) return null;
  const name = path.lastIndexOf('/') + 1;
  let score = 0;
  indices.forEach((i, k) => {
    if (k > 0 && indices[k - 1] === i - 1) score += 3;
    if (i >= name) score += 2;
    if (i === 0 || '/_-.'.includes(hay[i - 1])) score += 2;
  });
  return { score: score - (indices[indices.length - 1] - indices[0]) / 4, indices };
}

function plural(n: number, what: string): string {
  if (n === 1) return `1 ${what}`;
  return what === 'match' ? `${n} matches` : `${n} ${what}s`;
}

function search(files: Map<string, string>, mode: FilesMode, query: string): { rows: Found[]; note: string } {
  const paths = [...files.keys()].sort();
  if (mode === 'name') {
    const rows: Found[] = paths
      .flatMap((path) => {
        const hit = fuzzy(path, query);
        return hit ? [{ path, hit }] : [];
      })
      .sort((a, b) => b.hit.score - a.hit.score || a.path.length - b.path.length)
      .slice(0, MAX_NAMES)
      .map(({ path, hit }) => ({ kind: 'name', path, indices: hit.indices }));
    const note = !rows.length ? 'no file name matches' : rows.length >= MAX_NAMES ? `the best ${MAX_NAMES} files` : plural(rows.length, 'file');
    return { rows, note };
  }
  const rows: Found[] = [];
  let matches = 0;
  let count = 0;
  for (const path of paths) {
    const hits: Found[] = [];
    const content = files.get(path) ?? '';
    if (binary(content)) continue;
    textLines(content).forEach((text, i) => {
      const shown = text.replace(/\t/g, '    ').trimStart();
      const ranges = occurrences(shown, query);
      if (!ranges.length || matches >= MAX_MATCHES) return;
      hits.push({ kind: 'line', path, n: i + 1, text: shown, ranges });
      matches += 1;
    });
    if (!hits.length) continue;
    count += 1;
    rows.push({ kind: 'file', path, count: hits.length }, ...hits);
  }
  const first = matches >= MAX_MATCHES ? `the first ${MAX_MATCHES} matches` : plural(matches, 'match');
  return { rows, note: matches ? `${first} in ${plural(count, 'file')}` : 'nothing found' };
}

export const foundRows = (files: Map<string, string>, place: FilesPlace): Found[] => search(files, place.mode, wanted(place)).rows;

export const selectable = (rows: Found[]): number[] => rows.flatMap((r, i) => (r.kind === 'file' ? [] : [i]));

export interface PathLink {
  start: number;
  end: number;
  path: string;
  lines: Lines | null;
}

const number = (text: string): number | null => (/^\d+$/.test(text) ? Number(text) : null);

function location(rest: string): Lines | null {
  const cut = rest.search(/[-:]/);
  const first = number(cut < 0 ? rest : rest.slice(0, cut));
  if (first === null) return null;
  if (cut < 0 || rest[cut] !== '-') return [first, first];
  const last = number(rest.slice(cut + 1).replace(/^L+/, ''));
  return last === null ? null : [first, Math.max(first, last)];
}

function split(token: string): [string, Lines | null] {
  const hash = token.lastIndexOf('#L');
  if (hash >= 0) return [token.slice(0, hash), location(token.slice(hash + 2))];
  const name = token.lastIndexOf('/') + 1;
  const colon = token.indexOf(':', name);
  const lines = colon >= 0 ? location(token.slice(colon + 1)) : null;
  return lines ? [token.slice(0, colon), lines] : [token, null];
}

export function linkAt(row: string[], col: number): PathLink | null {
  const path = (i: number) => PATH_CHAR.test(row[i] ?? '');
  if (!path(col)) return null;
  let start = col;
  while (start > 0 && path(start - 1)) start -= 1;
  let end = col + 1;
  while (end < row.length && path(end)) end += 1;
  let token = row.slice(start, end).join('');
  while (token && TRAILING.includes(token[token.length - 1])) {
    token = token.slice(0, -1);
    end -= 1;
  }
  if (col >= end || token.includes('://')) return null;
  const [found, lines] = split(token);
  if (!found || /^[./]+$/.test(found)) return null;
  return { start, end, path: found, lines };
}

function normalize(parts: string[]): string {
  const out: string[] = [];
  for (const part of parts.flatMap((p) => p.split('/'))) {
    if (part === '..') out.pop();
    else if (part && part !== '.') out.push(part);
  }
  return out.join('/');
}

export function resolveLink(path: string, cwd: string[], root: string, files: Map<string, string>): string | null {
  const folder = absolutePath(root);
  const given = path.startsWith('~/') ? absolutePath(path) : path;
  const candidates = given.startsWith('/') ? [absolutePath(given)] : [absolutePath([folder, ...cwd, given].join('/')), absolutePath(`${folder}/${given}`)];
  const found = candidates.find((c) => files.has(c));
  return found?.startsWith(`${folder}/`) ? found.slice(folder.length + 1) : found ?? null;
}

export function absolutePath(path: string): string {
  return `/${normalize([path.replace(/^~(?=\/|$)/, '/home/you')])}`;
}

function languageName(path: string): string | null {
  return LANGUAGES.find(([re]) => re.test(path))?.[1] ?? null;
}

function tail(path: string, indices: number[], room: number): [string, number[]] {
  const chars = [...path];
  if (chars.length <= room || room < 2) return [path, indices];
  const skip = chars.length - room + 1;
  return [`…${chars.slice(skip).join('')}`, indices.filter((i) => i >= skip).map((i) => i - skip + 1)];
}

function paint(p: Painter, x: number, y: number, end: number, segments: Line, ranges: Lines[], lit: Style, current: Lines | null = null, row?: WrapRow): void {
  const room = Math.max(0, end - x);
  const shown = row ?? wrapRows(segments.map((s) => s.t).join(''), room, 1)[0];
  let col = x;
  let index = 0;
  for (const s of segments) {
    for (const ch of s.t) {
      if (index >= shown.to) break;
      if (index < shown.from) {
        index += 1;
        continue;
      }
      const now = !!current && index >= current[0] && index < current[1];
      const on = ranges.some(([a, b]) => index >= a && index < b);
      p.g.put(col, y, ch, now ? { ...s.s, ...CURRENT } : on ? { ...s.s, ...lit } : (s.s ?? {}));
      col += 1;
      index += 1;
    }
  }
  if (shown.end === 'cut') p.span(col, y, '…', DARK);
}

function pathLine(path: string, name: Style): Line {
  const cut = path.lastIndexOf('/') + 1;
  return [
    { t: path.slice(0, cut), s: DARK },
    { t: path.slice(cut), s: name },
  ];
}

interface Panel {
  app: App;
  place: FilesPlace;
  files: Map<string, string>;
  inner: Rect;
  body: Rect;
  surface: number;
  hover: number;
}

export function fieldShown(viewer: FilesViewer | null | undefined, files: Map<string, string>): boolean {
  if (!viewer?.find) return false;
  const content = files.get(viewer.path);
  return content !== undefined && !picture(content);
}

function panel(app: App, area: Rect): Panel | null {
  const w = app.workspace();
  if (!w) return null;
  const place = app.files.place(w.id);
  const files = app.filesMap();
  const head = HEADER_ROWS + (fieldShown(place.viewer, files) ? 1 : 0);
  return {
    app,
    place,
    files,
    inner: rect(area.x + 1, area.y, Math.max(0, area.w - 2), area.h),
    body: rect(area.x, area.y + head, area.w, Math.max(0, area.h - head)),
    surface: app.light ? 254 : 236,
    hover: app.light ? 255 : 235,
  };
}

export function drawFiles(p: Painter, areas: Areas): void {
  const app = p.app;
  drawPanelBorder(p, areas);
  p.g.clear(areas.changes);
  const f = panel(app, areas.changes);
  if (!f) return;
  const close = rect(right(f.inner) - 3, f.inner.y, 3, 1);
  const viewer = f.place.viewer;
  if (viewer) drawViewer(p, f, viewer, close);
  else {
    drawTitle(p, f, close);
    drawBar(p, f);
    const query = wanted(f.place);
    if (query) {
      const { rows, note } = search(f.files, f.place.mode, query);
      p.span(f.inner.x + 1, f.inner.y + 2, note, DARK, Math.max(0, f.inner.w - 1));
      drawFound(p, f, rows);
    } else drawTree(p, f);
  }
  p.span(close.x + 1, close.y, '×', p.hovered(close) ? { fg: 1, add: BOLD } : DARK);
  p.region({ r: close, click: () => app.toggleFiles(), cursor: 'pointer' });
}

function drawTitle(p: Painter, f: Panel, close: Rect): void {
  const end = close.x - 1;
  const x = p.span(f.inner.x, f.inner.y, ` ${FILES_LABEL} `, { fg: 6, bg: f.surface, add: BOLD }, Math.max(0, end - f.inner.x)) + 1;
  p.span(x, f.inner.y, truncateLeft(f.app.workspace()?.root ?? '', Math.max(0, end - x)), DARK, Math.max(0, end - x));
}

function drawBar(p: Painter, f: Panel): void {
  const { app, place, surface } = f;
  const row = rect(f.inner.x, f.inner.y + 1, f.inner.w, 1);
  p.g.fill(row, { bg: surface });
  const mode = rect(right(row) - 3, row.y, 3, 1);
  const clear = rect(mode.x - 3, row.y, 3, 1);
  const names = place.mode === 'name';
  p.region({ r: rect(row.x, row.y, mode.x - row.x, 1), click: () => app.focusFilesBar(), cursor: 'text' });
  p.span(mode.x, mode.y, ' ▤ ', names ? { fg: 0, bg: 6, add: BOLD } : p.hovered(mode) ? { fg: 6 } : DARK);
  p.region({ r: mode, click: () => app.toggleFilesMode(), cursor: 'pointer' });
  if (place.query) {
    p.span(clear.x + 1, clear.y, '×', p.hovered(clear) ? { fg: 1, add: BOLD } : DARK);
    p.region({ r: clear, click: () => app.editFilesQuery(''), cursor: 'pointer' });
  }
  const start = p.span(row.x + 1, row.y, '⌕', { fg: place.focused ? 6 : 8 }) + 1;
  const room = Math.max(0, clear.x - start - 1);
  if (!place.query) p.span(start, row.y, truncateRight(names ? 'file names' : 'text in the files', room), DARK, Math.max(0, clear.x - start));
  const end = p.span(start, row.y, truncateLeft(place.query, room), { add: BOLD }, Math.max(0, clear.x - start));
  if (place.focused && !app.overlay && end < clear.x) p.cursor = { x: end, y: row.y, shape: 'block' };
}

function drawTree(p: Painter, f: Panel): void {
  const { app, place, body } = f;
  const rows = treeRows(f.files, place.expanded);
  if (!rows.length) {
    p.span(body.x + 2, body.y, 'no files here', DARK, Math.max(0, body.w - 2));
    return;
  }
  const marks = statuses(app.activeChangesDiff());
  const max = Math.max(0, rows.length - body.h);
  const first = Math.min(place.treeScroll, max);
  p.region({ r: body, wheel: (dy) => app.scrollFiles(dy, max) });
  rows.slice(first, first + body.h).forEach((row, k) => {
    const r = rect(body.x, body.y + k, body.w, 1);
    const last = place.last === row.path;
    if (last) {
      p.g.fill(r, { bg: f.surface });
      p.span(r.x, r.y, '▌', { fg: 6 });
    } else if (p.hovered(r)) p.g.fill(r, { bg: f.hover });
    const x = r.x + 2 + 2 * row.depth;
    if (row.dir) p.span(x, r.y, row.open ? '▾' : '▸', DARK);
    const status = marks.get(row.path);
    const colour = status === 'M' ? 3 : status === 'A' ? 2 : status === 'D' ? 1 : undefined;
    const end = right(r) - 4;
    const slot = app.config.icons === 'off' ? 0 : SLOT;
    if (slot) {
      const icon = row.dir ? folderIcon(row.open) : fileIcon(row.name);
      p.span(x + 2, r.y, icon.glyph, iconStyle(icon, DARK), Math.max(0, end - x - 2));
    }
    const style: Style = { ...(colour === undefined ? {} : { fg: colour }), ...(last ? { add: BOLD } : {}) };
    const room = Math.max(0, end - x - 2 - slot);
    p.span(x + 2 + slot, r.y, truncateRight(row.name, room), style, room);
    if (status) p.span(end + 1, r.y, row.dir ? '●' : status, { fg: colour });
    p.region({ r, click: () => (row.dir ? app.toggleFilesFolder(row.path) : app.showFile(row.path)), cursor: 'pointer' });
  });
}

function drawFound(p: Painter, f: Panel, rows: Found[]): void {
  const { app, place, body } = f;
  const selected = selectable(rows)[place.selected];
  const max = Math.max(0, rows.length - body.h);
  const first = Math.min(place.scroll, max);
  p.region({ r: body, wheel: (dy) => app.scrollFiles(dy, max) });
  rows.slice(first, first + body.h).forEach((row, k) => {
    const i = first + k;
    const r = rect(body.x, body.y + k, body.w, 1);
    if (i === selected) {
      p.g.fill(r, { bg: f.surface });
      p.span(r.x, r.y, '▌', { fg: 6 });
    } else if (row.kind !== 'file' && p.hovered(r)) p.g.fill(r, { bg: f.hover });
    const slot = app.config.icons === 'off' ? 0 : SLOT;
    const mark = (path: string, end: number) => {
      if (slot) {
        const icon = fileIconAt(path);
        p.span(r.x + 2, r.y, icon.glyph, iconStyle(icon, DARK), Math.max(0, end - r.x - 2));
      }
    };
    if (row.kind === 'name') {
      mark(row.path, right(r) - 1);
      const [shown, indices] = tail(row.path, row.indices, r.w - 3 - slot);
      paint(p, r.x + 2 + slot, r.y, right(r) - 1, pathLine(shown, { fg: DEFAULT }), indices.map((n): Lines => [n, n + 1]), { fg: 6, add: BOLD });
    } else if (row.kind === 'file') {
      const count = String(row.count);
      const end = right(r) - count.length - 2;
      mark(row.path, end - 1);
      paint(p, r.x + 2 + slot, r.y, end - 1, pathLine(row.path, { fg: DEFAULT, add: BOLD }), [], {});
      p.span(end + 1, r.y, count, DARK);
    } else {
      const x = p.span(r.x + 2 + slot, r.y, String(row.n).padStart(5), DARK) + 2;
      const room = right(r) - x - 1;
      const hit = row.ranges[0];
      const skip = hit && hit[1] > room ? Math.max(0, hit[0] - 8) : 0;
      const text = [...row.text].slice(skip).join('');
      const ranges = row.ranges.map(([a, b]): Lines => [Math.max(0, a - skip), Math.max(0, b - skip)]);
      paint(p, x, r.y, right(r) - 1, [{ t: text }], ranges, LIT);
    }
    if (row.kind !== 'file') p.region({ r, click: () => app.openFound(i), cursor: 'pointer' });
  });
}

function size(bytes: number): string {
  if (bytes < 1024) return `${bytes} bytes`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

function described(pic: Picture): string[] {
  const pixels = `${pic.width}×${pic.height}`;
  return [`${pic.format} · ${pixels} · ${size(pic.bytes)}`, `${pic.format} · ${pixels}`, pic.format];
}

function fitImage(width: number, height: number, cols: number, rows: number): { cols: number; rows: number } | null {
  if (cols <= 0 || rows <= 0 || !width || !height) return null;
  const scale = Math.min(1, (cols * CELL.w) / width, (rows * CELL.h) / height);
  return { cols: Math.max(1, Math.ceil(Math.round(width * scale) / CELL.w)), rows: Math.max(1, Math.ceil(Math.round(height * scale) / CELL.h)) };
}

function drawPicture(p: Painter, f: Panel, pic: Picture): void {
  const { app, body } = f;
  if (app.overlay && isModal(app.overlay)) return;
  const rows = bottom(body) >= app.rows ? body.h - 1 : body.h;
  const room = rect(body.x + 1, body.y, body.w - 2, rows);
  const cells = fitImage(pic.width, pic.height, room.w, room.h);
  if (!cells) return;
  const r = rect(room.x + Math.floor((room.w - cells.cols) / 2), room.y, cells.cols, cells.rows);
  p.g.fill(r, {}, IMAGE_CELL);
  p.g.images.push({ r, src: pic.src, width: pic.width, height: pic.height });
}

function summary(path: string, count: number): string[] {
  const lines = count === 1 ? '1 line' : `${count} lines`;
  const name = languageName(path);
  return name ? [`${lines} · ${name}`, lines, String(count)] : [lines, String(count)];
}

function drawViewer(p: Painter, f: Panel, viewer: FilesViewer, close: Rect): void {
  const { app, inner, body } = f;
  const back = rect(inner.x, inner.y, 3, 1);
  p.span(back.x, back.y, ' ‹ ', p.hovered(back) ? { fg: 0, bg: 6, add: BOLD } : { fg: 6 });
  p.region({ r: back, click: () => app.closeFile(), cursor: 'pointer' });
  const full = rect(close.x - 3, inner.y, 3, 1);
  const fits = full.x >= right(back);
  const content = f.files.get(viewer.path);
  const pic = picture(content);
  const finder = rect(full.x - 3, inner.y, 3, 1);
  const finds = fits && content !== undefined && !pic && finder.x >= right(back);
  if (finds) {
    p.span(finder.x + 1, finder.y, '⌕', viewer.find || p.hovered(finder) ? { fg: 6 } : DARK);
    p.region({ r: finder, click: () => app.openFind(), cursor: 'pointer' });
  }
  if (fits) {
    const on = app.fileFullScreen();
    p.span(full.x + 1, full.y, on ? '❐' : '▢', on || p.hovered(full) ? { fg: 6 } : DARK);
    p.region({ r: full, click: () => app.toggleFullFile(), cursor: 'pointer' });
  }
  const end = finds ? finder.x - 1 : fits ? full.x - 1 : 0;
  const start = right(back) + 1;
  const shown = truncateLeft(viewer.path, Math.max(0, end - start));
  const cut = shown.lastIndexOf('/') + 1;
  const px = p.span(start, inner.y, shown.slice(0, cut), DARK, Math.max(0, end - start));
  p.span(px, inner.y, shown.slice(cut), { fg: DEFAULT, add: BOLD }, Math.max(0, end - px));

  const info = inner.y + 1;
  const place = (actions: FileAction[]) => {
    let bx = right(inner) - 1;
    return [...actions].reverse().map((action) => {
      const w = action.length + 2;
      const r = rect(bx - w, info, w, 1);
      bx -= w + 1;
      return { action, r };
    });
  };
  const all = pic ? IMAGE_ACTIONS : ACTIONS;
  const wide = place(all);
  const buttons = Math.min(...wide.map((b) => b.r.x)) - inner.x - 2 >= MIN_INFO ? wide : place(all.filter((a) => a !== 'open in…'));
  const text = pic ? [] : textLines(content ?? '');
  const sel = coveredLines(viewer);
  const room = Math.max(0, Math.min(...buttons.map((b) => b.r.x)) - 1 - (inner.x + 1));
  const [a, b] = sel ?? [0, 0];
  const options = pic ? described(pic) : !sel ? summary(viewer.path, text.length) : a === b ? [`line ${a}`, String(a)] : [`lines ${a}-${b}`, `${a}-${b}`];
  const fit = options.find((o) => [...o].length <= room) ?? options[options.length - 1];
  p.span(inner.x + 1, info, truncateRight(fit, room), sel ? { fg: 6 } : DARK);
  for (const { action, r } of buttons) {
    p.span(r.x, r.y, ` ${action} `, p.hovered(r) ? { fg: 0, bg: 6, add: BOLD } : { fg: 6 });
    p.region({ r, click: () => app.fileAction(action), cursor: 'pointer' });
  }

  if (pic) return drawPicture(p, f, pic);
  const hits = viewer.find ? findAll(textLines(content ?? ''), viewer.find.query) : [];
  const now = viewer.find ? nearest(hits, viewer.find.at) : null;
  if (viewer.find && content !== undefined) drawFind(p, f, viewer.find, hits, now);
  const note = content === undefined ? 'this file is gone' : !text.length ? 'an empty file' : null;
  if (note) {
    p.span(body.x + 2, body.y, note, DARK, Math.max(0, body.w - 2));
    return;
  }
  const g = gutter(
    app.activeChangesDiff().find((d) => d.change.path === viewer.path),
    text.length,
  );
  const digits = Math.max(3, String(text.length).length);
  const markX = body.x + 2 + digits;
  const codeX = markX + 2;
  const { rows, visual, first, max } = lay(f, viewer, text, g);
  const lang = language(viewer.path);
  const tints = tintsOf(app);
  p.region({ r: body, wheel: (dy) => app.scrollFiles(dy, max) });
  visual.slice(first, first + body.h).forEach(({ i, k, fit }, y) => {
    const row = rows[i];
    const r = rect(body.x, body.y + y, body.w, 1);
    if (row.kind === 'gone') {
      p.g.fill(r, { bg: tints.removed });
      if (k === 0) p.span(markX, r.y, '-', { fg: 1 });
      paint(p, codeX, r.y, right(r) - 1, [{ t: row.text, s: { fg: 1 } }], [], {}, null, fit);
      p.region({ r, click: () => app.toggleFileBlock(row.key), cursor: 'pointer' });
      return;
    }
    const n = row.n;
    const picked = !viewer.stretch && !!sel && n >= sel[0] && n <= sel[1];
    if (picked) p.g.fill(r, { bg: f.surface });
    const number = rect(r.x + 1, r.y, markX - r.x - 1, 1);
    if (k === 0) p.span(r.x + 1, r.y, String(n).padStart(digits), picked || p.hovered(number) ? { fg: 6, add: BOLD } : DARK);
    const mark = g?.marks[n - 1]?.[0];
    const glyph = mark === 'deleted' ? (fit.end !== 'wrap' ? '▁' : null) : mark === 'above' ? (k === 0 ? '▔' : null) : mark ? '▎' : null;
    if (mark && glyph) p.span(markX, r.y, glyph, { fg: mark === 'added' ? 2 : mark === 'modified' ? 4 : 1 });
    const line = text[n - 1];
    const here = hits.filter((h) => h.line === n);
    const current = now !== null && hits[now].line === n ? ([hits[now].a, hits[now].b] as Lines) : null;
    paint(p, codeX, r.y, right(r) - 1, highlight(line, lang), here.map((h): Lines => [h.a, h.b]), LIT, current, fit);
    if (viewer.stretch) {
      const lit = on(compare, viewer.stretch, n, [...line].length);
      if (lit) reverse(p.g, { x: codeX, y: r.y, w: right(r) - codeX }, line, fit, lit);
    }
    p.region({ r, click: (cx, cy) => app.selectFileLine(n, cx >= codeX ? { x: cx, y: cy } : null), cursor: 'pointer' });
    const key = block(g, n);
    if (key !== null) p.region({ r: rect(markX, r.y, 1, 1), click: () => app.toggleFileBlock(key), cursor: 'pointer' });
  });
}

interface Visual {
  i: number;
  k: number;
  fit: WrapRow;
}

interface Shown {
  body: Rect;
  rows: CodeRow[];
  visual: Visual[];
  first: number;
  max: number;
  codeX: number;
}

const CONTEXT = 3;

function visualIndex(visual: Visual[], i: number, column: number): number {
  const mine = visual.map((v, at) => ({ v, at })).filter(({ v }) => v.i === i);
  return (mine.find(({ v }) => column < v.fit.to) ?? mine[mine.length - 1])?.at ?? 0;
}

function lay(f: Panel, viewer: FilesViewer, text: string[], g: Gutter | null): Shown {
  const { body } = f;
  const rows = codeRows(text.length, g, viewer.unfolded);
  const codeX = body.x + 2 + Math.max(3, String(text.length).length) + 2;
  const room = Math.max(0, right(body) - codeX - 1);
  const visual = rows.flatMap((row, i) => wrapRows(row.kind === 'code' ? text[row.n - 1] : row.text, room).map((fit, k) => ({ i, k, fit })));
  const max = Math.max(0, visual.length - body.h);
  let at = 0;
  if (viewer.jump !== null) {
    const hits = viewer.find ? findAll(text, viewer.find.query) : [];
    const now = viewer.find ? nearest(hits, viewer.find.at) : null;
    const line = rows.findIndex((r) => r.kind === 'code' && r.n === viewer.jump);
    const column = now !== null && hits[now].line === viewer.jump ? hits[now].a : 0;
    at = Math.max(0, visualIndex(visual, line < 0 ? rows.length - 1 : line, column) - CONTEXT);
  } else if (rows.length) {
    const i = Math.min(viewer.top.line, rows.length - 1);
    const mine = visual.filter((v) => v.i === i);
    at = visual.findIndex((v) => v.i === i && v.k === Math.min(viewer.top.row, mine.length - 1));
  }
  return { body, rows, visual, first: Math.min(Math.max(0, at), max), max, codeX };
}

const topOf = (visual: Visual[], at: number): Top => ({ line: visual[at]?.i ?? 0, row: visual[at]?.k ?? 0 });

function drawFind(p: Painter, f: Panel, find: FindState, hits: { line: number }[], now: number | null): void {
  const { app, inner } = f;
  const row = rect(inner.x, inner.y + 2, inner.w, 1);
  p.g.fill(row, { bg: f.surface });
  const close = rect(right(row) - 3, row.y, 3, 1);
  const next = rect(close.x - 3, row.y, 3, 1);
  const prev = rect(next.x - 3, row.y, 3, 1);
  p.region({ r: row, click: () => app.openFind(), cursor: 'text' });
  p.span(close.x + 1, close.y, '×', p.hovered(close) ? { fg: 1, add: BOLD } : DARK);
  p.region({ r: close, click: () => app.closeFind(), cursor: 'pointer' });
  for (const [r, glyph, delta] of [[next, '↓', 1], [prev, '↑', -1]] as [Rect, string, number][]) {
    p.span(r.x + 1, r.y, glyph, p.hovered(r) ? { fg: 6 } : DARK);
    p.region({ r, click: () => app.stepFind(delta), cursor: 'pointer' });
  }
  const count = now !== null ? `${now + 1}/${hits.length}` : find.query.trim() ? 'no matches' : '';
  const end = prev.x - [...count].length - (count ? 1 : 0);
  p.span(end, row.y, count, DARK, Math.max(0, prev.x - end));
  const start = p.span(row.x + 1, row.y, '⌕', { fg: find.focused ? 6 : 8 }) + 1;
  const room = Math.max(0, end - start - 1);
  if (!find.query) p.span(start, row.y, truncateRight('text in this file', room), DARK, Math.max(0, end - start));
  const last = p.span(start, row.y, truncateLeft(find.query, room), { add: BOLD }, Math.max(0, end - start));
  if (find.focused && !f.app.overlay && last < end) p.cursor = { x: last, y: row.y, shape: 'block' };
}

export const fileLines = (files: Map<string, string>, viewer: FilesViewer): string[] => textLines(files.get(viewer.path) ?? '');

export function topSpot(app: App): [number, number] {
  const shown = shownRows(app);
  const top = shown?.visual[shown.first];
  if (!shown || !top) return [1, 0];
  const i = shown.rows.findIndex((r, at) => at >= top.i && r.kind === 'code');
  const row = shown.rows[i];
  return row?.kind === 'code' ? [row.n, i === top.i ? top.fit.from : 0] : [1, 0];
}

export function reveal(app: App, line: number, column: number): Top | null {
  const shown = shownRows(app);
  if (!shown) return null;
  const i = shown.rows.findIndex((r) => r.kind === 'code' && r.n === line);
  if (i < 0) return null;
  const at = visualIndex(shown.visual, i, column);
  if (at >= shown.first && at < shown.first + shown.body.h) return null;
  return topOf(shown.visual, Math.max(0, Math.min(shown.max, at - CONTEXT)));
}

export function scrolledTop(app: App, dy: number): Top | null {
  const shown = shownRows(app);
  return shown && topOf(shown.visual, Math.max(0, Math.min(shown.max, shown.first + dy)));
}

function shownRows(app: App): Shown | null {
  const f = panel(app, app.areas().changes);
  const viewer = f?.place.viewer;
  if (!f || !viewer || !f.body.h) return null;
  const content = f.files.get(viewer.path);
  if (picture(content)) return null;
  const text = textLines(content ?? '');
  const g = gutter(
    app.activeChangesDiff().find((d) => d.change.path === viewer.path),
    text.length,
  );
  return lay(f, viewer, text, g);
}

function visualUnder(shown: Shown, y: number): Visual | undefined {
  const { body, visual, first } = shown;
  return visual[Math.min(first + Math.max(0, Math.min(body.h - 1, y - body.y)), visual.length - 1)];
}

export function lineNear(app: App, y: number): number | null {
  const shown = shownRows(app);
  const under = shown && visualUnder(shown, y);
  if (!shown || !under) return null;
  for (let i = under.i; i >= 0; i--) {
    const row = shown.rows[i];
    if (row.kind === 'code') return row.n;
  }
  return null;
}

export function fileMax(app: App): number | null {
  return shownRows(app)?.max ?? null;
}

export function lineEdge(app: App, y: number): { delta: number; max: number } | null {
  const shown = shownRows(app);
  if (!shown) return null;
  const { body, first, max } = shown;
  if (y <= body.y && first > 0) return { delta: -1, max };
  if (y >= bottom(body) - 1 && first < max) return { delta: 1, max };
  return null;
}

export function stretchNear(app: App, x: number, y: number): Stretch<number> | null {
  const shown = shownRows(app);
  const f = panel(app, app.areas().changes);
  const content = f?.files.get(f.place.viewer?.path ?? '');
  const under = shown && visualUnder(shown, y);
  if (!shown || !under || content === undefined) return null;
  const { rows, codeX } = shown;
  const text = textLines(content);
  const row = rows[under.i];
  if (row.kind === 'code') {
    const [a, b] = x < codeX ? [under.fit.from, under.fit.from] : charsAt(text[row.n - 1], under.fit, x - codeX);
    return within(row.n, a, b);
  }
  for (let i = under.i; i >= 0; i--) {
    const above = rows[i];
    if (above.kind === 'code') return within(above.n, [...text[above.n - 1]].length, [...text[above.n - 1]].length);
  }
  const next = rows.find((r) => r.kind === 'code');
  return next?.kind === 'code' ? within(next.n, 0, 0) : null;
}

export function selectedText(files: Map<string, string>, viewer: FilesViewer): string {
  const content = files.get(viewer.path);
  if (content === undefined || picture(content)) return viewer.path;
  const lines = textLines(content);
  const s = viewer.stretch;
  if (s && s.from.line >= 1 && s.to.line <= lines.length) return textOf(s, lines.slice(s.from.line - 1, s.to.line));
  if (!viewer.selection) return content;
  const [a, b] = ordered(viewer.selection);
  return textLines(content).slice(a - 1, b).join('\n');
}
