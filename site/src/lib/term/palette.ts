export const DEFAULT = -1;

export const rgb = (r: number, g: number, b: number): number => 0x1000000 | (r << 16) | (g << 8) | b;

export interface Theme {
  foreground: string;
  background: string;
  ansi: string[];
  overrides: Record<number, string>;
}

export const theme: Theme = {
  foreground: '#dcdaea',
  background: '#0e0d14',
  ansi: [
    '#1c1a27',
    '#ff6b8b',
    '#58e6a0',
    '#f5d76e',
    '#6aa8ff',
    '#c38bff',
    '#45d9e8',
    '#b8b6c9',
    '#77748f',
    '#ff8aa3',
    '#7ef0b7',
    '#ffe28f',
    '#8dbdff',
    '#d4a8ff',
    '#7ae6f0',
    '#f6f4ff',
  ],
  overrides: { 99: '#875fff', 235: '#191723', 236: '#23212f', 239: '#454357', 254: '#e6e4ee' },
};

const hex = (n: number): string => n.toString(16).padStart(2, '0');

const cube = [0, 95, 135, 175, 215, 255];

function xterm(index: number): string {
  if (index < 232) {
    const i = index - 16;
    return `#${hex(cube[Math.floor(i / 36)])}${hex(cube[Math.floor(i / 6) % 6])}${hex(cube[i % 6])}`;
  }
  const level = 8 + (index - 232) * 10;
  return `#${hex(level)}${hex(level)}${hex(level)}`;
}

const cache = new Map<number, string>();

export function css(color: number, fallback: string, t: Theme = theme): string {
  if (color === DEFAULT) return fallback;
  const hit = t === theme ? cache.get(color) : undefined;
  if (hit) return hit;
  let out: string;
  if (color >= 0x1000000) out = `#${(color & 0xffffff).toString(16).padStart(6, '0')}`;
  else if (t.overrides[color]) out = t.overrides[color];
  else if (color < 16) out = t.ansi[color];
  else out = xterm(color);
  if (t === theme) cache.set(color, out);
  return out;
}

export const fg = (color: number, t: Theme = theme): string => css(color, t.foreground, t);
export const bg = (color: number, t: Theme = theme): string => css(color, t.background, t);

export const paper: Theme = {
  foreground: '#2a2833',
  background: '#fbfaf6',
  ansi: [
    '#2a2833',
    '#d6336c',
    '#2b8a3e',
    '#b35c00',
    '#1c64c4',
    '#8e3bb0',
    '#0b7f95',
    '#55536a',
    '#9a98aa',
    '#e64980',
    '#37b24d',
    '#e67700',
    '#228be6',
    '#ae3ec9',
    '#1098ad',
    '#15141b',
  ],
  overrides: {},
};

export const ember: Theme = {
  foreground: '#efe2cf',
  background: '#16110d',
  ansi: [
    '#2a211a',
    '#f2594b',
    '#b8bb26',
    '#fabd2f',
    '#83a598',
    '#d3869b',
    '#8ec07c',
    '#d5c4a1',
    '#7c6f64',
    '#fb4934',
    '#c7ca3a',
    '#ffd25a',
    '#9cc2b2',
    '#e2a0b4',
    '#a6d48f',
    '#fbf1c7',
  ],
  overrides: {},
};

export const themes = { midnight: theme, paper, ember };
