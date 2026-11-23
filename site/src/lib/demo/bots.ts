import type { App } from './app';
import { type BotsShape, marksWidth } from './layout';
import type { Key } from './programs';
import { plain } from './markdown';
import { type Editable, editKey, removeSelection } from './todo';

export type LineKind = 'you' | 'bot' | 'step' | 'error' | 'notice' | 'started' | 'finished' | 'waiting';
export type Access = 'cornercase' | 'full';

export interface ChatLine {
  id: number;
  kind: LineKind;
  text: string;
  at: number;
  routine?: number;
  agent?: { kind: string; pane: number };
  body?: string;
}

export interface Turn {
  setup?: boolean;
  since: number;
  steps: number;
  stopping: boolean;
}

export interface Bot {
  id: string;
  name: string;
  agent: string;
  model: string;
  effort: string;
  account: string;
  access: Access;
  about: string;
  made: number;
  lines: ChatLine[];
  unread: number;
  turn: Turn | null;
  held: Set<number>;
  draft: Editable;
  scroll: number;
  below: boolean;
  anchor: number | null;
  open: Set<number>;
  timers: (() => void)[];
}

export interface AgentChoice {
  title: string;
  kind: string;
  why: string | null;
  models: string[];
  accesses: Access[];
  limit: string | null;
  reads?: 'folder';
}

export const BOT_AGENTS: AgentChoice[] = [
  { title: 'Claude Code', kind: 'claude', why: null, models: ['opus', 'sonnet', 'haiku'], accesses: ['cornercase', 'full'], limit: null },
  { title: 'Codex', kind: 'codex', why: null, models: ['gpt-5.4', 'gpt-5.4-mini'], accesses: ['full'], limit: 'its sandbox cannot reach cornercase' },
  { title: 'Gemini CLI', kind: 'gemini', why: null, models: [], accesses: ['cornercase', 'full'], limit: null, reads: 'folder' },
  { title: 'opencode', kind: 'opencode', why: null, models: [], accesses: ['full'], limit: 'a command can write outside its folder' },
  { title: 'Grok Build', kind: 'grok', why: 'not yet', models: [], accesses: [], limit: null },
];

export const NO_ACCESS_WHY = 'its sandbox cannot reach cornercase';
export const MAX_BOTS = 50;
export const EXAMPLES = [
  'Which of my open pull requests are blocked, and why?',
  'What did my agents get done today, in every project?',
  'Which issues labelled bug came in today, in every project?',
  'Every weekday at 9, tell me which of my PRs are blocked.',
];
export const SETTING_UP = 'setting up';
export const IDEAS: [string, string][] = [
  ['PRs', 'Every weekday at 9:00, review my open pull requests and tell me which are blocked'],
  ['News', 'Every morning at 8:00, sum up the news about the tools I use'],
  ['Releases', 'Every Monday, check the new releases of the tools I use and tell me what changed'],
];
export type Lobby = 'empty' | 'pick';
const ANSWER_AFTER = [1400, 2600, 4200];
const SETUP_AFTER = [1200, 2400, 3600];
const SETUP_STEPS = [
  'cornercase queue list --bot {id} --json',
  "cornercase queue add --every 'day 09:00' --name 'Daily check' --prompt 'Look through my projects and tell me what needs me.'",
];
const SETUP_PROMPT = 'Look through my projects and tell me what needs me.';
const TICK = 500;
const FOLDER = '~/.local/state/cornercase/bots';
const MINUTE = 60000;
const RUN_ROUTINE = 1;
const RUN = 2;

const PRS_ANSWER = [
  'Two are **blocked**:',
  '',
  '- #482 waits for a review from Ana',
  '- #490 fails `test (macos)`',
  '',
  'To retry the failing job:',
  '',
  '```sh',
  'gh run rerun --failed',
  '```',
].join('\n');

const SETUP_ANSWER = [
  'Set up: every day at **09:00** I will look through your projects and tell you here what needs you.',
  '',
  '- Scheduled **Daily check**, every day 09:00',
  '- Wrote what each run needs in `memory.md`',
  '',
  'Nothing needed from you for now.',
].join('\n');

const DIGEST_ANSWER = ['Yesterday in **web-shop**:', '', '- 4 pull requests merged, #476 the last', '- `feat/gift-cards` still waits for tests'].join('\n');

const GENERIC_ANSWER = [
  'Here is what I found across your projects:',
  '',
  '- **web-shop**: two agents working, one waiting for you',
  '- **orders-api**: `fix/pagination` needs the column the cursor sorts by',
  '',
  'Want me to start an agent on one of them?',
].join('\n');

export const slug = (name: string): string => name.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '') || 'bot';

