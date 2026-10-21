import assert from 'node:assert/strict';
import { test } from 'node:test';

import { App } from '../src/lib/demo/app';
import { CHAPTERS } from '../src/lib/demo/boot';
import type { Mounted } from '../src/lib/demo/client';
import { activePane } from '../src/lib/demo/model';
import { Agent, Editor } from '../src/lib/demo/programs';
import { type Step, play } from '../src/lib/demo/tour';
import { world } from '../src/lib/demo/world';

function demo(columns: number): App {
  const app = new App();
  app.virtual = true;
  app.scripted = true;
  app.resize(columns, 34);
  world(app, { scripted: true });
  app.changesOpen = true;
  return app;
}

function fileRow(app: App): { x: number; y: number } {
  app.render();
  const at = app.textAt('▸ M') ?? app.textAt('▾ M');
  assert.ok(at);
  return at;
}

function onRow(app: App, needle: string, y: number): number {
  for (let nth = 0; ; nth++) {
    const at = app.textAt(needle, nth);
    if (!at) return -1;
    if (at.y === y) return at.x;
  }
}

function hover(app: App): { icon: number; check: number; y: number } {
  const row = fileRow(app);
  app.pointerMove(row.x + 6, row.y, 0);
  app.render();
  return { icon: onRow(app, '↺', row.y), check: onRow(app, '✓', row.y), y: row.y };
}

function slot(app: App, glyph: string): { x: number; y: number } {
  const row = fileRow(app);
  app.pointerMove(row.x + 6, row.y, 0);
  app.render();
  const x = onRow(app, `${glyph}  ✓`, row.y);
  assert.ok(x >= 0);
  return { x, y: row.y };
}

function press(app: App, x: number, y: number): void {
  app.pointerDown(x, y, 0);
  app.pointerUp();
  app.render();
}

