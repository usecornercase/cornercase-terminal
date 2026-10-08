import { type Rect, rect } from '../term/grid';
import type { App } from './app';
import { type Mounted, mount, richText } from './client';
import { isEmpty } from './layout';
import { activePane } from './model';
import { Agent } from './programs';
import { type Player, type Spot, type Step, play } from './tour';
import { agentIn, setPace, world } from './world';

const compact = (app: App) => app.cols < 90;

const open = (nav: 'projects' | 'workspaces'): Step => ({
  run: (app) => {
    if (compact(app)) {
      app.nav = nav;
      app.dirty();
    }
  },
});

const close: Step = {
  run: (app) => {
    if (compact(app)) {
      app.nav = null;
      app.dirty();
    }
  },
};

const activeAgent = (app: App): Agent | null => {
  const tab = app.tab();
  const fg = tab ? activePane(tab)?.shell.fg : null;
  return fg instanceof Agent ? fg : null;
};

const trusting = (app: App): boolean => activeAgent(app)?.trusting ?? false;

const prompted = (app: App): boolean => activeAgent(app)?.pending.includes('https://') ?? false;

const reveal = (scroll: number): Step => ({
  run: (app) => {
    if (compact(app)) app.nav = 'workspaces';
    app.workspacesScroll = scroll;
    app.dirty();
  },
});

export const CHAPTERS: Step[][] = [
  [
    reveal(5),
    { spot: 'workspaces' },
    { say: 'Meet the team: Claude Code and Codex, each in its own corner. Their model and context use sit under the tab name.' },
    { wait: 3800 },
    { spot: null },
    { say: 'Need one more? Pick an issue and press start. It gets its own branch and gets to work.' },
    open('workspaces'),
    { click: 'Issues', nth: -1 },
    { wait: 1200 },
    { click: '#482' },
    { wait: 1600 },
    { click: 'start   agent', dx: 1, orKey: 'Enter' },
    { until: trusting, timeout: 12000 },
    { wait: 1400 },
    { key: 'Enter' },
    { until: prompted, timeout: 12000 },
    { wait: 700 },
    { key: 'Enter' },
    { wait: 1500 },
    { run: (app) => void agentIn(app, 'api', 'fix/pagination')?.ask('src/orders.ts') },
    { wait: 1150 },
    { say: 'Someone needs you. `!` shows who, and your terminal taps you on the shoulder.' },
    { wait: 1900 },
    open('projects'),
    { spot: 'sidebar' },
    { wait: 2200 },
    { spot: null },
    open('projects'),
    { click: 'api (' },
    close,
    { wait: 900 },
    { say: 'One answer and it’s back to work. So are you.' },
    { wait: 1400 },
    { key: 'Enter' },
    { wait: 1600 },
    open('projects'),
    { click: 'shop (' },
    open('workspaces'),
    { click: 'main' },
    close,
    { wait: 1000 },
  ],
  [
    open('projects'),
    { say: 'Now quit. Seriously. Everything keeps running without you.' },
    { click: 'Quit', nth: -1 },
    { run: (app) => void (app.hover = null) },
    { wait: 1500 },
    { say: 'Go grab a coffee. Your agents don’t need you watching.' },
    { lapse: 3600, speed: 16, label: '25 minutes later' },
    { say: 'Back already? Type `cornercase`, here, in another terminal, or from your phone.' },
    { type: 'cornercase' },
    { wait: 400 },
    { key: 'Enter' },
    { wait: 900 },
    reveal(5),
    { spot: 'columns' },
    { say: 'Right where you left it. And `✓` shows who finished while you were out.' },
    { wait: 4200 },
    { spot: null },
    close,
  ],
  [
    reveal(9999),
    { say: 'Curious what it did?' },
    { click: '#482' },
    close,
    { wait: 1700 },
    { say: 'Every change, right next to the agent. Live.' },
    {
      run: (app) => {
        if (!compact(app)) app.widths = { ...app.widths, changes: Math.max(40, Math.min(64, app.cols - 83)) };
      },
    },
    { click: 'Changes', nth: -1, alt: '±' },
    { wait: 500 },
    { spot: 'changes' },
    { wait: 2800 },
    { say: 'Like it? Open it in your editor. Don’t? Send it back to the agent.' },
    { point: 'impl Address', nth: -1 },
    { wait: 3600 },
    { spot: null },
  ],
];

const TURN = 'Your turn. It’s all live: click around, right-click a pane, open settings, or type in a shell.';
const STILL = 'This isn’t a video. Click anything, or press 1 to watch the tour.';

function estimate(steps: Step[]): number {
  return steps.reduce((total, s) => {
    if ('wait' in s) return total + s.wait;
    if ('lapse' in s) return total + s.lapse * s.speed;
    if ('click' in s || 'point' in s) return total + 900;
    if ('type' in s) return total + s.type.length * 65;
    if ('key' in s) return total + 160;
    if ('until' in s) return total + 3000;
    return total;
  }, 0);
}

function spotArea(app: App, spot: Spot): Rect {
  const a = app.areas();
  if (spot === 'sidebar') return a.sidebar;
  if (spot === 'workspaces') return a.workspaces;
  if (spot === 'pane') return a.pane;
  if (spot === 'changes') return a.changes;
  if (a.compact) return isEmpty(a.sidebar) ? a.workspaces : a.sidebar;
  return rect(0, 0, a.workspaces.x + a.workspaces.w, app.rows);
}

