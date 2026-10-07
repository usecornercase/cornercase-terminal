export const APPS = ['cornercase', 'Warp', 'Orca', 'Emdash'];

export type Value = [number, string];

export interface Measure {
  title: string;
  what: string;
  values: Value[];
}

export interface Weight extends Measure {
  less: string;
  next: string;
}

export const slowest = (values: Value[]): number => Math.max(...values.map(([v]) => v));

export function lead(values: Value[]): { x: number; app: string } {
  const [[us]] = values;
  const [best, j] = values
    .map(([v], j): [number, number] => [v, j])
    .filter(([v, j]) => j > 0 && v > 0)
    .reduce((a, b) => (b[0] < a[0] ? b : a));
  return { x: best / us, app: APPS[j] };
}

export function times(x: number): string {
  if (x < 10) return `${Number(x.toFixed(1))}×`;
  if (x < 100) return `${Math.round(x)}×`;
  const unit = 10 ** Math.floor(Math.log10(x));
  return `${Math.round(x / unit) * unit}×`;
}

export function clock(seconds: number): string {
  return seconds.toFixed(3);
}

export function ticks(max: number): [number, string][] {
  const step = [0.1, 0.25, 0.5, 1, 2, 2.5, 5].find((s) => max / s <= 5) ?? 10;
  const out: [number, string][] = [];
  for (let i = 0; i * step <= max + 1e-9; i++) {
    const at = i * step;
    const label = at === 0 ? '0' : max < 1 ? `${Math.round(at * 1000)} ms` : `${Number(at.toFixed(2))} s`;
    out.push([at, label]);
  }
  return out;
}

export function countUp(el: HTMLElement, to: number, ms: number): void {
  const from = 1;
  const start = performance.now();
  const step = (now: number) => {
    const p = Math.min(1, (now - start) / ms);
    const eased = 1 - (1 - p) ** 4;
    el.textContent = times(p < 1 ? from + (to - from) * eased : to);
    if (p < 1) requestAnimationFrame(step);
  };
  requestAnimationFrame(step);
}
