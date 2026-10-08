import type { Cursor } from '../term/canvas';
import { BOLD, Grid, type Rect, contains } from '../term/grid';
import { AGENTS, FOLDERS, ISSUES, type Issue, MODES, type Tree } from './data';
import { BASES, type ChangesMode, type FileDiff, type HunkAction, hasChanges, workspaceDiff } from './changes';
import { TodoPanel, todoWidth } from './todo';
import { type FileAction, FilesPanel, type FilesPlace, type PathLink, foundRows, lineNear, linkAt, ordered, resolveLink, selectable, selectedText, typing, viewFile, wanted, workspaceFiles } from './files';
import {
  type Border,
  GROUP_STYLES,
  type Landing,
  type Nav,
  type Rows,
  SIDEBARS,
  Strip,
  TABS,
  type Sidebar,
  type SidebarRow,
  type TreeRow,
  type TreeShape,
  type Widths,
  type WorkspaceRow,
  activeRow,
  bottom,
  dragged,
  draggedStacked,
  isEmpty,
  layout,
  mainWidth,
  sidebarDrop,
  sidebarLayout,
  sidebarRows,
  type Details,
  tabLines,
  tabWidth,
  treeActiveRow,
  treeDrop,
  treeLayout,
  treeRows,
  updateNotes,
  workspaceDrop,
  workspaceLayout,
  workspaceRows,
} from './layout';
import { render as markdown } from './markdown';
import {
  type Activity,
  type Config,
  type ConfirmView,
  type Group,
  type IssuesOverlay,
  type MenuAction,
  type Overlay,
  type Pane,
  type PaneAction,
  type PickItem,
  type Pos,
  type Project,
  type SettingsOverlay,
  type Status,
  type Tab,
  type Target,
  type Workspace,
  NOTIFY_AFTER,
  NOTIFY_CHOICES,
  activePane,
  attention,
  defaultConfig,
  paneStatus,
  followAgent,
  projectLabel,
  tabContext,
  tabLabel,
  tabMemory,
  tabStatus,
  watchPane,
  workspaceLabel,
} from './model';
import { AGENT_KINDS, Agent, Editor, type Host, type Key, type Place, Shell } from './programs';
import { type KeysAction, type KeysGroup, type KeysStep, isPrefix, keysItems, keysLookup, prefixClash, prefixOf } from './keys';
import { type Node, type PanePlace, beside, fits, hasRoom, moved, placeArea, placeAt, ratioAt, remove, setRatio, split, visible } from './split';
import { type Line, folderSlug, seg, slug, truncateRight } from './text';
import { type Drag, type Frame, Painter, type Region } from './ui';

export interface SettingsRow {
  id: string;
  section: string;
  label: string;
  value: string;
  note: string;
  dangerous?: boolean;
}

export interface IssuesView {
  tabs: string[];
  toggles: { label: string; on: boolean }[];
  items: { key: string; title: string; meta: string; danger?: boolean }[];
  selected: number;
  scroll: number;
  empty: string;
  hint: string;
  buttons: string[];
  detail?: Line[];
  token?: { label: string; input: string; help: string[] };
  picking: boolean;
  error?: boolean;
}

export interface SearchResult {
  kind: number;
  name: string;
  context: string;
  keys: string[];
  go: () => void;
}

type Listener = (event: string, detail?: string) => void;

const SOURCE_NAMES: Record<string, string> = { all: 'All', github: 'GitHub', shortcut: 'Shortcut', linear: 'Linear', jira: 'Jira' };

type Remote = 'shortcut' | 'linear' | 'jira';
const REMOTES: Record<Remote, { name: string; token: string; env: string; help: string }> = {
  shortcut: {
    name: 'Shortcut',
    token: 'API token',
    env: 'SHORTCUT_API_TOKEN',
    help: 'Create a token in Shortcut under Settings → Your account → API Tokens, paste it here and press Enter.',
  },
  linear: {
    name: 'Linear',
    token: 'API key',
    env: 'LINEAR_API_KEY',
    help: 'Create a personal API key in Linear under Settings → Security & access → Personal API keys, paste it here and press Enter.',
  },
  jira: {
    name: 'Jira',
    token: 'API token',
    env: 'JIRA_API_TOKEN',
    help: 'Create an API token at id.atlassian.com under Security → Create and manage API tokens, paste it here and press Enter.',
  },
};
const isRemote = (s: string): s is Remote => Object.hasOwn(REMOTES, s);
const tokenName = (s: Remote) => `${REMOTES[s].name} ${REMOTES[s].token}`;

function checkSite(input: string): [string, string?] {
  const host = (input.trim().split('://').pop() ?? '').split('/')[0].replace(/\.+$/, '').toLowerCase();
  if (!host) return ['', 'type your site, such as acme.atlassian.net'];
  if (!/^[a-z0-9.-]+$/.test(host)) return ['', 'a site is a host name, such as acme.atlassian.net'];
  return [host.includes('.') ? host : `${host}.atlassian.net`];
}

function checkEmail(input: string): [string, string?] {
  const email = input.trim();
  const at = email.indexOf('@');
  return at > 0 && at < email.length - 1 && !/\s/.test(email) ? [email] : ['', 'type the email of your Atlassian account'];
}
const DOUBLE_CLICK = 400;
const AUTO_SCROLL_EVERY = 150;

interface Focus {
  project: number | null;
  workspace: number | null;
  tab: number | null;
  tree: boolean;
}

export interface AgentRow {
  pane: number;
  status: Status;
  agent: string;
  project: string;
  workspace: string | null;
  details: Details;
  active: boolean;
}

export type RowDragView =
  | { list: 'sidebar'; row: SidebarRow; landing: Landing | null }
  | { list: 'workspaces'; row: WorkspaceRow; landing: Landing | null }
  | { list: 'tree'; row: TreeRow; landing: Landing | null }
  | { list: 'bar'; row: TreeRow; landing: Landing | null };

function moveBefore<T>(items: T[], from: number, before: number, active: number): number {
  const to = before > from ? before - 1 : before;
  if (from < 0 || from >= items.length || to >= items.length || to === from) return active;
  items.splice(to, 0, ...items.splice(from, 1));
  if (active === from) return to;
  if (from < active && active <= to) return active - 1;
  if (to <= active && active < from) return active + 1;
  return active;
}

const RENAME: Record<Target['kind'], { label: string; hint: string }> = {
  group: { label: 'rename group', hint: 'leave it empty to keep the current name' },
  project: { label: 'rename project', hint: 'leave it empty to use the folder name' },
  workspace: { label: 'rename workspace', hint: 'leave it empty to use the branch name' },
  tab: { label: 'rename tab', hint: 'leave it empty to use the program name' },
};

const WATCHED = ['claude', 'codex'];
const DETAILS = [
  ['model', "under an agent's tab, such as Opus 5.5"],
  ['context', 'how full its context is, such as 23%'],
  ['memory', 'the RAM its processes use, such as 1.2 GB'],
] as const;
const DETAIL_NOTICES = {
  model: ['agent tabs show their model', 'agent tabs hide their model'],
  context: ['agent tabs show how full their context is', 'agent tabs hide their context'],
  memory: ['agent tabs show the memory they use', 'agent tabs hide their memory'],
} as const;

const RESTART_STOPS = "Restarting stops every program running in cornercase's terminals.";
const RESTART_NOTHING = 'Nothing is running in your terminals.';
const RESTART_COMES_BACK = 'Your projects, workspaces, tabs and splits come back, each tab with a new shell in its folder.';
const RESTART_RESUME = 'To pick up a Claude Code conversation afterwards, run `claude --continue` in its tab.';
const RUNNING_STATUSES: [Status, string][] = [
  ['working', 'working'],
  ['waiting', 'waiting for you'],
  ['done', 'done'],
  ['idle', 'idle'],
];

interface Running {
  program: string;
  place: string;
  agent: string | null;
  status: Status;
  resumes: boolean;
}

const counted = (n: number, word: string): string => (n === 1 ? `1 ${word}` : `${n} ${word}s`);

function restartText(running: Running[]): string {
  if (!running.length) return `${RESTART_NOTHING} ${RESTART_COMES_BACK}`;
  const agents = running.filter((r) => r.agent);
  const others = running.filter((r) => !r.agent);
  const parts: string[] = [];
  if (agents.length) {
    const statuses = RUNNING_STATUSES.flatMap(([status, said]) => {
      const n = agents.filter((a) => a.status === status).length;
      return n ? [`${n} ${said}`] : [];
    });
    parts.push(`${counted(agents.length, 'agent')} (${statuses.join(', ')})`);
  }
  if (others.length) {
    const named = others.map((r) => `\`${r.program}\` in ${r.place}`).join(', ');
    parts.push(`${counted(others.length, agents.length ? 'other program' : 'program')} (${named})`);
  }
  const text = `${RESTART_STOPS} Running now: ${parts.join(' and ')}. ${RESTART_COMES_BACK}`;
  const resumed = running.filter((r) => r.resumes).length;
  if (resumed === 1) return `${text} 1 agent conversation resumes in its tab.`;
  if (resumed > 1) return `${text} ${resumed} agent conversations resume in their tabs.`;
  return running.some((r) => r.agent === 'claude') ? `${text} ${RESTART_RESUME}` : text;
}

function stoppedTabs(tabs: number): string {
  if (tabs === 1) return ' Its tab and the programs running in it are stopped.';
  if (tabs > 1) return ` Its ${tabs} tabs and the programs running in them are stopped.`;
  return '';
}

function agentIn(pane: Pane): Agent | null {
  const fg = pane.shell.fg;
  return fg instanceof Agent && WATCHED.includes(fg.name) ? fg : null;
}

function agentActivity(agent: Agent | null): Activity | null {
  if (!agent) return null;
  if (agent.waiting) return 'waiting';
  return agent.working ? 'working' : 'idle';
}

export class App {
  cols = 120;
  rows = 34;
  widths: Widths = { projects: 32, workspaces: 26 };
  changesOpen = false;
  changesMode: ChangesMode = 'uncommitted';
  changesBase = BASES[0];
  changesScroll = 0;
  changesFilter: { query: string; focused: boolean } | null = null;
  todo = new TodoPanel([
    ['fix the 500 on /returns when the address has no second line', false],
    ['reply to the design review', false],
    ['bump the returns API client', true],
  ]);
  files = new FilesPanel();
  private linkPress: { at: Pos; link: PathLink; held: (() => void) | null } | null = null;
  private folded = new Map<string, boolean>();
  private viewed = new Set<string>();
  nav: Nav = null;
  groups: Group[] = [];
  projects: Project[] = [];
  active = 0;
  projectsScroll = 0;
  workspacesScroll = 0;
  tabBarScroll = 0;
  agentsScroll = 0;
  overlay: Overlay | null = null;
  hover: Pos | null = null;
  toast: { text: string; until: number; status?: Status; undo?: () => void; bug?: boolean } | null = null;
  focused = false;
  selection: { pane: number; from: Pos; to: Pos; rect: Rect } | null = null;
  dragging: Drag | null = null;
  rowDrag: { target: Target | null; row: Rect; moved: boolean; click?: () => void; scrolled: number } | null = null;
  paneDrag: { tab: Tab; pane: Pane; from: Pos; rect: Rect; alone: boolean; moved: boolean } | null = null;
  detached = false;
  scripted = false;
  agentPace = 900;
  outerLines: Line[] = [];
  outerInput = '';
  config: Config = defaultConfig();
  usage: { loading: boolean; at: number | null } = { loading: false, at: null };
  private ids = 1;
  private timers = new Set<ReturnType<typeof setTimeout>>();
  private listeners: Listener[] = [];
  private frame: Frame | null = null;
  private grid = new Grid(this.cols, this.rows);
  private lastClick: { at: number; border: string } | null = null;
  private followed: Focus = { project: null, workspace: null, tab: null, tree: false };
  private needsDraw = true;

  on(listener: Listener): void {
    this.listeners.push(listener);
  }

  emit(event: string, detail?: string): void {
    for (const l of this.listeners) l(event, detail);
  }

  id(): number {
    return this.ids++;
  }

  dirty(): void {
    this.needsDraw = true;
    this.emit('dirty');
  }

  virtual = false;
  light = false;
  private clockNow = 0;
  private queue: { at: number; fn: () => void; seq: number }[] = [];
  private seq = 0;

  now(): number {
    return this.virtual ? this.clockNow : Date.now();
  }

  advance(ms: number): void {
    const end = this.clockNow + ms;
    for (;;) {
      this.queue.sort((a, b) => a.at - b.at || a.seq - b.seq);
      const next = this.queue[0];
      if (!next || next.at > end) break;
      this.queue.shift();
      this.clockNow = next.at;
      next.fn();
    }
    this.clockNow = end;
  }

  after(ms: number, fn: () => void): () => void {
    if (this.virtual) {
      const item = { at: this.clockNow + ms, fn, seq: this.seq++ };
      this.queue.push(item);
      return () => {
        this.queue = this.queue.filter((q) => q !== item);
      };
    }
    const t = setTimeout(() => {
      this.timers.delete(t);
      fn();
    }, ms);
    this.timers.add(t);
    return () => {
      clearTimeout(t);
      this.timers.delete(t);
    };
  }

  destroy(): void {
    for (const t of this.timers) clearTimeout(t);
    this.timers.clear();
    this.queue = [];
  }

  resize(cols: number, rows: number): void {
    if (cols === this.cols && rows === this.rows) return;
    this.cols = cols;
    this.rows = rows;
    this.grid = new Grid(cols, rows);
    if (cols >= 90) this.nav = null;
    this.dirty();
  }

  render(): { grid: Grid; cursor: Cursor | null } {
    if (this.toast && this.toast.until < this.now()) this.toast = null;
    this.watchAgents();
    this.grid.reset();
    this.follow();
    this.frame = new Painter(this, this.grid).draw();
    this.needsDraw = false;
    return { grid: this.grid, cursor: this.frame.cursor };
  }

  get stale(): boolean {
    return this.needsDraw;
  }

  private visibleTab(): Tab | undefined {
    return this.nav ? undefined : this.tab();
  }

  private watchAgents(): void {
    const visible = this.visibleTab();
    const tree = this.areas().tree;
    const shown = (p: Project) => (tree ? !p.collapsed && !this.group(p.group)?.collapsed : p === this.project());
    const now = this.now();
    for (const p of this.projects) {
      for (const w of p.workspaces) {
        const measured = this.config.memory && shown(p) && !(tree && w.collapsed);
        for (const t of w.tabs) {
          for (const pane of t.panes) {
            const agent = agentIn(pane);
            const fg = pane.shell.fg;
            pane.context = fg instanceof Agent ? fg.context : null;
            if (!agent) pane.memory = null;
            else if (measured) pane.memory = agent.memory;
            followAgent(pane, agent?.name ?? null);
            const status = watchPane(pane, agentActivity(agent), t === visible, now);
            if (status && agent) this.notify(`${agent.name} ${status === 'waiting' ? 'needs you' : 'finished'} in ${projectLabel(p)} › ${workspaceLabel(w)}`, status);
            else if (pane.since === now && !pane.notified) this.after(NOTIFY_AFTER, () => this.dirty());
          }
        }
      }
    }
  }

  attentionElsewhere(): Status | null {
    const visible = this.visibleTab();
    const tabs = this.projects.flatMap((p) => p.workspaces.flatMap((w) => w.tabs));
    return attention(tabs.filter((t) => t !== visible).map(tabStatus));
  }

  project(): Project | undefined {
    return this.projects[this.active];
  }

  workspace(): Workspace | undefined {
    const p = this.project();
    return p?.workspaces[p.active];
  }

  tab(): Tab | undefined {
    const w = this.workspace();
    return w?.tabs[w.active];
  }

  place(p: Project, w: Workspace): Place {
    return { project: p.folder, root: w.root, branch: w.branch, repo: p.repo, tree: p.tree, flags: w.flags };
  }

  host(p: Project, w: Workspace, paneId: () => number): Host {
    return {
      after: (ms, fn) => this.after(ms, fn),
      dirty: () => this.dirty(),
      exit: () => this.exitPane(paneId()),
      place: () => this.place(p, w),
      pace: () => this.agentPace,
    };
  }

  newPane(p: Project, w: Workspace, lines: Line[] = []): Pane {
    const pane = { id: this.id(), rightClicks: false } as Pane;
    pane.shell = new Shell(this.host(p, w, () => pane.id), lines);
    return pane;
  }

  newTab(panesList: Pane[], layoutNode?: Node): Tab {
    return { id: this.id(), layout: layoutNode ?? { leaf: panesList[0].id }, panes: panesList, active: panesList[0].id };
  }

