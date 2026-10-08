import { BOLD } from '../term/grid';
import { App } from './app';
import { ADDRESS_RS, COMMITS, FOLDERS } from './data';
import { type Pane, type Project, type Workspace, activePane, projectLabel, workspaceLabel } from './model';
import { Agent, Editor } from './programs';
import { type Line, seg } from './text';

export const STORY_PACE = 9000;

function prompt(folder: string, command: string): Line {
  return [seg(folder, { fg: 6, add: BOLD }), seg(' '), seg('❯', { fg: 5 }), seg(` ${command}`)];
}

function gitLog(project: string, branch: string): Line[] {
  return (COMMITS[project] ?? []).map(([hash, msg], i) => [
    seg('* ', { fg: 1 }),
    seg(hash, { fg: 3 }),
    ...(i === 0 ? [seg(' ('), seg('HEAD -> ', { fg: 6, add: BOLD }), seg(branch, { fg: 2, add: BOLD }), seg(')', { fg: 3 })] : []),
    seg(` ${msg}`),
  ]);
}

function tests(root: string): Line[] {
  const head = (s: string) => seg(s.padStart(12), { fg: 2, add: BOLD });
  const ok = (t: string): Line => [seg(`test ${t} ... `), seg('ok', { fg: 2 })];
  return [
    prompt('feat-dark-mode', 'cargo test'),
    [head('Compiling'), seg(` shop v0.5.0 (${root})`)],
    [head('Finished'), seg(' `test` profile [unoptimized + debuginfo] target(s) in 2.41s')],
    [head('Running'), seg(' unittests src/main.rs')],
    [],
    [seg('running 7 tests')],
    ok('checkout::tests::totals_include_tax'),
    ok('checkout::tests::coupons_stack'),
    ok('returns::tests::label_is_printed'),
    ok('theme::tests::dark_background_is_dark'),
    ok('theme::tests::light_is_the_default'),
    ok('returns::address::tests::postcode_is_trimmed'),
    [seg('test returns::address::tests::empty_address_is_rejected ... '), seg('FAILED', { fg: 1 })],
    [],
    [seg('test result: '), seg('FAILED', { fg: 1 }), seg('. 6 passed; 1 failed; 0 ignored; finished in 0.02s')],
  ];
}

function inGroup(app: App, name: string, projects: Project[]): void {
  const group = app.addGroup(name);
  for (const p of projects) p.group = group.id;
}

export function agentTab(app: App, p: Project, w: Workspace, task: string, pace: number, beside: Pane[] = [], kind = 'claude'): Agent {
  const pane = app.newPane(p, w);
  const agent = new Agent(app.host(p, w, () => pane.id), () => pane.shell.finish(), kind, kind === 'claude' ? ['--permission-mode', 'plan'] : [], { working: task, shown: 1, pace });
  pane.shell.start(agent);
  const [right, below] = beside;
  w.tabs.push(
    right && below
      ? app.newTab([pane, right, below], { dir: 'right', ratio: 0.56, first: { leaf: pane.id }, second: { dir: 'down', ratio: 0.62, first: { leaf: right.id }, second: { leaf: below.id } } })
      : app.newTab([pane]),
  );
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

export function world(app: App, opts: { scripted?: boolean } = {}): App {
  const pace = opts.scripted ? STORY_PACE : 2600;
  if (opts.scripted) app.agentPace = STORY_PACE;
  const shop = app.addProject('shop', '~/code/shop', true, FOLDERS.shop.tree);
  const main = shop.workspaces[0];
  const editorPane = main.tabs[0].panes[0];
  editorPane.shell.start(new Editor(app.host(shop, main, () => editorPane.id), () => editorPane.shell.finish(), 'src/returns/address.rs', ADDRESS_RS, 11));
  main.behind = 2;
  main.tabs.push(app.newTab([app.newPane(shop, main, [prompt('shop', 'git log --oneline --graph'), ...gitLog('shop', 'main'), prompt('shop', 'git status -sb'), [seg('## main...origin/main [behind 2]')]])]));
  const dark = app.addWorkspace(shop, 'feat/dark-mode', true);
  const beside = app.cols >= 130 ? [app.newPane(shop, dark, tests(dark.root)), app.newPane(shop, dark, [prompt('feat-dark-mode', 'git status -sb'), [seg('## feat/dark-mode')], [seg(' M ', { fg: 1 }), seg('src/theme.rs')]])] : [];
  agentTab(app, shop, dark, 'https://github.com/acme/shop/issues/479', pace, beside);
  agentTab(app, shop, app.addWorkspace(shop, 'feat/gift-cards', true), 'Add gift cards to the checkout', pace + 600, [], 'codex');
  shop.active = 1;

  const api = app.addProject('api', '~/code/api', true, FOLDERS.api.tree);
  const apiMain = api.workspaces[0];
  apiMain.tabs[0].panes[0].shell.run('npm run dev');
  apiMain.tabs.push(app.newTab([app.newPane(api, apiMain, [prompt('api', 'git log --oneline'), ...gitLog('api', 'main')])]));
  const pagination = agentTab(app, api, app.addWorkspace(api, 'fix/pagination', true), 'Paginate GET /orders with a cursor', pace + 300);
  api.active = 1;
  if (!opts.scripted) app.after(9000, () => pagination.ask('src/orders.ts'));

  const infra = app.addProject('infra', '~/code/infra', true, FOLDERS.infra.tree);
  const notes = app.addProject('notes', '~/code/notes', false, FOLDERS.notes.tree);
  notes.workspaces[0].tabs[0].panes[0].shell.run('cat todo.md');
  inGroup(app, 'acme', [shop, api, infra]);

  app.active = 0;
  return app;
}
