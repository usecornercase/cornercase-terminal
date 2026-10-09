import { BOLD, DIM, Grid, INVERSE, ITALIC, STRIKE, UNDERLINE } from './grid';
import { DEFAULT, rgb } from './palette';

interface Pen {
  fg: number;
  bg: number;
  attr: number;
}

const MARK = /^\p{M}$/u;

const ATTRS: Record<number, number> = { 1: BOLD, 2: DIM, 3: ITALIC, 4: UNDERLINE, 7: INVERSE, 9: STRIKE };
const CLEARS: Record<number, number> = { 22: BOLD | DIM, 23: ITALIC, 24: UNDERLINE, 27: INVERSE, 29: STRIKE };

function color(params: number[], i: number): [number, number] {
  if (params[i + 1] === 5) return [params[i + 2] ?? 0, i + 2];
  if (params[i + 1] === 2) return [rgb(params[i + 2] ?? 0, params[i + 3] ?? 0, params[i + 4] ?? 0), i + 4];
  return [DEFAULT, i];
}

function sgr(pen: Pen, raw: string): void {
  const params = raw === '' ? [0] : raw.split(/[;:]/).map((p) => (p === '' ? 0 : Number(p)));
  for (let i = 0; i < params.length; i++) {
    const p = params[i];
    if (p === 0) {
      pen.fg = DEFAULT;
      pen.bg = DEFAULT;
      pen.attr = 0;
    } else if (ATTRS[p]) pen.attr |= ATTRS[p];
    else if (CLEARS[p]) pen.attr &= ~CLEARS[p];
    else if (p >= 30 && p <= 37) pen.fg = p - 30;
    else if (p >= 90 && p <= 97) pen.fg = p - 90 + 8;
    else if (p >= 40 && p <= 47) pen.bg = p - 40;
    else if (p >= 100 && p <= 107) pen.bg = p - 100 + 8;
    else if (p === 39) pen.fg = DEFAULT;
    else if (p === 49) pen.bg = DEFAULT;
    else if (p === 38) [pen.fg, i] = color(params, i);
    else if (p === 48) [pen.bg, i] = color(params, i);
  }
}

export function parseAnsi(text: string, cols?: number, rows?: number): Grid {
  const lines = text.replace(/\r/g, '').replace(/\n$/, '').split('\n');
  const visible = (line: string) => [...line.replace(/\x1b\[[0-9;:?]*[A-Za-z]/g, '')].filter((c) => !MARK.test(c)).length;
  const width = cols ?? Math.max(...lines.map(visible));
  const grid = new Grid(width, rows ?? lines.length);
  const pen: Pen = { fg: DEFAULT, bg: DEFAULT, attr: 0 };
  lines.slice(0, grid.rows).forEach((line, y) => {
    let x = 0;
    let i = 0;
    while (i < line.length) {
      if (line[i] === '\x1b' && line[i + 1] === '[') {
        const end = line.slice(i + 2).search(/[A-Za-z]/);
        if (end < 0) break;
        const final = line[i + 2 + end];
        if (final === 'm') sgr(pen, line.slice(i + 2, i + 2 + end));
        i += end + 3;
        continue;
      }
      const ch = String.fromCodePoint(line.codePointAt(i) ?? 32);
      i += ch.length;
      if (MARK.test(ch)) {
        const prev = grid.at(x - 1, y);
        if (prev) prev.ch += ch;
        continue;
      }
      const cell = grid.at(x, y);
      if (cell) {
        cell.ch = ch;
        cell.fg = pen.fg;
        cell.bg = pen.bg;
        cell.attr = pen.attr;
      }
      x += 1;
    }
  });
  return grid;
}
