import { parse } from 'smol-toml';
import { type Counts, type Entry, type Index, MANIFEST, TOPIC, VERSION, fields, isObject, validRepo } from './catalogue';

const API = 'https://api.github.com';
const RAW = 'https://raw.githubusercontent.com';
const PAGES = 10;
const MANIFEST_BYTES = 64 * 1024;
const AT_ONCE = 8;

export type Repo = { repo: string; branch: string; stars: number; created: string; fork: boolean; archived: boolean };

export type Blocklist = { repos: string[]; ids: string[] };

export type Found = { repo: Repo; commit: string; manifest: string | null };

export function entryFrom(found: Found): Entry {
  if (found.manifest === null) throw new Error('no manifest');
  if (new TextEncoder().encode(found.manifest).length > MANIFEST_BYTES) throw new Error('the manifest is too large');
  const raw: unknown = parse(found.manifest);
  if (!isObject(raw)) throw new Error('the manifest is not a table');
  if (typeof raw.api === 'number' && raw.api > 1) throw new Error('needs a newer plugin API');
  return { ...fields(raw, (field) => `${MANIFEST}: ${field}`), repo: found.repo.repo, commit: found.commit, stars: found.repo.stars };
}

export function assemble(found: Found[], blocklist: Blocklist, generated: Date): Index {
  const source: Counts = { total: found.length, missing: 0, invalid: 0, duplicate: 0, blocked: 0 };
  const blockedRepos = new Set(blocklist.repos.map((repo) => repo.toLowerCase()));
  const valid: { entry: Entry; created: string }[] = [];
  for (const item of found) {
    if (blockedRepos.has(item.repo.repo.toLowerCase())) {
      source.blocked += 1;
      continue;
    }
    if (item.manifest === null) {
      source.missing += 1;
      continue;
    }
    try {
      const entry = entryFrom(item);
      if (blocklist.ids.includes(entry.id)) source.blocked += 1;
      else valid.push({ entry, created: item.repo.created });
    } catch {
      source.invalid += 1;
    }
  }
  valid.sort((a, b) => a.created.localeCompare(b.created) || a.entry.repo.localeCompare(b.entry.repo));
  const plugins: Entry[] = [];
  const ids = new Set<string>();
  for (const { entry } of valid) {
    if (ids.has(entry.id)) {
      source.duplicate += 1;
      continue;
    }
    ids.add(entry.id);
    plugins.push(entry);
  }
  plugins.sort((a, b) => b.stars - a.stars || a.id.localeCompare(b.id));
  return { version: VERSION, generated: generated.toISOString(), source, plugins };
}

export function repoFrom(item: unknown): Repo | null {
  if (!isObject(item) || typeof item.full_name !== 'string' || !validRepo(item.full_name)) return null;
  return {
    repo: item.full_name,
    branch: typeof item.default_branch === 'string' ? item.default_branch : 'main',
    stars: typeof item.stargazers_count === 'number' ? item.stargazers_count : 0,
    created: typeof item.created_at === 'string' ? item.created_at : '',
    fork: item.fork === true,
    archived: item.archived === true,
  };
}

type Fetch = (url: string, init?: { headers?: Record<string, string> }) => Promise<{ ok: boolean; status: number; json(): Promise<unknown>; text(): Promise<string> }>;

export async function crawl(get: Fetch, token: string | undefined): Promise<Found[]> {
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
  const found: Found[] = [];
  for (let i = 0; i < repos.length; i += AT_ONCE) {
    found.push(...(await Promise.all(repos.slice(i, i + AT_ONCE).map((repo) => look(get, headers, repo)))).filter((f): f is Found => f !== null));
  }
  return found;
}

async function look(get: Fetch, headers: Record<string, string>, repo: Repo): Promise<Found | null> {
  const branch = await get(`${API}/repos/${repo.repo}/commits/${encodeURIComponent(repo.branch)}`, { headers });
  if (!branch.ok) return null;
  const body = await branch.json();
  const commit = isObject(body) && typeof body.sha === 'string' && /^[0-9a-f]{40}$/.test(body.sha) ? body.sha : null;
  if (!commit) return null;
  const manifest = await get(`${RAW}/${repo.repo}/${commit}/${MANIFEST}`);
  return { repo, commit, manifest: manifest.ok ? await manifest.text() : null };
}
