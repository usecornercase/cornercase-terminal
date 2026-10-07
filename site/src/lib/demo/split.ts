import { type Rect, rect } from '../term/grid';

export type Dir = 'right' | 'down';

export type Node = { leaf: number } | { dir: Dir; ratio: number; first: Node; second: Node };

export interface Divider {
  path: boolean[];
  dir: Dir;
  area: Rect;
  line: Rect;
  min: [number, number];
}

const PADDING = 1;
const MIN_COLS = 10;
const MIN_ROWS = 3;

const gap = (dir: Dir): number => (dir === 'right' ? 1 + PADDING : 1);

const leafMin = (dir: Dir): number => (dir === 'right' ? MIN_COLS : MIN_ROWS);

const span = (area: Rect, dir: Dir): [number, number] => {
  const total = dir === 'right' ? area.w : area.h;
  return [total, Math.min(gap(dir), total)];
};

const bounds = (room: number, [first, second]: [number, number]): [number, number] =>
  first + second <= room ? [first, room - second] : [1, room - 1];

function firstSize(room: number, ratio: number, min: [number, number]): number {
  if (room < 2) return room;
  const [low, high] = bounds(room, min);
  return Math.max(low, Math.min(high, Math.round(room * Math.max(0, Math.min(1, ratio)))));
}

function minSize(node: Node, along: Dir): number {
  if ('leaf' in node) return leafMin(along);
  const [a, b] = [minSize(node.first, along), minSize(node.second, along)];
  return node.dir === along ? a + gap(along) + b : Math.max(a, b);
}

export function hasRoom(node: Node, area: Rect): boolean {
  return 'leaf' in node || (area.w >= minSize(node, 'right') && area.h >= minSize(node, 'down'));
}

function divide(area: Rect, dir: Dir, ratio: number, min: [number, number]): [Rect, Rect, Rect] {
  const [total, g] = span(area, dir);
  const room = total - g;
  const first = firstSize(room, ratio, min);
  const second = room - first;
  if (dir === 'right') {
    return [
      rect(area.x, area.y, first, area.h),
      rect(area.x + first + g, area.y, second, area.h),
      rect(area.x + first, area.y, Math.min(g, 1), area.h),
    ];
  }
  return [
    rect(area.x, area.y, area.w, first),
    rect(area.x, area.y + first + g, area.w, second),
    rect(area.x, area.y + first, area.w, g),
  ];
}

const sides = (node: Extract<Node, { dir: Dir }>): [number, number] => [minSize(node.first, node.dir), minSize(node.second, node.dir)];

export function fits(area: Rect, dir: Dir): boolean {
  const [total, g] = span(area, dir);
  return total - g >= leafMin(dir) * 2;
}

export function ids(node: Node): number[] {
  return 'leaf' in node ? [node.leaf] : [...ids(node.first), ...ids(node.second)];
}

function panes(node: Node, area: Rect): [number, Rect][] {
  if ('leaf' in node) return [[node.leaf, area]];
  const [a, b] = divide(area, node.dir, node.ratio, sides(node));
  return [...panes(node.first, a), ...panes(node.second, b)];
}

export function visible(node: Node, area: Rect, active: number): [number, Rect][] {
  return hasRoom(node, area) ? panes(node, area) : [[active, area]];
}

function collectDividers(node: Node, area: Rect, path: boolean[]): Divider[] {
  if ('leaf' in node) return [];
  const min = sides(node);
  const [a, b, line] = divide(area, node.dir, node.ratio, min);
  return [
    { path, dir: node.dir, area, line, min },
    ...collectDividers(node.first, a, [...path, false]),
    ...collectDividers(node.second, b, [...path, true]),
  ];
}

export function dividers(node: Node, area: Rect): Divider[] {
  return hasRoom(node, area) ? collectDividers(node, area, []) : [];
}

export const grab = (d: Divider): Rect =>
  d.dir === 'right' ? rect(d.line.x, d.line.y, Math.min(d.line.w + PADDING, d.area.x + d.area.w - d.line.x), d.line.h) : d.line;

