export const VERSION = 1;
export const MANIFEST = 'cornercase-plugin.toml';
export const TOPIC = 'cornercase-plugin';
export const MAX_BYTES = 8 * 1024 * 1024;
export const MAX_MANIFESTS = 20;
const MAX_REPOS = 1000;
const MAX_PERMISSIONS = 16;
export const MAX_HISTORY = 8;
const PLATFORMS = ['macos', 'linux'];
const REPO_FIELDS = ['repo', 'description', 'stars', 'language', 'pushed', 'created', 'commit', 'invalid', 'blocked', 'joined', 'stars_history', 'plugins'];
const PLUGIN_FIELDS = ['path', 'id', 'name', 'version', 'description', 'min_cornercase', 'platforms', 'permissions', 'duplicate'];
const COUNTS = ['total', 'missing', 'invalid', 'duplicate', 'blocked'] as const;
export const UNSAFE = /[\u0000-\u001f\u007f-\u009f​-‏‪-‮⁦-⁩﻿]/;

export type Plugin = {
  path: string;
  id: string;
  name: string;
  version: string;
  description?: string;
  min_cornercase?: string;
  platforms?: string[];
  permissions?: string[];
  duplicate?: boolean;
};

export type Star = [day: string, stars: number];

export type Card = {
  repo: string;
  description?: string;
  stars: number;
  language?: string;
  pushed: string;
  created: string;
  commit: string;
  invalid?: number;
  blocked?: number;
  joined?: string;
  stars_history?: Star[];
  plugins: Plugin[];
};

export type Skipped = { repo: string; commit: string; invalid: number; blocked?: number };

export type Counts = Record<(typeof COUNTS)[number], number>;

export type Index = { version: number; generated: string; source: Counts; repos: Card[]; skipped: Skipped[] };

export const isObject = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null && !Array.isArray(value) && !(value instanceof Date);

export function text(where: string, value: unknown, max: number): string {
  if (typeof value !== 'string') throw new Error(`${where} must be a string`);
  if (!value.trim()) throw new Error(`${where} is empty`);
  if ([...value].length > max) throw new Error(`${where} is longer than ${max} characters`);
  if (UNSAFE.test(value)) throw new Error(`${where} holds control characters`);
  return value;
}

function list(where: string, value: unknown, max: number, item: (v: unknown, i: number) => string): string[] {
  if (!Array.isArray(value)) throw new Error(`${where} must be a list`);
  if (value.length > max) throw new Error(`${where} has more than ${max} items`);
  return value.map(item);
}

const count = (where: string, value: unknown): number => {
  if (typeof value !== 'number' || !Number.isSafeInteger(value) || value < 0) throw new Error(`${where} must be a whole number`);
  return value;
};

const DAY = /^\d{4}-\d{2}-\d{2}$/;

export const dayOf = (when: Date): string => when.toISOString().slice(0, 10);

function history(where: string, value: unknown): Star[] {
  if (!Array.isArray(value) || value.length > MAX_HISTORY) throw new Error(`${where} must be a list of at most ${MAX_HISTORY} samples`);
  const out: Star[] = [];
  for (const [i, sample] of value.entries()) {
    if (!Array.isArray(sample) || sample.length !== 2) throw new Error(`${where}[${i}] must be a day and a number of stars`);
    const [day, stars] = sample;
    if (typeof day !== 'string' || !DAY.test(day) || Number.isNaN(Date.parse(day)) || dayOf(new Date(day)) !== day) throw new Error(`${where}[${i}] must start with a day as YYYY-MM-DD`);
    if (out.length > 0 && out[out.length - 1][0] >= day) throw new Error(`${where} must list each day once, oldest first`);
    out.push([day, count(`${where}[${i}]`, stars)]);
  }
  return out;
}

const date = (where: string, value: unknown): string => {
  if (typeof value !== 'string' || Number.isNaN(Date.parse(value))) throw new Error(`${where} must be a date`);
  return value;
};

