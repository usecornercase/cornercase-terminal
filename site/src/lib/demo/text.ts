import type { Grid, Rect, Style } from '../term/grid';

export interface Seg {
  t: string;
  s?: Style;
}

export type Line = Seg[];

export interface Cell {
  ch: string;
  s?: Style;
}

export const seg = (t: string, s?: Style): Seg => ({ t, s });

export const plain = (t: string): Line => [{ t }];

export function width(line: Line): number {
  let n = 0;
  for (const s of line) n += [...s.t].length;
  return n;
}

export function wrap(line: Line, w: number): Cell[][] {
  const rows: Cell[][] = [[]];
  if (w <= 0) return rows;
  for (const s of line) {
    for (const ch of s.t) {
      if (rows[rows.length - 1].length >= w) rows.push([]);
      rows[rows.length - 1].push({ ch, s: s.s });
    }
  }
  return rows;
}

export function wrapAll(lines: Line[], w: number): Cell[][] {
  const out: Cell[][] = [];
  for (const line of lines) out.push(...wrap(line, w));
  return out;
}

export function wrapWords(line: Line, w: number, indent = 0): Cell[][] {
  if (w <= 0) return [[]];
  const lead = Math.min(Math.max(0, indent), w - 1);
  const cells: Cell[] = [];
  for (const s of line) for (const ch of s.t) cells.push({ ch, s: s.s });
  const rows: Cell[][] = [];
  let row: Cell[] = [];
  let start = 0;
  const flush = () => {
    rows.push(row);
    row = Array.from({ length: lead }, () => ({ ch: ' ' }));
    start = lead;
  };
  let i = 0;
  while (i < cells.length) {
    let j = i;
    while (j < cells.length && cells[j].ch !== ' ') j += 1;
    if (j === i) {
      if (row.length < w) row.push(cells[i]);
      i += 1;
      continue;
    }
    if (row.length + (j - i) > w && row.length > start) flush();
    for (let k = i; k < j; k += 1) {
      if (row.length >= w) flush();
      row.push(cells[k]);
    }
    i = j;
  }
  rows.push(row);
  return rows;
}

export function paintRows(g: Grid, r: Rect, rows: Cell[][], top = 0): void {
  for (let i = 0; i < r.h; i++) {
    const row = rows[top + i];
    if (!row) continue;
    for (let x = 0; x < Math.min(row.length, r.w); x++) {
      g.put(r.x + x, r.y + i, row[x].ch, row[x].s ?? {});
    }
  }
}

export function drawLine(g: Grid, x: number, y: number, line: Line, max: number): number {
  let col = x;
  for (const s of line) {
    for (const ch of s.t) {
      if (col - x >= max) return col;
      g.put(col, y, ch, s.s ?? {});
      col += 1;
    }
  }
  return col;
}

export function truncateRight(s: string, max: number): string {
  const chars = [...s];
  if (max <= 0) return '';
  if (chars.length <= max || max < 2) return s;
  return `${chars.slice(0, max - 1).join('').replace(/…+$/, '')}…`;
}

export function truncateLeft(s: string, max: number): string {
  const chars = [...s];
  if (max <= 0) return '';
  if (chars.length <= max || max < 2) return s;
  return `…${chars.slice(chars.length - max + 1).join('').replace(/^…+/, '')}`;
}

export const pad = (s: string, n: number): string => s + ' '.repeat(Math.max(0, n - [...s].length));

export function slug(text: string, max = 40): string {
  const folded = text
    .toLowerCase()
    .normalize('NFD')
    .replace(/[̀-ͯ]/g, '');
  let out = '';
  for (const c of folded) {
    if (/[a-z0-9]/.test(c)) out += c;
    else if (out && !out.endsWith('-')) out += '-';
  }
  out = out.replace(/-+$/, '');
  if (out.length <= max) return out;
  const cut = out.slice(0, max + 1);
  const end = cut.lastIndexOf('-') > 0 ? cut.lastIndexOf('-') : max;
  return out.slice(0, end).replace(/-+$/, '');
}

export function folderSlug(branch: string): string {
  let out = '';
  for (const c of branch) {
    if (/[A-Za-z0-9]/.test(c)) out += c.toLowerCase();
    else if (!out.endsWith('-')) out += '-';
  }
  out = out.replace(/^-+|-+$/g, '');
  return out || 'worktree';
}
