import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { policy } from '../src/lib/csp';
import { CSP, csp } from '../src/lib/plugins/page';

const read = (path: string) => readFileSync(path, 'utf8');

test('the production policies are exactly what they were', () => {
  assert.equal(
    CSP,
    "default-src 'none'; script-src 'self'; style-src 'self'; font-src 'self'; img-src 'self' data: https://avatars.githubusercontent.com https://github.com; connect-src 'none'; base-uri 'none'; form-action 'none'",
  );
  assert.equal(
    policy("'self' blob: data:"),
    "default-src 'none'; script-src 'self'; style-src 'self'; font-src 'self'; img-src 'self' blob: data:; connect-src 'none'; base-uri 'none'; form-action 'none'",
  );
  assert.equal(csp(false), CSP);
});

test('only the dev policy allows inline scripts and styles and the dev server\'s sockets', () => {
  const dev = csp(true);
  assert.ok(dev.includes("script-src 'self' 'unsafe-inline'"));
  assert.ok(dev.includes("style-src 'self' 'unsafe-inline'"));
  assert.ok(dev.includes('ws://localhost:*'));
  assert.ok(!CSP.includes('unsafe-inline') && !CSP.includes('ws:'));
  assert.ok(dev.includes("img-src 'self' data: https://avatars.githubusercontent.com https://github.com"));
});

test('both standalone pages take their policy from the shared helper with Astro\'s dev flag', () => {
  assert.ok(read('src/pages/stats.astro').includes('policy("\'self\' blob: data:", import.meta.env.DEV)'));
  assert.ok(read('src/pages/plugins/index.astro').includes('csp(import.meta.env.DEV)'));
  assert.ok(!/Content-Security-Policy" content="/.test(read('src/pages/stats.astro')), 'no hand-written policy left in stats');
});

test('the fixture index is read only by the dev server', () => {
  const page = read('src/pages/plugins/index.astro');
  assert.ok(page.includes("import.meta.env.DEV && process.env.PLUGINS_INDEX === 'dev'"));
  assert.ok(!/PLUGINS_INDEX === 'dev'(?!.*DEV)/.test(page.replace(/import\.meta\.env\.DEV && process\.env\.PLUGINS_INDEX === 'dev'/g, '')));
});

test('the plugin copy listener only handles plugin command buttons, never the footer\'s', () => {
  const client = read('src/lib/plugins/client.ts');
  assert.ok(!client.includes("[data-copy]"), 'the footer install box uses data-copy');
  assert.ok(client.includes("'.plugins-page [data-plugin-copy]'"));
  assert.ok(read('src/components/plugins/Command.astro').includes('data-plugin-copy={command}'));
  assert.ok(!read('src/components/plugins/Command.astro').includes(' data-copy='));
  assert.ok(read('src/components/landing/InstallBox.astro').includes('data-copy'));
});
