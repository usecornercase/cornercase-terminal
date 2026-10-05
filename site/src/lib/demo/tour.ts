import type { App } from './app';
import type { Mounted } from './client';

export type Spot = 'sidebar' | 'workspaces' | 'columns' | 'pane' | 'changes';

export type Step =
  | { say: string }
  | { spot: Spot | null }
  | { lapse: number; speed: number; label: string }
  | { click: string; nth?: number; dx?: number; right?: boolean; orKey?: string; alt?: string }
  | { until: (app: App) => boolean; timeout: number }
  | { point: string; nth?: number; dx?: number }
  | { key: string }
  | { type: string }
  | { wait: number }
  | { run: (app: App) => void };

export interface Player {
  stop(): void;
  done: Promise<void>;
}

export interface Hooks {
  say?: (text: string) => void;
  spot?: (spot: Spot | null) => void;
  lapse?: (speed: number, label: string | null) => void;
  instant?: boolean;
}

const STEP = 100;

function find(app: App, text: string, nth = 0, dx = 0): { x: number; y: number } | null {
  if (app.stale) app.render();
  let p: { x: number; y: number } | null = null;
  if (nth < 0) {
    for (let i = 0; ; i++) {
      const q = app.textAt(text, i);
      if (!q) break;
      p = q;
    }
  } else p = app.textAt(text, nth);
  return p ? { x: p.x + dx, y: p.y } : null;
}

const realSleep = (ms: number, signal: AbortSignal) =>
  new Promise<void>((resolve, reject) => {
    const t = setTimeout(resolve, ms);
    signal.addEventListener('abort', () => {
      clearTimeout(t);
      reject(new Error('stopped'));
    });
  });

export function play(m: Mounted, pointer: HTMLElement | null, steps: Step[], hooks: Hooks = {}): Player {
  const control = new AbortController();
  const { signal } = control;
  const app = m.app;
  const instant = !!hooks.instant;
  const wait = async (ms: number) => {
    if (instant) return app.advance(ms);
    const end = app.now() + ms;
    while (app.now() < end) await realSleep(16, signal);
  };
  const until = async (cond: (app: App) => boolean, timeout: number) => {
    const end = app.now() + timeout;
    while (!cond(app) && app.now() < end) await wait(STEP);
  };
  const move = async (x: number, y: number) => {
    app.hover = { x, y };
    app.dirty();
    if (instant) return;
    const at = m.pointTo(x, y);
    if (pointer) {
      pointer.hidden = false;
      pointer.style.transform = `translate(${at.left}px, ${at.top}px)`;
    }
    await realSleep(620, signal);
  };
  const tap = (right: boolean) => {
    if (!pointer || instant) return;
    pointer.classList.remove('tap');
    void pointer.offsetWidth;
    pointer.classList.add('tap');
    pointer.classList.toggle('right', right);
  };
  const run = async () => {
    for (const step of steps) {
      if (signal.aborted) return;
      if ('say' in step) {
        if (!instant) hooks.say?.(step.say);
      } else if ('spot' in step) {
        if (!instant) hooks.spot?.(step.spot);
      } else if ('lapse' in step) {
        if (instant) app.advance(step.lapse * step.speed);
        else {
          hooks.lapse?.(step.speed, step.label);
          try {
            await realSleep(step.lapse, signal);
          } finally {
            hooks.lapse?.(1, null);
          }
        }
      } else if ('wait' in step) await wait(step.wait);
      else if ('run' in step) step.run(app);
      else if ('key' in step) {
        app.key({ key: step.key });
        await wait(160);
      } else if ('type' in step) {
        for (const ch of step.type) {
          app.key({ key: ch });
          await wait(40 + Math.random() * 50);
        }
      } else if ('point' in step) {
        const p = find(app, step.point, step.nth, step.dx);
        if (p) await move(p.x, p.y);
      } else if ('until' in step) await until(step.until, step.timeout);
      else {
        const p = find(app, step.click, step.nth, step.dx) ?? (step.alt ? find(app, step.alt, step.nth) : null);
        if (!p) {
          if (step.orKey) {
            app.key({ key: step.orKey });
            await wait(260);
          }
          continue;
        }
        await move(p.x, p.y);
        tap(!!step.right);
        app.pointerDown(p.x, p.y, step.right ? 2 : 0);
        app.pointerUp();
        await wait(260);
      }
    }
  };
  const done = run()
    .catch(() => {})
    .finally(() => {
      if (pointer && !instant) pointer.hidden = true;
    });
  return {
    stop() {
      control.abort();
      if (pointer) pointer.hidden = true;
    },
    done,
  };
}
