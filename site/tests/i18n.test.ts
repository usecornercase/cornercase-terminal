import assert from 'node:assert/strict';
import { test } from 'node:test';

import { DEMO } from '../src/i18n/demo';
import * as en from '../src/i18n/en';
import * as es from '../src/i18n/es';
import * as ptBr from '../src/i18n/pt-br';
import { App } from '../src/lib/demo/app';
import { chapters } from '../src/lib/demo/boot';
import type { Mounted } from '../src/lib/demo/client';
import { play } from '../src/lib/demo/tour';
import { world } from '../src/lib/demo/world';

const translations = { es, 'pt-br': ptBr };

const marks = (s: string) => [...s.matchAll(/\{\w+\}|`[^`]+`/g)].map(([m]) => (m.startsWith('`') ? '`code`' : m)).sort();

function same(english: unknown, other: unknown, path: string): void {
  if (typeof english === 'function') {
    assert.equal(typeof other, 'function', path);
    const f = other as (...args: unknown[]) => unknown;
    assert.equal(f.length, (english as (...args: unknown[]) => unknown).length, path);
    same((english as (a: unknown) => unknown)('X'), f('X'), `${path}()`);
    return;
  }
  if (Array.isArray(english)) {
    assert.ok(Array.isArray(other), path);
    assert.equal(other.length, english.length, path);
    english.forEach((e, i) => same(e, other[i], `${path}[${i}]`));
    return;
  }
  if (typeof english === 'object' && english !== null) {
    assert.equal(typeof other, 'object', path);
    assert.deepEqual(Object.keys(other as object).sort(), Object.keys(english).sort(), path);
    for (const [k, v] of Object.entries(english)) same(v, (other as Record<string, unknown>)[k], `${path}.${k}`);
    return;
  }
  assert.equal(typeof other, typeof english, path);
  if (typeof english === 'string') {
    assert.equal((other as string) === '', english === '', path);
    assert.deepEqual(marks(other as string), marks(english), path);
  }
}

for (const [locale, words] of Object.entries(translations)) {
  test(`the ${locale} landing has every text the English one has`, () => {
    same(en.page, words.page, 'page');
    same(en.demo, words.demo, 'demo');
  });
}

function tourApp(columns: number, narrate: App['narration']): { app: App; said: string[] } {
  const app = new App();
  app.virtual = true;
  app.scripted = true;
  app.narration = narrate;
  app.resize(columns, 34);
  world(app, { scripted: true });
  const said: string[] = [];
  app.on((event, detail) => {
    if (event === 'narrate' && detail) said.push(detail);
  });
  return { app, said };
}

async function tour(locale: keyof typeof DEMO, columns: number): Promise<string[]> {
  const { app, said } = tourApp(columns, DEMO[locale].narrate);
  let finished = false;
  const steps = chapters(DEMO[locale].tour).flat();
  steps.push({ run: () => void (finished = true) });
  await play({ app } as Mounted, null, steps, { instant: true }).done;
  assert.ok(finished);
  app.destroy();
  return said;
}

for (const columns of [140, 80]) {
  test(`the hero tour narrates in each language and reaches its end at ${columns} columns`, async () => {
    const english = await tour('en', columns);
    assert.ok(english.length > 0);
    for (const locale of Object.keys(translations) as (keyof typeof DEMO)[]) {
      const said = await tour(locale, columns);
      assert.equal(said.length, english.length, locale);
      said.forEach((line, i) => assert.notEqual(line, english[i], `${locale}: ${line}`));
    }
  });
}
