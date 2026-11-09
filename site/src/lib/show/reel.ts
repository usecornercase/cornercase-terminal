import { App } from '../demo/app';
import { type Areas, isEmpty } from '../demo/layout';
import { world } from '../demo/world';
import type { Cursor } from '../term/canvas';
import { type Cell, Grid, type Rect, rect } from '../term/grid';
import { DEFAULT } from '../term/palette';
import { easeInOut, seeded } from './math';
import { CHAPTERS, CHAR, DWELL, KEY, LEAD, MOVE, SETTLE, STORY, type Chapter, type Spot, type Step, TIMES, lengthOf } from './script';

export const TICK = 50;
export const LAST = Math.ceil(TIMES.total / TICK);

const SEED = 482;
const TAP_FOR = 520;

interface Segment {
  at: number;
  ms: number;
  rate: number;
  enter?: () => void;
}

interface Delta {
  at: Uint16Array;
  was: Cell[];
  now: Cell[];
}

interface Glide {
  from: number;
  to: number;
  ax: number;
  ay: number;
  bx: number;
  by: number;
}

interface Light {
  at: number;
  rect: Rect | null;
}

export interface Pointer {
  x: number;
  y: number;
  shown: boolean;
  tap: number;
}

const key = (c: Cell): string => `${c.ch}\u0000${c.fg}\u0000${c.bg}\u0000${c.attr}`;

const BLANK: Cell = { ch: ' ', fg: DEFAULT, bg: DEFAULT, attr: 0 };

function spotRect(app: App, spot: Spot): Rect | null {
  const a = app.areas();
  const named: Record<Spot, Rect> = {
    sidebar: a.sidebar,
    pane: a.pane,
    changes: a.changes,
    left: a.compact ? (isEmpty(a.sidebar) ? a.workspaces : a.sidebar) : rect(0, 0, a.workspaces.x + a.workspaces.w, app.rows),
  };
  const found = named[spot];
  return isEmpty(found) ? null : found;
}

export class Reel {
  readonly cols: number;
  readonly rows: number;
  readonly areas: Areas;
  readonly missed: string[] = [];
  readonly failed: string[] = [];
  built = 0;

  private readonly app = new App();
  private readonly segments: Segment[] = [];
  private readonly frames: Delta[] = [];
  private readonly cursors: (Cursor | null)[] = [];
  private readonly glides: Glide[] = [];
  private readonly taps: number[] = [];
  private readonly hides: number[] = [];
  private readonly lights: Light[] = [];
  private readonly pool = new Map<string, Cell>([[key(BLANK), BLANK]]);
  private readonly held: Cell[];
  private index = 0;
  private entered = false;
  private clock = 0;
  private aim: { x: number; y: number } | null = null;
  private place: { x: number; y: number };

  constructor(cols: number, rows: number, story: Record<Chapter, Step[]> = STORY) {
    this.cols = cols;
    this.rows = rows;
    this.held = Array.from({ length: cols * rows }, () => BLANK);
    this.place = { x: Math.round(cols * 0.62), y: Math.round(rows * 0.58) };
    const app = this.app;
    app.virtual = true;
    app.scripted = true;
    app.random = seeded(SEED);
    app.resize(cols, rows);
    world(app, { scripted: true });
    app.render();
    this.areas = app.areas();
    this.segments.push({ at: 0, ms: LEAD, rate: 1 });
    let at = LEAD;
    for (const name of CHAPTERS) {
      for (const step of story[name]) {
        for (const segment of this.compile(step, at)) this.segments.push(segment);
        at += lengthOf(step);
      }
    }
  }

  get done(): boolean {
    return this.built > LAST;
  }

  extend(tick: number, budget = Number.POSITIVE_INFINITY): boolean {
    const want = Math.min(tick, LAST);
    const started = budget === Number.POSITIVE_INFINITY ? 0 : performance.now();
    while (this.built <= want) {
      if (this.built > 0) this.step();
      this.record();
      if (budget !== Number.POSITIVE_INFINITY && performance.now() - started > budget) break;
    }
    return this.built > want;
  }

  delta(tick: number): Delta {
    const frame = this.frames[tick];
    if (!frame) throw new Error(`tick ${tick} is not built`);
    return frame;
  }

  cursorAt(tick: number): Cursor | null {
    return this.cursors[Math.min(tick, this.built - 1)] ?? null;
  }

  pointerAt(ms: number): Pointer {
    let glide: Glide | null = null;
    for (const g of this.glides) if (g.from <= ms) glide = g;
    if (!glide) return { x: this.place.x, y: this.place.y, shown: false, tap: -1 };
    let hidden = -1;
    for (const h of this.hides) if (h <= ms) hidden = h;
    let tapped = -1;
    for (const t of this.taps) if (t <= ms) tapped = t;
    const t = easeInOut((ms - glide.from) / (glide.to - glide.from));
    return {
      x: glide.ax + (glide.bx - glide.ax) * t,
      y: glide.ay + (glide.by - glide.ay) * t,
      shown: hidden < glide.from,
      tap: tapped >= 0 && ms - tapped < TAP_FOR ? (ms - tapped) / TAP_FOR : -1,
    };
  }

  spotAt(ms: number): Rect | null {
    let found: Rect | null = null;
    for (const light of this.lights) if (light.at <= ms) found = light.rect;
    return found;
  }