  addProject(folder: string, root: string, repo: boolean, tree: Tree, branch?: string): Project {
    const p: Project = { id: this.id(), folder, root, repo, tree, workspaces: [], active: 0 };
    const w: Workspace = { id: this.id(), branch: repo ? branch ?? 'main' : undefined, root, worktree: false, tabs: [], active: 0, flags: {} };
    p.workspaces.push(w);
    w.tabs.push(this.newTab([this.newPane(p, w)]));
    this.projects.push(p);
    return p;
  }

  addWorkspace(p: Project, branch: string, worktree: boolean, name?: string): Workspace {
    const root = worktree ? `${this.config.worktreesDir}/${p.folder}/${folderSlug(branch)}` : p.root;
    const w: Workspace = { id: this.id(), name, branch, root, worktree, tabs: [], active: 0, flags: {} };
    p.workspaces.push(w);
    return w;
  }

  findPane(id: number): { p: Project; w: Workspace; t: Tab; pane: Pane } | null {
    for (const p of this.projects) {
      for (const w of p.workspaces) {
        for (const t of w.tabs) {
          const pane = t.panes.find((x) => x.id === id);
          if (pane) return { p, w, t, pane };
        }
      }
    }
    return null;
  }

  shells(): number {
    let n = 0;
    for (const p of this.projects) for (const w of p.workspaces) for (const t of w.tabs) n += t.panes.length;
    return n;
  }

  notify(text: string, status?: Status): void {
    this.toast = { text, until: this.now() + 2000, status };
    this.after(2050, () => this.dirty());
    this.dirty();
  }

  group(id: number | undefined): Group | undefined {
    return this.groups.find((g) => g.id === id);
  }

  groupIndex(id: number | undefined): number | null {
    const g = this.groups.findIndex((x) => x.id === id);
    return g >= 0 ? g : null;
  }

  sidebarRows(): SidebarRow[] {
    return sidebarRows(
      this.projects.map((p) => this.groupIndex(p.group)),
      this.groups.map((g) => g.collapsed),
    );
  }

  tabLines(): number[][] {
    return this.project()?.workspaces.map((w) => this.workspaceLines(w)) ?? [];
  }

  private workspaceLines(w: Workspace): number[] {
    return w.tabs.map((t) => tabLines(this.tabDetails(t)));
  }

  treeShape(): TreeShape {
    return {
      tabBar: !this.tabsListed(),
      groups: this.groups.map((g) => g.collapsed),
      projects: this.projects.map((p) => ({
        group: this.groupIndex(p.group),
        collapsed: !!p.collapsed,
        workspaces: p.workspaces.map((w) => ({ collapsed: !!w.collapsed, tabs: p.collapsed || w.collapsed ? [] : this.workspaceLines(w) })),
      })),
    };
  }

  private treeRowOf(t: Target): TreeRow | null {
    if (t.kind === 'group') {
      const g = this.groupIndex(t.group);
      return g === null ? null : { kind: 'group', g };
    }
    const p = this.projects.findIndex((x) => x.id === t.project);
    if (p < 0) return null;
    if (t.kind === 'project') return { kind: 'project', p };
    const w = this.projects[p].workspaces.findIndex((x) => x.id === t.workspace);
    if (w < 0) return null;
    if (t.kind === 'workspace') return { kind: 'ws', p, w };
    const tab = this.projects[p].workspaces[w].tabs.findIndex((x) => x.id === t.tab);
    return tab < 0 ? null : { kind: 'tab', p, w, t: tab };
  }

  agentRows(): AgentRow[] {
    const order = sidebarRows(
      this.projects.map((p) => this.groupIndex(p.group)),
      this.groups.map(() => false),
    );
    const c = this.config;
    const focused = this.tab();
    return order.flatMap((row) => {
      if (row.kind !== 'project') return [];
      const p = this.projects[row.p];
      return p.workspaces.flatMap((w) =>
        w.tabs.flatMap((t) =>
          t.panes.flatMap((pane): AgentRow[] => {
            const status = paneStatus(pane);
            if (!status) return [];
            return [
              {
                pane: pane.id,
                status,
                agent: pane.agent ?? '',
                project: projectLabel(p),
                workspace: p.workspaces.length > 1 ? workspaceLabel(w) : null,
                details: { model: c.model ? (pane.context?.model ?? null) : null, percent: c.context ? (pane.context?.percent ?? null) : null, memory: null },
                active: t === focused && t.active === pane.id,
              },
            ];
          }),
        ),
      );
    });
  }

  jumpToPane(id: number): void {
    const found = this.findPane(id);
    if (!found) return;
    const p = this.projects.indexOf(found.p);
    this.goto(p, found.p.workspaces.indexOf(found.w), found.w.tabs.indexOf(found.t));
    found.t.active = id;
    this.selection = null;
    this.emit('select', 'tab');
    this.dirty();
  }

  private agentsDragged(y: number): Widths {
    const end = bottom(this.areas().agents);
    const wanted = { ...this.widths, agents: Math.max(0, end - (y + 1)) };
    const rows = layout(this.cols, this.rows, wanted, this.nav, this.panelShown(), this.sidebar(), true, this.tabsOnTop()).agents.h;
    return { ...wanted, agents: rows };
  }

  tabDetails(t: Tab): Details {
    const c = this.config;
    const context = tabContext(t);
    return {
      model: c.model ? (context?.model ?? null) : null,
      percent: c.context ? (context?.percent ?? null) : null,
      memory: c.memory ? tabMemory(t) : null,
    };
  }

  areas() {
    return layout(this.cols, this.rows, this.widths, this.nav, this.panelShown(), this.sidebar(), this.config.agentsSection, this.tabsOnTop());
  }

  tabsOnTop(): boolean {
    return this.config.tabs === 'top';
  }

  tabsListed(): boolean {
    return isEmpty(this.areas().tabBar);
  }

  tabStrip(bar: Rect): Strip {
    const tabs = this.workspace()?.tabs ?? [];
    return new Strip(bar, tabs.map((t) => tabWidth(tabLabel(t), !!tabStatus(t), t.panes.length - 1)), this.tabBarScroll);
  }

  barDetails(): Details {
    const tab = this.tab();
    const pane = tab?.panes.find((p) => p.id === tab.active);
    const c = this.config;
    return {
      model: c.model ? (pane?.context?.model ?? null) : null,
      percent: c.context ? (pane?.context?.percent ?? null) : null,
      memory: c.memory ? (pane?.memory ?? null) : null,
    };
  }

  scrollTabBar(bar: Rect, dy: number): boolean {
    this.tabBarScroll = this.tabStrip(bar).scrolled(Math.sign(dy));
    this.dirty();
    return true;
  }

  rowDragView(): RowDragView | null {
    const d = this.rowDrag;
    const h = this.hover;
    if (!d?.moved || !h || !d.target) return null;
    const areas = this.areas();
    const row = this.treeRowOf(d.target);
    if (!row) return null;
    if (row.kind === 'tab' && !isEmpty(areas.tabBar)) {
      if (row.p !== this.active || row.w !== this.projects[row.p].active) return null;
      const before = this.tabStrip(areas.tabBar).drop(row.t, h.x, h.y);
      return { list: 'bar', row, landing: before === null ? null : { at: before, spot: { kind: 'tab', before } } };
    }
    if (areas.tree) return { list: 'tree', row, landing: treeDrop(areas.list, this.treeShape(), this.projectsScroll, row, h.x, h.y) };
    if (row.kind === 'group' || row.kind === 'project') {
      return { list: 'sidebar', row, landing: sidebarDrop(areas.list, areas.pitch, this.sidebarRows(), this.projectsScroll, row, h.x, h.y) };
    }
    if ((row.kind !== 'ws' && row.kind !== 'tab') || row.p !== this.active) return null;
    const listed: WorkspaceRow = row.kind === 'ws' ? { kind: 'ws', w: row.w } : { kind: 'tab', w: row.w, t: row.t };
    const landing = workspaceDrop(areas.workspacesList, areas.pitch, this.tabLines(), this.workspacesScroll, listed, h.x, h.y, this.tabsListed());
    return { list: 'workspaces', row: listed, landing };
  }

  private dropRow(target: Target): void {
    const spot = this.rowDragView()?.landing?.spot;
    const row = this.treeRowOf(target);
    if (!spot || !row) return;
    if (row.kind === 'group' && spot.kind === 'group') moveBefore(this.groups, row.g, spot.before, 0);
    else if (row.kind === 'project' && spot.kind === 'project') this.moveProject(this.projects[row.p].id, spot.group, spot.before);
    else if (row.kind === 'ws' && spot.kind === 'workspace') {
      const p = this.projects[row.p];
      p.active = moveBefore(p.workspaces, row.w, spot.before, p.active);
    } else if (row.kind === 'tab' && spot.kind === 'tab') {
      const w = this.projects[row.p].workspaces[row.w];
      w.active = moveBefore(w.tabs, row.t, spot.before, w.active);
    }
  }

  private moveProject(id: number, group: number | null, before: number | null): void {
    const groupId = group === null ? undefined : this.groups[group]?.id;
    const from = this.projects.findIndex((x) => x.id === id);
    if ((group !== null && groupId === undefined) || from < 0) return;
    const beforeId = before === null ? undefined : this.projects[before]?.id;
    const activeId = this.project()?.id;
    const [project] = this.projects.splice(from, 1);
    project.group = groupId;
    const at = this.projects.findIndex((x) => x.id === beforeId);
    const last = this.projects.map((x) => x.group).lastIndexOf(groupId);
    this.projects.splice(at >= 0 ? at : last >= 0 ? last + 1 : this.projects.length, 0, project);
    this.active = Math.max(0, this.projects.findIndex((x) => x.id === activeId));
  }

  private autoScroll(): void {
    const d = this.rowDrag;
    const h = this.hover;
    if (!d?.moved || !d.target || !h || this.now() - d.scrolled < AUTO_SCROLL_EVERY) return;
    const areas = this.areas();
    if (d.target.kind === 'tab' && !isEmpty(areas.tabBar)) {
      const strip = this.tabStrip(areas.tabBar);
      const delta = strip.edge(h.x, h.y);
      if (!delta) return;
      this.tabBarScroll = strip.scrolled(delta);
      d.scrolled = this.now();
      this.dirty();
      this.after(AUTO_SCROLL_EVERY, () => this.autoScroll());
      return;
    }
    const shape = this.treeShape();
    const sidebar = areas.tree || d.target.kind === 'group' || d.target.kind === 'project';
    const rows = areas.tree
      ? treeLayout(areas.list, shape, treeRows(shape), this.projectsScroll)
      : sidebar
        ? sidebarLayout(areas.list, areas.pitch, this.sidebarRows(), this.projectsScroll)
        : workspaceLayout(areas.workspacesList, areas.pitch, workspaceRows(this.tabLines(), this.tabsListed()), this.tabLines(), this.workspacesScroll);
    const delta = rows.edge(h.x, h.y);
    if (!delta) return;
    if (sidebar) this.projectsScroll = rows.scrolled(delta);
    else this.workspacesScroll = rows.scrolled(delta);
    d.scrolled = this.now();
    this.dirty();
    this.after(AUTO_SCROLL_EVERY, () => this.autoScroll());
  }

  private follow(): void {
    const p = this.project();
    const { tabBar } = this.areas();
    const { list, pitch, tree } = layout(this.cols, this.rows, this.widths, 'projects', false, this.sidebar(), this.config.agentsSection, this.tabsOnTop());
    const focus: Focus = { project: p?.id ?? null, workspace: this.workspace()?.id ?? null, tab: this.tab()?.id ?? null, tree };
    const before = this.followed;
    if (focus.project === before.project && focus.workspace === before.workspace && focus.tab === before.tab && tree === before.tree) return;
    this.followed = focus;
    if (before.project !== null && (focus.project !== before.project || focus.workspace !== before.workspace)) this.unfoldFocus();
    this.revealInTabBar(tabBar, focus.workspace !== before.workspace);
    if (tree) return this.revealInTree(list);
    if (!p || (focus.project === before.project && !before.tree)) return;
    const sidebar = this.sidebarRows();
    const i = activeRow(sidebar, this.active, this.groupIndex(p.group));
    if (i >= 0) this.projectsScroll = sidebarLayout(list, pitch, sidebar, this.projectsScroll).reveal(i);
  }

  private revealInTabBar(bar: Rect, moved: boolean): void {
    if (moved) this.tabBarScroll = 0;
    const w = this.workspace();
    if (isEmpty(bar) || !w?.tabs.length) return;
    this.tabBarScroll = this.tabStrip(bar).reveal(Math.max(0, w.tabs.indexOf(this.tab()!)));
  }

  private revealInTree(list: Rect): void {
    const p = this.project();
    if (!p) return;
    const w = p.workspaces[p.active];
    const shape = this.treeShape();
    const rows = treeRows(shape);
    const shown = [rows.findIndex((r) => r.kind === 'project' && r.p === this.active), treeActiveRow(rows, shape, this.active, p.active, w?.tabs.length ? w.active : null)];
    for (const i of shown) if (i >= 0) this.projectsScroll = treeLayout(list, shape, rows, this.projectsScroll).reveal(i);
  }

  private unfoldFocus(): void {
    const p = this.project();
    if (!p) return;
    p.collapsed = false;
    const w = p.workspaces[p.active];
    if (w) w.collapsed = false;
    const g = this.group(p.group);
    if (this.followed.tree && g) g.collapsed = false;
  }

  toggleFold(p: number, w?: number): void {
    const project = this.projects[p];
    const folded = w === undefined ? project : project?.workspaces[w];
    if (!folded) return;
    folded.collapsed = !folded.collapsed;
    this.dirty();
  }

  toggleGroup(g: number): void {
    const group = this.groups[g];
    if (!group) return;
    group.collapsed = !group.collapsed;
    this.dirty();
  }

  groupSize(id: number): number {
    return this.projects.filter((p) => p.group === id).length;
  }

  askDeleteGroup(id: number): void {
    this.overlay = { kind: 'deleteGroup', group: id };
    this.dirty();
  }

  deleteGroupMessage(id: number): string {
    const inside = this.groupSize(id);
    const message = `Delete the group ${this.group(id)?.name ?? ''}?`;
    if (inside === 0) return message;
    if (inside === 1) return `${message} Its project stays open, ungrouped.`;
    return `${message} Its ${inside} projects stay open, ungrouped.`;
  }

  askCloseProject(id: number): void {
    this.overlay = { kind: 'closeProject', project: id };
    this.dirty();
  }

  closeProjectMessage(id: number): string {
    const p = this.projects.find((x) => x.id === id);
    if (!p) return '';
    const stopped = stoppedTabs(p.workspaces.reduce((n, w) => n + w.tabs.length, 0));
    return `Close the project ${projectLabel(p)}?${stopped} Folders and worktrees stay on disk.`;
  }

  askCloseWorkspace(p: number, w: number): void {
    const project = this.projects[p];
    const ws = project?.workspaces[w];
    if (!project || !ws) return;
    if (ws.worktree) return this.closeWorkspace(p, w);
    this.overlay = { kind: 'closeWorkspace', project: project.id, workspace: ws.id };
    this.dirty();
  }

  askCloseTab(p: number, w: number, t: number): void {
    const project = this.projects[p];
    const ws = project?.workspaces[w];
    const tab = ws?.tabs[t];
    if (!project || !ws || !tab) return;
    this.overlay = { kind: 'closeTab', project: project.id, workspace: ws.id, tab: tab.id };
    this.dirty();
  }

  private closeTarget(project: number, workspace: number, tab?: number): { p: number; w: number; t: number } | null {
    const p = this.projects.findIndex((x) => x.id === project);
    const w = this.projects[p]?.workspaces.findIndex((x) => x.id === workspace) ?? -1;
    const t = tab === undefined ? 0 : (this.projects[p]?.workspaces[w]?.tabs.findIndex((x) => x.id === tab) ?? -1);
    return p < 0 || w < 0 || t < 0 ? null : { p, w, t };
  }

  closeWorkspaceMessage(project: number, workspace: number): string {
    const at = this.closeTarget(project, workspace);
    const ws = at && this.projects[at.p].workspaces[at.w];
    if (!ws) return '';
    return `Close the workspace ${workspaceLabel(ws)}?${stoppedTabs(ws.tabs.length)}`;
  }

  closeTabMessage(project: number, workspace: number, tab: number): string {
    const at = this.closeTarget(project, workspace, tab);
    const t = at && this.projects[at.p].workspaces[at.w].tabs[at.t];
    if (!t) return '';
    return `Close the tab ${tabLabel(t)}? The programs running in it are stopped.`;
  }

