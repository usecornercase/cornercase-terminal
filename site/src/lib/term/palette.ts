export const DEFAULT = -1;

export const rgb = (r: number, g: number, b: number): number => 0x1000000 | (r << 16) | (g << 8) | b;

export interface Theme {
  foreground: string;
  background: string;
  ansi: string[];
  overrides: Record<number, string>;
}

export const midnight: Theme = {
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

const hexByte = (n: number): string => n.toString(16).padStart(2, '0');

function mix(a: string, b: string, t: number): string {
  const part = (hex: string, i: number) => Number.parseInt(hex.slice(1 + i * 2, 3 + i * 2), 16);
  return `#${[0, 1, 2].map((i) => hexByte(Math.round(part(a, i) + (part(b, i) - part(a, i)) * t))).join('')}`;
}

const make = (background: string, foreground: string, ansi: string[], purple: string): Theme => ({
  foreground,
  background,
  ansi,
  overrides: { 99: purple, 235: mix(background, foreground, 0.05), 236: mix(background, foreground, 0.09), 239: mix(background, foreground, 0.22), 254: mix(background, foreground, 0.85) },
});

export const tokyo = make(
  '#1a1b26',
  '#c0caf5',
  ['#15161e', '#f7768e', '#9ece6a', '#e0af68', '#7aa2f7', '#bb9af7', '#7dcfff', '#a9b1d6', '#565f89', '#f7768e', '#9ece6a', '#e0af68', '#7aa2f7', '#bb9af7', '#7dcfff', '#c0caf5'],
  '#bb9af7',
);

export const catppuccin = make(
  '#1e1e2e',
  '#cdd6f4',
  ['#45475a', '#f38ba8', '#a6e3a1', '#f9e2af', '#89b4fa', '#f5c2e7', '#94e2d5', '#bac2de', '#6c7086', '#f38ba8', '#a6e3a1', '#f9e2af', '#89b4fa', '#f5c2e7', '#94e2d5', '#a6adc8'],
  '#cba6f7',
);

export const dracula = make(
  '#282a36',
  '#f8f8f2',
  ['#21222c', '#ff5555', '#50fa7b', '#f1fa8c', '#bd93f9', '#ff79c6', '#8be9fd', '#f8f8f2', '#6272a4', '#ff6e6e', '#69ff94', '#ffffa5', '#d6acff', '#ff92df', '#a4ffff', '#ffffff'],
  '#bd93f9',
);

export const onedark = make(
  '#282c34',
  '#abb2bf',
  ['#3f4451', '#e06c75', '#98c379', '#e5c07b', '#61afef', '#c678dd', '#56b6c2', '#d7dae0', '#5c6370', '#e06c75', '#98c379', '#e5c07b', '#61afef', '#c678dd', '#56b6c2', '#ffffff'],
  '#c678dd',
);

export const github = make(
  '#0d1117',
  '#e6edf3',
  ['#484f58', '#ff7b72', '#3fb950', '#d29922', '#58a6ff', '#bc8cff', '#39c5cf', '#b1bac4', '#6e7681', '#ffa198', '#56d364', '#e3b341', '#79c0ff', '#d2a8ff', '#56d4dd', '#ffffff'],
  '#bc8cff',
);

export const gruvbox = make(
  '#282828',
  '#ebdbb2',
  ['#3c3836', '#cc241d', '#98971a', '#d79921', '#458588', '#b16286', '#689d6a', '#a89984', '#928374', '#fb4934', '#b8bb26', '#fabd2f', '#83a598', '#d3869b', '#8ec07c', '#ebdbb2'],
  '#d3869b',
);

export const theme: Theme = tokyo;

const cube = [0, 95, 135, 175, 215, 255];

function xterm(index: number): string {
  if (index < 232) {
    const i = index - 16;
    return `#${hexByte(cube[Math.floor(i / 36)])}${hexByte(cube[Math.floor(i / 6) % 6])}${hexByte(cube[i % 6])}`;
  }
  const level = 8 + (index - 232) * 10;
  return `#${hexByte(level)}${hexByte(level)}${hexByte(level)}`;
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

export const themes = { tokyo, catppuccin, dracula, onedark, github, gruvbox, midnight, paper, ember };

export const PICKER: [keyof typeof themes, string][] = [
  ['tokyo', 'Tokyo Night'],
  ['catppuccin', 'Catppuccin Mocha'],
  ['dracula', 'Dracula'],
  ['onedark', 'One Dark Pro'],
  ['github', 'GitHub Dark'],
  ['gruvbox', 'Gruvbox Dark'],
  ['midnight', 'Midnight'],
];
