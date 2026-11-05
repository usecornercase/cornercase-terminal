import { BOLD, type Rect, type Style, UNDERLINE, rect } from '../term/grid';
import { drawPanelBorder } from './changes';
import { type Areas, right } from './layout';
import type { Painter } from './ui';

const DARK: Style = { fg: 8 };
const TITLE = 'Recap';
const SEPARATOR = ' · ';
const BODY_FROM = 3;
const BULLET = '▸ ';

export type Choice = 'now' | 'yesterday' | 'week';
export const CHOICES: [Choice, string][] = [
  ['now', 'now'],
  ['yesterday', 'yesterday'],
  ['week', '7 days'],
];

export interface Place {
  project: string;
  workspace: string;
  tab: string;
}

interface Seg {
  text: string;
  bold?: boolean;
  quote?: boolean;
  link?: Place;
}

interface Block {
  kind: 'heading' | 'bullet' | 'blank';
  segs: Seg[];
}

const dark: Place = { project: 'web-shop', workspace: 'feat/dark-mode', tab: 'claude' };
const gifts: Place = { project: 'web-shop', workspace: 'feat/gift-cards', tab: 'claude' };
const pagination: Place = { project: 'orders-api', workspace: 'fix/pagination', tab: 'codex' };
const blog: Place = { project: 'blog', workspace: 'default', tab: 'claude' };

const name = (p: Place): string => `${p.project} › ${p.workspace} › ${p.tab}`;
const link = (place: Place): Seg => ({ text: name(place), link: place, bold: false });
const text = (t: string): Seg => ({ text: t });
const quote = (t: string): Seg => ({ text: `"${t}"`, quote: true });
const heading = (t: string): Block => ({ kind: 'heading', segs: [{ text: t, bold: true }] });
const bullet = (...segs: Seg[]): Block => ({ kind: 'bullet', segs });
const blank: Block = { kind: 'blank', segs: [] };

export const RECAPS: Record<Choice, { line: string; blocks: Block[] }> = {
  now: {
    line: 'tue 4 nov 09:48 · Claude Code · 4 conversations · 38k tokens · 2 min ago',
    blocks: [
      heading('On you'),
      bullet(link(dark), text(': asked '), quote('Where does the checkout pick its colours?'), text(' (since 09:31)')),
      bullet(link(pagination), text(': waiting for src/orders.ts, the cursor column')),
      blank,
      heading('In progress'),
      bullet(link(gifts), text(': working 12m. You asked '), quote('Add gift cards to the checkout')),
      blank,
      heading('Done'),
      bullet(link(blog), text(': fixed the typos in the post on worktrees')),
    ],
  },
  yesterday: {
    line: 'yesterday · mon 3 nov · Claude Code · 3 conversations · 52k tokens · 1 day ago',
    blocks: [
      heading('Done'),
      bullet(link(blog), text(': you asked '), quote('Fix the typos in the post on worktrees'), text('; the post is ready')),
      blank,
      heading('In progress'),
      bullet(link(dark), text(': you asked '), quote('Use Theme everywhere. Keep the brand purple.'), text('; two screens are left')),
      blank,
      heading('On you'),
      bullet(link(pagination), text(': you asked '), quote('Paginate GET /orders with a cursor'), text('; it needs the column to sort by')),
    ],
  },
  week: {
    line: '7 days · Claude Code · 6 conversations · 190k tokens · 3 h ago',
    blocks: [
      heading('web-shop'),
      bullet(text('Dark mode and gift cards moved forward; '), link(dark), text(' still has two screens to go.')),
      blank,
      heading('orders-api'),
      bullet(text('Pagination is designed; '), link(pagination), text(' waits for the sort column. #212 is merged.')),
      blank,
      heading('blog'),
      bullet(text('The post on worktrees is corrected in '), link(blog), text(', nothing open.')),
    ],
  },
};

export class RecapPanel {
  open = false;
  choice: Choice = 'now';
  scroll = 0;
}

interface Run {
  x: number;
  text: string;
  seg: Seg;
  heading: boolean;
}

interface Row {
  bullet: boolean;
  runs: Run[];
}