  confirmView(): ConfirmView | null {
    const o = this.overlay;
    if (o?.kind === 'remove') return { title: 'remove workspace', message: this.removeMessage(o), submit: 'remove' };
    if (o?.kind === 'deleteGroup') return { title: 'delete group', message: this.deleteGroupMessage(o.group), submit: 'delete' };
    if (o?.kind === 'closeProject') return { title: 'close project', message: this.closeProjectMessage(o.project), submit: 'close' };
    if (o?.kind === 'closeWorkspace') return { title: 'close workspace', message: this.closeWorkspaceMessage(o.project, o.workspace), submit: 'close' };
    if (o?.kind === 'closeTab') return { title: 'close tab', message: this.closeTabMessage(o.project, o.workspace, o.tab), submit: 'close' };
    if (o?.kind === 'closePane') return { title: 'close pane', message: this.closePaneMessage(o.pane), submit: 'close' };
    return null;
  }

  submitConfirm(): void {
    const o = this.overlay;
    if (o?.kind === 'remove') return this.submitRemove();
    this.closeOverlay();
    if (o?.kind === 'closeProject') this.closeProject(o.project);
    else if (o?.kind === 'closeWorkspace') {
      const at = this.closeTarget(o.project, o.workspace);
      if (at) this.closeWorkspace(at.p, at.w);
    } else if (o?.kind === 'closeTab') {
      const at = this.closeTarget(o.project, o.workspace, o.tab);
      if (at) this.closeTab(at.p, at.w, at.t);
    } else if (o?.kind === 'deleteGroup') this.deleteGroup(o.group);
    else if (o?.kind === 'closePane') this.paneAction(o.pane, 'close pane');
  }

  closePaneMessage(id: number): string {
    const pane = this.findPane(id)?.pane;
    return pane ? `Close the pane running ${pane.shell.name || 'bash'}? What runs in it is stopped.` : '';
  }

  deleteGroup(id: number): void {
    this.groups = this.groups.filter((g) => g.id !== id);
    for (const p of this.projects) if (p.group === id) p.group = undefined;
  }

  addGroup(name: string): Group {
    const taken = ([icon, colour]: [string, number]) => this.groups.some((g) => g.icon === icon && g.colour === colour);
    const [icon, colour] = GROUP_STYLES.find((style) => !taken(style)) ?? GROUP_STYLES[this.groups.length % GROUP_STYLES.length];
    const group = { id: this.id(), name, icon, colour, collapsed: false };
    this.groups.push(group);
    return group;
  }

  setGroupStyle(icon?: string, colour?: number): void {
    const o = this.overlay;
    if (o?.kind !== 'groupStyle') return;
    const g = this.group(o.group);
    if (!g) return this.closeOverlay();
    if (icon !== undefined) g.icon = icon;
    if (colour !== undefined) g.colour = colour;
    this.dirty();
  }

  openNewMenu(at: Pos): void {
    this.overlay = { kind: 'menu', at, actions: [{ kind: 'openProject' }, { kind: 'newGroup' }] };
    this.dirty();
  }

  openGroupMenu(at: Pos, g: number): void {
    const id = this.groups[g]?.id;
    if (id === undefined) return;
    this.overlay = {
      kind: 'menu',
      at,
      actions: [
        { kind: 'addProject', group: id },
        { kind: 'rename', target: { kind: 'group', group: id } },
        { kind: 'groupStyle', group: id },
        { kind: 'deleteGroup', group: id },
      ],
    };
    this.dirty();
  }

  selectProject(i: number): void {
    this.active = i;
    this.unfoldFocus();
    if (this.nav) this.nav = 'workspaces';
    this.workspacesScroll = 0;
    this.emit('select', 'project');
    this.dirty();
  }

  closeProject(id: number): void {
    const i = this.projects.findIndex((x) => x.id === id);
    const p = this.projects[i];
    if (!p) return;
    for (const w of p.workspaces) for (const t of w.tabs) for (const pane of t.panes) pane.shell.fg?.dispose?.();
    this.projects.splice(i, 1);
    if (this.active >= this.projects.length) this.active = Math.max(0, this.projects.length - 1);
    else if (this.active > i) this.active -= 1;
    this.emit('narrate', `Closed ${projectLabel(p)}. Its shells were stopped.`);
    this.dirty();
  }

  private goto(p: number, w: number, t?: number): void {
    const project = this.projects[p];
    const ws = project?.workspaces[w];
    if (!ws) return;
    this.nav = null;
    this.active = p;
    project.active = w;
    if (t !== undefined && ws.tabs[t]) ws.active = t;
    this.unfoldFocus();
  }

  selectWorkspace(p: number, w: number): void {
    this.goto(p, w);
    this.dirty();
  }

  selectTab(p: number, w: number, t: number): void {
    this.goto(p, w, t);
    this.selection = null;
    this.emit('select', 'tab');
    this.dirty();
  }

  addTab(p: number, w: number): void {
    const project = this.projects[p];
    const ws = project?.workspaces[w];
    if (!ws) return;
    ws.tabs.push(this.newTab([this.newPane(project, ws)]));
    this.active = p;
    project.active = w;
    ws.active = ws.tabs.length - 1;
    this.nav = null;
    this.emit('narrate', 'A fresh tab. Type `help` to see what this little demo shell can do.');
    this.dirty();
  }

  closeTab(p: number, w: number, t: number): void {
    const ws = this.projects[p]?.workspaces[w];
    const tab = ws?.tabs[t];
    if (!ws || !tab) return;
    for (const pane of tab.panes) pane.shell.fg?.dispose?.();
    ws.tabs.splice(t, 1);
    ws.active = Math.max(0, Math.min(ws.active, ws.tabs.length - 1));
    this.dirty();
  }

  closeWorkspace(p: number, w: number): void {
    const project = this.projects[p];
    const ws = project?.workspaces[w];
    if (!ws) return;
    if (project.workspaces.filter((x) => !x.removing).length === 1) {
      for (const t of ws.tabs) for (const pane of t.panes) pane.shell.fg?.dispose?.();
      ws.tabs = [];
      this.dirty();
      return;
    }
    if (ws.worktree) {
      this.overlay = { kind: 'remove', project: project.id, workspace: ws.id };
      this.dirty();
      return;
    }
    for (const t of ws.tabs) for (const pane of t.panes) pane.shell.fg?.dispose?.();
    project.workspaces.splice(w, 1);
    project.active = Math.max(0, Math.min(project.active, project.workspaces.length - 1));
    this.dirty();
  }

  removeMessage(o: { project: number; workspace: number }): string {
    const p = this.projects.find((x) => x.id === o.project);
    const w = p?.workspaces.find((x) => x.id === o.workspace);
    if (!w) return '';
    return `Remove the workspace ${workspaceLabel(w)} and delete its worktree folder ${w.root}? The branch is kept.`;
  }

  submitRemove(): void {
    const o = this.overlay;
    if (!o || o.kind !== 'remove') return;
    this.overlay = null;
    const p = this.projects.find((x) => x.id === o.project);
    const w = p?.workspaces.find((x) => x.id === o.workspace);
    if (!p || !w) return this.dirty();
    for (const t of w.tabs) for (const pane of t.panes) pane.shell.fg?.dispose?.();
    w.tabs = [];
    w.removing = true;
    const at = p.workspaces.indexOf(w);
    if (p.active === at) {
      const others = [...p.workspaces.keys()].filter((i) => i < at).reverse().concat([...p.workspaces.keys()].filter((i) => i > at));
      const next = others.find((i) => !p.workspaces[i].removing);
      if (next !== undefined) p.active = next;
    }
    this.dirty();
    this.after(1800, () => {
      const i = p.workspaces.indexOf(w);
      if (i < 0) return;
      p.workspaces.splice(i, 1);
      if (p.active > i) p.active -= 1;
      this.notify(`removed ${workspaceLabel(w)}`);
      this.emit('narrate', `Workspace removed. Don’t worry, the branch ${w.branch} is still there.`);
    });
  }

  exitPane(id: number): void {
    const found = this.findPane(id);
    if (!found) return;
    const { w, t } = found;
    const next = remove(t.layout, id);
    t.panes = t.panes.filter((x) => x.id !== id);
    if (!next) {
      const i = w.tabs.indexOf(t);
      w.tabs.splice(i, 1);
      w.active = Math.max(0, Math.min(w.active, w.tabs.length - 1));
    } else {
      t.layout = next;
      if (t.active === id) t.active = t.panes[t.panes.length - 1].id;
    }
    this.dirty();
  }

  resetBorder(border: Border): void {
    const reset = { projects: 32, workspaces: 26, stack: null, agents: null, changes: null }[border];
    this.widths = { ...this.widths, [border]: reset };
    this.dirty();
  }

  sidebar(): Sidebar {
    return SIDEBARS.find(([id]) => id === this.config.sidebar)?.[0] ?? 'projects_on_top';
  }

  toggleNav(): void {
    this.nav = this.nav ? null : 'projects';
    if (this.nav) {
      this.changesOpen = false;
      this.closeTodo();
      this.files.close();
    }
    this.dirty();
  }

  panelShown(): boolean {
    return this.changesShown() || this.todo.open || this.filesShown();
  }

  changesShown(): boolean {
    return this.changesOpen && hasChanges(this.workspace());
  }

  changesDiff(): FileDiff[] {
    const p = this.project();
    const w = this.workspace();
    return p && w ? workspaceDiff(p, w, this.changesMode) : [];
  }

  private fileKey(f: FileDiff): string {
    return `${this.workspace()?.id}:${this.changesMode}:${f.change.path}`;
  }

  changesViewed(f: FileDiff): boolean {
    return this.viewed.has(this.fileKey(f));
  }

  changesFolded(f: FileDiff): boolean {
    return this.folded.get(this.fileKey(f)) ?? (!!f.change.lockfile || this.changesViewed(f));
  }

  toggleChanges(): void {
    this.changesOpen = !this.changesOpen;
    if (this.changesOpen) {
      this.closeTodo();
      this.files.close();
    }
    if (this.changesFilter) this.changesFilter.focused = false;
    this.nav = null;
    if (this.changesOpen) this.emit('narrate', 'Everything this workspace changed, right next to its agent. Hover a hunk to open it, copy it or send it back.');
    this.dirty();
  }

  toggleTodo(): void {
    if (this.todo.open) this.closeTodo();
    else {
      this.changesOpen = false;
      this.files.close();
      this.todo.open = true;
      this.emit('narrate', 'Your TODO list, next to the shells it is about. Click a line to edit it, ○ to tick it off.');
    }
    this.nav = null;
    this.dirty();
  }

  private closeTodo(): void {
    this.todo.commit();
    this.todo.open = false;
  }

  todoTyping(): boolean {
    return !this.overlay && this.todo.open && !!this.todo.field;
  }

  addTodo(): void {
    this.todo.commit();
    this.todo.edit(null);
    this.dirty();
  }

  editTodo(id: number, line: number, col: number): void {
    this.todo.commit();
    const item = this.todo.item(id);
    if (!item) return;
    this.todo.edit(id, item.text);
    this.placeTodoCursor(line, col);
  }

  placeTodoCursor(line: number, col: number): void {
    this.todo.place(todoWidth(this), line, col);
    this.dirty();
  }

  toggleTodoItem(id: number): void {
    this.todo.commit();
    this.todo.toggle(id);
    this.dirty();
  }

  deleteTodo(id: number): void {
    this.todo.commit();
    this.todo.remove(id);
    this.undoToast('deleted');
  }

  clearDoneTodos(): void {
    this.todo.commit();
    const n = this.todo.clearDone();
    if (n) this.undoToast(`cleared ${n} done`);
    else this.dirty();
  }

  private undoToast(text: string): void {
    this.toast = { text, until: this.now() + 6000, undo: () => this.undoTodo() };
    this.after(6050, () => this.dirty());
    this.dirty();
  }

  undoTodo(): void {
    this.todo.restore();
    this.toast = null;
    this.dirty();
  }

  scrollTodo(value: number): boolean {
    const changed = this.todo.scroll !== value;
    this.todo.scroll = value;
    if (changed) this.dirty();
    return changed;
  }

  filesShown(): boolean {
    return this.files.open && !!this.workspace();
  }

  filesMap(): Map<string, string> {
    const p = this.project();
    const w = this.workspace();
    return workspaceFiles(p, w, p && w ? workspaceDiff(p, w, 'all') : []);
  }

  private filesPlace(): FilesPlace | null {
    const w = this.workspace();
    return w ? this.files.place(w.id) : null;
  }

  toggleFiles(): void {
    if (this.files.open) this.files.close();
    else {
      this.openFiles();
      this.emit('narrate', 'Every file of the workspace in your terminal’s colours, with git’s marks in the tree and beside each line. Type to search the text, ▤ for names.');
    }
    this.nav = null;
    this.dirty();
  }

  private openFiles(): void {
    if (this.files.open) return;
    this.changesOpen = false;
    this.closeTodo();
    this.files.open = true;
  }

  filesTyping(): boolean {
    const place = this.filesPlace();
    return !this.overlay && this.filesShown() && !!place && typing(place);
  }

  focusFilesBar(): void {
    const place = this.filesPlace();
    if (place) place.focused = true;
    this.dirty();
  }

  toggleFilesMode(): void {
    const place = this.filesPlace();
    if (!place) return;
    place.mode = place.mode === 'name' ? 'text' : 'name';
    this.editFilesQuery(place.query);
  }

  editFilesQuery(query: string): void {
    const place = this.filesPlace();
    if (place) Object.assign(place, { query, focused: true, selected: 0, scroll: 0 });
    this.dirty();
  }

  scrollFiles(dy: number, max: number): boolean {
    const place = this.filesPlace();
    if (!place) return false;
    const before = place.viewer ? place.viewer.scroll : wanted(place) ? place.scroll : place.treeScroll;
    const after = Math.max(0, Math.min(max, Math.min(before, max) + dy));
    if (place.viewer) place.viewer.scroll = after;
    else if (wanted(place)) place.scroll = after;
    else place.treeScroll = after;
    if (after !== before) this.dirty();
    return after !== before;
  }

  toggleFilesFolder(path: string): void {
    const place = this.filesPlace();
    if (place && !place.expanded.delete(path)) place.expanded.add(path);
    this.dirty();
  }

  showFile(path: string, lines: [number, number] | null = null, find: string | null = null): void {
    const place = this.filesPlace();
    if (place) viewFile(place, path, lines, find);
    this.files.selecting = null;
    this.dirty();
  }

  openFound(i: number): void {
    const place = this.filesPlace();
    if (!place) return;
    const query = wanted(place);
    const rows = foundRows(this.filesMap(), place);
    const row = rows.slice(i).find((r) => r.kind !== 'file');
    place.focused = false;
    place.selected = selectable(rows).filter((r) => r < i).length;
    if (row?.kind === 'name') this.showFile(row.path);
    else if (row?.kind === 'line') this.showFile(row.path, [row.n, row.n], query);
    else this.dirty();
  }

  private moveFound(place: FilesPlace, delta: number): void {
    const rows = foundRows(this.filesMap(), place);
    const all = selectable(rows);
    if (!all.length) return;
    const last = all.length - 1;
    place.selected = Math.max(0, Math.min(last, Math.min(place.selected, last) + delta));
    const row = all[place.selected];
    const header = row > 0 && rows[row - 1].kind === 'file' ? 1 : 0;
    const shown = Math.max(1, this.areas().changes.h - 3);
    if (row - header < place.scroll) place.scroll = row - header;
    else if (row >= place.scroll + shown) place.scroll = row + 1 - shown;
  }

  private filesKey(k: Key): boolean {
    const place = this.filesPlace();
    if (!place) return false;
    const ch = this.typed(k);
    if (k.key === 'Escape') Object.assign(place, { query: '', focused: false, selected: 0, scroll: 0 });
    else if (k.key === 'Enter') {
      const i = selectable(foundRows(this.filesMap(), place))[place.selected];
      if (i !== undefined) this.openFound(i);
    } else if (k.key === 'ArrowUp' || k.key === 'ArrowDown') this.moveFound(place, k.key === 'ArrowUp' ? -1 : 1);
    else if (k.key === 'Backspace') this.editFilesQuery([...place.query].slice(0, -1).join(''));
    else if (ch) this.editFilesQuery(place.query + ch);
    this.dirty();
    return true;
  }

  closeFile(): void {
    const place = this.filesPlace();
    if (place) place.viewer = null;
    this.files.selecting = null;
    this.dirty();
  }

  toggleFileBlock(key: number): void {
    const viewer = this.filesPlace()?.viewer;
    if (viewer && !viewer.unfolded.delete(key)) viewer.unfolded.add(key);
    this.dirty();
  }

  selectFileLine(n: number): void {
    const viewer = this.filesPlace()?.viewer;
    if (!viewer) return;
    viewer.selection = [n, n];
    this.files.selecting = n;
    this.dirty();
  }

