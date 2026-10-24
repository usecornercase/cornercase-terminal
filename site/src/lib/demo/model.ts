import type { Tree } from './data';
import type { KeysGroup } from './keys';
import type { Context, Place, Shell } from './programs';
import type { Node } from './split';

export interface Pos {
  x: number;
  y: number;
}

export interface Pane {
  id: number;
  shell: Shell;
  rightClicks: boolean;
  agent?: string | null;
  activity?: Activity | null;
  context?: Context | null;
  memory?: number | null;
  subagents?: number;
  unseen?: boolean;
  pending?: boolean;
  since?: number;
  notified?: boolean;
  parent?: number | null;
  collapsed?: boolean;
}

export interface TreeSpot {
  depth: number;
  last: boolean;
  bars: number[];
  fold: boolean | null;
  hidden: unknown[];
}

export const MAX_AGENT_DEPTH = 3;
export const NARROW_AGENT_DEPTH = 1;

export function agentTree<T>(
  rows: T[],
  id: (row: T) => number,
  parent: (row: T) => number | null | undefined,
  cap = MAX_AGENT_DEPTH,
  folded: (row: T) => boolean = () => false,
): (T & TreeSpot)[] {
  const listed = new Set(rows.map(id));
  const children = new Map<number | null, T[]>();
  for (const row of rows) {
    const up = parent(row);
    const key = up !== null && up !== undefined && up !== id(row) && listed.has(up) ? up : null;
    children.set(key, [...(children.get(key) ?? []), row]);
  }
  const out: (T & TreeSpot)[] = [];
  const seen = new Set<number>();
  const hide = (key: number): T[] => {
    const gone: T[] = [];
    const stack = (children.get(key) ?? []).filter((row) => !seen.has(id(row))).reverse();
    for (const row of stack) seen.add(id(row));
    while (stack.length) {
      const row = stack.pop() as T;
      gone.push(row);
      for (const child of (children.get(id(row)) ?? []).filter((kid) => !seen.has(id(kid))).reverse()) {
        seen.add(id(child));
        stack.push(child);
      }
    }
    return gone;
  };
  const place = (row: T, depth: number): void => {
    seen.add(id(row));
    const spot = { ...row, depth, last: true, bars: [] as number[], fold: null as boolean | null, hidden: [] as unknown[] };
    out.push(spot);
    const kids = (children.get(id(row)) ?? []).filter((kid) => !seen.has(id(kid)));
    if (!kids.length) return;
    spot.fold = folded(row);
    if (spot.fold) spot.hidden = hide(id(row));
    else walk(id(row), depth + 1);
  };
  const walk = (key: number | null, depth: number): void => {
    for (const row of (children.get(key) ?? []).filter((kid) => !seen.has(id(kid)))) {
      if (seen.has(id(row))) continue;
      place(row, Math.min(depth, cap));
    }
  };
  walk(null, 0);
  for (const row of rows) {
    if (seen.has(id(row))) continue;
    place(row, 0);
  }
  return shapeTree(out);
}

export function shapeTree<T extends TreeSpot>(rows: T[], cap = MAX_AGENT_DEPTH): T[] {
  const capped = rows.map((row) => ({ ...row, depth: Math.min(row.depth, cap) }));
  let later: boolean[] = [];
  for (let i = capped.length - 1; i >= 0; i--) {
    const row = capped[i];
    const depth = row.depth;
    while (later.length <= depth) later.push(false);
    row.last = !later[depth];
    row.bars = [];
    for (let level = 1; level < depth; level++) if (later[level]) row.bars.push(level);
    later[depth] = true;
    later = later.slice(0, depth + 1);
  }
  return capped;
}

export type Activity = 'working' | 'waiting' | 'idle' | 'ready' | 'done' | 'failed';
export type Status = 'idle' | 'working' | 'done' | 'pending' | 'failed' | 'waiting';
const URGENCY: Status[] = ['idle', 'working', 'done', 'pending', 'failed', 'waiting'];

