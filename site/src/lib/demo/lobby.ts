import { BOLD, type Rect, type Style, rect } from '../term/grid';
import { DEFAULT } from '../term/palette';
import { IDEAS, type Lobby } from './bots';
import { wrapText } from './botchat';
import { bottom, isEmpty } from './layout';
import { type Line, seg, truncateRight, width } from './text';
import type { Painter } from './ui';

export const NEW_BOT = '+ New bot';
const WIDTH = 72;
const MARGIN = 2;
const NAME_COLUMN = 10;
const WHAT = 'A bot is an agent of your own, outside any project: a name, a folder, a memory and a chat.';
const SETS_UP = 'Tell it what it is for and it sets itself up, schedules its own work and answers here.';
const PICK = 'Pick a bot in the list to read its chat and talk to it.';
const FROM_AN_IDEA = 'Start from an idea';
const DIM: Style = { fg: 8 };
const CYAN: Style = { fg: 6 };

type Hit = 'new' | number;

interface Row {
  line: Line;
  hit?: { hit: Hit; x: number; w: number };
}

const HEADINGS: Record<Lobby, string> = { empty: 'Meet your bots', pick: 'Your bots' };
const LINES: Record<Lobby, string[]> = { empty: [WHAT, SETS_UP], pick: [PICK] };

function column(pane: Rect): Rect {
  const w = Math.max(0, Math.min(pane.w - 2 * MARGIN, WIDTH));
  return rect(pane.x + Math.floor((pane.w - w) / 2), pane.y, w, pane.h);
}

function centred(text: string, w: number, style: Style): Line {
  return [seg(' '.repeat(Math.max(0, Math.floor((w - width([seg(text)])) / 2)))), seg(text, style)];
}

function ideaRows(at: number, [name, about]: [string, string], w: number): Row[] {
  const lead = (first: boolean): string => (first ? truncateRight(name, NAME_COLUMN - 1) : '').padEnd(NAME_COLUMN);
  return wrapText(about, Math.max(1, w - NAME_COLUMN)).map((text, i) => ({
    line: [seg(lead(i === 0), { fg: 6, add: BOLD }), seg(text, { fg: 7 })],
    hit: { hit: at, x: 0, w },
  }));
}

function rows(lobby: Lobby, w: number): Row[] {
  const out: Row[] = [{ line: centred(HEADINGS[lobby], w, { fg: DEFAULT, add: BOLD }) }, { line: [] }];
  for (const text of LINES[lobby].flatMap((t) => wrapText(t, Math.max(1, w)))) out.push({ line: centred(text.trimEnd(), w, DIM) });
  const label = ` ${NEW_BOT} `;
  const lead = Math.max(0, Math.floor((w - label.length) / 2));
  out.push({ line: [] }, { line: [seg(' '.repeat(lead)), seg(label, CYAN)], hit: { hit: 'new', x: lead, w: label.length } });
  if (lobby === 'empty') {
    out.push({ line: [] }, { line: [seg(FROM_AN_IDEA, { fg: 8, add: BOLD })] }, { line: [] });
    IDEAS.forEach((idea, at) => out.push(...ideaRows(at, idea, w)));
  }
  return out;
}

export function drawLobby(p: Painter, pane: Rect, lobby: Lobby): void {
  const app = p.app;
  const col = column(pane);
  if (isEmpty(col)) return;
  const all = rows(lobby, col.w);
  const top = pane.y + Math.floor((pane.h - Math.min(all.length, pane.h)) / 2);
  const placed = all.map((row, i) => ({ row, r: rect(col.x, top + i, col.w, 1) })).filter(({ r }) => r.y < bottom(pane));
  IDEAS.forEach((_, at) => {
    const spans = placed.filter(({ row }) => row.hit?.hit === at).map(({ r }) => r);
    if (!spans.length) return;
    const span = rect(spans[0].x, spans[0].y, spans[0].w, spans[spans.length - 1].y + 1 - spans[0].y);
    if (p.sidebarHovered(span)) p.g.fill(span, { bg: p.hoverSurface });
    p.region({ r: span, click: () => app.openBotIdea(at), cursor: 'pointer' });
  });
  for (const { row, r } of placed) {
    const hit = row.hit;
    if (hit?.hit !== 'new') {
      p.line(r, row.line);
      continue;
    }
    const button = rect(r.x + hit.x, r.y, Math.min(hit.w, Math.max(0, r.w - hit.x)), 1);
    const line = [...row.line];
    line[line.length - 1] = seg(` ${NEW_BOT} `, p.buttonStyle(button, CYAN, 6));
    p.line(r, line);
    p.region({ r: button, click: () => app.openBotForm(), cursor: 'pointer' });
  }
}