  fileAction(action: FileAction): void {
    const p = this.project();
    const w = this.workspace();
    const viewer = this.filesPlace()?.viewer;
    if (!p || !w || !viewer) return;
    const sel = viewer.selection && ordered(viewer.selection);
    const files = this.filesMap();
    if (action === 'open') this.openInEditor(p, w, viewer.path, files.get(viewer.path) ?? '', sel ? sel[0] : 1);
    else if (action === 'ask agent') this.askAgent(w, !sel ? viewer.path : sel[0] === sel[1] ? `${viewer.path}:${sel[0]}` : `${viewer.path}:${sel[0]}-${sel[1]}`);
    else {
      this.emit('copy', selectedText(files, viewer));
      this.notify('copied to clipboard');
    }
    this.dirty();
  }

  private openLink(link: PathLink): void {
    this.openFiles();
    this.nav = null;
    this.showFile(link.path, link.lines);
    this.emit('narrate', 'A path printed in a pane is a link: it opens here, at its line.');
  }

  private linkUnder(pane: Pane, r: Rect, x: number, y: number, grid: Grid): PathLink | null {
    const w = this.workspace();
    if (!w || !contains(r, x, y)) return null;
    const row = Array.from({ length: r.w }, (_, i) => grid.at(r.x + i, y)?.ch ?? ' ');
    const link = linkAt(row, x - r.x);
    const path = link && resolveLink(link.path, pane.shell.cwd, w.root, this.filesMap());
    return link && path ? { start: r.x + link.start, end: r.x + link.end, path, lines: link.lines } : null;
  }

  paneLanding(drag = this.paneDrag): { area: Rect; place: PanePlace; layout: Node } | null {
    const h = this.hover;
    if (!drag?.moved || !h || this.tab() !== drag.tab) return null;
    const area = this.areas().pane;
    const under = this.panesOf(drag.tab, area).find(([id, r]) => id !== drag.pane.id && contains(r, h.x, h.y));
    if (!under) return null;
    const [target, r] = under;
    const place = placeAt(r, h.x, h.y);
    const layout = moved(drag.tab.layout, drag.pane.id, target, place);
    if (!layout || !hasRoom(layout, area)) return null;
    return { area: placeArea(place, r), place, layout };
  }

  hoveredLink(pane: Pane, r: Rect, grid: Grid): { y: number; start: number; end: number } | null {
    const h = this.hover;
    if (!h || this.overlay || this.rowDrag || this.paneDrag || this.selection || this.dragging) return null;
    const link = this.linkUnder(pane, r, h.x, h.y, grid);
    return link && { y: h.y, start: link.start, end: link.end };
  }

  openChangesFilter(): void {
    if (this.changesFilter) this.changesFilter.focused = true;
    else this.changesFilter = { query: '', focused: true };
    this.dirty();
  }

  closeChangesFilter(): void {
    this.changesFilter = null;
    this.dirty();
  }

  private changesFilterKey(k: Key): boolean {
    const filter = this.changesFilter;
    if (!filter) return false;
    const ch = this.typed(k);
    if (k.key === 'Escape') this.changesFilter = null;
    else if (k.key === 'Enter') filter.focused = false;
    else if (k.key === 'Backspace') filter.query = filter.query.slice(0, -1);
    else if (ch) filter.query += ch;
    else return true;
    this.changesScroll = 0;
    this.dirty();
    return true;
  }

  private filteringChanges(): boolean {
    return !this.overlay && this.changesShown() && !!this.changesFilter?.focused;
  }

  setChangesMode(mode: ChangesMode): void {
    this.changesMode = mode;
    this.changesScroll = 0;
    this.dirty();
  }

  toggleChangesFile(f: FileDiff): void {
    this.folded.set(this.fileKey(f), !this.changesFolded(f));
    this.dirty();
  }

  toggleChangesViewed(f: FileDiff): void {
    const key = this.fileKey(f);
    if (!this.viewed.delete(key)) this.viewed.add(key);
    this.folded.delete(key);
    this.dirty();
  }

  foldAllChanges(): void {
    const files = this.changesDiff();
    const fold = files.some((f) => !this.changesFolded(f));
    for (const f of files) this.folded.set(this.fileKey(f), fold);
    this.dirty();
  }

  scrollChanges(dy: number, max: number): boolean {
    const next = Math.max(0, Math.min(max, Math.min(this.changesScroll, max) + dy));
    if (next === this.changesScroll) return false;
    this.changesScroll = next;
    this.dirty();
    return true;
  }

  openChangesBase(at: Pos): void {
    const branches = [...BASES, ...(this.project()?.workspaces.map((w) => w.branch).filter((b): b is string => !!b && !BASES.includes(b)) ?? [])];
    this.overlay = { kind: 'menu', at, actions: branches.map((branch) => ({ kind: 'base', branch })) };
    this.dirty();
  }

  hunkAction(action: HunkAction, f: FileDiff, h: number): void {
    const hunk = f.hunks[h];
    const added = hunk.lines.filter((l) => l.kind === '+').map((l) => l.new ?? 0);
    const lines = added.length ? (added.length > 1 ? `${added[0]}-${added[added.length - 1]}` : `${added[0]}`) : `${hunk.newStart}`;
    const p = this.project();
    const w = this.workspace();
    if (!p || !w) return;
    if (action === 'copy') {
      this.emit('copy', hunk.lines.map((l) => `${l.kind}${l.text}`).join('\n'));
      this.notify('copied to clipboard');
    } else if (action === 'ask agent') this.askAgent(w, `${f.change.path}:${lines}`);
    else this.openInEditor(p, w, f.change.path, f.change.after, Number(lines.split('-')[0]));
    this.dirty();
  }

  private askAgent(w: Workspace, reference: string): void {
    const tab = w.tabs.find((t) => t.panes.some((pane) => pane.shell.fg instanceof Agent));
    const pane = tab?.panes.find((x) => x.shell.fg instanceof Agent);
    if (!tab || !pane) {
      this.emit('copy', reference);
      this.notify('no agent here, so the reference is copied');
      return;
    }
    w.active = w.tabs.indexOf(tab);
    tab.active = pane.id;
    pane.shell.paste(`${reference} `);
    this.notify('sent to the agent');
  }

  private openInEditor(p: Project, w: Workspace, path: string, text: string, line: number): void {
    const pane = this.newPane(p, w);
    pane.shell.start(new Editor(this.host(p, w, () => pane.id), () => pane.shell.finish(), path, text, line - 1));
    w.tabs.push(this.newTab([pane]));
    w.active = w.tabs.length - 1;
  }

  navTo(nav: Exclude<Nav, null>): void {
    this.nav = nav;
    this.dirty();
  }

  scrollProjects(rows: Rows, dy: number): boolean {
    const next = rows.scrolled(dy);
    if (next === this.projectsScroll) return false;
    this.projectsScroll = next;
    this.dirty();
    return true;
  }

  scrollAgents(rows: Rows, dy: number): boolean {
    const next = rows.scrolled(dy);
    if (next === this.agentsScroll) return false;
    this.agentsScroll = next;
    this.dirty();
    return true;
  }

  scrollWorkspaces(rows: Rows, dy: number): boolean {
    const next = rows.scrolled(dy);
    if (next === this.workspacesScroll) return false;
    this.workspacesScroll = next;
    this.dirty();
    return true;
  }

  closeOverlay(): void {
    this.overlay = null;
    this.dirty();
  }

  openMenu(at: Pos, target: Target): void {
    const actions: MenuAction[] = [{ kind: 'rename', target }];
    if (target.kind === 'project' && this.groups.length) actions.push({ kind: 'moveToGroup', project: target.project });
    this.overlay = { kind: 'menu', at, actions };
    this.dirty();
  }

  renameLabel(t: Target): string {
    return RENAME[t.kind].label;
  }

  renameHint(t: Target): string {
    return RENAME[t.kind].hint;
  }

  menuLabel(a: MenuAction): string {
    if (a.kind === 'rename') return this.renameLabel(a.target);
    if (a.kind === 'moveToGroup') return 'move to group';
    if (a.kind === 'setGroup') {
      const g = a.group === null ? undefined : this.group(a.group);
      return a.group === null ? 'no group' : g ? `${g.icon} ${g.name}` : '';
    }
    if (a.kind === 'groupStyle') return 'icon and colour';
    if (a.kind === 'deleteGroup') return 'delete group';
    if (a.kind === 'addProject') return 'add project';
    if (a.kind === 'openProject') return 'open project';
    if (a.kind === 'newGroup') return 'new group';
    if (a.kind === 'base') return a.branch;
    return a.action;
  }

  private resolveTarget(t: Exclude<Target, { kind: 'group' }>): { p?: Project; w?: Workspace; tab?: Tab } {
    const p = this.projects.find((x) => x.id === t.project);
    if (t.kind === 'project') return { p };
    const w = p?.workspaces.find((x) => x.id === t.workspace);
    if (t.kind === 'workspace') return { p, w };
    return { p, w, tab: w?.tabs.find((x) => x.id === t.tab) };
  }

  private currentName(t: Target): string | undefined {
    if (t.kind === 'group') return this.group(t.group)?.name;
    const { p, w, tab } = this.resolveTarget(t);
    return t.kind === 'project' ? p?.name : t.kind === 'workspace' ? w?.name : tab?.name;
  }

  private rename(t: Target, name: string | undefined): void {
    if (t.kind === 'group') {
      const g = this.group(t.group);
      if (g && name) g.name = name;
      return;
    }
    const { p, w, tab } = this.resolveTarget(t);
    if (t.kind === 'project' && p) p.name = name;
    if (t.kind === 'workspace' && w) w.name = name;
    if (t.kind === 'tab' && tab) tab.name = name;
  }

  chooseMenu(i: number): void {
    const o = this.overlay;
    if (o?.kind !== 'menu') return;
    const a = o.actions[i];
    this.overlay = null;
    if (!a) return this.dirty();
    if (a.kind === 'rename') {
      this.overlay = { kind: 'rename', target: a.target, input: this.currentName(a.target) ?? '' };
    } else if (a.kind === 'moveToGroup') {
      const current = this.projects.find((x) => x.id === a.project)?.group;
      const actions: MenuAction[] = this.groups.filter((g) => g.id !== current).map((g) => ({ kind: 'setGroup', project: a.project, group: g.id }));
      if (current !== undefined) actions.push({ kind: 'setGroup', project: a.project, group: null });
      this.overlay = { kind: 'menu', at: o.at, actions };
    } else if (a.kind === 'setGroup') {
      const p = this.projects.find((x) => x.id === a.project);
      if (p) p.group = a.group ?? undefined;
    } else if (a.kind === 'groupStyle') this.overlay = { kind: 'groupStyle', group: a.group };
    else if (a.kind === 'deleteGroup') this.askDeleteGroup(a.group);
    else if (a.kind === 'base') {
      this.changesBase = a.branch;
      this.changesScroll = 0;
    } else if (a.kind === 'addProject') return this.openPicker(a.group);
    else if (a.kind === 'openProject') return this.openPicker();
    else if (a.kind === 'newGroup') {
      this.nav = null;
      this.overlay = { kind: 'newGroup', input: '' };
      this.emit('narrate', 'Name the group. Right-click a project or its ⋯ to move it in.');
    } else this.paneAction(a.pane, a.action);
    this.dirty();
  }

  private paneAction(id: number, action: PaneAction): void {
    const found = this.findPane(id);
    if (!found) return;
    const { p, w, t, pane } = found;
    if (action === 'split right' || action === 'split down') {
      const fresh = this.newPane(p, w);
      t.panes.push(fresh);
      t.layout = split(t.layout, pane.id, action === 'split right' ? 'right' : 'down', fresh.id);
      t.active = fresh.id;
      this.emit('narrate', 'Split! Drag the divider to resize.');
    } else if (action === 'close pane') this.exitPane(pane.id);
    else pane.rightClicks = !pane.rightClicks;
  }

  openPaneMenu(at: Pos, tab: Tab, pane: Pane, area: Rect, alone: boolean): void {
    const actions: PaneAction[] = [];
    if (!alone && fits(area, 'right')) actions.push('split right');
    if (!alone && fits(area, 'down')) actions.push('split down');
    actions.push(pane.rightClicks ? 'use this menu on right-click' : 'send right-clicks to the pane');
    actions.push('close pane');
    tab.active = pane.id;
    this.overlay = { kind: 'menu', at, actions: actions.map((action) => ({ kind: 'pane', pane: pane.id, action })) };
    this.dirty();
  }

  openNewWorkspace(p: number): void {
    const project = this.projects[p];
    if (!project) return;
    this.overlay = { kind: 'newWorkspace', project: project.id, input: '', worktree: project.repo ? true : null };
    this.emit('narrate', 'Name it. In a repository it gets its own branch and folder, so nothing collides.');
    this.dirty();
  }

  newWorkspaceHint(o: { project: number; input: string; worktree: boolean | null }): string {
    const p = this.projects.find((x) => x.id === o.project);
    if (!p) return '';
    if (!o.worktree) return `in ${p.root}`;
    return `in ${this.config.worktreesDir}/${p.folder}/${folderSlug(o.input || 'branch')}`;
  }

  toggleWorktree(): void {
    const o = this.overlay;
    if (o?.kind === 'newWorkspace' && o.worktree !== null) {
      o.worktree = !o.worktree;
      this.dirty();
    }
  }

  submitForm(): void {
    const o = this.overlay;
    if (!o) return;
    if (o.kind === 'groupStyle') return this.closeOverlay();
    if (o.kind === 'newGroup') {
      const name = o.input.trim();
      if (!name) return;
      this.addGroup(name);
      this.closeOverlay();
      return;
    }
    if (o.kind === 'rename') {
      this.rename(o.target, o.input.trim() || undefined);
      this.overlay = null;
      this.dirty();
      return;
    }
    if (o.kind !== 'newWorkspace' || o.creating) return;
    const name = o.input.trim();
    if (!name) {
      o.error = 'the name is required';
      this.dirty();
      return;
    }
    const p = this.projects.find((x) => x.id === o.project);
    if (!p) return;
    if (o.worktree && p.workspaces.some((w) => w.branch === name)) {
      o.error = `a branch named '${name}' already exists`;
      this.dirty();
      return;
    }
    const finish = () => {
      const w = o.worktree ? this.addWorkspace(p, name, true) : this.addWorkspace(p, p.workspaces[0]?.branch ?? 'main', false, name);
      w.tabs.push(this.newTab([this.newPane(p, w)]));
      p.active = p.workspaces.length - 1;
      this.active = this.projects.indexOf(p);
      this.overlay = null;
      this.emit('narrate', o.worktree ? `A fresh copy of the repo on ${name}, .env included.` : 'A new workspace in the project folder.');
      this.dirty();
    };
    if (o.worktree) {
      o.creating = true;
      this.dirty();
      this.after(650, finish);
    } else finish();
  }

  openPicker(group?: number): void {
    this.nav = null;
    this.overlay = { kind: 'picker', dir: ['code'], filter: '', selected: null, scroll: 0, group };
    this.emit('narrate', 'Pick a folder. Type to filter, Enter to go in, open to add it.');
    this.dirty();
  }

  pickerPath(o: { dir: string[] }): string {
    return `~/${o.dir.map((d) => `${d}/`).join('')}`;
  }

  private pickerNode(dir: string[]): Tree | null {
    if (dir.length === 0) return { code: {}, Documents: {}, Downloads: {} };
    if (dir[0] !== 'code') return {};
    if (dir.length === 1) return Object.fromEntries(Object.keys(FOLDERS).map((k) => [k, FOLDERS[k].tree]));
    let node: string | Tree | undefined = FOLDERS[dir[1]]?.tree;
    for (const part of dir.slice(2)) node = node && typeof node !== 'string' ? node[part] : undefined;
    return node && typeof node !== 'string' ? node : null;
  }

  pickerItems(o: { dir: string[]; filter: string }): { name: string; branch?: string }[] {
    const node = this.pickerNode(o.dir) ?? {};
    const names = Object.keys(node)
      .filter((k) => typeof node[k] !== 'string' && !k.startsWith('.'))
      .sort();
    const items = names.map((name) => ({ name, branch: o.dir.length === 1 && o.dir[0] === 'code' && FOLDERS[name]?.repo ? 'main' : undefined }));
    const f = o.filter.toLowerCase();
    if (f) return items.filter((i) => i.name.toLowerCase().includes(f));
    return o.dir.length ? [{ name: '..' }, ...items] : items;
  }

