import assert from 'node:assert/strict';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { test } from 'node:test';

import * as en from '../src/i18n/en';
import { CHAPTERS } from '../src/lib/show/script';

const dir = 'src/components/landing';

test('the landing sections come in the planned order', () => {
  const home = readFileSync(`${dir}/Home.astro`, 'utf8');
  const order = ['<Stage />', '<Hero />', '<Features />', '<Why />', '<Play />', '<Flows />', '<More />', '<Stats />'].map((s) => home.indexOf(s));
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

test('every section the show reads is a section of the page', () => {
  const read = (name: string) => readFileSync(name, 'utf8');
  const show = readdirSync('src/lib/show').filter((f) => f.endsWith('.ts')).map((f) => read(`src/lib/show/${f}`)).join('\n');
  const asked = new Set([...show.matchAll(/(?:progress|raw|seen|at|key)\('([a-z]+)'/g)].map((m) => m[1]));
  for (const name of CHAPTERS) asked.add(name);
  const places = show.split('const PLACES')[1].split('];')[0];
  for (const m of places.matchAll(/\['[a-z]+', '([a-z]+)', '([a-z]+)'\]/g)) asked.add(m[1]).add(m[2]);
  const pages = readdirSync(dir).filter((f) => f.endsWith('.astro')).map((f) => read(`${dir}/${f}`)).join('\n');
  const sections = new Set([...pages.matchAll(/data-section="([a-z]+)"/g)].map((m) => m[1]));
  for (const m of read(`${dir}/Why.astro`).matchAll(/\{ id: '([a-z]+)'/g)) sections.add(m[1]);
  assert.ok(asked.size >= 10, `only ${[...asked]} found`);
  for (const id of asked) assert.ok(sections.has(id), `no section ${id}`);
  for (const anchor of ['features', 'why', 'demo', 'flows', 'more', 'speed', 'install']) {
    assert.ok(new RegExp(`id=\\{?["']?${anchor}["'}]`).test(pages), `no #${anchor} for the nav and the footer`);
  }
});

test('the show keeps the plain page under it', () => {
  const layout = readFileSync('src/layouts/Landing.astro', 'utf8');
  assert.ok(layout.includes("matchMedia('(prefers-reduced-motion: reduce)')") && layout.includes('WebGL2RenderingContext'));
  assert.ok(layout.includes("classList.remove('motion', 'intro')"), 'the page gives the show up when its script never runs');
  for (const file of ['Hero.astro', 'Features.astro', 'Why.astro', 'Flows.astro', 'Stage.astro']) {
    const text = readFileSync(`${dir}/${file}`, 'utf8');
    assert.ok(text.includes(':global(.motion)'), `${file} has no rules of its own for the show`);
  }
  const stage = readFileSync(`${dir}/Stage.astro`, 'utf8');
  assert.ok(/\.scene \{\s+display: none;/.test(stage), 'the stage shows without the show');
  assert.ok(stage.includes('aria-hidden="true"'));
});

test('the README picture and og.png take the hero text from the landing', () => {
  const brand = readFileSync('capture/brand.mjs', 'utf8').replace(/<br\s*\/?>/g, ' ');
  const { title, pitch, strip } = en.page.hero;
  const phrases = [...title, pitch, strip].flatMap((text) => text.split(/[,.]/)).map((s) => s.trim()).filter((s) => s.includes(' '));
  assert.ok(phrases.length > 5, `only ${phrases.length} phrases`);
  for (const phrase of phrases) assert.ok(!brand.includes(phrase), `capture/brand.mjs copies "${phrase}" instead of reading page.hero from src/i18n/en.ts`);
  assert.ok(brand.includes("'site/src/i18n/en.ts'"), 'capture/brand.mjs does not read src/i18n/en.ts');
  for (const key of ['title', 'pitch', 'strip']) assert.ok(brand.includes(`hero.${key}`), `capture/brand.mjs does not show hero.${key}`);
});
