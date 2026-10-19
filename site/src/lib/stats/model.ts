export type WindowName = '24h' | '7d' | 'all';
export type Numbers = [agents: number, prompts: number, agent: number, busy: number, most: number, out: number, turns?: number, waited?: number, input?: number, cache?: number, merged?: number];
export interface Stats { windows: Record<WindowName, Numbers>; days: number; levels: number[] }
export const LABELS: Record<WindowName, string> = { '24h': 'last 24 h', '7d': 'last 7 days', all: 'in total' };
const numbers = (n: number) => `(\\d+(?:\\.\\d+){${n - 1}})`;
const fragment = (version: number, n: number) => new RegExp(`^#v${version}&24h=${numbers(n)}&7d=${numbers(n)}&all=${numbers(n)}&days=(\\d+)&map=([A-Za-z0-9_-]*)$`);
const FRAGMENTS = [fragment(1, 6), fragment(2, 10), fragment(3, 11)];
export const ERROR = 'this link is not a cornercase stats link';

export function parse(hash: string): Stats | null {
  if (hash.length > 1024) return null;
  const match = FRAGMENTS.map((pattern) => pattern.exec(hash)).find((found) => found);
  if (!match || match[0] !== hash) return null;
  const arrays = [match[1], match[2], match[3]].map((text) => text.split('.').map(Number));
  const days = Number(match[4]);
  if ([days, ...arrays.flat()].some((n) => !Number.isSafeInteger(n) || n < 0)) return null;
  const blocks = Math.min(days + 1, 182);
  const bytes = Math.ceil(blocks * 3 / 8);
  const map = match[5];
  if (map.length !== Math.ceil(bytes * 4 / 3)) return null;
  let binary: string;
  try { binary = atob(map.replace(/-/g, '+').replace(/_/g, '/')); } catch { return null; }
  if (binary.length !== bytes || btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '') !== map) return null;
  const levels = Array.from({ length: blocks }, (_, i) => {
    let level = 0;
    for (let bit = 0; bit < 3; bit++) { const at = i * 3 + bit; level = level * 2 + ((binary.charCodeAt(Math.floor(at / 8)) >> (7 - at % 8)) & 1); }
    return level;
  });
  if (levels.some((level) => level > 4)) return null;
  const spare = bytes * 8 - blocks * 3;
  if (spare > 0 && (binary.charCodeAt(bytes - 1) & ((1 << spare) - 1)) !== 0) return null;
  return { windows: { '24h': arrays[0] as Numbers, '7d': arrays[1] as Numbers, all: arrays[2] as Numbers }, days, levels };
}

export function duration(seconds: number): string {
  const minutes = Math.floor(seconds / 60); const hours = Math.floor(minutes / 60); const rest = minutes % 60;
  return hours === 0 ? `${rest}m` : rest === 0 ? `${hours}h` : `${hours}h ${String(rest).padStart(2, '0')}m`;
}
export function tokens(number: number): string {
  for (const [scale, suffix] of [[1e9, 'B'], [1e6, 'M'], [1e3, 'k']] as const) {
    if (number >= scale) { const n = number / scale; return `${n.toFixed(n < 10 ? 1 : 0)}${suffix}`; }
  }
  return String(number);
}
export function count(n: number): string { return n < 10_000 ? String(n) : n < 1e6 ? `${(n / 1e3).toFixed(1)}k` : tokens(n); }
export function factor(numbers: Numbers): string { return numbers[3] < 60 ? 'no agent work' : `${(numbers[2] / numbers[3]).toFixed(1)}× in parallel`; }
export const ramp = [239, 24, 26, 33, 75];
export const ROWS: [string, number, (n: number) => string][] = [
  ['agents started', 0, count], ['prompts', 1, count], ['turns', 6, count], ['agent time', 2, duration], ['most at once', 4, count],
  ['waited for you', 7, duration], ['tokens out', 5, tokens], ['tokens in', 8, tokens], ['tokens from cache', 9, tokens], ['merged PRs', 10, count],
];
export function rows(n: Numbers): [string, string][] {
  return ROWS.flatMap(([label, at, format]): [string, string][] => { const value = n[at]; return value === undefined ? [] : [[label, format(value)]]; });
}
export function counting(days: number): string { return `counting for ${days} day${days === 1 ? '' : 's'}`; }
export const EXAMPLE: Stats = {
  windows: {
    '24h': [4, 38, 22320, 6600, 5, 410000, 35, 1320, 1200000, 38000000, 2],
    '7d': [31, 402, 147780, 43800, 7, 3100000, 377, 13200, 11000000, 310000000, 9],
    all: [212, 3118, 1116000, 300000, 9, 24000000, 2904, 100800, 90000000, 2400000000, 61],
  },
  days: 110,
  levels: Array.from({ length: 111 }, (_, i) => i % 11 === 0 ? 0 : (i * 7 + Math.floor(i / 3)) % 4 + 1),
};
export function postText(stats: Stats, window: WindowName): string {
  const n = stats.windows[window];
  return `${factor(n)} ${window === '7d' ? 'this week' : LABELS[window]}: ${duration(n[2])} of agent work in ${duration(n[3])}, with cornercase`;
}

export function shareLink(stats: Stats): string {
  const levels = stats.levels.slice(0, Math.min(stats.days + 1, 182));
  const bytes = new Uint8Array(Math.ceil(levels.length * 3 / 8));
  levels.forEach((level, i) => { for (let bit = 0; bit < 3; bit++) if (level & (1 << (2 - bit))) { const at = i * 3 + bit; bytes[Math.floor(at / 8)] |= 1 << (7 - at % 8); } });
  const map = btoa(String.fromCharCode(...bytes)).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
  const length = stats.windows['24h'].length;
  const version = length === 11 ? 3 : length === 10 ? 2 : 1;
  return `https://usecornercase.dev/stats/#v${version}&24h=${stats.windows['24h'].join('.')}&7d=${stats.windows['7d'].join('.')}&all=${stats.windows.all.join('.')}&days=${stats.days}&map=${map}`;
}
