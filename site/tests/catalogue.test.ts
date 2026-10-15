import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { MAX_BYTES, installCommand, sourceUrl, validate } from '../src/lib/plugins/catalogue';
import { type Found, type Repo, assemble, crawl, manifestPaths, pluginFrom, repoFrom, scan } from '../src/lib/plugins/crawl';

const C1 = '0123456789abcdef0123456789abcdef01234567';
const C2 = '89abcdef0123456789abcdef0123456789abcdef';
const MANIFEST = `id = "acme.notes"
name = "Notes"
version = "1.0.0"
description = "A notes file per workspace"
platforms = ["macos", "linux"]
permissions = ["read", "send"]

[[actions]]
id = "open"
title = "open notes"
contexts = ["pane"]
command = ["./notes"]
`;
const withId = (id: string) => MANIFEST.replace('acme.notes', id);

const repo = (name: string, stars = 0, created = '2026-01-01T00:00:00Z'): Repo => ({
  repo: name, branch: 'main', stars, created, pushed: '2026-10-01T00:00:00Z', fork: false, archived: false, language: 'Rust', description: 'A repo',
});
const found = (name: string, manifests: { path: string; text: string | null }[], stars = 0, created?: string): Found => ({
  repo: repo(name, stars, created), commit: C1, scan: scan(manifests),
});
const empty = { repos: [], ids: [] };
const when = new Date('2026-10-15T12:00:00Z');

test('a manifest becomes a plugin at its folder', () => {
  assert.deepEqual(pluginFrom('plugins/notes', MANIFEST), {
    path: 'plugins/notes', id: 'acme.notes', name: 'Notes', version: '1.0.0', description: 'A notes file per workspace',
    platforms: ['macos', 'linux'], permissions: ['read', 'send'],
  });
});

test('bad manifests are counted as invalid', () => {
  const bad = [
    'id = ',
    withId('Acme Notes'),
    MANIFEST.replace('"Notes"', '"No\\u001btes"'),
    MANIFEST.replace('per workspace', 'per \\u202eworkspace'),
    MANIFEST.replace('"linux"]', '"windows"]'),
    `api = 2\n${MANIFEST}`,
    MANIFEST.replace('version = "1.0.0"\n', ''),
  ];
  assert.deepEqual(scan(bad.map((text) => ({ path: '', text }))), { plugins: [], invalid: bad.length });
});

test('manifests are found at the root and in subfolders, nearest first', () => {
  const tree = { tree: [
    { type: 'blob', path: 'plugins/b/cornercase-plugin.toml' },
    { type: 'blob', path: 'cornercase-plugin.toml' },
    { type: 'blob', path: 'plugins/a/cornercase-plugin.toml' },
    { type: 'tree', path: 'cornercase-plugin.toml' },
    { type: 'blob', path: 'docs/not-cornercase-plugin.toml' },
    { type: 'blob', path: '-x/cornercase-plugin.toml' },
  ] };
  assert.deepEqual(manifestPaths(tree), ['', 'plugins/a', 'plugins/b']);
});

test('the index makes one card per repository and marks a reused id as a duplicate of the older one', () => {
  const index = assemble(
    [
      found('acme/tools', [{ path: '', text: MANIFEST }, { path: 'tasks', text: withId('acme.tasks') }, { path: 'broken', text: 'id =' }], 3, '2026-02-01T00:00:00Z'),
      found('copy/notes', [{ path: '', text: MANIFEST }], 90, '2026-03-01T00:00:00Z'),
      found('acme/empty', []),
      found('bad/actor', [{ path: '', text: withId('bad.actor') }], 500),
      found('acme/clock', [{ path: '', text: withId('acme.clock') }], 10),
    ],
    { repos: ['Bad/Actor'], ids: [] },
    when,
  );
  assert.deepEqual(index.source, { total: 5, missing: 1, invalid: 1, duplicate: 1, blocked: 1 });
  assert.deepEqual(index.repos.map((c) => [c.repo, c.plugins.map((p) => p.path || '.')]), [['acme/clock', ['.']], ['acme/tools', ['.', 'tasks']]]);
  assert.equal(index.repos[1].invalid, 1);
  assert.deepEqual(index.skipped.map((s) => s.repo), ['acme/empty']);
  assert.ok(validate(JSON.stringify(index)));
});

