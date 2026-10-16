import { FOILS, GLARE, foilAt, foilBox, foilCss, glareAt, glareCss } from './sheen';
import { type Light, type Pose, CENTER, LIT, REST, STAGE, aimed, approach, css, drift, ticketMatrix } from './pose';
import { HALO, SHADOW, backdrop, placed } from './stage';

export interface Parts {
  stage: HTMLElement;
  space: HTMLElement;
  backdrop: HTMLCanvasElement;
  shadow: HTMLCanvasElement;
  halo: HTMLCanvasElement;
  ticket: HTMLElement;
  foils: HTMLElement[];
  glare: HTMLElement;
}

export class Tilt {
  private pose: Pose = { ...REST };
  private light: Light = { ...LIT };
  private pointer: { x: number; y: number } | null = null;
  private frame = 0;
  private last = 0;
  private visible = true;
  private readonly still: boolean;
  private readonly observers: (ResizeObserver | IntersectionObserver)[] = [];

  constructor(private readonly parts: Parts) {
    this.still = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    for (const layer of parts.foils) {
      const foil = FOILS.find((one) => one.key === layer.dataset.foil); if (!foil) continue;
      const box = foilBox(foil);
      Object.assign((layer.firstElementChild as HTMLElement).style, { width: `${box.w}px`, height: `${box.h}px`, background: foilCss(foil) });
      layer.style.opacity = String(foil.opacity); layer.style.mixBlendMode = foil.blend;
    }
    const glare = parts.glare.firstElementChild as HTMLElement;
    Object.assign(glare.style, { width: `${GLARE.size}px`, height: `${GLARE.size}px`, background: glareCss() });
    parts.glare.style.mixBlendMode = GLARE.blend;
    const resize = new ResizeObserver(() => this.resize()); resize.observe(parts.stage);
    const seen = new IntersectionObserver(([entry]) => { this.visible = entry.isIntersecting; this.wake(); }); seen.observe(parts.stage);
    this.observers.push(resize, seen);
    const at = (event: PointerEvent) => {
      const box = parts.stage.getBoundingClientRect();
      this.pointer = { x: ((event.clientX - box.left) / box.width - CENTER.x / STAGE.w) / 0.4, y: ((event.clientY - box.top) / box.height - CENTER.y / STAGE.h) / 0.42 };
      this.wake();
    };
    const away = () => { this.pointer = null; this.wake(); };
    parts.stage.onpointermove = (event) => { if (event.pointerType !== 'touch' || event.buttons) at(event); };
    parts.stage.onpointerdown = at;
    parts.stage.onpointerleave = away;
    parts.stage.onpointercancel = away;
    parts.stage.onpointerup = (event) => { if (event.pointerType === 'touch') away(); };
    this.resize();
    this.apply();
    this.wake();
  }

  stop(): void {
    cancelAnimationFrame(this.frame); this.frame = 0;
    this.observers.forEach((observer) => observer.disconnect());
    const stage = this.parts.stage;
    stage.onpointermove = stage.onpointerdown = stage.onpointerleave = stage.onpointercancel = stage.onpointerup = null;
  }

  private resize(): void {
    const { stage, space, backdrop: canvas } = this.parts;
    const width = stage.clientWidth; if (!width) return;
    const s = width / STAGE.w;
    space.style.transform = `scale(${s})`;
    const dpr = Math.min(2, window.devicePixelRatio || 1);
    canvas.width = Math.round(STAGE.w * s * dpr); canvas.height = Math.round(STAGE.h * s * dpr);
    const ctx = canvas.getContext('2d'); if (!ctx) return;
    ctx.scale(s * dpr, s * dpr); backdrop(ctx, STAGE.w, STAGE.h);
  }

  private wake(): void {
    if (this.frame || !this.visible || this.still) return;
    this.last = performance.now();
    this.frame = requestAnimationFrame((now) => this.tick(now));
  }

  private tick(now: number): void {
    this.frame = 0;
    const seconds = Math.min(0.05, (now - this.last) / 1000); this.last = now;
    const goal = this.pointer ? aimed(this.pointer) : drift(now / 1000);
    const rate = this.pointer ? 9 : 3;
    this.pose = approach(this.pose, goal.pose, rate, seconds);
    this.light = approach(this.light, goal.light, rate * 0.8, seconds);
    this.apply();
    if (this.visible) this.frame = requestAnimationFrame((t) => this.tick(t));
  }

  private apply(): void {
    const { ticket, shadow, halo, foils, glare } = this.parts;
    ticket.style.transform = css(ticketMatrix(this.pose));
    shadow.style.transform = css(placed(SHADOW, this.pose));
    halo.style.transform = css(placed(HALO, this.pose));
    for (const layer of foils) {
      const foil = FOILS.find((one) => one.key === layer.dataset.foil); if (!foil) continue;
      const [fx, fy] = foilAt(foil, this.light);
      (layer.firstElementChild as HTMLElement).style.transform = `translate(${fx.toFixed(2)}px, ${fy.toFixed(2)}px)`;
    }
    const [gx, gy] = glareAt(this.light);
    (glare.firstElementChild as HTMLElement).style.transform = `translate(${gx.toFixed(1)}px, ${gy.toFixed(1)}px)`;
  }
}
