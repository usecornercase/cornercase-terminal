import type { Index, Star } from '../../../src/lib/plugins/catalogue';
import { type Blocklist, type Found, assemble, scan } from '../../../src/lib/plugins/crawl';

const DAY = 86_400_000;

const manifest = (id: string, name: string, description: string): string => `id = "${id}"
name = "${name}"
version = "1.0.0"
description = "${description}"
permissions = ["read"]

[[actions]]
id = "run"
title = "run ${name}"
contexts = ["pane"]
command = ["./run"]
`;

type Plugin = [id: string, name: string, description: string, path?: string];

type Spec = {
  repo: string;
  stars: number;
  language?: string;
  description: string;
  created: number;
  joined: number;
  pushed: number;
  weekAgo?: number;
  older?: number;
  plugins: Plugin[];
  invalid?: number;
};

export const blocklist: Blocklist = { repos: ['bad/actor'], ids: [] };

const SPECS: Spec[] = [
  { repo: 'acme/notes', stars: 420, language: 'TypeScript', description: 'A notes file per workspace', created: 400, joined: 300, pushed: 2, weekAgo: 400, older: 380, plugins: [['acme.notes', 'Notes', 'Notes per workspace'], ['acme.tasks', 'Tasks', 'A task list in the sidebar', 'tasks']] },
  { repo: 'big/engine', stars: 2000, language: 'Rust', description: 'Plugins for heavy lifting', created: 700, joined: 400, pushed: 1, weekAgo: 1960, plugins: [['big.build', 'Build watcher', 'Rebuild on change'], ['big.deploy', 'Deploy', 'Deploy from a pane', 'deploy'], ['big.logs', 'Logs', 'Tail logs in a tab', 'logs']] },
  { repo: 'fast/risers', stars: 90, language: 'Python', description: 'A young project taking off', created: 50, joined: 20, pushed: 1, weekAgo: 40, plugins: [['fast.rise', 'Riser', 'Climbs the list']] },
  { repo: 'mid/pulse', stars: 300, language: 'Go', description: 'Steady growth', created: 200, joined: 150, pushed: 6, weekAgo: 270, plugins: [['mid.pulse', 'Pulse', 'Heartbeat for agents']] },
  { repo: 'new/alpha', stars: 12, language: 'Lua', description: 'Brand new, joined yesterday', created: 20, joined: 1, pushed: 0, plugins: [['new.alpha', 'Alpha', 'First of the new']] },
  { repo: 'new/beta', stars: 7, language: 'Shell', description: 'Also new', created: 10, joined: 3, pushed: 1, plugins: [['new.beta', 'Beta', 'Second of the new']] },
  { repo: 'new/gamma', stars: 3, description: 'New, no language', created: 40, joined: 9, pushed: 4, plugins: [['new.gamma', 'Gamma', 'Third of the new']] },
  { repo: 'old/found', stars: 55, language: 'Ruby', description: 'Old repository found only this week', created: 200, joined: 5, pushed: 30, plugins: [['old.found', 'Found late', 'Not new: created long ago']] },
  { repo: 'steady/clock', stars: 30, language: 'C++', description: 'Created recently, joined long ago', created: 30, joined: 20, pushed: 12, plugins: [['steady.clock', 'Clock', 'Not new: seen too long ago']] },
  { repo: 'copycat/notes-fork', stars: 5, language: 'TypeScript', description: 'Reuses an id', created: 100, joined: 60, pushed: 50, plugins: [['acme.notes', 'Notes copy', 'Same id as acme/notes']] },
  { repo: 'messy/plugins', stars: 18, language: 'JavaScript', description: 'One good manifest, two broken', created: 300, joined: 250, pushed: 90, invalid: 2, plugins: [['messy.ok', 'Messy', 'The good one']] },
  { repo: 'empty/shell', stars: 2, description: 'Only broken manifests', created: 90, joined: 80, pushed: 80, invalid: 2, plugins: [] },
  { repo: 'bad/actor', stars: 900, language: 'Shell', description: 'Blocked by moderation', created: 150, joined: 100, pushed: 3, plugins: [['bad.actor', 'Bad', 'Never listed']] },
];

const COMMIT = (n: number) => String(n).padStart(2, '0').repeat(20);

export function fixtureInput(now: Date): { found: Found[]; blocklist: Blocklist } {
  const ago = (days: number) => new Date(now.getTime() - days * DAY).toISOString();
  const day = (days: number) => ago(days).slice(0, 10);
  const found = SPECS.map((spec, i): Found => {
    const history: Star[] = [];
    if (spec.older !== undefined) history.push([day(14), spec.older]);
    if (spec.weekAgo !== undefined) history.push([day(7), spec.weekAgo]);
    const texts = spec.plugins.map(([id, name, description, path]) => ({ path: path ?? '', text: manifest(id, name, description) }));
    for (let n = 0; n < (spec.invalid ?? 0); n += 1) texts.push({ path: `broken${n}`, text: 'id =' });
    return {
      repo: { repo: spec.repo, branch: 'main', stars: spec.stars, language: spec.language, description: spec.description, created: ago(spec.created), pushed: ago(spec.pushed), fork: false, archived: false },
      commit: COMMIT(i + 1),
      scan: scan(texts),
      joined: ago(spec.joined),
      history,
    };
  });
  return { found, blocklist };
}

export function fixtureIndex(now: Date): Index {
  const input = fixtureInput(now);
  return assemble(input.found, input.blocklist, now);
}
