import { policy } from '../csp';
import { type Index, installCommand } from './catalogue';
import { SORTS, type Shown, type Sort, counts, fresh, haystack, languageColour, shown, sorted, trending, weeklyGain } from './market';

const IMAGES = "'self' data: https://avatars.githubusercontent.com https://github.com";

export const CSP = policy(IMAGES);

export const csp = (dev: boolean): string => policy(IMAGES, dev);

export const PAGE_SIZE = 24;

export type Entry = { name: string; version: string; command: string };

export type CardView = {
  repo: string;
  owner: string;
  name: string;
  description?: string;
  stars: number;
  language?: string;
  coloured: boolean;
  pushed: string;
  joined: string;
  plugins: Entry[];
  haystack: string;
  ranks: Record<Sort, number>;
  gain: number | null;
  before: number | null;
};

export type View = {
  generated: string;
  plugins: number;
  repos: number;
  gained: number;
  joinedWeek: number;
  trending: CardView[];
  fresh: CardView[];
  cards: CardView[];
  empty: boolean;
  line: string[];
};

const number = (n: number): string => n.toLocaleString('en-US');

export const noun = (n: number, one: string, many: string): string => `${number(n)} ${n === 1 ? one : many}`;

export const headline = (plugins: number, repos: number): string =>
  `${noun(plugins, 'plugin', 'plugins')} · ${noun(repos, 'repository', 'repositories')} · refreshed every hour`;

export function view(index: Index): View {
  const cards = shown(index);
  const generated = index.generated;
  const ranks = Object.fromEntries(SORTS.map((sort) => [sort, new Map(sorted(cards, sort, generated).map((card, i) => [card.repo, i]))])) as Record<Sort, Map<string, number>>;
  const make = (card: Shown): CardView => {
    const [owner = '', name = ''] = card.repo.split('/');
    const gain = weeklyGain(card, generated);
    return {
      repo: card.repo,
      owner,
      name,
      description: card.description,
      stars: card.stars,
      language: card.language,
      coloured: languageColour(card.language) !== null,
      pushed: card.pushed,
      joined: card.joined ?? card.created,
      plugins: card.plugins.map((p) => ({ name: p.name, version: p.version, command: installCommand(card, p) })),
      haystack: haystack(card),
      ranks: Object.fromEntries(SORTS.map((sort) => [sort, ranks[sort].get(card.repo) ?? 0])) as Record<Sort, number>,
      gain,
      before: gain === null ? null : card.stars - gain,
    };
  };
  const all = new Map(cards.map((card) => [card.repo, make(card)]));
  const pick = (list: Shown[]) => list.map((card) => all.get(card.repo) as CardView);
  const total = counts(cards, generated);
  const line = [noun(total.plugins, 'plugin', 'plugins'), noun(total.repos, 'repository', 'repositories')];
  if (total.gained > 0) line.push(`+${number(total.gained)} ★ this week`);
  if (total.joined > 0) line.push(`${number(total.joined)} joined this week`);
  return {
    generated,
    plugins: total.plugins,
    repos: total.repos,
    gained: total.gained,
    joinedWeek: total.joined,
    trending: pick(trending(cards, generated)),
    fresh: pick(fresh(cards, generated)),
    cards: pick(sorted(cards, 'popular', generated)),
    empty: cards.length === 0,
    line,
  };
}
