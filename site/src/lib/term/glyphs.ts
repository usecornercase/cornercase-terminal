export type Prim =
  | { k: 'rect'; x: number; y: number; w: number; h: number; alpha: number }
  | { k: 'path'; d: string; lw: number }
  | { k: 'circle'; cx: number; cy: number; r: number };

type Arms = [number, number, number, number];

const LINES: Record<string, Arms> = {
  '─': [0, 1, 0, 1], '━': [0, 2, 0, 2], '│': [1, 0, 1, 0], '┃': [2, 0, 2, 0],
  '┄': [0, 1, 0, 1], '┅': [0, 2, 0, 2], '┆': [1, 0, 1, 0], '┇': [2, 0, 2, 0],
  '┈': [0, 1, 0, 1], '┉': [0, 2, 0, 2], '┊': [1, 0, 1, 0], '┋': [2, 0, 2, 0],
  '╌': [0, 1, 0, 1], '╍': [0, 2, 0, 2], '╎': [1, 0, 1, 0], '╏': [2, 0, 2, 0],
  '┌': [0, 1, 1, 0], '┍': [0, 2, 1, 0], '┎': [0, 1, 2, 0], '┏': [0, 2, 2, 0],
  '┐': [0, 0, 1, 1], '┑': [0, 0, 1, 2], '┒': [0, 0, 2, 1], '┓': [0, 0, 2, 2],
  '└': [1, 1, 0, 0], '┕': [1, 2, 0, 0], '┖': [2, 1, 0, 0], '┗': [2, 2, 0, 0],
  '┘': [1, 0, 0, 1], '┙': [1, 0, 0, 2], '┚': [2, 0, 0, 1], '┛': [2, 0, 0, 2],
  '├': [1, 1, 1, 0], '┝': [1, 2, 1, 0], '┞': [2, 1, 1, 0], '┟': [1, 1, 2, 0],
  '┠': [2, 1, 2, 0], '┡': [2, 2, 1, 0], '┢': [1, 2, 2, 0], '┣': [2, 2, 2, 0],
  '┤': [1, 0, 1, 1], '┥': [1, 0, 1, 2], '┦': [2, 0, 1, 1], '┧': [1, 0, 2, 1],
  '┨': [2, 0, 2, 1], '┩': [2, 0, 1, 2], '┪': [1, 0, 2, 2], '┫': [2, 0, 2, 2],
  '┬': [0, 1, 1, 1], '┭': [0, 1, 1, 2], '┮': [0, 2, 1, 1], '┯': [0, 2, 1, 2],
  '┰': [0, 1, 2, 1], '┱': [0, 1, 2, 2], '┲': [0, 2, 2, 1], '┳': [0, 2, 2, 2],
  '┴': [1, 1, 0, 1], '┵': [1, 1, 0, 2], '┶': [1, 2, 0, 1], '┷': [1, 2, 0, 2],
  '┸': [2, 1, 0, 1], '┹': [2, 1, 0, 2], '┺': [2, 2, 0, 1], '┻': [2, 2, 0, 2],
  '┼': [1, 1, 1, 1], '╋': [2, 2, 2, 2], '⎿': [1, 1, 0, 0],
  '═': [0, 3, 0, 3], '║': [3, 0, 3, 0], '╒': [0, 3, 1, 0], '╓': [0, 1, 3, 0],
  '╔': [0, 3, 3, 0], '╕': [0, 0, 1, 3], '╖': [0, 0, 3, 1], '╗': [0, 0, 3, 3],
  '╘': [1, 3, 0, 0], '╙': [3, 1, 0, 0], '╚': [3, 3, 0, 0], '╛': [1, 0, 0, 3],
  '╜': [3, 0, 0, 1], '╝': [3, 0, 0, 3], '╞': [1, 3, 1, 0], '╟': [3, 1, 3, 0],
  '╠': [3, 3, 3, 0], '╡': [1, 0, 1, 3], '╢': [3, 0, 3, 1], '╣': [3, 0, 3, 3],
  '╤': [0, 3, 1, 3], '╥': [0, 1, 3, 1], '╦': [0, 3, 3, 3], '╧': [1, 3, 0, 3],
  '╨': [3, 1, 0, 1], '╩': [3, 3, 0, 3], '╪': [1, 3, 1, 3], '╫': [3, 1, 3, 1],
  '╬': [3, 3, 3, 3],
  '╴': [0, 0, 0, 1], '╵': [1, 0, 0, 0], '╶': [0, 1, 0, 0], '╷': [0, 0, 1, 0],
  '╸': [0, 0, 0, 2], '╹': [2, 0, 0, 0], '╺': [0, 2, 0, 0], '╻': [0, 0, 2, 0],
  '╼': [0, 2, 0, 1], '╽': [1, 0, 2, 0], '╾': [0, 1, 0, 2], '╿': [2, 0, 1, 0],
};

type Box = [number, number, number, number, number?];

