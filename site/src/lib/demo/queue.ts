import { BOLD, type Rect, rect } from '../term/grid';
import { type Areas, Rows, bottom, right } from './layout';
import type { Painter } from './ui';

export interface QueueEntry {
  id: number;
  at: number;
  prompt: string;
  label: string;
  project: number;
  workspace: number | null;
  branch: string;
  agent: string;
  mode: string;
  command: string;
  state: 'pending' | 'started' | 'failed' | 'missed';
  pane?: number;
  error?: string;
}

export interface QueueForm {
  kind: 'queue';
  id?: number;
  prompt: string;
  label?: string;
  at: string;
  branch: string;
  project: number;
  workspace: number | null;
  agent: string;
  mode: string;
  worktree: boolean;
  field: 'prompt' | 'at' | 'branch';
  cursor: number;
  error?: string;
}

export function queueTime(text: string, now: number): number | null {
  const value = text.trim();
  if (value === 'claude reset' || value === 'codex reset') return now + 2 * 3600000;
  const delay = /^in ([1-9][0-9]*)(m|h|d)$/.exec(value);
  if (delay) return now + Number(delay[1]) * { m: 60000, h: 3600000, d: 86400000 }[delay[2] as 'm' | 'h' | 'd'];
  const base = new Date(now);
  const clock = /^(?:tomorrow )?(\d{1,2}):(\d{2})$/.exec(value);
  if (clock && Number(clock[1]) < 24 && Number(clock[2]) < 60) {
    base.setHours(Number(clock[1]), Number(clock[2]), 0, 0);
    if (value.startsWith('tomorrow ') || base.getTime() <= now) base.setDate(base.getDate() + 1);
    return base.getTime();
  }
  if (!/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}$/.test(value)) return null;
  const date = new Date(value.replace(' ', 'T'));
  const at = date.getTime();
  return Number.isFinite(at) && queueWhen(at) === value && at > now ? at : null;
}

