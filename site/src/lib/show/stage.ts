import { CanvasView } from '../term/canvas';
import { Explode } from './explode';
import { COBALT, type Colour } from './field/field';
import type { Life } from './field/life';
import { crossed, easeOut } from './math';
import { one, paint, text } from './paint';
import { LAST, Reel, Screen, TICK } from './reel';
import type { Scroll } from './scroll';
import { type Chapter, LEAD, type Mark, TIMES } from './script';
import { cameraKeys, idleAt, storyAt } from './shots';
import { ASPECT, type Placed, type Size, WIDE, bigOf, closest, lens, place, poseAt, sizeFor, transformOf, windowOf } from './track';

const PIXELS = 9_000_000;
const CLOSEST = 2.2;
const BLINK = 0.56;
const BUILD_MS = 5;
const SEEK_MS = 9;

export const YELLOW: Colour = [0.96, 0.78, 0.28];
export const ORANGE: Colour = [1.0, 0.45, 0.0];
export const GREEN: Colour = [0.2, 0.92, 0.55];

const FLOODS: [Mark, number, Colour, number][] = [
  ['picked', 300, COBALT, 620],
  ['started', 700, COBALT, 900],
  ['needs', 1000, ORANGE, 800],
  ['answered', 200, YELLOW, 500],
  ['finished', 0, GREEN, 800],
  ['review', 300, COBALT, 900],
  ['quit', 0, COBALT, 1300],
  ['back', 200, GREEN, 1100],
];

export class Stage {
  entrance = 1;
  story = 0;
  lite = false;
  shownAt = 0;
  big = 1;

  private readonly scroll: Scroll;
  private readonly life: Life;
  private readonly scene = one('[data-scene]');
  private readonly rig = one('[data-rig]');
  private readonly spot = one('[data-spot]');
  private readonly ptr = one('[data-ptr]');
  private readonly tap = one('[data-ptr-tap]');
  private readonly label = one('[data-win-size]');
  private readonly view: CanvasView;
  private readonly explode: Explode;
  private size: Size = { cols: 0, rows: 0, cell: 7.2 };
  private reel: Reel | null = null;
  private screen: Screen | null = null;
  private keys: Placed[] = [];
  private fresh = true;
  private blinkOn = true;
  private blinkAt = 0;
  private cursorWas = '';
  private cursorRow = -1;
  private before = 0;

  constructor(scroll: Scroll, life: Life) {
    this.scroll = scroll;
    this.life = life;
    this.view = new CanvasView(one<HTMLCanvasElement>('[data-screen]'));
    this.explode = new Explode(scroll);
  }

  measure(): void {
    const view = { width: this.scroll.width, height: this.scroll.height };
    this.big = view.width >= WIDE ? bigOf(view.width) : 1;
    const size = sizeFor(view);
    if (size.cols !== this.size.cols || size.rows !== this.size.rows) {
      this.reel = new Reel(size.cols, size.rows);
      this.screen = new Screen(this.reel);
      text(this.label, `${size.cols}×${size.rows}`);
    }
    this.size = size;
    const [, , width, height] = windowOf(size);
    paint(this.rig, 'width', `${width.toFixed(1)}px`);
    paint(this.rig, 'height', `${height.toFixed(1)}px`);
    this.keys = place(cameraKeys((id, progress) => this.scroll.at(id, progress), size, view), view);
    const across = size.cols * size.cell;
    const ratio = this.lite ? 1 : Math.min(window.devicePixelRatio || 1, 2);
    const wanted = ratio * Math.min(closest(this.keys), CLOSEST);
    const most = Math.sqrt(PIXELS / (across * size.rows * size.cell * ASPECT));
    this.view.measure(across, size.cols, size.rows, Math.max(1, Math.min(wanted, most)));
    this.fresh = true;
    if (this.reel) this.explode.measure(size, this.reel, ratio);
  }

