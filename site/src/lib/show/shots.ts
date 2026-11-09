import { issuesArea } from '../demo/layout';
import { span } from './math';
import { CHAPTERS, COMPACT, type Chapter, IDLE, LEAD, type Mark, TIMES, changesWidth } from './script';
import { type Box, type Key, type Shot, type Size, type View, WIDE, cells, freeArea, windowOf } from './track';

export const RUNS: Record<Chapter, [number, number]> = {
  start: [0.1, 0.9],
  glance: [0.1, 0.9],
  review: [0.08, 0.9],
  leave: [0.08, 0.9],
};

export function storyAt(progress: (id: Chapter) => number): number {
  let at = LEAD;
  for (const name of CHAPTERS) {
    const [from, to] = TIMES.chapters[name];
    const [a, b] = RUNS[name];
    const p = progress(name);
    at = from + span(p, a, b) * (to - from);
    if (p < 1) break;
  }
  return at;
}

export function idleAt(ms: number): number {
  const [from, to] = IDLE;
  if (ms < to) return Math.max(0, ms);
  const lap = (ms - to) % ((to - from) * 2);
  return lap < to - from ? to - lap : from + lap - (to - from);
}

export function whereIs(ms: number): { chapter: Chapter; progress: number } {
  for (const name of CHAPTERS) {
    const [from, to] = TIMES.chapters[name];
    if (ms > to && name !== 'leave') continue;
    const [a, b] = RUNS[name];
    return { chapter: name, progress: a + span(ms, from, to) * (b - a) };
  }
  return { chapter: 'leave', progress: 1 };
}

export type Place = (id: string, progress: number) => number;

interface Frames {
  whole: Box;
  dialog: Box;
  left: Box;
  work: Box;
  panel: Box;
  prompt: Box;
  menu: Box;
}

function frames(size: Size, view: View): Frames {
  const { cols, rows } = size;
  const whole = windowOf(size);
  if (cols < COMPACT) {
    const tall = Math.min(whole[3], freeArea('all', view)[3]);
    const top: Box = [0, 0, whole[2], tall];
    return { whole: top, dialog: top, left: top, work: top, panel: top, prompt: top, menu: [0, whole[3] - tall, whole[2], tall] };
  }
  const d = issuesArea(cols, rows);
  const narrow = view.width < WIDE;
  const reach = narrow ? 70 : 108;
  const panel = changesWidth(cols);
  return {
    whole,
    dialog: cells(size, d.x - 1, d.y - 1, d.w + 2, d.h + 2),
    left: cells(size, 0, 0, Math.min(cols, narrow ? 62 : 96), rows),
    work: cells(size, 0, 0, Math.min(cols, reach + 12), rows),
    panel: cells(size, Math.max(0, cols - panel - (narrow ? 10 : 46)), 0, Math.min(cols, panel + (narrow ? 10 : 46)), rows),
    prompt: cells(size, -2, -3, Math.min(cols, narrow ? 64 : 92), Math.min(rows, 26)),
    menu: cells(size, 0, 0, Math.min(cols, reach + 12), rows),
  };
}

export function cameraKeys(at: Place, size: Size, view: View): Key[] {
  const f = frames(size, view);
  const wide = view.width >= WIDE;
  const side = (free: 'left' | 'right') => (wide ? free : 'all');
  const key = (id: string, progress: number, shot: Shot): Key => ({ at: at(id, progress), shot });
  const mark = (name: Mark, plus: number, shot: Shot): Key => {
    const where = whereIs(TIMES.marks[name] + plus);
    return key(where.chapter, where.progress, shot);
  };
  const peek: Shot = wide ? { focus: f.whole, free: 'all', rx: 38, dy: 0.66, zoom: 1.04 } : { focus: f.whole, free: 'all', rx: 30, dy: 0.58, zoom: 1.0 };
  const front: Shot = { focus: f.whole, free: 'all' };
  const back: Shot = { focus: f.whole, free: 'all', zoom: 0.84, rx: 14, dy: 0.02, fade: 0.26 };
  const away: Shot = { focus: f.whole, free: 'all', zoom: 1.5, rx: 52, dy: 1.3, fade: 0 };
  const iso: Shot = wide ? { focus: f.whole, free: 'right', rx: 55, rz: -34, zoom: 0.6, dy: 0.03, dx: -0.03 } : { focus: f.whole, free: 'all', rx: 56, rz: -33, zoom: size.cols < COMPACT ? 0.66 : 0.9, dy: 0.02 };
  const drift: Shot = wide ? { ...iso, rx: 50, rz: -25, zoom: 0.64 } : { ...iso, rx: 51, rz: -24 };
  return [
    key('hero', 0, peek),
    key('hero', 1, front),
    key('features', 0.07, { focus: f.whole, free: side('right'), zoom: 0.96 }),
    key('features', 0.165, iso),
    key('features', 0.86, drift),
    key('features', 0.975, front),
    key('why', 0.3, back),
    key('why', 0.82, back),
    key('start', 0.07, { focus: f.menu, free: side('right'), ry: -5 }),
    mark('issues', 600, { focus: f.menu, free: side('right'), ry: -4 }),
    mark('picked', -300, { focus: f.dialog, free: side('right'), ry: -3 }),
    mark('started', -400, { focus: f.dialog, free: side('right'), ry: -2 }),
    mark('started', 900, { focus: f.work, free: side('right'), ry: -3 }),
    key('start', 0.94, { focus: f.work, free: side('right'), ry: -1 }),
    key('glance', 0.07, { focus: f.left, free: side('left'), ry: 6, rx: 2 }),
    key('glance', 0.94, { focus: f.left, free: side('left'), ry: -2, rx: -1 }),
    key('review', 0.07, { focus: f.work, free: side('right'), ry: -5 }),
    mark('review', -500, { focus: f.work, free: side('right'), ry: -4 }),
    mark('review', 900, { focus: f.panel, free: side('right'), ry: -3 }),
    key('review', 0.94, { focus: f.panel, free: side('right'), ry: 1 }),
    key('leave', 0.07, { focus: f.menu, free: side('right'), ry: -4 }),
    mark('quit', -200, { focus: f.menu, free: side('right'), ry: -3 }),
    mark('away', 0, { focus: f.prompt, free: side('right'), dy: -0.2, ry: -4 }),
    mark('typing', 600, { focus: f.prompt, free: side('right'), dy: -0.2, ry: -1 }),
    mark('back', 500, { focus: f.work, free: side('right'), ry: -2 }),
    key('leave', 0.92, { focus: f.work, free: side('right'), ry: 1 }),
    key('leave', 1, away),
  ];
}
