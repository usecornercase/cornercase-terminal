import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { parse } from 'smol-toml';

export const MAX_BYTES = 256 * 1024;
export const MAX_ROLES = 8;
export const MAX_NAME = 80;
export const MAX_DESCRIPTION = 300;
export const MAX_PROMPT = 32 * 1024;
export const MAX_WORD = 60;
export const BUILT_IN_AUTHOR = 'cornercase';
export const BUILT_IN_DIR = 'built-in';
export const PROJECT = 'usecornercase/cornercase-terminal';
export const REPO = /^[\w.-]+\/[\w.-]+$/;
export const TOPIC = 'cornercase-flow';
export const FILES = ['flow.toml', 'cornercase-flow.toml'];
export const VERSION = 2;
export const AGENTS = ['claude', 'codex', 'gemini', 'opencode', 'grok', 'cursor'];

export interface RegistryRole {
  name: string;
  agent: string;
  model?: string;
  effort?: string;
  edits: boolean;
  worktree: 'shared' | 'own';
  prompt: string;
}

export interface RegistryStage {
  roles: string[];
  back?: { to: number; when: string; atMost: number };
}

export interface Snapshot {
  repo: string;
  builtIn?: boolean;
  slug?: string;
  stars: number;
  description: string;
  topics: string[];
  file: string;
  text: string;
}

export interface RegistryFlow {
  builtIn: boolean;
  repo: string;
  owner: string;
  name: string;
  slug: string;
  stars: number;
  description: string;
  tags: string[];
  file: string;
  end: 'pull_request' | 'commits';
  orchestrator: RegistryRole;
  roles: RegistryRole[];
  stages: RegistryStage[];
  text: string;
}

const control = (text: string, allowed = ''): boolean => [...text].some((c) => c < ' ' && !allowed.includes(c));

function role(value: unknown, fallback: string): RegistryRole {
  const r = (value ?? {}) as Record<string, unknown>;
  return {
    name: typeof r.name === 'string' ? r.name : fallback,
    agent: typeof r.agent === 'string' ? r.agent : '',
    model: typeof r.model === 'string' ? r.model : undefined,
    effort: typeof r.effort === 'string' ? r.effort : undefined,
    edits: r.edits === true,
    worktree: r.worktree === 'own' ? 'own' : 'shared',
    prompt: typeof r.prompt === 'string' ? r.prompt : '',
  };
}

const short = (value: unknown, max: number): boolean => value === undefined || (typeof value === 'string' && value.length <= max && !control(value));

export function problems(text: string): string[] {
  if (new TextEncoder().encode(text).length > MAX_BYTES) return [`longer than ${MAX_BYTES / 1024} KiB`];
  let data: Record<string, unknown>;
  try {
    data = parse(text.replace(/^﻿/, '')) as Record<string, unknown>;
  } catch (e) {
    return [(e as Error).message];
  }
  const out: string[] = [];
  const roles = (Array.isArray(data.role) ? data.role : []) as Record<string, unknown>[];
  const stages = (Array.isArray(data.stage) ? data.stage : []) as Record<string, unknown>[];
  const lead = (data.orchestrator ?? {}) as Record<string, unknown>;
  if (typeof data.name !== 'string' || !data.name.trim() || data.name.length > MAX_NAME) out.push('no name, or one that is too long');
  if (!short(data.description, MAX_DESCRIPTION)) out.push('the description is too long or not text');
  if (typeof data.version === 'number' && data.version > VERSION) out.push('made by a newer cornercase');
  if (data.end !== undefined && data.end !== 'pull_request' && data.end !== 'commits') out.push('unknown end');
  if (typeof data.name === 'string' && control(data.name)) out.push('name has a control character');
  if (roles.length === 0 || roles.length > MAX_ROLES) out.push(`needs 1 to ${MAX_ROLES} roles besides the orchestrator`);
  if (stages.length === 0) out.push('no stages');
  const names = new Set<string>();
  for (const [who, r] of [['orchestrator', lead] as const, ...roles.map((r) => [String(r.name ?? ''), r] as const)]) {
    if (!AGENTS.includes(String(r.agent))) out.push(`${who || 'a role'} runs an unknown agent`);
    if (r.env !== undefined || r.args !== undefined) out.push(`${who} sets env or args`);
    if (r.prompt !== undefined && (typeof r.prompt !== 'string' || r.prompt.length > MAX_PROMPT || control(r.prompt, '\n\t\r'))) {
      out.push(`${who} has a prompt that is too long, not text or has a control character`);
    }
    if (!short(r.name, MAX_WORD) || !short(r.model, MAX_WORD) || !short(r.effort, MAX_WORD)) out.push(`${who} has a name, model or effort that is too long or not text`);
    if (r.edits !== undefined && typeof r.edits !== 'boolean') out.push(`${who} has edits that is not true or false`);
    if (r.worktree !== undefined && r.worktree !== 'shared' && r.worktree !== 'own') out.push(`${who} has a worktree that is not shared or own`);
    if (r.worktree === 'own' && who === 'orchestrator') out.push('the orchestrator cannot have a worktree of its own');
    if (who !== 'orchestrator') {
      if (!who.trim() || who === 'orchestrator' || names.has(who)) out.push(`bad or repeated role name ${JSON.stringify(who)}`);
      names.add(who);
    }
  }
  if (roles.some((r) => r.worktree === 'own') && !(typeof data.version === 'number' && data.version >= 2)) out.push('a role has a worktree of its own, which needs version = 2');
  const byName = new Map(roles.map((r) => [String(r.name ?? ''), r] as const));
  const staged = new Set<string>();
  stages.forEach((s, i) => {
    const list = (Array.isArray(s.roles) ? s.roles : []) as unknown[];
    if (!list.every((n) => typeof n === 'string')) {
      out.push(`stage ${i + 1} has a role that is not a name`);
      return;
    }
    if (list.length === 0) out.push(`stage ${i + 1} has no roles`);
    const sharing = (list as string[]).filter((n) => byName.get(n)?.edits === true && byName.get(n)?.worktree !== 'own');
    if (sharing.length > 1) out.push(`stage ${i + 1} runs ${sharing.join(' and ')} side by side, and they both edit the shared worktree`);
    for (const name of list as string[]) {
      if (!names.has(name)) out.push(`stage ${i + 1} names ${JSON.stringify(name)}, which is not a role`);
      else if (staged.has(name)) out.push(`${name} is in two stages`);
      staged.add(name);
    }
    const back = s.back as Record<string, unknown> | undefined;
    if (back && !short(back.when, MAX_WORD)) out.push(`stage ${i + 1} has a loop label that is too long or not text`);
    if (back && !(Number(back.to) >= 1 && Number(back.to) <= i && Number(back.at_most ?? 3) >= 1)) out.push(`stage ${i + 1} goes back to a bad stage`);
  });
  for (const name of names) if (!staged.has(name)) out.push(`${name} is in no stage`);
  return out;
}

