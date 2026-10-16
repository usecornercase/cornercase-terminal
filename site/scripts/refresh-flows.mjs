import { readFile, writeFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const DEFAULT_API = 'https://api.github.com';
const API = process.env.GITHUB_API_URL ?? DEFAULT_API;
const TIMEOUT = 15000;
const POOL = 8;
const RAW = process.env.CORNERCASE_FLOWS_RAW ?? 'https://raw.githubusercontent.com';
const TOPIC = 'cornercase-flow';
const FILES = ['flow.toml', 'cornercase-flow.toml'];
const MAX_BYTES = 256 * 1024;
const MAX_REPOS = 300;
const out = join(dirname(fileURLToPath(import.meta.url)), '..', 'src', 'data', 'flows.json');

const headers = { Accept: 'application/vnd.github+json', 'User-Agent': 'cornercase-flows' };
if (process.env.GITHUB_TOKEN && API === DEFAULT_API) headers.Authorization = `Bearer ${process.env.GITHUB_TOKEN}`;

async function repositories() {
  const found = [];
  for (let page = 1; found.length < MAX_REPOS; page += 1) {
    const url = `${API}/search/repositories?q=${encodeURIComponent(`topic:${TOPIC}`)}&sort=stars&order=desc&per_page=100&page=${page}`;
    const response = await fetch(url, { headers, signal: AbortSignal.timeout(TIMEOUT) });
    if (!response.ok) throw new Error(`GitHub answered ${response.status} for the search: ${(await response.text()).slice(0, 200)}`);
    const body = await response.json();
    found.push(...body.items);
    if (body.items.length < 100) break;
  }
  return found.filter((r) => !r.fork && !r.private && !r.archived && !r.disabled);
}

async function flowOf(repo) {
  for (const file of FILES) {
    const response = await fetch(`${RAW}/${repo.full_name}/HEAD/${file}`, { headers: { 'User-Agent': headers['User-Agent'] }, signal: AbortSignal.timeout(TIMEOUT) });
    if (response.status === 404) continue;
    if (!response.ok) throw new Error(`answered ${response.status} for ${file}`);
    const bytes = Buffer.from(await response.arrayBuffer());
    if (bytes.length > MAX_BYTES) throw new Error(`${file} is longer than ${MAX_BYTES / 1024} KiB`);
    return { file, text: bytes.toString('utf8') };
  }
  return null;
}

try {
  const all = await repositories();
  const slots = new Array(all.length).fill(null);
  let next = 0;
  const worker = async () => {
    for (let i = next++; i < all.length; i = next++) {
      const repo = all[i];
      if (!/^[\w.-]+\/[\w.-]+$/.test(repo.full_name)) {
        console.warn(`flows: skipped ${repo.full_name}: not an owner/repo name`);
        continue;
      }
      try {
      const found = await flowOf(repo);
      if (!found) {
        console.warn(`flows: ${repo.full_name} has none of ${FILES.join(', ')}`);
        continue;
      }
      slots[i] = {
        repo: repo.full_name,
        stars: repo.stargazers_count,
        description: (repo.description ?? '').replace(/[\u0000-\u001f]/g, ' ').slice(0, 300),
        topics: repo.topics ?? [],
        ...found,
      };
      } catch (e) {
        console.warn(`flows: skipped ${repo.full_name}: ${e.message}`);
      }
    }
  };
  await Promise.all(Array.from({ length: Math.min(POOL, all.length) }, worker));
  const flows = slots.filter(Boolean);
  const before = JSON.parse(await readFile(out, 'utf8').catch(() => '{}'));
  const same = JSON.stringify(before.flows) === JSON.stringify(flows);
  await writeFile(out, `${JSON.stringify({ fetched: same ? before.fetched : new Date().toISOString(), flows }, null, 2)}\n`);
  console.log(`flows: ${flows.length} repositories with the topic ${TOPIC}`);
} catch (e) {
  console.error(`flows: could not refresh, keeping the snapshot: ${e.message}`);
  process.exit(1);
}