const commitOf = (where: string, value: unknown): string => {
  if (typeof value !== 'string' || !/^[0-9a-f]{40}$/.test(value)) throw new Error(`${where} must be 40 lower-case hexadecimal digits`);
  return value;
};

const optional = (where: string, value: unknown, max: number): string | undefined =>
  value === undefined || (typeof value === 'string' && !value.trim()) ? undefined : text(where, value, max);

export const validId = (id: string): boolean => id.length <= 64 && /^[a-z0-9][a-z0-9._-]*$/.test(id);

export const validRepo = (repo: string): boolean => {
  const [owner, name, ...rest] = repo.split('/');
  return (
    rest.length === 0 &&
    name !== undefined &&
    /^[A-Za-z0-9][A-Za-z0-9-]{0,38}$/.test(owner) &&
    /^[A-Za-z0-9_][A-Za-z0-9._-]{0,99}$/.test(name) &&
    !name.toLowerCase().endsWith('.git')
  );
};

export const validPath = (path: string): boolean =>
  path === '' || (path.length <= 200 && path.split('/').every((part) => /^[A-Za-z0-9_][A-Za-z0-9._-]*$/.test(part)));

export function manifestFields(raw: Record<string, unknown>, where: (field: string) => string): Omit<Plugin, 'path' | 'duplicate'> {
  const id = text(where('id'), raw.id, 64);
  if (!validId(id)) throw new Error(`${where('id')} must be lower-case letters, digits, . _ or -`);
  const out: Omit<Plugin, 'path' | 'duplicate'> = {
    id,
    name: text(where('name'), raw.name, 64),
    version: text(where('version'), raw.version, 32),
  };
  const description = optional(where('description'), raw.description, 200);
  if (description) out.description = description;
  if (raw.min_cornercase !== undefined) out.min_cornercase = text(where('min_cornercase'), raw.min_cornercase, 32);
  if (raw.platforms !== undefined) {
    out.platforms = list(where('platforms'), raw.platforms, PLATFORMS.length, (p) => {
      if (typeof p !== 'string' || !PLATFORMS.includes(p)) throw new Error(`${where('platforms')} holds an unknown platform`);
      return p;
    });
  }
  if (raw.permissions !== undefined) {
    out.permissions = list(where('permissions'), raw.permissions, MAX_PERMISSIONS, (p, i) => text(`${where('permissions')}[${i}]`, p, 32));
  }
  return out;
}

function keys(raw: Record<string, unknown>, allowed: string[], where: (field: string) => string): void {
  for (const key of Object.keys(raw)) {
    if (!allowed.includes(key)) throw new Error(`${where(key)} is not a field of the index`);
  }
}

function plugin(raw: unknown, where: (field: string) => string): Plugin {
  if (!isObject(raw)) throw new Error(`${where('')} must be an object`);
  keys(raw, PLUGIN_FIELDS, where);
  if (typeof raw.path !== 'string' || !validPath(raw.path)) throw new Error(`${where('path')} must be a folder inside the repository`);
  const out: Plugin = { path: raw.path, ...manifestFields(raw, where) };
  if (raw.duplicate !== undefined) {
    if (raw.duplicate !== true) throw new Error(`${where('duplicate')} must be true when present`);
    out.duplicate = true;
  }
  return out;
}

