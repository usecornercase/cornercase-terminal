import { drawFace, drawFoil, drawShape, outline } from './face';
import { FOILS, paintSheen } from './sheen';
import { type Light, type Matrix, type Pose, LIT, REST, STAGE, TICKET, haloMatrix, multiply, project, shadowMatrix, ticketMatrix, translate } from './pose';
import type { Face } from './ticket';

type Ctx = CanvasRenderingContext2D;
export interface Blur { pad: number; blur: number; colour: string; texel: number; at: (pose: Pose) => Matrix }
export const SHADOW: Blur = { pad: 90, blur: 30, colour: 'rgba(3,2,8,0.8)', texel: 1, at: shadowMatrix };
export const HALO: Blur = { pad: 220, blur: 90, colour: 'rgba(135,95,255,0.5)', texel: 0.5, at: haloMatrix };
export const placed = (b: Blur, pose: Pose): Matrix => multiply(b.at(pose), translate(-b.pad, -b.pad));

function canvas(w: number, h: number): [HTMLCanvasElement, Ctx] {
  const c = document.createElement('canvas'); c.width = Math.round(w); c.height = Math.round(h);
  const ctx = c.getContext('2d'); if (!ctx) throw new Error('no 2d context');
  return [c, ctx];
}

export interface Textures { face: HTMLCanvasElement; foils: Record<string, HTMLCanvasElement>; shape: HTMLCanvasElement }

export function textures(f: Face, scale: number): Textures {
  const make = (draw: (ctx: Ctx) => void) => { const [c, ctx] = canvas(TICKET.w * scale, TICKET.h * scale); ctx.scale(scale, scale); draw(ctx); return c; };
  return { face: make((ctx) => drawFace(ctx, f)), foils: Object.fromEntries(FOILS.map((foil) => [foil.key, make((ctx) => drawFoil(ctx, f, foil.key))])), shape: make(drawShape) };
}

export function blurred(b: Blur): HTMLCanvasElement {
  const { pad, blur, colour, texel: scale } = b;
  const [c, ctx] = canvas((TICKET.w + pad * 2) * scale, (TICKET.h + pad * 2) * scale);
  ctx.scale(scale, scale);
  ctx.translate(pad - 10000, pad);
  ctx.shadowColor = colour; ctx.shadowBlur = blur * 2 * scale; ctx.shadowOffsetX = 10000 * scale;
  ctx.fillStyle = '#000'; outline(ctx); ctx.fill();
  return c;
}

export function backdrop(ctx: Ctx, w: number, h: number): void {
  ctx.fillStyle = '#07060b'; ctx.fillRect(0, 0, w, h);
  const blob = (x: number, y: number, rx: number, ry: number, colour: string) => {
    ctx.save(); ctx.translate(x, y); ctx.scale(rx / ry, 1);
    const g = ctx.createRadialGradient(0, 0, 0, 0, 0, ry); g.addColorStop(0, colour); g.addColorStop(1, 'rgba(0,0,0,0)');
    ctx.fillStyle = g; ctx.fillRect(-ry, -ry, ry * 2, ry * 2); ctx.restore();
  };
  blob(w * 0.16, h * 0.12, 620, 420, 'rgba(135,95,255,0.30)');
  blob(w * 0.9, h * 0.32, 640, 430, 'rgba(69,217,232,0.12)');
  blob(w * 0.52, h * 0.88, 820, 480, 'rgba(135,95,255,0.20)');
  for (let y = 12; y < h; y += 24) for (let x = 12; x < w; x += 24) {
    const dx = (x - w / 2) / (w * 0.62), dy = (y - h * 0.45) / (h * 0.7);
    const fade = Math.max(0, 1 - Math.hypot(dx, dy));
    if (fade <= 0) continue;
    ctx.fillStyle = `rgba(255,255,255,${(0.09 * fade).toFixed(3)})`; ctx.beginPath(); ctx.arc(x, y, 1, 0, Math.PI * 2); ctx.fill();
  }
}

