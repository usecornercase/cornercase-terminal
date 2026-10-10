import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

import { build } from 'esbuild';

const dir = await mkdtemp(join(tmpdir(), 'cornercase-demo-'));
try {
  const bundle = join(dir, 'tests.mjs');
  await build({
    entryPoints: ['tests/changes.test.ts'],
    outfile: bundle,
    bundle: true,
    platform: 'node',
    format: 'esm',
    loader: { '.png': 'dataurl' },
  });
  await import(pathToFileURL(bundle).href);
} finally {
  await rm(dir, { recursive: true, force: true });
}
