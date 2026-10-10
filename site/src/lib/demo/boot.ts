import { type Rect, rect } from '../term/grid';
import { type Theme, theme as defaultPalette, themes } from '../term/palette';
import { App } from './app';
import { type Mounted, mount, richText } from './client';
import { type Areas, isEmpty } from './layout';
import { activePane, type Pos } from './model';
import { Agent } from './programs';
import { posterState } from './scenes';
import { type Player, type Spot, type Step, play } from './tour';
import { agentIn, setPace, world } from './world';

const compact = (app: App) => app.cols < 90;

const viewport = () => window.innerWidth;

const open = (nav: 'projects' | 'workspaces'): Step => ({
  run: (app) => {
    if (compact(app)) {
      app.nav = nav;
      app.changesOpen = false;
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

const reveal: Step = {
  run: (app) => {
    if (compact(app)) app.nav = 'workspaces';
    app.workspacesScroll = 0;
    app.dirty();
  },
};

const show = (text: string): Step => ({
  run: (app) => {
    for (let i = 0; i < 12 && !app.textAt(text); i++) {
      app.workspacesScroll += 1;
      app.dirty();
      app.render();
    }
  },
});

const fixed = (app: App): boolean => app.projects.some((p) => p.workspaces.some((w) => !!w.flags.fixed));
const quiet = (app: App): boolean => !app.toast;

const changesWidth: Step = {
  run: (app) => {
    if (!compact(app)) app.widths = { ...app.widths, changes: Math.max(40, Math.min(64, app.cols - 83)) };
  },
};

export const CHAPTERS: Step[][] = [
  [
    reveal,
    { say: 'Pick an issue and press start. It gets its own branch and gets to work.' },
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
  ],
  [
    reveal,
    { spot: 'columns' },
    { say: 'Meet the team: Claude Code and Codex, each in its own corner. Their model and context use sit under the tab name.' },
    { wait: 3800 },
    { spot: null },
    { run: (app) => void agentIn(app, 'orders-api', 'fix/pagination')?.ask('src/orders.ts') },
    { wait: 1150 },
    { say: 'Someone needs you. `!` shows who, and your terminal taps you on the shoulder.' },
    { wait: 1900 },
    open('projects'),
    { spot: 'sidebar' },
    { wait: 2200 },
    { spot: null },
    open('projects'),
    { click: 'orders-api (' },
    close,
    { wait: 900 },
    { say: 'One answer and it’s back to work. So are you.' },
    { wait: 1400 },
    { key: 'Enter' },
    { wait: 1600 },
    open('projects'),
    { click: 'web-shop (' },
    open('workspaces'),
    { click: 'feat/dark-mode' },
    close,
    { wait: 1000 },
  ],
  [
    reveal,
    { say: 'Curious what it’s doing? Every change, right next to the agent. Live.' },
    show('#482'),
    { click: '#482' },
    close,
    { until: fixed, timeout: 40000 },
    { wait: 1700 },
    changesWidth,
    { click: 'Changes', nth: -1, alt: '±' },
    { wait: 500 },
    { spot: 'changes' },
    { wait: 2800 },
    { say: 'Like it? Open it in your editor. Don’t? Send it back to the agent.' },
    { point: 'impl Address', nth: -1 },
    { wait: 3600 },
    { spot: null },
  ],
  [
    open('projects'),
    { say: 'Now quit. Seriously. Everything keeps running without you.' },
    { until: quiet, timeout: 7000 },
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
    reveal,
    { spot: 'columns' },
    { say: 'Right where you left it. And `✓` shows who finished while you were out.' },
    { wait: 4200 },
    { spot: null },
    close,
  ],
];

const TURN = 'Your turn. It’s all live: click around, right-click a pane, open settings, or type in a shell.';
const STILL = 'This isn’t a video. Click anything, or watch the tour.';

function spotArea(app: App, spot: Spot): Rect {
  const a = app.areas();
  if (spot === 'sidebar') return a.sidebar;
  if (spot === 'workspaces') return a.workspaces;
  if (spot === 'pane') return a.pane;
  if (spot === 'changes') return a.changes;
  if (a.compact) return isEmpty(a.sidebar) ? a.workspaces : a.sidebar;
  return rect(0, 0, a.workspaces.x + a.workspaces.w, app.rows);
}

const posterAreas = (cols: number, rows: number): Areas => {
  const app = new App();
  app.virtual = true;
  app.resize(cols, rows);
  posterState(app);
  app.render();
  const areas = app.areas();
  app.destroy();
  return areas;
};

const isRect = (v: unknown): v is Rect => typeof v === 'object' && v !== null && 'x' in v && 'y' in v && 'w' in v && 'h' in v;

const contains = (r: Rect, x: number, y: number): boolean => x >= r.x && y >= r.y && x < r.x + r.w && y < r.y + r.h;

function mapCell(from: Areas, to: Areas, x: number, y: number): Pos | null {
  let best: [string, Rect] | null = null;
  for (const [name, r] of Object.entries(from)) {
    if (!isRect(r) || isEmpty(r) || !contains(r, x, y)) continue;
    if (!best || r.w * r.h < best[1].w * best[1].h) best = [name, r];
  }
  if (!best) return null;
  const target = (to as unknown as Record<string, unknown>)[best[0]];
  if (!isRect(target) || isEmpty(target)) return null;
  return { x: target.x + Math.min(target.w - 1, x - best[1].x), y: target.y + Math.min(target.h - 1, y - best[1].y) };
}

async function hero(root: HTMLElement): Promise<void> {
  const pointer = root.querySelector<HTMLElement>('.pointer');
  const caption = root.querySelector<HTMLElement>('[data-caption]');
  const spotlight = root.querySelector<HTMLElement>('.spot');
  const badge = root.querySelector<HTMLElement>('.lapse');
  const badgeText = root.querySelector<HTMLElement>('[data-lapse]');
  const replay = root.querySelector<HTMLButtonElement>('[data-replay]');
  const replayLabel = root.querySelector<HTMLElement>('[data-replay-label]');
  const playButton = root.querySelector<HTMLButtonElement>('[data-play]');
  const posters = [...root.querySelectorAll<HTMLElement>('.poster')];
  let m: Mounted | null = null;
  let palette: Theme = defaultPalette;
  let player: Player | null = null;
  let touring = false;
  let waking = false;
  let token = 0;
  let target: Spot | null = null;

  const state = (s: 'poster' | 'tour' | 'free') => {
    root.dataset.state = s;
  };
  const say = (text: string) => {
    if (caption) caption.innerHTML = richText(text);
  };
  const offer = (label: string | null) => {
    if (replayLabel && label) replayLabel.textContent = label;
    if (replay) replay.hidden = !label;
  };
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
    m?.speed(speed);
    if (badge) badge.hidden = !label;
    if (badgeText && label) badgeText.textContent = label;
  };
  const make = async (scripted: boolean, build: (app: App) => void) => {
    const made = await mount(root, {
      rows: () => (viewport() < 560 ? 34 : viewport() < 900 ? 32 : 46),
      cell: () => (viewport() < 560 ? 7.4 : viewport() < 900 ? 7.8 : 6.6),
      clock: true,
      build: (app) => {
        app.scripted = scripted;
        build(app);
      },
      narrate: (html) => {
        if (!touring && caption) caption.innerHTML = html;
      },
      painted: () => place(),
    });
    (window as unknown as { demo?: Mounted }).demo = made;
    made.setTheme(palette);
    return made;
  };
  let mounting = Promise.resolve();
  const remount = (scripted: boolean, build: (app: App) => void) => {
    const next = mounting.then(async () => {
      m?.destroy();
      m = await make(scripted, build);
    });
    mounting = next.catch(() => {});
    return next;
  };
  const loosen = () => {
    if (!m) return;
    m.app.scripted = false;
    m.app.hover = null;
    m.app.dirty();
    setPace(m.app, 900);
  };
  const stop = () => {
    token += 1;
    player?.stop();
    player = null;
  };
  const free = (text = TURN, label = 'watch again') => {
    stop();
    touring = false;
    spot(null);
    lapse(1, null);
    loosen();
    say(text);
    offer(label);
    state('free');
  };
  const tour = async (from: number) => {
    stop();
    const mine = token;
    touring = true;
    state('tour');
    offer(null);
    spot(null);
    lapse(1, null);
    await remount(true, (app) => void world(app, { scripted: true }));
    const mounted = m;
    if (mine !== token || !mounted) {
      if (!touring) loosen();
      return;
    }
    for (let i = 0; i < from && mine === token; i++) await play(mounted, null, CHAPTERS[i], { instant: true }).done;
    for (let i = from; i < CHAPTERS.length; i++) {
      if (mine !== token) return;
      player = play(mounted, pointer, CHAPTERS[i], { say, spot, lapse });
      await player.done;
    }
    if (mine === token) free();
  };
  const live = async (): Promise<Mounted | null> => {
    stop();
    const mine = token;
    touring = false;
    await remount(false, posterState);
    if (mine !== token || !m) return null;
    free(STILL, 'watch the tour');
    return m;
  };
  const focusKeys = () => root.querySelector<HTMLTextAreaElement>('.keys')?.focus({ preventScroll: true });
  const tryIt = async () => {
    if (waking || root.dataset.state !== 'poster') return;
    waking = true;
    try {
      if (await live()) focusKeys();
    } finally {
      waking = false;
    }
  };
  const swatches = [...root.querySelectorAll<HTMLButtonElement>('[data-palette]')];
  const paint = (t: Theme) => {
    palette = t;
    root.style.setProperty('--term', t.background);
    for (const s of swatches) s.setAttribute('aria-pressed', String(themes[s.dataset.palette as keyof typeof themes] === t));
    m?.setTheme(t);
  };
  const pick = async (id: string) => {
    const t = themes[id as keyof typeof themes];
    if (!t) return;
    if (root.dataset.state !== 'poster') {
      paint(t);
      return;
    }
    if (waking) return;
    waking = true;
    try {
      paint(t);
      if (await live()) focusKeys();
    } finally {
      waking = false;
    }
  };
  const wake = async (e: MouseEvent) => {
    if (waking || root.dataset.state !== 'poster') return;
    waking = true;
    const poster = e.currentTarget as HTMLElement;
    const r = poster.getBoundingClientRect();
    const cols = Number(poster.style.getPropertyValue('--cols')) || 1;
    const rows = Number(poster.style.getPropertyValue('--rows')) || 1;
    const cx = (e.clientX - r.left) / r.width;
    const cy = (e.clientY - r.top) / r.height;
    try {
      const mounted = await live();
      if (!mounted) return;
      const px = Math.min(cols - 1, Math.max(0, Math.floor(cx * cols)));
      const py = Math.min(rows - 1, Math.max(0, Math.floor(cy * rows)));
      mounted.app.render();
      const cell = mapCell(posterAreas(cols, rows), mounted.app.areas(), px, py);
      if (cell) {
        mounted.app.pointerDown(cell.x, cell.y, 0);
        mounted.app.pointerUp();
      }
      if ((e as PointerEvent).pointerType === 'mouse') focusKeys();
    } finally {
      waking = false;
    }
  };

  root.addEventListener('demo-interact', () => {
    if (touring) free();
  });
  playButton?.addEventListener('click', () => void tryIt());
  swatches.forEach((s) => s.addEventListener('click', () => void pick(s.dataset.palette ?? '')));
  replay?.addEventListener('click', () => void tour(0));
  posters.forEach((p) => p.addEventListener('click', (e) => void wake(e)));
}

export function boot(): void {
  for (const root of document.querySelectorAll<HTMLElement>('[data-demo]')) {
    if (root.dataset.booted) continue;
    root.dataset.booted = 'true';
    hero(root);
  }
}
