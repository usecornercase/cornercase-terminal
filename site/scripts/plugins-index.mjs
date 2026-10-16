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
      contents: "export { crawl, assemble, keep } from './src/lib/plugins/crawl'; export { validate } from './src/lib/plugins/catalogue';",
      resolveDir: process.cwd(),
      loader: 'ts',
    },
    outfile: out,
    bundle: true,
    platform: 'node',
    format: 'esm',
  });
  const { crawl, assemble, keep, validate } = await import(pathToFileURL(out).href);
  const published = async () => {
    try {
      const live = await fetch(PUBLISHED);
      return live.ok ? validate(await live.text()) : null;
    } catch {
      return null;
    }
  };
  const previous = await published();
  const blocklist = JSON.parse(await readFile('plugins/blocklist.json', 'utf8'));
  let text;
  try {
    const index = assemble(await crawl(fetch, process.env.GITHUB_TOKEN, previous), blocklist, new Date());
    text = `${JSON.stringify(index, null, 2)}\n`;
    const plugins = index.repos.reduce((n, card) => n + card.plugins.filter((p) => !p.duplicate).length, 0);
    const { total, missing, invalid, duplicate, blocked } = index.source;
    console.log(`${plugins} plugins in ${index.repos.length} of ${total} repositories (${missing} without a manifest, ${invalid} invalid manifests, ${duplicate} duplicates, ${blocked} blocked)`);
  } catch (error) {
    if (!previous) throw error;
    console.warn(`the crawl failed (${error.message}); keeping the published index`);
    text = `${JSON.stringify(keep(previous, blocklist), null, 2)}\n`;
  }
  validate(text);
  await writeFile('plugins/index.json', text);
} finally {
  await rm(dir, { recursive: true, force: true });
}
