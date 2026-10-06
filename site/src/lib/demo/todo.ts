import { BOLD, type Rect, STRIKE, type Style, contains, rect } from '../term/grid';
import type { App } from './app';
import { drawPanelBorder } from './changes';
import { type Areas, GAP, Rows, bottom, intersect, moreAbove, right } from './layout';
import type { Key } from './programs';
import type { Painter } from './ui';

const TEXT_X = 5;
const BUTTONS = 4;
const NEW_TODO = '+ new todo';
const CLEAR_DONE = 'clear done';
const PLACEHOLDER = 'enter adds, esc closes';
const DARK: Style = { fg: 8 };

export interface TodoItem {
  id: number;
  text: string;
  done: boolean;
}

interface Field {
  item: number | null;
  chars: string[];
  cursor: number;
}

type Span = [number, number];

export function wrap(chars: string[], width: number): Span[] {
  const w = Math.max(1, width);
  const lines: Span[] = [];
  let start = 0;
  while (chars.length - start > w) {
    const end = start + w;
    let space = -1;
    for (let i = end - 1; i > start; i--) {
      if (chars[i] === ' ') {
        space = i;
        break;
      }
    }
    const next = space < 0 ? end : space + 1;
    lines.push([start, next]);
    start = next;
  }
  lines.push([start, chars.length]);
  return lines;
}

function fieldLines(f: Field, width: number): Span[] {
  const lines = wrap(f.chars, width);
  const last = lines[lines.length - 1];
  if (last[1] > last[0] && last[1] - last[0] >= Math.max(1, width)) lines.push([f.chars.length, f.chars.length]);
  return lines;
}

function fieldPosition(f: Field, width: number): [number, number] {
  const lines = fieldLines(f, width);
  let line = 0;
  lines.forEach(([start], i) => {
    if (start <= f.cursor) line = i;
  });
  return [line, f.cursor - lines[line][0]];
}

function placeIn(f: Field, width: number, line: number, col: number): void {
  const lines = fieldLines(f, width);
  const span = lines[line];
  if (!span) return;
  const room = line + 1 === lines.length ? span[1] - span[0] : Math.max(0, span[1] - span[0] - 1);
  f.cursor = span[0] + Math.min(col, room);
}

const clean = (text: string): string => text.replace(/\s+/g, ' ').trim();

export class TodoPanel {
  open = false;
  items: TodoItem[] = [];
  scroll = 0;
  field: Field | null = null;
  undo: [number, TodoItem][] | null = null;
  private next = 1;

  constructor(seed: [string, boolean][]) {
    this.items = seed.map(([text, done]) => ({ id: this.next++, text, done }));
  }

  private pending(): number {
    return this.items.filter((i) => !i.done).length;
  }

  item(id: number): TodoItem | undefined {
    return this.items.find((i) => i.id === id);
  }

  add(text: string): boolean {
    const t = clean(text);
    if (!t) return false;
    this.items.splice(this.pending(), 0, { id: this.next++, text: t, done: false });
    return true;
  }

  toggle(id: number): void {
    const at = this.items.findIndex((i) => i.id === id);
    if (at < 0) return;
    const [item] = this.items.splice(at, 1);
    item.done = !item.done;
    this.items.splice(item.done ? this.items.length : this.pending(), 0, item);
  }

  remove(id: number): void {
    const at = this.items.findIndex((i) => i.id === id);
    if (at >= 0) this.undo = [[at, this.items.splice(at, 1)[0]]];
  }

  clearDone(): number {
    const split = this.pending();
    const done = this.items.splice(split).map((item, i): [number, TodoItem] => [split + i, item]);
    if (done.length) this.undo = done;
    return done.length;
  }

  restore(): void {
    for (const [at, item] of this.undo ?? []) {
      const split = this.pending();
      this.items.splice(item.done ? Math.max(split, Math.min(at, this.items.length)) : Math.min(at, split), 0, item);
    }
    this.undo = null;
  }