export function ratioAt(d: Divider, x: number, y: number): number {
  const [start, at] = d.dir === 'right' ? [d.area.x, x] : [d.area.y, y];
  const [total, g] = span(d.area, d.dir);
  const room = total - g;
  if (room < 2) return 0.5;
  const [low, high] = bounds(room, d.min);
  return Math.max(low, Math.min(high, at - start)) / room;
}

export function split(node: Node, target: number, dir: Dir, fresh: number): Node {
  if ('leaf' in node) {
    return node.leaf === target ? { dir, ratio: 0.5, first: node, second: { leaf: fresh } } : node;
  }
  return { ...node, first: split(node.first, target, dir, fresh), second: split(node.second, target, dir, fresh) };
}

export function remove(node: Node, target: number): Node | null {
  if ('leaf' in node) return node.leaf === target ? null : node;
  const first = remove(node.first, target);
  const second = remove(node.second, target);
  if (!first) return second;
  if (!second) return first;
  return { ...node, first, second };
}

export function setRatio(node: Node, path: boolean[], value: number): Node {
  if ('leaf' in node) return node;
  if (path.length === 0) return { ...node, ratio: value };
  const [head, ...rest] = path;
  return head ? { ...node, second: setRatio(node.second, rest, value) } : { ...node, first: setRatio(node.first, rest, value) };
}

export type PanePlace = 'swap' | 'left' | 'right' | 'above' | 'below';

export function placeAt(r: Rect, x: number, y: number): PanePlace {
  const [w, h] = [Math.max(1, r.w), Math.max(1, r.h)];
  const px = (x - r.x) * 2 + 1;
  const py = (y - r.y) * 2 + 1;
  if (3 * px >= 2 * w && 3 * px < 4 * w && 3 * py >= 2 * h && 3 * py < 4 * h) return 'swap';
  const edges: [PanePlace, number][] = [
    ['left', px * h],
    ['right', Math.max(0, 2 * w - px) * h],
    ['above', py * w],
    ['below', Math.max(0, 2 * h - py) * w],
  ];
  return edges.reduce((best, e) => (e[1] < best[1] ? e : best))[0];
}

export function placeArea(place: PanePlace, r: Rect): Rect {
  const [w, h] = [Math.floor(r.w / 2), Math.floor(r.h / 2)];
  if (place === 'left') return rect(r.x, r.y, w, r.h);
  if (place === 'right') return rect(r.x + r.w - w, r.y, w, r.h);
  if (place === 'above') return rect(r.x, r.y, r.w, h);
  if (place === 'below') return rect(r.x, r.y + r.h - h, r.w, h);
  return r;
}

function insert(node: Node, target: number, dir: Dir, before: boolean, fresh: number): Node {
  if ('leaf' in node) {
    if (node.leaf !== target) return node;
    const [first, second] = before ? [fresh, target] : [target, fresh];
    return { dir, ratio: 0.5, first: { leaf: first }, second: { leaf: second } };
  }
  return { ...node, first: insert(node.first, target, dir, before, fresh), second: insert(node.second, target, dir, before, fresh) };
}

function swap(node: Node, a: number, b: number): Node {
  if ('leaf' in node) return node.leaf === a ? { leaf: b } : node.leaf === b ? { leaf: a } : node;
  return { ...node, first: swap(node.first, a, b), second: swap(node.second, a, b) };
}

export function moved(node: Node, pane: number, target: number, place: PanePlace): Node | null {
  const all = ids(node);
  if (pane === target || !all.includes(pane) || !all.includes(target)) return null;
  if (place === 'swap') return swap(node, pane, target);
  const rest = remove(node, pane);
  if (!rest) return null;
  const dir: Dir = place === 'left' || place === 'right' ? 'right' : 'down';
  return insert(rest, target, dir, place === 'left' || place === 'above', pane);
}
