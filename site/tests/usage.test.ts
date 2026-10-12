import assert from 'node:assert/strict';
import { test } from 'node:test';
import { App } from '../src/lib/demo/app';
import { world } from '../src/lib/demo/world';
import { usageArea } from '../src/lib/demo/layout';

for (const columns of [110, 46]) test(`Usage opens on Stats and keeps its border at ${columns} columns`, () => {
  const app = new App(); app.virtual = true; app.resize(columns, 38); world(app, { scripted: true });
  app.openUsage(); app.render();
  assert.equal(app.usage.page, 1); assert.equal(app.usage.loading, false);
  assert.ok(app.textAt('agents started')); assert.ok(app.textAt('tokens from cache')); assert.ok(app.textAt('share'));
  const height = columns === 110 ? 21 : 23;
  const stats = usageArea(columns, 38, height);
  const label = app.textAt('agents started'); assert.ok(label);
  assert.equal(label.x, stats.x + 2);
  app.usagePage(0); app.render(); assert.equal(app.usage.loading, true);
  assert.deepEqual(usageArea(columns, 38, height), stats);
  app.advance(1500); app.render(); assert.ok(app.textAt('resets in'));
  app.closeOverlay(); app.openUsage(); assert.equal(app.usage.page, 0);
  app.usagePage(1); app.render(); assert.ok(app.textAt('prompts'));
  app.destroy();
});