  edit(id: number | null, text = ''): void {
    const chars = [...text];
    this.field = { item: id, chars, cursor: chars.length };
  }

  commit(): void {
    const f = this.field;
    this.field = null;
    if (!f) return;
    const text = f.chars.join('');
    const item = f.item === null ? null : this.item(f.item);
    if (f.item === null) this.add(text);
    else if (item && clean(text)) item.text = clean(text);
  }

  submit(): void {
    const f = this.field;
    if (f && f.item === null && this.add(f.chars.join(''))) this.field = { item: null, chars: [], cursor: 0 };
    else this.commit();
  }

  key(k: Key, width: number): void {
    const f = this.field;
    if (!f) return;
    const [line, col] = fieldPosition(f, width);
    if (k.key === 'Escape') this.field = null;
    else if (k.key === 'Enter') this.submit();
    else if (k.key === 'Backspace' && f.cursor > 0) f.chars.splice(--f.cursor, 1);
    else if (k.key === 'Delete') f.chars.splice(f.cursor, 1);
    else if (k.key === 'ArrowLeft') f.cursor = Math.max(0, f.cursor - 1);
    else if (k.key === 'ArrowRight') f.cursor = Math.min(f.chars.length, f.cursor + 1);
    else if (k.key === 'Home') f.cursor = 0;
    else if (k.key === 'End') f.cursor = f.chars.length;
    else if (k.key === 'ArrowUp' && line > 0) placeIn(f, width, line - 1, col);
    else if (k.key === 'ArrowDown') placeIn(f, width, line + 1, col);
    else if (k.key.length === 1 && !k.ctrl && !k.alt) this.type(k.key);
  }

  place(width: number, line: number, col: number): void {
    if (this.field) placeIn(this.field, width, line, col);
  }

  type(text: string): void {
    const f = this.field;
    if (!f) return;
    const chars = [...text.replace(/[\r\n\t]/g, ' ')];
    f.chars.splice(f.cursor, 0, ...chars);
    f.cursor += chars.length;
  }
}

const textWidth = (area: Rect): number => Math.max(1, area.w - TEXT_X - BUTTONS);

function itemLines(item: TodoItem, field: Field | null, width: number): number {
  return field?.item === item.id ? fieldLines(field, width).length : wrap([...item.text], width).length;
}

function rows(area: Rect, t: TodoPanel, width: number): Rows {
  const y = Math.min(area.y + 1 + GAP, bottom(area));
  const adding = t.field?.item === null ? t.field : null;
  return new Rows(
    rect(area.x, y, area.w, bottom(area) - y),
    t.items.map((i) => Math.max(1, itemLines(i, t.field, width))),
    adding ? fieldLines(adding, width).length : 1,
    t.scroll,
  );
}

export function todoWidth(app: App): number {
  return textWidth(app.areas().changes);
}

