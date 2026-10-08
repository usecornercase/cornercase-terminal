import { type Rect, rect } from '../term/grid';
import type { Key } from './programs';
import type { PanePlace } from './split';

export type KeysGroup = 'find' | 'git' | 'workspace';

export type KeysAction =
  | { kind: 'nextTab' }
  | { kind: 'previousTab' }
  | { kind: 'tab'; t: number }
  | { kind: 'nextWorkspace' }
  | { kind: 'previousWorkspace' }
  | { kind: 'nextProject' }
  | { kind: 'previousProject' }
  | { kind: 'pane'; side: PanePlace }
  | { kind: 'agent' }
  | { kind: 'search' }
  | { kind: 'newTab' }
  | { kind: 'split'; dir: 'right' | 'down' }
  | { kind: 'closePane' }
  | { kind: 'renameTab' }
  | { kind: 'findNames' }
  | { kind: 'findText' }
  | { kind: 'files' }
  | { kind: 'changes' }
  | { kind: 'base' }
  | { kind: 'newWorkspace' }
  | { kind: 'renameWorkspace' }
  | { kind: 'closeWorkspace' }
  | { kind: 'issues' }
  | { kind: 'todo' }
  | { kind: 'settings' }
  | { kind: 'usage' }
  | { kind: 'quit' };

export type KeysStep = { run: KeysAction } | { open: KeysGroup };

export interface KeysItem {
  key: string;
  label: string;
  step: KeysStep | null;
}

const run = (key: string, label: string, action: KeysAction): KeysItem => ({ key, label, step: { run: action } });
const open = (key: string, group: KeysGroup): KeysItem => ({ key, label: `${group}…`, step: { open: group } });

const ROOT: KeysItem[] = [
  run('n', 'next tab', { kind: 'nextTab' }),
  run('p', 'previous tab', { kind: 'previousTab' }),
  { key: '1-9', label: 'go to tab', step: null },
  run(']', 'next workspace', { kind: 'nextWorkspace' }),
  run('[', 'previous workspace', { kind: 'previousWorkspace' }),
  run('}', 'next project', { kind: 'nextProject' }),
  run('{', 'previous project', { kind: 'previousProject' }),
  { key: '←↑↓→', label: 'pane on that side', step: null },
  run('a', 'agent that needs you', { kind: 'agent' }),
  run('/', 'search', { kind: 'search' }),
  run('c', 'new tab', { kind: 'newTab' }),
  run('|', 'split right', { kind: 'split', dir: 'right' }),
  run('-', 'split down', { kind: 'split', dir: 'down' }),
  run('x', 'close pane', { kind: 'closePane' }),
  run('r', 'rename tab', { kind: 'renameTab' }),
  open('f', 'find'),
  open('g', 'git'),
  open('w', 'workspace'),
  run('i', 'issues', { kind: 'issues' }),
  run('t', 'todo', { kind: 'todo' }),
  run(',', 'settings', { kind: 'settings' }),
  run('u', 'usage', { kind: 'usage' }),
  run('q', 'quit', { kind: 'quit' }),
];

const GROUPS: Record<KeysGroup, KeysItem[]> = {
  find: [
    run('f', 'files by name', { kind: 'findNames' }),
    run('w', 'text in files', { kind: 'findText' }),
    run('e', 'file tree', { kind: 'files' }),
    run('p', 'projects, workspaces, tabs', { kind: 'search' }),
  ],
  git: [run('d', 'changes', { kind: 'changes' }), run('b', 'compare against a branch', { kind: 'base' })],
  workspace: [
    run('n', 'new workspace', { kind: 'newWorkspace' }),
    run('r', 'rename workspace', { kind: 'renameWorkspace' }),
    run('x', 'close workspace', { kind: 'closeWorkspace' }),
  ],
};

export const keysItems = (group: KeysGroup | null): KeysItem[] => (group ? GROUPS[group] : ROOT);

const ARROWS: Record<string, PanePlace> = { ArrowLeft: 'left', ArrowRight: 'right', ArrowUp: 'above', ArrowDown: 'below' };

export function keysLookup(group: KeysGroup | null, k: Key): KeysStep | null {
  if (k.ctrl || k.alt) return null;
  if (!group && ARROWS[k.key]) return { run: { kind: 'pane', side: ARROWS[k.key] } };
  if (!group && /^[1-9]$/.test(k.key)) return { run: { kind: 'tab', t: Number(k.key) - 1 } };
  return keysItems(group).find((item) => item.key === k.key)?.step ?? null;
}

