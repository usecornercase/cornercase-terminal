import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import type { Index } from '../src/lib/plugins/catalogue';
import { languageColour } from '../src/lib/plugins/market';
import { CSP, csp, view } from '../src/lib/plugins/page';
import { fixtureIndex } from './fixtures/plugins/fixture';

const NOW = new Date('2026-10-20T12:00:00Z');
const index = fixtureIndex(NOW);
const page = view(index);
const astro = readFileSync('src/pages/plugins/index.astro', 'utf8');
const without = (keep: (repo: string) => boolean): Index => ({ ...index, repos: index.repos.filter((c) => keep(c.repo)) });

test('the page has its shelves and the browse grid in Popular order', () => {
  assert.deepEqual(page.trending.map((c) => c.repo), ['fast/risers', 'mid/pulse', 'acme/notes', 'big/engine']);
  assert.deepEqual(page.fresh.map((c) => c.repo), ['new/alpha', 'new/beta', 'new/gamma']);
  assert.deepEqual(page.cards.map((c) => c.repo).slice(0, 3), ['big/engine', 'acme/notes', 'mid/pulse']);
  assert.deepEqual(page.cards.map((c) => c.ranks.popular), page.cards.map((_, i) => i));
});

test('trending and new are hidden under three repositories', () => {
  const few = view(without((repo) => !['acme/notes', 'new/gamma'].includes(repo)));
  assert.deepEqual(few.trending.map((c) => c.repo), ['fast/risers', 'mid/pulse', 'big/engine']);
  const two = view(without((repo) => !['acme/notes', 'big/engine', 'new/gamma'].includes(repo)));
  assert.deepEqual(two.trending, []);
  assert.deepEqual(two.fresh, []);
  assert.deepEqual(view(without((repo) => repo !== 'new/gamma')).fresh, []);
});

test('a card whose only plugin copies another id never renders', () => {
  assert.ok(index.repos.every((c) => c.repo !== 'copycat/notes-fork') || index.repos.find((c) => c.repo === 'copycat/notes-fork')?.plugins.every((p) => p.duplicate));
  assert.ok(!page.cards.some((c) => c.repo === 'copycat/notes-fork'));
  assert.ok(!page.cards.some((c) => c.repo === 'bad/actor'));
});

test('the counts are visible plugins and shown repositories', () => {
  assert.equal(page.plugins, 13);
  assert.equal(page.repos, 10);
  assert.equal(page.gained, 140);
  assert.equal(page.joinedWeek, 3);
  assert.deepEqual(page.line, ['13 plugins', '10 repositories', '+140 ★ this week', '3 joined this week']);
});

test('the stars and joined parts of the stats line are left out at zero', () => {
  const quiet: Index = {
    ...index,
    repos: [{ ...index.repos.find((c) => c.repo === 'old/found')!, stars_history: undefined, joined: '2026-01-01T00:00:00Z', plugins: [index.repos.find((c) => c.repo === 'old/found')!.plugins[0]] }],
  };
  assert.deepEqual(view(quiet).line, ['1 plugin', '1 repository']);
});

test('a single-plugin card carries its install command pinned to a commit', () => {
  const card = page.cards.find((c) => c.repo === 'mid/pulse');
  assert.equal(card?.plugins.length, 1);
  assert.match(card?.plugins[0].command ?? '', /^cornercase plugin install mid\/pulse@[0-9a-f]{40}$/);
  const many = page.cards.find((c) => c.repo === 'big/engine');
  assert.equal(many?.plugins.length, 3);
  assert.match(many?.plugins[1].command ?? '', /^cornercase plugin install big\/engine\/deploy@[0-9a-f]{40}$/);
});

test('the policy lets images come from GitHub and nothing else, and the script no network', () => {
  const img = CSP.split('; ').find((part) => part.startsWith('img-src'));
  assert.equal(img, "img-src 'self' data: https://avatars.githubusercontent.com https://github.com");
  assert.ok(CSP.includes("connect-src 'none'"));
  assert.ok(CSP.includes("script-src 'self'"));
  assert.ok(!CSP.includes('unsafe-inline'));
  assert.ok(astro.includes('content={csp(import.meta.env.DEV)}'));
  assert.equal(csp(false), CSP);
  assert.ok(!/<script is:inline|style="|onerror=|onclick=/.test(astro), 'no inline script, style or handler under the policy');
  assert.ok(!/fetch\(|XMLHttpRequest|WebSocket/.test(readFileSync('src/lib/plugins/client.ts', 'utf8')));
});

test('an empty index renders the empty state with no shelves', () => {
  const empty = view({ ...index, repos: [], skipped: [] });
  assert.equal(empty.empty, true);
  assert.equal(empty.plugins, 0);
  assert.deepEqual([empty.trending, empty.fresh, empty.cards], [[], [], []]);
  assert.deepEqual(empty.line, ['0 plugins', '0 repositories']);
  assert.equal(page.empty, false);
});

test('the language dots in the stylesheet use market\'s colours', () => {
  for (const language of ['TypeScript', 'JavaScript', 'Rust', 'Python', 'Go', 'Lua', 'Shell', 'C++', 'Ruby']) {
    const rule = new RegExp(`data-lang='${language.replace('+', '\\+')}'\\]\\s*\\{\\s*background: (#[0-9a-fA-F]{6});`, 'i').exec(astro);
    assert.equal(rule?.[1].toLowerCase(), languageColour(language)?.toLowerCase(), language);
  }
});

test('the page reads the dev index only when asked, and is built by the standalone nav', () => {
  assert.ok(astro.includes("import.meta.env.DEV && process.env.PLUGINS_INDEX === 'dev'"));
  assert.ok(astro.includes('<Nav standalone />') && astro.includes('<Footer standalone />'));
  assert.ok(!astro.includes('StarlightPage'));
});

test('duplicate plugins never render, and a card left with none is dropped by the page itself', () => {
  const base = index.repos.find((c) => c.repo === 'mid/pulse')!;
  const mixed = { ...base, repo: 'x/mixed', plugins: [{ ...base.plugins[0], id: 'x.dup', duplicate: true }, { ...base.plugins[0], id: 'x.real' }] };
  const onlyDup = { ...base, repo: 'x/only-dup', plugins: [{ ...base.plugins[0], id: 'x.dup2', duplicate: true }] };
  const built = view({ ...index, repos: [mixed, onlyDup] });
  assert.deepEqual(built.cards.map((c) => [c.repo, c.plugins.length]), [['x/mixed', 1]]);
  assert.equal(built.plugins, 1);
  assert.equal(built.repos, 1);
});
