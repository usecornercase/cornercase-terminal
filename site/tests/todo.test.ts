import assert from 'node:assert/strict';
import { test } from 'node:test';

import { App } from '../src/lib/demo/app';
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
  [140, ' TODO ● ', '   TODO '],
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
