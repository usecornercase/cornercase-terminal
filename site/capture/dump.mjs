import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const [, , dataModule, out] = process.argv;
const data = await import(dataModule);
const now = Date.now();
const assets = fileURLToPath(new URL('../src/assets/demo/', import.meta.url));

function write(dir, tree) {
  mkdirSync(dir, { recursive: true });
  for (const [name, node] of Object.entries(tree)) {
    if (typeof node !== 'string') write(join(dir, name), node);
    else if (data.picture(node)) writeFileSync(join(dir, name), readFileSync(join(assets, name)));
    else writeFileSync(join(dir, name), node);
  }
}

for (const [name, tree] of Object.entries(data.TREES)) write(join(out, 'trees', name), tree);

const fixed = join(out, 'fixed');
mkdirSync(fixed, { recursive: true });
writeFileSync(join(fixed, 'address.rs'), data.ADDRESS_FIXED_RS);
writeFileSync(join(fixed, 'mod.rs'), data.RETURNS_FIXED_RS);
writeFileSync(join(fixed, 'tests.rs'), data.RETURNS_TESTS_RS);

const ms = { h: 3600e3, d: 86400e3, w: 7 * 86400e3, mo: 30 * 86400e3, y: 365 * 86400e3 };
const ago = (age) => new Date(now - Number(age.replace(/\D/g, '')) * ms[age.replace(/\d/g, '')]).toISOString();
const issue = (i) => ({
  number: i.number,
  title: i.title,
  state: i.state.toUpperCase(),
  labels: i.labels.map((name) => ({ name })),
  assignees: i.mine ? [{ login: 'ana' }] : [],
  author: { login: i.author },
  updatedAt: ago(i.age),
  url: i.url,
});
const github = data.ISSUES.filter((i) => i.source === 'github');
const gh = join(out, 'gh');
mkdirSync(gh, { recursive: true });
writeFileSync(join(gh, 'list.json'), JSON.stringify(github.map(issue)));
writeFileSync(join(gh, 'mine.json'), JSON.stringify(github.filter((i) => i.mine).map(issue)));
for (const i of github) {
  const comments = i.number === 482 ? [{ author: { login: 'luis' }, body: 'Reproduced on main with an empty address line. Happens since 0.5.0.', createdAt: ago('2d') }] : [];
  writeFileSync(join(gh, `view-${i.number}.json`), JSON.stringify({ ...issue(i), body: i.body, comments }));
}

const run = (pull, i) => `https://github.com/acme/web-shop/actions/runs/${7000 + pull.number}/job/${9000 + i}`;
const rollup = (pull) =>
  pull.checks.map((check, i) =>
    check.workflow
      ? {
          __typename: 'CheckRun',
          name: check.name,
          workflowName: check.workflow,
          status: check.outcome === 'running' ? 'IN_PROGRESS' : 'COMPLETED',
          conclusion: { passed: 'SUCCESS', failed: 'FAILURE', skipped: 'SKIPPED', running: '' }[check.outcome],
          detailsUrl: run(pull, i),
          startedAt: ago('1h'),
        }
      : { __typename: 'StatusContext', context: check.name, state: 'SUCCESS', targetUrl: '', startedAt: ago('1h') },
  );
for (const pull of data.PULLS) {
  const url = `https://github.com/acme/web-shop/pull/${pull.number}`;
  const view = { number: pull.number, title: pull.title, url, state: 'OPEN', isDraft: false, baseRefName: 'main', mergeable: 'MERGEABLE', statusCheckRollup: rollup(pull) };
  writeFileSync(join(gh, `pr-${pull.branch.replace(/\//g, '-')}.json`), JSON.stringify(view));
}

const rollupState = (pull) => (pull.checks.some((c) => c.outcome === 'failed') ? 'FAILURE' : pull.checks.some((c) => c.outcome === 'running') ? 'PENDING' : 'SUCCESS');
const layer = (number, i) => {
  const pull = data.PULLS.find((p) => p.number === number);
  const url = `https://github.com/acme/web-shop/pull/${number}`;
  const node = { number, title: pull.title, url, state: 'OPEN', isDraft: false, mergeable: 'MERGEABLE', reviewDecision: null, statusCheckRollup: { state: rollupState(pull) } };
  return { position: i + 1, pullRequest: node };
};
const stack = { number: data.STACK.number, size: data.STACK.layers.length, baseRefName: 'main', entries: { nodes: data.STACK.layers.map(layer) } };
writeFileSync(join(gh, 'stack.json'), JSON.stringify({ data: { resource: { stack } } }));

const at = (minutes) => new Date(now + minutes * 60e3).toISOString();
const unix = (minutes) => Math.floor((now + minutes * 60e3) / 1000);
const limit = (kind, group, percent, severity, minutes, scope = null) => ({ kind, group, percent, severity, resets_at: at(minutes), scope, is_active: false });
const claude = {
  type: 'control_response',
  response: {
    subtype: 'success',
    request_id: 'usage',
    response: {
      subscription_type: 'max',
      rate_limits_available: true,
      rate_limits: {
        five_hour: { utilization: 34, resets_at: at(134) },
        seven_day: { utilization: 81, resets_at: at(3 * 1440 + 240) },
        extra_usage: { is_enabled: false },
        limits: [
          limit('session', 'session', 34, 'normal', 134),
          limit('weekly_all', 'weekly', 81, 'warning', 3 * 1440 + 240),
          limit('weekly_scoped', 'weekly', 12, 'normal', 3 * 1440 + 240, { model: { id: null, display_name: 'Opus' } }),
        ],
      },
    },
  },
};
const window = (used, minutes, resetMinutes) => ({ usedPercent: used, windowDurationMins: minutes, resetsAt: unix(resetMinutes) });
const codexLimits = {
  limitId: 'codex',
  limitName: null,
  normalModelSlug: null,
  primary: window(22, 300, 221),
  secondary: window(9, 10080, 4 * 1440 + 600),
  credits: { hasCredits: false, unlimited: false, balance: '0' },
  individualLimit: null,
  spendControlReached: false,
  planType: 'plus',
  rateLimitReachedType: null,
};
const codex = { id: 2, result: { ordinaryUsageAllowed: true, rateLimits: codexLimits, rateLimitsByLimitId: { codex: codexLimits } } };
const usage = join(out, 'usage');
mkdirSync(usage, { recursive: true });
writeFileSync(join(usage, 'claude.json'), JSON.stringify(claude));
const plan = (subscription, session, week, opus, sessionMinutes) => {
  const response = structuredClone(claude);
  const answer = response.response.response;
  answer.subscription_type = subscription;
  answer.rate_limits.five_hour = { utilization: session, resets_at: at(sessionMinutes) };
  answer.rate_limits.seven_day = { utilization: week, resets_at: at(3 * 1440 + 240) };
  answer.rate_limits.limits = [
    limit('session', 'session', session, session >= 80 ? 'warning' : 'normal', sessionMinutes),
    limit('weekly_all', 'weekly', week, week >= 80 ? 'warning' : 'normal', 3 * 1440 + 240),
    limit('weekly_scoped', 'weekly', opus, 'normal', 3 * 1440 + 240, { model: { id: null, display_name: 'Opus' } }),
  ];
  return response;
};
writeFileSync(join(usage, 'claude-personal.json'), JSON.stringify(plan('pro', 8, 19, 0, 263)));
writeFileSync(join(usage, 'codex.json'), JSON.stringify(codex));
