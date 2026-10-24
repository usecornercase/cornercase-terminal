import assert from 'node:assert/strict';
import { test } from 'node:test';

import { App } from '../src/lib/demo/app';
import type { Key } from '../src/lib/demo/programs';
import { INVERSE } from '../src/lib/term/grid';
import { menuArea } from '../src/lib/demo/layout';
import { type Editable, editKey } from '../src/lib/demo/todo';
import { world } from '../src/lib/demo/world';

function demo(columns: number): App {
  const app = new App();
  app.virtual = true;
  app.scripted = true;
  app.resize(columns, 34);
  world(app, { scripted: true });
  app.todo.items = [];
  return app;
}

function shown(app: App, needle: string): boolean {
  app.render();
  return app.textAt(needle) !== null;
}

for (const [columns, dot, plain] of [
  [140, ' TODO ● ', ' TODO   '],
  [80, '☐ ●', '☐  '],
] as const) {
  test(`the TODO button shows a dot while items are pending at ${columns} columns`, () => {
    const app = demo(columns);
    const seen = [shown(app, dot)];
    app.todo.add('a');
    seen.push(shown(app, dot));
    const id = app.todo.items[0].id;
    app.toggleTodoItem(id);
    seen.push(shown(app, dot), shown(app, plain));
    app.toggleTodoItem(id);
    seen.push(shown(app, dot));
    app.toggleTodoItem(id);
    app.clearDoneTodos();
    seen.push(shown(app, dot));
    assert.deepEqual(seen, [false, true, false, true, true, false]);
    app.destroy();
  });

  test(`the TODO dot can be switched off at ${columns} columns`, () => {
    const app = demo(columns);
    app.todo.add('a');
    app.config.todoDot = false;
    assert.equal(shown(app, dot), false);
    app.destroy();
  });
}

test('the send button of a pending TODO picks an agent and sends the text', () => {
  const app = demo(140);
  app.todo.add('fix the empty address');
  app.toggleTodo();
  app.todo.open = true;
  app.render();
  assert.equal(app.textAt('▸'), null);
  const item = app.textAt('fix the empty address');
  assert.ok(item);
  app.pointerMove(item.x, item.y, 0);
  app.render();
  const at = app.textAt('▸');
  assert.ok(at);
  app.pointerDown(at.x, at.y, 0);
  app.pointerUp();
  assert.equal(app.overlay?.kind, 'menu');
  const labels = app.overlay?.kind === 'menu' ? app.overlay.actions.map((a) => app.menuLabel(a)) : [];
  assert.ok(labels.length > 0);
  assert.match(labels[0], /^\S .+ · \w+/);
  app.chooseMenu(0);
  assert.equal(app.todo.items[0].done, false);
  app.advance(400);
  assert.match(app.toast?.text ?? '', /^sent to \w+ in /);
  assert.equal(app.todo.items[0].done, true);
  app.destroy();
});

test('the send button is always shown in compact mode and not on done items', () => {
  const app = demo(80);
  app.todo.add('a');
  app.todo.add('b');
  app.toggleTodoItem(app.todo.items[1].id);
  app.toggleTodo();
  app.todo.open = true;
  app.render();
  assert.ok(app.textAt('▸'));
  assert.equal(app.textAt('▸', 1), null);
  app.destroy();
});

for (const [name, back, forward] of [
  ['ctrl and the arrows', { key: 'ArrowLeft', ctrl: true }, { key: 'ArrowRight', ctrl: true }],
  ['option and the arrows', { key: 'ArrowLeft', alt: true }, { key: 'ArrowRight', alt: true }],
  ['option b and f', { key: 'b', alt: true }, { key: 'f', alt: true }],
] as const) {
  test(`${name} move a field by word, as in the app`, () => {
    const f = { chars: [...'fix the login-bug'], cursor: 0 };
    const walk = (key: Key): number[] =>
      [0, 1, 2, 3, 4].map(() => {
        editKey(f, key, 40);
        return f.cursor;
      });
    assert.deepEqual(walk(forward), [4, 8, 13, 14, 17]);
    assert.deepEqual(walk(back), [14, 13, 8, 4, 0]);
  });
}

function todoField(): { app: App; x: number; y: number } {
  const app = demo(140);
  app.toggleTodo();
  app.todo.open = true;
  app.addTodo();
  app.todo.type('fix the login');
  app.render();
  const at = app.textAt('fix the login');
  assert.ok(at);
  return { app, x: at.x, y: at.y };
}

const text = (app: App): string => app.todo.field?.chars.join('') ?? '';

test('dragging over a TODO field selects, reverses the cells and typing replaces it', () => {
  const { app, x, y } = todoField();
  app.pointerDown(x + 4, y, 0);
  app.pointerMove(x + 7, y, 1);
  app.pointerUp();
  const { grid } = app.render();
  const reversed = [3, 4, 6, 7].map((dx) => !!((grid.at(x + dx, y)?.attr ?? 0) & INVERSE));
  assert.deepEqual(reversed, [false, true, true, false]);
  app.key({ key: 'X' } as Key);
  assert.equal(text(app), 'fix X login');
  app.destroy();
});

test('a double click on a TODO field selects a word', () => {
  const { app, x, y } = todoField();
  for (let n = 0; n < 2; n++) {
    app.pointerDown(x + 5, y, 0);
    app.pointerUp();
  }
  app.key({ key: 'X' } as Key);
  assert.equal(text(app), 'fix X login');
  app.destroy();
});

test('shift with the arrows selects and a plain arrow collapses to its edge', () => {
  const f: Editable = { chars: [...'fix the login'], cursor: 13 };
  editKey(f, { key: 'ArrowLeft', shift: true, ctrl: true } as Key, 40);
  assert.deepEqual([f.anchor, f.cursor], [13, 8]);
  editKey(f, { key: 'ArrowLeft' } as Key, 40);
  assert.deepEqual([f.anchor, f.cursor], [null, 8]);
  editKey(f, { key: 'End', shift: true } as Key, 40);
  editKey(f, { key: 'Backspace' } as Key, 40);
  assert.equal(f.chars.join(''), 'fix the ');
});

test('the menu of a TODO field copies, cuts and selects all', () => {
  const { app, x, y } = todoField();
  const heard: [string, string | undefined][] = [];
  app.on((event, detail) => heard.push([event, detail]));
  app.pointerDown(x + 4, y, 0);
  app.pointerMove(x + 7, y, 1);
  app.pointerUp();
  app.pointerDown(x + 5, y, 2);
  assert.ok(app.fieldMenu);
  app.render();
  const r = menuArea(app.cols, app.rows, { x: x + 5, y }, ['copy', 'cut', 'select all']);
  app.pointerDown(r.x + 2, r.y + 2, 0);
  assert.deepEqual(heard.filter(([event]) => event === 'copy'), [['copy', 'the']]);
  assert.equal(text(app), 'fix  login');
  app.pointerDown(x + 1, y, 2);
  app.pointerDown(r.x + 2, r.y + 3, 0);
  app.key({ key: 'Y' } as Key);
  assert.equal(text(app), 'Y');
  app.destroy();
});
