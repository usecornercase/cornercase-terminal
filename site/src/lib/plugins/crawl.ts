import { parse } from 'smol-toml';
import {
  type Card,
  type Counts,
  type Index,
  MANIFEST,
  MAX_MANIFESTS,
  type Plugin,
  type Skipped,
  TOPIC,
  UNSAFE,
  VERSION,
  isObject,
  manifestFields,
  validId,
  validPath,
  validRepo,
} from './catalogue';

const API = 'https://api.github.com';
const RAW = 'https://raw.githubusercontent.com';
const PAGES = 10;
const MANIFEST_BYTES = 64 * 1024;
const AT_ONCE = 8;

export type Repo = {
  repo: string;
  branch: string;
  description?: string;
  stars: number;
  language?: string;
  pushed: string;
  created: string;
  fork: boolean;
  archived: boolean;
};

export type Blocklist = { repos: string[]; ids: string[] };

export type Scan = { plugins: Plugin[]; invalid: number };

export type Found = { repo: Repo; commit: string; scan: Scan };

const MAX_ENTRIES = 64;
const CONTEXTS = ['project', 'workspace', 'tab', 'pane'];
const EVENTS = ['opened', 'closed', 'status', 'exited'];

const tables = (where: string, value: unknown): Record<string, unknown>[] => {
  if (value === undefined) return [];
  if (!Array.isArray(value) || !value.every(isObject)) throw new Error(`${where} must be a list of tables`);
  return value;
};

const unsafeIn = (value: unknown): boolean =>
  typeof value === 'string'
    ? UNSAFE.test(value)
    : Array.isArray(value)
      ? value.some(unsafeIn)
      : isObject(value)
        ? Object.entries(value).some(([key, item]) => UNSAFE.test(key) || unsafeIn(item))
        : false;

function command(where: string, value: unknown): void {
  if (!Array.isArray(value) || value.length === 0 || !value.every((arg) => typeof arg === 'string')) {
    throw new Error(`${where} needs a command: a non-empty list of strings`);
  }
  const program = value[0];
  if (!program.trim()) throw new Error(`${where} has no program`);
  if (!program.startsWith('/') && program.split('/').includes('..')) throw new Error(`${where} runs a program outside the plugin's folder`);
}

function identified(where: string, entry: Record<string, unknown>, seen: Set<string>): void {
  if (typeof entry.id !== 'string' || !validId(entry.id)) throw new Error(`${where} needs an id of lower-case letters, digits, . _ or -`);
  if (seen.has(`${where}:${entry.id}`)) throw new Error(`${where} ${entry.id} is declared twice`);
  seen.add(`${where}:${entry.id}`);
}

function checkManifest(raw: Record<string, unknown>): void {
  if (unsafeIn(raw)) throw new Error('the manifest holds control characters');
  if (raw.api !== undefined && (typeof raw.api !== 'bigint' || raw.api < 0n || raw.api > 4294967295n)) throw new Error('api must be a whole number');
  if (typeof raw.api === 'bigint' && raw.api > 1n) throw new Error('needs a newer plugin API');
  const actions = tables('actions', raw.actions);
  const events = tables('events', raw.events);
  const service = tables('service', raw.service);
  const window = tables('window', raw.window);
  const build = tables('build', raw.build);
  if (actions.length + events.length + service.length + window.length > MAX_ENTRIES) throw new Error(`more than ${MAX_ENTRIES} entries`);
  const seen = new Set<string>();
  for (const action of actions) {
    identified('action', action, seen);
    if (typeof action.title !== 'string' || !action.title.trim()) throw new Error('an action needs a title');
    const { contexts } = action;
    if (!Array.isArray(contexts) || contexts.length === 0 || !contexts.every((c) => typeof c === 'string' && CONTEXTS.includes(c))) {
      throw new Error('an action needs contexts from project, workspace, tab and pane');
    }
    command('an action', action.command);
  }
  for (const hook of events) {
    if (typeof hook.on !== 'string' || !EVENTS.includes(hook.on)) throw new Error('a hook needs an event: opened, closed, status or exited');
    command('a hook', hook.command);
  }
  for (const [kind, entries] of [['service', service], ['window', window]] as const) {
    for (const entry of entries) {
      identified(kind, entry, seen);
      command(kind, entry.command);
    }
  }
  for (const step of build) command('a build step', step.command);
  if (raw.download !== undefined) {
    if (!isObject(raw.download)) throw new Error('download must be a table of targets');
    for (const [target, entry] of Object.entries(raw.download)) {
      if (!isObject(entry) || typeof entry.url !== 'string' || typeof entry.sha256 !== 'string') throw new Error(`download ${target} needs a url and a sha256`);
      if (!/^[0-9a-fA-F]{64}$/.test(entry.sha256)) throw new Error(`download ${target} needs a sha256 of 64 hexadecimal digits`);
      if (!/^(https:\/\/|http:\/\/(127\.0\.0\.1|localhost):)/.test(entry.url) || /\s/.test(entry.url)) throw new Error(`download ${target} must be an https:// address`);
    }
  }
}

