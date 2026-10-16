import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { Card, Plugin } from '../src/lib/plugins/catalogue';
import { counts, fresh, languageColour, search, shown, sorted, timeAgo, trending, trendingScore, weeklyGain } from '../src/lib/plugins/market';
import { fixtureIndex } from './fixtures/plugins/fixture';

const NOW = new Date('2026-10-20T12:00:00Z');
const GENERATED = NOW.toISOString();
const ago = (days: number) => new Date(NOW.getTime() - days * 86_400_000).toISOString();
const day = (days: number) => ago(days).slice(0, 10);

const plugin = (id: string, extra: Partial<Plugin> = {}): Plugin => ({ path: '', id, name: id, version: '1.0.0', ...extra });
const card = (repo: string, extra: Partial<Card> = {}): Card => ({
  repo, stars: 100, pushed: ago(5), created: ago(100), commit: 'a'.repeat(40), plugins: [plugin(`${repo.replace('/', '.')}`)], ...extra,
});
const view = (cards: Card[]) => shown({ version: 1, generated: GENERATED, source: { total: 0, missing: 0, invalid: 0, duplicate: 0, blocked: 0 }, repos: cards, skipped: [] });

test('the weekly gain uses the sample dated exactly seven days before the crawl day', () => {
  const stars = 100;
  assert.equal(weeklyGain({ stars, stars_history: [[day(9), 50], [day(7), 80], [day(6), 90]] }, GENERATED), 20);
});

test('without that sample the gain uses the newest one at least seven days old', () => {
  assert.equal(weeklyGain({ stars: 100, stars_history: [[day(12), 40], [day(9), 70], [day(5), 95]] }, GENERATED), 30);
});

test('a history younger than seven days gives no gain and never trends', () => {
  assert.equal(weeklyGain({ stars: 100, stars_history: [[day(6), 90], [day(0), 100]] }, GENERATED), null);
  assert.equal(weeklyGain({ stars: 100 }, GENERATED), null);
  assert.equal(trendingScore({ stars: 100 }, null), null);
});

test('a gain of ten qualifies for trending and nine does not', () => {
  assert.equal(trendingScore({ stars: 110 }, 10), 10 / 100);
  assert.equal(trendingScore({ stars: 109 }, 9), null);
});

test('the score divides by the size before the gain, at least one', () => {
  assert.equal(trendingScore({ stars: 50 }, 50), 50);
  assert.equal(trendingScore({ stars: 60 }, 50), 5);
});

const risers = (n: number) =>
  Array.from({ length: n }, (_, i) => card(`up/r${i}`, { stars: 200 + i * 10, stars_history: [[day(7), 100]] }));

test('trending needs three qualifying repositories and shows the best six by score', () => {
  assert.deepEqual(trending(view(risers(2)), GENERATED), []);
  const six = trending(view(risers(8)), GENERATED);
  assert.equal(six.length, 6);
  assert.deepEqual(six.map((c) => c.repo), ['up/r7', 'up/r6', 'up/r5', 'up/r4', 'up/r3', 'up/r2']);
  assert.equal(trending(view(risers(3)), GENERATED).length, 3);
});

test('a repository with a gain under ten does not count towards the three', () => {
  const cards = [...risers(2), card('flat/one', { stars: 105, stars_history: [[day(7), 100]] })];
  assert.deepEqual(trending(view(cards), GENERATED), []);
});

const young = (name: string, joined: number, created: number) => card(name, { joined: ago(joined), created: ago(created) });

test('new needs joined within fourteen days and created within sixty', () => {
  const cards = [young('n/a', 14, 60), young('n/b', 15, 10), young('n/c', 1, 61), young('n/d', 2, 5), young('n/e', 3, 5)];
  assert.deepEqual(fresh(view(cards), GENERATED).map((c) => c.repo), ['n/d', 'n/e', 'n/a']);
});

test('new is hidden under three, newest joined first, and shows six', () => {
  assert.deepEqual(fresh(view([young('n/a', 1, 5), young('n/b', 2, 5)]), GENERATED), []);
  const many = Array.from({ length: 9 }, (_, i) => young(`n/m${i}`, i + 1, 5));
  const got = fresh(view(many), GENERATED);
  assert.equal(got.length, 6);
  assert.equal(got[0].repo, 'n/m0');
});

test('a card without joined falls back to its created date for new and newest', () => {
  const cards = [card('n/a', { created: ago(3) }), card('n/b', { created: ago(4) }), card('n/c', { created: ago(5) })];
  assert.equal(fresh(view(cards), GENERATED).length, 3);
  assert.deepEqual(sorted(view(cards), 'newest', GENERATED).map((c) => c.repo), ['n/a', 'n/b', 'n/c']);
});

const set = [
  card('b/two', { stars: 50, pushed: ago(1), joined: ago(30), stars_history: [[day(7), 49]] }),
  card('a/one', { stars: 50, pushed: ago(9), joined: ago(2), stars_history: [[day(7), 30]] }),
  card('c/three', { stars: 80, pushed: ago(4), joined: ago(60), stars_history: [[day(7), 70]] }),
  card('d/four', { stars: 20, pushed: ago(2), joined: ago(5) }),
];
const order = (sort: Parameters<typeof sorted>[1]) => sorted(view(set), sort, GENERATED).map((c) => c.repo);

test('popular sorts by stars, then by the last push', () => {
  assert.deepEqual(order('popular'), ['c/three', 'b/two', 'a/one', 'd/four']);
});

test('trending sorts by score, null as minus one, then gain, then stars', () => {
  assert.deepEqual(order('trending'), ['a/one', 'c/three', 'b/two', 'd/four']);
});

