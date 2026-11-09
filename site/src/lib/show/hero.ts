import { backOut, clamp, damp, easeExpo, mix, span, veil } from './math';
import { all, fade, one, paint } from './paint';

const REST = 112;
const NEAR = 125;
const THIN = 62;
const ENTERED = 1.6;

interface Letter {
  el: HTMLElement;
  x: number;
  y: number;
  bump: number;
}

export class Hero {
  private readonly letters: Letter[];
  private readonly rises: HTMLElement[];
  private readonly hint: HTMLElement;
  private shownAt = -1;
  private pointerX = -9999;
  private pointerY = -9999;

  constructor() {
    this.rises = all('[data-hero-rise]');
    this.hint = one('[data-hero-hint]');
    this.letters = all('[data-ch]').map((el) => ({ el, x: 0, y: 0, bump: 0 }));
  }

  measure(): void {
    for (const letter of this.letters) {
      const box = letter.el.getBoundingClientRect();
      letter.x = box.left + box.width / 2;
      letter.y = box.top + box.height / 2;
    }
  }

  pointer(x: number, y: number): void {
    this.pointerX = x;
    this.pointerY = y;
  }

  show(time: number): void {
    this.shownAt = time;
    document.documentElement.classList.remove('intro');
    this.measure();
  }

  update(time: number, dt: number, progress: number): void {
    if (this.shownAt < 0) return;
    const since = time - this.shownAt;
    this.fadeRest(progress, since);
    if (progress >= 1) return;
    const count = this.letters.length;
    const squeeze = span(progress, 0.02, 0.62);
    this.letters.forEach((letter, i) => this.letter(letter, i, count, since, squeeze, time, dt));
  }

  private letter(letter: Letter, i: number, count: number, since: number, squeeze: number, time: number, dt: number): void {
    const local = clamp((since - i * 0.022) / 1.05);
    const rise = easeExpo(local);
    const dx = letter.x - this.pointerX;
    const dy = letter.y - this.pointerY;
    const close = Math.exp(-(dx * dx + dy * dy) / (2 * 150 * 150));
    letter.bump = damp(letter.bump, close, 10, dt);
    const wave = Math.sin(time * 0.9 - i * 0.34) * 2.4;
    const stretch = mix(THIN, REST + wave + (NEAR - REST) * letter.bump, backOut(local));
    const squeezed = mix(stretch, THIN, squeeze);
    const y = (1 - rise) * 62 - squeeze * 80 * (1 + (i % 3) * 0.14);
    const spread = (i - count / 2) * squeeze * 5;
    const opacity = clamp(local * 2.4) * (1 - span(squeeze, 0.45, 0.98));
    paint(letter.el, 'font-stretch', `${clamp(squeezed, THIN, NEAR).toFixed(1)}%`);
    paint(letter.el, 'font-weight', (800 + 100 * letter.bump).toFixed(0));
    paint(letter.el, 'transform', `translate3d(${spread.toFixed(1)}px,${y.toFixed(1)}%,0)`);
    paint(letter.el, 'opacity', opacity.toFixed(3));
    paint(letter.el, '--blk', veil(local, 0, 0.06, 0.14, 0.34).toFixed(3));
  }

  private fadeRest(progress: number, since: number): void {
    const out = span(progress, 0, 0.3);
    for (const rise of this.rises) {
      if (since > ENTERED) paint(rise, 'transition', 'none');
      fade(rise, 1 - out);
      paint(rise, 'translate', `0 ${(-out * 46).toFixed(1)}px`);
    }
    paint(this.hint, 'opacity', progress > 0.02 ? '0' : '1');
  }
}