export function drawTodo(p: Painter, areas: Areas): void {
  const app = p.app;
  const t = app.todo;
  const area = areas.changes;
  drawPanelBorder(p, areas);
  p.g.clear(area);
  const header = rect(area.x + 1, area.y, Math.max(0, area.w - 2), 1);
  const x = p.span(header.x, header.y, 'todo', { add: BOLD });
  const pending = t.items.filter((i) => !i.done).length;
  if (pending) p.span(x + 1, header.y, String(pending), DARK);
  const close = rect(right(header) - 3, header.y, 3, 1);
  p.span(close.x + 1, close.y, '×', p.hovered(close) ? { fg: 1, add: BOLD } : DARK);
  p.region({ r: close, click: () => app.toggleTodo(), cursor: 'pointer' });
  if (pending < t.items.length) {
    const clear = intersect(rect(close.x - CLEAR_DONE.length - 3, header.y, CLEAR_DONE.length + 2, 1), header);
    p.span(clear.x, clear.y, ` ${CLEAR_DONE} `, p.hovered(clear) ? { fg: 0, bg: 6 } : DARK);
    p.region({ r: clear, click: () => app.clearDoneTodos(), cursor: 'pointer' });
  }
  const width = textWidth(area);
  const surface = app.light ? 254 : 236;
  const hoverFill = app.light ? 255 : 235;
  const layout = rows(area, t, width);
  for (let i = layout.first(); i < layout.end(); i++) {
    const item = t.items[i];
    const row = layout.item(i);
    const editing = t.field?.item === item.id ? t.field : null;
    const hovered = p.hovered(row) && !editing;
    const bg: Style = hovered ? { bg: hoverFill } : {};
    if (hovered) p.g.fill(row, bg);
    const box = rect(row.x + 1, row.y, 3, 1);
    p.span(box.x, box.y, item.done ? '[x]' : '[ ]', p.hovered(box) ? { ...bg, fg: 6, add: BOLD } : item.done ? { ...bg, fg: 8 } : bg);
    p.region({ r: box, click: () => app.toggleTodoItem(item.id), cursor: 'pointer' });
    if (editing) {
      drawField(p, editing, row, width, '', surface);
      continue;
    }
    const chars = [...item.text];
    wrap(chars, width).forEach(([a, b], line) => {
      if (row.y + line >= bottom(row)) return;
      const style: Style = item.done ? { ...bg, fg: 8, add: STRIKE } : bg;
      p.span(row.x + TEXT_X, row.y + line, chars.slice(a, b).join('').trimEnd(), style, width);
    });
    const text = rect(row.x + TEXT_X, row.y, width, row.h);
    p.region({ r: text, click: (cx, cy) => app.editTodo(item.id, cy - row.y, cx - text.x), cursor: 'text' });
    if (!hovered) continue;
    const del = rect(right(row) - BUTTONS, row.y, 3, 1);
    p.span(del.x + 1, del.y, '×', p.hovered(del) ? { ...bg, fg: 1, add: BOLD } : { ...bg, fg: 8 });
    p.region({ r: del, click: () => app.deleteTodo(item.id), cursor: 'pointer' });
  }
  const [above, below] = layout.hidden();
  for (const [n, arrow, m] of [
    [above, '↑', moreAbove(layout.list)],
    [below, '↓', layout.moreBelow()],
  ] as const) {
    if (n) p.span(m.x, m.y, `  ${arrow} ${n} more`, DARK, m.w);
  }
  p.region({ r: layout.list, wheel: (dy) => app.scrollTodo(layout.scrolled(Math.sign(dy) * 3)) });
  const button = layout.buttonRect();
  if (t.field?.item === null) drawField(p, t.field, button, width, '+', surface);
  else {
    p.span(button.x + 1, button.y, ` ${NEW_TODO} `, p.hovered(button) ? { fg: 0, bg: 6, add: BOLD } : { fg: 6 });
    p.region({ r: button, click: () => app.addTodo(), cursor: 'pointer' });
  }
}

function drawField(p: Painter, f: Field, r: Rect, width: number, mark: string, surface: number): void {
  const field = rect(r.x + TEXT_X - 1, r.y, Math.max(0, r.w - TEXT_X - BUTTONS + 2), r.h);
  p.g.fill(field, { bg: surface });
  if (mark) p.span(r.x + 2, r.y, mark, { fg: 6, add: BOLD });
  const x = r.x + TEXT_X;
  if (!f.chars.length) p.span(x, r.y, PLACEHOLDER, { fg: 8, bg: surface }, right(field) - x);
  fieldLines(f, width).forEach(([a, b], line) => {
    if (r.y + line < bottom(r)) p.span(x, r.y + line, f.chars.slice(a, b).join(''), { bg: surface }, right(field) - x);
  });
  const [line, col] = fieldPosition(f, width);
  const at = { x: x + col, y: r.y + line };
  if (contains(r, at.x, at.y)) p.cursor = { ...at, shape: 'block' };
  p.region({ r: field, click: (cx, cy) => p.app.placeTodoCursor(cy - r.y, cx - x), cursor: 'text' });
}
