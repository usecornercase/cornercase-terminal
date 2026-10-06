import { glyph, isGlyph } from './glyphs';
import { BOLD, DIM, type Grid, INVERSE, ITALIC, STRIKE, UNDERLINE } from './grid';
import { DEFAULT, type Theme, bg as bgColor, fg as fgColor, theme as defaultTheme } from './palette';

export type Op =
  | { k: 'rect'; x: number; y: number; w: number; h: number; fill: string; alpha: number }
  | { k: 'path'; d: string; stroke: string; lw: number; alpha: number }
  | { k: 'circle'; cx: number; cy: number; r: number; fill: string; alpha: number }
  | { k: 'text'; x: number; y: number; text: string; fill: string; bold: boolean; italic: boolean; alpha: number; cells: number; single: boolean };

export interface Metrics {
  cw: number;
  ch: number;
  lw: number;
  snap?: (v: number) => number;
  theme?: Theme;
  dim?: number;
}

const ascii = (ch: string) => ch.length === 1 && ch.charCodeAt(0) < 0x7f;

export function paint(grid: Grid, m: Metrics): Op[] {
  const t = m.theme ?? defaultTheme;
  const snap = m.snap ?? ((v: number) => v);
  const dimAlpha = m.dim ?? 0.5;
  const backgrounds: Op[] = [];
  const shapes: Op[] = [];
  const texts: Op[] = [];
  for (let row = 0; row < grid.rows; row++) {
    const y = row * m.ch;
    let run: { x: number; text: string; fill: string; bold: boolean; italic: boolean; alpha: number; cells: number } | null = null;
    let bgRun: { x: number; w: number; fill: string } | null = null;
    const flushText = () => {
      if (run && run.text.trim()) {
        const text = run.text.replace(/\s+$/, '');
        texts.push({ k: 'text', x: run.x, y, text, fill: run.fill, bold: run.bold, italic: run.italic, alpha: run.alpha, cells: [...text].length, single: false });
      }
      run = null;
    };
    const flushBg = () => {
      if (bgRun) {
        const x0 = snap(bgRun.x);
        const y0 = snap(y);
        backgrounds.push({ k: 'rect', x: x0, y: y0, w: snap(bgRun.x + bgRun.w) - x0, h: snap(y + m.ch) - y0, fill: bgRun.fill, alpha: 1 });
      }
      bgRun = null;
    };
    for (let col = 0; col < grid.cols; col++) {
      const cell = grid.cells[row * grid.cols + col];
      const x = col * m.cw;
      const inverse = (cell.attr & INVERSE) !== 0;
      const fgIndex = inverse ? cell.bg : cell.fg;
      const bgIndex = inverse ? cell.fg : cell.bg;
      const fill = inverse && fgIndex === DEFAULT ? t.background : fgColor(fgIndex, t);
      const back = inverse && bgIndex === DEFAULT ? t.foreground : bgIndex === DEFAULT ? null : bgColor(bgIndex, t);
      if (back) {
        if (bgRun && bgRun.fill === back && Math.abs(bgRun.x + bgRun.w - x) < 0.01) bgRun.w += m.cw;
        else {
          flushBg();
          bgRun = { x, w: m.cw, fill: back };
        }
      } else flushBg();
      const alpha = cell.attr & DIM ? dimAlpha : 1;
      const bold = (cell.attr & BOLD) !== 0;
      const italic = (cell.attr & ITALIC) !== 0;
      if (cell.attr & UNDERLINE) {
        shapes.push({ k: 'rect', x, y: snap(y + m.ch - Math.max(m.lw, 1) * 2), w: m.cw, h: m.lw, fill, alpha });
      }
      if (cell.attr & STRIKE) {
        shapes.push({ k: 'rect', x, y: snap(y + m.ch / 2), w: m.cw, h: m.lw, fill, alpha });
      }
      if (isGlyph(cell.ch)) {
        flushText();
        for (const p of glyph(cell.ch, x, y, m.cw, m.ch, m.lw, snap) ?? []) {
          if (p.k === 'rect') shapes.push({ k: 'rect', x: p.x, y: p.y, w: p.w, h: p.h, fill, alpha: alpha * p.alpha });
          else if (p.k === 'path') shapes.push({ k: 'path', d: p.d, stroke: fill, lw: p.lw, alpha });
          else shapes.push({ k: 'circle', cx: p.cx, cy: p.cy, r: p.r, fill, alpha });
        }
        continue;
      }
      if (!ascii(cell.ch)) {
        flushText();
        if (cell.ch.trim()) {
          texts.push({ k: 'text', x, y, text: cell.ch, fill, bold, italic, alpha, cells: 1, single: true });
        }
        continue;
      }
      if (run && run.fill === fill && run.bold === bold && run.italic === italic && run.alpha === alpha) {
        run.text += cell.ch;
        run.cells += 1;
      } else if (cell.ch === ' ' && run) {
        flushText();
      } else {
        flushText();
        if (cell.ch !== ' ') run = { x, text: cell.ch, fill, bold, italic, alpha, cells: 1 };
      }
    }
    flushText();
    flushBg();
  }
  return [...backgrounds, ...shapes, ...texts];
}
