import { type Grid, imageBox } from './grid';
import { paint } from './paint';
import { type Theme, theme as defaultTheme } from './palette';

export const FONT = '"JetBrains Mono Variable", "JetBrains Mono", ui-monospace, monospace';
export const ASPECT = 2.08;

export interface Cursor {
  x: number;
  y: number;
  shape: 'block' | 'bar';
}

export interface Size {
  cols: number;
  rows: number;
  cw: number;
  ch: number;
}

export class CanvasView {
  readonly canvas: HTMLCanvasElement;
  private ctx: CanvasRenderingContext2D;
  private dpr = 1;
  private cwD = 8;
  private chD = 17;
  theme: Theme;
  cols = 0;
  rows = 0;
  onImage: (() => void) | null = null;
  private pictures = new Map<string, HTMLImageElement>();

  constructor(canvas: HTMLCanvasElement, t: Theme = defaultTheme) {
    this.canvas = canvas;
    const ctx = canvas.getContext('2d', { alpha: false });
    if (!ctx) throw new Error('no 2d context');
    this.ctx = ctx;
    this.theme = t;
  }

  measure(cssWidth: number, cols: number, rows: number): Size {
    this.dpr = Math.max(1, Math.min(3, window.devicePixelRatio || 1));
    this.cols = cols;
    this.rows = rows;
    this.cwD = Math.max(4, (cssWidth / cols) * this.dpr);
    this.chD = this.cwD * ASPECT;
    this.canvas.width = Math.round(this.cwD * cols);
    this.canvas.height = Math.round(this.chD * rows);
    this.canvas.style.width = `${this.canvas.width / this.dpr}px`;
    this.canvas.style.height = `${this.canvas.height / this.dpr}px`;
    return { cols, rows, cw: this.cwD / this.dpr, ch: this.chD / this.dpr };
  }

  cellAt(clientX: number, clientY: number): { x: number; y: number } {
    const box = this.canvas.getBoundingClientRect();
    const x = Math.floor(((clientX - box.left) / box.width) * this.cols);
    const y = Math.floor(((clientY - box.top) / box.height) * this.rows);
    return { x: Math.max(0, Math.min(this.cols - 1, x)), y: Math.max(0, Math.min(this.rows - 1, y)) };
  }

  cellBox(x: number, y: number): { left: number; top: number; width: number; height: number } {
    const w = this.cwD / this.dpr;
    const h = this.chD / this.dpr;
    return { left: x * w, top: y * h, width: w, height: h };
  }

  draw(grid: Grid, cursor: Cursor | null, focused: boolean, blinkOn: boolean): void {
    const { ctx, cwD, chD } = this;
    const t = this.theme;
    ctx.globalAlpha = 1;
    ctx.fillStyle = t.background;
    ctx.fillRect(0, 0, this.canvas.width, this.canvas.height);
    const lw = Math.max(1, Math.round(this.dpr));
    const ops = paint(grid, { cw: cwD, ch: chD, lw, snap: Math.round, theme: t });
    const size = cwD / 0.6;
    let font = '';
    for (const op of ops) {
      ctx.globalAlpha = op.alpha;
      if (op.k === 'rect') {
        ctx.fillStyle = op.fill;
        ctx.fillRect(op.x, op.y, op.w, op.h);
      } else if (op.k === 'path') {
        ctx.strokeStyle = op.stroke;
        ctx.lineWidth = op.lw;
        ctx.lineCap = 'round';
        ctx.lineJoin = 'round';
        ctx.stroke(new Path2D(op.d));
      } else if (op.k === 'circle') {
        ctx.fillStyle = op.fill;
        ctx.beginPath();
        ctx.arc(op.cx, op.cy, op.r, 0, Math.PI * 2);
        ctx.fill();
      } else {
        const next = `${op.italic ? 'italic ' : ''}${op.bold ? 700 : 400} ${size}px ${FONT}`;
        if (next !== font) {
          ctx.font = next;
          font = next;
        }
        ctx.fillStyle = op.fill;
        ctx.textBaseline = 'middle';
        const y = op.y + chD / 2 + size * 0.04;
        ctx.textAlign = 'center';
        if (op.single) ctx.fillText(op.text, op.x + cwD / 2, y);
        else {
          let x = op.x + cwD / 2;
          for (const ch of op.text) {
            if (ch !== ' ') ctx.fillText(ch, x, y);
            x += cwD;
          }
        }
      }
    }
    ctx.globalAlpha = 1;
    this.drawImages(grid);
    if (cursor && (blinkOn || !focused)) this.drawCursor(grid, cursor, focused, size);
  }

  private picture(src: string): HTMLImageElement | null {
    let img = this.pictures.get(src);
    if (!img) {
      img = new Image();
      img.decoding = 'async';
      img.addEventListener('load', () => this.onImage?.());
      img.src = src;
      this.pictures.set(src, img);
    }
    return img.complete && img.naturalWidth ? img : null;
  }

  private drawImages(grid: Grid): void {
    const { ctx, cwD, chD } = this;
    for (const image of grid.images) {
      const runs = grid.imageRuns(image);
      const img = runs.length ? this.picture(image.src) : null;
      if (!img) continue;
      const box = imageBox(image, cwD, chD);
      ctx.save();
      ctx.beginPath();
      for (const r of runs) ctx.rect(r.x * cwD, r.y * chD, r.w * cwD, r.h * chD);
      ctx.clip();
      ctx.imageSmoothingQuality = 'high';
      ctx.drawImage(img, box.x, box.y, box.w, box.h);
      ctx.restore();
    }
  }

  private drawCursor(grid: Grid, cursor: Cursor, focused: boolean, size: number): void {
    const { ctx, cwD, chD } = this;
    const x = cursor.x * cwD;
    const y = cursor.y * chD;
    ctx.fillStyle = this.theme.foreground;
    ctx.strokeStyle = this.theme.foreground;
    if (cursor.shape === 'bar') {
      ctx.fillRect(x, y + 1, Math.max(2, Math.round(this.dpr * 2)), chD - 2);
      return;
    }
    if (!focused) {
      const lw = Math.max(1, Math.round(this.dpr));
      ctx.lineWidth = lw;
      ctx.strokeRect(x + lw / 2, y + lw / 2, cwD - lw, chD - lw);
      return;
    }
    ctx.fillRect(x, y, cwD, chD);
    const cell = grid.at(cursor.x, cursor.y);
    if (cell && cell.ch.trim()) {
      ctx.fillStyle = this.theme.background;
      ctx.font = `${cell.attr & 1 ? 700 : 400} ${size}px ${FONT}`;
      ctx.textAlign = 'center';
      ctx.textBaseline = 'middle';
      ctx.fillText(cell.ch, x + cwD / 2, y + chD / 2 + size * 0.04);
    }
  }
}
