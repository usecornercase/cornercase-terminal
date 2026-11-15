import type { App } from './app';
import type { BotsShape } from './layout';
import type { Key } from './programs';
import { type Editable, editKey, removeSelection } from './todo';

export type LineKind = 'you' | 'bot' | 'step' | 'error' | 'notice';
export type Access = 'cornercase' | 'full';

export interface ChatLine {
  id: number;
  kind: LineKind;
  text: string;
  at: number;
  routine?: number;
}

export interface BotRoutine {
  id: number;
  label: string;
  repeat: string;
  at: number;
}

export interface Turn {
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
  routines: BotRoutine[];
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
}

export const BOT_AGENTS: AgentChoice[] = [
  { title: 'Claude Code', kind: 'claude', why: null, models: ['opus', 'sonnet', 'haiku'], accesses: ['cornercase', 'full'], limit: null },
  { title: 'Codex', kind: 'codex', why: null, models: ['gpt-5.4', 'gpt-5.4-mini'], accesses: ['full'], limit: 'its sandbox cannot reach cornercase' },
  { title: 'Gemini CLI', kind: 'gemini', why: null, models: [], accesses: ['cornercase', 'full'], limit: null },
  { title: 'opencode', kind: 'opencode', why: null, models: [], accesses: ['full'], limit: 'its rules let a command write outside its folder' },
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
const ANSWER_AFTER = [1400, 2600, 4200];
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
  folded = false;
  scroll = 0;
  private next = 1;

  constructor(private readonly app: App) {}

  shape(): BotsShape | null {
    return this.list.length ? { rows: this.list.length, folded: this.folded } : null;
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
      routines: [],
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

  add(bot: Bot, at: number, kind: LineKind, text: string, routine?: number): ChatLine {
    const line: ChatLine = { id: this.next++, kind, text, at, ...(routine === undefined ? {} : { routine }) };
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
    return bot.routines.length ? Math.min(...bot.routines.map((r) => r.at)) : null;
  }

  show(id: string, run?: number): void {
    const bot = this.list.find((b) => b.id === id);
    if (!bot) return;
    this.shown = id;
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
  }

  toggleFold(): void {
    this.folded = !this.folded;
    this.app.dirty();
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

  retry(bot: Bot): void {
    const last = [...bot.lines].reverse().find((l) => l.kind === 'you');
    if (last && !bot.turn) this.send(bot, last.text);
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

  private start(bot: Bot): void {
    const app = this.app;
    bot.turn = { since: app.now(), steps: 0, stopping: false };
    const steps = ['cornercase status --json', 'gh pr list --json number,title,reviewDecision'];
    steps.forEach((text, i) => {
      bot.timers.push(
        app.after(ANSWER_AFTER[i], () => {
          if (!bot.turn) return;
          bot.turn.steps += 1;
          this.add(bot, Date.now(), 'step', text);
          app.dirty();
        }),
      );
    });
    bot.timers.push(app.after(ANSWER_AFTER[2], () => this.finish(bot)));
    this.tick(bot);
    app.dirty();
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
    bot.turn = null;
    bot.timers = [];
    this.add(bot, Date.now(), 'bot', this.answer(bot));
    if (this.shown === bot.id) bot.unread = 0;
    else {
      bot.unread += 1;
      app.notify(`${bot.name}: ${bot.lines[bot.lines.length - 1].text.split('\n')[0]}`, undefined, () => this.show(bot.id));
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
    else if (k.key === 'Escape') {
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
  prs.routines.push({ id: RUN_ROUTINE, label: 'Review PRs', repeat: '2h', at: ran + 120 * MINUTE });
  bots.add(prs, ran, 'notice', `↻ Review PRs · ${clock(ran)}`, RUN);
  bots.add(prs, ran + MINUTE, 'step', 'cornercase status --json', RUN);
  bots.add(prs, ran + MINUTE, 'step', 'gh pr list --json number,title,reviewDecision', RUN);
  bots.add(prs, ran + 2 * MINUTE, 'bot', PRS_ANSWER, RUN);
  bots.add(prs, now - 9 * MINUTE, 'you', 'which of my pull requests are blocked?');
  bots.add(prs, now - 8 * MINUTE, 'step', 'cornercase status --json');
  bots.add(prs, now - 8 * MINUTE, 'step', 'gh pr checks 490');
  bots.add(prs, now - 7 * MINUTE, 'bot', PRS_ANSWER);
  bots.add(prs, now - 5 * MINUTE, 'notice', 'new conversation');
  const digest = bots.create({ name: 'digest', agent: 'codex', model: '', access: 'full', about: 'A short digest of what merged yesterday' });
  bots.add(digest, now - 2 * 60 * MINUTE, 'you', 'what merged yesterday?');
  bots.add(digest, now - 2 * 60 * MINUTE, 'step', 'cornercase status --json');
  bots.add(digest, now - 2 * 60 * MINUTE, 'bot', DIGEST_ANSWER);
  digest.unread = 1;
}

export function seedBotQueue(app: App): void {
  const prs = app.bots.list.find((b) => b.id === 'prs');
  const routine = prs?.routines[0];
  if (!prs || !routine) return;
  const base = { prompt: 'Review my open pull requests and tell me which are blocked.', label: routine.label, project: 0, folder: '', workspace: null, branch: '', agent: prs.agent, mode: '', command: '', bot: prs.id };
  app.nextQueueId = RUN + 1;
  app.queued.push({ ...base, id: RUN_ROUTINE, at: routine.at, repeat: routine.repeat, state: 'pending' });
  app.queued.push({ ...base, id: RUN, at: prs.lines[0].at, routine: RUN_ROUTINE, state: 'answered' });
  app.queued.sort((a, b) => a.at - b.at || a.id - b.id);
}
