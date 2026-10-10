import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const [, , dataModule, out] = process.argv;
const data = await import(dataModule);
const now = Date.now();

function write(dir, tree) {
  mkdirSync(dir, { recursive: true });
  for (const [name, node] of Object.entries(tree)) {
    if (typeof node === 'string') writeFileSync(join(dir, name), node);
    else write(join(dir, name), node);
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
writeFileSync(join(usage, 'codex.json'), JSON.stringify(codex));
