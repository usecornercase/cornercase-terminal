import { easeInOut, mix } from './math';

export type Box = [number, number, number, number];
export type Free = 'all' | 'left' | 'right';

export const WIDE = 980;
export const SHORTEST = 420;
export const NAV = 64;

const LEAST_ROOM = 40;
const MOST_ZOOM = 2.4;

export interface View {
  width: number;
  height: number;
}

export interface Shot {
  focus: Box;
  free: Free;
  rx?: number;
  ry?: number;
  rz?: number;
  zoom?: number;
  dx?: number;
  dy?: number;
  fade?: number;
}

export interface Key {
  at: number;
  shot: Shot;
}

export interface Pose {
  x: number;
  y: number;
  scale: number;
  cx: number;
  cy: number;
  rx: number;
  ry: number;
  rz: number;
  fade: number;
}

export function sheetHeight(view: View): number {
  return Math.min(view.height * 0.36, 300);
}

export function bigOf(width: number): number {
  return width >= 2400 ? 1.5 : width >= 1900 ? 1.25 : 1;
}

export function captionWidth(view: View): number {
  return Math.max(400, Math.min(view.width * 0.36, 640 * bigOf(view.width)));
}

export function freeArea(free: Free, view: View): Box {
  const { width, height } = view;
  const top = NAV + 12;
  if (width < WIDE) return [10, top, width - 20, Math.max(LEAST_ROOM, height - top - sheetHeight(view) - 12)];
  const margin = 28;
  const inner: Box = [margin, top, width - margin * 2, Math.max(LEAST_ROOM, height - top - margin)];
  if (free === 'all') return inner;
  const caption = captionWidth(view);
  return free === 'right' ? [caption, top, width - caption - margin, inner[3]] : [margin, top, width - caption - margin, inner[3]];
}

export function pose(shot: Shot, view: View): Pose {
  const [x, y, w, h] = shot.focus;
  const [ax, ay, aw, ah] = freeArea(shot.free, view);
  const scale = Math.min(aw / w, ah / h, MOST_ZOOM) * (shot.zoom ?? 1);
  const cx = x + w / 2;
  const cy = y + h / 2;
  return {
    x: ax + aw / 2 - cx * scale + (shot.dx ?? 0) * view.width,
    y: ay + ah / 2 - cy * scale + (shot.dy ?? 0) * view.height,
    scale,
    cx,
    cy,
    rx: shot.rx ?? 0,
    ry: shot.ry ?? 0,
    rz: shot.rz ?? 0,
    fade: shot.fade ?? 1,
  };
}

export function blend(a: Pose, b: Pose, amount: number): Pose {
  return {
    x: mix(a.x, b.x, amount),
    y: mix(a.y, b.y, amount),
    scale: a.scale * (b.scale / a.scale) ** amount,
    cx: mix(a.cx, b.cx, amount),
    cy: mix(a.cy, b.cy, amount),
    rx: mix(a.rx, b.rx, amount),
    ry: mix(a.ry, b.ry, amount),
    rz: mix(a.rz, b.rz, amount),
    fade: mix(a.fade, b.fade, amount),
  };
}

export interface Placed {
  at: number;
  pose: Pose;
}

export function place(keys: Key[], view: View): Placed[] {
  return [...keys].sort((a, b) => a.at - b.at).map((key) => ({ at: key.at, pose: pose(key.shot, view) }));
}

export function poseAt(keys: Placed[], at: number): Pose {
  const first = keys[0];
  const last = keys[keys.length - 1];
  if (!first || !last) throw new Error('a track needs keys');
  if (at <= first.at) return { ...first.pose };
  if (at >= last.at) return { ...last.pose };
  let low = 0;
  let high = keys.length - 1;
  while (high - low > 1) {
    const middle = (low + high) >> 1;
    if (keys[middle].at <= at) low = middle;
    else high = middle;
  }
  const a = keys[low];
  const b = keys[high];
  return blend(a.pose, b.pose, easeInOut((at - a.at) / (b.at - a.at)));
}

export function closest(keys: Placed[]): number {
  return keys.reduce((most, key) => (key.pose.fade > 0.5 ? Math.max(most, key.pose.scale) : most), 1);
}

export const DEPTH = 1500;

export function lens(view: View): string {
  const x = (view.width / 2).toFixed(1);
  const y = (view.height * 0.46).toFixed(1);
  return `translate3d(${x}px,${y}px,0) perspective(${DEPTH}px) translate3d(-${x}px,-${y}px,0) `;
}

export function transformOf(p: Pose): string {
  const turned = p.rx !== 0 || p.ry !== 0 || p.rz !== 0;
  const turn = turned
    ? ` translate(${p.cx.toFixed(1)}px,${p.cy.toFixed(1)}px) rotateX(${p.rx.toFixed(2)}deg) rotateY(${p.ry.toFixed(2)}deg) rotateZ(${p.rz.toFixed(2)}deg) translate(${(-p.cx).toFixed(1)}px,${(-p.cy).toFixed(1)}px)`
    : '';
  return `translate3d(${p.x.toFixed(2)}px,${p.y.toFixed(2)}px,0) scale(${p.scale.toFixed(4)})${turn}`;
}

export interface Size {
  cols: number;
  rows: number;
  cell: number;
}

export const ASPECT = 2.08;
export const PHONE_ROWS = 34;
export const BAR = 34;
export const PAD = 10;

export function sizeFor(view: View): Size {
  const { width, height } = view;
  if (width < 700) {
    const cell = 7.4;
    return { cols: Math.max(36, Math.min(62, Math.floor((width - 20 - PAD * 2) / cell))), rows: PHONE_ROWS, cell };
  }
  if (width < WIDE) {
    const cell = 7.2;
    const room = height - NAV - 12 - sheetHeight(view) - 12 - BAR - PAD * 2;
    return { cols: Math.max(92, Math.min(128, Math.floor((width - 20 - PAD * 2) / cell))), rows: Math.max(24, Math.min(40, Math.floor(room / (cell * ASPECT)))), cell };
  }
  const cell = Math.max(7.2, Math.min(9.4, width / 200));
  const [, , aw, ah] = freeArea('all', view);
  return {
    cols: Math.max(124, Math.min(196, Math.floor((aw - PAD * 2) / cell))),
    rows: Math.max(30, Math.min(48, Math.floor((ah - BAR - PAD * 2) / (cell * ASPECT)))),
    cell,
  };
}

export function windowOf(size: Size): Box {
  return [0, 0, size.cols * size.cell + PAD * 2, size.rows * size.cell * ASPECT + BAR + PAD * 2];
}

export function cells(size: Size, x: number, y: number, w: number, h: number): Box {
  return [PAD + x * size.cell, BAR + PAD + y * size.cell * ASPECT, w * size.cell, h * size.cell * ASPECT];
}