export const pasted = (f: Editable, text: string): void => {
  removeSelection(f);
  const chars = [...text.replace(/\r\n?/g, '\n').replace(/\t/g, ' ')];
  f.chars.splice(f.cursor, 0, ...chars);
  f.cursor += chars.length;
};

export const draftOf = (text = ''): Editable => ({ chars: [...text], cursor: [...text].length, anchor: null });

export function clock(at: number): string {
  const now = Date.now();
  const d = new Date(at);
  const pad = (n: number) => String(n).padStart(2, '0');
  const hm = `${pad(d.getHours())}:${pad(d.getMinutes())}`;
  if (d.toDateString() === new Date(now).toDateString()) return hm;
  const names = d.toLocaleDateString('en', { weekday: 'short', day: 'numeric', month: 'short' }).replace(',', '');
  return `${names} ${hm}`;
}

export function shortClock(at: number): string {
  const d = new Date(at);
  const pad = (n: number) => String(n).padStart(2, '0');
  const hm = `${pad(d.getHours())}:${pad(d.getMinutes())}`;
  const start = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const days = Math.round((start(d) - start(new Date())) / 86400000);
  if (days === 0) return hm;
  if (days > 0 && days < 7) return `${d.toLocaleDateString('en', { weekday: 'short' })} ${hm}`;
  return `${d.getDate()} ${d.toLocaleDateString('en', { month: 'short' })} ${hm}`;
}

export function age(since: number, now: number): string {
  const s = Math.max(0, Math.floor((now - since) / 1000));
  return s < 60 ? `${s}s` : `${Math.floor(s / 60)}m ${String(s % 60).padStart(2, '0')}s`;
}

export const details = (b: Bot): string => [b.agent, b.model, b.effort, b.account].filter(Boolean).join(' · ');
export const summary = (b: Bot): string => [b.agent, b.model].filter(Boolean).join(' · ');

export class Bots {
  list: Bot[] = [];
  shown: string | null = null;
  page = false;
  last: string | null = null;
  scroll = 0;
  chatRows = 10;
  private next = 1;

  constructor(private readonly app: App) {}

  shape(): BotsShape {
    return { offered: true, marks: marksWidth(this.thinking(), this.unread()), page: this.page };
  }

  lobby(): Lobby | null {
    if (!this.page || this.current()) return null;
    return this.list.length ? 'pick' : 'empty';
  }

  open(compact: boolean): void {
    const last = this.list.find((b) => b.id === this.last) ?? this.list[0];
    if (last && !compact) return this.show(last.id);
    this.shown = null;
    this.page = true;
    this.app.files.full = false;
    if (compact) this.app.nav = this.list.length ? 'bots' : null;
    this.app.dirty();
  }

  current(): Bot | null {
    return this.list.find((b) => b.id === this.shown) ?? null;
  }

  unread(): number {
    return this.list.reduce((n, b) => n + b.unread, 0);
  }

  thinking(): boolean {
    return this.list.some((b) => b.turn);
  }

  create(init: Pick<Bot, 'name' | 'agent' | 'model' | 'access' | 'about'> & Partial<Pick<Bot, 'effort' | 'account'>>): Bot {
    const bot: Bot = {
      id: slug(init.name),
      effort: '',
      account: '',
      ...init,
      made: Date.now(),
      lines: [],
      unread: 0,
      turn: null,
      held: new Set(),
      draft: draftOf(),
      scroll: 0,
      below: false,
      anchor: null,
      open: new Set(),
      timers: [],
    };
    this.list.push(bot);
    return bot;
  }

  add(bot: Bot, at: number, kind: LineKind, text: string, routine?: number, agent?: { line: ChatLine['agent']; body?: string }): ChatLine {
    const line: ChatLine = {
      id: this.next++,
      kind,
      text,
      at,
      ...(routine === undefined ? {} : { routine }),
      ...(agent?.line ? { agent: agent.line } : {}),
      ...(agent?.body === undefined ? {} : { body: agent.body }),
    };
    bot.lines.push(line);
    if (bot.scroll > 0) bot.below = true;
    return line;
  }

  folder(bot: Bot): string {
    return `${FOLDER}/${bot.id}`;
  }

  named(name: string): boolean {
    const lower = name.trim().toLowerCase();
    return this.list.some((b) => b.name.toLowerCase() === lower);
  }

  nextRun(bot: Bot): number | null {
    const times = this.app.botQueued.filter((e) => e.bot === bot.id && !e.routine && e.state === 'pending' && !e.paused).map((e) => e.at);
    return times.length ? Math.min(...times) : null;
  }

  show(id: string, run?: number): void {
    const bot = this.list.find((b) => b.id === id);
    if (!bot) return;
    this.shown = id;
    this.page = true;
    this.last = id;
    bot.unread = 0;
    bot.scroll = 0;
    bot.below = false;
    bot.anchor = run ?? null;
    this.app.nav = null;
    this.app.files.full = false;
    this.app.dirty();
  }

