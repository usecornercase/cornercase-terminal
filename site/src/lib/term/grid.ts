import { DEFAULT } from './palette';

export const BOLD = 1;
export const DIM = 2;
export const ITALIC = 4;
export const UNDERLINE = 8;
export const INVERSE = 16;
export const STRIKE = 32;
export const IMAGE_CELL = '\u{10EEEE}';

export interface Cell {
  ch: string;
  fg: number;
  bg: number;
  attr: number;
}

export interface Style {
  fg?: number;
  bg?: number;
  add?: number;
  sub?: number;
}

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface GridImage {
  r: Rect;
  src: string;
  width: number;
  height: number;
}

export const rect = (x: number, y: number, w: number, h: number): Rect => ({ x, y, w: Math.max(0, w), h: Math.max(0, h) });

export function imageBox(image: GridImage, cw: number, ch: number): { x: number; y: number; w: number; h: number } {
  const room = { w: image.r.w * cw, h: image.r.h * ch };
  const scale = Math.min(1, room.w / image.width, room.h / image.height);
  const w = image.width * scale;
  const h = image.height * scale;
  return { x: image.r.x * cw + (room.w - w) / 2, y: image.r.y * ch + (room.h - h) / 2, w, h };
}

export const contains = (r: Rect, x: number, y: number): boolean => x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h;

export const blank = (): Cell => ({ ch: ' ', fg: DEFAULT, bg: DEFAULT, attr: 0 });

export class Grid {
  cols: number;
  rows: number;
  cells: Cell[];
  images: GridImage[] = [];

  constructor(cols: number, rows: number) {
    this.cols = cols;
    this.rows = rows;
    this.cells = Array.from({ length: cols * rows }, blank);
  }

  at(x: number, y: number): Cell | undefined {
    if (x < 0 || y < 0 || x >= this.cols || y >= this.rows) return undefined;
    return this.cells[y * this.cols + x];
  }

  reset(): void {
    this.images = [];
    for (const c of this.cells) {
      c.ch = ' ';
      c.fg = DEFAULT;
      c.bg = DEFAULT;
      c.attr = 0;
    }
  }

  style(x: number, y: number, s: Style): void {
    const c = this.at(x, y);
    if (!c) return;
    if (s.fg !== undefined) c.fg = s.fg;
    if (s.bg !== undefined) c.bg = s.bg;
    if (s.add) c.attr |= s.add;
    if (s.sub) c.attr &= ~s.sub;
  }

  put(x: number, y: number, ch: string, s: Style = {}): void {
    const c = this.at(x, y);
    if (!c) return;
    c.ch = ch;
    this.style(x, y, s);
  }

  text(x: number, y: number, str: string, s: Style = {}, max = Infinity): number {
    let col = x;
    for (const ch of str) {
      if (col - x >= max) break;
      this.put(col, y, ch, s);
      col += 1;
    }
    return col;
  }

  fill(r: Rect, s: Style, ch?: string): void {
    for (let y = r.y; y < r.y + r.h; y++) {
      for (let x = r.x; x < r.x + r.w; x++) {
        if (ch === undefined) this.style(x, y, s);
        else this.put(x, y, ch, s);
      }
    }
  }

  clear(r: Rect): void {
    for (let y = r.y; y < r.y + r.h; y++) {
      for (let x = r.x; x < r.x + r.w; x++) {
        const c = this.at(x, y);
        if (!c) continue;
        c.ch = ' ';
        c.fg = DEFAULT;
        c.bg = DEFAULT;
        c.attr = 0;
      }
    }
  }

  box(r: Rect, s: Style, title?: string, titleStyle: Style = {}): void {
    if (r.w < 2 || r.h < 2) return;
    const right = r.x + r.w - 1;
    const bottom = r.y + r.h - 1;
    for (let x = r.x + 1; x < right; x++) {
      this.put(x, r.y, '─', s);
      this.put(x, bottom, '─', s);
    }
    for (let y = r.y + 1; y < bottom; y++) {
      this.put(r.x, y, '│', s);
      this.put(right, y, '│', s);
    }
    this.put(r.x, r.y, '╭', s);
    this.put(right, r.y, '╮', s);
    this.put(r.x, bottom, '╰', s);
    this.put(right, bottom, '╯', s);
    if (title) this.text(r.x + 2, r.y, ` ${title} `, titleStyle, r.w - 3);
  }

  copy(from: Grid, at: Rect, dx = 0, dy = 0): void {
    for (let y = 0; y < at.h; y++) {
      for (let x = 0; x < at.w; x++) {
        const src = from.at(x + dx, y + dy);
        const dst = this.at(at.x + x, at.y + y);
        if (!src || !dst) continue;
        dst.ch = src.ch;
        dst.fg = src.fg;
        dst.bg = src.bg;
        dst.attr = src.attr;
      }
    }
  }

  crop(r: Rect): Grid {
    const out = new Grid(r.w, r.h);
    out.copy(this, rect(0, 0, r.w, r.h), r.x, r.y);
    out.images = this.images.map((image) => ({ ...image, r: { ...image.r, x: image.r.x - r.x, y: image.r.y - r.y } }));
    return out;
  }

  imageRuns(image: GridImage): Rect[] {
    const runs: Rect[] = [];
    for (let y = image.r.y; y < image.r.y + image.r.h; y++) {
      let start = -1;
      for (let x = image.r.x; x <= image.r.x + image.r.w; x++) {
        const shown = x < image.r.x + image.r.w && this.at(x, y)?.ch.startsWith(IMAGE_CELL);
        if (shown && start < 0) start = x;
        if (!shown && start >= 0) {
          runs.push(rect(start, y, x - start, 1));
          start = -1;
        }
      }
    }
    return runs;
  }

  line(y: number): string {
    let s = '';
    for (let x = 0; x < this.cols; x++) s += this.cells[y * this.cols + x].ch;
    return s;
  }

  plain(): string {
    const lines: string[] = [];
    for (let y = 0; y < this.rows; y++) lines.push(this.line(y).trimEnd());
    return lines.join('\n').replace(/\n+$/, '');
  }
}
