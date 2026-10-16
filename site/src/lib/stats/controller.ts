import { parse, EXAMPLE, type Stats, type WindowName } from './model';
import { type Face, face } from './ticket';

export class Card {
  window: WindowName = '7d';
  constructor(readonly stats: Stats, readonly example: boolean) {}
  select(window: WindowName): void { this.window = window; }
  face(): Face { return face(this.stats, this.window, this.example); }
}
export function fromFragment(fragment: string): Card | null {
  const stats = fragment ? parse(fragment) : EXAMPLE;
  return stats ? new Card(stats, !fragment) : null;
}
