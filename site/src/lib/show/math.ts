export function clamp(value: number, low = 0, high = 1): number {
  return Math.min(high, Math.max(low, value));
}

export function mix(from: number, to: number, amount: number): number {
  return from + (to - from) * amount;
}

export function span(value: number, from: number, to: number): number {
  return to === from ? (value >= to ? 1 : 0) : clamp((value - from) / (to - from));
}

export function smooth(value: number): number {
  const t = clamp(value);
  return t * t * (3 - 2 * t);
}

export function easeOut(value: number): number {
  return 1 - (1 - clamp(value)) ** 3;
}

export function easeExpo(value: number): number {
  const t = clamp(value);
  return t >= 1 ? 1 : 1 - 2 ** (-10 * t);
}

export function easeInOut(value: number): number {
  const t = clamp(value);
  return t < 0.5 ? 4 * t * t * t : 1 - (-2 * t + 2) ** 3 / 2;
}

export function backOut(value: number): number {
  if (value <= 0) return 0;
  if (value >= 1) return 1;
  const t = value - 1;
  return 1 + 2.2 * t * t * t + 1.2 * t * t;
}

export function damp(current: number, target: number, rate: number, dt: number): number {
  return mix(current, target, 1 - Math.exp(-rate * dt));
}

export function beat(value: number, from: number, to: number): number {
  return easeInOut(span(value, from, to));
}

export function veil(value: number, from: number, to: number, leave: number, gone: number): number {
  return beat(value, from, to) * (1 - beat(value, leave, gone));
}

export function crossed(before: number, now: number, mark: number): boolean {
  return before < mark && now >= mark;
}

export function seeded(seed: number): () => number {
  let state = seed >>> 0;
  return () => {
    state = (state + 0x6d2b79f5) >>> 0;
    let t = state;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}
