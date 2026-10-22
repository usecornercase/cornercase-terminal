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

export function fit(text: string, room: number): { shown: number; cut: boolean; total: number } {
  const total = [...text].length;
  const cut = total > room;
  return { shown: cut ? Math.max(0, room - 1) : total, cut, total };
}

export function charsAt(text: string, room: number, column: number): [number, number] {
  const { shown, cut, total } = fit(text, room);
  if (column < 0) return [0, 0];
  if (column < shown) return [column, column + 1];
  return cut && column === shown ? [shown, total] : [total, total];
}

export function reverse(g: Grid, code: { x: number; y: number; w: number }, text: string, [from, to, broken]: On): void {
  const style = (a: number, b: number) => {
    for (let x = a; x < b; x++) g.style(code.x + x, code.y, { add: INVERSE });
  };
  if (broken && from === 0) return style(0, code.w);
  const { shown, cut } = fit(text, code.w - 1);
  const hidden = cut && to > from && to > shown;
  const start = to > from && from < shown ? from : hidden ? shown : shown + (cut ? 1 : 0);
  const stop = broken ? code.w : hidden ? shown + 1 : Math.max(start, Math.min(to, shown));
  style(start, stop);
}