test('ties in popular go by the last push whatever the input order, and in trending by gain', () => {
  const tied = [card('t/old', { stars: 50, pushed: ago(9) }), card('t/new', { stars: 50, pushed: ago(1) })];
  assert.deepEqual(sorted(view(tied), 'popular', GENERATED).map((c) => c.repo), ['t/new', 't/old']);
  const small = [card('g/big', { stars: 50 }), card('g/gain', { stars: 10, stars_history: [[day(7), 5]] })];
  assert.deepEqual(sorted(view(small), 'trending', GENERATED).map((c) => c.repo), ['g/gain', 'g/big']);
});

test('recently updated, newest and name sort as herdr does', () => {
  assert.deepEqual(order('recent'), ['b/two', 'd/four', 'c/three', 'a/one']);
  assert.deepEqual(order('newest'), ['a/one', 'd/four', 'b/two', 'c/three']);
  assert.deepEqual(order('name'), ['a/one', 'b/two', 'c/three', 'd/four']);
});

const found = [
  card('acme/notes', { description: 'A Notes file', language: 'Rust', plugins: [plugin('acme.notes', { name: 'Notes', description: 'per workspace' }), plugin('acme.tasks', { name: 'Tasks' })] }),
  card('zed/clock', { description: 'tick', language: 'Go', plugins: [plugin('zed.clock', { name: 'Clock' })] }),
];
const hits = (query: string) => search(view(found), query).map((c) => c.repo);

test('search matches every word as a substring of one haystack, trimmed and lower-cased', () => {
  assert.deepEqual(hits('  NOTES  rust '), ['acme/notes']);
  assert.deepEqual(hits('notes go'), []);
  assert.deepEqual(hits('acme'), ['acme/notes']);
  assert.deepEqual(hits('tasks'), ['acme/notes']);
  assert.deepEqual(hits('workspace'), ['acme/notes']);
  assert.deepEqual(hits('zed.clock'), ['zed/clock']);
  assert.deepEqual(hits('   '), ['acme/notes', 'zed/clock']);
});

test('search does not read a duplicate plugin', () => {
  const dup = card('copy/fork', { plugins: [plugin('acme.notes', { name: 'Notes copy', duplicate: true }), plugin('copy.real', { name: 'Real' })] });
  assert.deepEqual(search(view([dup]), 'copy'), view([dup]));
  assert.deepEqual(search(view([dup]), 'notes'), []);
});

test('a card whose plugins are all duplicates is not shown or counted', () => {
  const dup = card('copy/fork', { plugins: [plugin('acme.notes', { duplicate: true })] });
  const cards = view([card('acme/notes'), dup]);
  assert.deepEqual(cards.map((c) => c.repo), ['acme/notes']);
  assert.equal(counts(cards, GENERATED).repos, 1);
});

test('counts add visible plugins, positive gains and repositories joined this week', () => {
  const cards = view([
    card('a/a', { stars: 130, stars_history: [[day(7), 100]], joined: ago(7), plugins: [plugin('a.one'), plugin('a.two'), plugin('a.dup', { duplicate: true })] }),
    card('b/b', { stars: 90, stars_history: [[day(7), 100]], joined: ago(8) }),
    card('c/c', { stars: 10, joined: ago(1) }),
  ]);
  assert.deepEqual(counts(cards, GENERATED), { plugins: 4, repos: 3, gained: 30, joined: 2 });
});

test('language colours are GitHub linguist\'s and others have none', () => {
  assert.deepEqual(
    ['TypeScript', 'JavaScript', 'Rust', 'Python', 'Go', 'Lua', 'Shell', 'C++', 'Ruby', 'Zig', undefined].map(languageColour),
    ['#3178c6', '#f1e05a', '#dea584', '#3572A5', '#00ADD8', '#000080', '#89e051', '#f34b7d', '#701516', null, null],
  );
});

test('relative time floors each unit and says just now under a minute', () => {
  const at = (seconds: number) => timeAgo(new Date(NOW.getTime() - seconds * 1000), NOW);
  assert.deepEqual(
    [0, 59, 60, 3599, 3600, 86_399, 86_400, 604_799, 604_800, 2_591_999, 2_592_000, 31_535_999, 31_536_000, 63_072_000].map(at),
    ['just now', 'just now', '1m ago', '59m ago', '1h ago', '23h ago', '1d ago', '6d ago', '1w ago', '4w ago', '1mo ago', '12mo ago', '1y ago', '2y ago'],
  );
});

test('the fixture assembles into trending, new, duplicates, invalid and blocked as designed', () => {
  const index = fixtureIndex(NOW);
  const cards = shown(index);
  assert.ok(cards.length >= 10);
  assert.ok(!cards.some((c) => c.repo === 'copycat/notes-fork'), 'the duplicate-only card is hidden');
  assert.ok(!cards.some((c) => c.repo === 'bad/actor'), 'the blocked repository is hidden');
  assert.equal(index.source.duplicate, 1);
  assert.equal(index.source.blocked, 1);
  assert.equal(cards.find((c) => c.repo === 'messy/plugins')?.invalid, 2);
  assert.ok(cards.filter((c) => c.plugins.length > 1).length >= 2);
  assert.deepEqual(trending(cards, index.generated).map((c) => c.repo), ['fast/risers', 'mid/pulse', 'acme/notes', 'big/engine']);
  assert.deepEqual(fresh(cards, index.generated).map((c) => c.repo), ['new/alpha', 'new/beta', 'new/gamma']);
  assert.deepEqual(index.skipped.map((s) => s.repo), ['empty/shell']);
});
