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
  VERSION,
  isObject,
  manifestFields,
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

export function pluginFrom(path: string, manifest: string): Plugin {
  if (new TextEncoder().encode(manifest).length > MANIFEST_BYTES) throw new Error('the manifest is too large');
  const raw: unknown = parse(manifest);
  if (!isObject(raw)) throw new Error('the manifest is not a table');
  if (typeof raw.api === 'number' && raw.api > 1) throw new Error('needs a newer plugin API');
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
    if (plugins.length < scan.plugins.length) source.blocked += 1;
    if (plugins.length === 0) {
      if (scan.plugins.length === 0 && scan.invalid === 0) source.missing += 1;
      skipped.push({ repo: repo.repo, commit, invalid: scan.invalid });
      continue;
    }
    const card: Card = { repo: repo.repo, stars: repo.stars, pushed: repo.pushed, created: repo.created, commit, plugins: plugins.map((p) => ({ ...p })) };
    if (repo.description?.trim()) card.description = repo.description.trim().slice(0, 350);
    if (repo.language) card.language = repo.language;
    if (scan.invalid) card.invalid = scan.invalid;
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
    scans.set(card.repo.toLowerCase(), {
      commit: card.commit,
      scan: { plugins: card.plugins.map(({ duplicate: _, ...plugin }) => plugin), invalid: card.invalid ?? 0 },
    });
  }
  for (const skip of previous.skipped) scans.set(skip.repo.toLowerCase(), { commit: skip.commit, scan: { plugins: [], invalid: skip.invalid } });
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
    const items = isObject(body) && Array.isArray(body.items) ? body.items : [];
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
  if (!head.ok) return null;
  const body = await head.json();
  const commit = isObject(body) && typeof body.sha === 'string' && /^[0-9a-f]{40}$/.test(body.sha) ? body.sha : null;
  if (!commit) return null;
  const seen = known.get(repo.repo.toLowerCase());
  if (seen?.commit === commit) return { repo, commit, scan: seen.scan };
  const tree = await get(`${API}/repos/${repo.repo}/git/trees/${commit}?recursive=1`, { headers });
  if (!tree.ok) return null;
  const paths = manifestPaths(await tree.json());
  const manifests = await Promise.all(
    paths.map(async (path) => {
      const file = await get(`${RAW}/${repo.repo}/${commit}/${path ? `${path}/` : ''}${MANIFEST}`);
      return { path, text: file.ok ? await file.text() : null };
    }),
  );
  return { repo, commit, scan: scan(manifests) };
}
