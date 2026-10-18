import type { App } from './app';
import { type Flow, type Message, Progress, TRAVEL } from './flow';
import { type Project, markPane } from './model';
import { Agent } from './programs';

export const ORCHESTRATOR = 'orchestrator';
export const RUN_BRANCH = 'fix/webhook-signature';
export const RUN_DIR = '/home/you/.local/state/cornercase/runs/2026-10-17-fix-webhook-signature';
export const RUN_TASK = 'Check the Stripe signature before a webhook changes a charge.';
const MEMBER_PACE = 1500;
const BETWEEN = 1500;
const POLL = 300;
const AGAIN_AFTER = 20000;
const FRAME = 50;

export interface Run {
  workspace: number;
  flow: Flow;
  task: string;
  members: { role: string; pane: number }[];
  progress: Progress;
  messages: Message[];
  files: RunFile[];
  ended: boolean;
}

export interface RunFile {
  name: string;
  at: number;
  text: string;
}

export const FULL_ISSUE: Flow = {
  name: 'Full issue',
  orchestrator: { name: ORCHESTRATOR, agent: 'claude', model: 'opus' },
  roles: [
    { name: 'coder', agent: 'codex', model: 'gpt-5.4' },
    { name: 'reviewer', agent: 'claude' },
    { name: 'security', agent: 'gemini' },
    { name: 'tester', agent: 'claude' },
  ],
  stages: [
    { roles: ['coder'] },
    { roles: ['reviewer', 'security'], back: { to: 1, when: 'findings', atMost: 3 } },
    { roles: ['tester'], back: { to: 1, when: 'failures', atMost: 2 } },
  ],
  end: 'open a pull request',
};

const SCRIPT: [string, string][][] = [
  [['coder', 'Check the Stripe signature before handling a webhook, then commit.']],
  [
    ['reviewer', 'Review the signature check on this branch against main.'],
    ['security', 'Look for security problems in the signature check.'],
  ],
  [['coder', 'reviewer found 1 problem: compare the signatures in constant time.']],
  [
    ['reviewer', 'Review it again: coder now compares in constant time.'],
    ['security', 'Look for security problems again.'],
  ],
  [['tester', 'Test the webhooks with a signed and a forged event.']],
];

const ARGS: Record<string, string[]> = {
  claude: ['--dangerously-skip-permissions'],
  codex: ['--dangerously-bypass-approvals-and-sandbox'],
  gemini: ['--yolo', '--skip-trust'],
};

function agentOf(app: App, run: Run, role: string): Agent | null {
  const id = run.members.find((m) => m.role === role)?.pane;
  const fg = id === undefined ? null : app.findPane(id)?.pane.shell.fg;
  return fg instanceof Agent ? fg : null;
}

export function startRun(app: App, p: Project): Run {
  const w = app.addWorkspace(p, RUN_BRANCH, true);
  const run: Run = { workspace: w.id, flow: FULL_ISSUE, task: RUN_TASK, members: [], progress: new Progress(), messages: [], files: [], ended: false };
  for (const role of [FULL_ISSUE.orchestrator, ...FULL_ISSUE.roles]) {
    const pane = app.newPane(p, w);
    if (run.members.length) pane.parent = run.members[0].pane;
    const args = [...(ARGS[role.agent] ?? []), ...(role.model ? ['--model', role.model] : [])];
    pane.shell.start(new Agent(app.host(p, w, () => pane.id), () => pane.shell.finish(), role.agent, args, { trusted: true }));
    const tab = app.newTab([pane]);
    tab.name = role.name;
    w.tabs.push(tab);
    run.members.push({ role: role.name, pane: pane.id });
  }
  app.runs.push(run);
  play(app, run);
  return run;
}

export function record(app: App, run: Run, to: string, text: string): void {
  const message = { from: ORCHESTRATOR, to, text, at: app.now() };
  run.messages.push(message);
  run.progress.send(run.flow, message);
  for (let t = FRAME; t <= TRAVEL + FRAME; t += FRAME) app.after(t, () => app.dirty());
  app.dirty();
}

function write(app: App, run: Run, role: string, summary: string): void {
  const n = run.files.filter((f) => f.name.startsWith(`${role}-`)).length + 1;
  run.files.push({ name: `${role}-${n}.md`, at: app.now(), text: `# ${role}, answer ${n}\n\n${summary}\n` });
  app.dirty();
}

function play(app: App, run: Run): void {
  const pane = (role: string) => run.members.find((m) => m.role === role)?.pane ?? 0;
  const lead = () => agentOf(app, run, ORCHESTRATOR);
  const when = (ready: () => boolean, then: () => void) => {
    const check = () => (app.flowPlaying(run) && ready() ? then() : app.after(POLL, check));
    app.after(POLL, check);
  };
  const stage = (i: number) => {
    const sends = SCRIPT[i];
    if (!sends) return finish();
    for (const [role, text] of sends) {
      lead()?.tool('Bash', `cornercase send --pane ${pane(role)} --enter '${text}'`, `sent to ${role}`);
      agentOf(app, run, role)?.submit(text, MEMBER_PACE);
      record(app, run, role, text);
    }
    const panes = sends.map(([role]) => `--pane ${pane(role)}`).join(' ');
    lead()?.tool('Bash', `cornercase wait --all ${panes} --until turn-over --timeout 900`, 'idle');
    when(
      () => sends.every(([role]) => !agentOf(app, run, role)?.working),
      () => {
        const summary = i === 1 ? 'reviewer: 1 finding, security: no findings' : 'done';
        for (const [role, text] of sends) write(app, run, role, role === 'reviewer' && i === 1 ? 'One finding: compare the signatures in constant time.' : `Done: ${text}`);
        lead()?.tool('Bash', `cornercase read --last-message ${panes}`, summary);
        write(app, run, ORCHESTRATOR, `Read ${sends.map(([role]) => role).join(' and ')}: ${summary}.`);
        app.after(BETWEEN, () => when(() => true, () => stage(i + 1)));
      },
    );
  };
  const finish = () => {
    const own = app.findPane(pane(ORCHESTRATOR))?.pane;
    lead()?.tool('Bash', 'gh pr create --fill', 'https://github.com/acme/payments/pull/31');
    lead()?.tool('Bash', `cornercase mark --pane ${pane(ORCHESTRATOR)}`, 'marked as pending review');
    lead()?.end('**Done.** The pull request is open: one round of review fixed the comparison, and the tests pass.', '2m 40s');
    if (own) markPane(own, true);
    app.dirty();
    app.after(AGAIN_AFTER, () =>
      when(
        () => true,
        () => {
          if (own) markPane(own, false);
          run.progress = new Progress();
          run.ended = false;
          run.messages = [];
          run.files = [];
          begin();
        },
      ),
    );
  };
  const begin = () => {
    lead()?.begin(`You are the orchestrator of a team of coding agents that cornercase started for the flow "Full issue". Task: ${RUN_TASK}`);
    lead()?.tool('Bash', `cornercase wait --all ${run.members.slice(1).map((m) => `--pane ${m.pane}`).join(' ')} --until turn-over --timeout 300`, 'all ready');
    app.after(BETWEEN, () => stage(0));
  };
  when(() => true, begin);
}
