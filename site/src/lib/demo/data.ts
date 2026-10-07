export type Tree = { [name: string]: string | Tree };

export const ADDRESS_RS = `use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Address {
    pub lines: Vec<String>,
    pub city: String,
    pub postcode: String,
}

impl Address {
    pub fn first_line(&self) -> &str {
        self.lines.first().unwrap()
    }
}
`;

export const ADDRESS_FIXED_RS = `use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Address {
    pub lines: Vec<String>,
    pub city: String,
    pub postcode: String,
}

impl Address {
    pub fn first_line(&self) -> Option<&str> {
        self.lines.first().map(String::as_str)
    }
}
`;

export const THEME_RS = `pub enum Theme {
    Light,
    Dark,
}

impl Theme {
    pub fn background(&self) -> &'static str {
        match self {
            Theme::Light => "#ffffff",
            Theme::Dark => "#0e0d14",
        }
    }
}
`;

export const RETURNS_RS = `mod address;

use axum::{Json, Router, routing::post};

pub use address::Address;

pub fn routes() -> Router {
    Router::new().route("/returns", post(create))
}

async fn create(Json(address): Json<Address>) -> String {
    format!("label for {}", address.first_line())
}
`;

const SHOP: Tree = {
  'Cargo.toml': `[package]
name = "shop"
version = "0.5.0"
edition = "2024"

[dependencies]
axum = "0.8"
serde = { version = "1", features = ["derive"] }
tokio = { version = "1", features = ["full"] }
`,
  'README.md': `# shop

The storefront and returns service.

## Running

    cargo run
`,
  '.worktreeinclude': '.env\n',
  '.env': 'DATABASE_URL=postgres://localhost/shop\n',
  src: {
    'main.rs': `mod checkout;
mod returns;
mod theme;

#[tokio::main]
async fn main() {
    let app = returns::routes().merge(checkout::routes());
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
`,
    'checkout.rs': `use axum::Router;

pub fn routes() -> Router {
    Router::new()
}
`,
    'theme.rs': THEME_RS,
    returns: {
      'mod.rs': RETURNS_RS,
      'address.rs': ADDRESS_RS,
    },
  },
};

export const API: Tree = {
  'package.json': '{ "name": "api", "version": "2.1.0", "type": "module" }\n',
  'README.md': '# api\n\nPublic API for orders and returns.\n',
  src: {
    'server.ts': `import { createServer } from 'node:http';

export const port = 8080;

createServer((req, res) => {
  res.writeHead(200, { 'content-type': 'application/json' });
  res.end(JSON.stringify({ ok: true, path: req.url }));
}).listen(port);
`,
    'orders.ts': 'export const orders = new Map<string, number>();\n',
  },
};

const INFRA: Tree = {
  'main.tf': 'terraform {\n  required_version = ">= 1.9"\n}\n',
  'variables.tf': 'variable "region" {\n  default = "eu-west-1"\n}\n',
  modules: { network: { 'main.tf': '# network\n' } },
};

const NOTES: Tree = {
  'todo.md': '# Notes\n\n- ship the returns fix\n- review dark mode\n- try three agents on the same issue\n',
  'ideas.md': '# Ideas\n\n- gift cards\n- faster checkout\n',
};

export const TREES: Record<string, Tree> = { shop: SHOP, api: API, infra: INFRA, notes: NOTES };

export const FOLDERS: Record<string, { repo: boolean; tree: Tree }> = {
  shop: { repo: true, tree: SHOP },
  api: { repo: true, tree: API },
  infra: { repo: true, tree: INFRA },
  notes: { repo: false, tree: NOTES },
  web: { repo: true, tree: { 'index.html': '<!doctype html>\n<title>web</title>\n', 'package.json': '{ "name": "web" }\n' } },
  dotfiles: { repo: false, tree: { '.zshrc': 'export EDITOR=nvim\n' } },
  playground: { repo: false, tree: { 'scratch.txt': 'hello\n' } },
};

export const COMMITS: Record<string, [string, string, string][]> = {
  shop: [
    ['b1b91ec', 'Release 0.5.0', '3 days ago'],
    ['d12e736', 'Explain how to run it', '12 days ago'],
    ['6b14f1e', 'Start the returns service', '3 weeks ago'],
  ],
  api: [
    ['4f2a9c1', 'Paginate orders', '2 days ago'],
    ['91c03de', 'Initial API', '6 weeks ago'],
  ],
  infra: [['7a7d2b0', 'Add terraform skeleton', '2 months ago']],
};

export type SourceId = 'github' | 'shortcut' | 'linear' | 'jira';

export interface Issue {
  source: SourceId;
  number: number;
  key: string;
  title: string;
  labels: string[];
  state: string;
  author: string;
  age: string;
  url: string;
  body: string;
  mine: boolean;
  closed?: boolean;
}

const BODY_482 = `When a customer starts a return without an address line, the returns page crashes with a 500.

## Steps to reproduce

1. Open a delivered order
2. Click **Start a return**
3. Clear the address and submit

## Expected

A validation error: *Enter the first line of your address*.

## Logs

\`\`\`rust
thread 'tokio-runtime-worker' panicked at src/returns/address.rs:12:33:
called \`Option::unwrap()\` on a \`None\` value
\`\`\`

- [ ] Answer 422 instead of panicking
- [ ] Add a test with an empty \`lines\` array`;

const gh = (number: number, title: string, labels: string[], author: string, age: string, body: string, mine = false): Issue => ({
  source: 'github',
  number,
  key: `#${number}`,
  title,
  labels,
  state: 'open',
  author,
  age,
  url: `https://github.com/acme/shop/issues/${number}`,
  body,
  mine,
});