export interface Tab {
  id: number;
  name?: string;
  layout: Node;
  panes: Pane[];
  active: number;
}

export interface Workspace {
  id: number;
  name?: string;
  branch?: string;
  root: string;
  worktree: boolean;
  tabs: Tab[];
  active: number;
  flags: Place['flags'];
  behind?: number;
  collapsed?: boolean;
  removing?: boolean;
  closed?: Tab[];
}

export interface Group {
  id: number;
  name: string;
  icon: string;
  colour: number;
  collapsed: boolean;
}

export interface Project {
  id: number;
  name?: string;
  group?: number;
  folder: string;
  root: string;
  repo: boolean;
  tree: Tree;
  workspaces: Workspace[];
  active: number;
  collapsed?: boolean;
}

export type Target =
  | { kind: 'group'; group: number }
  | { kind: 'project'; project: number }
  | { kind: 'workspace'; project: number; workspace: number }
  | { kind: 'tab'; project: number; workspace: number; tab: number };

export type PaneAction = 'split right' | 'split down' | 'send right-clicks to the pane' | 'use this menu on right-click' | 'close pane';

export type MenuAction =
  | { kind: 'rename'; target: Target }
  | { kind: 'moveToGroup'; project: number }
  | { kind: 'setGroup'; project: number; group: number | null }
  | { kind: 'groupStyle'; group: number }
  | { kind: 'deleteGroup'; group: number }
  | { kind: 'addProject'; group: number }
  | { kind: 'openProject' }
  | { kind: 'newGroup' }
  | { kind: 'mark'; panes: number[]; pending: boolean }
  | { kind: 'agentFold'; pane: number; collapsed: boolean }
  | { kind: 'closeAgent'; pane: number }
  | { kind: 'closeAgents'; workspace: number }
  | { kind: 'reopenAgents'; workspace: number }
  | { kind: 'forgetAgents'; workspace: number }
  | { kind: 'pane'; pane: number; action: PaneAction }
  | { kind: 'base'; branch: string }
  | { kind: 'showFlow'; workspace: number }
  | { kind: 'newWorkspace'; project: number }
  | { kind: 'startFlow'; project: number }
  | { kind: 'projectSettings'; project: number }
  | { kind: 'openIn' }
  | { kind: 'sendTodo'; todo: number; pane: number }
  | { kind: 'worktrees'; project: number }
  | { kind: 'help'; item: HelpItem };

export type HelpItem = 'tour' | 'catalogue' | 'docs' | 'keys' | 'bug';

export interface PickItem {
  value: string;
  note: string;
  dangerous?: boolean;
}

export interface SettingsOverlay {
  kind: 'settings';
  page: number;
  cursor: number;
  pick?: { row: string; title: string; items: PickItem[]; selected: number; filter: string };
  edit?: { row: string; label: string; input: string; token: boolean; error?: string };
  notice?: string;
  busy?: string;
  capturing?: boolean;
}

export interface IssuesOverlay {
  kind: 'issues';
  project: number;
  tab: number;
  closed: boolean;
  mine: boolean;
  filter: string;
  selected: number;
  scroll: number;
  detail: string | null;
  raw: boolean;
  detailScroll: number;
  loading: boolean;
  busy?: string;
  notice?: string;
  token: { input: string; checking: boolean; error?: string; step?: 'site' | 'email' | 'workspace' | 'server' | 'token' };
  agentPick: { selected: number; filter: string } | null;
  chosen: string | null;
  naming: { issue: string; branch: string; name: string; field: 0 | 1; error?: string } | null;
}

export interface ConfirmView {
  title: string;
  message: string;
  submit: string;
  extra?: string;
  note?: string;
  dangerous?: boolean;
}