function affine(s: [number, number][], d: [number, number][]): [number, number, number, number, number, number] {
  const [[x0, y0], [x1, y1], [x2, y2]] = s;
  const [[u0, v0], [u1, v1], [u2, v2]] = d;
  const det = (x1 - x0) * (y2 - y0) - (x2 - x0) * (y1 - y0);
  const a = ((u1 - u0) * (y2 - y0) - (u2 - u0) * (y1 - y0)) / det;
  const c = ((u2 - u0) * (x1 - x0) - (u1 - u0) * (x2 - x0)) / det;
  const b = ((v1 - v0) * (y2 - y0) - (v2 - v0) * (y1 - y0)) / det;
  const dd = ((v2 - v0) * (x1 - x0) - (v1 - v0) * (x2 - x0)) / det;
  return [a, b, c, dd, u0 - a * x0 - c * y0, v0 - b * x0 - dd * y0];
}

export function warp(ctx: Ctx, image: HTMLCanvasElement, m: Matrix, w: number, h: number, columns = 40, rows = 20): void {
  const texel = image.width / w;
  const grow = 0.45;
  for (let row = 0; row < rows; row++) for (let col = 0; col < columns; col++) {
    const x0 = (col / columns) * w, x1 = ((col + 1) / columns) * w, y0 = (row / rows) * h, y1 = ((row + 1) / rows) * h;
    const p00 = project(m, x0, y0), p10 = project(m, x1, y0), p01 = project(m, x0, y1), p11 = project(m, x1, y1);
    for (const [src, dst] of [[[[x0, y0], [x1, y0], [x0, y1]], [p00, p10, p01]], [[[x1, y1], [x0, y1], [x1, y0]], [p11, p01, p10]]] as [[number, number][], [number, number][]][]) {
      const cx = (dst[0][0] + dst[1][0] + dst[2][0]) / 3, cy = (dst[0][1] + dst[1][1] + dst[2][1]) / 3;
      ctx.save();
      ctx.beginPath();
      dst.forEach(([x, y], i) => {
        const len = Math.hypot(x - cx, y - cy) || 1;
        const px = x + (x - cx) / len * grow, py = y + (y - cy) / len * grow;
        if (i === 0) ctx.moveTo(px, py); else ctx.lineTo(px, py);
      });
      ctx.closePath(); ctx.clip();
      ctx.transform(...affine(src.map(([x, y]) => [x * texel, y * texel]), dst));
      const sx = Math.max(0, Math.floor(x0 * texel) - 2), sy = Math.max(0, Math.floor(y0 * texel) - 2);
      const sw = Math.min(image.width - sx, Math.ceil((x1 - x0) * texel) + 4), sh = Math.min(image.height - sy, Math.ceil((y1 - y0) * texel) + 4);
      ctx.drawImage(image, sx, sy, sw, sh, sx, sy, sw, sh);
      ctx.restore();
    }
  }
}

export interface Shot { height?: number; scale?: number; pose?: Pose; light?: Light }

export function render(target: HTMLCanvasElement, f: Face, shot: Shot = {}): void {
  const scale = shot.scale ?? 2, height = shot.height ?? STAGE.h, pose = shot.pose ?? REST, light = shot.light ?? LIT;
  target.width = STAGE.w * scale; target.height = height * scale;
  const ctx = target.getContext('2d'); if (!ctx) throw new Error('no 2d context');
  ctx.imageSmoothingQuality = 'high';
  ctx.scale(scale, scale);
  backdrop(ctx, STAGE.w, height);
  ctx.translate(0, (height - STAGE.h) / 2);
  for (const b of [HALO, SHADOW]) warp(ctx, blurred(b), placed(b, pose), TICKET.w + b.pad * 2, TICKET.h + b.pad * 2, 12, 8);
  const t = textures(f, scale);
  const fctx = t.face.getContext('2d');
  if (fctx) paintSheen(fctx, t.foils, t.shape, light, scale);
  warp(ctx, t.face, ticketMatrix(pose), TICKET.w, TICKET.h);
}
