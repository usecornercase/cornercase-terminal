import { ASPECT } from '../term/canvas';
import { BOLD, IMAGE_CELL, type Rect, type Style, rect } from '../term/grid';
import type { App } from './app';
import { type FileDiff, TINTS, drawPanelBorder } from './changes';
import { type Picture, type Tree, picture } from './data';
import { highlight, language } from './highlight';
import { type Areas, FILES_LABEL, bottom, right } from './layout';
import { type Project, type Workspace, isModal } from './model';
import { type Line, truncateLeft, truncateRight } from './text';
import type { Painter } from './ui';

export type FilesMode = 'text' | 'name';
export type FileAction = 'open' | 'ask agent' | 'copy';
type Mark = 'added' | 'modified' | 'deleted' | 'above';
type Lines = [number, number];

const ACTIONS: FileAction[] = ['open', 'ask agent', 'copy'];
const IMAGE_ACTIONS: FileAction[] = ['ask agent', 'copy'];
const CELL = { w: 9, h: 9 * ASPECT };
const DARK: Style = { fg: 8 };
const LIT: Style = { fg: 0, bg: 3 };
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

export interface FilesViewer {
  path: string;
  scroll: number;
  selection: Lines | null;
  unfolded: Set<number>;
  find: string | null;
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
    this.selecting = null;
    this.unfocus();
  }

  unfocus(): void {
    for (const place of this.places.values()) place.focused = false;
  }
}

export const wanted = (place: FilesPlace): string => place.query.trim();

export const typing = (place: FilesPlace): boolean => !place.viewer && place.focused;

export function viewFile(place: FilesPlace, path: string, lines: Lines | null = null, find: string | null = null): void {
  place.last = path;
  place.viewer = { path, scroll: lines ? Math.max(0, lines[0] - 4) : 0, selection: lines, unfolded: new Set(), find };
}

export const ordered = ([a, b]: Lines): Lines => [Math.min(a, b), Math.max(a, b)];

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

export function occurrences(text: string, query: string): Lines[] {
  if (!query) return [];
  const fold = !/\p{Lu}/u.test(query);
  const hay = [...(fold ? text.toLowerCase() : text)];
  const needle = [...(fold ? query.toLowerCase() : query)];
  const out: Lines[] = [];
  let i = 0;
  while (i + needle.length <= hay.length) {
    if (needle.every((c, k) => hay[i + k] === c)) {
      out.push([i, i + needle.length]);
      i += needle.length;
    } else i += 1;
  }
  return out;
}

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

