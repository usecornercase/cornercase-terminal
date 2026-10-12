import { cardGrid } from './card';
import { parse, EXAMPLE, type Stats, type WindowName } from './model';

export class Card {
  window: WindowName = '7d';
  constructor(readonly stats: Stats, readonly example: boolean) {}
  select(window: WindowName): void { this.window = window; }
  grid() { return cardGrid(this.stats, this.window); }
}
export function fromFragment(fragment: string): Card | null {
  const stats = fragment ? parse(fragment) : EXAMPLE;
  return stats ? new Card(stats, !fragment) : null;
}
