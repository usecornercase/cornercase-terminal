import { type Numbers, type Stats, type WindowName, LABELS, counting, duration, factor, rows } from './model';

export interface Face {
  window: string;
  hero: string;
  parallel: boolean;
  work: string;
  busy: string;
  lines: [string, string][][];
  levels: number[];
  days: string;
  example: boolean;
}

export const WEEKS = 26;
export const PER_LINE = 5;

export function face(stats: Stats, window: WindowName, example = false): Face {
  const n: Numbers = stats.windows[window];
  const parallel = n[3] >= 60;
  const all = rows(n);
  const lines = Array.from({ length: Math.ceil(all.length / PER_LINE) }, (_, i) => all.slice(i * PER_LINE, (i + 1) * PER_LINE));
  return {
    window: LABELS[window],
    hero: parallel ? factor(n).replace(' in parallel', '') : 'no agent work',
    parallel,
    work: duration(n[2]),
    busy: duration(n[3]),
    lines,
    levels: Array.from({ length: WEEKS * 7 }, (_, i) => stats.levels[i] ?? 0),
    days: counting(stats.days),
    example,
  };
}

export function block(week: number, day: number): number {
  return (WEEKS - 1 - week) * 7 + 6 - day;
}
