import { COBALT, Field } from './field/field';
import { Life } from './field/life';
import { Hero } from './hero';
import { EACH, startOf, turnOf } from './layers';
import { clamp, easeOut, mix, span } from './math';
import { all, fade, one, paint, wipe } from './paint';
import { Scroll } from './scroll';
import { Stage } from './stage';
import { Finale, Flows, Hand, Here, Leave, Statement } from './story';
import { SHORTEST, WIDE } from './track';

let stopped = false;

function bootstrap(field: Field): void {
  const scroll = new Scroll();
  const life = new Life(field);
  const hero = new Hero();
  const stage = new Stage(scroll, life);
  const statement = new Statement(scroll);
  const leave = new Leave(stage);
  const hand = new Hand(scroll);
  const flows = new Flows(scroll);
  const here = new Here(scroll);
  const finale = new Finale(scroll, field);
  const caps = all('[data-chapter]').map((section) => ({
    id: section.dataset.section ?? '',
    pin: one('.pin', section),
    cap: one('[data-cap]', section),
    title: one('[data-title]', section),
    side: section.dataset.side === 'right' ? 1 : -1,
  }));
  let chapterWas = '';
  const canvas = one<HTMLCanvasElement>('[data-field]');
  const root = document.documentElement;

  const guard = (work: () => void) => () => {
    if (stopped) return;
    try {
      work();
    } catch {
      fallback();
    }
  };
  let contextLost = false;
  let lastWidth = 0;
  let lastHeight = 0;
  let quality = 0;
  let scale = Math.min(window.devicePixelRatio || 1, 2);
  const size = () => {
    if (window.innerHeight < SHORTEST) {
      fallback();
      return;
    }
    const width = root.clientWidth;
    const height = canvas.clientHeight || window.innerHeight;
    if (!contextLost && (width !== lastWidth || Math.abs(height - lastHeight) > 2)) {
      field.resize(width, height, scale);
      lastWidth = width;
      lastHeight = height;
    }
    scroll.measure();
    stage.measure();
    hero.measure();
  };

  const reduceTo = (level: number) => {
    quality = level;
    if (level === 1) field.iterations = 8;
    if (level >= 2) {
      field.iterations = 4;
      scale = 1;
      stage.lite = true;
      root.classList.add('lite');
      lastWidth = 0;
      size();
    }
  };

  const settle = () => {
    const m = life.mood;
    m.hue = COBALT;
    m.gain = 1;
    m.glow = 0.62;
    m.emit = 1;
    m.low = 0;
    const heroP = scroll.progress('hero');
    if (heroP < 1) {
      m.low = 1 - heroP;
      m.emit = 1.25;
      return;
    }
    const stating = scroll.progress('why');
    if (stating > 0 && stating < 1) {
      m.gain = 0.66;
      m.emit = 0.8;
      return;
    }
    const exploding = scroll.progress('features');
    if (exploding > 0.08 && exploding < 1) {
      m.gain = 0.62;
      m.emit = 0.8;
      return;
    }
    const hue = stage.hue();
    if (hue) {
      m.hue = hue;
      m.gain = 0.92;
      m.emit = 1.25;
      return;
    }
    const leaving = scroll.progress('leave');
    if (leaving > 0 && leaving < 1) {
      const away = leave.away();
      m.emit = mix(1.1, 0.7, away);
      m.gain = mix(0.86, 0.55, away);
      return;
    }
    if (scroll.progress('flows') >= 1) {
      m.gain = 0.5;
      m.emit = 0.66;
      return;
    }
    if (scroll.progress('leave') >= 1) {
      m.gain = 0.6;
      m.emit = 0.72;
      return;
    }
    m.gain = 0.9;
  };

  const mood = () => {
    settle();
    if (scroll.width < WIDE) life.mood.gain *= 0.72;
  };

  const captions = (time: number) => {
    const wide = scroll.width >= WIDE;
    for (const { id, pin, cap, title, side } of caps) {
      const p = scroll.progress(id);
      if (p <= 0 || p >= 1) {
        fade(cap, 0);
        paint(pin, '--scrim', '0');
        continue;
      }
      const enter = easeOut(span(p, 0.03, 0.14));
      const out = span(p, 0.9, 0.99);
      const opacity = enter * (1 - out);
      const float = Math.sin(time * 0.8) * 3;
      const y = (1 - enter) * 44 - out * 34 + float * opacity;
      const x = (1 - enter) * 26 * side;
      fade(cap, opacity);
      paint(pin, '--scrim', opacity.toFixed(3));
      paint(title, 'font-stretch', `${mix(72, 112, easeOut(span(p, 0.02, 0.17))).toFixed(1)}%`);
      if (p > 0.05 && p < 0.1 && chapterWas !== id) {
        chapterWas = id;
        life.flood(side > 0 ? 0.2 : 0.8, 0.5, COBALT, 620);
      }
      paint(cap, 'transform', wide ? `translate3d(${x.toFixed(1)}px,calc(-50% + ${y.toFixed(1)}px),0)` : `translate3d(0,${y.toFixed(1)}px,0)`);
    }
  };

  const reach = (el: HTMLElement, id: string, at: () => number, shown: (p: number) => boolean) => {
    el.addEventListener('focusin', () => {
      if (!stopped && !shown(scroll.progress(id))) scrollTo({ top: scroll.at(id, at()) });
    });
  };
  for (const { id, cap } of caps) {
    if (id !== 'features') reach(cap, id, () => 0.3, (p) => p > 0.14 && p < 0.9);
  }
  all('[data-feature]').forEach((item, i) => reach(item, 'features', () => startOf(i) + EACH / 2, (p) => turnOf(p) === i));
  reach(one('[data-hand-text]'), 'hand', () => 0.4, (p) => p > 0.14 && p < 0.9);

  size();
  addEventListener('resize', guard(size));
  void Promise.all([document.fonts.load('16px "JetBrains Mono Variable"'), document.fonts.load('700 16px "JetBrains Mono Variable"'), document.fonts.load('400 16px "cornercase symbols"', '\ue5ff'), document.fonts.ready]).finally(
    guard(() => {
      if (!contextLost) field.refreshGlyphs();
      size();
    }),
  );
  canvas.addEventListener('webglcontextlost', (event) => {
    event.preventDefault();
    contextLost = true;
  });
  canvas.addEventListener(
    'webglcontextrestored',
    guard(() => {
      field.restore();
      contextLost = false;
      lastWidth = 0;
      size();
    }),
  );
  matchMedia('(prefers-reduced-motion: reduce)').addEventListener('change', (event) => {
    if (event.matches) fallback();
  });

  addEventListener(
    'pointermove',
    (event) => {
      if (stopped) return;
      life.pointer(event.clientX, event.clientY, root.clientWidth, canvas.clientHeight || innerHeight);
      hero.pointer(event.clientX, event.clientY);
    },
    { passive: true },
  );

  const intro = scroll.now < 60;
  let burst = !intro;
  let heroShown = intro ? Number.POSITIVE_INFINITY : -10;
  if (intro) {
    field.markAt = [0.5, 0.54];
    field.markSize = Math.min(innerHeight * 0.42, root.clientWidth * 0.62);
    field.markFray = 0.06;
    field.markOn = 0;
  } else {
    hero.show(0);
    life.awake = true;
  }

  let started = -1;
  let last = 0;
  let debt = 0;
  let frames = 0;
  let framesAt = 0;
  let slow = 0;

  const tick = (now: number) => {
    if (started < 0) {
      started = now;
      last = now;
      framesAt = now;
    }
    const dt = Math.min(0.05, Math.max(0.001, (now - last) / 1000));
    last = now;
    const time = (now - started) / 1000;

    scroll.tick(dt);
    const heroP = scroll.progress('hero');

    if (!burst) {
      field.markOn = clamp(time / 0.55);
      const cursorOn = time > 0.62 && Math.floor((time - 0.62) * 6) % 2 === 1;
      field.markFray = cursorOn ? 0.16 : 0.05;
      if (time > 1.08) {
        burst = true;
        field.stamp();
        field.markOn = 0;
        field.burst(0.5, 0.54, Math.max(1000, field.markSize * 3.4), [0.0, 0.14, 0.4]);
        life.awake = true;
        hero.show(time);
        heroShown = time;
      }
    }

    mood();
    const finaleP = burst ? finale.update() : 0;
    if (finaleP > 0) life.mood.gain = mix(life.mood.gain, 0.66, finaleP);
    life.tick(dt, scroll.velocity, Math.max(1, scroll.height));

    hero.update(time, dt, heroP);
    stage.entrance = clamp((time - heroShown) / 1.15);
    stage.shownAt = Math.max(0, heroShown);
    stage.update(time);
    captions(time);
    statement.update();
    leave.update();
    hand.update(time);
    flows.update();
    here.update();

    debt += dt;
    if (debt >= 1 / 72 && !contextLost) {
      life.walk(debt, time);
      field.frame(debt, time);
      debt = 0;
    }

    frames += 1;
    if (now - framesAt > 500) {
      const rate = (frames * 1000) / (now - framesAt);
      slow = rate < 38 && !document.hidden ? slow + 1 : 0;
      if (slow >= 3 && quality < 2) {
        reduceTo(quality + 1);
        slow = 0;
      }
      frames = 0;
      framesAt = now;
    }
    requestAnimationFrame(guarded);
  };
  const guarded = (now: number) => {
    if (stopped) return;
    try {
      tick(now);
    } catch {
      fallback();
    }
  };
  requestAnimationFrame(guarded);
}

function fallback(): void {
  if (stopped) return;
  stopped = true;
  document.documentElement.classList.remove('motion', 'intro', 'lite');
  wipe();
  dispatchEvent(new Event('cornercase:plain'));
}

(window as unknown as { cornercaseShow: boolean }).cornercaseShow = true;

if (document.documentElement.classList.contains('motion')) {
  try {
    const field = Field.start(one<HTMLCanvasElement>('[data-field]'));
    if (field) bootstrap(field);
    else fallback();
  } catch {
    fallback();
  }
}
