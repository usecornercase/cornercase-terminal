import { BOLD, type Rect, type Style, rect } from '../term/grid';
import { bottom, intersect, isEmpty, right } from './layout';
import type { Key } from './programs';
import { seg, truncateRight } from './text';
import type { Painter } from './ui';

export const COMBO_ROWS = 8;
export const ARROW = '▾';

const MUTED: Style = { fg: 8 };

interface Open {
  query: string;
  selected: number;
  scroll: number;
}

export interface Combo {
  options: string[];
  chosen: number | null;
  open: Open | null;
}

export const combo = (options: string[], chosen: number | null): Combo => ({
  options,
  chosen: chosen !== null && chosen >= 0 && chosen < options.length ? chosen : null,
  open: null,
});

export const comboLabel = (c: Combo): string | null => (c.chosen === null ? null : (c.options[c.chosen] ?? null));

export function comboShown(c: Combo): number[] {
  const query = (c.open?.query ?? '').toLowerCase();
  return c.options.map((_, i) => i).filter((i) => c.options[i].toLowerCase().includes(query));
}

export const comboHighlighted = (c: Combo): number | null => (c.open ? (comboShown(c)[c.open.selected] ?? null) : null);

function moveBy(open: Open, delta: number, len: number, rows: number): void {
  if (!len) return;
  const i = Math.max(0, Math.min(len - 1, Math.min(open.selected, len - 1) + delta));
  open.selected = i;
  if (i < open.scroll) open.scroll = i;
  else if (rows > 0 && i >= open.scroll + rows) open.scroll = i + 1 - rows;
}

export function openCombo(c: Combo): void {
  const open: Open = { query: '', selected: 0, scroll: 0 };
  moveBy(open, c.chosen ?? 0, c.options.length, COMBO_ROWS);
  c.open = open;
}

export function chooseCombo(c: Combo, i: number): void {
  if (i >= 0 && i < c.options.length) c.chosen = i;
  c.open = null;
}

export function scrollCombo(c: Combo, delta: number, rows: number): void {
  if (!c.open) return;
  const max = Math.max(0, comboShown(c).length - rows);
  c.open.scroll = Math.min(max, Math.max(0, Math.min(c.open.scroll, max) + delta));
}

function filter(open: Open, query: string): void {
  open.query = query;
  open.selected = 0;
  open.scroll = 0;
}

export function comboKey(c: Combo, k: Key, rows: number): boolean {
  const typed = k.key.length === 1 && !k.ctrl && !k.alt ? k.key : null;
  if (!c.open) {
    if (['ArrowDown', 'ArrowUp', 'Enter', ' '].includes(k.key)) openCombo(c);
    else if (typed) {
      openCombo(c);
      if (c.open) filter(c.open, typed);
    }
    return false;
  }
  const open = c.open;
  if (k.key === 'Escape') c.open = null;
  else if (k.key === 'Enter') {
    const chosen = comboHighlighted(c);
    const changed = chosen !== null && c.chosen !== chosen;
    if (chosen === null) c.open = null;
    else chooseCombo(c, chosen);
    return changed;
  } else {
    const len = comboShown(c).length;
    const page = Math.max(1, rows);
    const moves: Record<string, number> = { ArrowUp: -1, ArrowDown: 1, PageUp: -page, PageDown: page, Home: -Infinity, End: Infinity };
    if (k.key in moves) moveBy(open, moves[k.key], len, rows);
    else if (k.key === 'Backspace') filter(open, [...open.query].slice(0, -1).join(''));
    else if (typed) filter(open, open.query + typed);
  }
  return false;
}

export function comboList(field: Rect, c: Combo, screen: Rect): Rect {
  const height = Math.min(Math.max(comboShown(c).length, 1), COMBO_ROWS) + 2;
  const below = bottom(field) + height <= bottom(screen);
  const above = field.y - screen.y >= height;
  const y = below || !above ? bottom(field) : field.y - height;
  return intersect(rect(Math.max(0, field.x - 1), y, field.w + 2, height), screen);
}

export const comboRows = (list: Rect): number => Math.max(0, list.h - 2);

function item(list: Rect, row: number): Rect {
  const inner = rect(list.x + 1, list.y + 1, list.w - 2, list.h - 2);
  return intersect(rect(inner.x, inner.y + row, inner.w, 1), inner);
}

export function drawComboField(p: Painter, field: Rect, c: Combo, placeholder: string, focused: boolean): void {
  if (isEmpty(field)) return;
  const bg: Style = { bg: p.surface };
  const text: Style = { ...bg, fg: p.app.light ? 0 : 15, add: BOLD };
  const room = Math.max(0, field.w - 4);
  const typing = !!c.open && c.open.query !== '';
  const chosen = comboLabel(c);
  const [label, style] = typing ? [c.open?.query ?? '', text] : chosen !== null ? [chosen, text] : [placeholder, { ...bg, ...MUTED }];
  const shown = truncateRight(label, room);
  const gap = Math.max(0, room - [...shown].length);
  const lit = focused || !!c.open || p.hovered(field);
  p.g.fill(field, bg);
  p.line(field, [seg(' ', bg), seg(shown, style), seg(' '.repeat(gap + 1), bg), seg(ARROW, { ...bg, fg: lit ? 6 : 8 }), seg(' ', bg)]);
  if (typing && focused) p.cursor = { shape: 'block', x: Math.min(field.x + 1 + [...shown].length, right(field) - 1), y: field.y };
}

export function drawComboList(p: Painter, list: Rect, c: Combo, choose: (i: number) => void): void {
  if (isEmpty(list)) return;
  p.g.clear(list);
  p.g.box(list, MUTED);
  const shown = comboShown(c);
  const width = Math.max(0, list.w - 3);
  if (!shown.length) {
    const r = item(list, 0);
    p.span(r.x, r.y, ' no match', MUTED, r.w);
    return;
  }
  const scroll = c.open?.scroll ?? 0;
  const visible = comboRows(list);
  const highlighted = comboHighlighted(c);
  for (let row = 0; row < visible; row++) {
    const i = shown[scroll + row];
    if (i === undefined) break;
    const r = item(list, row);
    const selected = highlighted === i;
    const bg: Style = selected ? { bg: p.surface } : p.hovered(r) ? { bg: p.hoverSurface } : {};
    p.line(r, [seg(selected ? '▌' : ' ', selected ? { ...bg, fg: 6 } : bg), seg(truncateRight(c.options[i], width), selected ? { ...bg, add: BOLD } : bg)], bg);
    p.region({ r, click: () => choose(i), cursor: 'pointer' });
  }
  const below = Math.max(0, shown.length - scroll - visible);
  for (const [count, arrow, y] of [[scroll, '↑', list.y], [below, '↓', bottom(list) - 1]] as const) {
    if (count > 0) p.span(list.x + 2, y, ` ${arrow} ${count} more `, MUTED, Math.max(0, list.w - 4));
  }
}
