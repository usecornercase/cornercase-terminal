import { type Card, type Index, type Plugin, dayOf } from './catalogue';

export type Shown = Omit<Card, 'plugins'> & { plugins: Plugin[] };

export type Sort = 'popular' | 'trending' | 'recent' | 'newest' | 'name';

export const SORTS: Sort[] = ['popular', 'trending', 'recent', 'newest', 'name'];

export const TREND_MIN_GAIN = 10;
export const NEW_SEEN_DAYS = 14;
export const NEW_CREATED_DAYS = 60;
export const SHELF_CARDS = 6;
export const SHELF_MIN = 3;
const DAY = 86_400_000;
const WEEK = 7;

const COLOURS: Record<string, string> = {
  TypeScript: '#3178c6',
  JavaScript: '#f1e05a',
  Rust: '#dea584',
  Python: '#3572A5',
  Go: '#00ADD8',
  Lua: '#000080',
  Shell: '#89e051',
  'C++': '#f34b7d',
  Ruby: '#701516',
};

export const languageColour = (language: string | undefined): string | null => (language ? (COLOURS[language] ?? null) : null);

export function shown(index: Index): Shown[] {
  const cards: Shown[] = [];
  for (const card of index.repos) {
    const plugins = card.plugins.filter((p) => !p.duplicate);
    if (plugins.length > 0) cards.push({ ...card, plugins });
  }
  return cards;
}

const daysBefore = (day: string, days: number): string => dayOf(new Date(Date.parse(day) - days * DAY));

export function weeklyGain(card: Pick<Card, 'stars' | 'stars_history'>, generated: string): number | null {
  const history = card.stars_history ?? [];
  const target = daysBefore(generated.slice(0, 10), WEEK);
  const exact = history.find(([day]) => day === target);
  const sample = exact ?? [...history].reverse().find(([day]) => day < target);
  return sample ? card.stars - sample[1] : null;
}

export const trendingScore = (card: Pick<Card, 'stars'>, gain: number | null): number | null =>
  gain !== null && gain >= TREND_MIN_GAIN ? gain / Math.max(card.stars - gain, 1) : null;

const scoreOf = (card: Shown, generated: string) => trendingScore(card, weeklyGain(card, generated));

const age = (when: string, generated: string): number => (Date.parse(generated) - Date.parse(when)) / DAY;

const joinedOf = (card: Shown): string => card.joined ?? card.created;

export function trending(cards: Shown[], generated: string): Shown[] {
  const scored = cards
    .map((card) => ({ card, score: scoreOf(card, generated) }))
    .filter((entry): entry is { card: Shown; score: number } => entry.score !== null)
    .sort((a, b) => b.score - a.score || b.card.stars - a.card.stars || a.card.repo.localeCompare(b.card.repo))
    .slice(0, SHELF_CARDS)
    .map((entry) => entry.card);
  return scored.length < SHELF_MIN ? [] : scored;
}

export function fresh(cards: Shown[], generated: string): Shown[] {
  const recent = cards
    .filter((card) => age(joinedOf(card), generated) <= NEW_SEEN_DAYS && age(card.created, generated) <= NEW_CREATED_DAYS)
    .sort((a, b) => joinedOf(b).localeCompare(joinedOf(a)) || a.repo.localeCompare(b.repo))
    .slice(0, SHELF_CARDS);
  return recent.length < SHELF_MIN ? [] : recent;
}

export function sorted(cards: Shown[], sort: Sort, generated: string): Shown[] {
  const gain = (card: Shown) => weeklyGain(card, generated) ?? -1;
  const score = (card: Shown) => scoreOf(card, generated) ?? -1;
  const order: Record<Sort, (a: Shown, b: Shown) => number> = {
    popular: (a, b) => b.stars - a.stars || b.pushed.localeCompare(a.pushed),
    trending: (a, b) => score(b) - score(a) || gain(b) - gain(a) || b.stars - a.stars,
    recent: (a, b) => b.pushed.localeCompare(a.pushed),
    newest: (a, b) => joinedOf(b).localeCompare(joinedOf(a)),
    name: (a, b) => a.repo.localeCompare(b.repo),
  };
  return [...cards].sort(order[sort]);
}

export function search(cards: Shown[], query: string): Shown[] {
  const words = query.trim().toLowerCase().split(/\s+/).filter(Boolean);
  if (words.length === 0) return cards;
  return cards.filter((card) => {
    const [owner = '', name = ''] = card.repo.split('/');
    const haystack = [card.repo, owner, name, card.description, card.language, ...card.plugins.flatMap((p) => [p.name, p.id, p.description])]
      .filter((part): part is string => typeof part === 'string')
      .join('\n')
      .toLowerCase();
    return words.every((word) => haystack.includes(word));
  });
}

export function counts(cards: Shown[], generated: string): { plugins: number; repos: number; gained: number; joined: number } {
  let gained = 0;
  let joined = 0;
  for (const card of cards) {
    const gain = weeklyGain(card, generated);
    if (gain !== null && gain > 0) gained += gain;
    if (age(joinedOf(card), generated) <= WEEK) joined += 1;
  }
  return { plugins: cards.reduce((n, card) => n + card.plugins.length, 0), repos: cards.length, gained, joined };
}

export function timeAgo(from: string | Date, now: Date): string {
  const seconds = Math.floor((now.getTime() - new Date(from).getTime()) / 1000);
  if (!Number.isFinite(seconds) || seconds < 60) return 'just now';
  const units: [string, number][] = [['y', 31_536_000], ['mo', 2_592_000], ['w', 604_800], ['d', 86_400], ['h', 3_600], ['m', 60]];
  for (const [label, size] of units) {
    const value = Math.floor(seconds / size);
    if (value >= 1) return `${value}${label} ago`;
  }
  return 'just now';
}