  private compile(step: Step, at: number): Segment[] {
    if ('wait' in step) return [{ at, ms: step.wait, rate: step.rate ?? 1 }];
    if ('click' in step) {
      return [
        { at, ms: MOVE, rate: 1, enter: () => this.glide(at, this.find(step.click, step.nth, step.dx) ?? (step.alt ? this.find(step.alt, step.nth) : null), step.orKey ? null : step.click) },
        { at: at + MOVE, ms: DWELL, rate: 1, enter: () => this.hover() },
        { at: at + MOVE + DWELL, ms: SETTLE, rate: 1, enter: () => this.press(at + MOVE + DWELL, step.orKey) },
      ];
    }
    if ('point' in step) {
      return [
        { at, ms: MOVE, rate: 1, enter: () => this.glide(at, this.find(step.point, step.nth, step.dx), step.point) },
        { at: at + MOVE, ms: 0, rate: 1, enter: () => this.hover() },
      ];
    }
    if ('key' in step) return [{ at, ms: KEY, rate: 1, enter: () => this.app.key({ key: step.key }) }];
    if ('type' in step) return [...step.type].map((ch, i) => ({ at: at + i * CHAR, ms: CHAR, rate: 1, enter: () => this.app.key({ key: ch }) }));
    if ('run' in step) return [{ at, ms: 0, rate: 1, enter: () => step.run(this.app) }];
    if ('expect' in step) return [{ at, ms: 0, rate: 1, enter: () => void (step.expect(this.app) || this.failed.push(step.what)) }];
    if ('spot' in step) return [{ at, ms: 0, rate: 1, enter: () => void this.lights.push({ at, rect: step.spot ? spotRect(this.app, step.spot) : null }) }];
    if ('pointer' in step) return [{ at, ms: 0, rate: 1, enter: () => void this.hides.push(at) }];
    return [];
  }

  private find(text: string, nth = 0, dx = 0): { x: number; y: number } | null {
    const app = this.app;
    if (app.stale) app.render();
    let found: { x: number; y: number } | null = null;
    if (nth < 0) {
      for (let i = 0; ; i++) {
        const next = app.textAt(text, i);
        if (!next) break;
        found = next;
      }
    } else found = app.textAt(text, nth);
    return found ? { x: found.x + dx, y: found.y } : null;
  }

  private glide(at: number, target: { x: number; y: number } | null, text: string | null): void {
    this.aim = target;
    if (!target) {
      if (text) this.missed.push(text);
      return;
    }
    const to = { x: target.x + 0.5, y: target.y + 0.5 };
    this.glides.push({ from: at, to: at + MOVE, ax: this.place.x, ay: this.place.y, bx: to.x, by: to.y });
    this.place = to;
  }

  private hover(): void {
    if (!this.aim) return;
    this.app.hover = { ...this.aim };
    this.app.dirty();
  }

  private press(at: number, orKey?: string): void {
    const aim = this.aim;
    if (!aim) {
      if (orKey) this.app.key({ key: orKey });
      return;
    }
    this.taps.push(at);
    this.app.pointerDown(aim.x, aim.y, 0);
    this.app.pointerUp();
  }

  private step(): void {
    const app = this.app;
    const end = this.clock + TICK;
    while (this.clock < end) {
      const segment = this.segments[this.index];
      if (!segment) {
        app.advance(end - this.clock);
        this.clock = end;
        break;
      }
      if (!this.entered) {
        this.entered = true;
        segment.enter?.();
      }
      const take = Math.min(segment.at + segment.ms - this.clock, end - this.clock);
      if (take > 0) {
        app.advance(take * segment.rate);
        this.clock += take;
      }
      if (this.clock >= segment.at + segment.ms) {
        this.index += 1;
        this.entered = false;
      }
    }
  }

  private record(): void {
    const { grid, cursor } = this.app.render();
    const at: number[] = [];
    const was: Cell[] = [];
    const now: Cell[] = [];
    const held = this.held;
    grid.cells.forEach((cell, i) => {
      const before = held[i];
      if (cell.ch === before.ch && cell.fg === before.fg && cell.bg === before.bg && cell.attr === before.attr) return;
      const id = key(cell);
      let kept = this.pool.get(id);
      if (!kept) {
        kept = { ch: cell.ch, fg: cell.fg, bg: cell.bg, attr: cell.attr };
        this.pool.set(id, kept);
      }
      at.push(i);
      was.push(before);
      now.push(kept);
      held[i] = kept;
    });
    this.frames.push({ at: Uint16Array.from(at), was, now });
    this.cursors.push(cursor ? { ...cursor } : null);
    this.built += 1;
  }
}

export class Screen {
  readonly grid: Grid;
  at = -1;
  private readonly reel: Reel;
  private top = Number.POSITIVE_INFINITY;
  private bottom = -1;

  constructor(reel: Reel) {
    this.reel = reel;
    this.grid = new Grid(reel.cols, reel.rows);
    this.grid.cells = this.grid.cells.map(() => BLANK);
  }

  get cursor(): Cursor | null {
    return this.at < 0 ? null : this.reel.cursorAt(this.at);
  }

  seek(tick: number): void {
    const to = Math.max(0, Math.min(tick, this.reel.built - 1));
    while (this.at < to) this.apply(this.reel.delta(++this.at), true);
    while (this.at > to) this.apply(this.reel.delta(this.at--), false);
  }

  dirty(): [number, number] | null {
    if (this.bottom < 0) return null;
    const rows: [number, number] = [this.top, this.bottom];
    this.top = Number.POSITIVE_INFINITY;
    this.bottom = -1;
    return rows;
  }

  private apply(delta: Delta, forward: boolean): void {
    const cells = this.grid.cells;
    const cols = this.grid.cols;
    const from = forward ? delta.now : delta.was;
    for (let i = 0; i < delta.at.length; i++) {
      const index = delta.at[i];
      cells[index] = from[i];
      const row = (index / cols) | 0;
      if (row < this.top) this.top = row;
      if (row > this.bottom) this.bottom = row;
    }
  }
}