async function hero(root: HTMLElement): Promise<void> {
  const pointer = root.querySelector<HTMLElement>('.pointer');
  const caption = root.querySelector<HTMLElement>('[data-caption]');
  const spotlight = root.querySelector<HTMLElement>('.spot');
  const badge = root.querySelector<HTMLElement>('.lapse');
  const badgeText = root.querySelector<HTMLElement>('[data-lapse]');
  const replay = root.querySelector<HTMLButtonElement>('[data-replay]');
  const chips = [...root.querySelectorAll<HTMLButtonElement>('[data-chapter]')];
  let m: Mounted;
  let player: Player | null = null;
  let touring = false;
  let touched = false;
  let token = 0;
  let target: Spot | null = null;
  let progress: { start: number; length: number; chip: HTMLElement } | null = null;

  const say = (text: string) => {
    if (caption) caption.innerHTML = richText(text);
  };
  const chapter = (i: number) =>
    chips.forEach((chip, k) => {
      chip.dataset.state = k < i ? 'done' : k === i ? 'active' : 'todo';
      chip.style.setProperty('--p', '0');
    });
  const place = () => {
    if (!spotlight || !m) return;
    const area = target && !m.app.detached ? spotArea(m.app, target) : null;
    if (!area || isEmpty(area)) {
      spotlight.hidden = true;
      return;
    }
    const b = m.box(area);
    spotlight.hidden = false;
    spotlight.style.left = `${b.left - 3}px`;
    spotlight.style.top = `${b.top - 3}px`;
    spotlight.style.width = `${b.width + 6}px`;
    spotlight.style.height = `${b.height + 6}px`;
  };
  const spot = (s: Spot | null) => {
    target = s;
    place();
  };
  const lapse = (speed: number, label: string | null) => {
    m.speed(speed);
    if (badge) badge.hidden = !label;
    if (badgeText && label) badgeText.textContent = label;
  };
  const make = async (scripted: boolean) => {
    const made = await mount(root, {
      rows: (w) => (w < 560 ? 34 : w < 900 ? 30 : 32),
      cell: (w) => (w < 560 ? 7.4 : w < 900 ? 7.8 : 9.6),
      clock: true,
      build: (app) => {
        app.scripted = scripted;
        world(app, { scripted });
      },
      narrate: (html) => {
        if (!touring && caption) caption.innerHTML = html;
      },
      painted: () => place(),
    });
    (window as unknown as { demo?: Mounted }).demo = made;
    return made;
  };
  let mounting = Promise.resolve();
  const remount = (scripted: boolean) => {
    const next = mounting.then(async () => {
      m.destroy();
      m = await make(scripted);
    });
    mounting = next.catch(() => {});
    return next;
  };
  const loosen = () => {
    m.app.scripted = false;
    m.app.hover = null;
    m.app.dirty();
    setPace(m.app, 900);
  };
  const stop = () => {
    token += 1;
    player?.stop();
    player = null;
    progress = null;
  };
  const free = (text = TURN) => {
    stop();
    touring = false;
    spot(null);
    lapse(1, null);
    loosen();
    chapter(3);
    say(text);
    if (replay) replay.hidden = false;
  };
  const tour = async (from: number) => {
    stop();
    const mine = token;
    touring = true;
    if (replay) replay.hidden = true;
    spot(null);
    lapse(1, null);
    chapter(from);
    await remount(true);
    if (mine !== token) {
      if (!touring) loosen();
      return;
    }
    for (let i = 0; i < from && mine === token; i++) await play(m, null, CHAPTERS[i], { instant: true }).done;
    for (let i = from; i < CHAPTERS.length; i++) {
      if (mine !== token) return;
      chapter(i);
      progress = { start: m.app.now(), length: estimate(CHAPTERS[i]), chip: chips[i] };
      player = play(m, pointer, CHAPTERS[i], { say, spot, lapse });
      await player.done;
    }
    if (mine === token) free();
  };

  m = await make(false);
  root.dataset.ready = 'true';

  const frame = () => {
    requestAnimationFrame(frame);
    if (progress) progress.chip.style.setProperty('--p', String(Math.min(0.97, (m.app.now() - progress.start) / progress.length)));
  };
  requestAnimationFrame(frame);

  root.addEventListener('demo-interact', () => {
    touched = true;
    if (touring) free();
  });
  chips.forEach((chip, i) =>
    chip.addEventListener('click', () => {
      touched = true;
      if (i < CHAPTERS.length) tour(i);
      else if (touring) free();
      else {
        chapter(i);
        say(TURN);
      }
    }),
  );
  replay?.addEventListener('click', () => tour(0));

  if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
    say(STILL);
    return;
  }
  const seen = new IntersectionObserver(
    ([entry]) => {
      if (!entry.isIntersecting) return;
      seen.disconnect();
      setTimeout(() => {
        if (!touched) tour(0);
      }, 500);
    },
    { threshold: 0.35 },
  );
  seen.observe(root);
}

export function boot(): void {
  for (const root of document.querySelectorAll<HTMLElement>('[data-demo]')) {
    if (root.dataset.booted) continue;
    root.dataset.booted = 'true';
    hero(root);
  }
}