const BLOCKS: Record<string, Box[]> = {
  '▀': [[0, 0, 1, 0.5]], '▄': [[0, 0.5, 1, 0.5]], '█': [[0, 0, 1, 1]],
  '▌': [[0, 0, 0.5, 1]], '▐': [[0.5, 0, 0.5, 1]],
  '▁': [[0, 7 / 8, 1, 1 / 8]], '▂': [[0, 6 / 8, 1, 2 / 8]], '▃': [[0, 5 / 8, 1, 3 / 8]],
  '▅': [[0, 3 / 8, 1, 5 / 8]], '▆': [[0, 2 / 8, 1, 6 / 8]], '▇': [[0, 1 / 8, 1, 7 / 8]],
  '▉': [[0, 0, 7 / 8, 1]], '▊': [[0, 0, 6 / 8, 1]], '▋': [[0, 0, 5 / 8, 1]],
  '▍': [[0, 0, 3 / 8, 1]], '▎': [[0, 0, 2 / 8, 1]], '▏': [[0, 0, 1 / 8, 1]],
  '▔': [[0, 0, 1, 1 / 8]], '▕': [[7 / 8, 0, 1 / 8, 1]],
  '░': [[0, 0, 1, 1, 0.25]], '▒': [[0, 0, 1, 1, 0.5]], '▓': [[0, 0, 1, 1, 0.75]],
  '▖': [[0, 0.5, 0.5, 0.5]], '▗': [[0.5, 0.5, 0.5, 0.5]], '▘': [[0, 0, 0.5, 0.5]], '▝': [[0.5, 0, 0.5, 0.5]],
  '▙': [[0, 0, 0.5, 1], [0.5, 0.5, 0.5, 0.5]], '▛': [[0, 0, 1, 0.5], [0, 0.5, 0.5, 0.5]],
  '▜': [[0, 0, 1, 0.5], [0.5, 0.5, 0.5, 0.5]], '▟': [[0.5, 0, 0.5, 1], [0, 0.5, 0.5, 0.5]],
  '▚': [[0, 0, 0.5, 0.5], [0.5, 0.5, 0.5, 0.5]], '▞': [[0.5, 0, 0.5, 0.5], [0, 0.5, 0.5, 0.5]],
};

const ROUND = new Set(['╭', '╮', '╯', '╰']);

const SYMBOLS = new Set(['❯', '⌕', '✓', '●', '≡', '→', '←']);

const BRAILLE_DOTS: [number, number, number][] = [
  [0x01, 0, 0], [0x02, 0, 1], [0x04, 0, 2], [0x08, 1, 0],
  [0x10, 1, 1], [0x20, 1, 2], [0x40, 0, 3], [0x80, 1, 3],
];

export const isGlyph = (ch: string): boolean => {
  if (ch in LINES || ch in BLOCKS || ROUND.has(ch) || SYMBOLS.has(ch)) return true;
  const code = ch.codePointAt(0) ?? 0;
  return code > 0x2800 && code <= 0x28ff;
};

const r = (x: number, y: number, w: number, h: number, alpha = 1): Prim => ({ k: 'rect', x, y, w, h, alpha });

function lines(arms: Arms, x: number, y: number, cw: number, ch: number, lw: number, snap: (v: number) => number): Prim[] {
  const out: Prim[] = [];
  const [up, right, down, left] = arms;
  const thick = (w: number) => (w === 2 ? Math.max(lw * 2, lw + 1) : lw);
  const size = (a: number, b: number) => Math.max(a ? thick(a === 3 ? 1 : a) : 0, b ? thick(b === 3 ? 1 : b) : 0);
  const left0 = snap(x);
  const right0 = snap(x + cw);
  const top0 = snap(y);
  const bottom0 = snap(y + ch);
  const midX = x + cw / 2;
  const midY = y + ch / 2;
  const vT = size(up, down);
  const hT = size(left, right);
  const top = left || right ? snap(midY - hT / 2) : midY;
  const bottom = left || right ? top + hT : midY;
  const start = up || down ? snap(midX - vT / 2) : midX;
  const end = up || down ? start + vT : midX;
  const gap = Math.max(lw, 1) * 1.5;
  const vertical = (w: number, y0: number, y1: number) => {
    if (w === 3) {
      out.push(r(snap(midX - gap - lw / 2), y0, lw, y1 - y0), r(snap(midX + gap - lw / 2), y0, lw, y1 - y0));
      return;
    }
    const t = thick(w);
    out.push(r(snap(midX - t / 2), y0, t, y1 - y0));
  };
  const horizontal = (w: number, x0: number, x1: number) => {
    if (w === 3) {
      out.push(r(x0, snap(midY - gap - lw / 2), x1 - x0, lw), r(x0, snap(midY + gap - lw / 2), x1 - x0, lw));
      return;
    }
    const t = thick(w);
    out.push(r(x0, snap(midY - t / 2), x1 - x0, t));
  };
  if (up && up === down) vertical(up, top0, bottom0);
  else {
    if (up) vertical(up, top0, bottom);
    if (down) vertical(down, top, bottom0);
  }
  if (left && left === right) horizontal(left, left0, right0);
  else {
    if (left) horizontal(left, left0, end);
    if (right) horizontal(right, start, right0);
  }
  return out;
}

