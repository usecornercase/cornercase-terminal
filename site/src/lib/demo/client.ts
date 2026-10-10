import { CanvasView, type Cursor } from '../term/canvas';
import type { Grid, Rect } from '../term/grid';
import { App } from './app';
import type { Key } from './programs';

export interface Box {
  left: number;
  top: number;
  width: number;
  height: number;
}

export interface Mounted {
  app: App;
  view: CanvasView;
  pointTo(x: number, y: number): { left: number; top: number };
  box(r: Rect): Box;
  speed(times: number): void;
  destroy(): void;
}

export interface MountOptions {
  rows: (width: number) => number;
  cell: (width: number) => number;
  build: (app: App) => void;
  narrate?: (html: string) => void;
  interactive?: boolean;
  clock?: boolean;
  painted?: () => void;
}

const KEYS = new Set(['Enter', 'Backspace', 'Escape', 'Tab', 'ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight', 'Delete', 'Home', 'End', 'PageUp', 'PageDown', 'F10']);

const escapeHtml = (s: string) => s.replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c] ?? c);

export const richText = (s: string) => escapeHtml(s).replace(/`([^`]+)`/g, '<code>$1</code>');

export async function mount(root: HTMLElement, opts: MountOptions): Promise<Mounted> {
  const canvas = root.querySelector('canvas') as HTMLCanvasElement;
  const input = root.querySelector('textarea') as HTMLTextAreaElement | null;
  const screen = canvas.parentElement as HTMLElement;
  try {
    await Promise.all([
      document.fonts.load('400 16px "JetBrains Mono Variable"'),
      document.fonts.load('700 16px "JetBrains Mono Variable"'),
    ]);
  } catch {}
  const view = new CanvasView(canvas);
  const app = new App();
  app.virtual = !!opts.clock;
  let last: { grid: Grid; cursor: Cursor | null } | null = null;
  let raf = 0;
  let blinkOn = true;
  let visible = true;
  let rate = 1;
  let clock = 0;
  let before = 0;
  const tick = (now: number) => {
    clock = requestAnimationFrame(tick);
    const dt = before ? Math.min(100, now - before) : 0;
    before = now;
    if (visible) app.advance(dt * rate);
  };
  if (opts.clock) clock = requestAnimationFrame(tick);

  const paint = () => {
    raf = 0;
    if (!visible) return;
    if (app.stale || !last) last = app.render();
    view.draw(last.grid, last.cursor, app.focused, blinkOn);
    opts.painted?.();
  };
  const schedule = () => {
    if (!raf) raf = requestAnimationFrame(paint);
  };
  view.onImage = schedule;
  app.on((event, detail) => {
    if (event === 'dirty') schedule();
    if (event === 'narrate' && detail && opts.narrate) opts.narrate(richText(detail));
    if (event === 'copy' && detail) navigator.clipboard?.writeText(detail).catch(() => {});
  });

  const fit = () => {
    const style = getComputedStyle(screen);
    const width = screen.clientWidth - parseFloat(style.paddingLeft) - parseFloat(style.paddingRight);
    if (width <= 0) return;
    const cell = opts.cell(width);
    const cols = Math.max(36, Math.floor(width / cell));
    const rows = opts.rows(width);
    view.measure(width, cols, rows);
    app.resize(cols, rows);
    last = null;
    schedule();
  };
  fit();
  opts.build(app);
  const resize = new ResizeObserver(fit);
  resize.observe(screen);

  const seen = new IntersectionObserver(([entry]) => {
    visible = entry.isIntersecting;
    if (visible) schedule();
  });
  seen.observe(canvas);

  const blink = setInterval(() => {
    if (!app.focused || !visible) return;
    blinkOn = !blinkOn;
    schedule();
  }, 560);

  const cleanup: (() => void)[] = [];
  const listen = <K extends keyof HTMLElementEventMap>(el: HTMLElement, type: K, fn: (e: HTMLElementEventMap[K]) => void, options?: AddEventListenerOptions) => {
    el.addEventListener(type, fn as EventListener, options);
    cleanup.push(() => el.removeEventListener(type, fn as EventListener, options));
  };

  if (opts.interactive !== false) {
    let touch: { x: number; y: number; timer: ReturnType<typeof setTimeout> | null; long: boolean; moved: boolean } | null = null;
    const focus = () => {
      if (input && document.activeElement !== input) input.focus({ preventScroll: true });
    };
    listen(canvas, 'pointermove', (e) => {
      const { x, y } = view.cellAt(e.clientX, e.clientY);
      if (touch) {
        if (Math.abs(x - touch.x) + Math.abs(y - touch.y) > 0) {
          touch.moved = true;
          if (touch.timer) clearTimeout(touch.timer);
        }
        return;
      }
      app.pointerMove(x, y, e.buttons);
      canvas.style.cursor = app.cursorAt(x, y);
    });
    listen(canvas, 'pointerleave', () => {
      if (!touch) app.pointerLeave();
    });
    listen(canvas, 'pointerdown', (e) => {
      root.dispatchEvent(new CustomEvent('demo-interact'));
      const { x, y } = view.cellAt(e.clientX, e.clientY);
      if (e.pointerType === 'touch') {
        touch = { x, y, timer: null, long: false, moved: false };
        const current = touch;
        current.timer = setTimeout(() => {
          current.long = true;
          app.pointerDown(x, y, 2);
          app.pointerUp();
          navigator.vibrate?.(10);
        }, 480);
        return;
      }
      e.preventDefault();
      focus();
      canvas.setPointerCapture(e.pointerId);
      app.pointerDown(x, y, e.button === 2 ? 2 : e.button === 1 ? 1 : 0);
    });
    listen(canvas, 'pointerup', () => {
      if (touch) {
        const t = touch;
        touch = null;
        if (t.timer) clearTimeout(t.timer);
        if (t.long || t.moved) return;
        app.pointerDown(t.x, t.y, 0);
        app.pointerUp();
        app.hover = null;
        const o = app.overlay;
        if (o && ['newWorkspace', 'rename', 'search', 'picker'].includes(o.kind)) focus();
        return;
      }
      app.pointerUp();
    });
    listen(canvas, 'pointercancel', () => {
      if (touch?.timer) clearTimeout(touch.timer);
      touch = null;
    });
    listen(canvas, 'contextmenu', (e) => e.preventDefault());
    listen(
      canvas,
      'wheel',
      (e) => {
        const { x, y } = view.cellAt(e.clientX, e.clientY);
        if (app.wheel(x, y, e.deltaY)) e.preventDefault();
      },
      { passive: false },
    );
    if (input) {
      listen(input, 'focus', () => {
        app.focused = true;
        blinkOn = true;
        app.dirty();
        root.classList.add('is-focused');
      });
      listen(input, 'blur', () => {
        app.focused = false;
        app.dirty();
        root.classList.remove('is-focused');
      });
      listen(input, 'keydown', (e) => {
        root.dispatchEvent(new CustomEvent('demo-interact'));
        if (e.metaKey || e.isComposing) return;
        if (e.ctrlKey && (e.key === 'v' || e.key === 'V')) return;
        const overlay = app.overlay?.kind;
        if (e.key === 'Tab' && !(overlay === 'settings' || overlay === 'issues' || overlay === 'picker' || overlay === 'newWorkspace')) return;
        const named = KEYS.has(e.key);
        if (!named && e.key.length !== 1) return;
        const k: Key = { key: e.ctrlKey && e.key.length === 1 ? e.key.toLowerCase() : e.key, ctrl: e.ctrlKey, alt: e.altKey, shift: e.shiftKey };
        if (app.key(k)) e.preventDefault();
        blinkOn = true;
      });
      listen(input, 'input', () => {
        const text = input.value;
        input.value = '';
        for (const ch of text) app.key({ key: ch });
      });
      listen(input, 'paste', (e) => {
        const text = e.clipboardData?.getData('text/plain') ?? '';
        e.preventDefault();
        if (text) app.paste(text);
      });
    }
  }

  return {
    app,
    view,
    pointTo(x: number, y: number) {
      const box = view.cellBox(x, y);
      return { left: canvas.offsetLeft + box.left + box.width / 2, top: canvas.offsetTop + box.top + box.height / 2 };
    },
    box(r: Rect) {
      const a = view.cellBox(r.x, r.y);
      const b = view.cellBox(r.x + r.w - 1, r.y + r.h - 1);
      return { left: canvas.offsetLeft + a.left, top: canvas.offsetTop + a.top, width: b.left + b.width - a.left, height: b.top + b.height - a.top };
    },
    speed(times: number) {
      rate = times;
    },
    destroy() {
      cancelAnimationFrame(raf);
      cancelAnimationFrame(clock);
      clearInterval(blink);
      resize.disconnect();
      seen.disconnect();
      cleanup.forEach((c) => c());
      app.destroy();
    },
  };
}