test('a blocked id is left out wherever it comes from', () => {
  const index = assemble([found('acme/notes', [{ path: '', text: MANIFEST }])], { repos: [], ids: ['acme.notes'] }, when);
  assert.equal(index.repos.length, 0);
  assert.equal(index.source.blocked, 1);
});

const answer = (status: number, body: unknown) => ({ ok: status === 200, status, json: async () => body, text: async () => String(body) });

function github(asked: string[]) {
  return async (url: string) => {
    asked.push(url);
    if (url.includes('/search/repositories')) {
      return answer(200, { items: [
        { full_name: 'acme/notes', default_branch: 'trunk', stargazers_count: 4, created_at: '2026-01-01T00:00:00Z', pushed_at: '2026-10-01T00:00:00Z', language: 'Go', description: 'Notes\u001b[31m' },
        { full_name: 'some/fork', default_branch: 'main', fork: true },
        { full_name: 'https://evil', default_branch: 'main' },
      ] });
    }
    if (url.endsWith('/repos/acme/notes/commits/trunk')) return answer(200, { sha: C2 });
    if (url.includes(`/repos/acme/notes/git/trees/${C2}`)) return answer(200, { tree: [{ type: 'blob', path: 'plugin/cornercase-plugin.toml' }] });
    if (url.endsWith(`/acme/notes/${C2}/plugin/cornercase-plugin.toml`)) return answer(200, MANIFEST);
    return answer(404, {});
  };
}

test('the crawl searches the topic, skips forks and reads every manifest at the head commit', async () => {
  const asked: string[] = [];
  const crawled = await crawl(github(asked), 'token', null);
  assert.deepEqual(crawled.map((f) => [f.repo.repo, f.commit, f.scan.plugins.map((p) => p.path)]), [['acme/notes', C2, ['plugin']]]);
  assert.equal(crawled[0].repo.description, 'Notes [31m');
  assert.ok(asked[0].includes('topic%3Acornercase-plugin'));
  assert.equal(asked.length, 4);
  assert.equal(repoFrom({ full_name: 'a/b/c' }), null);
});

test('a repository whose head did not change is not read again', async () => {
  const first = assemble(await crawl(github([]), undefined, null), empty, when);
  const asked: string[] = [];
  const again = await crawl(github(asked), undefined, first);
  assert.equal(asked.length, 2);
  assert.deepEqual(again[0].scan.plugins.map((p) => p.id), ['acme.notes']);
});

test('the published index is valid, and bad indexes are refused', () => {
  assert.ok(validate(readFileSync('plugins/index.json', 'utf8')));
  const good = assemble([found('acme/notes', [{ path: 'sub', text: MANIFEST }])], empty, when);
  const card = (change: Record<string, unknown>) => JSON.stringify({ ...good, repos: [{ ...good.repos[0], ...change }] });
  const plugin = (change: Record<string, unknown>) => card({ plugins: [{ ...good.repos[0].plugins[0], ...change }] });
  const bad: [string, string][] = [
    ['newer version', JSON.stringify({ ...good, version: 2 })],
    ['unknown field', card({ homepage: 'https://example.com' })],
    ['short commit', card({ commit: 'abc' })],
    ['url for a repo', card({ repo: 'https://github.com/a/b' })],
    ['negative stars', card({ stars: -1 })],
    ['no plugins', card({ plugins: [] })],
    ['climbing path', plugin({ path: '../x' })],
    ['escape in a name', plugin({ name: 'a\u001bb' })],
    ['oversize', ' '.repeat(MAX_BYTES + 1)],
  ];
  for (const [what, source] of bad) assert.throws(() => validate(source), Error, what);
});

test('the page gives each plugin its pinned install command and source folder', () => {
  const index = assemble([found('acme/notes', [{ path: '', text: MANIFEST }, { path: 'plugins/tasks', text: withId('acme.tasks') }])], empty, when);
  const [card] = index.repos;
  assert.equal(installCommand(card, card.plugins[0]), `cornercase plugin install acme/notes@${C1}`);
  assert.equal(installCommand(card, card.plugins[1]), `cornercase plugin install acme/notes/plugins/tasks@${C1}`);
  assert.equal(sourceUrl(card, card.plugins[1]), `https://github.com/acme/notes/tree/${C1}/plugins/tasks`);
});
