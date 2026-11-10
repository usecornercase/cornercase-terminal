import { all } from './paint';
import { damp, span } from './math';

export class Scroll {
  now = 0;
  velocity = 0;
  height = 1;
  width = 1;
  private readonly tops = new Map<string, [number, number]>();
  private last = 0;
  private readonly rate: number;

  constructor() {
    this.rate = matchMedia('(pointer: coarse)').matches ? 16 : 9;
    this.now = window.scrollY;
    this.last = this.now;
  }

  measure(): void {
    this.width = document.documentElement.clientWidth;
    this.height = window.innerHeight;
    for (const section of all('[data-section]')) {
      const box = section.getBoundingClientRect();
      const top = box.top + window.scrollY;
      this.tops.set(section.dataset.section ?? '', [top, Math.max(1, box.height - this.height)]);
    }
  }

  tick(dt: number): void {
    const target = window.scrollY;
    this.now = Math.abs(target - this.now) < 0.05 ? target : damp(this.now, target, this.rate, dt);
    const seen = (this.now - this.last) / Math.max(dt, 0.001);
    this.velocity = damp(this.velocity, seen, 12, dt);
    this.last = this.now;
  }

  at(id: string, progress: number): number {
    const place = this.tops.get(id);
    if (!place) throw new Error(`no section ${id}`);
    return place[0] + place[1] * progress;
  }

  progress(id: string): number {
    const place = this.tops.get(id);
    if (!place) return 0;
    return span(this.now, place[0], place[0] + place[1]);
  }

  raw(id: string): number {
    const place = this.tops.get(id);
    if (!place) return 0;
    return (this.now - place[0]) / place[1];
  }

  seen(id: string): boolean {
    const place = this.tops.get(id);
    if (!place) return false;
    return this.now > place[0] - this.height && this.now < place[0] + place[1] + this.height;
  }
}