  hide(): void {
    this.shown = null;
    this.page = false;
  }

  scrollTo(bot: Bot, delta: number, max: number): boolean {
    const next = Math.max(0, Math.min(max, Math.min(bot.scroll, max) + delta));
    if (next === bot.scroll) return false;
    bot.scroll = next;
    if (!next) bot.below = false;
    this.app.dirty();
    return true;
  }

  send(bot: Bot, text: string): void {
    const clean = text.trim();
    if (!clean) return;
    const line = this.add(bot, Date.now(), 'you', clean);
    bot.draft = draftOf();
    bot.scroll = 0;
    bot.below = false;
    if (bot.turn) bot.held.add(line.id);
    else this.start(bot);
    this.app.dirty();
  }

  setup(bot: Bot): void {
    if (bot.turn || !bot.about.trim()) return;
    this.add(bot, Date.now(), 'notice', SETTING_UP);
    this.start(bot, true);
    this.app.dirty();
  }

  retry(bot: Bot): void {
    const last = [...bot.lines].reverse().find((l) => !l.routine && (l.kind === 'you' || (l.kind === 'notice' && l.text === SETTING_UP)));
    if (!last || bot.turn) return;
    if (last.kind === 'notice') this.setup(bot);
    else this.send(bot, last.text);
  }

  example(bot: Bot, text: string): void {
    bot.draft = draftOf(text);
    this.app.dirty();
  }

  newConversation(bot: Bot): void {
    if (bot.turn) return this.app.notify(`${bot.name} is answering: stop it first`);
    this.add(bot, Date.now(), 'notice', 'new conversation');
    bot.scroll = 0;
    this.app.dirty();
  }

  stop(bot: Bot): void {
    if (!bot.turn || bot.turn.stopping) return;
    bot.turn.stopping = true;
    this.clear(bot);
    this.app.after(700, () => {
      bot.turn = null;
      bot.held.clear();
      this.add(bot, Date.now(), 'notice', 'stopped');
      this.app.dirty();
    });
    this.app.dirty();
  }

  private clear(bot: Bot): void {
    for (const cancel of bot.timers) cancel();
    bot.timers = [];
  }

  private answer(bot: Bot): string {
    if (bot.id === 'prs') return PRS_ANSWER;
    return bot.id === 'digest' ? DIGEST_ANSWER : GENERIC_ANSWER;
  }

  private start(bot: Bot, setup = false): void {
    const app = this.app;
    bot.turn = { since: app.now(), steps: 0, stopping: false, ...(setup ? { setup } : {}) };
    const after = setup ? SETUP_AFTER : ANSWER_AFTER;
    const steps = setup ? SETUP_STEPS.map((s) => s.replace('{id}', bot.id)) : ['cornercase status --json', 'gh pr list --json number,title,reviewDecision'];
    steps.forEach((text, i) => {
      bot.timers.push(
        app.after(after[i], () => {
          if (!bot.turn) return;
          bot.turn.steps += 1;
          this.add(bot, Date.now(), 'step', text);
          app.dirty();
        }),
      );
    });
    bot.timers.push(app.after(after[2], () => this.finish(bot)));
    this.tick(bot);
    app.dirty();
  }

  private scheduled(bot: Bot): void {
    const app = this.app;
    const next = new Date();
    next.setHours(9, 0, 0, 0);
    if (next.getTime() <= Date.now()) next.setDate(next.getDate() + 1);
    const base = queueBase(bot, 'Daily check', SETUP_PROMPT);
    app.botQueued.push({ ...base, id: app.nextQueueId++, at: next.getTime(), repeat: 'day 09:00', state: 'pending' });
    app.botQueued.sort((a, b) => a.at - b.at || a.id - b.id);
  }

  private tick(bot: Bot): void {
    this.app.after(TICK, () => {
      if (!bot.turn) return;
      this.app.dirty();
      this.tick(bot);
    });
  }

  private finish(bot: Bot): void {
    const app = this.app;
    const setup = bot.turn?.setup ?? false;
    bot.turn = null;
    bot.timers = [];
    if (setup) this.scheduled(bot);
    this.add(bot, Date.now(), 'bot', setup ? SETUP_ANSWER : this.answer(bot));
    if (this.shown === bot.id) bot.unread = 0;
    else {
      bot.unread += 1;
      const text = bot.lines[bot.lines.length - 1].text;
      const first = plain(text).split('\n').map((l) => l.trim()).find(Boolean) ?? text.split('\n')[0];
      app.notify(`${bot.name}: ${first}`, undefined, () => this.show(bot.id));
    }
    const held = bot.lines.filter((l) => bot.held.has(l.id));
    bot.held.clear();
    if (held.length) this.start(bot);
    app.dirty();
  }