function paint(p: Painter, x: number, y: number, end: number, segments: Line, ranges: Lines[], lit: Style): void {
  const room = Math.max(0, end - x);
  const total = segments.reduce((n, s) => n + [...s.t].length, 0);
  const limit = total > room ? room - 1 : room;
  let col = x;
  let index = 0;
  for (const s of segments) {
    for (const ch of s.t) {
      if (index >= limit) break;
      const on = ranges.some(([a, b]) => index >= a && index < b);
      p.g.put(col, y, ch, on ? { ...s.s, ...lit } : (s.s ?? {}));
      col += 1;
      index += 1;
    }
  }
  if (total > room) p.span(col, y, '…', DARK);
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

function panel(app: App, area: Rect): Panel | null {
  const w = app.workspace();
  if (!w) return null;
  return {
    app,
    place: app.files.place(w.id),
    files: app.filesMap(),
    inner: rect(area.x + 1, area.y, Math.max(0, area.w - 2), area.h),
    body: rect(area.x, area.y + HEADER_ROWS, area.w, Math.max(0, area.h - HEADER_ROWS)),
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
  const marks = statuses(app.changesDiff());
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
    const style: Style = { ...(colour === undefined ? {} : { fg: colour }), ...(last ? { add: BOLD } : {}) };
    p.span(x + 2, r.y, truncateRight(row.name, Math.max(0, end - x - 2)), style, Math.max(0, end - x - 2));
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
    if (row.kind === 'name') {
      const [shown, indices] = tail(row.path, row.indices, r.w - 3);
      paint(p, r.x + 2, r.y, right(r) - 1, pathLine(shown, { fg: 15 }), indices.map((n): Lines => [n, n + 1]), { fg: 6, add: BOLD });
    } else if (row.kind === 'file') {
      const count = String(row.count);
      const end = right(r) - count.length - 2;
      paint(p, r.x + 2, r.y, end - 1, pathLine(row.path, { fg: 15, add: BOLD }), [], {});
      p.span(end + 1, r.y, count, DARK);
    } else {
      const x = p.span(r.x + 2, r.y, String(row.n).padStart(5), DARK) + 2;
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
  const end = close.x - 1;
  const start = right(back) + 1;
  const shown = truncateLeft(viewer.path, Math.max(0, end - start));
  const cut = shown.lastIndexOf('/') + 1;
  const px = p.span(start, inner.y, shown.slice(0, cut), DARK, Math.max(0, end - start));
  p.span(px, inner.y, shown.slice(cut), { fg: 15, add: BOLD }, Math.max(0, end - px));

  const info = inner.y + 1;
  const content = f.files.get(viewer.path);
  const pic = picture(content);
  let bx = right(inner) - 1;
  const buttons = [...(pic ? IMAGE_ACTIONS : ACTIONS)].reverse().map((action) => {
    const w = action.length + 2;
    const r = rect(bx - w, info, w, 1);
    bx -= w + 1;
    return { action, r };
  });
  const text = pic ? [] : textLines(content ?? '');
  const sel = viewer.selection && ordered(viewer.selection);
  const room = Math.max(0, Math.min(...buttons.map((b) => b.r.x)) - 1 - (inner.x + 1));
  const [a, b] = sel ?? [0, 0];
  const options = pic ? described(pic) : !sel ? summary(viewer.path, text.length) : a === b ? [`line ${a}`, String(a)] : [`lines ${a}–${b}`, `${a}–${b}`];
  const fit = options.find((o) => [...o].length <= room) ?? options[options.length - 1];
  p.span(inner.x + 1, info, truncateRight(fit, room), sel ? { fg: 6 } : DARK);
  for (const { action, r } of buttons) {
    p.span(r.x, r.y, ` ${action} `, p.hovered(r) ? { fg: 0, bg: 6, add: BOLD } : { fg: 6 });
    p.region({ r, click: () => app.fileAction(action), cursor: 'pointer' });
  }

  if (pic) return drawPicture(p, f, pic);
  const note = content === undefined ? 'this file is gone' : !text.length ? 'an empty file' : null;
  if (note) {
    p.span(body.x + 2, body.y, note, DARK, Math.max(0, body.w - 2));
    return;
  }
  const g = gutter(
    app.changesDiff().find((d) => d.change.path === viewer.path),
    text.length,
  );
  const rows = codeRows(text.length, g, viewer.unfolded);
  const digits = Math.max(3, String(text.length).length);
  const markX = body.x + 2 + digits;
  const max = Math.max(0, rows.length - body.h);
  const first = Math.min(viewer.scroll, max);
  const lang = language(viewer.path);
  const tints = app.light ? TINTS.light : TINTS.dark;
  p.region({ r: body, wheel: (dy) => app.scrollFiles(dy, max) });
  rows.slice(first, first + body.h).forEach((row, k) => {
    const r = rect(body.x, body.y + k, body.w, 1);
    if (row.kind === 'gone') {
      p.g.fill(r, { bg: tints.removed });
      p.span(markX, r.y, '-', { fg: 1 });
      paint(p, markX + 2, r.y, right(r) - 1, [{ t: row.text, s: { fg: 1 } }], [], {});
      p.region({ r, click: () => app.toggleFileBlock(row.key), cursor: 'pointer' });
      return;
    }
    const n = row.n;
    const picked = !!sel && n >= sel[0] && n <= sel[1];
    if (picked) p.g.fill(r, { bg: f.surface });
    const number = rect(r.x + 1, r.y, markX - r.x - 1, 1);
    p.span(r.x + 1, r.y, String(n).padStart(digits), picked || p.hovered(number) ? { fg: 6, add: BOLD } : DARK);
    const mark = g?.marks[n - 1]?.[0];
    if (mark) p.span(markX, r.y, mark === 'deleted' ? '▁' : mark === 'above' ? '▔' : '▎', { fg: mark === 'added' ? 2 : mark === 'modified' ? 4 : 1 });
    const line = text[n - 1];
    paint(p, markX + 2, r.y, right(r) - 1, highlight(line, lang), occurrences(line, viewer.find ?? ''), LIT);
    p.region({ r, click: () => app.selectFileLine(n), cursor: 'pointer' });
    const key = block(g, n);
    if (key !== null) p.region({ r: rect(markX, r.y, 1, 1), click: () => app.toggleFileBlock(key), cursor: 'pointer' });
  });
}

export function lineNear(app: App, y: number): number | null {
  const f = panel(app, app.areas().changes);
  const viewer = f?.place.viewer;
  if (!f || !viewer || !f.body.h) return null;
  const content = f.files.get(viewer.path);
  if (picture(content)) return null;
  const text = textLines(content ?? '');
  const g = gutter(
    app.changesDiff().find((d) => d.change.path === viewer.path),
    text.length,
  );
  const rows = codeRows(text.length, g, viewer.unfolded);
  if (!rows.length) return null;
  const first = Math.min(viewer.scroll, Math.max(0, rows.length - f.body.h));
  const at = Math.min(first + Math.max(0, Math.min(f.body.h - 1, y - f.body.y)), rows.length - 1);
  for (let i = at; i >= 0; i--) {
    const row = rows[i];
    if (row.kind === 'code') return row.n;
  }
  return null;
}

export function selectedText(files: Map<string, string>, viewer: FilesViewer): string {
  if (!viewer.selection) return viewer.path;
  const [a, b] = ordered(viewer.selection);
  return textLines(files.get(viewer.path) ?? '')
    .slice(a - 1, b)
    .join('\n');
}
