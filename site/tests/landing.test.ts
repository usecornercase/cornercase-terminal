import assert from 'node:assert/strict';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { test } from 'node:test';

import * as en from '../src/i18n/en';

const dir = 'src/components/landing';

test('the landing sections come in the planned order', () => {
  const home = readFileSync(`${dir}/Home.astro`, 'utf8');
  const order = ['<Hero />', '<Features />', '<Why />', '<Flows />', '<More />', '<Stats />'].map((s) => home.indexOf(s));
  assert.ok(order.every((i) => i >= 0));
  assert.deepEqual(order, [...order].sort((a, b) => a - b));
});

test('More does not repeat flows and lists one link per item', () => {
  const more = readFileSync(`${dir}/More.astro`, 'utf8');
  assert.ok(!more.includes('guides/flows/'));
  const links = more.split('\n  [\n')[1].split('\n  ],')[0].match(/^    '/gm)?.length;
  assert.equal(links, en.page.more.groups[0].items.length);
});

test('every feature tile has one link', () => {
  const text = readFileSync(`${dir}/Features.astro`, 'utf8');
  const links = text.split('const links = [\n')[1].split('\n];')[0].match(/^ {2}'/gm)?.length;
  assert.equal(links, en.page.features.items.length);
});

test('every docs link on the landing leads to a page', () => {
  const paths = new Set<string>();
  for (const file of readdirSync(dir).filter((f) => f.endsWith('.astro'))) {
    const text = readFileSync(`${dir}/${file}`, 'utf8');
    const found = [
      ...[...text.matchAll(/\/docs\/([a-z0-9#/-]*)/g)].map((m) => m[1]),
      ...[...text.matchAll(/docs\('([^']*)'\)/g)].map((m) => m[1]),
      ...[...text.matchAll(/'((?:guides|reference|help|installation|first-steps)\/[^']*|#[a-z]+(?:-[a-z0-9]+)+)'/g)].map((m) => m[1]),
    ];
    for (const p of found) if (!p.startsWith('#')) paths.add(p.replace(/#.*$/, '').replace(/\/$/, ''));
  }
  assert.ok(paths.size > 20, `only ${paths.size} paths found`);
  for (const p of paths) {
    const base = `src/content/docs/docs/${p}`;
    const ok = p === '' ? existsSync('src/content/docs/docs/index.mdx') : existsSync(`${base}.mdx`) || existsSync(`${base}/index.mdx`);
    assert.ok(ok, `no docs page for ${p}`);
  }
});