  hue(): Colour | null {
    const glance = this.scroll.progress('glance');
    if (glance <= 0 || glance >= 1) return null;
    const { marks } = TIMES;
    if (this.story >= marks.finished) return GREEN;
    if (this.story >= marks.answered) return YELLOW;
    if (this.story >= marks.needs + 1000) return ORANGE;
    return YELLOW;
  }

  update(time: number): void {
    const { reel, screen, scroll } = this;
    if (!reel || !screen) return;
    if (!reel.done) reel.extend(LAST, BUILD_MS);
    const pose = poseAt(this.keys, scroll.now);
    const shown = pose.fade > 0.004;
    paint(this.scene, 'display', shown ? 'block' : 'none');
    if (!shown) return;
    if (this.entrance < 1) pose.y += (1 - easeOut(this.entrance)) * scroll.height * 0.48;
    const transform = lens({ width: scroll.width, height: scroll.height }) + transformOf(pose);
    paint(this.rig, 'transform', transform);
    paint(this.rig, 'opacity', pose.fade.toFixed(3));
    const told = storyAt((id: Chapter) => scroll.progress(id));
    this.story = told > LEAD ? told : idleAt((time - this.shownAt) * 1000);
    const tick = Math.round(this.story / TICK);
    reel.extend(tick, SEEK_MS);
    screen.seek(tick);
    this.draw(screen, time);
    this.explode.update(transform);
    this.point(reel);
    this.floods();
    this.before = this.story;
  }

  private draw(screen: Screen, time: number): void {
    if (time - this.blinkAt > BLINK) {
      this.blinkAt = time;
      this.blinkOn = !this.blinkOn;
    }
    const cursor = screen.cursor;
    const now = cursor ? `${cursor.x},${cursor.y},${this.blinkOn}` : '';
    let rows = screen.dirty();
    if (now !== this.cursorWas) {
      const row = cursor ? cursor.y : this.cursorRow;
      const from = Math.min(row, this.cursorRow < 0 ? row : this.cursorRow);
      const to = Math.max(row, this.cursorRow);
      if (from >= 0) rows = rows ? [Math.min(rows[0], from), Math.max(rows[1], to)] : [from, to];
      this.cursorWas = now;
      this.cursorRow = cursor ? cursor.y : -1;
    }
    if (this.fresh) rows = [0, screen.grid.rows - 1];
    if (!rows) return;
    this.fresh = false;
    this.view.draw(screen.grid, cursor, true, this.blinkOn, rows);
  }

  private point(reel: Reel): void {
    const { cell } = this.size;
    const pointer = reel.pointerAt(this.story);
    paint(this.ptr, 'opacity', pointer.shown ? '1' : '0');
    paint(this.ptr, 'transform', `translate3d(${(pointer.x * cell).toFixed(1)}px,${(pointer.y * cell * ASPECT).toFixed(1)}px,0)`);
    paint(this.tap, 'opacity', pointer.tap < 0 ? '0' : ((1 - pointer.tap) * 0.9).toFixed(3));
    paint(this.tap, 'transform', `scale(${(0.3 + 1.1 * Math.max(0, pointer.tap)).toFixed(3)})`);
    const lit = reel.spotAt(this.story);
    paint(this.spot, 'opacity', lit ? '1' : '0');
    if (!lit) return;
    paint(this.spot, 'left', `${(lit.x * cell).toFixed(1)}px`);
    paint(this.spot, 'top', `${(lit.y * cell * ASPECT).toFixed(1)}px`);
    paint(this.spot, 'width', `${(lit.w * cell).toFixed(1)}px`);
    paint(this.spot, 'height', `${(lit.h * cell * ASPECT).toFixed(1)}px`);
  }

  private floods(): void {
    for (const [mark, plus, colour, power] of FLOODS) {
      if (crossed(this.before, this.story, TIMES.marks[mark] + plus)) this.life.flood(0.5, 0.5, colour, power);
    }
  }
}