export const ISSUES: Issue[] = [
  gh(482, 'Returns page crashes on empty address', ['bug'], 'ana', '3d', BODY_482, true),
  gh(479, 'Dark mode for the checkout', ['feature', 'design'], 'luis', '5d', 'The checkout ignores `prefers-color-scheme`.\n\n## Scope\n\n- Use the `Theme` from `src/theme.rs`\n- Keep the brand purple on both themes'),
  gh(476, 'Refund emails show the wrong currency', ['bug'], 'marta', '8d', 'Refunds for orders paid in EUR are announced in USD.'),
  gh(471, 'Add Apple Pay to the checkout', ['feature'], 'ana', '15d', 'Behind a feature flag first.', true),
  gh(468, 'Flaky test: returns::label_is_printed', ['ci'], 'luis', '22d', 'Fails about once in twenty runs on CI.'),
  gh(455, 'Rate-limit the public returns endpoint', ['security'], 'marta', '1mo', 'Ten requests per minute per IP is plenty.'),
  {
    source: 'shortcut',
    number: 48,
    key: 'sc-48',
    title: 'Dark mode',
    labels: [],
    state: 'In Review',
    author: 'luis',
    age: '2mo',
    url: 'https://app.shortcut.com/acme/story/48',
    body: 'Ship dark mode on every page, starting with the checkout.',
    mine: false,
  },
  {
    source: 'shortcut',
    number: 52,
    key: 'sc-52',
    title: 'Gift cards at checkout',
    labels: ['payments'],
    state: 'Ready',
    author: 'ana',
    age: '6d',
    url: 'https://app.shortcut.com/acme/story/52',
    body: 'Accept gift card codes next to coupons.',
    mine: true,
  },
  {
    source: 'linear',
    number: 123,
    key: 'ENG-123',
    title: 'Speed up the order history query',
    labels: ['performance'],
    state: 'In Progress',
    author: 'marta',
    age: '1d',
    url: 'https://linear.app/acme/issue/ENG-123',
    body: 'The query takes 900 ms for customers with many orders. Add an index on `(customer_id, created_at)`.',
    mine: true,
  },
  {
    source: 'linear',
    number: 131,
    key: 'ENG-131',
    title: 'Split payments between two cards',
    labels: ['payments'],
    state: 'Todo',
    author: 'luis',
    age: '4d',
    url: 'https://linear.app/acme/issue/ENG-131',
    body: 'Customers want to pay part with a gift card and the rest with a card.',
    mine: false,
  },
  {
    source: 'jira',
    number: 77,
    key: 'SHOP-77',
    title: 'Show the return label as a QR code',
    labels: ['returns'],
    state: 'In Progress',
    author: 'marta',
    age: '2d',
    url: 'https://acme.atlassian.net/browse/SHOP-77',
    body: 'Customers without a printer should be able to show a QR code at the drop-off point.\n\n- [ ] Generate the code from the label id\n- [ ] Add it to the confirmation email',
    mine: true,
  },
  {
    source: 'jira',
    number: 81,
    key: 'SHOP-81',
    title: 'Translate the returns page into Spanish',
    labels: ['i18n'],
    state: 'To Do',
    author: 'luis',
    age: '9d',
    url: 'https://acme.atlassian.net/browse/SHOP-81',
    body: 'Every string of the returns flow goes through `t()`; the **es** catalogue is missing.',
    mine: false,
  },
];

export const AGENTS: [string, string][] = [
  ['claude', 'claude'],
  ['codex', 'codex'],
  ['gemini', 'gemini'],
  ['opencode', 'opencode'],
  ['cursor', 'cursor-agent'],
  ['copilot', 'copilot'],
  ['amp', 'amp'],
  ['droid', 'droid'],
  ['pi', 'pi'],
  ['qwen', 'qwen'],
  ['kimi', 'kimi'],
  ['cline', 'cline'],
  ['goose', 'goose'],
  ['aider', 'aider'],
];

export const MODES: Record<string, [string, string][]> = {
  claude: [
    ['accept edits', '--permission-mode acceptEdits'],
    ['auto', '--permission-mode auto'],
    ['plan', '--permission-mode plan'],
    ['skip permissions (dangerous)', '--dangerously-skip-permissions'],
  ],
  codex: [
    ['read only', '--sandbox read-only'],
    ['workspace write', '--sandbox workspace-write'],
    ['no sandbox, no approvals (dangerous)', '--dangerously-bypass-approvals-and-sandbox'],
  ],
  gemini: [
    ['auto edit', '--approval-mode auto_edit'],
    ['yolo (dangerous)', '--yolo'],
  ],
};

export interface UsageWindow {
  label: string;
  percent: number;
  severity: 'normal' | 'warning' | 'critical';
  resets: string;
}

export interface UsageSection {
  agent: string;
  plan: string;
  windows: UsageWindow[];
}

export const USAGE: UsageSection[] = [
  {
    agent: 'Claude Code',
    plan: 'max',
    windows: [
      { label: 'session (5h)', percent: 34, severity: 'normal', resets: 'resets in 2h 14m' },
      { label: 'week', percent: 81, severity: 'warning', resets: 'resets in 3d 4h' },
      { label: 'week · Opus', percent: 12, severity: 'normal', resets: 'resets in 3d 4h' },
    ],
  },
  {
    agent: 'Codex',
    plan: 'plus',
    windows: [
      { label: 'session (5h)', percent: 22, severity: 'normal', resets: 'resets in 3h 41m' },
      { label: 'week', percent: 9, severity: 'normal', resets: 'resets in 5d 6h' },
    ],
  },
];
