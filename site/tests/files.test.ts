import assert from 'node:assert/strict';
import { test } from 'node:test';

import { App } from '../src/lib/demo/app';
import { ADDRESS_RS } from '../src/lib/demo/data';
import { world } from '../src/lib/demo/world';

const PATH = 'src/returns/address.rs';

function demo(columns: number): App {
  const app = new App();
  app.virtual = true;
  app.scripted = true;
  app.resize(columns, 34);
  world(app, { scripted: true });
  app.files.open = true;
  app.showFile(PATH, [11, 12]);
  app.render();
  return app;
}

function click(app: App, needle: string): void {
  const at = app.textAt(needle);
  assert.ok(at, needle);
  app.pointerDown(at.x, at.y, 0);
  app.pointerUp();
  app.render();
}

for (const columns of [140, 80]) {
  test(`the file takes the window and goes back to its panel at ${columns} columns`, () => {
    const app = demo(columns);
    const panel = app.areas().changes;
    const widths = JSON.stringify(app.widths);
    click(app, '▢');
    assert.deepEqual(app.areas().changes, { x: 0, y: 0, w: columns, h: 34 });
    assert.ok(app.textAt('❐'));
    assert.equal(app.textAt('Projects'), null);
    click(app, '❐');
    assert.deepEqual(app.areas().changes, panel);
    assert.equal(JSON.stringify(app.widths), widths);
    app.destroy();
  });

  test(`escape goes back to the panel first at ${columns} columns`, () => {
    const app = demo(columns);
    click(app, '▢');
    app.key({ key: 'Escape' } as never);
    assert.equal(app.fileFullScreen(), false);
    assert.equal(app.filesShown(), true);
    app.key({ key: 'Escape' } as never);
    assert.equal(app.filesShown(), false);
    app.destroy();
  });

  test(`going back to the tree and another tab end it at ${columns} columns`, () => {
    const app = demo(columns);
    click(app, '▢');
    app.closeFile();
    assert.equal(app.fileFullScreen(), false);
    app.showFile(PATH);
    app.toggleFullFile();
    assert.equal(app.fileFullScreen(), true);
    app.selectTab(0, 0, 0);
    assert.equal(app.fileFullScreen(), false);
    app.destroy();
  });

  test(`a click on a blank part of the full file reaches nothing under it at ${columns} columns`, () => {
    const app = demo(columns);
    click(app, '▢');
    const before = app.active;
    app.pointerDown(2, 30, 0);
    app.pointerUp();
    assert.equal(app.active, before);
    assert.equal(app.fileFullScreen(), true);
    app.destroy();
  });
}

test('a panel too narrow for both leaves the back button alone', () => {
  const app = demo(9);
  assert.equal(app.textAt('▢'), null);
  const back = app.textAt('‹');
  assert.ok(back);
  app.pointerDown(back.x, back.y, 0);
  app.pointerUp();
  app.render();
  assert.equal(app.textAt('‹'), null);
  app.destroy();
});

test('a selection held at the bottom scrolls the file, and copy without one takes the whole file', () => {
  const app = new App();
  app.virtual = true;
  app.scripted = true;
  app.resize(140, 12);
  world(app, { scripted: true });
  app.files.open = true;
  app.showFile(PATH);
  app.render();
  const copied: string[] = [];
  app.on((event, detail) => {
    if (event === 'copy') copied.push(detail ?? '');
  });
  const viewer = app.files.place(app.workspace()?.id ?? -1).viewer;
  assert.ok(viewer);
  app.fileAction('copy');
  assert.equal(copied.at(-1), ADDRESS_RS);

  const panel = app.areas().changes;
  const lines = ADDRESS_RS.trimEnd().split('\n');
  app.selectFileLine(1);
  app.pointerMove(panel.x + 2, panel.y + panel.h - 1, 1);
  app.advance(1000);
  assert.ok(viewer.scroll > 0);
  assert.deepEqual(viewer.selection, [1, lines.length]);
  app.pointerUp();
  app.fileAction('copy');
  assert.equal(copied.at(-1), lines.join('\n'));
  app.destroy();
});

test('the wheel scrolls a selection and moves its end, as the app does', () => {
  const app = new App();
  app.virtual = true;
  app.scripted = true;
  app.resize(140, 12);
  world(app, { scripted: true });
  app.files.open = true;
  app.showFile(PATH);
  app.render();
  const viewer = app.files.place(app.workspace()?.id ?? -1).viewer;
  assert.ok(viewer);
  const panel = app.areas().changes;
  const top = panel.y + 3;
  app.selectFileLine(1);
  assert.equal(app.wheel(panel.x + 2, top, 1), true);
  assert.equal(viewer.scroll, 3);
  assert.deepEqual(viewer.selection, [1, 4]);
  app.pointerUp();
  app.destroy();
});