function card(raw: unknown, at: number): Card {
  if (!isObject(raw)) throw new Error(`repos[${at}] must be an object`);
  const where = (field: string) => `repos[${at}].${field}`;
  keys(raw, REPO_FIELDS, where);
  const repo = text(where('repo'), raw.repo, 140);
  if (!validRepo(repo)) throw new Error(`${where('repo')} must be owner/repo on github.com`);
  if (!Array.isArray(raw.plugins) || raw.plugins.length === 0 || raw.plugins.length > MAX_MANIFESTS) {
    throw new Error(`${where('plugins')} must list 1 to ${MAX_MANIFESTS} plugins`);
  }
  const out: Card = {
    repo,
    stars: count(where('stars'), raw.stars),
    pushed: date(where('pushed'), raw.pushed),
    created: date(where('created'), raw.created),
    commit: commitOf(where('commit'), raw.commit),
    plugins: raw.plugins.map((p, i) => plugin(p, (field) => `${where('plugins')}[${i}]${field ? `.${field}` : ''}`)),
  };
  const description = optional(where('description'), raw.description, 350);
  if (description) out.description = description;
  const language = optional(where('language'), raw.language, 40);
  if (language) out.language = language;
  if (raw.invalid !== undefined) out.invalid = count(where('invalid'), raw.invalid);
  if (raw.blocked !== undefined) out.blocked = count(where('blocked'), raw.blocked);
  if (raw.joined !== undefined) out.joined = date(where('joined'), raw.joined);
  if (raw.stars_history !== undefined) out.stars_history = history(where('stars_history'), raw.stars_history);
  if (new Set(out.plugins.map((p) => p.path)).size !== out.plugins.length) throw new Error(`${where('plugins')} lists a folder twice`);
  return out;
}

function skipped(raw: unknown, at: number): Skipped {
  if (!isObject(raw)) throw new Error(`skipped[${at}] must be an object`);
  const where = (field: string) => `skipped[${at}].${field}`;
  keys(raw, ['repo', 'commit', 'invalid', 'blocked'], where);
  const repo = text(where('repo'), raw.repo, 140);
  if (!validRepo(repo)) throw new Error(`${where('repo')} must be owner/repo on github.com`);
  const out: Skipped = { repo, commit: commitOf(where('commit'), raw.commit), invalid: count(where('invalid'), raw.invalid) };
  if (raw.blocked !== undefined) out.blocked = count(where('blocked'), raw.blocked);
  return out;
}

export function validate(source: string): Index {
  if (new TextEncoder().encode(source).length > MAX_BYTES) throw new Error(`the index is larger than ${MAX_BYTES / 1024 / 1024} MiB`);
  const raw: unknown = JSON.parse(source);
  if (!isObject(raw)) throw new Error('the index must be an object');
  keys(raw, ['version', 'generated', 'source', 'repos', 'skipped'], (key) => key);
  if (raw.version !== VERSION) throw new Error(`the index must be version ${VERSION}`);
  const generated = date('generated', raw.generated);
  if (!isObject(raw.source)) throw new Error('source must be an object');
  const given = raw.source;
  const counts = Object.fromEntries(COUNTS.map((key) => [key, count(`source.${key}`, given[key])])) as Counts;
  if (!Array.isArray(raw.repos) || raw.repos.length > MAX_REPOS) throw new Error(`repos must be a list of at most ${MAX_REPOS}`);
  if (!Array.isArray(raw.skipped) || raw.skipped.length > MAX_REPOS) throw new Error(`skipped must be a list of at most ${MAX_REPOS}`);
  const repos = raw.repos.map(card);
  const ids = new Set<string>();
  const names = new Set<string>();
  for (const { repo, plugins } of repos) {
    if (names.has(repo.toLowerCase())) throw new Error(`${repo} is listed twice`);
    names.add(repo.toLowerCase());
    for (const p of plugins.filter((p) => !p.duplicate)) {
      if (ids.has(p.id)) throw new Error(`${p.id} is listed twice`);
      ids.add(p.id);
    }
  }
  return { version: VERSION, generated, source: counts, repos, skipped: raw.skipped.map(skipped) };
}

export const installSpec = (card: Card, plugin: Plugin): string =>
  `${card.repo}${plugin.path ? `/${plugin.path}` : ''}@${card.commit}`;

export const installCommand = (card: Card, plugin: Plugin): string => `cornercase plugin install ${installSpec(card, plugin)}`;

export const sourceUrl = (card: Card, plugin: Plugin): string =>
  `https://github.com/${card.repo}/tree/${card.commit}${plugin.path ? `/${plugin.path}` : ''}`;
