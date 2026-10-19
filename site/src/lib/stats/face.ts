import { TICKET } from './pose';
import { type Face, WEEKS, block } from './ticket';

type Ctx = CanvasRenderingContext2D;

const DISPLAY = '"Archivo Variable", ui-sans-serif, sans-serif';
const BODY = '"Archivo Variable", ui-sans-serif, sans-serif';
const MONO = '"JetBrains Mono Variable", ui-monospace, monospace';
const SHAPE = { radius: 30, stub: 790, notch: 18, pad: 52 };
const LEVELS = ['rgba(255,255,255,0.065)', '#12306e', '#0a47c2', '#2f74ff', '#a8c6ff'];

export function outline(ctx: Ctx, inset = 0): void {
  const { w, h } = TICKET; const r = SHAPE.radius - inset, s = SHAPE.stub, n = SHAPE.notch + inset, i = inset;
  ctx.beginPath();
  ctx.moveTo(i + r, i);
  ctx.lineTo(s - n, i);
  ctx.arc(s, 0, n, Math.PI, 0, true);
  ctx.lineTo(w - i - r, i);
  ctx.arc(w - i - r, i + r, r, -Math.PI / 2, 0);
  ctx.lineTo(w - i, h - i - r);
  ctx.arc(w - i - r, h - i - r, r, 0, Math.PI / 2);
  ctx.lineTo(s + n, h - i);
  ctx.arc(s, h, n, 0, Math.PI, true);
  ctx.lineTo(i + r, h - i);
  ctx.arc(i + r, h - i - r, r, Math.PI / 2, Math.PI);
  ctx.lineTo(i, i + r);
  ctx.arc(i + r, i + r, r, Math.PI, Math.PI * 1.5);
  ctx.closePath();
}

interface Cell { label: string; value: string; x: number; line: number }
interface Layout { heroSize: number; heroWidth: number; side: number; valueSize: number; cells: Cell[]; pill: number }

const LINE_Y = [338, 410];
const heroY = (f: Face): number => f.parallel ? 268 : 236;

function tracked(ctx: Ctx, text: string, spacing: number): number {
  return [...text].reduce((sum, ch) => sum + ctx.measureText(ch).width + spacing, -spacing);
}
function drawTracked(ctx: Ctx, text: string, x: number, y: number, spacing: number): void {
  for (const ch of text) { ctx.fillText(ch, x, y); x += ctx.measureText(ch).width + spacing; }
}

function measure(ctx: Ctx, f: Face): Layout {
  const inner = SHAPE.stub - SHAPE.pad * 2;
  ctx.font = `600 40px ${DISPLAY}`;
  const side = f.parallel ? Math.max(ctx.measureText('in parallel').width, 210) : 0;
  let heroSize = f.parallel ? 176 : 92;
  ctx.font = `800 ${heroSize}px ${DISPLAY}`;
  let heroWidth = tracked(ctx, f.hero, -heroSize * 0.035);
  const room = inner - (side ? side + 30 : 0);
  if (heroWidth > room) { heroSize *= room / heroWidth; ctx.font = `800 ${heroSize}px ${DISPLAY}`; heroWidth = tracked(ctx, f.hero, -heroSize * 0.035); }
  const columns = Math.max(...f.lines.map((line) => line.length));
  let valueSize = 30;
  const widths = (): number[] => Array.from({ length: columns }, (_, col) => Math.max(...f.lines.map((line) => {
    const cell = line[col]; if (!cell) return 0;
    ctx.font = `400 12.5px ${MONO}`; const label = ctx.measureText(cell[0]).width;
    ctx.font = `700 ${valueSize}px ${DISPLAY}`; return Math.max(label, ctx.measureText(cell[1]).width);
  })));
  let sizes = widths();
  while (valueSize > 18 && sizes.reduce((a, b) => a + b, 0) + (columns - 1) * 22 > inner) { valueSize -= 1; sizes = widths(); }
  const gap = columns > 1 ? (inner - sizes.reduce((a, b) => a + b, 0)) / (columns - 1) : 0;
  const xs = sizes.map((_, col) => SHAPE.pad + sizes.slice(0, col).reduce((a, b) => a + b, 0) + gap * col);
  const cells = f.lines.flatMap((line, l) => line.map(([label, value], col) => ({ label, value, x: xs[col], line: l })));
  ctx.font = `500 12px ${MONO}`;
  const pill = tracked(ctx, badge(f), 1.8) + 26;
  return { heroSize, heroWidth, side, valueSize, cells, pill };
}

const badge = (f: Face): string => `${f.example ? 'EXAMPLE · ' : ''}${f.window.toUpperCase()}`;

