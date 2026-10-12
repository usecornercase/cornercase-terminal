import { Grid, BOLD, rect } from '../term/grid';
import { CanvasView } from '../term/canvas';
import { type Stats, type WindowName, LABELS, count, duration, tokens, factor, ramp } from './model';

export function cardGrid(stats: Stats, window: WindowName): Grid {
  const grid = new Grid(100, 16);
  const n = stats.windows[window];
  grid.box(rect(0, 0, 100, 16), { fg: 239 }, 'Usage and stats', { add: BOLD });
  const rows = [['agents started', count(n[0])], ['prompts', count(n[1])], ['agent time', duration(n[2])], ['most at once', count(n[4])], ['tokens out', tokens(n[5])]];
  rows.forEach(([label, value], i) => { grid.text(4, i + 2, label, { fg: 8 }); grid.text(28 - value.length, i + 2, value, { add: BOLD }); });
  const weeks = 26;
  for (let row = 0; row < 7; row++) for (let week = 0; week < weeks; week++) {
    const i = (weeks - 1 - week) * 7 + 6 - row;
    const level = stats.levels[i];
    if (level !== undefined) grid.put(44 + week * 2, 3 + row, '■', { fg: ramp[level] });
  }
  grid.text(77, 11, 'less · ', { fg: 8 });
  for (let level = 1; level <= 4; level++) grid.put(82 + level * 2, 11, '■', { fg: ramp[level] });
  grid.text(92, 11, 'more', { fg: 8 });
  grid.text(4, 13, '26 weeks · one block per 24 h · newest bottom right', { fg: 8 });
  return grid;
}

export async function fonts(): Promise<void> {
  await Promise.all(['400 20px "JetBrains Mono Variable"', '700 64px "Bricolage Grotesque Variable"', '400 20px "Geist Variable"'].map((font) => document.fonts.load(font)));
}

export function drawCard(canvas: HTMLCanvasElement, stats: Stats, window: WindowName, example = false, preview = false): void {
  canvas.width = 2400; canvas.height = preview ? 1260 : 1350;
  const ctx = canvas.getContext('2d');
  if (!ctx) throw new Error('no 2d context');
  ctx.scale(2, 2);
  ctx.fillStyle = '#0b0911'; ctx.fillRect(0, 0, 1200, 675);
  const glow = ctx.createRadialGradient(850, 100, 0, 850, 100, 800);
  glow.addColorStop(0, '#33204e'); glow.addColorStop(1, '#0b0911');
  ctx.fillStyle = glow; ctx.fillRect(0, 0, 1200, 675);
  ctx.fillStyle = '#ffffff0b';
  for (let y = 12; y < 675; y += 24) for (let x = 12; x < 1200; x += 24) { ctx.beginPath(); ctx.arc(x, y, 1, 0, Math.PI * 2); ctx.fill(); }
  ctx.save(); ctx.translate(60, 38); ctx.scale(1.25, 1.25);
  ctx.fillStyle = '#0e0d14'; ctx.beginPath(); ctx.roundRect(0, 0, 32, 32, 7); ctx.fill();
  ctx.fillStyle = '#875fff'; ctx.fill(new Path2D('M12 7h14v5H12zM7 12h5v14H7z'));
  ctx.fillStyle = '#45d9e8'; ctx.fillRect(17, 17, 3, 8); ctx.restore();
  ctx.fillStyle = '#efebf8'; ctx.font = '700 30px "Bricolage Grotesque Variable"'; ctx.fillText('cornercase', 116, 73);
  ctx.font = '700 60px "Bricolage Grotesque Variable"'; ctx.fillText(preview ? 'cornercase stats' : factor(stats.windows[window]), 60, 152);
  const n = stats.windows[window];
  ctx.font = '400 21px "Geist Variable"'; ctx.fillStyle = '#bbb3cb';
  ctx.fillText(`${duration(n[2])} of agent work in ${duration(n[3])} · ${LABELS[window]}${example ? ' · example' : ''}`, 62, 192);
  const terminal = document.createElement('canvas'); const view = new CanvasView(terminal, undefined, true);
  view.measure(1080, 100, 16, 2); view.draw(cardGrid(stats, window), null, false, true);
  const y = preview ? 218 : 231; const h = preview ? 325 : 359.4;
  const cw = 1080 / 100, ch = h / 16;
  ctx.fillStyle = view.theme.background;
  ctx.beginPath(); ctx.roundRect(60 + cw / 2, y + ch / 2, 1080 - cw, h - ch, cw / 2); ctx.fill();
  ctx.drawImage(terminal, 60, y, 1080, h);
  ctx.font = '400 18px "Geist Variable"'; ctx.fillStyle = '#aaa2ba';
  ctx.fillText(`counting for ${stats.days} days`, 62, preview ? 588 : 634);
  ctx.textAlign = 'right'; ctx.fillStyle = '#cfc4ec'; ctx.fillText('usecornercase.dev', 1138, preview ? 588 : 634);
}