export function toFlow(entry: Snapshot): RegistryFlow {
  if (!REPO.test(entry.repo) || entry.repo.split('/').some((part) => part === '.' || part === '..')) throw new Error('not an owner/repo name');
  const found = problems(entry.text);
  if (found.length > 0) throw new Error(found.join('; '));
  const data = parse(entry.text.replace(/^﻿/, '')) as Record<string, unknown>;
  const [owner] = entry.repo.split('/');
  return {
    builtIn: entry.builtIn === true,
    repo: entry.repo,
    owner: entry.builtIn ? BUILT_IN_AUTHOR : owner,
    name: String(data.name).trim(),
    slug: entry.slug ?? entry.repo.toLowerCase(),
    stars: entry.stars,
    description: (typeof data.description === 'string' && data.description) || entry.description,
    tags: entry.topics.filter((t) => t !== TOPIC),
    file: entry.file,
    end: data.end === 'commits' ? 'commits' : 'pull_request',
    orchestrator: role({ ...(data.orchestrator as object) }, 'orchestrator'),
    roles: ((data.role ?? []) as unknown[]).map((r) => role(r, '')),
    stages: ((data.stage ?? []) as Record<string, unknown>[]).map((s) => {
      const back = s.back as Record<string, unknown> | undefined;
      return {
        roles: (s.roles ?? []) as string[],
        back: back ? { to: Number(back.to), when: String(back.when ?? 'problems'), atMost: Number(back.at_most ?? 3) } : undefined,
      };
    }),
    text: entry.text,
  };
}

export const builtInPath = (): string => resolve(process.cwd(), process.env.CORNERCASE_BUILTIN_FLOWS ?? 'src/data/builtin-flows.json');

export function builtIns(file = builtInPath()): RegistryFlow[] {
  const data = JSON.parse(readFileSync(file, 'utf8')) as { flows: { slug: string; file: string; text: string }[] };
  return data.flows.map((f) =>
    toFlow({ repo: PROJECT, builtIn: true, slug: `${BUILT_IN_DIR}/${f.slug}`, stars: 0, description: '', topics: [], file: f.file, text: f.text }),
  );
}

export const dataPath = (): string => resolve(process.cwd(), process.env.CORNERCASE_FLOWS_DATA ?? 'src/data/flows.json');

export function registry(file = dataPath()): RegistryFlow[] {
  const snapshot = JSON.parse(readFileSync(file, 'utf8')) as { flows?: Snapshot[] };
  const flows: RegistryFlow[] = [];
  for (const entry of snapshot.flows ?? []) {
    if (entry.repo.split('/')[0].toLowerCase() === BUILT_IN_DIR) {
      console.warn(`flows: skipped ${entry.repo}: its owner is the name of the built-in flows' folder`);
      continue;
    }
    try {
      flows.push(toFlow(entry));
    } catch (e) {
      console.warn(`flows: skipped ${entry.repo}: ${(e as Error).message}`);
    }
  }
  return flows.sort((a, b) => b.stars - a.stars || a.name.localeCompare(b.name));
}

export const everyFlow = (): RegistryFlow[] => [...builtIns(), ...registry()];

export const endLabel = (end: RegistryFlow['end']): string => (end === 'commits' ? 'leaves the commits' : 'opens a pull request');
export const repoUrl = (repo: string): string => `https://github.com/${repo}`;
export const fileUrl = (flow: RegistryFlow): string => `${repoUrl(flow.repo)}/blob/HEAD/${flow.file}`;
export const BUILT_IN_NOTE = 'It already comes with cornercase, so there is nothing to import: edit it in Settings → Agents → flows.';
export const importCommand = (flow: RegistryFlow): string => `cornercase flow import ${flow.repo}`;