function glow(ctx: Ctx, x: number, y: number, r: number, colour: string): void {
  const gradient = ctx.createRadialGradient(x, y, 0, x, y, r);
  gradient.addColorStop(0, colour); gradient.addColorStop(1, 'rgba(0,0,0,0)');
  ctx.fillStyle = gradient; ctx.fillRect(x - r, y - r, r * 2, r * 2);
}

function logo(ctx: Ctx, x: number, y: number, size: number, corner: string, cursor: string): void {
  ctx.save(); ctx.translate(x, y); ctx.scale(size / 32, size / 32); ctx.translate(7, 7); ctx.scale(19 / 12, 19 / 12);
  ctx.fillStyle = corner; ctx.fill(new Path2D('M3 0h9v3H3zM0 3h3v9H0z'));
  ctx.fillStyle = cursor; ctx.fill(new Path2D('M6 6h2v5H6z'));
  ctx.restore();
}

function squares(ctx: Ctx, f: Face, paint: (level: number) => string | null, lit = false): void {
  const size = 11, pitch = 14.5;
  const x0 = SHAPE.stub + (TICKET.w - SHAPE.stub) / 2 - (pitch * 6 + size) / 2, y0 = 76;
  const scale = ctx.getTransform().a;
  for (let week = 0; week < WEEKS; week++) for (let day = 0; day < 7; day++) {
    const level = f.levels[block(week, day)] ?? 0;
    const fill = paint(level); if (!fill) continue;
    ctx.save();
    if (lit && level === 4) { ctx.shadowColor = 'rgba(160,128,255,0.85)'; ctx.shadowBlur = 9 * scale; }
    ctx.fillStyle = fill; ctx.beginPath(); ctx.roundRect(x0 + day * pitch, y0 + week * pitch, size, size, 2.5); ctx.fill();
    ctx.restore();
  }
}

function hero(ctx: Ctx, f: Face, layout: Layout): void {
  ctx.font = `800 ${layout.heroSize}px ${DISPLAY}`;
  drawTracked(ctx, f.hero, SHAPE.pad - 4, heroY(f), -layout.heroSize * 0.035);
}

function guilloche(ctx: Ctx): void {
  ctx.lineWidth = 1;
  for (let k = 0; k < 22; k++) {
    ctx.strokeStyle = `rgba(190,170,255,${0.018 + (k % 3 === 0 ? 0.016 : 0)})`;
    ctx.beginPath();
    for (let x = -10; x <= SHAPE.stub + 10; x += 5) {
      const y = 18 + k * 23 + 13 * Math.sin(x / 68 + k * 0.55) + 5 * Math.sin(x / 21 + k * 1.3);
      if (x === -10) ctx.moveTo(x, y); else ctx.lineTo(x, y);
    }
    ctx.stroke();
  }
}

