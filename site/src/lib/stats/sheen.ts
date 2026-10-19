import { type Light, TICKET } from './pose';

type Ctx = CanvasRenderingContext2D;
type Gradient = [number, string][];

export interface Foil { key: string; angle: number; period: number; colours: string[]; opacity: number; blend: GlobalCompositeOperation }
export const FOILS: Foil[] = [
  { key: 'iris', angle: 118, period: 520, colours: ['#7fa8ff', '#d3ddf5', '#ffffff', '#86d0ff', '#4f7dff'], opacity: 0.78, blend: 'multiply' },
  { key: 'edge', angle: 118, period: 520, colours: ['#86d0ff', '#e8eefc', '#9fe7ff', '#7fa8ff', '#4f7dff'], opacity: 0.9, blend: 'color-dodge' },
];
export const SWEEP = { x: 1100, y: 620 };
export const GLARE = {
  size: 2000, radius: 750, blend: 'soft-light' as GlobalCompositeOperation,
  spot: [[0, 'rgba(255,255,255,0.75)'], [0.28, 'rgba(255,255,255,0.32)'], [0.62, 'rgba(255,255,255,0.08)'], [1, 'rgba(255,255,255,0)']] as Gradient,
  streak: { angle: 104, stops: [[0, 'rgba(255,255,255,0)'], [0.455, 'rgba(255,255,255,0)'], [0.5, 'rgba(255,255,255,0.45)'], [0.545, 'rgba(255,255,255,0)'], [1, 'rgba(255,255,255,0)']] as Gradient },
};

const direction = (angle: number): [number, number] => { const a = angle * Math.PI / 180; return [Math.sin(a), -Math.cos(a)]; };
const percent = (stops: Gradient): string => stops.map(([at, colour]) => `${colour} ${+(at * 100).toFixed(3)}%`).join(', ');

export function foilBox(foil: Foil): { x: number; y: number; w: number; h: number } {
  const [dx, dy] = direction(foil.angle);
  return { x: -Math.max(0, dx * foil.period), y: -Math.max(0, dy * foil.period), w: TICKET.w + Math.abs(dx) * foil.period, h: TICKET.h + Math.abs(dy) * foil.period };
}

export function phase(foil: Foil, light: Light): number {
  const [dx, dy] = direction(foil.angle);
  const raw = (0.5 - light.x) * SWEEP.x * dx + (0.5 - light.y) * SWEEP.y * dy;
  return ((raw % foil.period) + foil.period) % foil.period;
}

export function foilAt(foil: Foil, light: Light): [number, number] {
  const [dx, dy] = direction(foil.angle), box = foilBox(foil), t = phase(foil, light);
  return [box.x + dx * t, box.y + dy * t];
}

export function foilCss(foil: Foil): string {
  const step = foil.period / foil.colours.length;
  return `repeating-linear-gradient(${foil.angle}deg, ${foil.colours.map((colour, i) => `${colour} ${+(i * step).toFixed(3)}px`).join(', ')}, ${foil.colours[0]} ${foil.period}px)`;
}

function mix(a: string, b: string, t: number): string {
  const part = (hex: string, i: number) => Number.parseInt(hex.slice(1 + i * 2, 3 + i * 2), 16);
  return `rgb(${[0, 1, 2].map((i) => Math.round(part(a, i) + (part(b, i) - part(a, i)) * t)).join(',')})`;
}

export function foilPaint(ctx: Ctx, foil: Foil): CanvasGradient {
  const box = foilBox(foil), [dx, dy] = direction(foil.angle);
  const length = Math.abs(box.w * dx) + Math.abs(box.h * dy);
  const gradient = ctx.createLinearGradient(box.w / 2 - dx * length / 2, box.h / 2 - dy * length / 2, box.w / 2 + dx * length / 2, box.h / 2 + dy * length / 2);
  const n = foil.colours.length, step = foil.period / n;
  for (let k = 0; k * step <= length; k++) gradient.addColorStop(k * step / length, foil.colours[k % n]);
  const last = Math.floor(length / step), rest = length / step - last;
  if (rest > 0) gradient.addColorStop(1, mix(foil.colours[last % n], foil.colours[(last + 1) % n], rest));
  return gradient;
}

export const glareAt = (light: Light): [number, number] => [light.x * TICKET.w - GLARE.size / 2, light.y * TICKET.h - GLARE.size / 2];
export const glareCss = (): string => `linear-gradient(${GLARE.streak.angle}deg, ${percent(GLARE.streak.stops)}), radial-gradient(circle ${GLARE.radius}px at 50% 50%, ${percent(GLARE.spot)})`;

export function glarePaint(ctx: Ctx): void {
  const c = GLARE.size / 2;
  const spot = ctx.createRadialGradient(c, c, 0, c, c, GLARE.radius);
  for (const [at, colour] of GLARE.spot) spot.addColorStop(at, colour);
  ctx.fillStyle = spot; ctx.fillRect(0, 0, GLARE.size, GLARE.size);
  const [dx, dy] = direction(GLARE.streak.angle), half = (Math.abs(GLARE.size * dx) + Math.abs(GLARE.size * dy)) / 2;
  const streak = ctx.createLinearGradient(c - dx * half, c - dy * half, c + dx * half, c + dy * half);
  for (const [at, colour] of GLARE.streak.stops) streak.addColorStop(at, colour);
  ctx.fillStyle = streak; ctx.fillRect(0, 0, GLARE.size, GLARE.size);
}

export function paintSheen(ctx: Ctx, masks: Record<string, HTMLCanvasElement>, shape: HTMLCanvasElement, light: Light, scale: number): void {
  const layer = document.createElement('canvas'); layer.width = shape.width; layer.height = shape.height;
  const l = layer.getContext('2d'); if (!l) return;
  const put = (mask: HTMLCanvasElement, blend: GlobalCompositeOperation, opacity: number, at: [number, number], paint: () => void) => {
    l.setTransform(1, 0, 0, 1, 0, 0); l.globalCompositeOperation = 'source-over'; l.clearRect(0, 0, layer.width, layer.height);
    l.scale(scale, scale); l.translate(...at); paint();
    l.setTransform(1, 0, 0, 1, 0, 0); l.globalCompositeOperation = 'destination-in'; l.drawImage(mask, 0, 0);
    ctx.save(); ctx.setTransform(1, 0, 0, 1, 0, 0); ctx.globalAlpha = opacity; ctx.globalCompositeOperation = blend; ctx.drawImage(layer, 0, 0); ctx.restore();
  };
  for (const foil of FOILS) {
    const box = foilBox(foil);
    put(masks[foil.key], foil.blend, foil.opacity, foilAt(foil, light), () => { l.fillStyle = foilPaint(l, foil); l.fillRect(0, 0, box.w, box.h); });
  }
  put(shape, GLARE.blend, 1, glareAt(light), () => glarePaint(l));
}
