import { backOut, beat, span } from './math';

export const LAYERS = ['projects', 'agents', 'issues', 'changes', 'files', 'flow', 'server', 'phone', 'cli'] as const;

export type Layer = (typeof LAYERS)[number];

export const FIRST = 0.17;
export const EACH = 0.076;
export const LAND: [number, number] = [0.88, 0.96];
export const RISE = 0.5;

export const HEIGHT: Record<Layer, number> = {
  projects: 60,
  agents: 100,
  issues: 190,
  changes: 270,
  files: 340,
  flow: 410,
  server: -150,
  phone: 120,
  cli: 480,
};

export function turnOf(progress: number): number {
  if (progress < FIRST) return -1;
  return Math.min(LAYERS.length, Math.floor((progress - FIRST) / EACH));
}

export function startOf(index: number): number {
  return FIRST + index * EACH;
}

export function lift(progress: number, index: number): number {
  const from = startOf(index);
  return backOut(span(progress, from, from + EACH * RISE)) * (1 - beat(progress, LAND[0], LAND[1]));
}
