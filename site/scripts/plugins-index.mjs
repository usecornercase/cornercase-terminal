import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

import { build } from 'esbuild';

const PUBLISHED = 'https://usecornercase.dev/plugins/index.json';
const dir = await mkdtemp(join(tmpdir(), 'cornercase-plugins-'));
try {
  const out = join(dir, 'index.mjs');
  await build({
    stdin: {
      contents: "export { crawl, assemble } from './src/lib/plugins/crawl'; export { validate } from './src/lib/plugins/catalogue';",
      resolveDir: process.cwd(),
      loader: 'ts',
    },
    outfile: out,
    bundle: true,
    platform: 'node',
    format: 'esm',
  });
  const { crawl, assemble, validate } = await import(pathToFileURL(out).href);
  const blocklist = JSON.parse(await readFile('plugins/blocklist.json', 'utf8'));
  let text;
  try {
    const index = assemble(await crawl(fetch, process.env.GITHUB_TOKEN), blocklist, new Date());
    text = `${JSON.stringify(index, null, 2)}\n`;
    const { total, missing, invalid, duplicate, blocked } = index.source;
    console.log(`${index.plugins.length} plugins from ${total} repositories (${missing} without a manifest, ${invalid} invalid, ${duplicate} duplicate, ${blocked} blocked)`);
  } catch (error) {
    console.warn(`the crawl failed (${error.message}); keeping the published index`);
    const live = await fetch(PUBLISHED);
    if (!live.ok) throw new Error(`the published index answered ${live.status}`);
    text = await live.text();
  }
  validate(text);
  await writeFile('plugins/index.json', text);
} finally {
  await rm(dir, { recursive: true, force: true });
}
