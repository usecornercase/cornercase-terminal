import { INVERSE, type Grid } from '../term/grid';

export type Cmp<L> = (a: L, b: L) => number;
export interface Cell<L> {
  line: L;
  offset: number;
}
export interface Stretch<L> {
  from: Cell<L>;
  to: Cell<L>;
}
export type On = [from: number, to: number, broken: boolean];

const earlier = <L>(cmp: Cmp<L>, a: Cell<L>, b: Cell<L>): boolean => cmp(a.line, b.line) < 0 || (cmp(a.line, b.line) === 0 && a.offset < b.offset);

export const within = <L>(line: L, from: number, to: number): Stretch<L> => ({ from: { line, offset: from }, to: { line, offset: to } });

export function union<L>(cmp: Cmp<L>, a: Stretch<L>, b: Stretch<L>): Stretch<L> {
  return { from: earlier(cmp, b.from, a.from) ? b.from : a.from, to: earlier(cmp, a.to, b.to) ? b.to : a.to };
}

const empty = <L>(cmp: Cmp<L>, s: Stretch<L>): boolean => !earlier(cmp, s.from, s.to);

export const endsBefore = <L>(cmp: Cmp<L>, s: Stretch<L>): boolean => s.to.offset === 0 && cmp(s.to.line, s.from.line) !== 0;

export function dragged<L>(cmp: Cmp<L>, current: Stretch<L> | null, anchor: Stretch<L>, pointer: Stretch<L>): Stretch<L> | null {
  const hull = union(cmp, anchor, pointer);
  if (empty(cmp, hull)) return null;
  const same = cmp(hull.from.line, anchor.from.line) === 0 && hull.from.offset === anchor.from.offset && cmp(hull.to.line, anchor.to.line) === 0 && hull.to.offset === anchor.to.offset;
  return current || !same ? hull : null;
}

export function on<L>(cmp: Cmp<L>, s: Stretch<L>, line: L, len: number): On | null {
  if (cmp(line, s.from.line) < 0 || cmp(line, s.to.line) > 0) return null;
  const start = cmp(line, s.from.line) === 0 ? Math.min(s.from.offset, len) : 0;
  const end = cmp(line, s.to.line) === 0 ? Math.min(s.to.offset, len) : len;
  const broken = cmp(line, s.to.line) !== 0;
  const to = Math.max(start, end);
  return to > start || broken ? [start, to, broken] : null;
}

export function textOf<L>(s: Stretch<L>, lines: string[]): string {
  const last = lines.length - 1;
  return lines.map((line, i) => [...line].slice(i === 0 ? s.from.offset : 0, i === last ? s.to.offset : undefined).join('')).join('\n');
}

export const MAX_WRAPPED_ROWS = 100;

export interface WrapRow {
  from: number;
  to: number;
  end: 'line' | 'wrap' | 'cut';
}

const breakable = (c: string): boolean => /\s/.test(c) || /[!-/:-@[-`{-~]/.test(c);

export function wrapRows(text: string, room: number, most = MAX_WRAPPED_ROWS): WrapRow[] {
  const chars = [...text];
  const rows: WrapRow[] = [];
  let at = 0;
  while (rows.length < most) {
    if (chars.length - at <= room) {
      rows.push({ from: at, to: chars.length, end: 'line' });
      break;
    }
    if (rows.length + 1 === most || room <= 0) {
      rows.push({ from: at, to: at + Math.max(0, room - 1), end: 'cut' });
      break;
    }
    let len = room;
    for (let i = room - 1; i >= 0; i--) {
      if (breakable(chars[at + i])) {
        len = i + 1;
        break;
      }
    }
    rows.push({ from: at, to: at + len, end: 'wrap' });
    at += len;
  }
  return rows;
}

export function fit(text: string, room: number): { shown: number; cut: boolean; total: number } {
  const [row] = wrapRows(text, room, 1);
  return { shown: row.to, cut: row.end === 'cut', total: [...text].length };
}

export function charsAt(text: string, row: WrapRow, column: number): [number, number] {
  const total = [...text].length;
  if (column < 0) return [row.from, row.from];
  if (column < row.to - row.from) return [row.from + column, row.from + column + 1];
  if (row.end === 'wrap') return [row.to, row.to];
  return row.end === 'cut' && column === row.to - row.from ? [row.to, total] : [total, total];
}

export function reverse(g: Grid, code: { x: number; y: number; w: number }, text: string, row: WrapRow, [from, to, broken]: On): void {
  const total = [...text].length;
  const last = row.end !== 'wrap';
  const limit = row.end === 'cut' ? total : row.to;
  const holds = (from < limit && to > row.from && to > from) || (last && broken && from <= row.to);
  if (!holds) return;
  const width = row.to - row.from;
  const edge = width + (row.end === 'cut' ? 1 : 0);
  const hidden = row.end === 'cut' && to > from && to > row.to;
  const past = broken || (row.end === 'wrap' && to > row.to);
  const start = from <= row.from ? 0 : to > from && from < row.to ? from - row.from : hidden ? width : edge;
  const stop = past ? code.w : hidden ? width + 1 : Math.max(start, Math.min(to, row.to) - row.from);
  for (let x = start; x < stop; x++) g.style(code.x + x, code.y, { add: INVERSE });
}
