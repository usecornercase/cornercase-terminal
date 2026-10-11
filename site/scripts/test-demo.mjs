import { mkdtemp, readdir, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

import { build } from 'esbuild';

const dir = await mkdtemp(join(tmpdir(), 'cornercase-demo-'));
try {
  const tests = (await readdir('tests')).filter((name) => name.endsWith('.test.ts'));
  await build({
    entryPoints: tests.map((name) => join('tests', name)),
    outdir: dir,
    outExtension: { '.js': '.mjs' },
    bundle: true,
    platform: 'node',
    format: 'esm',
    loader: { '.png': 'dataurl' },
  });
  for (const name of tests) await import(pathToFileURL(join(dir, name.replace(/\.ts$/, '.mjs'))).href);
} finally {
  await rm(dir, { recursive: true, force: true });
}
