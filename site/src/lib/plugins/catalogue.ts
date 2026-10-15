export const VERSION = 1;
export const MANIFEST = 'cornercase-plugin.toml';
export const TOPIC = 'cornercase-plugin';
export const MAX_BYTES = 4 * 1024 * 1024;
const MAX_PLUGINS = 1000;
const MAX_PERMISSIONS = 16;
const PLATFORMS = ['macos', 'linux'];
const ENTRY = ['id', 'name', 'description', 'repo', 'commit', 'version', 'min_cornercase', 'platforms', 'permissions', 'stars'];
const REQUIRED = ['id', 'name', 'repo', 'commit', 'version', 'stars'];
const COUNTS = ['total', 'missing', 'invalid', 'duplicate', 'blocked'] as const;
const UNSAFE = /[\u0000-\u001f\u007f-\u009f​-‏‪-‮⁦-⁩﻿]/;

export type Entry = {
  id: string;
  name: string;
  description?: string;
  repo: string;
  commit: string;
  version: string;
  min_cornercase?: string;
  platforms?: string[];
  permissions?: string[];
  stars: number;
};

export type Counts = Record<(typeof COUNTS)[number], number>;

export type Index = { version: number; generated: string; source: Counts; plugins: Entry[] };

export const isObject = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null && !Array.isArray(value);

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

export function fields(raw: Record<string, unknown>, where: (field: string) => string): Omit<Entry, 'repo' | 'commit' | 'stars'> {
  const id = text(where('id'), raw.id, 64);
  if (!validId(id)) throw new Error(`${where('id')} must be lower-case letters, digits, . _ or -`);
  const out: Omit<Entry, 'repo' | 'commit' | 'stars'> = {
    id,
    name: text(where('name'), raw.name, 64),
    version: text(where('version'), raw.version, 32),
  };
  if (raw.description !== undefined && String(raw.description).trim()) {
    out.description = text(where('description'), raw.description, 200);
  }
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

function entry(raw: unknown, at: number): Entry {
  if (!isObject(raw)) throw new Error(`plugins[${at}] must be an object`);
  const where = (field: string) => `plugins[${at}].${field}`;
  for (const key of Object.keys(raw)) {
    if (!ENTRY.includes(key)) throw new Error(`${where(key)} is not a field of the index`);
  }
  for (const key of REQUIRED) {
    if (!(key in raw)) throw new Error(`${where(key)} is missing`);
  }
  const repo = text(where('repo'), raw.repo, 140);
  if (!validRepo(repo)) throw new Error(`${where('repo')} must be owner/repo on github.com`);
  const commit = text(where('commit'), raw.commit, 40);
  if (!/^[0-9a-f]{40}$/.test(commit)) throw new Error(`${where('commit')} must be 40 lower-case hexadecimal digits`);
  return { ...fields(raw, where), repo, commit, stars: count(where('stars'), raw.stars) };
}

export function validate(source: string): Index {
  if (new TextEncoder().encode(source).length > MAX_BYTES) throw new Error(`the index is larger than ${MAX_BYTES / 1024 / 1024} MiB`);
  const raw: unknown = JSON.parse(source);
  if (!isObject(raw)) throw new Error('the index must be an object');
  for (const key of Object.keys(raw)) {
    if (!['version', 'generated', 'source', 'plugins'].includes(key)) throw new Error(`${key} is not a field of the index`);
  }
  if (raw.version !== VERSION) throw new Error(`the index must be version ${VERSION}`);
  if (typeof raw.generated !== 'string' || Number.isNaN(Date.parse(raw.generated))) throw new Error('generated must be a date');
  if (!isObject(raw.source)) throw new Error('source must be an object');
  const counts = Object.fromEntries(COUNTS.map((key) => [key, count(`source.${key}`, raw.source && (raw.source as Record<string, unknown>)[key])])) as Counts;
  if (!Array.isArray(raw.plugins)) throw new Error('plugins must be a list');
  if (raw.plugins.length > MAX_PLUGINS) throw new Error(`more than ${MAX_PLUGINS} plugins`);
  const plugins = raw.plugins.map(entry);
  const seen = new Set<string>();
  for (const plugin of plugins) {
    if (seen.has(plugin.id)) throw new Error(`${plugin.id} is listed twice`);
    seen.add(plugin.id);
  }
  return { version: VERSION, generated: raw.generated, source: counts, plugins };
}

export const installCommand = (entry: Entry): string => `cornercase plugin install ${entry.repo}@${entry.commit}`;

export const sourceUrl = (entry: Entry): string => `https://github.com/${entry.repo}/tree/${entry.commit}`;
