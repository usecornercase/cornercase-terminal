import { type Grid, imageBox } from './grid';
import { type Metrics, paint } from './paint';
import { theme as defaultTheme } from './palette';

const esc = (s: string) =>
  s.replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c] ?? c);

const n = (v: number) => (Math.round(v * 100) / 100).toString();

let clips = 0;

export interface SvgOptions extends Partial<Metrics> {
  label: string;
  className?: string;
}

export function toSvg(grid: Grid, opts: SvgOptions): string {
  const cw = opts.cw ?? 6;
  const ch = opts.ch ?? 12.6;
  const m: Metrics = { cw, ch, lw: opts.lw ?? 0.9, theme: opts.theme, dim: opts.dim };
  const t = m.theme ?? defaultTheme;
  const width = grid.cols * cw;
  const height = grid.rows * ch;
  const size = cw / 0.6;
  const parts: string[] = [];
  for (const op of paint(grid, m)) {
    const alpha = op.alpha < 1 ? ` opacity="${n(op.alpha)}"` : '';
    if (op.k === 'rect') {
      parts.push(`<rect x="${n(op.x)}" y="${n(op.y)}" width="${n(op.w)}" height="${n(op.h)}" fill="${op.fill}"${alpha}/>`);
    } else if (op.k === 'path') {
      parts.push(`<path d="${op.d}" fill="none" stroke="${op.stroke}" stroke-width="${n(op.lw)}" stroke-linecap="round" stroke-linejoin="round"${alpha}/>`);
    } else if (op.k === 'circle') {
      parts.push(`<circle cx="${n(op.cx)}" cy="${n(op.cy)}" r="${n(op.r)}" fill="${op.fill}"${alpha}/>`);
    } else {
      const weight = op.bold ? ' font-weight="700"' : '';
      const style = op.italic ? ' font-style="italic"' : '';
      const y = n(op.y + ch / 2);
      if (op.single) {
        parts.push(`<text x="${n(op.x + cw / 2)}" y="${y}" text-anchor="middle" fill="${op.fill}"${weight}${style}${alpha}>${esc(op.text)}</text>`);
      } else {
        const fit = op.cells > 1 ? ` textLength="${n(op.cells * cw)}" lengthAdjust="spacing"` : '';
        parts.push(`<text x="${n(op.x)}" y="${y}" fill="${op.fill}"${fit}${weight}${style}${alpha} xml:space="preserve">${esc(op.text)}</text>`);
      }
    }
  }
  for (const image of grid.images) {
    const runs = grid.imageRuns(image);
    if (!runs.length) continue;
    const id = `term-image-${clips++}`;
    const box = imageBox(image, cw, ch);
    const cells = runs.map((r) => `<rect x="${n(r.x * cw)}" y="${n(r.y * ch)}" width="${n(r.w * cw)}" height="${n(r.h * ch)}"/>`);
    parts.push(`<clipPath id="${id}">${cells.join('')}</clipPath>`);
    parts.push(`<image href="${esc(image.src)}" x="${n(box.x)}" y="${n(box.y)}" width="${n(box.w)}" height="${n(box.h)}" preserveAspectRatio="none" clip-path="url(#${id})"/>`);
  }
  const cls = opts.className ? ` class="${opts.className}"` : '';
  return [
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${n(width)} ${n(height)}"${cls} role="img" aria-label="${esc(opts.label)}" font-size="${n(size)}" dominant-baseline="central">`,
    `<rect width="${n(width)}" height="${n(height)}" fill="${t.background}"/>`,
    ...parts,
    '</svg>',
  ].join('');
}
