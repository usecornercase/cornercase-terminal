import type { Tree } from './data';
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
  unseen?: boolean;
  since?: number;
  notified?: boolean;
}

export type Activity = 'working' | 'waiting' | 'idle';
export type Status = 'idle' | 'working' | 'done' | 'waiting';
const URGENCY: Status[] = ['idle', 'working', 'done', 'waiting'];

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
  | { kind: 'openProject' }
  | { kind: 'newGroup' }
  | { kind: 'pane'; pane: number; action: PaneAction }
  | { kind: 'base'; branch: string };

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
  token: { input: string; checking: boolean; error?: string };
  agentPick: { selected: number; filter: string } | null;
  chosen: string | null;
}

export interface ConfirmView {
  title: string;
  message: string;
  submit: string;
  note?: string;
}

export type Overlay =
  | { kind: 'menu'; at: Pos; actions: MenuAction[] }
  | { kind: 'newGroup'; input: string }
  | { kind: 'groupStyle'; group: number }
  | { kind: 'newWorkspace'; project: number; input: string; worktree: boolean | null; error?: string; creating?: boolean }
  | { kind: 'rename'; target: Target; input: string }
  | { kind: 'remove'; project: number; workspace: number; removing?: boolean }
  | { kind: 'deleteGroup'; group: number }
  | { kind: 'closeProject'; project: number }
  | { kind: 'picker'; dir: string[]; filter: string; selected: number | null; scroll: number }
  | { kind: 'search'; query: string; selected: number; scroll: number }
  | { kind: 'usage' }
  | SettingsOverlay
  | IssuesOverlay;

export interface Config {
  worktreesDir: string;
  fetchMinutes: number;
  agent: string;
  submit: boolean;
  trust: boolean;
  sidebar: string;
  dim: boolean;
  model: boolean;
  context: boolean;
  memory: boolean;
  notify: string;
  updates: boolean;
  agentArgs: Record<string, string[]>;
  sources: string[];
  accounts: { shortcut: boolean; linear: boolean };
}

export const defaultConfig = (): Config => ({
  worktreesDir: '~/.cornercase/worktrees',
  fetchMinutes: 5,
  agent: 'claude',
  submit: false,
  trust: true,
  sidebar: 'side_by_side',
  dim: true,
  model: true,
  context: true,
  memory: false,
  notify: 'auto',
  updates: true,
  agentArgs: { claude: ['--permission-mode', 'plan'] },
  sources: ['all', 'github', 'shortcut', 'linear'],
  accounts: { shortcut: false, linear: false },
});

export const projectLabel = (p: Project) => p.name || p.folder;
export const workspaceLabel = (w: Workspace) => w.name || w.branch || 'default';

export function activePane(t: Tab): Pane | undefined {
  return t.panes.find((p) => p.id === t.active) ?? t.panes[0];
}

export const tabLabel = (t: Tab) => t.name || activePane(t)?.shell.name || 'bash';

export const NOTIFY_AFTER = 1000;

export const NOTIFY_CHOICES: [string, string][] = [
  ['auto', 'what your terminal understands'],
  ['osc777', 'Ghostty, WezTerm, foot, Konsole, Warp, Rio'],
  ['osc9', 'iTerm2'],
  ['osc99', 'kitty, Contour, VS Code'],
  ['bell', 'a beep or a mark on the window, in any terminal'],
  ['off', 'only the toast and the marks in cornercase'],
];

export function followAgent(pane: Pane, agent: string | null): void {
  if ((pane.agent ?? null) === agent) return;
  pane.agent = agent;
  pane.activity = null;
  pane.unseen = false;
  pane.since = undefined;
  pane.notified = false;
}

export function watchPane(pane: Pane, activity: Activity | null, seen: boolean, now: number): Status | null {
  const before = paneStatus(pane);
  const finished = pane.activity === 'working' || pane.activity === 'waiting';
  pane.unseen = activity === 'idle' && !seen && (!!pane.unseen || finished);
  pane.activity = activity;
  const status = paneStatus(pane);
  if (status !== before) {
    pane.since = now;
    pane.notified = false;
  }
  if (seen) pane.notified = true;
  const settled = pane.since !== undefined && now - pane.since >= NOTIFY_AFTER;
  if (!settled || pane.notified || (status !== 'done' && status !== 'waiting')) return null;
  pane.notified = true;
  return status;
}

export function paneStatus(pane: Pane): Status | null {
  if (!pane.activity) return null;
  if (pane.activity === 'idle') return pane.unseen ? 'done' : 'idle';
  return pane.activity;
}

function mostUrgent(statuses: (Status | null)[]): Status | null {
  let best: Status | null = null;
  for (const s of statuses) if (s && (!best || URGENCY.indexOf(s) > URGENCY.indexOf(best))) best = s;
  return best;
}

export const attention = (statuses: (Status | null)[]): Status | null => mostUrgent(statuses.filter((s) => s === 'done' || s === 'waiting'));
export const tabStatus = (t: Tab): Status | null => mostUrgent(t.panes.map(paneStatus));
export const tabContext = (t: Tab): Context | null => activePane(t)?.context ?? t.panes.find((p) => p.context)?.context ?? null;
export const tabMemory = (t: Tab): number | null =>
  t.panes.reduce<number | null>((sum, p) => (p.memory == null ? sum : (sum ?? 0) + p.memory), null);
export const projectAttention = (p: Project): Status | null => attention(p.workspaces.flatMap((w) => w.tabs.map(tabStatus)));