const REFUSED: Record<string, string> = { c: 'it stops programs', d: 'it ends the shell', z: 'it suspends programs' };

const CLASHES: Record<string, string> = {
  'ctrl+b': "tmux's prefix, so it never reaches a tmux inside a pane",
  'ctrl+a': "screen's prefix and the shell's start of line",
  'ctrl+space': 'switches the input source on macOS',
  'ctrl+r': "the shell's history search",
  'ctrl+l': 'clears the screen in shells',
  'ctrl+e': "the shell's end of line",
};

export function prefixOf(k: Key): string | { error: string } {
  const fn = /^F([1-9]|1[0-9]|2[0-4])$/.exec(k.key);
  let key = k.key === ' ' ? 'space' : k.key;
  let shift = !!k.shift;
  if ([...key].length === 1) {
    if (/[A-Z]/.test(key)) {
      key = key.toLowerCase();
      shift = true;
    } else if (!/[a-z]/.test(key)) shift = false;
  } else if (key !== 'space' && !fn) return { error: 'use ctrl or alt with a key, or a function key' };
  if (!fn && !k.ctrl && !k.alt) return { error: 'use ctrl or alt with a key, or a function key' };
  if (k.ctrl && !k.alt && !shift && REFUSED[key]) return { error: `ctrl+${key} cannot be the prefix: ${REFUSED[key]}` };
  const mods = [k.ctrl && 'ctrl', k.alt && 'alt', shift && 'shift'].filter(Boolean).join('+');
  return `${mods ? `${mods}+` : ''}${fn ? `f${fn[1]}` : key}`;
}

export const prefixClash = (prefix: string): string | null => CLASHES[prefix] ?? null;

export function isPrefix(prefix: string, k: Key): boolean {
  if (!prefix) return false;
  const pressed = prefixOf(k);
  return typeof pressed === 'string' && pressed === prefix;
}

const GAP = 2;
const KEY_GAP = 2;

const len = (s: string): number => [...s].length;

export const keyWidth = (items: KeysItem[]): number => Math.max(0, ...items.map((i) => len(i.key)));

const cellWidth = (items: KeysItem[]): number => 1 + keyWidth(items) + KEY_GAP + Math.max(0, ...items.map((i) => len(i.label))) + 1;

function columns(items: KeysItem[], room: number): number {
  const count = Math.max(1, items.length);
  const fit = Math.max(1, Math.min(count, Math.floor((room + GAP) / (cellWidth(items) + GAP))));
  return Math.ceil(count / Math.ceil(count / fit));
}

const rowsOf = (items: KeysItem[], cols: number): number => Math.ceil(items.length / Math.max(1, cols));

export function keysFrame(pane: Rect, screen: Rect, items: KeysItem[]): Rect {
  const rows = rowsOf(items, columns(items, Math.max(0, pane.w - 2)));
  return rows + 4 <= pane.h ? pane : screen;
}

export function keysArea(frame: Rect, items: KeysItem[], hint: string): Rect {
  const cols = columns(items, Math.max(0, frame.w - 2));
  const rows = rowsOf(items, cols);
  const cells = cols * cellWidth(items) + (cols - 1) * GAP;
  const w = Math.min(Math.max(cells, len(hint) + 2) + 2, frame.w);
  const h = Math.min(rows + 4, frame.h);
  return rect(frame.x + Math.floor((frame.w - w) / 2), frame.y + frame.h - h, w, h);
}

export function keysItem(menu: Rect, items: KeysItem[], i: number): Rect {
  const cols = columns(items, Math.max(0, menu.w - 2));
  const rows = Math.max(1, rowsOf(items, cols));
  const body = rect(menu.x + 1, menu.y + 1, Math.max(0, menu.w - 2), Math.max(0, menu.h - 4));
  const x = body.x + Math.floor(i / rows) * (cellWidth(items) + GAP);
  const r = rect(x, body.y + (i % rows), cellWidth(items), 1);
  const x0 = Math.max(r.x, body.x);
  const y0 = Math.max(r.y, body.y);
  const x1 = Math.min(r.x + r.w, body.x + body.w);
  const y1 = Math.min(r.y + r.h, body.y + body.h);
  return x1 > x0 && y1 > y0 ? rect(x0, y0, x1 - x0, y1 - y0) : rect(0, 0, 0, 0);
}
