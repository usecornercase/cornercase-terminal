import { scrub } from '../flowshow/client';
import { INTRO } from '../flowshow/timeline';
import type { Field } from './field/field';
import { easeOut, mix, span, veil } from './math';
import { all, fade, flag, one, paint, text } from './paint';
import type { Scroll } from './scroll';
import { TIMES } from './script';
import type { Stage } from './stage';
import { WIDE } from './track';

export class Statement {
  private readonly scroll: Scroll;
  private readonly say = one('[data-statement]');
  private readonly title = one('[data-title]', this.say);

  constructor(scroll: Scroll) {
    this.scroll = scroll;
  }

  update(): void {
    const p = this.scroll.progress('why');
    const shown = veil(p, 0.16, 0.34, 0.74, 0.92);
    fade(this.say, shown);
    if (shown <= 0.002) return;
    const rise = (1 - easeOut(span(p, 0.16, 0.4))) * 60 - span(p, 0.74, 0.92) * 50;
    paint(this.say, 'transform', `translate3d(-50%,calc(-50% + ${rise.toFixed(1)}px),0)`);
    paint(this.title, 'font-stretch', `${mix(70, 112, easeOut(span(p, 0.14, 0.42))).toFixed(1)}%`);
  }
}

const DONE = [0.3, 0.55, 0.8];
const AWAY_MINUTES = 25;

export class Leave {
  private readonly stage: Stage;
  private readonly diagram = one('[data-diagram]');
  private readonly procs = all('[data-proc]', this.diagram);
  private readonly clock = one('[data-away-clock]', this.diagram);

  constructor(stage: Stage) {
    this.stage = stage;
  }

  away(): number {
    const { quit, back } = TIMES.marks;
    return veil(this.stage.story, quit, quit + 600, back, back + 500);
  }

  update(): void {
    const { quit, away, typing, back } = TIMES.marks;
    const at = this.stage.story;
    const shown = veil(at, quit + 500, quit + 1300, back + 200, back + 900);
    fade(this.diagram, shown);
    if (shown <= 0.002) return;
    paint(this.diagram, 'transform', `translate3d(-50%,calc(-50% + ${((1 - easeOut(span(at, quit + 500, quit + 1500))) * 40).toFixed(1)}px),0) scale(${this.stage.big})`);
    const closed = at >= quit && at < back;
    if (this.diagram.dataset.link !== (closed ? 'closed' : 'attached')) this.diagram.dataset.link = closed ? 'closed' : 'attached';
    const lapse = span(at, away, typing);
    this.procs.forEach((proc, i) => flag(proc, 'is-done', lapse >= (DONE[i] ?? 2)));
    const seconds = Math.round(lapse * AWAY_MINUTES * 60);
    text(this.clock, closed ? ` · ${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}` : '');
  }
}

const PAUSE = 7;

export class Hand {
  private readonly scroll: Scroll;
  private readonly words = one('[data-hand-text]');
  private readonly title = one('[data-title]', this.words);
  private readonly transcript = one('[data-transcript]');
  private readonly lines: { el: HTMLElement; chars: number; typed: boolean; from: number; to: number }[] = [];
  private readonly total: number;

  constructor(scroll: Scroll) {
    this.scroll = scroll;
    let at = 0;
    for (const el of all('[data-line]', this.transcript)) {
      const chars = Number(el.dataset.chars ?? 0);
      const typed = el.dataset.line === 'run';
      const cost = typed ? chars : PAUSE;
      this.lines.push({ el, chars, typed, from: at, to: at + cost });
      at += cost;
    }
    this.total = at;
  }

