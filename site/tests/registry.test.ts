import assert from 'node:assert/strict';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { test } from 'node:test';

import { builtIns, everyFlow, problems, registry, toFlow } from '../src/lib/registry';
import { diagramSvg } from '../src/lib/registry-diagram';

const fixture = resolve(process.cwd(), 'tests/fixtures/flows.json');

test('the list is sorted by stars, and an entry that sets env is skipped', () => {
  const warn = console.warn;
  const skipped: string[] = [];
  console.warn = (m: string) => skipped.push(m);
  try {
    const flows = registry(fixture);
    assert.deepEqual(flows.map((f) => f.repo), ['octocat/docs-pass', 'mona/quiet']);
    assert.deepEqual(skipped.map((m) => m.includes('hubot/with-env') && m.includes('env or args')), [true]);
  } finally {
    console.warn = warn;
  }
});

test('a listed flow has what its page shows', () => {
  for (const flow of registry(fixture)) {
    assert.ok(flow.owner && flow.roles.length > 0 && flow.stages.length > 0 && flow.text.includes('prompt'));
    assert.ok(diagramSvg(flow, flow.name).startsWith('<svg'));
  }
  const [first] = registry(fixture);
  assert.deepEqual(first.tags, ['docs']);
  assert.equal(first.slug, 'octocat/docs-pass');
});

test('a flow that is too big, unreadable or broken is refused', () => {
  const [{ text }] = JSON.parse(JSON.stringify(registry(fixture)));
  assert.deepEqual(problems(text), []);
  assert.ok(problems('x'.repeat(256 * 1024 + 1))[0].includes('KiB'));
  assert.ok(problems('name = = 1').length > 0);
  assert.ok(problems(text.replace('agent = "codex"', 'agent = "nobody"')).some((p) => p.includes('unknown agent')));
  assert.ok(problems(text.replace('roles = ["checker"]', 'roles = ["ghost"]')).some((p) => p.includes('not a role')));
  assert.ok(problems(text.replace('prompt = "Keep', 'args = ["--x"]\nprompt = "Keep')).some((p) => p.includes('env or args')));
  assert.ok(problems(text.replace('to = 1', 'to = 2')).some((p) => p.includes('bad stage')));
});

test('an empty snapshot lists nothing', () => {
  const empty = resolve(process.cwd(), 'src/data/flows.json');
  assert.ok(Array.isArray(registry(empty)));
});

test('a hostile name never becomes markup in the diagram', () => {
  const [{ text }] = JSON.parse(JSON.stringify(registry(fixture)));
  const name = '</svg><script>alert(1)</script>"&';
  const flow = toFlow({ repo: 'a/b', stars: 1, description: '', topics: [], file: 'flow.toml', text: text.replace('Docs pass', 'x').replace('name = "x"', `name = ${JSON.stringify(name)}`) });
  const roleName = toFlow({ repo: 'a/b', stars: 1, description: '', topics: [], file: 'flow.toml', text: text.replaceAll('writer', '<img onerror=x>') });
  for (const f of [flow, roleName]) {
    const svg = diagramSvg(f, f.name);
    assert.ok(!svg.includes('<script') && !svg.includes('<img'), svg.slice(0, 200));
    assert.equal(svg.match(/<\/svg>/g)?.length, 1);
  }
});

test('the repository name must be owner/repo', () => {
  const [{ text }] = JSON.parse(JSON.stringify(registry(fixture)));
  const entry = (repo: string) => ({ repo, stars: 1, description: '', topics: [], file: 'flow.toml', text });
  for (const bad of ['a', 'a/b/c', '../b', 'a/..', '/b', 'a b/c', 'a/b?x']) assert.throws(() => toFlow(entry(bad)), /owner\/repo/, bad);
  assert.equal(toFlow(entry('a.b/c_d-e')).slug, 'a.b/c_d-e');
});

test('lengths and types are capped', () => {
  const [{ text }] = JSON.parse(JSON.stringify(registry(fixture)));
  assert.ok(problems(text.replace('Docs pass', 'x'.repeat(81))).length > 0);
  assert.ok(problems(text.replace('A writer', 'x'.repeat(301))).some((p) => p.includes('description')));
  assert.ok(problems(text.replace('prompt = "Keep', `prompt = "${'x'.repeat(33 * 1024)}`)).some((p) => p.includes('prompt')));
  assert.ok(problems(text.replace('edits = true', 'edits = true\nmodel = 3')).some((p) => p.includes('model')));
  assert.ok(problems(text.replace('edits = true', 'edits = "yes"')).some((p) => p.includes('edits')));
  assert.ok(problems(text.replace('roles = ["writer"]', 'roles = [1]')).some((p) => p.includes('not a name')));
  assert.ok(problems(text.replace('when = "findings"', `when = "${'x'.repeat(61)}"`)).some((p) => p.includes('loop label')));
});