function rounded(ch: string, x: number, y: number, cw: number, h: number, lw: number): Prim {
  const cx = x + cw / 2;
  const cy = y + h / 2;
  const rad = cw / 2;
  const f = (n: number) => n.toFixed(2);
  const d: Record<string, string> = {
    '╭': `M${f(cx)} ${f(y + h)}L${f(cx)} ${f(cy + rad)}A${f(rad)} ${f(rad)} 0 0 1 ${f(x + cw)} ${f(cy)}`,
    '╮': `M${f(cx)} ${f(y + h)}L${f(cx)} ${f(cy + rad)}A${f(rad)} ${f(rad)} 0 0 0 ${f(x)} ${f(cy)}`,
    '╯': `M${f(cx)} ${f(y)}L${f(cx)} ${f(cy - rad)}A${f(rad)} ${f(rad)} 0 0 1 ${f(x)} ${f(cy)}`,
    '╰': `M${f(cx)} ${f(y)}L${f(cx)} ${f(cy - rad)}A${f(rad)} ${f(rad)} 0 0 0 ${f(x + cw)} ${f(cy)}`,
  };
  return { k: 'path', d: d[ch], lw };
}

function symbol(ch: string, x: number, y: number, cw: number, h: number, lw: number): Prim[] {
  const f = (n: number) => n.toFixed(2);
  const px = (fx: number) => f(x + fx * cw);
  const py = (fy: number) => f(y + fy * h);
  const stroke = Math.max(lw * 1.5, cw * 0.13);
  if (ch === '❯') return [{ k: 'path', d: `M${px(0.28)} ${py(0.33)}L${px(0.72)} ${py(0.5)}L${px(0.28)} ${py(0.67)}`, lw: stroke * 1.15 }];
  if (ch === '✓') return [{ k: 'path', d: `M${px(0.14)} ${py(0.52)}L${px(0.4)} ${py(0.66)}L${px(0.88)} ${py(0.34)}`, lw: stroke }];
  if (ch === '●') return [{ k: 'circle', cx: x + cw / 2, cy: y + h / 2, r: cw * 0.33 }];
  if (ch === '≡') {
    return [0.38, 0.5, 0.62].map((fy) => ({ k: 'rect' as const, x: x + cw * 0.12, y: y + h * fy - lw / 2, w: cw * 0.76, h: lw, alpha: 1 }));
  }
  if (ch === '⌕') {
    const r = cw * 0.27;
    const cx = x + cw * 0.44;
    const cy = y + h * 0.46;
    return [
      { k: 'path', d: `M${f(cx + r)} ${f(cy)}A${f(r)} ${f(r)} 0 1 1 ${f(cx - r)} ${f(cy)}A${f(r)} ${f(r)} 0 1 1 ${f(cx + r)} ${f(cy)}`, lw: stroke * 0.8 },
      { k: 'path', d: `M${f(cx + r * 0.7)} ${f(cy + r * 0.7)}L${px(0.9)} ${py(0.66)}`, lw: stroke },
    ];
  }
  const left = ch === '←';
  const [a, b] = left ? [0.85, 0.15] : [0.15, 0.85];
  const head = left ? 0.42 : 0.58;
  return [{ k: 'path', d: `M${px(a)} ${py(0.5)}L${px(b)} ${py(0.5)}M${px(head)} ${py(0.38)}L${px(b)} ${py(0.5)}L${px(head)} ${py(0.62)}`, lw: stroke * 0.85 }];
}

export function glyph(ch: string, x: number, y: number, cw: number, h: number, lw: number, snap: (v: number) => number = (v) => v): Prim[] | null {
  const arms = LINES[ch];
  if (arms) return lines(arms, x, y, cw, h, lw, snap);
  const boxes = BLOCKS[ch];
  if (boxes) {
    return boxes.map(([bx, by, bw, bh, alpha]) => {
      const x0 = snap(x + bx * cw);
      const y0 = snap(y + by * h);
      return r(x0, y0, snap(x + (bx + bw) * cw) - x0, snap(y + (by + bh) * h) - y0, alpha ?? 1);
    });
  }
  if (ROUND.has(ch)) return [rounded(ch, x, y, cw, h, lw)];
  if (SYMBOLS.has(ch)) return symbol(ch, x, y, cw, h, lw);
  const code = ch.codePointAt(0) ?? 0;
  if (code > 0x2800 && code <= 0x28ff) {
    const bits = code - 0x2800;
    const rad = Math.max(cw * 0.12, 0.6);
    return BRAILLE_DOTS.filter(([bit]) => bits & bit).map(([, col, row]) => ({
      k: 'circle' as const,
      cx: x + cw * (col ? 0.7 : 0.3),
      cy: y + h * (0.14 + row * 0.24),
      r: rad,
    }));
  }
  return null;
}