export function queueWhen(at: number): string {
  const date = new Date(at);
  const pad = (number: number) => String(number).padStart(2, '0');
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

export function queueFormArea(cols: number, rows: number): Rect {
  const w = Math.min(72, Math.max(0, cols - 2));
  const h = Math.min(22, rows);
  return rect(Math.floor((cols - w) / 2), Math.floor((rows - h) / 2), w, h);
}

export function drawQueue(p: Painter, areas: Areas): void {
  const app = p.app;
  const area = areas.changes;
  p.g.clear(area);
  p.span(area.x + 1, area.y, `Queue ${app.queued.filter((e) => e.state === 'pending').length}`, { add: BOLD });
  const close = rect(right(area) - 3, area.y, 3, 1);
  p.span(close.x + 1, close.y, '×', { fg: 8 });
  p.region({ r: close, click: () => app.toggleQueue(), cursor: 'pointer' });
  if (app.queued.some((entry) => entry.state === 'started')) {
    const clear = rect(Math.max(area.x, close.x - 16), area.y, 15, 1);
    p.span(clear.x, clear.y, ' clear started ', { fg: 8 });
    p.region({ r: clear, click: () => { app.queued = app.queued.filter((entry) => entry.state !== 'started'); app.dirty(); }, cursor: 'pointer' });
  }
  const list = rect(area.x, area.y + 2, area.w, Math.max(0, area.h - 2));
  const rows = new Rows(list, app.queued.map(() => 5), 1, app.queueScroll);
  app.queued.forEach((entry, i) => {
    const r = rows.item(i);
    if (!r.h) return;
    if (p.hovered(r)) p.g.fill(r, { bg: 235 });
    const when = queueWhen(entry.at);
    p.span(r.x + 1, r.y, when, { fg: 8 }, r.w - 2);
    p.span(r.x + 1, r.y + 1, entry.label, {}, r.w - 5);
    p.span(r.x + 1, r.y + 2, `${entry.agent} · ${entry.branch ? 'worktree' : 'tab'}`, { fg: 8 }, r.w - 2);
    p.span(r.x + 1, r.y + 3, entry.error ? `failed · ${entry.error}` : entry.state, { fg: entry.state === 'failed' ? 1 : entry.state === 'started' ? 2 : 8 }, r.w - 14);
    p.region({ r, click: () => app.openQueueForm(entry), cursor: 'pointer' });
    const remove = rect(right(r) - 3, r.y + 1, 3, 1);
    if (p.hovered(r) || areas.compact) p.span(remove.x + 1, remove.y, '×', { fg: 8 });
    p.region({ r: remove, click: () => { app.queued = app.queued.filter((item) => item.id !== entry.id); app.dirty(); }, cursor: 'pointer' });
    if (entry.state !== 'started') {
      const run = rect(right(r) - 12, r.y + 3, 11, 1);
      p.span(run.x, run.y, ' start now ', { fg: 6 });
      p.region({ r: run, click: () => app.runQueue(entry), cursor: 'pointer' });
    }
  });
  const add = rows.buttonRect();
  p.span(add.x + 1, add.y, '+ new', { fg: 6 });
  p.region({ r: add, click: () => app.openQueueForm(), cursor: 'pointer' });
  p.region({ r: list, wheel: (dy) => { app.queueScroll = rows.scrolled(Math.sign(dy) * 3); app.dirty(); return true; } });
}

export function drawQueueForm(p: Painter, o: QueueForm): void {
  const app = p.app;
  const r = queueFormArea(app.cols, app.rows);
  p.g.clear(r);
  p.g.box(r, { fg: 8 }, 'Schedule an agent', { add: BOLD });
  const x = r.x + 2;
  const w = Math.max(0, r.w - 4);
  const row = (at: number, h = 1) => rect(x, r.y + 1 + at, w, h);
  p.span(x, row(0).y, 'Prompt', { fg: 8 });
  p.span(x, row(5).y, `When · ${Intl.DateTimeFormat().resolvedOptions().timeZone} · 14:30, tomorrow 9:00, in 2h`, { fg: 8 }, w);
  for (const [field, at, h] of [['prompt', 1, 3], ['at', 6, 1], ['branch', 13, 1]] as const) {
    const area = row(at, h);
    p.g.fill(area, { bg: 235 });
    const chars = [...o[field]];
    for (let line = 0; line < h; line++) p.span(x, area.y + line, chars.slice(line * w, (line + 1) * w).join(''), { add: BOLD }, w);
    p.region({ r: area, click: () => { o.field = field; o.cursor = [...o[field]].length; app.dirty(); }, cursor: 'text' });
    if (o.field === field && area.w) p.cursor = { shape: 'block', x: x + Math.min(o.cursor % w, w - 1), y: area.y + Math.min(Math.floor(o.cursor / w), h - 1) };
  }
  const project = app.projects.find((project) => project.id === o.project);
  const workspace = project?.workspaces.find((workspace) => workspace.id === o.workspace);
  for (const [at, label, action] of [
    [9, `Place: ${project?.name ?? project?.folder ?? ''}${workspace ? ` › ${workspace.name ?? workspace.branch}` : ' · root'} ›`, () => app.queuePlace(o)],
    [10, `Agent: ${o.agent} ›`, () => app.queueAgent(o)],
    [11, `Mode: ${o.mode || 'default'} ›`, () => app.queueMode(o)],
    [12, `${o.worktree ? '✓' : '○'} New worktree · branch below`, () => { if (project?.repo && !workspace) o.worktree = !o.worktree; app.dirty(); }],
  ] as const) {
    const area = row(at);
    p.span(x, area.y, label, { fg: 8 }, w);
    p.region({ r: area, click: action, cursor: 'pointer' });
  }
  for (const [offset, label] of [[0, 'claude reset'], [15, 'codex reset']] as const) {
    const area = rect(x + offset, row(7).y, 14, 1);
    p.span(area.x, area.y, ` ${label} `, { fg: 8 });
    p.region({ r: area, click: () => { o.at = label; o.field = 'at'; o.cursor = label.length; app.dirty(); }, cursor: 'pointer' });
  }
  p.span(x, row(15).y, 'Queued agents always accept their folder trust question.', { fg: 8 }, w);
  if (o.error) p.span(x, row(17).y, o.error, { fg: 1 }, w);
  const cancel = rect(right(r) - 10, bottom(r) - 2, 8, 1);
  const submit = rect(cancel.x - 11, cancel.y, 10, 1);
  p.span(submit.x, submit.y, ' schedule ', { fg: 6, add: BOLD });
  p.span(cancel.x, cancel.y, ' cancel ', { fg: 8 });
  p.region({ r: submit, click: () => app.submitQueue(), cursor: 'pointer' });
  p.region({ r: cancel, click: () => app.closeOverlay(), cursor: 'pointer' });
}
