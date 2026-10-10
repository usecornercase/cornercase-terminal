import { App } from './app';
import { FOLDERS } from './data';
import { type Project, type Workspace, activePane, projectLabel, workspaceLabel } from './model';
import { Agent } from './programs';

export const STORY_PACE = 9000;
export const POSTER_PACE = 600000;
export const DARK_QUESTION = 'Where does the checkout pick its colours?';
export const DARK_PROMPT = 'Use Theme everywhere. Keep the brand purple.';
const STACK_ROWS = 14;

function inGroup(app: App, name: string, projects: Project[]): void {
  const group = app.addGroup(name);
  for (const p of projects) p.group = group.id;
}

export interface AgentOptions {
  kind?: string;
  shown?: number;
  history?: string[];
  args?: string[];
}

export function agentTab(app: App, p: Project, w: Workspace, task: string, pace: number, opts: AgentOptions = {}): Agent {
  const kind = opts.kind ?? 'claude';
  const args = opts.args ?? (kind === 'claude' ? ['--permission-mode', 'plan'] : []);
  const pane = app.newPane(p, w);
  const agent = new Agent(app.host(p, w, () => pane.id), () => pane.shell.finish(), kind, args, { working: task, shown: opts.shown ?? 1, pace, history: opts.history ?? [] });
  pane.shell.start(agent);
  w.tabs.push(app.newTab([pane]));
  return agent;
}

export function agentIn(app: App, project: string, workspace: string): Agent | null {
  const p = app.projects.find((x) => projectLabel(x) === project);
  const w = p?.workspaces.find((x) => workspaceLabel(x) === workspace);
  for (const t of w?.tabs ?? []) {
    const fg = activePane(t)?.shell.fg;
    if (fg instanceof Agent) return fg;
  }
  return null;
}

export function setPace(app: App, pace: number): void {
  app.agentPace = pace;
  for (const p of app.projects)
    for (const w of p.workspaces)
      for (const t of w.tabs)
        for (const pane of t.panes) if (pane.shell.fg instanceof Agent) pane.shell.fg.setPace(pace);
}

function shellIn(app: App, name: string, command: string): Project {
  const p = app.addProject(name, `~/code/${name}`, FOLDERS[name].repo, FOLDERS[name].tree);
  p.workspaces[0].tabs[0].panes[0].shell.run(command);
  return p;
}

export function world(app: App, opts: { scripted?: boolean } = {}): App {
  const pace = opts.scripted ? STORY_PACE : 2600;
  const slow = opts.scripted ? STORY_PACE : POSTER_PACE;
  if (opts.scripted) app.agentPace = STORY_PACE;
  const shop = app.addProject('web-shop', '~/code/web-shop', true, FOLDERS['web-shop'].tree);
  shop.workspaces.splice(0, 1);
  agentTab(app, shop, app.addWorkspace(shop, 'feat/dark-mode', true), DARK_PROMPT, slow, { shown: 3, history: [DARK_QUESTION], args: ['--permission-mode', 'acceptEdits'] });
  agentTab(app, shop, app.addWorkspace(shop, 'feat/gift-cards', true), 'Add gift cards to the checkout', slow, { shown: 2 });
  if (!opts.scripted) agentTab(app, shop, app.addWorkspace(shop, 'fix/empty-address', true), 'Fix the crash on an empty address', slow, { shown: 2 });
  shop.active = 0;

  const api = shellIn(app, 'orders-api', 'npm run dev');
  const pagination = agentTab(app, api, app.addWorkspace(api, 'fix/pagination', true), 'Paginate GET /orders with a cursor', pace + 300, { kind: 'codex' });
  api.active = 1;
  if (!opts.scripted) app.after(9000, () => pagination.ask('src/orders.ts'));
  const mobile = shellIn(app, 'mobile-app', 'git status');
  const payments = shellIn(app, 'payments', 'git log --oneline');

  const blog = shellIn(app, 'blog', 'git status');
  agentTab(app, blog, blog.workspaces[0], 'Fix the typos in the post on worktrees', pace, { shown: 3 });
  const dotfiles = shellIn(app, 'dotfiles', 'git log --oneline');
  const playground = shellIn(app, 'playground', 'ls');
  inGroup(app, 'work', [shop, api, mobile, payments]);
  inGroup(app, 'personal', [blog, dotfiles, playground]);

  app.active = 0;
  app.widths = { ...app.widths, stack: STACK_ROWS };
  return app;
}