  pickerHint(o: { dir: string[]; filter: string; selected: number | null }): string {
    const items = this.pickerItems(o);
    const sel = o.selected !== null ? items[o.selected] : undefined;
    if (sel?.name === '..') return 'enter goes up';
    if (sel) return `enter goes into ${sel.name}`;
    const path = this.pickerPath(o).replace(/\/$/, '');
    return `enter opens ${path}`;
  }

  pickerClick(i: number): void {
    const o = this.overlay;
    if (o?.kind !== 'picker') return;
    o.selected = i;
    this.pickerEnter();
  }

  pickerScroll(dy: number): boolean {
    const o = this.overlay;
    if (o?.kind !== 'picker') return false;
    o.scroll = Math.max(0, o.scroll + dy);
    this.dirty();
    return true;
  }

  private pickerEnter(): void {
    const o = this.overlay;
    if (o?.kind !== 'picker') return;
    const items = this.pickerItems(o);
    const sel = o.selected !== null ? items[o.selected] : undefined;
    if (!sel) return this.pickerOpen();
    if (sel.name === '..') o.dir = o.dir.slice(0, -1);
    else o.dir = [...o.dir, sel.name];
    o.filter = '';
    o.selected = null;
    o.scroll = 0;
    this.dirty();
  }

  pickerOpen(): void {
    const o = this.overlay;
    if (o?.kind !== 'picker') return;
    const dir = o.dir;
    const root = this.pickerPath(o).replace(/\/$/, '');
    const existing = this.projects.findIndex((p) => p.root === root);
    this.overlay = null;
    const group = o.group === undefined ? undefined : this.group(o.group);
    if (group) group.collapsed = false;
    if (existing >= 0) {
      if (group) this.projects[existing].group = group.id;
      this.selectProject(existing);
      return;
    }
    const folder = dir[dir.length - 1] ?? '~';
    const known = dir.length === 2 && dir[0] === 'code' ? FOLDERS[folder] : undefined;
    const tree = this.pickerNode(dir) ?? {};
    const added = this.addProject(folder, root, !!known?.repo, known?.tree ?? tree);
    if (group) added.group = group.id;
    this.active = this.projects.length - 1;
    this.emit('narrate', `Opened ${root}. Each project keeps its own workspaces and tabs.`);
    this.dirty();
  }

  openSearch(): void {
    this.overlay = { kind: 'search', query: '', selected: 0, scroll: 0 };
    this.emit('narrate', 'Type anything: projects, branches, tabs. Enter takes you there.');
    this.dirty();
  }

  searchResults(query: string): SearchResult[] {
    const q = query.trim().toLowerCase();
    if (!q) return [];
    const all: (SearchResult & { order: number })[] = [];
    let order = 0;
    this.groups.forEach((g) => {
      all.push({ kind: -1, name: `${g.icon} ${g.name}`, context: '', keys: [g.name], go: () => this.gotoGroup(g.id), order: order++ });
    });
    this.projects.forEach((p, pi) => {
      const project = projectLabel(p);
      const group = this.group(p.group)?.name ?? '';
      all.push({ kind: 0, name: project, context: group, keys: [project], go: () => this.selectProject(pi), order: order++ });
      p.workspaces.forEach((w, wi) => {
        const label = workspaceLabel(w);
        const keys = [label, ...(w.branch ? [w.branch] : [])];
        all.push({
          kind: 1,
          name: label,
          context: project,
          keys,
          go: () => this.goto(pi, wi),
          order: order++,
        });
        w.tabs.forEach((t, ti) => {
          const name = tabLabel(t);
          all.push({
            kind: 2,
            name,
            context: `${project} › ${label}`,
            keys: [name, ...keys],
            go: () => this.goto(pi, wi, ti),
            order: order++,
          });
        });
      });
    });
    const score = (r: SearchResult) => {
      let best = 3;
      for (const k of r.keys) {
        const key = k.toLowerCase();
        if (key === q) best = Math.min(best, 0);
        else if (key.startsWith(q)) best = Math.min(best, 1);
        else if (key.includes(q)) best = Math.min(best, 2);
      }
      return best;
    };
    return all
      .map((r) => ({ r, s: score(r) }))
      .filter((x) => x.s < 3)
      .sort((a, b) => a.s - b.s || a.r.kind - b.r.kind || a.r.order - b.r.order)
      .map((x) => x.r);
  }

  private gotoGroup(id: number): void {
    const g = this.group(id);
    if (g) g.collapsed = false;
    const first = this.projects.findIndex((p) => p.group === id);
    if (first >= 0) this.active = first;
  }

  searchGo(i: number): void {
    const o = this.overlay;
    if (o?.kind !== 'search') return;
    const r = this.searchResults(o.query)[i];
    this.overlay = null;
    this.nav = null;
    r?.go();
    this.dirty();
  }

  openUsage(): void {
    this.nav = null;
    this.overlay = { kind: 'usage' };
    this.emit('narrate', 'How much of your Claude and Codex plans is left, at a glance. No tokens spent.');
    if (!this.usage.loading) {
      this.usage.loading = true;
      this.after(1500, () => {
        this.usage = { loading: false, at: this.now() };
        this.dirty();
      });
    }
    this.dirty();
  }

  openSettings(): void {
    this.nav = null;
    this.overlay = { kind: 'settings', page: 0, cursor: 0 };
    this.emit('narrate', 'Change anything, it’s saved at once. Try Agents → default agent.');
    this.dirty();
  }

  private capturePrefix(o: SettingsOverlay, k: Key): void {
    const plain = !k.ctrl && !k.alt && !k.shift;
    if (plain && k.key === 'Escape') {
      o.capturing = false;
      o.notice = undefined;
      return;
    }
    if (plain && (k.key === 'Backspace' || k.key === 'Delete')) {
      o.capturing = false;
      this.config.prefix = '';
      o.notice = 'keyboard shortcuts are off';
      return;
    }
    const prefix = prefixOf(k);
    if (typeof prefix !== 'string') {
      o.notice = prefix.error;
      return;
    }
    o.capturing = false;
    this.config.prefix = prefix;
    const clash = prefixClash(prefix);
    o.notice = clash ? `${prefix} opens the keys menu; it is also ${clash}` : `${prefix} opens the keys menu`;
  }

  keysHint(): string {
    return `esc closes · ${this.config.prefix} twice types it in the pane`;
  }

  private keysKey(group: KeysGroup | null, k: Key): boolean {
    this.overlay = null;
    if (isPrefix(this.config.prefix, k)) {
      const t = this.tab();
      const pane = t ? activePane(t) : undefined;
      pane?.shell.key(k);
    } else if (k.key !== 'Escape') this.keysStep(keysLookup(group, k));
    this.dirty();
    return true;
  }

  keysClick(group: KeysGroup | null, i: number): void {
    this.overlay = null;
    this.keysStep(keysItems(group)[i]?.step ?? null);
    this.dirty();
  }

  private keysStep(step: KeysStep | null): void {
    if (!step) return;
    if ('open' in step) this.overlay = { kind: 'keys', group: step.open };
    else this.shortcut(step.run);
  }

  private bug(text: string): void {
    this.toast = { text, until: this.now() + 6000, bug: true };
    this.after(6050, () => this.dirty());
  }

  private cycle(at: number, len: number, delta: number): number | null {
    return len > 0 ? (((at + delta) % len) + len) % len : null;
  }

  private shortcut(a: KeysAction): void {
    const p = this.active;
    const project = this.project();
    const ws = this.workspace();
    const w = project?.active ?? 0;
    const t = this.tab();
    if (a.kind === 'nextTab' || a.kind === 'previousTab' || a.kind === 'tab') {
      if (!ws) return;
      const next = a.kind === 'tab' ? (a.t < ws.tabs.length ? a.t : null) : this.cycle(ws.active, ws.tabs.length, a.kind === 'nextTab' ? 1 : -1);
      if (next !== null) this.selectTab(p, w, next);
    } else if (a.kind === 'nextWorkspace' || a.kind === 'previousWorkspace') {
      const next = project && this.cycle(w, project.workspaces.length, a.kind === 'nextWorkspace' ? 1 : -1);
      if (next !== null && next !== undefined) this.selectWorkspace(p, next);
    } else if (a.kind === 'nextProject' || a.kind === 'previousProject') {
      const order = this.sidebarRows().flatMap((row) => (row.kind === 'project' ? [row.p] : []));
      const next = this.cycle(Math.max(0, order.indexOf(p)), order.length, a.kind === 'nextProject' ? 1 : -1);
      if (next !== null) this.goto(order[next], this.projects[order[next]].active);
    } else if (a.kind === 'pane') {
      const id = t && beside(this.panesOf(t, this.areas().pane), t.active, a.side);
      if (t && id) t.active = id;
    } else if (a.kind === 'agent') {
      const rows = this.agentRows();
      const at = rows.findIndex((r) => r.active);
      const next = rows.map((_, k) => rows[(at + 1 + k) % rows.length]).find((r) => r.status === 'waiting' || r.status === 'done');
      if (next) this.jumpToPane(next.pane);
      else this.notify('no agent needs you');
    } else if (a.kind === 'search') this.openSearch();
    else if (a.kind === 'newTab') this.addTab(p, w);
    else if (a.kind === 'split') {
      const area = t && this.panesOf(t, this.areas().pane).find(([id]) => id === t.active)?.[1];
      if (t && area && fits(area, a.dir)) this.paneAction(t.active, a.dir === 'right' ? 'split right' : 'split down');
      else if (t) this.bug('no room to split this pane');
    } else if (a.kind === 'closePane') {
      if (t && t.panes.length > 1) this.overlay = { kind: 'closePane', pane: t.active };
      else if (t && ws) this.askCloseTab(p, w, ws.active);
    } else if (a.kind === 'renameTab' || a.kind === 'renameWorkspace') {
      if (!project || !ws) return;
      const target: Target =
        a.kind === 'renameWorkspace' ? { kind: 'workspace', project: project.id, workspace: ws.id } : { kind: 'tab', project: project.id, workspace: ws.id, tab: t?.id ?? -1 };
      if (a.kind === 'renameTab' && !t) return;
      this.overlay = { kind: 'rename', target, input: this.currentName(target) ?? '' };
    } else if (a.kind === 'findNames' || a.kind === 'findText') {
      this.openFiles();
      const place = this.filesPlace();
      if (!place) return;
      place.viewer = null;
      const mode = a.kind === 'findNames' ? 'name' : 'text';
      if (place.mode !== mode) Object.assign(place, { mode, query: '', selected: 0, scroll: 0 });
      place.focused = true;
    } else if (a.kind === 'files') this.toggleFiles();
    else if (a.kind === 'changes' || a.kind === 'base') {
      if (!hasChanges(ws)) return this.bug('this workspace is not in a git repository');
      const opening = !this.changesOpen;
      if (a.kind === 'changes' || opening) this.toggleChanges();
      if (a.kind === 'changes' && opening) this.openChangesFilter();
      const panel = this.areas().changes;
      if (a.kind === 'base') this.openChangesBase({ x: panel.x + 2, y: panel.y + 2 });
    } else if (a.kind === 'newWorkspace') {
      if (project) this.openNewWorkspace(p);
    } else if (a.kind === 'closeWorkspace') {
      if (ws) this.askCloseWorkspace(p, w);
    } else if (a.kind === 'issues') this.openIssues();
    else if (a.kind === 'todo') {
      const opening = !this.todo.open;
      this.toggleTodo();
      if (opening) this.addTodo();
    } else if (a.kind === 'settings') this.openSettings();
    else if (a.kind === 'usage') this.openUsage();
    else if (a.kind === 'quit') this.quit();
  }

  settingsPage(i: number): void {
    const o = this.overlay;
    if (o?.kind !== 'settings') return;
    o.page = i;
    o.cursor = 0;
    o.pick = undefined;
    o.edit = undefined;
    o.notice = undefined;
    this.dirty();
  }

  private listedKinds(): string[] {
    const kinds: string[] = [];
    const add = (k: string) => {
      if (!kinds.includes(k)) kinds.push(k);
    };
    if (this.config.agent !== 'auto') add(this.config.agent);
    Object.keys(MODES).forEach(add);
    Object.keys(this.config.agentArgs).forEach(add);
    return kinds;
  }

  private modeOf(kind: string): string | null {
    const args = (this.config.agentArgs[kind] ?? []).join(' ');
    const found = (MODES[kind] ?? []).filter(([, a]) => args.includes(a)).sort((a, b) => b[1].length - a[1].length)[0];
    return found ? found[0] : null;
  }

  settingsRows(page: number): SettingsRow[] {
    const c = this.config;
    if (page === 0) {
      return [
        { id: 'folder', section: '', label: 'worktrees folder', value: c.worktreesDir, note: 'new worktrees go in <folder>/<repo>/<branch>' },
        { id: 'fetch', section: '', label: 'fetch branches every', value: c.fetchMinutes ? `${c.fetchMinutes} min` : 'off', note: 'commits to pull show as ↓n' },
      ];
    }
    if (page === 1) {
      const rows: SettingsRow[] = [
        { id: 'agent', section: 'Agent', label: 'default agent', value: c.agent, note: c.agent === 'auto' ? 'the agent in your tab, otherwise ask' : '' },
        { id: 'submit', section: 'Agent', label: 'send the prompt', value: c.submit ? '[x] sent for you' : '[ ] typed, you press Enter', note: '' },
        { id: 'trust', section: 'Agent', label: 'trust prompts', value: c.trust ? '[x] accepted for you' : '[ ] left to you', note: "saying yes runs the repo's agent config" },
        { id: 'resume', section: 'Agent', label: 'resume conversations', value: c.resume ? '[x] after a restart' : '[ ] never', note: 'Claude Code and Codex, in their tabs' },
      ];
      for (const kind of this.listedKinds()) {
        const mode = this.modeOf(kind);
        rows.push({
          id: `kind:${kind}`,
          section: 'How each agent starts',
          label: kind,
          value: mode ?? 'default',
          note: c.agent === kind ? 'the default agent' : '',
          dangerous: !!mode && mode.includes('danger'),
        });
      }
      rows.push({ id: 'add', section: 'How each agent starts', label: '+ another agent…', value: '', note: '' });
      return rows;
    }
    if (page === 2) {
      const token = (s: Remote, section: string): SettingsRow => {
        const on = c.accounts[s];
        return { id: `token:${s}`, section, label: tokenName(s), value: on ? this.accountOf(s) : 'not connected', note: on ? 'saved' : 'enter pastes one' };
      };
      const rows: SettingsRow[] = [
        token('shortcut', 'Accounts'),
        token('linear', 'Accounts'),
        { id: 'jira-site', section: 'Jira', label: 'Jira site', value: c.jiraSite || 'not set', note: 'such as acme.atlassian.net' },
        { id: 'jira-email', section: 'Jira', label: 'Jira email', value: c.jiraEmail || 'not set', note: 'the one you sign in with' },
        token('jira', 'Jira'),
        { id: 'jira-jql', section: 'Jira', label: 'Jira filter', value: c.jiraJql || 'none', note: 'JQL, such as project = SHOP' },
      ];
      const hidden = ['all', 'github', 'shortcut', 'linear', 'jira'].filter((s) => !c.sources.includes(s));
      for (const s of [...c.sources, ...hidden]) {
        rows.push({ id: `src:${s}`, section: 'Sources shown', label: `${c.sources.includes(s) ? '[x]' : '[ ]'} ${SOURCE_NAMES[s]}`, value: '', note: s === 'all' ? 'every source together' : '' });
      }
      return rows;
    }
    return [
      { id: 'sidebar', section: '', label: 'sidebar', value: this.sidebar(), note: 'how projects, workspaces and tabs are laid out' },
      { id: 'tabs', section: '', label: 'tabs', value: this.tabsOnTop() ? 'top' : 'sidebar', note: "where a workspace's tabs are listed" },
      { id: 'agents', section: '', label: 'agents section', value: c.agentsSection ? '[x] shown' : '[ ] hidden', note: 'every running agent in the sidebar' },
      { id: 'counts', section: '', label: 'counts', value: c.counts ? '[x] shown' : '[ ] hidden', note: 'what a project or folded group holds, such as (3)' },
      { id: 'dim', section: '', label: 'inactive panes', value: c.dim ? '[x] dimmed' : '[ ] as bright as the active one', note: 'in a split tab' },
      ...DETAILS.map(([id, note]) => ({ id, section: '', label: id, value: c[id] ? '[x] shown' : '[ ] hidden', note })),
      { id: 'notify', section: '', label: 'desktop notifications', value: c.notify, note: 'when an agent in another tab needs you or finishes' },
      { id: 'updates', section: '', label: 'check for updates', value: c.updates ? '[x] every hour' : '[ ] never', note: 'asks GitHub for the latest release' },
      this.prefixRow(),
    ];
  }