test('the built-in flows come first, as built in, and each draws a plan', () => {
  const all = everyFlow();
  const built = builtIns();
  assert.deepEqual(built.map((f) => f.name), ['Full issue', 'Cross review', 'Quick', 'Bug, test first', 'Arena']);
  assert.deepEqual(all.slice(0, built.length).map((f) => f.slug), built.map((f) => f.slug));
  for (const flow of built) {
    assert.ok(flow.builtIn && flow.owner === 'cornercase' && flow.repo === 'usecornercase/cornercase-terminal' && flow.stars === 0);
    assert.ok(flow.slug.startsWith('built-in/') && flow.roles.length > 0 && flow.text.includes('prompt'));
    assert.ok(diagramSvg(flow, flow.name).startsWith('<svg'));
  }
  assert.equal(new Set(all.map((f) => f.slug)).size, all.length);
});

test('a community repository cannot take the built-in flows folder', () => {
  const dir = mkdtempSync(join(tmpdir(), 'flows-'));
  const [{ text }] = JSON.parse(JSON.stringify(registry(fixture)));
  const file = join(dir, 'flows.json');
  writeFileSync(file, JSON.stringify({ flows: [{ repo: 'Built-In/x', stars: 5, description: '', topics: [], file: 'flow.toml', text }] }));
  const warn = console.warn;
  const said: string[] = [];
  console.warn = (m: string) => said.push(m);
  try {
    assert.deepEqual(registry(file), []);
  } finally {
    console.warn = warn;
    rmSync(dir, { recursive: true, force: true });
  }
  assert.ok(said[0].includes('built-in'));
});

test('a role can have a worktree of its own, with version 2', () => {
  const [{ text }] = JSON.parse(JSON.stringify(registry(fixture)));
  const own = text.replace('version = 1', 'version = 2').replace('edits = true', 'edits = true\nworktree = "own"');
  assert.deepEqual(problems(own), []);
  const flow = toFlow({ repo: 'a/b', stars: 1, description: '', topics: [], file: 'flow.toml', text: own });
  assert.deepEqual(flow.roles.map((r) => r.worktree), ['own', 'shared']);
  assert.ok(problems(own.replace('version = 2', 'version = 1')).some((p) => p.includes('version = 2')));
  assert.ok(problems(own.replace('version = 2\n', '')).some((p) => p.includes('version = 2')));
  assert.ok(problems(own.replace('version = 2', 'version = 3')).some((p) => p.includes('newer')));
  assert.ok(problems(own.replace('"own"', '"mine"')).some((p) => p.includes('shared or own')));
  assert.ok(problems(own.replace('prompt = "Keep', 'worktree = "own"\nprompt = "Keep')).some((p) => p.includes('orchestrator cannot')));
});

test('two roles that edit the shared worktree side by side are refused', () => {
  const [{ text }] = JSON.parse(JSON.stringify(registry(fixture)));
  const both = text
    .replace('prompt = "You check docs', 'edits = true\nprompt = "You check docs')
    .replace('[[stage]]\nroles = ["writer"]\n\n[[stage]]\nroles = ["checker"]\n\n[stage.back]\nto = 1\nwhen = "findings"\nat_most = 2', '[[stage]]\nroles = ["writer", "checker"]');
  assert.ok(problems(both).some((p) => p.includes('both edit the shared worktree')), problems(both).join('; '));
  const apart = both.replace('version = 1', 'version = 2').replace('edits = true\nprompt = "You check docs', 'edits = true\nworktree = "own"\nprompt = "You check docs');
  assert.deepEqual(problems(apart), []);
});

test('the arena flow has competing coders with a worktree of their own, and its plan says so', () => {
  const arena = builtIns().find((f) => f.name === 'Arena');
  assert.ok(arena);
  const own = arena.roles.filter((r) => r.worktree === 'own');
  assert.ok(own.length >= 2 && own.every((r) => r.edits));
  assert.ok(arena.roles.filter((r) => r.worktree !== 'own').every((r) => r.name !== own[0].name));
  const svg = diagramSvg(arena, arena.name);
  assert.ok(svg.startsWith('<svg') && svg.includes('own'));
  const plain = diagramSvg({ ...arena, roles: arena.roles.map((r) => ({ ...r, worktree: 'shared' as const })) }, arena.name);
  assert.notEqual(svg, plain, 'an own role draws its tag');
});