  key(k: Key, width: number): boolean {
    const bot = this.current();
    if (!bot) return false;
    const draft = bot.draft;
    if (k.key === 'Enter' && !k.shift && !k.ctrl && !k.alt) this.send(bot, draft.chars.join(''));
    else if (k.key === 'PageUp' || k.key === 'PageDown') {
      const page = Math.max(1, this.chatRows - 1);
      bot.scroll = Math.max(0, bot.scroll + (k.key === 'PageUp' ? page : -page));
      if (!bot.scroll) bot.below = false;
    } else if (k.key === 'Escape') {
      if (draft.chars.length) bot.draft = draftOf();
      else if (this.app.panelShown()) this.app.closePanel();
    } else editKey(draft, k, width);
    this.app.dirty();
    return true;
  }

  paste(text: string): void {
    const bot = this.current();
    if (!bot) return;
    pasted(bot.draft, text);
    this.app.dirty();
  }
}

export function seedBots(app: App): void {
  const bots = app.bots;
  const now = Date.now();
  const prs = bots.create({ name: 'prs', agent: 'claude', model: 'opus', effort: 'high', account: 'home', access: 'cornercase', about: 'Review my open pull requests and tell me which are blocked' });
  const ran = now - 30 * MINUTE;
  bots.add(prs, ran, 'notice', `↻ Review PRs · ${clock(ran)}`, RUN);
  bots.add(prs, ran + MINUTE, 'step', 'cornercase status --json', RUN);
  bots.add(prs, ran + MINUTE, 'step', 'gh pr list --json number,title,reviewDecision', RUN);
  bots.add(prs, ran + 2 * MINUTE, 'bot', PRS_ANSWER, RUN);
  bots.add(prs, now - 9 * MINUTE, 'you', 'which of my pull requests are blocked?');
  bots.add(prs, now - 8 * MINUTE, 'step', 'cornercase status --json');
  bots.add(prs, now - 8 * MINUTE, 'step', 'gh pr checks 490');
  bots.add(prs, now - 7 * MINUTE, 'bot', PRS_ANSWER);
  bots.add(prs, now - 5 * MINUTE, 'notice', 'new conversation');
  const fixed = { kind: 'claude', pane: 0 };
  bots.add(prs, now - 4 * MINUTE, 'you', 'start an agent to fix the failing test of #490');
  bots.add(prs, now - 4 * MINUTE + 5000, 'step', 'cornercase start --project 1 --worktree fix/macos-test --prompt "Fix the failing test (macos) of #490"');
  bots.add(prs, now - 4 * MINUTE + 8000, 'started', '▶ started claude in web-shop › fix/macos-test', undefined, { line: fixed });
  bots.add(prs, now - 4 * MINUTE + 12000, 'bot', "Started Claude Code on `fix/macos-test`. I'll tell you when it's done.");
  bots.add(prs, now - 2 * MINUTE, 'finished', '✓ claude finished in web-shop › fix/macos-test', undefined, {
    line: fixed,
    body: 'Fixed the flaky path in `tests/paths.rs` and pushed `fix/macos-test`.\nAll 214 tests pass on macOS.',
  });
  bots.add(prs, now - 2 * MINUTE + 6000, 'bot', '#490 passes `test (macos)` now. Nothing else is blocking it: it needs one review.');
  const digest = bots.create({ name: 'digest', agent: 'codex', model: '', access: 'full', about: 'A short digest of what merged yesterday' });
  bots.add(digest, now - 2 * 60 * MINUTE, 'you', 'what merged yesterday?');
  bots.add(digest, now - 2 * 60 * MINUTE, 'step', 'cornercase status --json');
  bots.add(digest, now - 2 * 60 * MINUTE, 'bot', DIGEST_ANSWER);
  digest.unread = 1;
}

function queueBase(bot: Bot, label: string, prompt: string) {
  return { prompt, label, project: 0, folder: '', workspace: null, branch: '', agent: bot.agent, mode: '', command: '', bot: bot.id };
}

export function seedBotQueue(app: App): void {
  const prs = app.bots.list.find((b) => b.id === 'prs');
  if (!prs) return;
  const ran = prs.lines[0].at;
  const base = queueBase(prs, 'Review PRs', 'Review my open pull requests and tell me which are blocked.');
  app.nextQueueId = RUN + 1;
  app.botQueued.push({ ...base, id: RUN_ROUTINE, at: ran + 120 * MINUTE, repeat: '2h', state: 'pending' });
  app.botQueued.push({ ...base, id: RUN, at: ran, routine: RUN_ROUTINE, state: 'answered' });
  app.botQueued.sort((a, b) => a.at - b.at || a.id - b.id);
}
