import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { MAX_BYTES, installCommand, sourceUrl, validate } from '../src/lib/plugins/catalogue';
import { type Found, assemble, crawl, entryFrom, repoFrom } from '../src/lib/plugins/crawl';

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

const repo = (name: string, stars = 0, created = '2026-01-01T00:00:00Z') => ({ repo: name, branch: 'main', stars, created, fork: false, archived: false });
const found = (name: string, manifest: string | null, stars = 0, created?: string): Found => ({ repo: repo(name, stars, created), commit: C1, manifest });
const empty = { repos: [], ids: [] };
const when = new Date('2026-10-15T12:00:00Z');

test('a manifest becomes an entry pinned to the commit the crawl saw', () => {
  assert.deepEqual(entryFrom(found('acme/notes', MANIFEST, 7)), {
    id: 'acme.notes',
    name: 'Notes',
    version: '1.0.0',
    description: 'A notes file per workspace',
    platforms: ['macos', 'linux'],
    permissions: ['read', 'send'],
    repo: 'acme/notes',
    commit: C1,
    stars: 7,
  });
});

test('bad manifests are refused', () => {
  const bad: [string, string][] = [
    ['not toml', 'id = '],
    ['bad id', MANIFEST.replace('acme.notes', 'Acme Notes')],
    ['escape in the name', MANIFEST.replace('"Notes"', '"No\\u001btes"')],
    ['bidi in the description', MANIFEST.replace('per workspace', 'per \\u202eworkspace')],
    ['unknown platform', MANIFEST.replace('"linux"]', '"windows"]')],
    ['newer api', `api = 2\n${MANIFEST}`],
    ['no version', MANIFEST.replace('version = "1.0.0"\n', '')],
  ];
  for (const [what, manifest] of bad) assert.throws(() => entryFrom(found('acme/notes', manifest)), Error, what);
});

test('the index counts what it left out, keeps the oldest of a duplicate id and sorts by stars', () => {
  const index = assemble(
    [
      found('acme/notes', MANIFEST, 3, '2026-02-01T00:00:00Z'),
      found('copy/notes', MANIFEST, 90, '2026-03-01T00:00:00Z'),
      found('acme/empty', null),
      found('acme/broken', 'id ='),
      found('bad/actor', MANIFEST.replace('acme.notes', 'bad.actor'), 500),
      found('acme/tasks', MANIFEST.replace('acme.notes', 'acme.tasks'), 10),
    ],
    { repos: ['Bad/Actor'], ids: [] },
    when,
  );
  assert.deepEqual(index.source, { total: 6, missing: 1, invalid: 1, duplicate: 1, blocked: 1 });
  assert.deepEqual(index.plugins.map((p) => [p.id, p.repo]), [['acme.tasks', 'acme/tasks'], ['acme.notes', 'acme/notes']]);
  assert.equal(index.generated, '2026-10-15T12:00:00.000Z');
  assert.ok(validate(JSON.stringify(index)));
});

test('a blocked id is left out wherever it comes from', () => {
  const index = assemble([found('acme/notes', MANIFEST)], { repos: [], ids: ['acme.notes'] }, when);
  assert.equal(index.plugins.length, 0);
  assert.equal(index.source.blocked, 1);
});

test('the crawl searches the topic, skips forks and reads each manifest at the head commit', async () => {
  const asked: string[] = [];
  const answer = (status: number, body: unknown) => ({ ok: status === 200, status, json: async () => body, text: async () => String(body) });
  const get = async (url: string) => {
    asked.push(url);
    if (url.includes('/search/repositories')) {
      return answer(200, {
        items: [
          { full_name: 'acme/notes', default_branch: 'trunk', stargazers_count: 4, created_at: '2026-01-01T00:00:00Z' },
          { full_name: 'some/fork', default_branch: 'main', fork: true },
          { full_name: 'https://evil', default_branch: 'main' },
        ],
      });
    }
    if (url.endsWith('/repos/acme/notes/commits/trunk')) return answer(200, { sha: C2 });
    if (url.endsWith(`/acme/notes/${C2}/cornercase-plugin.toml`)) return answer(200, MANIFEST);
    return answer(404, {});
  };
  const crawled = await crawl(get, 'token');
  assert.deepEqual(crawled.map((f) => [f.repo.repo, f.commit, f.manifest !== null]), [['acme/notes', C2, true]]);
  assert.ok(asked[0].includes('topic%3Acornercase-plugin'));
  assert.equal(asked.length, 3);
  assert.equal(repoFrom({ full_name: 'a/b/c' }), null);
});

test('the published index is valid, and bad indexes are refused', () => {
  assert.ok(validate(readFileSync('plugins/index.json', 'utf8')));
  const good = assemble([found('acme/notes', MANIFEST)], empty, when);
  const changed = (field: string, value: unknown) => JSON.stringify({ ...good, plugins: [{ ...good.plugins[0], [field]: value }] });
  const bad: [string, string][] = [
    ['newer version', JSON.stringify({ ...good, version: 2 })],
    ['unknown field', changed('homepage', 'https://example.com')],
    ['short commit', changed('commit', 'abc')],
    ['url for a repo', changed('repo', 'https://github.com/a/b')],
    ['negative stars', changed('stars', -1)],
    ['escape in a name', changed('name', 'a\u001bb')],
    ['oversize', ' '.repeat(MAX_BYTES + 1)],
  ];
  for (const [what, source] of bad) assert.throws(() => validate(source), Error, what);
});

test('the page gives each plugin its pinned install command and source', () => {
  const [notes] = assemble([found('acme/notes', MANIFEST)], empty, when).plugins;
  assert.equal(installCommand(notes), `cornercase plugin install acme/notes@${C1}`);
  assert.equal(sourceUrl(notes), `https://github.com/acme/notes/tree/${C1}`);
});