export function pluginFrom(path: string, manifest: string): Plugin {
  if (new TextEncoder().encode(manifest).length > MANIFEST_BYTES) throw new Error('the manifest is too large');
  const raw: unknown = parse(manifest, { integersAsBigInt: true });
  if (!isObject(raw)) throw new Error('the manifest is not a table');
  checkManifest(raw);
  return { path, ...manifestFields(raw, (field) => `${path ? `${path}/` : ''}${MANIFEST}: ${field}`) };
}

export function scan(manifests: { path: string; text: string | null }[]): Scan {
  const plugins: Plugin[] = [];
  let invalid = 0;
  for (const { path, text } of manifests) {
    try {
      if (text === null) throw new Error('unreadable');
      plugins.push(pluginFrom(path, text));
    } catch {
      invalid += 1;
    }
  }
  return { plugins, invalid };
}

export function manifestPaths(tree: unknown): string[] {
  const entries = isObject(tree) && Array.isArray(tree.tree) ? tree.tree : [];
  const paths: string[] = [];
  for (const entry of entries) {
    if (!isObject(entry) || entry.type !== 'blob' || typeof entry.path !== 'string') continue;
    const file = entry.path;
    if (file !== MANIFEST && !file.endsWith(`/${MANIFEST}`)) continue;
    const folder = file === MANIFEST ? '' : file.slice(0, -MANIFEST.length - 1);
    if (validPath(folder)) paths.push(folder);
  }
  return paths.sort((a, b) => a.split('/').length - b.split('/').length || a.localeCompare(b)).slice(0, MAX_MANIFESTS);
}

export function assemble(found: Found[], blocklist: Blocklist, generated: Date): Index {
  const source: Counts = { total: found.length, missing: 0, invalid: 0, duplicate: 0, blocked: 0 };
  const blockedRepos = new Set(blocklist.repos.map((repo) => repo.toLowerCase()));
  const repos: Card[] = [];
  const skipped: Skipped[] = [];
  for (const { repo, commit, scan } of [...found].sort((a, b) => a.repo.created.localeCompare(b.repo.created) || a.repo.repo.localeCompare(b.repo.repo))) {
    if (blockedRepos.has(repo.repo.toLowerCase())) {
      source.blocked += 1;
      continue;
    }
    source.invalid += scan.invalid;
    const plugins = scan.plugins.filter((p) => !blocklist.ids.includes(p.id));
    const withheld = scan.plugins.length - plugins.length;
    if (withheld) source.blocked += 1;
    if (plugins.length === 0) {
      if (scan.plugins.length === 0 && scan.invalid === 0) source.missing += 1;
      skipped.push({ repo: repo.repo, commit, invalid: scan.invalid, ...(withheld ? { blocked: withheld } : {}) });
      continue;
    }
    const card: Card = { repo: repo.repo, stars: repo.stars, pushed: repo.pushed, created: repo.created, commit, plugins: plugins.map((p) => ({ ...p })) };
    if (repo.description?.trim()) card.description = repo.description.trim().slice(0, 350);
    if (repo.language) card.language = repo.language;
    if (scan.invalid) card.invalid = scan.invalid;
    if (withheld) card.blocked = withheld;
    repos.push(card);
  }
  const ids = new Set<string>();
  for (const card of repos) {
    for (const plugin of card.plugins) {
      if (ids.has(plugin.id)) {
        plugin.duplicate = true;
        source.duplicate += 1;
      } else {
        ids.add(plugin.id);
      }
    }
  }
  const shown = repos.filter((card) => card.plugins.some((p) => !p.duplicate));
  shown.sort((a, b) => b.stars - a.stars || a.repo.localeCompare(b.repo));
  return { version: VERSION, generated: generated.toISOString(), source, repos: shown, skipped };
}

