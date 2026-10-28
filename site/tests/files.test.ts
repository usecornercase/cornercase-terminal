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
  assert.ok(viewer.top.line > 0 || viewer.top.row > 0);
  assert.deepEqual(viewer.selection, [1, lines.length]);
  app.pointerUp();
  app.fileAction('copy');
  assert.equal(copied.at(-1), lines.join('\n'));
  app.destroy();
});

test('dragging over the text selects characters and copy takes exactly them, and the numbers still select lines', () => {
  const app = demo(140);
  app.showFile(PATH);
  app.render();
  const copied: string[] = [];
  app.on((event, detail) => {
    if (event === 'copy') copied.push(detail ?? '');
  });
  const viewer = app.files.place(app.workspace()?.id ?? -1).viewer;
  assert.ok(viewer);
  const at = app.textAt('use serde::Deserialize');
  assert.ok(at);
  app.pointerDown(at.x + 4, at.y, 0);
  app.pointerUp();
  assert.deepEqual([viewer.selection, viewer.stretch], [[1, 1], null]);
  app.fileAction('copy');
  assert.equal(copied.at(-1), 'use serde::Deserialize;');

  app.pointerDown(at.x + 4, at.y, 0);
  app.pointerMove(at.x + 8, at.y, 1);
  app.pointerUp();
  assert.ok(viewer.stretch);
  app.fileAction('copy');
  assert.equal(copied.at(-1), 'serde');
  app.render();
  assert.ok(app.textAt('line 1'));

  app.pointerDown(at.x + 4, at.y, 0);
  app.pointerMove(at.x + 2, at.y + 2, 1);
  app.pointerUp();
  app.fileAction('copy');
  assert.equal(copied.at(-1), 'serde::Deserialize;\n\n#[d');

  app.pointerDown(at.x - 4, at.y, 0);
  app.pointerMove(at.x - 4, at.y + 2, 1);
  app.pointerUp();
  assert.deepEqual([viewer.selection, viewer.stretch], [[1, 3], null]);
  app.fileAction('copy');
  assert.equal(copied.at(-1), 'use serde::Deserialize;\n\n#[derive(Debug, Deserialize)]');
  app.destroy();
});

test('ending a character selection in the numbers of a line leaves that line out of the lines it covers', () => {
  const app = demo(140);
  app.showFile(PATH);
  app.render();
  const viewer = app.files.place(app.workspace()?.id ?? -1).viewer;
  assert.ok(viewer);
  const at = app.textAt('use serde::Deserialize');
  assert.ok(at);
  app.pointerDown(at.x + 4, at.y, 0);
  app.pointerMove(at.x - 4, at.y + 2, 1);
  app.pointerUp();
  app.render();
  assert.ok(app.textAt('lines 1-2'));
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
  assert.deepEqual(viewer.top, { line: 3, row: 0 });
  assert.deepEqual(viewer.selection, [1, 4]);
  app.pointerUp();
  app.destroy();
});

test('the viewer offers open in… when it has room for it, and a click says it needs a real cornercase', () => {
  const app = demo(140);
  click(app, '▢');
  assert.ok(app.textAt('open in…'));
  click(app, 'open in…');
  assert.ok(app.textAt('the demo opens no other apps'));
  app.destroy();
});

test('a narrow viewer leaves open in… out so the line info keeps its room', () => {
  const app = demo(100);
  assert.equal(app.textAt('open in…'), null);
  assert.ok(app.textAt('ask agent'));
  app.destroy();
});

function viewer(app: App) {
  const found = app.files.place(app.workspace()?.id ?? -1).viewer;
  assert.ok(found);
  return found;
}

function key(app: App, name: string): void {
  app.key({ key: name } as never);
  app.render();
}

test('the find field lights the matches, steps through them and closes with Esc', () => {
  const app = demo(140);
  click(app, '⌕  ▢');
  assert.ok(app.textAt('text in this file'));
  for (const c of 'line') app.key({ key: c } as never);
  app.render();
  assert.ok(app.textAt('1/3'));
  key(app, 'ArrowDown');
  assert.ok(app.textAt('2/3'));
  click(app, '↑');
  assert.ok(app.textAt('1/3'));
  assert.deepEqual(viewer(app).selection, [11, 12]);
  key(app, 'Escape');
  assert.ok(app.textAt('text in this file'));
  assert.equal(viewer(app).find?.query, '');
  key(app, 'Escape');
  assert.equal(viewer(app).find, null);
  assert.equal(app.filesShown(), true);
  key(app, 'Escape');
  assert.equal(app.filesShown(), false);
  app.destroy();
});

test('typing in the find field says when nothing matches and wraps at the ends', () => {
  const app = demo(140);
  click(app, '⌕  ▢');
  for (const c of 'zzz') app.key({ key: c } as never);
  app.render();
  assert.ok(app.textAt('no matches'));
  for (let i = 0; i < 3; i++) key(app, 'Backspace');
  for (const c of 'line') app.key({ key: c } as never);
  key(app, 'ArrowUp');
  assert.ok(app.textAt('3/3'));
  key(app, 'Enter');
  assert.ok(app.textAt('1/3'));
  app.destroy();
});

test('a press outside the panel gives the find keys back and keeps the query', () => {
  const app = demo(140);
  click(app, '⌕  ▢');
  for (const c of 'fn') app.key({ key: c } as never);
  assert.equal(app.filesTyping(), true);
  const pane = app.areas().pane;
  app.pointerDown(pane.x + 2, pane.y + 2, 0);
  app.pointerUp();
  assert.equal(app.filesTyping(), false);
  assert.equal(viewer(app).find?.query, 'fn');
  app.destroy();
});

test('a text search result opens the file with the field holding the query', () => {
  const app = new App();
  app.virtual = true;
  app.scripted = true;
  app.resize(140, 34);
  world(app, { scripted: true });
  app.files.open = true;
  app.render();
  click(app, 'text in the files');
  for (const c of 'first_line') app.key({ key: c } as never);
  app.render();
  key(app, 'ArrowDown');
  key(app, 'Enter');
  const opened = viewer(app);
  assert.equal(opened.find?.query, 'first_line');
  assert.equal(opened.find?.focused, true);
  assert.deepEqual(opened.selection && opened.selection[0] === opened.selection[1], true);
  assert.ok(app.textAt('1/1'));
  click(app, '‹');
  assert.equal(app.files.place(app.workspace()?.id ?? -1).viewer, null);
  app.destroy();
});

test('a long line wraps under its number and a press on its second row selects it', () => {
  const app = demo(140);
  const first = app.textAt('pub fn first_line(&self) -> ');
  const second = app.textAt('&str {');
  assert.ok(first && second);
  assert.equal(second.y, first.y + 1);
  const on = (needle: string, y: number) => [0, 1, 2, 3, 4, 5].some((n) => app.textAt(needle, n)?.y === y);
  assert.ok(on('11', first.y) && !on('11', second.y));
  app.pointerDown(second.x, second.y, 0);
  app.pointerUp();
  const viewer = app.files.place(app.workspace()?.id ?? -1).viewer;
  assert.deepEqual(viewer?.selection, [11, 11]);
  app.destroy();
});

test('finding a match on a later row shows that row', () => {
  const app = demo(140);
  app.resize(140, 10);
  app.render();
  app.openFind();
  app.editFind('unwrap');
  app.render();
  assert.ok(app.textAt('unwrap'));
  const viewer = app.files.place(app.workspace()?.id ?? -1).viewer;
  assert.ok(viewer);
  app.destroy();
});
