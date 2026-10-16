import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:http';
import type { AddressInfo } from 'node:net';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';

function refresh(search: object): Promise<{ code: number; before: string; after: string }> {
  const dir = mkdtempSync(join(tmpdir(), 'refresh-'));
  const out = join(dir, 'flows.json');
  const before = '{"fetched":"old","flows":[]}\n';
  writeFileSync(out, before);
  const server = createServer((request, response) => {
    response.setHeader('Content-Type', 'application/json');
    response.end(request.url?.startsWith('/search/') ? JSON.stringify(search) : '');
  });
  return new Promise((done) => {
    server.listen(0, '127.0.0.1', () => {
      const url = `http://127.0.0.1:${(server.address() as AddressInfo).port}`;
      execFile('node', ['scripts/refresh-flows.mjs'], { env: { ...process.env, GITHUB_API_URL: url, CORNERCASE_FLOWS_OUT: out, GITHUB_TOKEN: '' } }, (error) => {
        server.close();
        const after = readFileSync(out, 'utf8');
        rmSync(dir, { recursive: true, force: true });
        done({ code: error ? ((error as { code?: number }).code ?? 1) : 0, before, after });
      });
    });
  });
}

test('incomplete search results fail the refresh and keep the snapshot', async () => {
  const run = await refresh({ incomplete_results: true, items: [] });
  assert.equal(run.code, 1);
  assert.equal(run.after, run.before);
});

test('a complete, empty search refreshes the snapshot', async () => {
  const run = await refresh({ incomplete_results: false, total_count: 0, items: [] });
  assert.equal(run.code, 0);
  assert.deepEqual(JSON.parse(run.after).flows, []);
});
