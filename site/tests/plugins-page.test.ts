import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import type { Index } from '../src/lib/plugins/catalogue';
import { languageColour } from '../src/lib/plugins/market';
import { CSP, csp, headline, view } from '../src/lib/plugins/page';
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

const footer = readFileSync('src/components/landing/Footer.astro', 'utf8');

test('the standalone footer leaves out the landing\'s call to action, and the landing keeps it', () => {
  assert.match(footer, /!standalone && \(\s*<div class="cta reveal">/);
  assert.ok(footer.includes('class:list={[\'footer\', { standalone }]}'));
  assert.ok(astro.includes('<Footer standalone />'));
  assert.ok(readFileSync('src/components/landing/Home.astro', 'utf8').includes('<Footer />'));
});

test('the sections have command lines above their titles, and none of herdr\'s titles or subtitles remain', () => {
  const sections: [string, string, string][] = [
    ['trending-title', '$ sort --stars --since 7d', 'Rising this week'],
    ['new-title', '$ ls --newest', 'Just landed'],
    ['browse-title', '$ ls --all', 'Every plugin'],
  ];
  for (const [id, command, title] of sections) {
    const at = astro.indexOf(`id="${id}"`);
    assert.ok(at > 0, id);
    assert.ok(astro.slice(Math.max(0, at - 160), at).includes(`<p class="eyebrow cmd-line" aria-hidden="true">${command}</p>`), command);
    assert.ok(astro.slice(at, at + 80).includes(`>${title}</h2>`), title);
    assert.ok(astro.includes(`aria-labelledby="${id}"`));
  }
  for (const old of ['Trending this week', 'New to cornercase', 'Browse everything', 'fastest-growing', 'fresh projects', 'the full index', 'Want your plugin listed']) {
    assert.ok(!astro.includes(old), old);
  }
  assert.ok(astro.includes('data-see="trending"') && astro.includes('data-see="newest"'));
});

test('the publish block is a terminal line built from the topic and manifest constants', () => {
  assert.ok(astro.includes('const topicCommand = `gh repo edit --add-topic ${TOPIC}`;'));
  assert.ok(astro.includes('<code>{topicCommand}</code>'));
  assert.ok(astro.includes('data-plugin-copy={topicCommand}'));
  assert.ok(astro.includes('<code>{MANIFEST}</code>'));
  assert.ok(astro.includes('#publish-a-plugin'));
  assert.ok(!astro.includes('cornercase-plugin'), 'the topic and manifest names come from the constants');
  assert.ok(!astro.includes('class="kicker"'));
});

test('the publish block has a visible title, a paragraph, its own guide link and a plain command box', () => {
  const block = astro.slice(astro.indexOf('<section class="publish"'), astro.indexOf('</section>', astro.indexOf('<section class="publish"')));
  const order = [
    '<p class="eyebrow cmd-line" aria-hidden="true">$ {topicCommand}</p>',
    "<h2 id=\"publish-title\">{page.empty ? 'Publish the first one' : 'Publish yours'}</h2>",
    'Add a <code>{MANIFEST}</code> to a public repository, then tag it with the topic.',
    'with no submission and no review.',
    '<p class="guide-link"><a href={`${guide}#publish-a-plugin`}>How publishing works →</a></p>',
    '<div class="term">',
  ].map((part) => block.indexOf(part));
  assert.ok(order.every((at) => at >= 0), String(order));
  assert.deepEqual(order, [...order].sort((a, b) => a - b));
  assert.ok(!block.includes('sr-only'));
  assert.ok(!astro.includes('.plugins-page .publish code'), 'the light inline-code field must not reach the command box');
  assert.ok(astro.includes('.plugins-page .publish .after code'));
  const rule = astro.slice(astro.indexOf('.plugins-page .term code {'), astro.indexOf('}', astro.indexOf('.plugins-page .term code {')));
  assert.ok(/padding: 0;/.test(rule) && /border: 0;/.test(rule) && /background: none;/.test(rule), 'the command sits on the box, not in a field');
});

test('the header is a command, a title, its output and one sentence, in one column', () => {
  const header = astro.slice(astro.indexOf('<header class="intro">'), astro.indexOf('</header>'));
  assert.ok(header.includes('<p class="eyebrow cmd-line" aria-hidden="true">$ cornercase plugin list</p>'));
  assert.ok(header.includes('<h1>Extend your terminal.</h1>'));
  assert.ok(header.includes('<p class="out">{headline(page.plugins, page.repos)}</p>'));
  assert.ok(header.includes("Small tools other people built on cornercase's commands. Install one with <code>cornercase plugin install</code>."));
  assert.ok(header.includes("Listings aren't reviewed, so install at your own discretion.") && header.includes('Trust guidance →'));
  for (const old of ['the marketplace', 'Built by the community', 'class="side"', 'data-hero-count', 'plugins across', 'discovered automatically']) {
    assert.ok(!astro.includes(old), old);
  }
  assert.ok(!header.includes('Publish yours'), 'the header has no publish link');
  assert.ok(!/\.intro \{\s*display: grid/.test(astro), 'the header has no second column');
});

test('the header count is the command\'s output with singular and plural forms', () => {
  assert.equal(headline(page.plugins, page.repos), '13 plugins · 10 repositories · refreshed every hour');
  assert.equal(headline(1, 1), '1 plugin · 1 repository · refreshed every hour');
  assert.equal(headline(0, 0), 'no plugins yet · be the first');
  assert.equal(headline(1200, 2), '1,200 plugins · 2 repositories · refreshed every hour');
});

test('the empty state drops the browse section and shows the publish block once', () => {
  assert.ok(astro.includes('!page.empty && ('));
  assert.equal(astro.split('<section class="publish"').length - 1, 1);
  assert.equal(astro.split('data-plugin-copy={topicCommand}').length - 1, 1);
  assert.ok(!astro.includes('data-empty'));
  assert.ok(astro.includes("{page.empty ? 'Your plugin' : 'It'} shows up"));
});