  private prefixRow(): SettingsRow {
    const o = this.overlay;
    const clash = prefixClash(this.config.prefix);
    const value = o?.kind === 'settings' && o.capturing ? 'press a key…' : this.config.prefix || 'off';
    return { id: 'prefix', section: '', label: 'prefix key', value, note: clash ? `also ${clash}` : 'opens a menu of keyboard shortcuts', dangerous: !!clash };
  }

  settingsHint(o: SettingsOverlay): string {
    if (o.capturing) return 'press the keys you want · backspace turns them off · esc cancels';
    if (o.edit) return 'enter saves · esc cancels';
    if (o.pick) return 'enter picks · type to filter · esc goes back';
    return 'enter changes the selected setting · tab or ←→ switches tabs · every change is saved at once';
  }

  settingsClick(i: number): void {
    const o = this.overlay;
    if (o?.kind !== 'settings') return;
    o.cursor = i;
    this.settingsActivate();
  }

  private settingsActivate(): void {
    const o = this.overlay;
    if (o?.kind !== 'settings') return;
    const row = this.settingsRows(o.page)[o.cursor];
    if (!row) return;
    const c = this.config;
    o.notice = undefined;
    if (row.id === 'folder') o.edit = { row: row.id, label: 'worktrees folder', input: c.worktreesDir, token: false };
    else if (row.id === 'fetch') o.edit = { row: row.id, label: 'fetch branches every (minutes, 0 turns it off)', input: String(c.fetchMinutes), token: false };
    else if (row.id === 'submit') {
      c.submit = !c.submit;
      o.notice = c.submit ? 'the prompt is sent for you' : 'the prompt is typed; you press Enter';
    } else if (row.id === 'trust') {
      c.trust = !c.trust;
      o.notice = c.trust ? 'trust prompts are accepted for you' : 'trust prompts are left to you';
    } else if (row.id === 'resume') {
      c.resume = !c.resume;
      o.notice = c.resume ? 'conversations resume after a restart' : 'agents no longer resume after a restart';
    } else if (row.id === 'model' || row.id === 'context' || row.id === 'memory') {
      c[row.id] = !c[row.id];
      o.notice = DETAIL_NOTICES[row.id][c[row.id] ? 0 : 1];
    } else if (row.id === 'agents') {
      c.agentsSection = !c.agentsSection;
      o.notice = c.agentsSection ? 'the sidebar lists every agent' : 'the sidebar no longer lists agents';
    } else if (row.id === 'counts') {
      c.counts = !c.counts;
      o.notice = c.counts ? 'rows show how many they hold' : 'rows no longer show how many they hold';
    } else if (row.id === 'dim') {
      c.dim = !c.dim;
      o.notice = c.dim ? 'inactive panes are dimmed' : 'every pane looks the same';
    } else if (row.id === 'prefix') {
      o.capturing = true;
    } else if (row.id === 'updates') {
      c.updates = !c.updates;
      o.notice = c.updates ? 'cornercase looks for new versions' : 'cornercase no longer looks for new versions';
    } else if (row.id === 'sidebar') {
      const items = SIDEBARS.map(([value, note]) => ({ value, note }));
      o.pick = { row: row.id, title: 'How should projects, workspaces and tabs be laid out?', items, selected: Math.max(0, items.findIndex((i) => i.value === this.sidebar())), filter: '' };
    } else if (row.id === 'tabs') {
      const items = TABS.map(([value, note]) => ({ value, note }));
      const current = this.tabsOnTop() ? 'top' : 'sidebar';
      o.pick = { row: row.id, title: "Where should a workspace's tabs go?", items, selected: Math.max(0, items.findIndex((i) => i.value === current)), filter: '' };
    } else if (row.id === 'notify') {
      const items = NOTIFY_CHOICES.map(([value, note]) => ({ value, note }));
      o.pick = { row: row.id, title: 'How should your terminal notify you?', items, selected: Math.max(0, items.findIndex((i) => i.value === c.notify)), filter: '' };
    } else if (row.id === 'agent') {
      o.pick = {
        row: row.id,
        title: 'Which agent takes an issue by default?',
        items: [{ value: 'auto', note: 'the agent in your tab, otherwise ask' }, ...AGENTS.map(([k, bin]) => ({ value: k, note: bin }))],
        selected: Math.max(0, ['auto', ...AGENTS.map(([k]) => k)].indexOf(c.agent)),
        filter: '',
      };
    } else if (row.id.startsWith('kind:')) {
      const kind = row.id.slice(5);
      const modes = MODES[kind] ?? [];
      const items: PickItem[] = [{ value: 'default', note: 'no mode arguments' }, ...modes.map(([name, args]) => ({ value: name, note: args, dangerous: name.includes('danger') })), { value: 'extra arguments…', note: '' }];
      const current = this.modeOf(kind) ?? 'default';
      o.pick = { row: row.id, title: `How should ${kind} start?`, items, selected: Math.max(0, items.findIndex((i) => i.value === current)), filter: '' };
    } else if (row.id === 'add') {
      const listed = this.listedKinds();
      const items = AGENTS.filter(([k]) => !listed.includes(k)).map(([k, bin]) => ({ value: k, note: bin }));
      o.pick = { row: row.id, title: 'Which agent do you want to set up?', items, selected: 0, filter: '' };
    } else if (row.id.startsWith('token:')) {
      const source = row.id.slice(6) as Remote;
      if (c.accounts[source]) o.notice = `${REMOTES[source].name} is connected; delete removes the token`;
      else o.edit = { row: row.id, label: tokenName(source), input: '', token: true };
    } else if (row.id === 'jira-site') {
      o.edit = { row: row.id, label: 'Jira site, such as acme.atlassian.net', input: c.jiraSite, token: false };
    } else if (row.id === 'jira-email') {
      o.edit = { row: row.id, label: 'Jira email', input: c.jiraEmail, token: false };
    } else if (row.id === 'jira-jql') {
      o.edit = { row: row.id, label: 'Jira filter (JQL, such as project = SHOP; empty lists everything)', input: c.jiraJql, token: false };
    } else if (row.id.startsWith('src:')) {
      const id = row.id.slice(4);
      if (c.sources.includes(id)) {
        if (c.sources.length === 1) o.notice = 'at least one tab stays';
        else c.sources = c.sources.filter((s) => s !== id);
      } else c.sources = [...c.sources, id];
      if (!o.notice) o.notice = `tabs: ${c.sources.join(', ')}`;
    }
    this.dirty();
  }

  openRestart(): void {
    this.overlay = { kind: 'restart', scroll: 0 };
    this.dirty();
  }

  restartNotes(width: number): Line[] {
    const running = this.projects.flatMap((p) =>
      p.workspaces.flatMap((w) =>
        w.tabs.flatMap((t) =>
          t.panes.flatMap((pane): Running[] => {
            const place = `${projectLabel(p)} › ${workspaceLabel(w)}`;
            const resumes = this.config.resume;
            if (pane.agent) return [{ program: pane.agent, place, agent: pane.agent, status: paneStatus(pane) ?? 'idle', resumes }];
            return pane.shell.busy ? [{ program: pane.shell.name, place, agent: null, status: 'idle', resumes: false }] : [];
          }),
        ),
      ),
    );
    return markdown(restartText(running), width);
  }

  scrollRestart(dy: number): boolean {
    const o = this.overlay;
    if (o?.kind !== 'restart') return false;
    const notes = updateNotes(this.cols, this.rows);
    const max = Math.max(0, this.restartNotes(notes.w).length - notes.h);
    o.scroll = Math.max(0, Math.min(max, o.scroll + dy));
    this.dirty();
    return true;
  }

  restartNow(): void {
    this.closeOverlay();
    const agents = this.config.resume ? ', agents back in their conversations' : '';
    this.emit('narrate', `For real, the server starts again and every tab comes back with a new shell${agents}. The demo keeps yours running.`);
  }

  pickChoices(o: SettingsOverlay): PickItem[] {
    if (!o.pick) return [];
    const f = o.pick.filter.trim().toLowerCase();
    return o.pick.items.filter((i) => i.value.toLowerCase().includes(f));
  }

  choosePick(i: number): void {
    const o = this.overlay;
    if (o?.kind !== 'settings' || !o.pick) return;
    const item = this.pickChoices(o)[i];
    const row = o.pick.row;
    o.pick = undefined;
    if (!item) return this.dirty();
    const c = this.config;
    if (row === 'agent') {
      c.agent = item.value;
      o.notice = `default agent: ${item.value}`;
    } else if (row === 'sidebar') {
      c.sidebar = item.value;
      o.notice = `sidebar: ${item.value}`;
    } else if (row === 'tabs') {
      c.tabs = item.value;
      o.notice = `tabs: ${item.value}`;
    } else if (row === 'notify') {
      c.notify = item.value;
      o.notice = `desktop notifications: ${item.value}`;
    } else if (row.startsWith('kind:')) {
      const kind = row.slice(5);
      if (item.value === 'extra arguments…') {
        o.edit = { row, label: `${kind} extra arguments`, input: '', token: false };
      } else {
        const mode = (MODES[kind] ?? []).find(([name]) => name === item.value);
        if (mode) c.agentArgs[kind] = mode[1].split(' ');
        else delete c.agentArgs[kind];
        o.notice = `${kind} starts with: ${mode ? mode[1] : 'no arguments'}`;
      }
    } else if (row === 'add') {
      c.agentArgs[item.value] = c.agentArgs[item.value] ?? [];
      o.notice = `${item.value} starts with: no arguments`;
    }
    this.dirty();
  }

  private submitEdit(): void {
    const o = this.overlay;
    if (o?.kind !== 'settings' || !o.edit) return;
    const edit = o.edit;
    if (edit.row === 'folder') {
      const v = edit.input.trim();
      if (!v) edit.error = 'the folder is required';
      else if (!v.startsWith('/') && !v.startsWith('~/')) edit.error = 'use an absolute path or one starting with ~/';
      else {
        this.config.worktreesDir = v;
        o.edit = undefined;
        o.notice = `new worktrees go in ${v}/<repo>/<branch>`;
      }
    } else if (edit.row === 'fetch') {
      const v = edit.input.trim();
      if (!/^\d+$/.test(v)) edit.error = 'use a whole number of minutes, 0 turns it off';
      else {
        this.config.fetchMinutes = Number(v);
        o.edit = undefined;
        o.notice = this.config.fetchMinutes ? `branches are fetched every ${this.config.fetchMinutes} min` : 'branches are not fetched: commits to pull are not shown';
      }
    } else if (edit.row.startsWith('token:')) {
      const source = edit.row.slice(6) as Remote;
      if (source === 'jira' && (!this.config.jiraSite || !this.config.jiraEmail)) {
        edit.error = 'set the Jira site and email first';
      } else if (!edit.input.trim()) {
        edit.error = `paste the ${REMOTES[source].token} first`;
      } else {
        o.busy = 'checking…';
        this.after(900, () => {
          o.busy = undefined;
          o.edit = undefined;
          this.config.accounts[source] = true;
          o.notice = `${REMOTES[source].name} connected as ${this.accountOf(source)}`;
          this.dirty();
        });
      }
    } else if (edit.row === 'jira-site' || edit.row === 'jira-email') {
      const site = edit.row === 'jira-site';
      const name = site ? 'site' : 'email';
      const input = edit.input.trim();
      const [value, error] = !input ? [''] : site ? checkSite(input) : checkEmail(input);
      if (error) edit.error = error;
      else {
        if (site) this.config.jiraSite = value;
        else this.config.jiraEmail = value;
        if (!value) this.config.accounts.jira = false;
        o.edit = undefined;
        o.notice = value ? `Jira ${name}: ${value}` : `the Jira ${name} was cleared`;
      }
    } else if (edit.row === 'jira-jql') {
      this.config.jiraJql = edit.input.trim();
      o.edit = undefined;
      o.notice = this.config.jiraJql ? `Jira lists only: ${this.config.jiraJql}` : 'Jira lists every issue you can see';
    } else if (edit.row.startsWith('kind:')) {
      const kind = edit.row.slice(5);
      const mode = this.modeOf(kind);
      const modeArgs = mode ? (MODES[kind] ?? []).find(([n]) => n === mode)?.[1].split(' ') ?? [] : [];
      const extra = edit.input.trim() ? edit.input.trim().split(/\s+/) : [];
      this.config.agentArgs[kind] = [...modeArgs, ...extra];
      o.edit = undefined;
      o.notice = `${kind} starts with: ${[...modeArgs, ...extra].join(' ') || 'no arguments'}`;
    }
    this.dirty();
  }

  openIssues(): void {
    const p = this.project();
    if (!p) return;
    this.nav = null;
    this.overlay = {
      kind: 'issues',
      project: p.id,
      tab: 0,
      closed: false,
      mine: false,
      filter: '',
      selected: 0,
      scroll: 0,
      detail: null,
      raw: false,
      detailScroll: 0,
      loading: true,
      token: { input: '', checking: false },
      agentPick: null,
      chosen: null,
    };
    this.overlay.token = this.freshToken(this.issuesSource(this.overlay));
    this.emit('narrate', 'Your issues from GitHub, Shortcut, Linear and Jira. Click one to read it, then press start.');
    this.after(450, () => {
      const o = this.overlay;
      if (o?.kind === 'issues') {
        o.loading = false;
        this.dirty();
      }
    });
    this.dirty();
  }

  issuesProjectName(o: IssuesOverlay): string {
    const p = this.projects.find((x) => x.id === o.project);
    return p ? projectLabel(p) : '';
  }

  private issuesSource(o: IssuesOverlay): string {
    return this.config.sources[o.tab] ?? 'all';
  }

  private accountOf(source: Remote): string {
    return `@you in ${source === 'jira' ? this.config.jiraSite : 'acme'}`;
  }

  private freshToken(source: string): IssuesOverlay['token'] {
    return source === 'jira' ? { input: this.config.jiraSite, checking: false, step: 'site' } : { input: '', checking: false };
  }

  private issuesList(o: IssuesOverlay): Issue[] {
    const source = this.issuesSource(o);
    const p = this.projects.find((x) => x.id === o.project);
    const github = !!p?.repo;
    const allowed = (s: string) => (s === 'github' ? github : isRemote(s) && this.config.accounts[s]);
    const f = o.filter.trim().toLowerCase();
    const project = this.config.jiraJql.trim().match(/^project\s*=\s*"?([a-z][a-z0-9_]*)"?$/i)?.[1].toUpperCase();
    return ISSUES.filter((i) => (source === 'all' ? allowed(i.source) : i.source === source && allowed(i.source)))
      .filter((i) => i.source !== 'jira' || !project || i.key.startsWith(`${project}-`))
      .filter((i) => !o.mine || i.mine)
      .filter((i) => !f || [i.key, i.title, i.author, i.state, ...i.labels].some((k) => k.toLowerCase().includes(f)));
  }

  branchFor(i: Issue): string {
    const prefix = i.source === 'github' ? `issue-${i.number}` : i.source === 'shortcut' ? `sc-${i.number}` : i.key;
    const s = slug(i.title, 40);
    return s ? `${prefix}-${s}` : prefix;
  }

  private startHint(i: Issue, o: IssuesOverlay): string {
    const p = this.projects.find((x) => x.id === o.project);
    if (p?.repo) return `in its own worktree, on ${this.branchFor(i)}`;
    return 'in a new tab';
  }

  private pickedAgent(o: IssuesOverlay): string | null {
    if (o.chosen) return o.chosen;
    if (this.config.agent !== 'auto') return this.config.agent;
    return this.runningAgent();
  }

  private runningAgent(): string | null {
    const t = this.tab();
    const name = t ? activePane(t)?.shell.name : undefined;
    return name && name in AGENT_KINDS ? AGENT_KINDS[name] : null;
  }

  private agentCommand(kind: string): string {
    const bin = AGENTS.find(([k]) => k === kind)?.[1] ?? kind;
    return [bin, ...(this.config.agentArgs[kind] ?? [])].join(' ');
  }

  private detailLines(i: Issue, o: IssuesOverlay, width: number): Line[] {
    const dim = { fg: 8 };
    const meta = [i.state, ...(i.labels.length ? [i.labels.join(', ')] : []), `opened by @${i.author}`, `updated ${i.age} ago`].join(' · ');
    const kind = this.pickedAgent(o);
    const agent: Line = kind
      ? [
          seg(kind, { add: BOLD }),
          ...(this.modeOf(kind) ? [seg(` · ${this.modeOf(kind)}`, this.modeOf(kind)?.includes('danger') ? { fg: 1 } : {})] : []),
          seg(`  ${this.agentCommand(kind)}`, dim),
          ...(this.config.agent === kind ? [seg('  (default)', dim)] : []),
        ]
      : [seg('none chosen yet: start asks which one', dim)];
    const lines: Line[] = [
      [seg(i.key, { fg: 6, add: BOLD }), seg(` ${i.title}`, { add: BOLD })],
      [seg(meta, dim)],
      [seg(i.url, dim)],
      [],
      [seg('start   ', dim), seg(this.startHint(i, o))],
      [seg('agent   ', dim), ...agent],
      [seg('prompt  ', dim), seg(i.url), seg(`  (${this.config.submit ? 'sent for you' : 'typed, you press enter'})`, dim)],
      [],
    ];
    if (o.raw) lines.push(...i.body.split('\n').map((l) => [seg(l)]));
    else lines.push(...markdown(i.body, width));
    return lines;
  }

