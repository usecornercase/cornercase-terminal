import { LAYOUTS, focusPoint, pathD, pointAt, type EdgeId, type Layout, type NodeId } from './graph';
import { frameAt, type Frame } from './timeline';

type Writer = (el: Element | null, name: string, value: string | null) => void;

function writer(): Writer {
  const seen = new WeakMap<Element, Map<string, string | null>>();
  return (el, name, value) => {
    if (!el) return;
    let m = seen.get(el);
    if (!m) seen.set(el, (m = new Map()));
    if (m.get(name) === value) return;
    m.set(name, value);
    if (value === null) el.removeAttribute(name);
    else el.setAttribute(name, value);
  };
}

const ease = (t: number): number => t * t * (3 - 2 * t);
const n = (v: number): string => String(Math.round(v * 100) / 100);

interface View {
  apply(frame: Frame): void;
}

function view(svg: SVGSVGElement, layout: Layout): View {
  const set = writer();
  const q = <T extends Element>(sel: string): T | null => svg.querySelector<T>(sel);
  const all = <T extends Element>(sel: string): T[] => [...svg.querySelectorAll<T>(sel)];
  const nodes = all('[data-node]');
  const edges = all('[data-edge]');
  const heads = all('[data-head]');
  const packets = all('[data-packet]');
  const trails = all<SVGPathElement>('[data-trail]');
  const pips = all('[data-pip]');
  const ripple = q<SVGRectElement>('[data-ripple]');
  const spot = q('[data-spot]');
  const tick = q('[data-tick]');
  const loop = q('[data-loop-text]');
  const end = q('.fs-end');
  const orch = layout.nodes.orchestrator;
  let tickText = tick?.textContent ?? '';
  let loopText = loop?.textContent ?? '';

  return {
    apply(f) {
      for (const el of nodes) set(el, 'data-state', f.nodes[(el as HTMLElement | SVGElement).getAttribute('data-node') as NodeId]);
      set(end, 'data-checks', f.checks);
      for (const el of edges) set(el, 'data-lit', f.lit.includes(el.getAttribute('data-edge') as EdgeId) ? '' : null);
      for (const el of heads) set(el, 'data-lit', f.lit.includes(el.getAttribute('data-head') as EdgeId) ? '' : null);
      for (const el of edges) set(el, 'data-hot', f.hot.includes(el.getAttribute('data-edge') as EdgeId) ? '' : null);
      for (const el of heads) set(el, 'data-hot', f.hot.includes(el.getAttribute('data-head') as EdgeId) ? '' : null);
      packets.forEach((el, i) => {
        const p = f.packets[i];
        set(el, 'visibility', p ? 'visible' : 'hidden');
        if (!p) return;
        const [x, y] = pointAt(layout.edges[p.edge], p.at);
        set(el, 'transform', `translate(${n(x)} ${n(y)})`);
        set(el, 'data-tone', p.tone);
      });
      trails.forEach((el, i) => {
        const p = f.packets[i];
        set(el, 'visibility', p ? 'visible' : 'hidden');
        if (!p) return;
        set(el, 'd', pathD(layout.edges[p.edge]));
        set(el, 'stroke-dashoffset', n(18 - p.at * 100));
        set(el, 'data-tone', p.tone);
      });
      if (ripple) {
        const r = f.ripple;
        const g = r === null ? 0 : 60 * r;
        set(ripple, 'x', n(-g));
        set(ripple, 'y', n(-g));
        set(ripple, 'width', n(orch.w + 2 * g));
        set(ripple, 'height', n(orch.h + 2 * g));
        set(ripple, 'rx', n(12 + g));
        set(ripple, 'opacity', r === null ? '0' : n(0.6 * (1 - r)));
      }
      if (spot) {
        const a = focusPoint(layout, f.focus.from);
        const b = focusPoint(layout, f.focus.to);
        const k = ease(f.focus.mix);
        set(spot, 'cx', n(a[0] + (b[0] - a[0]) * k));
        set(spot, 'cy', n(a[1] + (b[1] - a[1]) * k));
      }
      for (const el of pips) set(el, 'data-on', f.commits >= Number(el.getAttribute('data-pip')) ? '' : null);
      if (tick && tickText !== f.ticker) tick.textContent = tickText = f.ticker;
      const label = layout.loopText(f.rounds);
      if (loop && loopText !== label) loop.textContent = loopText = label;
      set(svg, 'data-rounds', String(f.rounds));
    },
  };
}

export function mount(root: HTMLElement): void {
  const still = matchMedia('(prefers-reduced-motion: reduce)').matches;
  if (still) {
    root.dataset.still = '';
    return;
  }
  const views = [...root.querySelectorAll<SVGSVGElement>('svg[data-layout]')].map((svg) => view(svg, LAYOUTS[svg.dataset.layout as 'wide' | 'tall']));
  const says = [...root.querySelectorAll<HTMLElement>('[data-say]')];
  let mood = '';
  let said = '';
  const show = (f: Frame): void => {
    for (const v of views) v.apply(f);
    if (mood !== f.mood) root.dataset.mood = mood = f.mood;
    if (said !== f.caption) {
      said = f.caption;
      for (const s of says) s.classList.toggle('on', s.dataset.say === f.caption);
    }
  };
  root.dataset.ready = '';
  show(frameAt(0));
  const toggle = root.querySelector<HTMLButtonElement>('[data-toggle]');
  let elapsed = 0;
  let last = 0;
  let raf = 0;
  let visible = false;
  let paused = false;
  const tick = (now: number): void => {
    elapsed += Math.min(100, now - last);
    last = now;
    show(frameAt(elapsed));
    raf = requestAnimationFrame(tick);
  };
  const run = (): void => {
    if (raf || !visible || paused || document.hidden) return;
    root.dataset.live = '';
    last = performance.now();
    raf = requestAnimationFrame(tick);
  };
  const stop = (): void => {
    cancelAnimationFrame(raf);
    raf = 0;
  };
  new IntersectionObserver(
    ([e]) => {
      visible = e.isIntersecting;
      if (visible) run();
      else stop();
    },
    { threshold: 0.3 },
  ).observe(root);
  document.addEventListener('visibilitychange', () => (document.hidden ? stop() : run()));
  toggle?.addEventListener('click', () => {
    paused = !paused;
    root.toggleAttribute('data-paused', paused);
    toggle.textContent = (paused ? toggle.dataset.play : toggle.dataset.pause) ?? '';
    if (paused) stop();
    else run();
  });
}