  update(time: number): void {
    const p = this.scroll.progress('hand');
    const shown = veil(p, 0.03, 0.14, 0.9, 0.99);
    for (const el of [this.words, this.transcript]) fade(el, shown);
    if (shown <= 0.002) return;
    const wide = this.scroll.width >= WIDE;
    const enter = 1 - easeOut(span(p, 0.03, 0.16));
    paint(this.words, 'transform', wide ? `translate3d(${(-enter * 30).toFixed(1)}px,${(Math.sin(time * 0.8) * 3).toFixed(1)}px,0)` : 'none');
    paint(this.transcript, 'transform', `translate3d(0,${(enter * 50).toFixed(1)}px,0)`);
    paint(this.title, 'font-stretch', `${mix(72, 112, easeOut(span(p, 0.02, 0.17))).toFixed(1)}%`);
    const at = span(p, 0.16, 0.8) * this.total;
    for (const line of this.lines) {
      const typed = at >= line.to ? line.chars : line.typed ? Math.max(0, Math.floor(at - line.from)) : 0;
      paint(line.el, '--typed', String(typed));
    }
  }
}

export class Finale {
  private readonly scroll: Scroll;
  private readonly field: Field;
  private readonly mark: HTMLElement | null;
  private shown = false;

  constructor(scroll: Scroll, field: Field) {
    this.scroll = scroll;
    this.field = field;
    this.mark = document.querySelector<HTMLElement>('[data-finale-mark]');
  }

  update(): number {
    const box = this.mark?.getBoundingClientRect();
    if (!box || box.width === 0) return this.hide();
    const height = this.scroll.height;
    const p = span(box.top + box.height / 2, height * 1.05, height * 0.56);
    if (p <= 0 || box.bottom < -height * 0.2) return this.hide();
    this.shown = true;
    this.field.markAt = [(box.left + box.width / 2) / this.scroll.width, 1 - (box.top + box.height / 2) / this.field.lvh];
    this.field.markSize = box.width;
    this.field.markOn = easeOut(p);
    this.field.markFray = mix(0.5, 0.06, p);
    return p;
  }

  private hide(): number {
    if (this.shown) this.field.markOn = 0;
    this.shown = false;
    return 0;
  }
}

const RUN = 18000;

export class Flows {
  private readonly scroll: Scroll;
  private readonly head = one('[data-flows-head]');
  private readonly title = one('[data-title]', this.head);
  private readonly figure = one('[data-flowshow]');
  private readonly show = scrub(this.figure);

  constructor(scroll: Scroll) {
    this.scroll = scroll;
  }

  update(): void {
    const p = this.scroll.raw('flows');
    const seen = this.scroll.seen('flows');
    const shown = seen ? veil(p, -0.08, 0.02, 0.97, 1.06) : 0;
    fade(this.head, shown);
    paint(this.figure, 'opacity', shown.toFixed(3));
    if (!seen) {
      this.show(null);
      return;
    }
    const enter = 1 - easeOut(span(p, -0.08, 0.06));
    paint(this.head, 'transform', `translate3d(0,${(enter * 40).toFixed(1)}px,0)`);
    paint(this.figure, 'transform', `translate3d(0,${(enter * 90).toFixed(1)}px,0)`);
    paint(this.title, 'font-stretch', `${mix(72, 112, easeOut(span(p, -0.08, 0.08))).toFixed(1)}%`);
    this.show(INTRO + span(p, 0.1, 0.92) * RUN);
  }
}

const PLACES: [string, string, string][] = [
  ['features', 'features', 'features'],
  ['why', 'why', 'hand'],
  ['demo', 'play', 'play'],
  ['flows', 'flows', 'flows'],
];

export class Here {
  private readonly scroll: Scroll;
  private readonly links = new Map(all('[data-nav]').map((link) => [link.dataset.nav ?? '', link]));

  constructor(scroll: Scroll) {
    this.scroll = scroll;
  }

  update(): void {
    const middle = this.scroll.now + this.scroll.height * 0.5;
    for (const [name, first, last] of PLACES) {
      const from = this.scroll.at(first, 0);
      const to = this.scroll.at(last, 1) + this.scroll.height;
      flag(this.links.get(name) ?? null, 'is-here', middle >= from && middle < to);
    }
  }
}