export function previousScans(previous: Index | null): Map<string, { commit: string; scan: Scan }> {
  const scans = new Map<string, { commit: string; scan: Scan }>();
  if (!previous) return scans;
  for (const card of previous.repos) {
    if (card.blocked) continue;
    scans.set(card.repo.toLowerCase(), {
      commit: card.commit,
      scan: { plugins: card.plugins.map(({ duplicate: _, ...plugin }) => plugin), invalid: card.invalid ?? 0 },
    });
  }
  for (const skip of previous.skipped) if (!skip.blocked) scans.set(skip.repo.toLowerCase(), { commit: skip.commit, scan: { plugins: [], invalid: skip.invalid } });
  return scans;
}

export function repoFrom(item: unknown): Repo | null {
  if (!isObject(item) || typeof item.full_name !== 'string' || !validRepo(item.full_name)) return null;
  const textOf = (value: unknown) => (typeof value === 'string' ? value : undefined);
  return {
    repo: item.full_name,
    branch: textOf(item.default_branch) ?? 'main',
    description: textOf(item.description)?.replace(/[\u0000-\u001f\u007f-\u009f​-‏‪-‮⁦-⁩﻿]/g, ' '),
    stars: typeof item.stargazers_count === 'number' ? item.stargazers_count : 0,
    language: textOf(item.language),
    pushed: textOf(item.pushed_at) ?? new Date(0).toISOString(),
    created: textOf(item.created_at) ?? new Date(0).toISOString(),
    fork: item.fork === true,
    archived: item.archived === true,
  };
}

type Answer = { ok: boolean; status: number; json(): Promise<unknown>; text(): Promise<string> };
type Fetch = (url: string, init?: { headers?: Record<string, string> }) => Promise<Answer>;

export async function crawl(get: Fetch, token: string | undefined, previous: Index | null): Promise<Found[]> {
  const headers: Record<string, string> = { Accept: 'application/vnd.github+json', 'User-Agent': 'cornercase-plugin-index' };
  if (token) headers.Authorization = `Bearer ${token}`;
  const repos: Repo[] = [];
  for (let page = 1; page <= PAGES; page += 1) {
    const query = encodeURIComponent(`topic:${TOPIC} fork:false archived:false`);
    const answer = await get(`${API}/search/repositories?q=${query}&per_page=100&page=${page}`, { headers });
    if (!answer.ok) throw new Error(`GitHub search answered ${answer.status}`);
    const body = await answer.json();
    if (!isObject(body) || body.incomplete_results === true || !Array.isArray(body.items) || typeof body.total_count !== 'number') {
      throw new Error('GitHub answered the search with incomplete or malformed results');
    }
    const items = body.items;
    for (const item of items) {
      const repo = repoFrom(item);
      if (repo && !repo.fork && !repo.archived) repos.push(repo);
    }
    if (items.length < 100) break;
  }
  const known = previousScans(previous);
  const found: Found[] = [];
  for (let i = 0; i < repos.length; i += AT_ONCE) {
    const batch = await Promise.all(repos.slice(i, i + AT_ONCE).map((repo) => look(get, headers, repo, known)));
    found.push(...batch.filter((f): f is Found => f !== null));
  }
  return found;
}

async function look(get: Fetch, headers: Record<string, string>, repo: Repo, known: Map<string, { commit: string; scan: Scan }>): Promise<Found | null> {
  const head = await get(`${API}/repos/${repo.repo}/commits/${encodeURIComponent(repo.branch)}`, { headers });
  if (head.status === 404 || head.status === 409) return null;
  if (!head.ok) throw new Error(`GitHub answered ${head.status} for the head of ${repo.repo}`);
  const body = await head.json();
  const commit = isObject(body) && typeof body.sha === 'string' && /^[0-9a-f]{40}$/.test(body.sha) ? body.sha : null;
  if (!commit) throw new Error(`GitHub gave no commit for ${repo.repo}`);
  const seen = known.get(repo.repo.toLowerCase());
  if (seen?.commit === commit) return { repo, commit, scan: seen.scan };
  const tree = await get(`${API}/repos/${repo.repo}/git/trees/${commit}?recursive=1`, { headers });
  if (tree.status === 404) return null;
  if (!tree.ok) throw new Error(`GitHub answered ${tree.status} for the tree of ${repo.repo}`);
  const paths = manifestPaths(await tree.json());
  const manifests = await Promise.all(
    paths.map(async (path) => {
      const file = await get(`${RAW}/${repo.repo}/${commit}/${path ? `${path}/` : ''}${MANIFEST}`);
      if (file.status !== 404 && !file.ok) throw new Error(`GitHub answered ${file.status} for the manifest of ${repo.repo}`);
      return { path, text: file.ok ? await file.text() : null };
    }),
  );
  return { repo, commit, scan: scan(manifests) };
}
