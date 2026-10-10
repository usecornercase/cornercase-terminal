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

for (const columns of [140, 80]) {
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
    app.projects[0].workspaces[0].flags.fixed = true;
    app.setChangesScope('every');
    app.render();
    const header = app.textAt('▾ shop');
    const workspace = app.textAt('▾ main');
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
      const file = app.changesDiff().find((f) => f.owner?.project.folder === 'api');
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
    assert.ok(finished && app.changesOpen);
    app.destroy();
  });
}
