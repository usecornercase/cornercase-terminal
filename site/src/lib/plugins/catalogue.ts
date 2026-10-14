export const VERSION = 1;
export const MAX_BYTES = 1024 * 1024;
const MAX_PLUGINS = 500;
const MAX_PERMISSIONS = 16;
const PLATFORMS = ['macos', 'linux'];
const TARGETS = ['aarch64-apple-darwin', 'x86_64-apple-darwin', 'aarch64-unknown-linux-gnu', 'x86_64-unknown-linux-gnu'];
const FIELDS = ['id', 'name', 'description', 'repo', 'ref', 'commit', 'version', 'min_cornercase', 'platforms', 'permissions', 'sha256'];
const REQUIRED = ['id', 'name', 'description', 'repo', 'commit', 'version'];
const UNSAFE = /[\u0000-\u001f\u007f-\u009f​-‏‪-‮⁦-⁩﻿]/;

export type Entry = {
  id: string;
  name: string;
  description: string;
  repo: string;
  ref?: string;
  commit: string;
  version: string;
  min_cornercase?: string;
  platforms?: string[];
  permissions?: string[];
  sha256?: Record<string, string>;
};

export type Index = { version: number; plugins: Entry[] };

const isObject = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null && !Array.isArray(value);

function text(where: string, value: unknown, max: number): string {
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

export const validRepo = (repo: string): boolean => {
  const [owner, name, ...rest] = repo.split('/');
  return (
    rest.length === 0 &&
    name !== undefined &&
    /^[A-Za-z0-9][A-Za-z0-9-]{0,38}$/.test(owner) &&
    /^[A-Za-z0-9_][A-Za-z0-9._-]{0,99}$/.test(name) &&
    !name.endsWith('.git')
  );
};

const validRef = (ref: string): boolean =>
  ref.length <= 100 && /^[A-Za-z0-9_.][A-Za-z0-9_./-]*$/.test(ref) && !ref.includes('..');

function entry(raw: unknown, at: number): Entry {
  if (!isObject(raw)) throw new Error(`plugins[${at}] must be an object`);
  const where = (field: string) => `plugins[${at}].${field}`;
  for (const key of Object.keys(raw)) {
    if (!FIELDS.includes(key)) throw new Error(`${where(key)} is not a field of the catalogue`);
  }
  for (const key of REQUIRED) {
    if (!(key in raw)) throw new Error(`${where(key)} is missing`);
  }
  const id = text(where('id'), raw.id, 64);
  if (!/^[a-z0-9][a-z0-9._-]*$/.test(id)) throw new Error(`${where('id')} must be lower-case letters, digits, . _ or -`);
  const repo = text(where('repo'), raw.repo, 140);
  if (!validRepo(repo)) throw new Error(`${where('repo')} must be owner/repo on github.com`);
  const commit = text(where('commit'), raw.commit, 40);
  if (!/^[0-9a-f]{40}$/.test(commit)) throw new Error(`${where('commit')} must be 40 lower-case hexadecimal digits`);
  const out: Entry = {
    id,
    name: text(where('name'), raw.name, 64),
    description: text(where('description'), raw.description, 200),
    repo,
    commit,
    version: text(where('version'), raw.version, 32),
  };
  if ('ref' in raw) {
    const ref = text(where('ref'), raw.ref, 100);
    if (!validRef(ref)) throw new Error(`${where('ref')} must be a tag or a branch name`);
    out.ref = ref;
  }
  if ('min_cornercase' in raw) out.min_cornercase = text(where('min_cornercase'), raw.min_cornercase, 32);
  if ('platforms' in raw) {
    out.platforms = list(where('platforms'), raw.platforms, PLATFORMS.length, (p) => {
      if (typeof p !== 'string' || !PLATFORMS.includes(p)) throw new Error(`${where('platforms')} holds an unknown platform`);
      return p;
    });
  }
  if ('permissions' in raw) {
    out.permissions = list(where('permissions'), raw.permissions, MAX_PERMISSIONS, (p, i) => text(`${where('permissions')}[${i}]`, p, 32));
  }
  if ('sha256' in raw) {
    if (!isObject(raw.sha256)) throw new Error(`${where('sha256')} must be an object`);
    const sums: Record<string, string> = {};
    for (const [target, sum] of Object.entries(raw.sha256)) {
      if (!TARGETS.includes(target)) throw new Error(`${where('sha256')} names ${target}, which cornercase does not build`);
      if (typeof sum !== 'string' || !/^[0-9a-f]{64}$/.test(sum)) throw new Error(`${where('sha256')}.${target} must be 64 lower-case hexadecimal digits`);
      sums[target] = sum;
    }
    out.sha256 = sums;
  }
  return out;
}

export function validate(source: string): Index {
  if (new TextEncoder().encode(source).length > MAX_BYTES) throw new Error(`the index is larger than ${MAX_BYTES / 1024} KiB`);
  const raw: unknown = JSON.parse(source);
  if (!isObject(raw)) throw new Error('the index must be an object');
  for (const key of Object.keys(raw)) {
    if (key !== 'version' && key !== 'plugins') throw new Error(`${key} is not a field of the index`);
  }
  if (raw.version !== VERSION) throw new Error(`the index must be version ${VERSION}`);
  if (!Array.isArray(raw.plugins)) throw new Error('plugins must be a list');
  if (raw.plugins.length > MAX_PLUGINS) throw new Error(`more than ${MAX_PLUGINS} plugins`);
  const plugins = raw.plugins.map(entry);
  const seen = new Set<string>();
  for (const plugin of plugins) {
    if (seen.has(plugin.id)) throw new Error(`${plugin.id} is listed twice`);
    seen.add(plugin.id);
  }
  return { version: VERSION, plugins };
}

export const installCommand = (entry: Entry): string => `cornercase plugin install ${entry.repo}@${entry.commit}`;

export const sourceUrl = (entry: Entry): string => `https://github.com/${entry.repo}/tree/${entry.commit}`;
