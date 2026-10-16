import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

import { build } from 'esbuild';

const dir = await mkdtemp(join(tmpdir(), 'cornercase-plugins-dev-'));
try {
  const out = join(dir, 'dev.mjs');
  await build({
    stdin: {
      contents: "export { fixtureIndex } from './tests/fixtures/plugins/fixture'; export { validate } from './src/lib/plugins/catalogue';",
      resolveDir: process.cwd(),
      loader: 'ts',
    },
    outfile: out,
    bundle: true,
    platform: 'node',
    format: 'esm',
  });
  const { fixtureIndex, validate } = await import(pathToFileURL(out).href);
  const text = `${JSON.stringify(fixtureIndex(new Date()), null, 2)}\n`;
  validate(text);
  await writeFile('plugins/index.dev.json', text);
  console.log('wrote plugins/index.dev.json');
} finally {
  await rm(dir, { recursive: true, force: true });
}