export type Overlay =
  | { kind: 'menu'; at: Pos; actions: MenuAction[] }
  | { kind: 'newGroup'; input: string; cursor?: number; anchor?: number | null }
  | { kind: 'groupStyle'; group: number }
  | { kind: 'newWorkspace'; project: number; input: string; cursor?: number; anchor?: number | null; worktree: boolean | null; error?: string; creating?: boolean }
  | { kind: 'rename'; target: Target; input: string; cursor?: number; anchor?: number | null }
  | { kind: 'remove'; project: number; workspace: number }
  | { kind: 'deleteGroup'; group: number }
  | { kind: 'closeProject'; project: number }
  | { kind: 'closeWorkspace'; project: number; workspace: number }
  | { kind: 'closeTab'; project: number; workspace: number; tab: number }
  | { kind: 'forgetAgents'; workspace: number }
  | { kind: 'picker'; dir: string[]; filter: string; selected: number | null; scroll: number; group?: number }
  | { kind: 'search'; query: string; selected: number; scroll: number; mode: 'names' | 'sessions' }
  | { kind: 'usage' }
  | { kind: 'restart'; scroll: number }
  | { kind: 'keys'; group: KeysGroup | null }
  | { kind: 'closePane'; pane: number }
  | { kind: 'discard'; key: string; path: string; status: string; untracked: boolean; old?: string }
  | { kind: 'flowMessage'; workspace: number; edge: string; to: string | null; scroll: number }
  | import('./queue').QueueForm
  | SettingsOverlay
  | IssuesOverlay;

export const REOPEN_ICON = '▶';

export const agentsLabel = (count: number): string => (count === 1 ? '1 agent' : `${count} agents`);

export const isModal = (o: Overlay): boolean => o.kind !== 'menu' && o.kind !== 'search' && o.kind !== 'keys';

export interface Config {
  worktreesDir: string;
  fetchMinutes: number;
  agent: string;
  submit: boolean;
  trust: boolean;
  resume: boolean;
  awake: boolean;
  sidebar: string;
  tabs: string;
  agentsSection: boolean;
  icons: string;
  workingIcon: string;
  counts: boolean;
  todoDot: boolean;
  dim: boolean;
  model: boolean;
  context: boolean;
  memory: boolean;
  notify: string;
  updates: boolean;
  starPrompt: boolean;
  prefix: string;
  agentArgs: Record<string, string[]>;
  sources: string[];
  accounts: { gitlab: boolean; shortcut: boolean; linear: boolean; jira: boolean; plane: boolean; asana: boolean };
  gitlabUrl: string;
  jiraSite: string;
  jiraEmail: string;
  jiraJql: string;
  planeWorkspace: string;
  planeUrl: string;
  planeFilter: string;
  asanaWorkspace: string;
  asanaWorkspaceName: string;
  asanaProjects: string;
}

export const defaultConfig = (): Config => ({
  worktreesDir: '~/.cornercase/worktrees',
  fetchMinutes: 5,
  agent: 'claude',
  submit: false,
  trust: false,
  resume: true,
  awake: false,
  sidebar: 'projects_on_top',
  tabs: 'sidebar',
  agentsSection: false,
  icons: 'auto',
  workingIcon: 'moon',
  counts: true,
  todoDot: true,
  dim: true,
  model: true,
  context: true,
  memory: false,
  notify: 'auto',
  updates: true,
  starPrompt: true,
  prefix: '',
  agentArgs: { claude: ['--permission-mode', 'plan'] },
  sources: ['all', 'github', 'gitlab', 'shortcut', 'linear', 'jira', 'plane', 'asana'],
  accounts: { gitlab: false, shortcut: false, linear: false, jira: false, plane: false, asana: false },
  gitlabUrl: '',
  jiraSite: '',
  jiraEmail: '',
  jiraJql: '',
  planeWorkspace: '',
  planeUrl: '',
  planeFilter: '',
  asanaWorkspace: '',
  asanaWorkspaceName: '',
  asanaProjects: '',
});

export const projectLabel = (p: Project) => p.name || p.folder;
export const workspaceLabel = (w: Workspace) => w.name || w.branch || 'default';

export function activePane(t: Tab): Pane | undefined {
  return t.panes.find((p) => p.id === t.active) ?? t.panes[0];
}

