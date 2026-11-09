import { COBALT } from './field';
import type { Colour, Field } from './field';
import { damp } from '../math';

interface Walker {
  a: number;
  b: number;
  phase: number;
  reach: number;
}

export interface Mood {
  gain: number;
  glow: number;
  emit: number;
  hue: Colour;
  low: number;
}

const WALKERS: Walker[] = [
  { a: 0.31, b: 0.23, phase: 0.4, reach: 0.46 },
  { a: 0.23, b: 0.37, phase: 1.9, reach: 0.42 },
  { a: 0.41, b: 0.29, phase: 3.1, reach: 0.48 },
  { a: 0.19, b: 0.43, phase: 4.6, reach: 0.4 },
  { a: 0.37, b: 0.17, phase: 5.5, reach: 0.44 },
];

export class Life {
  readonly mood: Mood = { gain: 1, glow: 0.6, emit: 1, hue: [...COBALT], low: 0 };
  awake = false;
  private readonly field: Field;
  private readonly now: Mood = { gain: 1, glow: 0.6, emit: 1, hue: [...COBALT], low: 0 };
  private pointerX = -1;
  private pointerY = -1;

  constructor(field: Field) {
    this.field = field;
  }

  pointer(x: number, y: number, width: number, height: number): void {
    const u = x / width;
    const v = 1 - y / height;
    if (this.pointerX >= 0) {
      const dx = u - this.pointerX;
      const dy = v - this.pointerY;
      const moved = Math.hypot(dx, dy);
      if (moved > 0.0006) {
        const power = 5200;
        const hue = this.now.hue;
        const amount = Math.min(0.1, 0.03 + moved * 0.9);
        this.field.splat(u, v, dx * power, dy * power, [hue[0] * amount, hue[1] * amount, hue[2] * amount], 0.0018);
      }
    }
    this.pointerX = u;
    this.pointerY = v;
  }

  flood(x: number, y: number, colour: Colour, power = 700): void {
    this.field.burst(x, y, power, [colour[0] * 0.045, colour[1] * 0.045, colour[2] * 0.045]);
  }

  tick(dt: number, velocity: number, height: number): void {
    const { now, mood, field } = this;
    now.gain = damp(now.gain, mood.gain, 3, dt);
    now.glow = damp(now.glow, mood.glow, 3, dt);
    now.emit = damp(now.emit, mood.emit, 2.5, dt);
    now.low = damp(now.low, mood.low, 2, dt);
    for (let i = 0; i < 3; i += 1) now.hue[i] = damp(now.hue[i] as number, mood.hue[i] as number, 3.5, dt);
    field.gain = now.gain;
    field.glow = now.glow;
    field.driftY = Math.max(-1.4, Math.min(1.4, (velocity / height) * 0.4));
  }

  walk(elapsed: number, time: number): void {
    if (!this.awake) return;
    const { now, field } = this;
    const share = Math.min(elapsed * 60, 2);
    const dose = 0.06 * now.emit * share;
    const hue = now.hue;
    const colour: Colour = [hue[0] * dose, hue[1] * dose, hue[2] * dose];
    for (const walker of WALKERS) {
      const ax = time * walker.a + walker.phase;
      const ay = time * walker.b + walker.phase * 2.1;
      const x = 0.5 + walker.reach * Math.sin(ax);
      const y = 0.5 - now.low * 0.22 + walker.reach * Math.sin(ay) * (1 - now.low * 0.45);
      const vx = walker.reach * walker.a * Math.cos(ax) * 1500 + Math.sin(time * 1.9 + walker.phase) * 330;
      const vy = walker.reach * walker.b * Math.cos(ay) * 1500 + Math.cos(time * 2.3 + walker.phase) * 330;
      field.splat(x, y, vx * share, vy * share, colour, 0.0026);
    }
  }
}