export function drawFace(ctx: Ctx, f: Face): void {
  const { w, h } = TICKET;
  const layout = measure(ctx, f);
  ctx.save();
  outline(ctx); ctx.clip();
  const base = ctx.createLinearGradient(0, 0, w, h);
  base.addColorStop(0, '#171c28'); base.addColorStop(0.5, '#10131b'); base.addColorStop(1, '#0b0d12');
  ctx.fillStyle = base; ctx.fillRect(0, 0, w, h);
  glow(ctx, 200, 210, 470, 'rgba(0,95,255,0.26)');
  glow(ctx, 990, 20, 340, 'rgba(168,198,255,0.08)');
  guilloche(ctx);
  ctx.fillStyle = 'rgba(6,8,12,0.32)'; ctx.fillRect(SHAPE.stub, 0, w - SHAPE.stub, h);
  const sheen = ctx.createLinearGradient(0, 0, 0, 160);
  sheen.addColorStop(0, 'rgba(255,255,255,0.06)'); sheen.addColorStop(1, 'rgba(255,255,255,0)');
  ctx.fillStyle = sheen; ctx.fillRect(0, 0, w, 160);
  ctx.restore();

  ctx.save(); outline(ctx, 0.75); ctx.lineWidth = 1.5;
  const edge = ctx.createLinearGradient(0, 0, w, h);
  edge.addColorStop(0, 'rgba(255,255,255,0.42)'); edge.addColorStop(0.45, 'rgba(170,195,255,0.14)'); edge.addColorStop(1, 'rgba(255,255,255,0.1)');
  ctx.strokeStyle = edge; ctx.stroke(); ctx.restore();

  ctx.save(); ctx.strokeStyle = 'rgba(255,255,255,0.22)'; ctx.lineWidth = 1.5; ctx.setLineDash([5, 7]);
  ctx.beginPath(); ctx.moveTo(SHAPE.stub, SHAPE.notch + 10); ctx.lineTo(SHAPE.stub, h - SHAPE.notch - 10); ctx.stroke(); ctx.restore();

  logo(ctx, SHAPE.pad - 6, 44, 38, '#2f74ff', '#f3f5f9');
  ctx.fillStyle = '#f3f5f9'; ctx.font = `700 27px ${DISPLAY}`; ctx.textBaseline = 'alphabetic';
  ctx.fillText('cornercase', SHAPE.pad + 38, 73);

  const right = SHAPE.stub - SHAPE.pad;
  ctx.fillStyle = f.example ? 'rgba(245,215,110,0.12)' : 'rgba(0,95,255,0.18)'; ctx.strokeStyle = f.example ? 'rgba(245,215,110,0.6)' : 'rgba(150,185,255,0.42)'; ctx.lineWidth = 1;
  ctx.beginPath(); ctx.roundRect(right - layout.pill, 50, layout.pill, 28, 14); ctx.fill(); ctx.stroke();
  ctx.fillStyle = f.example ? '#f5d76e' : '#d6e2ff'; ctx.font = `500 12px ${MONO}`;
  drawTracked(ctx, badge(f), right - layout.pill + 13, 68.5, 1.8);

  const silver = ctx.createLinearGradient(0, heroY(f) - layout.heroSize * 0.75, 0, heroY(f));
  silver.addColorStop(0, '#ffffff'); silver.addColorStop(0.55, '#dfe7fb'); silver.addColorStop(1, '#8fa9e8');
  ctx.fillStyle = silver; hero(ctx, f, layout);

  if (f.parallel) {
    const x = SHAPE.pad + layout.heroWidth + 26;
    ctx.fillStyle = '#ffffff'; ctx.font = `600 40px ${DISPLAY}`; ctx.fillText('in parallel', x, 196);
    const pair = (y: number, parts: [string, boolean][]) => {
      let at = x;
      for (const [text, strong] of parts) { ctx.font = `${strong ? 600 : 400} 19px ${BODY}`; ctx.fillStyle = strong ? '#ffffff' : '#b4bccb'; ctx.fillText(text, at, y); at += ctx.measureText(text).width; }
    };
    pair(236, [[f.work, true], [' of agent work', false]]);
    pair(264, [['in ', false], [f.busy, true], [' of your time', false]]);
  }

  ctx.fillStyle = 'rgba(255,255,255,0.09)'; ctx.fillRect(SHAPE.pad, 302, SHAPE.stub - SHAPE.pad * 2, 1);
  for (const cell of layout.cells) {
    const y = f.lines.length === 1 ? (LINE_Y[0] + LINE_Y[1]) / 2 : LINE_Y[cell.line];
    ctx.fillStyle = '#8d95a6'; ctx.font = `400 12.5px ${MONO}`; ctx.fillText(cell.label, cell.x, y);
    ctx.fillStyle = '#f3f5f9'; ctx.font = `700 ${layout.valueSize}px ${DISPLAY}`; ctx.fillText(cell.value, cell.x, y + 35);
  }

  ctx.font = `400 12px ${MONO}`; ctx.fillStyle = '#8a91a1';
  ctx.fillText(f.days, SHAPE.pad, 484);

  const middle = SHAPE.stub + (w - SHAPE.stub) / 2;
  ctx.fillStyle = '#98a0b0'; ctx.font = `500 11px ${MONO}`;
  const label = 'AGENTS · 26 WEEKS';
  drawTracked(ctx, label, middle - tracked(ctx, label, 1.6) / 2, 60, 1.6);
  squares(ctx, f, (level) => LEVELS[level], true);
  ctx.save(); ctx.translate(TICKET.w - 17, TICKET.h / 2); ctx.rotate(Math.PI / 2);
  ctx.fillStyle = 'rgba(255,255,255,0.3)'; ctx.font = `500 9.5px ${MONO}`;
  const note = 'ONE BLOCK PER 24 H · NEWEST AT THE BOTTOM';
  drawTracked(ctx, note, -tracked(ctx, note, 1.3) / 2, 3, 1.3); ctx.restore();
  ctx.textAlign = 'center'; ctx.fillStyle = '#9fbcff'; ctx.font = `500 12.5px ${MONO}`;
  ctx.fillText('usecornercase.dev', middle, 484);
  ctx.textAlign = 'left';
}

export function drawFoil(ctx: Ctx, f: Face, key: string): void {
  const layout = measure(ctx, f);
  ctx.fillStyle = '#ffffff'; ctx.strokeStyle = '#ffffff';
  if (key === 'iris') { hero(ctx, f, layout); return; }
  ctx.save(); outline(ctx, 1); ctx.lineWidth = 2; ctx.globalAlpha = 0.75; ctx.stroke(); ctx.restore();
  const right = SHAPE.stub - SHAPE.pad;
  ctx.lineWidth = 1.2; ctx.beginPath(); ctx.roundRect(right - layout.pill, 50, layout.pill, 28, 14); ctx.stroke();
}

export function drawShape(ctx: Ctx): void {
  ctx.fillStyle = '#ffffff'; outline(ctx); ctx.fill();
}