  issuesView(o: IssuesOverlay): IssuesView {
    const tabs = this.config.sources.map((s) => SOURCE_NAMES[s]);
    const toggles = [
      { label: 'closed', on: o.closed },
      { label: o.mine ? 'people: assignee me' : 'people', on: o.mine },
    ];
    const source = this.issuesSource(o);
    const width = Math.min(this.cols - 4, 110) - 4;
    if (o.detail) {
      const issue = ISSUES.find((i) => i.key === o.detail);
      if (issue) {
        const lines = this.detailLines(issue, o, width);
        const rows = Math.min(this.rows - 2, 30) - 4;
        const more = lines.length > rows ? ` · lines ${o.detailScroll + 1}-${Math.min(o.detailScroll + rows, lines.length)} of ${lines.length}` : '';
        return {
          tabs,
          toggles,
          items: [],
          selected: 0,
          scroll: 0,
          empty: '',
          hint: `enter starts it${more}`,
          buttons: ['start', 'agent…', o.raw ? 'rendered' : 'raw', 'copy url', 'back'],
          detail: lines,
          picking: false,
        };
      }
    }
    if (o.agentPick) {
      const kinds = AGENTS.map(([k]) => k).filter((k) => k.includes(o.agentPick!.filter.trim().toLowerCase()));
      const running = this.runningAgent();
      return {
        tabs,
        toggles,
        items: kinds.map((k) => ({ key: k, title: this.agentCommand(k), meta: [k === this.config.agent ? 'default' : '', k === running ? 'running in your tab' : ''].filter(Boolean).join(' · ') })),
        selected: Math.min(o.agentPick.selected, Math.max(0, kinds.length - 1)),
        scroll: 0,
        empty: 'nothing matches',
        hint: 'which agent should take it? settings set how each one starts',
        buttons: ['choose', 'back'],
        picking: true,
      };
    }
    if (isRemote(source) && !this.config.accounts[source]) {
      const remote = REMOTES[source];
      const step = o.token.step ?? 'token';
      const help = [`Connect ${remote.name}.`, ''];
      let label = remote.token;
      let input = '•'.repeat(Math.min(o.token.input.length, 40));
      if (step === 'site') {
        help.push('Type your Jira Cloud site, such as acme.atlassian.net, and press Enter.');
        [label, input] = ['site', o.token.input];
      } else if (step === 'email') {
        help.push('Type the email you sign in to Atlassian with and press Enter.');
        [label, input] = ['email', o.token.input];
      } else {
        if (source === 'jira') help.push(`Signing in to ${this.config.jiraSite} as ${this.config.jiraEmail}.`, '');
        help.push(remote.help);
      }
      help.push(
        '',
        step === 'token'
          ? `It is saved in secrets.json (only you can read it); you can also paste it in settings. ${remote.env}, when set, takes precedence.`
          : 'The site and the email are saved in your settings.',
      );
      const back = step !== 'site' && source === 'jira' ? ['back'] : [];
      return {
        tabs,
        toggles,
        items: [],
        selected: 0,
        scroll: 0,
        empty: '',
        hint: o.token.error ?? '',
        error: !!o.token.error,
        buttons: [step === 'token' ? 'connect' : 'next', ...back, 'cancel'],
        token: { label, input, help },
        picking: false,
      };
    }
    const list = o.loading ? [] : this.issuesList(o);
    const selected = Math.min(o.selected, Math.max(0, list.length - 1));
    const sel = list[selected];
    const p = this.projects.find((x) => x.id === o.project);
    const nothing = source === 'all' && !p?.repo && !Object.values(this.config.accounts).some(Boolean);
    const empty = o.loading
      ? ''
      : o.filter
        ? 'no matches'
        : source === 'github' && !p?.repo
          ? 'this project is not in a git repository'
          : nothing
            ? 'nothing to list here: connect Shortcut, Linear or Jira in their tabs'
            : 'no open issues';
    const account = isRemote(source) ? this.accountOf(source) : '';
    const hint = o.loading ? 'loading…' : [account, sel ? `enter reads ${sel.key} · start works on it ${this.startHint(sel, o)}` : ''].filter(Boolean).join(' · ');
    const buttons = ['start', 'refresh', ...(isRemote(source) ? ['disconnect'] : []), 'cancel'];
    return {
      tabs,
      toggles,
      items: list.map((i) => ({
        key: i.key,
        title: i.title,
        meta: [i.source === 'github' ? '' : i.state, i.labels.join(', '), i.author, i.age].filter(Boolean).join(' · '),
      })),
      selected,
      scroll: o.scroll,
      empty,
      hint,
      buttons,
      picking: false,
    };
  }

  issuesTab(i: number): void {
    const o = this.overlay;
    if (o?.kind !== 'issues') return;
    o.tab = i;
    o.selected = 0;
    o.scroll = 0;
    o.filter = '';
    o.token = this.freshToken(this.issuesSource(o));
    this.dirty();
  }

  issuesToggle(i: number): void {
    const o = this.overlay;
    if (o?.kind !== 'issues') return;
    if (i === 0) o.closed = !o.closed;
    else o.mine = !o.mine;
    o.selected = 0;
    o.loading = true;
    this.after(300, () => {
      o.loading = false;
      this.dirty();
    });
    this.dirty();
  }

  issuesClick(i: number): void {
    const o = this.overlay;
    if (o?.kind !== 'issues') return;
    if (o.agentPick) {
      o.agentPick.selected = i;
      return this.issuesButton('choose');
    }
    const issue = this.issuesList(o)[i];
    if (!issue) return;
    o.selected = i;
    o.detail = issue.key;
    o.detailScroll = 0;
    this.emit('narrate', `${issue.key}. Press start and an agent takes it, on its own branch.`);
    this.dirty();
  }

  issuesScroll(dy: number): boolean {
    const o = this.overlay;
    if (o?.kind !== 'issues') return false;
    const n = this.issuesList(o).length;
    o.scroll = Math.max(0, Math.min(Math.max(0, n - 1), o.scroll + dy));
    this.dirty();
    return true;
  }

  issuesScrollDetail(dy: number, max: number): boolean {
    const o = this.overlay;
    if (o?.kind !== 'issues') return false;
    o.detailScroll = Math.max(0, Math.min(Math.max(0, max), o.detailScroll + dy));
    this.dirty();
    return true;
  }

  issuesButton(label: string): void {
    const o = this.overlay;
    if (o?.kind !== 'issues') return;
    if (label === 'cancel') return this.closeOverlay();
    if (label === 'back' && !o.agentPick && !o.detail && o.token.step && o.token.step !== 'site') {
      const email = o.token.step === 'token';
      o.token = { input: email ? this.config.jiraEmail : this.config.jiraSite, checking: false, step: email ? 'email' : 'site' };
      return this.dirty();
    }
    if (label === 'back') {
      if (o.agentPick) o.agentPick = null;
      else o.detail = null;
      return this.dirty();
    }
    if (label === 'raw' || label === 'rendered') {
      o.raw = !o.raw;
      return this.dirty();
    }
    if (label === 'refresh') {
      o.loading = true;
      this.after(500, () => {
        o.loading = false;
        this.dirty();
      });
      return this.dirty();
    }
    if (label === 'disconnect') {
      const source = this.issuesSource(o) as Remote;
      this.config.accounts[source] = false;
      o.token = this.freshToken(source);
      return this.dirty();
    }
    if (label === 'connect' || label === 'next') return this.issuesConnect();
    if (label === 'copy url') {
      const issue = ISSUES.find((i) => i.key === o.detail);
      if (issue) {
        this.emit('copy', issue.url);
        o.notice = `copied ${issue.url}`;
        this.after(2000, () => {
          o.notice = undefined;
          this.dirty();
        });
      }
      return this.dirty();
    }
    if (label === 'agent…') {
      o.agentPick = { selected: Math.max(0, AGENTS.findIndex(([k]) => k === this.pickedAgent(o))), filter: '' };
      return this.dirty();
    }
    if (label === 'choose') {
      if (!o.agentPick) return;
      const kinds = AGENTS.map(([k]) => k).filter((k) => k.includes(o.agentPick!.filter.trim().toLowerCase()));
      o.chosen = kinds[Math.min(o.agentPick.selected, kinds.length - 1)] ?? o.chosen;
      o.agentPick = null;
      if (o.chosen && o.chosen !== this.config.agent) o.notice = `${o.chosen} takes this one`;
      return this.dirty();
    }
    if (label === 'start') {
      const list = this.issuesList(o);
      const issue = o.detail ? ISSUES.find((i) => i.key === o.detail) : list[Math.min(o.selected, list.length - 1)];
      if (issue) this.startIssue(issue, o);
    }
  }

  private issuesConnect(): void {
    const o = this.overlay;
    if (o?.kind !== 'issues') return;
    const source = this.issuesSource(o) as Remote;
    const input = o.token.input.trim();
    if (o.token.step === 'site' || o.token.step === 'email') {
      const site = o.token.step === 'site';
      const [value, error] = site ? checkSite(input) : checkEmail(input);
      if (error) o.token.error = error;
      else if (site) {
        this.config.jiraSite = value;
        o.token = { input: this.config.jiraEmail, checking: false, step: 'email' };
      } else {
        this.config.jiraEmail = value;
        o.token = { input: '', checking: false, step: 'token' };
      }
      return this.dirty();
    }
    if (!input) {
      o.token.error = `paste the ${REMOTES[source].token} first`;
      return this.dirty();
    }
    o.busy = 'checking…';
    this.after(900, () => {
      o.busy = undefined;
      this.config.accounts[source] = true;
      o.token = this.freshToken(source);
      o.loading = true;
      this.after(400, () => {
        o.loading = false;
        this.dirty();
      });
      this.dirty();
    });
    this.dirty();
  }

  startIssue(issue: Issue, o: IssuesOverlay): void {
    const kind = this.pickedAgent(o);
    if (!kind) {
      o.agentPick = { selected: 0, filter: '' };
      this.dirty();
      return;
    }
    const p = this.projects.find((x) => x.id === o.project);
    if (!p) return;
    o.busy = 'starting…';
    this.dirty();
    this.after(650, () => {
      this.overlay = null;
      const branch = this.branchFor(issue);
      let w: Workspace;
      if (p.repo) {
        w = p.workspaces.find((x) => x.branch === branch) ?? this.addWorkspace(p, branch, true, truncateRight(`${issue.key} ${issue.title}`, 40));
      } else w = p.workspaces[p.active];
      const pane = this.newPane(p, w);
      const tab = this.newTab([pane]);
      if (!p.repo) tab.name = truncateRight(`${issue.key} ${issue.title}`, 40);
      w.tabs.push(tab);
      this.active = this.projects.indexOf(p);
      p.active = p.workspaces.indexOf(w);
      w.active = w.tabs.length - 1;
      this.workspacesScroll = 9999;
      this.emit('narrate', p.repo ? `A fresh branch for it: ${branch}.` : 'A new tab for the agent.');
      this.dirty();
      this.launch(pane, kind, issue.url);
    });
  }

  launch(pane: Pane, kind: string, prompt: string): void {
    const command = this.agentCommand(kind);
    this.after(300, () => {
      this.emit('narrate', `Starting ${command}…`);
      pane.shell.type(command);
    });
    let stage: 'agent' | 'asked' | 'trusted' | 'pasted' = 'agent';
    let waited = 0;
    const watch = () => {
      const agent = pane.shell.fg instanceof Agent ? pane.shell.fg : null;
      if (!(stage === 'asked' && agent?.trusting)) waited += 150;
      if (stage === 'agent' && agent?.trusting) {
        if (this.config.trust) {
          stage = 'trusted';
          this.emit('narrate', '“Do you trust this folder?” Yes, answered for you.');
          this.after(650, () => agent.key({ key: 'Enter' }));
        } else {
          stage = 'asked';
          this.emit('narrate', 'The agent asks if you trust this folder. Your call: answer it, and the issue follows.');
        }
      } else if (stage !== 'pasted' && agent?.ready) {
        stage = 'pasted';
        this.after(450, () => {
          agent.paste(prompt);
          if (this.config.submit) {
            this.emit('narrate', 'Issue sent. The agent’s on it, in its own corner.');
            this.after(350, () => agent.key({ key: 'Enter' }));
          } else this.emit('narrate', 'The issue is in the prompt. Click the pane and press Enter to send it.');
        });
        return;
      }
      if (waited < 20000) this.after(150, watch);
    };
    this.after(450, watch);
  }

  quit(): void {
    this.overlay = null;
    this.nav = null;
    this.detached = true;
    this.outerLines = [];
    this.outerInput = '';
    const shells = this.shells();
    this.emit('narrate', `Gone, but not stopped: your ${shells} shells keep running, agents included.`);
    this.emit('detached');
    this.dirty();
    if (this.scripted) return;
    let i = 0;
    const word = 'cornercase';
    const type = () => {
      if (!this.detached) return;
      if (i < word.length) {
        this.outerInput += word[i++];
        this.dirty();
        this.after(70, type);
      } else this.after(350, () => this.reattach());
    };
    this.after(1900, type);
  }

  reattach(): void {
    if (!this.detached) return;
    this.detached = false;
    this.emit('narrate', 'And we’re back. Same projects, same tabs, everything still running.');
    this.emit('attached');
    this.dirty();
  }

  selectedCells(r: Rect): [number, number][] {
    const s = this.selection;
    if (!s) return [];
    let a = s.from;
    let b = s.to;
    if (b.y < a.y || (b.y === a.y && b.x < a.x)) [a, b] = [b, a];
    const out: [number, number][] = [];
    for (let y = a.y; y <= b.y; y++) {
      const x0 = y === a.y ? a.x : r.x;
      const x1 = y === b.y ? b.x : r.x + r.w - 1;
      for (let x = x0; x <= x1; x++) out.push([x, y]);
    }
    return out;
  }

  private selectionText(r: Rect): string {
    const lines = new Map<number, string>();
    for (const [x, y] of this.selectedCells(r)) lines.set(y, (lines.get(y) ?? '') + (this.grid.at(x, y)?.ch ?? ''));
    return [...lines.values()].map((l) => l.trimEnd()).join('\n');
  }

  private hit(x: number, y: number, want: (r: Region) => boolean): Region | null {
    const regions = this.frame?.regions ?? [];
    for (let i = regions.length - 1; i >= 0; i--) {
      const r = regions[i];
      if (contains(r.r, x, y) && want(r)) return r;
    }
    return null;
  }

  cursorAt(x: number, y: number): string {
    if ((this.rowDrag?.moved && this.rowDrag.target) || this.paneDrag?.moved) return 'grabbing';
    return this.hit(x, y, (r) => !!r.cursor)?.cursor ?? 'default';
  }

  pointerMove(x: number, y: number, buttons: number): void {
    const prev = this.hover;
    this.hover = { x, y };
    const press = this.linkPress;
    if (press && (press.at.x !== x || press.at.y !== y)) {
      this.linkPress = null;
      press.held?.();
    }
    if (this.paneDrag) {
      if (buttons === 2) this.paneDrag.moved ||= !contains(this.paneDrag.rect, x, y);
      else this.paneDrag = null;
      this.dirty();
      return;
    }
    if (this.files.selecting !== null && buttons & 1) {
      const line = lineNear(this, y);
      const viewer = this.filesPlace()?.viewer;
      if (line !== null && viewer) viewer.selection = [this.files.selecting, line];
      this.dirty();
      return;
    }
    if (this.rowDrag) {
      if (buttons === 1) {
        this.rowDrag.moved ||= !contains(this.rowDrag.row, x, y);
        this.autoScroll();
      } else this.rowDrag = null;
      this.dirty();
      return;
    }
    if (this.dragging && buttons & 1) {
      if (this.dragging.kind === 'border') {
        const { border } = this.dragging;
        const total = border === 'changes' ? this.cols : mainWidth(this.widths, this.cols, this.panelShown());
        this.widths =
          border === 'agents'
            ? this.agentsDragged(y)
            : border !== 'changes' && this.sidebar() !== 'side_by_side'
              ? draggedStacked(this.widths, border, x, y, total, this.rows, this.config.agentsSection)
              : dragged(this.widths, border, x, total);
      }
      else {
        const { tab, divider } = this.dragging;
        tab.layout = setRatio(tab.layout, divider.path, ratioAt(divider, x, y));
        this.dragging = { ...this.dragging, divider: { ...divider } };
      }
      this.dirty();
      return;
    }
    if (this.selection && buttons & 1) {
      const r = this.selection.rect;
      this.selection.to = { x: Math.max(r.x, Math.min(r.x + r.w - 1, x)), y: Math.max(r.y, Math.min(r.y + r.h - 1, y)) };
      this.dirty();
      return;
    }
    if (!prev || prev.x !== x || prev.y !== y) this.dirty();
  }

