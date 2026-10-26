import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { MAX_BYTES, installCommand, sourceUrl, validate } from '../src/lib/plugins/catalogue';
import { type Found, type Repo, assemble, crawl, keep, manifestPaths, pluginFrom, previousScans, repoFrom, scan } from '../src/lib/plugins/crawl';

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
const BUTTON = MANIFEST.replace('contexts = ["pane"]', 'contexts = ["button"]');
const sized = (fields: string) => MANIFEST.replace('command = ["./notes"]', `command = ["./notes"]\n${fields}`);

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
      return answer(200, { total_count: 3, incomplete_results: false, items: [
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

const quiet = async <T>(run: () => Promise<T>): Promise<{ value: T; warnings: string[] }> => {
  const warn = console.warn;
  const warnings: string[] = [];
  console.warn = (message: string) => void warnings.push(String(message));
  try {
    return { value: await run(), warnings };
  } finally {
    console.warn = warn;
  }
};

const broken = (match: string, status: number, base = github([])) => async (url: string) => (url.includes(match) ? answer(status, {}) : base(url));
const published = async () => {
  const index = assemble(await crawl(github([]), undefined, null), empty, when);
  index.repos[0].commit = C1;
  return index;
};
const ids = (found: Found[]) => found.map((f) => [f.repo.repo, f.scan.plugins.map((p) => p.id)]);
const RAW_HOST = 'https://raw.githubusercontent.com';
const failures: [string, (url: string) => boolean][] = [
  ['head', (u) => u.includes('/commits/')],
  ['tree', (u) => u.includes('/git/trees/')],
  ['manifest', (u) => u.startsWith(RAW_HOST)],
];

for (const status of [404, 410, 451]) {
  for (const [what, match] of failures.slice(0, 2)) {
    test(`a repository whose ${what} answers ${status} is skipped, not kept`, async () => {
      const { value, warnings } = await quiet(async () => crawl(async (u) => (match(u) ? answer(status, {}) : github([])(u)), undefined, await published()));
      assert.deepEqual(value, []);
      assert.match(warnings[0], /acme\/notes/);
    });
  }
}

for (const [what, match] of failures) {
  test(`a 503 on the ${what} keeps the published card`, async () => {
    const { value, warnings } = await quiet(async () => crawl(async (u) => (match(u) ? answer(503, {}) : github([])(u)), undefined, await published()));
    assert.deepEqual(ids(value), [['acme/notes', ['acme.notes']]]);
    assert.equal(value[0].commit, C1);
    assert.match(warnings[0], /acme\/notes.*503.*keeping/);
  });
  test(`a 503 on the ${what} with no published card skips the repository`, async () => {
    const { value, warnings } = await quiet(() => crawl(async (u) => (match(u) ? answer(503, {}) : github([])(u)), undefined, null));
    assert.deepEqual(value, []);
    assert.match(warnings[0], /skipping/);
  });
}

test('a network error for one repository keeps its published card', async () => {
  const { value } = await quiet(async () =>
    crawl(async (u) => { if (u.includes('/commits/')) throw new Error('socket hang up'); return github([])(u); }, undefined, await published()),
  );
  assert.deepEqual(ids(value), [['acme/notes', ['acme.notes']]]);
});

for (const [what, body] of [['an empty object', {}], ['a tree that is not a list', { tree: 'x' }], ['entries that are not objects', { tree: ['a', 1] }]] as const) {
  const malformed = (url: string) => (url.includes('/git/trees/') ? answer(200, body) : github([])(url));
  test(`a tree answer that is ${what} keeps the published card`, async () => {
    const { value, warnings } = await quiet(async () => crawl(async (u) => malformed(u), undefined, await published()));
    assert.deepEqual(ids(value), [['acme/notes', ['acme.notes']]]);
    assert.equal(value[0].commit, C1);
    assert.match(warnings[0], /acme\/notes.*malformed tree.*keeping/);
  });
  test(`a tree answer that is ${what} with no published card skips the repository`, async () => {
    const { value, warnings } = await quiet(() => crawl(async (u) => malformed(u), undefined, null));
    assert.deepEqual(value, []);
    assert.match(warnings[0], /malformed tree.*skipping/);
  });
}

test('a truncated but well-formed tree is still read', async () => {
  const { value } = await quiet(() =>
    crawl(async (u) => (u.includes('/git/trees/') ? answer(200, { truncated: true, tree: [{ type: 'blob', path: 'plugin/cornercase-plugin.toml' }] }) : github([])(u)), undefined, null),
  );
  assert.deepEqual(ids(value), [['acme/notes', ['acme.notes']]]);
});

test('a head answer without a commit keeps the published card', async () => {
  const { value } = await quiet(async () => crawl(async (u) => (u.includes('/commits/') ? answer(200, {}) : github([])(u)), undefined, await published()));
  assert.deepEqual(ids(value), [['acme/notes', ['acme.notes']]]);
});

test('one failing repository does not stop the others', async () => {
  const two = async (url: string) => {
    if (url.includes('/search/repositories')) {
      const items = ['acme/notes', 'acme/clock'].map((full_name) => ({ full_name, default_branch: 'trunk', stargazers_count: 1, created_at: '2026-01-01T00:00:00Z', pushed_at: '2026-10-01T00:00:00Z' }));
      return answer(200, { total_count: 2, incomplete_results: false, items });
    }
    if (url.includes('/repos/acme/clock/commits/')) return answer(503, {});
    return github([])(url);
  };
  const { value } = await quiet(() => crawl(two, undefined, null));
  assert.deepEqual(ids(value), [['acme/notes', ['acme.notes']]]);
});

test('a failing search still fails the whole crawl', async () => {
  await assert.rejects(crawl(broken('/search/repositories', 503), undefined, await published()), /503/);
});

test('a manifest that 404s after the tree listed it is unreadable, not a failure', async () => {
  const { value } = await quiet(() => crawl(broken(RAW_HOST, 404), undefined, null));
  assert.deepEqual(value.map((f) => f.scan), [{ plugins: [], invalid: 1 }]);
});

test('the kept index gets the current blocklist, and stays valid', () => {
  const index = assemble(
    [
      found('acme/tools', [{ path: '', text: MANIFEST }, { path: 'tasks', text: withId('acme.tasks') }], 3, '2026-02-01T00:00:00Z'),
      found('bad/actor', [{ path: '', text: withId('bad.actor') }], 500, '2026-03-01T00:00:00Z'),
      found('acme/clock', [{ path: '', text: withId('acme.clock') }], 10, '2026-04-01T00:00:00Z'),
    ],
    empty,
    when,
  );
  const kept = keep(index, { repos: ['Bad/Actor'], ids: ['acme.tasks'] });
  assert.deepEqual(kept.repos.map((c) => [c.repo, c.plugins.map((p) => p.id), c.blocked]), [['acme/clock', ['acme.clock'], undefined], ['acme/tools', ['acme.notes'], 1]]);
  assert.equal(kept.generated, index.generated);
  assert.ok(validate(JSON.stringify(kept)));
  const again = keep(kept, empty);
  assert.deepEqual(again.repos.map((c) => c.repo), ['acme/clock', 'acme/tools']);
  assert.equal(again.repos[1].blocked, 1);
  assert.equal(keep(index, empty).repos.length, 3);
});

test('a card that lost plugins to the blocklist stays marked after being kept', () => {
  const index = assemble([found('acme/notes', [{ path: '', text: MANIFEST }])], { repos: [], ids: ['acme.notes'] }, when);
  const kept = keep(index, { repos: [], ids: ['acme.notes'] });
  assert.deepEqual(kept.skipped, [{ repo: 'acme/notes', commit: C1, invalid: 0, blocked: 1 }]);
  assert.equal(previousScans(kept).size, 0);
});

test('manifests the app would refuse are invalid', () => {
  const bad = [
    `api = "invalid"\n${MANIFEST}`,
    `api = -1\n${MANIFEST}`,
    `api = 1.5\n${MANIFEST}`,
    `api = 1.0\n${MANIFEST}`,
    `api = 4294967296\n${MANIFEST}`,
    `actions = "not a list"\n${MANIFEST.split('[[actions]]')[0]}`,
    MANIFEST.replace('contexts = ["pane"]', 'contexts = []'),
    MANIFEST.replace('contexts = ["pane"]', 'contexts = ["window"]'),
    MANIFEST.replace('command = ["./notes"]', 'command = []'),
    MANIFEST.replace('command = ["./notes"]', 'command = ["../notes"]'),
    MANIFEST.replace('command = ["./notes"]', 'command = [1]'),
    `${MANIFEST}\n[[build]]\ncommand = ["make", "a\\u001bb"]\n`,
    `${MANIFEST}\n[[build]]\ncommand = ["make", "a\\u202eb"]\n`,
    `"a\\u001bb" = 1\n${MANIFEST}`,
    `${MANIFEST}\n[[actions]]\nid = "open"\ntitle = "again"\ncontexts = ["tab"]\ncommand = ["x"]\n`,
    MANIFEST.replace('title = "open notes"', 'title = "  "'),
    MANIFEST.replace('command = ["./notes"]', 'command = ["./notes", 1]'),
    `${MANIFEST}\n[[build]]\ncommand = ["../make"]\n`,
    `${MANIFEST}\n[[build]]\ncommand = []\n`,
    `${MANIFEST}\n[[events]]\non = "started"\ncommand = ["x"]\n`,
    `${MANIFEST}\n[[service]]\nid = "Bad Id"\ncommand = ["x"]\n`,
    `${MANIFEST}\n[download.macos-arm]\nurl = "http://example.com/a.tar.gz"\nsha256 = "${'a'.repeat(64)}"\n`,
    `${MANIFEST}\n[download.macos-arm]\nurl = "https://example.com/a b.tar.gz"\nsha256 = "${'a'.repeat(64)}"\n`,
    `${MANIFEST}\n[download.macos-arm]\nurl = "https://example.com/a.tar.gz"\nsha256 = "abc"\n`,
    `${MANIFEST}\n${'[[events]]\non = "opened"\ncommand = ["x"]\n'.repeat(64)}`,
    MANIFEST.replace('contexts = ["pane"]', 'contexts = ["Button"]'),
    ...['"modal"', '1'].map((placement) => sized(`placement = ${placement}`)),
    ...['', 'placement = "job"\n'].flatMap((placement) => [sized(`${placement}width = 80`), sized(`${placement}height = 10`)]),
    ...['width = 19', 'width = 501', 'height = 4', 'height = 201', 'width = 80.5', 'width = "80"'].map((size) => sized(`placement = "popup"\n${size}`)),
    `${MANIFEST}\n[button]\ntitle = "Top"\n`,
    `button = "x"\n${BUTTON}`,
    `button = 1979-05-27\n${BUTTON}`,
    `download = 1979-05-27\n${MANIFEST}`,
    ...['title = "  "', 'title = 1', 'icon = "ab"', 'icon = ""', 'icon = 1', 'icon = "\u0301"', 'icon = "界"', 'icon = "🚀"'].map((field) => `${BUTTON}\n[button]\n${field}\n`),
  ];
  assert.deepEqual(scan(bad.map((text) => ({ path: '', text }))), { plugins: [], invalid: bad.length });
  const good = `${MANIFEST}\napi = 0\n[[events]]\non = "opened"\ncommand = ["x"]\n[[service]]\nid = "s"\ncommand = ["./s"]\n[[build]]\ncommand = ["make"]\n[download.macos-arm]\nurl = "https://example.com/a.tar.gz"\nsha256 = "${'a'.repeat(64)}"\n`;
  assert.equal(scan([{ path: '', text: good.replace(`api = 0\n`, '') }]).plugins.length, 1);
  for (const api of [0, 1]) assert.equal(scan([{ path: '', text: `api = ${api}\n${MANIFEST}` }]).plugins.length, 1, `api = ${api}`);
});

test('actions on the plugin\'s button and popups are plugins', () => {
  const good = [
    BUTTON,
    sized('placement = "popup"').replace('contexts = ["pane"]', 'contexts = ["button", "project", "workspace", "tab", "pane"]'),
    ...['title = "Top"\nicon = "T"', 'title = "Top"', 'icon = "T"', 'icon = "▶"', 'icon = "⚙"', 'icon = "✔"', 'icon = "♥"', 'icon = "\u0903"', ''].map((fields) => `${BUTTON}\n[button]\n${fields}\n`),
    ...['width = 20', 'width = 500', 'height = 5', 'height = 200'].map((size) => sized(`placement = "popup"\n${size}`)),
  ];
  for (const text of good) assert.equal(pluginFrom('', text).id, 'acme.notes', text);
});

test('a bad context names every context there is', () => {
  assert.throws(() => pluginFrom('', MANIFEST.replace('contexts = ["pane"]', 'contexts = ["window"]')), /project, workspace, tab, pane and button/);
});

test('every example manifest in the plugins guide is a plugin', () => {
  const guide = readFileSync('src/content/docs/docs/guides/plugins.mdx', 'utf8');
  const examples = [...guide.matchAll(/```toml\n([\s\S]*?)```/g)].map((block) => block[1]).filter((block) => block.includes('[[actions]]'));
  assert.ok(examples.length >= 2, 'the guide lost an example manifest');
  for (const example of examples) {
    const text = example.replace(/sha256 = "[^"]*"/g, `sha256 = "${'a'.repeat(64)}"`);
    assert.doesNotThrow(() => pluginFrom('', text), example);
  }
});

test('unblocking an id brings its unchanged plugin back', async () => {
  const blocklist = { repos: [], ids: ['acme.notes'] };
  const blocked = assemble(await crawl(github([]), undefined, null), blocklist, when);
  assert.equal(blocked.repos.length, 0);
  assert.equal(blocked.skipped[0].blocked, 1);
  assert.equal(previousScans(blocked).size, 0);
  const asked: string[] = [];
  const again = assemble(await crawl(github(asked), undefined, blocked), empty, when);
  assert.deepEqual(again.repos.map((c) => c.plugins.map((p) => p.id)), [['acme.notes']]);
  assert.ok(asked.some((u) => u.includes('/git/trees/')));
  assert.ok(validate(JSON.stringify(blocked)));
});

test('a card that lost one plugin to the blocklist is read again, not reused', () => {
  const one = assemble([found('acme/tools', [{ path: '', text: MANIFEST }, { path: 'tasks', text: withId('acme.tasks') }])], { repos: [], ids: ['acme.tasks'] }, when);
  assert.equal(one.repos[0].blocked, 1);
  assert.equal(previousScans(one).size, 0);
  assert.ok(validate(JSON.stringify(one)));
});

const search = (body: unknown) => async (url: string) => (url.includes('/search/repositories') ? answer(200, body) : answer(404, {}));

for (const [what, body] of [
  ['an incomplete search', { total_count: 0, incomplete_results: true, items: [] }],
  ['a search without items', { total_count: 0, incomplete_results: false }],
  ['a search whose items are not a list', { total_count: 0, incomplete_results: false, items: 'none' }],
  ['a search without a total', { incomplete_results: false, items: [] }],
  ['a search that is not an object', []],
] as const) {
  test(`${what} fails the crawl`, async () => {
    await assert.rejects(crawl(search(body), undefined, null), /incomplete or malformed/);
  });
}

test('a complete, empty search is accepted', async () => {
  assert.deepEqual(await crawl(search({ total_count: 0, incomplete_results: false, items: [] }), undefined, null), []);
});

const T1 = new Date('2026-10-15T08:00:00Z');
const T2 = new Date('2026-10-16T08:00:00Z');
const stamped = (name: string, stars: number, extra: Partial<Found> = {}): Found => ({ ...found(name, [{ path: '', text: withId(`${name.replace('/', '.')}`) }], stars), ...extra });
const only = (index: ReturnType<typeof assemble>, name: string) => index.repos.find((c) => c.repo === name);

test('a repository the published index lacks joins at the crawl time with its first sample', () => {
  const card = only(assemble([stamped('acme/new', 7)], empty, T1), 'acme/new');
  assert.equal(card?.joined, T1.toISOString());
  assert.deepEqual(card?.stars_history, [['2026-10-15', 7]]);
});

test('joined and the history are carried from the published card', () => {
  const first = assemble([stamped('acme/new', 7)], empty, T1);
  const next = only(assemble([stamped('acme/new', 9)], empty, T2, first), 'acme/new');
  assert.equal(next?.joined, T1.toISOString());
  assert.deepEqual(next?.stars_history, [['2026-10-15', 7], ['2026-10-16', 9]]);
});

test('a second crawl on the same day updates nothing', () => {
  const first = assemble([stamped('acme/new', 7)], empty, T1);
  const later = new Date('2026-10-15T20:00:00Z');
  const next = only(assemble([stamped('acme/new', 50)], empty, later, first), 'acme/new');
  assert.deepEqual(next?.stars_history, [['2026-10-15', 7]]);
});

test('a published card without joined gets its created date, not the crawl time', () => {
  const first = assemble([stamped('acme/old', 7)], empty, T1);
  const legacy = JSON.parse(JSON.stringify(first));
  delete legacy.repos[0].joined;
  delete legacy.repos[0].stars_history;
  const next = only(assemble([stamped('acme/old', 8)], empty, T2, legacy), 'acme/old');
  assert.equal(next?.joined, '2026-01-01T00:00:00Z');
  assert.deepEqual(next?.stars_history, [['2026-10-16', 8]]);
});

test('the history keeps eight samples and drops the oldest', () => {
  let index = assemble([stamped('acme/new', 1)], empty, new Date('2026-10-01T08:00:00Z'));
  for (let d = 2; d <= 10; d += 1) index = assemble([stamped('acme/new', d)], empty, new Date(`2026-10-${String(d).padStart(2, '0')}T08:00:00Z`), index);
  const history = only(index, 'acme/new')?.stars_history ?? [];
  assert.equal(history.length, 8);
  assert.deepEqual(history[0], ['2026-10-03', 3]);
  assert.deepEqual(history[7], ['2026-10-10', 10]);
});

test('a card kept after a failed request keeps its joined and history, without a new sample', async () => {
  const first = assemble(await crawl(github([]), undefined, null), empty, T1);
  first.repos[0].commit = C1;
  const { value } = await quiet(() => crawl(async (u) => (u.includes('/commits/') ? answer(503, {}) : github([])(u)), undefined, first));
  const kept = only(assemble(value, empty, T2, first), 'acme/notes');
  assert.equal(kept?.joined, T1.toISOString());
  assert.deepEqual(kept?.stars_history, [['2026-10-15', 4]]);
});

test('the kept-index path keeps joined and history', () => {
  const first = assemble([stamped('acme/new', 7)], empty, T1);
  const kept = keep(first, empty);
  assert.equal(kept.repos[0].joined, T1.toISOString());
  assert.deepEqual(kept.repos[0].stars_history, [['2026-10-15', 7]]);
  assert.ok(validate(JSON.stringify(kept)));
});

test('the new card fields validate, and bad ones are refused', () => {
  const good = assemble([stamped('acme/new', 7)], empty, T1);
  const card = (change: Record<string, unknown>) => JSON.stringify({ ...good, repos: [{ ...good.repos[0], ...change }] });
  assert.ok(validate(card({ stars_history: [['2026-10-14', 3], ['2026-10-15', 7]], joined: '2026-10-01T00:00:00Z' })));
  const bad: [string, string][] = [
    ['not a list', card({ stars_history: 'x' })],
    ['nine samples', card({ stars_history: Array.from({ length: 9 }, (_, i) => [`2026-10-0${i + 1}`, 1]) })],
    ['a sample of three', card({ stars_history: [['2026-10-01', 1, 2]] })],
    ['a bad day', card({ stars_history: [['2026-13-01', 1]] })],
    ['a day that rolls over', card({ stars_history: [['2026-02-30', 1]] })],
    ['a day without zeros', card({ stars_history: [['2026-1-1', 1]] })],
    ['a day with a time', card({ stars_history: [['2026-10-01T00:00', 1]] })],
    ['a day twice', card({ stars_history: [['2026-10-01', 1], ['2026-10-01', 2]] })],
    ['descending days', card({ stars_history: [['2026-10-02', 1], ['2026-10-01', 2]] })],
    ['negative stars', card({ stars_history: [['2026-10-01', -1]] })],
    ['fractional stars', card({ stars_history: [['2026-10-01', 1.5]] })],
    ['a bad joined', card({ joined: 'yesterday' })],
  ];
  for (const [what, source] of bad) assert.throws(() => validate(source), Error, what);
});