function wrapBlock(block: Block, width: number): Row[] {
  if (block.kind === 'blank') return [{ bullet: false, runs: [] }];
  const indent = block.kind === 'bullet' ? BULLET.length : 0;
  const rows: Row[] = [{ bullet: block.kind === 'bullet', runs: [] }];
  let x = indent;
  for (const seg of block.segs) {
    for (const word of seg.text.split(/\s+/).filter(Boolean)) {
      const last = rows[rows.length - 1];
      const empty = last.runs.length === 0;
      if (!empty && x + 1 + word.length > width) {
        rows.push({ bullet: false, runs: [] });
        x = indent;
      } else if (!empty) x += 1;
      rows[rows.length - 1].runs.push({ x, text: word, seg, heading: block.kind === 'heading' });
      x += word.length;
    }
  }
  return rows;
}

export function recapRows(width: number, choice: Choice): Row[] {
  return RECAPS[choice].blocks.flatMap((b) => wrapBlock(b, Math.max(8, width)));
}

export function drawRecap(p: Painter, areas: Areas): void {
  const app = p.app;
  const panel = app.recap;
  const area = areas.changes;
  drawPanelBorder(p, areas);
  p.g.clear(area);
  const header = rect(area.x + 1, area.y, Math.max(0, area.w - 2), 1);
  p.span(header.x, header.y, TITLE, { add: BOLD });
  let x = header.x + TITLE.length + 3;
  CHOICES.forEach(([id, label], i) => {
    const r = rect(x, header.y, label.length, 1);
    const style: Style = id === panel.choice ? { fg: 6, add: BOLD } : p.hovered(r) ? { fg: 6 } : DARK;
    p.span(r.x, r.y, label, style, right(header) - r.x);
    p.region({ r, click: () => app.showRecap(id), cursor: 'pointer' });
    if (i + 1 < CHOICES.length) p.span(right(r), r.y, SEPARATOR, DARK, right(header) - right(r));
    x += label.length + SEPARATOR.length;
  });
  const close = rect(right(header) - 3, header.y, 3, 1);
  const refresh = rect(close.x - 3, header.y, 3, 1);
  p.span(refresh.x + 1, refresh.y, '↻', p.hovered(refresh) ? { fg: 6 } : DARK);
  p.region({ r: refresh, click: () => app.showRecap(panel.choice), cursor: 'pointer' });
  p.span(close.x + 1, close.y, '×', p.hovered(close) ? { fg: 1, add: BOLD } : DARK);
  p.region({ r: close, click: () => app.toggleRecap(), cursor: 'pointer' });
  p.span(area.x + 1, area.y + 1, RECAPS[panel.choice].line, DARK, Math.max(0, area.w - 2));
  drawBody(p, area, panel);
}

function drawBody(p: Painter, area: Rect, panel: RecapPanel): void {
  const app = p.app;
  const body = rect(area.x + 1, Math.min(area.y + BODY_FROM, area.y + area.h), Math.max(0, area.w - 2), Math.max(0, area.h - BODY_FROM));
  const rows = recapRows(body.w, panel.choice);
  const top = Math.min(panel.scroll, Math.max(0, rows.length - body.h));
  rows.slice(top, top + body.h).forEach((row, i) => {
    const y = body.y + i;
    if (row.bullet) p.span(body.x, y, BULLET, DARK, body.w);
    for (const run of row.runs) {
      const r = rect(body.x + run.x, y, run.text.length, 1);
      const seg = run.seg;
      let style: Style = {};
      if (seg.link) style = { fg: 6, add: p.hovered(r) ? UNDERLINE : 0 };
      else if (seg.quote) style = DARK;
      if (seg.bold || run.heading) style = { ...style, add: (style.add ?? 0) | BOLD };
      p.span(r.x, y, run.text, style, body.w - run.x);
      if (seg.link) {
        const place = seg.link;
        p.region({ r: rect(r.x, y, run.text.length + 1, 1), click: () => app.recapGo(place), cursor: 'pointer' });
      }
    }
  });
  const below = rows.length - top - body.h;
  if (below > 0 && body.h > 0) {
    const text = ` ↓ ${below} more `;
    p.span(right(body) - text.length, body.y + body.h - 1, text, DARK, text.length);
  }
  p.region({ r: body, wheel: (dy) => app.scrollRecap(dy, Math.max(0, rows.length - body.h)) });
}