  pointerLeave(): void {
    this.hover = null;
    this.dirty();
  }

  pointerDown(x: number, y: number, button: number): void {
    this.hover = { x, y };
    if (this.detached) return this.reattach();
    if (this.rowDrag) {
      this.rowDrag = null;
      this.dirty();
      return;
    }
    if (this.changesFilter && !this.overlay && !(this.changesShown() && contains(this.areas().changes, x, y))) this.changesFilter.focused = false;
    if (this.todo.field && !this.overlay && !(this.todo.open && contains(this.areas().changes, x, y))) this.todo.commit();
    if (!this.overlay && !(this.filesShown() && contains(this.areas().changes, x, y))) this.files.unfocus();
    const region = this.hit(x, y, (r) => !!(r.click || r.right || r.drag || r.pane));
    if (!region) return;
    if (region.drag && button === 0) {
      const now = this.now();
      const id = region.drag.kind === 'border' ? region.drag.border : `div:${region.drag.divider.path.join()}`;
      if (this.lastClick && this.lastClick.border === id && now - this.lastClick.at < DOUBLE_CLICK) {
        this.lastClick = null;
        if (region.double) region.double();
        else if (region.drag.kind === 'divider') {
          region.drag.tab.layout = setRatio(region.drag.tab.layout, region.drag.divider.path, 0.5);
          this.dirty();
        }
        return;
      }
      this.lastClick = { at: now, border: id };
      this.dragging = region.drag;
      this.dirty();
      return;
    }
    if ((region.grab || region.hold) && button === 0) {
      const click = region.click;
      this.rowDrag = { target: region.grab ?? null, row: region.r, moved: false, click: click && (() => click(x, y)), scrolled: 0 };
      return;
    }
    if (region.pane) {
      const { pane, rect, tab, alone } = region.pane;
      if (button === 2) {
        this.paneDrag = { tab, pane, from: { x, y }, rect, alone, moved: false };
        return;
      }
      if (tab.active !== pane.id) {
        tab.active = pane.id;
        this.selection = null;
        this.dirty();
        return;
      }
      const link = button === 0 ? this.linkUnder(pane, rect, x, y, this.grid) : null;
      const press = (held: (() => void) | null) => link && { at: { x, y }, link, held };
      const fg = pane.shell.fg;
      if (pane.shell.mouse && fg?.click) {
        const forward = () => fg.click?.(x - rect.x, y - rect.y);
        this.linkPress = press(forward);
        if (!link) forward();
        return;
      }
      this.linkPress = press(null);
      this.selection = { pane: pane.id, from: { x, y }, to: { x, y }, rect };
      this.dirty();
      return;
    }
    if (button === 2) {
      region.right?.(x, y);
      return;
    }
    if (button === 0) region.click?.(x, y);
  }

  pointerUp(): void {
    this.files.selecting = null;
    const paneDrag = this.paneDrag;
    if (paneDrag) {
      this.paneDrag = null;
      if (!paneDrag.moved) return this.openPaneMenu(paneDrag.from, paneDrag.tab, paneDrag.pane, paneDrag.rect, paneDrag.alone);
      const landing = this.paneLanding(paneDrag);
      if (landing) {
        paneDrag.tab.layout = landing.layout;
        paneDrag.tab.active = paneDrag.pane.id;
      }
      this.dirty();
      return;
    }
    const press = this.linkPress;
    this.linkPress = null;
    const drag = this.rowDrag;
    if (drag) {
      if (drag.moved && drag.target) this.dropRow(drag.target);
      this.rowDrag = null;
      if (!drag.moved) drag.click?.();
      this.dirty();
      return;
    }
    if (this.dragging) {
      this.dragging = null;
      this.dirty();
      return;
    }
    if (press?.held) {
      this.openLink(press.link);
      return;
    }
    const s = this.selection;
    if (s) {
      const moved = s.from.x !== s.to.x || s.from.y !== s.to.y;
      if (moved) {
        const text = this.selectionText(s.rect);
        if (text.trim()) {
          this.emit('copy', text);
          this.notify('copied to clipboard');
        }
      } else if (press) this.openLink(press.link);
      this.selection = null;
      this.dirty();
    }
  }

  wheel(x: number, y: number, dy: number): boolean {
    const region = this.hit(x, y, (r) => !!r.wheel);
    return region?.wheel?.(dy > 0 ? 3 : -3) ?? false;
  }

  paste(text: string): void {
    if (this.overlay?.kind === 'keys') this.overlay = null;
    const o = this.overlay;
    if (o) {
      if (o.kind === 'newWorkspace' || o.kind === 'rename' || o.kind === 'newGroup') o.input += text.replace(/\s+/g, ' ');
      else if (o.kind === 'search') o.query += text;
      else if (o.kind === 'picker') o.filter += text;
      else if (o.kind === 'settings' && o.edit) o.edit.input += o.edit.token ? text.replace(/\s/g, '') : text;
      else if (o.kind === 'issues') o.token.input += text.trim();
      this.dirty();
      return;
    }
    if (this.todoTyping()) {
      this.todo.type(text);
      this.dirty();
      return;
    }
    if (this.filesTyping()) {
      this.editFilesQuery((this.filesPlace()?.query ?? '') + text.replace(/\p{Cc}/gu, ''));
      return;
    }
    if (this.filteringChanges() && this.changesFilter) {
      this.changesFilter.query += text.replace(/\s+/g, ' ');
      this.changesScroll = 0;
      this.dirty();
      return;
    }
    const t = this.tab();
    if (t) activePane(t)?.shell.paste(text);
  }

  key(k: Key): boolean {
    if (this.detached) {
      if (k.key === 'Enter') this.reattach();
      else if (k.key === 'Backspace') this.outerInput = this.outerInput.slice(0, -1);
      else if (this.typed(k)) this.outerInput += k.key;
      this.dirty();
      return true;
    }
    if ((this.rowDrag || this.paneDrag) && k.key === 'Escape') {
      this.rowDrag = null;
      this.paneDrag = null;
      this.dirty();
      return true;
    }
    const o = this.overlay;
    if (o?.kind === 'keys') return this.keysKey(o.group, k);
    if ((!o || o.kind === 'menu') && isPrefix(this.config.prefix, k)) {
      this.overlay = { kind: 'keys', group: null };
      this.dirty();
      return true;
    }
    if (o) return this.overlayKey(o, k);
    if (this.todoTyping()) {
      this.todo.key(k, todoWidth(this));
      this.dirty();
      return true;
    }
    if (this.filesTyping()) return this.filesKey(k);
    if (this.filteringChanges()) return this.changesFilterKey(k);
    const t = this.tab();
    const pane = t ? activePane(t) : undefined;
    if (!pane) return false;
    pane.shell.key(k);
    return true;
  }

  private typed(k: Key): string | null {
    return k.key.length === 1 && !k.ctrl && !k.alt ? k.key : null;
  }

  private overlayKey(o: Overlay, k: Key): boolean {
    const ch = this.typed(k);
    if (o.kind === 'menu') {
      if (k.key === 'Escape') this.closeOverlay();
      return true;
    }
    if (o.kind === 'groupStyle' || o.kind === 'usage') {
      if (k.key === 'Escape' || k.key === 'Enter') this.closeOverlay();
      return true;
    }
    if (o.kind === 'restart') {
      if (k.key === 'Escape') this.closeOverlay();
      else if (k.key === 'Enter') this.restartNow();
      else if (k.key === 'ArrowDown' || k.key === 'ArrowUp') this.scrollRestart(k.key === 'ArrowDown' ? 1 : -1);
      return true;
    }
    if (o.kind === 'newWorkspace' || o.kind === 'rename' || o.kind === 'newGroup') {
      if (k.key === 'Escape') this.closeOverlay();
      else if (k.key === 'Enter') this.submitForm();
      else if (o.kind === 'newWorkspace' && o.creating) return true;
      else if (o.kind === 'newWorkspace' && k.key === 'Tab') this.toggleWorktree();
      else if (k.key === 'Backspace') o.input = o.input.slice(0, -1);
      else if (ch) o.input += ch;
      if (o.kind === 'newWorkspace') o.error = undefined;
      this.dirty();
      return true;
    }
    if (this.confirmView()) {
      if (k.key === 'Escape') this.closeOverlay();
      else if (k.key === 'Enter') this.submitConfirm();
      return true;
    }
    if (o.kind === 'search') {
      const results = this.searchResults(o.query);
      if (k.key === 'Escape') this.closeOverlay();
      else if (k.key === 'Enter') this.searchGo(Math.min(o.selected, results.length - 1));
      else if (k.key === 'ArrowDown') o.selected = Math.min(o.selected + 1, Math.max(0, results.length - 1));
      else if (k.key === 'ArrowUp') o.selected = Math.max(0, o.selected - 1);
      else if (k.key === 'Backspace') {
        o.query = o.query.slice(0, -1);
        o.selected = 0;
      } else if (ch) {
        o.query += ch;
        o.selected = 0;
      }
      this.dirty();
      return true;
    }
    if (o.kind === 'picker') {
      const items = this.pickerItems(o);
      if (k.key === 'Escape') this.closeOverlay();
      else if (k.key === 'Enter') this.pickerEnter();
      else if (k.key === 'ArrowDown') o.selected = o.selected === null ? 0 : Math.min(o.selected + 1, items.length - 1);
      else if (k.key === 'ArrowUp') o.selected = o.selected === null ? 0 : Math.max(0, o.selected - 1);
      else if (k.key === 'ArrowLeft') {
        o.dir = o.dir.slice(0, -1);
        o.filter = '';
        o.selected = null;
      } else if (k.key === 'ArrowRight' || k.key === 'Tab') {
        if (o.selected !== null) this.pickerEnter();
      } else if (k.key === 'Backspace') {
        if (o.filter) o.filter = o.filter.slice(0, -1);
        else o.dir = o.dir.slice(0, -1);
        o.selected = o.filter ? 0 : null;
      } else if (ch) {
        if (ch === '/') {
          const match = this.pickerItems(o)[0];
          if (match && o.filter) {
            o.selected = 0;
            this.pickerEnter();
          }
        } else {
          o.filter += ch;
          o.selected = 0;
        }
      }
      this.dirty();
      return true;
    }
    if (o.kind === 'settings') return this.settingsKey(o, k, ch);
    if (o.kind === 'issues') return this.issuesKey(o, k, ch);
    return true;
  }

  private settingsKey(o: SettingsOverlay, k: Key, ch: string | null): boolean {
    if (o.busy) return true;
    if (o.capturing) {
      this.capturePrefix(o, k);
      this.dirty();
      return true;
    }
    if (o.edit) {
      if (k.key === 'Escape') o.edit = undefined;
      else if (k.key === 'Enter') this.submitEdit();
      else if (k.key === 'Backspace') o.edit.input = o.edit.input.slice(0, -1);
      else if (ch) o.edit.input += ch;
      if (o.edit && k.key !== 'Enter') o.edit.error = undefined;
      this.dirty();
      return true;
    }
    if (o.pick) {
      const n = this.pickChoices(o).length;
      if (k.key === 'Escape') o.pick = undefined;
      else if (k.key === 'Enter') this.choosePick(Math.min(o.pick.selected, n - 1));
      else if (k.key === 'ArrowDown') o.pick.selected = Math.min(o.pick.selected + 1, n - 1);
      else if (k.key === 'ArrowUp') o.pick.selected = Math.max(0, o.pick.selected - 1);
      else if (k.key === 'Backspace') o.pick.filter = o.pick.filter.slice(0, -1);
      else if (ch) {
        o.pick.filter += ch;
        o.pick.selected = 0;
      }
      this.dirty();
      return true;
    }
    const rows = this.settingsRows(o.page);
    o.notice = undefined;
    if (k.key === 'Escape') this.closeOverlay();
    else if (k.key === 'Enter' || k.key === ' ') this.settingsActivate();
    else if (k.key === 'Tab' || k.key === 'ArrowRight') this.settingsPage((o.page + (k.shift ? 3 : 1)) % 4);
    else if (k.key === 'ArrowLeft') this.settingsPage((o.page + 3) % 4);
    else if (k.key === 'ArrowDown') o.cursor = Math.min(o.cursor + 1, rows.length - 1);
    else if (k.key === 'ArrowUp') o.cursor = Math.max(0, o.cursor - 1);
    else if ((k.key === 'Delete' || k.key === 'Backspace') && rows[o.cursor]?.id.startsWith('token:')) {
      const source = rows[o.cursor].id.slice(6) as Remote;
      if (this.config.accounts[source]) {
        this.config.accounts[source] = false;
        o.notice = `the ${tokenName(source)} was removed`;
      }
    }
    this.dirty();
    return true;
  }

  private issuesKey(o: IssuesOverlay, k: Key, ch: string | null): boolean {
    if (o.busy) return true;
    const view = this.issuesView(o);
    if (k.key === 'Escape') {
      if (o.agentPick) o.agentPick = null;
      else if (o.detail) o.detail = null;
      else this.closeOverlay();
      this.dirty();
      return true;
    }
    if (view.token) {
      if (k.key === 'Enter') this.issuesConnect();
      else if (k.key === 'Tab' || k.key === 'ArrowRight') this.issuesTab((o.tab + 1) % this.config.sources.length);
      else if (k.key === 'ArrowLeft') this.issuesTab((o.tab + this.config.sources.length - 1) % this.config.sources.length);
      else if (k.key === 'Backspace' || ch) {
        o.token.input = k.key === 'Backspace' ? o.token.input.slice(0, -1) : o.token.input + ch;
        o.token.error = undefined;
      }
      this.dirty();
      return true;
    }
    if (o.detail) {
      if (k.key === 'Enter') this.issuesButton('start');
      else if (k.key === 'ArrowDown') this.issuesScrollDetail(1, (view.detail?.length ?? 0) - 10);
      else if (k.key === 'ArrowUp') this.issuesScrollDetail(-1, 999);
      return true;
    }
    if (o.agentPick) {
      if (k.key === 'Enter') this.issuesButton('choose');
      else if (k.key === 'ArrowDown') o.agentPick.selected = Math.min(o.agentPick.selected + 1, view.items.length - 1);
      else if (k.key === 'ArrowUp') o.agentPick.selected = Math.max(0, o.agentPick.selected - 1);
      else if (k.key === 'Backspace') o.agentPick.filter = o.agentPick.filter.slice(0, -1);
      else if (ch) {
        o.agentPick.filter += ch;
        o.agentPick.selected = 0;
      }
      this.dirty();
      return true;
    }
    if (k.key === 'Enter') this.issuesClick(view.selected);
    else if (k.key === 'ArrowDown') o.selected = Math.min(o.selected + 1, Math.max(0, view.items.length - 1));
    else if (k.key === 'ArrowUp') o.selected = Math.max(0, o.selected - 1);
    else if (k.key === 'Tab' || k.key === 'ArrowRight') this.issuesTab((o.tab + 1) % this.config.sources.length);
    else if (k.key === 'ArrowLeft') this.issuesTab((o.tab + this.config.sources.length - 1) % this.config.sources.length);
    else if (k.key === 'Backspace') o.filter = o.filter.slice(0, -1);
    else if (ch) {
      o.filter += ch;
      o.selected = 0;
    }
    this.dirty();
    return true;
  }

  regionRect(find: (r: Region) => boolean): Rect | null {
    const regions = this.frame?.regions ?? [];
    for (let i = regions.length - 1; i >= 0; i--) if (find(regions[i])) return regions[i].r;
    return null;
  }

  textAt(needle: string, nth = 0): Pos | null {
    let seen = 0;
    for (let y = 0; y < this.rows; y++) {
      const line = this.grid.line(y);
      let from = 0;
      for (;;) {
        const at = line.indexOf(needle, from);
        if (at < 0) break;
        if (seen === nth) return { x: at, y };
        seen += 1;
        from = at + 1;
      }
    }
    return null;
  }

  panesOf(t: Tab, area: Rect): [number, Rect][] {
    return visible(t.layout, area, t.active);
  }
}
