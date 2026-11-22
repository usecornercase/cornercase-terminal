import type { QueueEntry } from './queue';

export interface QueueListDialog {
  kind: 'queuelist';
  filter: string;
  selected: number | null;
  scroll: number;
}

export interface QueueShown {
  entry: QueueEntry;
  run: boolean;
  fold: boolean | null;
  hidden: number;
}

export const openQueueList = (selected: number | null = null): QueueListDialog => ({ kind: 'queuelist', filter: '', selected, scroll: 0 });

const band = (entry: QueueEntry): number => (entry.repeat ? (entry.paused ? 1 : 0) : entry.state === 'pending' ? 0 : 2);
const newest = (a: QueueEntry, b: QueueEntry): number => b.at - a.at || b.id - a.id;
const inOrder = (a: QueueEntry, b: QueueEntry): number => band(a) - band(b) || (band(a) === 2 ? newest(a, b) : a.at - b.at || a.id - b.id);

export function arranged(entries: QueueEntry[]): QueueEntry[][] {
  const routines = new Set(entries.filter((entry) => entry.repeat).map((entry) => entry.id));
  const under = (entry: QueueEntry) => entry.routine !== undefined && routines.has(entry.routine);
  const top = entries.filter((entry) => !under(entry)).sort(inOrder);
  return top.map((entry) => (entry.repeat ? [entry, ...entries.filter((run) => run.routine === entry.id).sort(newest)] : [entry]));
}

const row = (entry: QueueEntry, run: boolean, fold: boolean | null = null, hidden = 0): QueueShown => ({ entry, run, fold, hidden });

export function queueShown(entries: QueueEntry[], folded: Set<number>, query: string, matches: (entry: QueueEntry) => boolean): QueueShown[] {
  const groups = arranged(entries);
  if (query) {
    return groups.flatMap(([head, ...runs]) => {
      const hits = runs.filter(matches);
      return hits.length || matches(head) ? [row(head, false), ...hits.map((run) => row(run, true))] : [];
    });
  }
  return groups.flatMap(([head, ...runs]) => {
    const closed = folded.has(head.id);
    const fold = runs.length ? closed : null;
    return [row(head, false, fold, closed ? runs.length : 0), ...(closed ? [] : runs.map((run) => row(run, true)))];
  });
}

export function queueIndex(o: QueueListDialog, shown: QueueShown[]): number | null {
  const at = shown.findIndex((item) => item.entry.id === o.selected);
  return at >= 0 ? at : shown.length ? 0 : null;
}

export function moveQueueList(o: QueueListDialog, shown: QueueShown[], delta: number, rows: number): void {
  if (!shown.length) return;
  const next = Math.max(0, Math.min(shown.length - 1, (queueIndex(o, shown) ?? 0) + delta));
  o.selected = shown[next].entry.id;
  if (next < o.scroll) o.scroll = next;
  else if (next >= o.scroll + rows) o.scroll = next + 1 - rows;
}

export function foldAsked(o: QueueListDialog, shown: QueueShown[], fold: boolean): number | null {
  const at = queueIndex(o, shown);
  if (o.filter || at === null) return null;
  const item = shown[at];
  const routine = item.run && fold ? item.entry.routine ?? null : item.fold !== null && item.fold !== fold ? item.entry.id : null;
  if (routine !== null) o.selected = routine;
  return routine;
}

export function queueButtons(selected: QueueEntry | undefined, clearable: boolean, width: number): string[] {
  const runnable = !!selected && (selected.repeat ? true : !selected.routine && selected.state !== 'started' && selected.state !== 'answered');
  const buttons = [
    'new',
    ...(runnable ? ['start now'] : []),
    ...(selected?.repeat ? [selected.paused ? 'resume' : 'pause'] : []),
    ...(selected ? ['remove'] : []),
    ...(clearable ? ['clear started'] : []),
    'done',
  ];
  const needed = buttons.reduce((sum, label) => sum + [...label].length + 3, 0);
  return needed > width + 1 ? buttons.filter((label) => label !== 'clear started') : buttons;
}
