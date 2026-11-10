import { issuesArea } from '../demo/layout';
import { CanvasView } from '../term/canvas';
import { type Grid, rect } from '../term/grid';
import { HEIGHT, LAND, LAYERS, type Layer, lift, turnOf } from './layers';
import { all, flag, one, paint } from './paint';
import { type Reel, Screen, TICK } from './reel';
import type { Scroll } from './scroll';
import { COMPACT, IDLE } from './script';
import { type Still, still } from './stills';
import { ASPECT, BAR, PAD, type Size, windowOf } from './track';

const PHONE_CELL = 0.74;
const REST = IDLE[1] / TICK;

interface Sheet {
  el: HTMLElement;
  view: CanvasView | null;
  grid: (() => Grid) | null;
  cell: number;
  docked: number;
  x: number;
  y: number;
}

export class Explode {
  private readonly scroll: Scroll;
  private readonly sheets = {} as Record<Layer, Sheet>;
  private readonly items = all('[data-feature]');
  private density = 1;
  private pending: Layer[] = [];

  constructor(scroll: Scroll) {
    this.scroll = scroll;
    for (const name of LAYERS) {
      const el = one(`[data-layer="${name}"]`);
      const canvas = el.querySelector('canvas');
      this.sheets[name] = { el, view: canvas ? new CanvasView(canvas) : null, grid: null, cell: 1, docked: 0, x: 0, y: 0 };
    }
  }

  measure(size: Size, reel: Reel, density: number): void {
    const { cols, rows, cell } = size;
    const line = cell * ASPECT;
    const side = reel.areas.pane.x - 1;
    const first = () => {
      const screen = new Screen(reel);
      reel.extend(REST);
      screen.seek(REST);
      return screen.grid;
    };
    const cut = (name: Still) => () => still(name, cols, rows);
    const dialog = issuesArea(cols, rows);
    const [, , width, height] = windowOf(size);
    this.density = density;
    if (cols < COMPACT) {
      this.put('projects', 0, 0, cell, cut('menu'));
      this.put('agents', 0, 0, cell, () => first().crop(rect(0, 0, cols, rows)));
    } else {
      this.put('projects', 0, 0, cell, () => first().crop(rect(0, 0, side, rows)));
      this.put('agents', reel.areas.pane.x * cell, 0, cell, () => first().crop(reel.areas.pane));
    }
    this.put('issues', dialog.x * cell, dialog.y * line, cell, cut('issues'));
    for (const name of ['changes', 'files', 'flow'] as const) this.put(name, 0, 0, cell, cut(name), cols);
    this.put('phone', cols * cell * 0.62, rows * line + 50, cell * PHONE_CELL, cut('phone'));
    this.box('cli', PAD + (cols < COMPACT ? 10 : reel.areas.pane.x * cell + 70), BAR + PAD + 40, Math.min(560, cols * cell - 20), 48);
    this.box('server', -46, -36, width + 92, height + 150);
    this.pending = [...LAYERS];
  }

  update(rig: string): void {
    const p = this.scroll.progress('features');
    const seen = p > 0.1 && p < 0.99;
    const turn = seen ? turnOf(p) : -2;
    this.items.forEach((item, i) => {
      flag(item, 'is-on', i === turn || (turn >= LAYERS.length && i === LAYERS.length - 1));
      flag(item, 'was-on', turn > i);
    });
    if (p > 0 && p < 1) this.draw();
    LAYERS.forEach((name, i) => {
      const { el, x, y } = this.sheets[name];
      const up = seen ? lift(p, i) : 0;
      const raised = up > 0.004;
      paint(el, 'visibility', raised ? 'visible' : 'hidden');
      if (!raised) return;
      const on = i === turn && p < LAND[0];
      paint(el, 'opacity', Math.min(1, up * 3).toFixed(3));
      paint(el, 'transform', `${rig} translate3d(${x.toFixed(1)}px,${y.toFixed(1)}px,${(HEIGHT[name] * up).toFixed(1)}px)`);
      flag(el, 'is-on', on);
    });
  }

  private put(name: Layer, x: number, y: number, cell: number, grid: () => Grid, docked = 0): void {
    const sheet = this.sheets[name];
    sheet.grid = grid;
    sheet.cell = cell;
    sheet.docked = docked;
    sheet.x = x + PAD;
    sheet.y = y + BAR + PAD;
  }

  private box(name: Layer, x: number, y: number, width: number, height: number): void {
    const sheet = this.sheets[name];
    sheet.x = x;
    sheet.y = y;
    paint(sheet.el, 'width', `${width.toFixed(1)}px`);
    paint(sheet.el, 'height', `${height.toFixed(1)}px`);
  }

  private draw(): void {
    const name = this.pending.shift();
    if (!name) return;
    const sheet = this.sheets[name];
    const { view, grid, cell, docked } = sheet;
    if (!view || !grid) return;
    const cut = grid();
    if (docked) sheet.x = PAD + (docked - cut.cols) * cell;
    view.measure(cut.cols * cell, cut.cols, cut.rows, this.density);
    view.draw(cut, null, false, true);
  }
}