for (const columns of [140, 80]) {
  test(`hovering a file shows the discard icon left of its check at ${columns} columns`, () => {
    const app = demo(columns);
    const { icon, check } = hover(app);
    assert.ok(icon >= 0 && icon < check);
    app.destroy();
  });

  test(`the discard icon asks, cancel keeps the file and discard drops it at ${columns} columns`, () => {
    const app = demo(columns);
    const files = app.changesDiff().length;
    const { icon, y } = hover(app);
    press(app, icon, y);
    assert.equal(app.confirmView()?.title, 'Discard changes');
    assert.equal(app.confirmView()?.dangerous, true);
    assert.match(app.confirmView()?.message ?? '', /cannot be undone/);
    const cancel = app.textAt(' cancel ');
    assert.ok(cancel);
    press(app, cancel.x + 1, cancel.y);
    assert.equal(app.confirmView(), null);
    assert.equal(app.changesDiff().length, files);
    press(app, hover(app).icon, y);
    const discard = app.textAt(' discard ');
    assert.ok(discard);
    press(app, discard.x + 1, discard.y);
    assert.equal(app.changesDiff().length, files - 1);
    assert.match(app.toast?.text ?? '', /^(discarded the changes to|deleted) /);
    app.destroy();
  });

  test(`a hovered row reads ↺, +, ✓ in that order at ${columns} columns`, () => {
    const app = demo(columns);
    const { icon, check, y } = hover(app);
    assert.ok(onRow(app, '↺  +  ✓', y) >= 0);
    assert.equal(check - icon, 6);
    assert.equal(slot(app, '+').x, check - 3);
    app.destroy();
  });

  test(`the + stages with no dialog, the row shows ● and a second click unstages at ${columns} columns`, () => {
    const app = demo(columns);
    const file = app.changesDiff()[0];
    const { x, y } = slot(app, '+');
    press(app, x, y);
    assert.equal(app.confirmView(), null);
    assert.equal(app.toast, null);
    assert.equal(app.changesStaged(file), true);
    app.pointerMove(0, 0, 0);
    app.render();
    assert.ok(onRow(app, '●', y) >= 0);
    app.pointerMove(x, y, 0);
    app.render();
    assert.ok(onRow(app, '−  ✓', y) >= 0);
    press(app, x, y);
    assert.equal(app.changesStaged(file), false);
    app.destroy();
  });

  test(`Commits shows neither the discard icon nor the stage slot at ${columns} columns`, () => {
    const app = demo(columns);
    app.setChangesMode('commits');
    const { icon, check, y } = hover(app);
    assert.equal(icon, -1);
    assert.equal(onRow(app, '+  ✓', y), -1);
    assert.ok(check >= 0);
    app.destroy();
  });

  test(`Commits has no discard icon and a click there folds the file at ${columns} columns`, () => {
    const app = demo(columns);
    app.setChangesMode('commits');
    const { icon, check, y } = hover(app);
    assert.equal(icon, -1);
    const folded = app.changesFolded(app.changesDiff()[0]);
    press(app, check - 2, y);
    assert.equal(app.confirmView(), null);
    assert.notEqual(app.changesFolded(app.changesDiff()[0]), folded);
    app.destroy();
  });

  test(`the last cell of Every project selects its scope at ${columns} columns`, () => {
    const app = demo(columns);
    app.render();
    const label = ' Every project ';
    const position = app.textAt(label);
    assert.ok(position);
    app.pointerDown(position.x + label.length - 1, position.y, 0);
    app.pointerUp();
    assert.equal(app.changesScope, 'every');
    app.destroy();
  });

  test(`files sit beyond their workspace headers at ${columns} columns`, () => {
    const app = demo(columns);
    app.projects[0].workspaces[1].flags.fixed = true;
    app.setChangesScope('every');
    app.render();
    const header = app.textAt('▾ web-shop');
    const workspace = app.textAt('▾ feat/dark-mode');
    const file = app.textAt('▸ M');
    assert.ok(header && workspace && file);
    assert.ok(header.x < workspace.x && workspace.x <= file.x);
    app.destroy();
  });

  for (const action of ['open', 'ask agent'] as const) {
    test(`${action} reveals the file's project and workspace at ${columns} columns`, () => {
      const app = demo(columns);
      app.config.sidebar = 'tree';
      app.setChangesScope('every');
      app.render();
      const file = app.changesDiff().find((f) => f.owner?.project.folder === 'orders-api');
      assert.ok(file?.owner);
      const { project, workspace } = file.owner;
      project.collapsed = true;
      workspace.collapsed = true;
      const group = app.groups.find((g) => g.id === project.group);
      assert.ok(group);
      group.collapsed = true;
      app.nav = 'projects';
      app.hunkAction(action, file, 0);
      assert.equal(app.project(), project);
      assert.equal(app.workspace(), workspace);
      assert.equal(app.nav, null);
      assert.ok(!project.collapsed && !workspace.collapsed);
      if (columns >= 90) assert.ok(!group.collapsed);
      assert.ok(app.changesOpen && app.changesScope === 'every');
      const tab = app.tab();
      assert.ok(tab);
      const program = activePane(tab)?.shell.fg;
      assert.ok(action === 'open' ? program instanceof Editor : program instanceof Agent);
      app.destroy();
    });
  }

  for (const panel of ['changes', 'files', 'todo'] as const) {
    test(`Escape closes the ${panel} panel before it reaches the pane at ${columns} columns`, () => {
      const app = demo(columns);
      app.changesOpen = false;
      if (panel === 'changes') app.toggleChanges();
      else if (panel === 'files') app.toggleFiles();
      else app.toggleTodo();
      const tab = app.tab();
      const shell = tab && activePane(tab)?.shell;
      assert.ok(shell);
      const reached: string[] = [];
      const key = shell.key.bind(shell);
      shell.key = (k) => {
        reached.push(k.key);
        key(k);
      };
      if (panel === 'todo') {
        app.todo.field = { item: null, chars: ['x'], cursor: 1 };
        app.key({ key: 'Escape' });
        assert.ok(app.panelShown() && !app.todo.field);
      }
      app.key({ key: 'Escape' });
      assert.ok(!app.panelShown());
      assert.deepEqual(reached, []);
      app.key({ key: 'Escape' });
      assert.deepEqual(reached, ['Escape']);
      app.destroy();
    });
  }

  test(`the hero tour reaches its end at ${columns} columns`, async () => {
    const app = demo(columns);
    app.changesOpen = false;
    let finished = false;
    const steps: Step[] = CHAPTERS.flat().flatMap((step): Step[] => {
      if (!('click' in step) || step.orKey || step.alt) return [step];
      return [{ run: (app) => { app.render(); assert.ok(app.textAt(step.click, Math.max(0, step.nth ?? 0)), step.click); } }, step];
    });
    steps.push({ run: () => { finished = true; } });
    await play({ app } as Mounted, null, steps, { instant: true }).done;
    assert.ok(finished);
    assert.equal(app.changesOpen, columns >= 90);
    app.destroy();
  });
}