function firstLeaf(node: Node): number {
  return 'leaf' in node ? node.leaf : firstLeaf(node.first);
}

export function firstPane(t: Tab): Pane | undefined {
  const first = firstLeaf(t.layout);
  return t.panes.find((p) => p.id === first) ?? t.panes[0];
}

export const tabLabel = (t: Tab) => t.name || firstPane(t)?.shell.name || 'bash';

export const NOTIFY_AFTER = 1000;

export const NOTIFY_CHOICES: [string, string][] = [
  ['auto', 'what your terminal understands'],
  ['osc777', 'Ghostty, WezTerm, Warp, Rio, foot before 1.18, Konsole before 24.12'],
  ['osc9', 'iTerm2'],
  ['osc99', 'kitty, Contour, VS Code, foot 1.18+, Konsole 24.12+'],
  ['bell', 'a beep or a mark on the window, in any terminal'],
  ['off', 'only the toast and the marks in cornercase'],
];

export function followAgent(pane: Pane, agent: string | null): void {
  if ((pane.agent ?? null) === agent) return;
  pane.agent = agent;
  pane.activity = null;
  pane.unseen = false;
  pane.pending = false;
  pane.since = undefined;
  pane.notified = false;
}

export function watchPane(pane: Pane, activity: Activity | null, seen: boolean, now: number): Status | null {
  const before = paneStatus(pane);
  const finished = pane.activity === 'working' || pane.activity === 'waiting';
  if (activity === 'working' && (pane.activity === 'idle' || pane.activity === 'ready' || pane.activity === 'done' || pane.activity === 'failed' || pane.activity === 'waiting')) pane.pending = false;
  pane.unseen = !seen && (activity === 'idle' ? !!pane.unseen || finished : activity === 'done' && (!!pane.unseen || pane.activity !== activity));
  pane.activity = activity;
  const status = paneStatus(pane);
  if (status !== before) {
    pane.since = now;
    pane.notified = false;
  }
  if (seen) pane.notified = true;
  const settled = pane.since !== undefined && now - pane.since >= NOTIFY_AFTER;
  if (!settled || pane.notified || (status !== 'done' && status !== 'failed' && status !== 'waiting')) return null;
  pane.notified = true;
  return status;
}

export function paneStatus(pane: Pane): Status | null {
  if (!pane.activity) return null;
  if (pane.activity === 'idle' || pane.activity === 'ready' || pane.activity === 'done') return pane.pending ? 'pending' : pane.unseen ? 'done' : 'idle';
  return pane.activity;
}

function mostUrgent(statuses: (Status | null)[]): Status | null {
  let best: Status | null = null;
  for (const s of statuses) if (s && (!best || URGENCY.indexOf(s) > URGENCY.indexOf(best))) best = s;
  return best;
}

export const needsYou = (s: Status | null): boolean => s === 'done' || s === 'pending' || s === 'failed' || s === 'waiting';
export const highest = (statuses: (Status | null)[]): Status | null => mostUrgent(statuses);
export const attention = (statuses: (Status | null)[]): Status | null => mostUrgent(statuses.filter(needsYou));

export function markPane(pane: Pane, pending: boolean): void {
  pane.pending = pending;
  pane.unseen = pending && !!pane.unseen;
  pane.notified = true;
}
export const tabStatus = (t: Tab): Status | null => mostUrgent(t.panes.map(paneStatus));
export const tabRunning = (t: Tab): boolean => t.panes.some((p) => p.shell.busy);
export const tabContext = (t: Tab): Context | null => activePane(t)?.context ?? t.panes.find((p) => p.context)?.context ?? null;
export const tabMemory = (t: Tab): number | null =>
  t.panes.reduce<number | null>((sum, p) => (p.memory == null ? sum : (sum ?? 0) + p.memory), null);
export const tabSubagents = (t: Tab): number => t.panes.reduce((sum, p) => sum + (p.subagents ?? 0), 0);
export const projectAttention = (p: Project): Status | null => attention(p.workspaces.flatMap((w) => w.tabs.map(tabStatus)));
