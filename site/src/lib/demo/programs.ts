import type { Cursor } from '../term/canvas';
import { BOLD, type Grid, type Rect, type Style } from '../term/grid';
import { ADDRESS_FIXED_RS, COMMITS, type Tree } from './data';
import { highlight, language } from './highlight';
import { type Line, drawLine, paintRows, pad, plain, seg, truncateRight, width, wrapAll, wrapWords } from './text';

export interface Context {
  model: string;
  percent: number | null;
}

export interface Key {
  key: string;
  ctrl?: boolean;
  alt?: boolean;
  shift?: boolean;
}

export interface Place {
  project: string;
  root: string;
  branch?: string;
  repo: boolean;
  tree: Tree;
  flags: { fixed?: boolean; dark?: boolean };
}

export interface Host {
  after(ms: number, fn: () => void): () => void;
  dirty(): void;
  exit(): void;
  place(): Place;
  pace(): number;
}

export interface Program {
  readonly name: string;
  readonly mouse: boolean;
  draw(g: Grid, r: Rect, focused: boolean): Cursor | null;
  key(k: Key): void;
  paste(text: string): void;
  click?(x: number, y: number): void;
  screen(): string;
  dispose?(): void;
}

export const AGENT_KINDS: Record<string, string> = {
  claude: 'claude',
  codex: 'codex',
  gemini: 'gemini',
  opencode: 'opencode',
  'cursor-agent': 'cursor',
  copilot: 'copilot',
  amp: 'amp',
  droid: 'droid',
  pi: 'pi',
  qwen: 'qwen',
  kimi: 'kimi',
  cline: 'cline',
  goose: 'goose',
  aider: 'aider',
};

const CYAN: Style = { fg: 6 };
const DIMMED: Style = { fg: 8 };
const GREEN: Style = { fg: 2 };
const RED: Style = { fg: 1 };
const YELLOW: Style = { fg: 3 };
const MAGENTA: Style = { fg: 5 };
const BLUE_BOLD: Style = { fg: 4, add: BOLD };

function lookup(tree: Tree, path: string[]): string | Tree | undefined {
  let node: string | Tree | undefined = tree;
  for (const part of path) {
    if (!node || typeof node === 'string') return undefined;
    node = node[part];
  }
  return node;
}

function resolve(cwd: string[], arg: string): string[] {
  const parts = arg.startsWith('/') || arg.startsWith('~') ? [] : [...cwd];
  for (const p of arg.split('/')) {
    if (!p || p === '.' || p === '~') continue;
    if (p === '..') parts.pop();
    else parts.push(p);
  }
  return parts;
}

const shortFolder = (name: string) => (name.length > 22 ? `${name.slice(0, 21)}…` : name);

type Step = [number, Line];

export class Shell implements Program {
  lines: Line[] = [];
  input = '';
  cwd: string[] = [];
  fg: Program | null = null;
  private job: { name: string; cancel: () => void } | null = null;
  private history: string[] = [];
  private back = 0;
  private typing: (() => void) | null = null;

  constructor(
    private host: Host,
    lines: Line[] = [],
  ) {
    this.lines = lines;
  }

  get name(): string {
    return this.fg?.name ?? this.job?.name ?? 'bash';
  }

  get mouse(): boolean {
    return this.fg?.mouse ?? false;
  }

  get busy(): boolean {
    return this.fg !== null || this.job !== null;
  }

  get memory(): number {
    return this.busy ? PROGRAM_MEMORY : SHELL_MEMORY;
  }

  folder(): string {
    const place = this.host.place();
    if (this.cwd.length) return shortFolder(this.cwd[this.cwd.length - 1]);
    const base = place.root.split('/').pop() ?? '~';
    return shortFolder(base);
  }

  prompt(): Line {
    return [seg(this.folder(), { fg: 6, add: BOLD }), seg(' '), seg('❯', MAGENTA), seg(' ')];
  }

  print(line: Line | string): void {
    this.lines.push(typeof line === 'string' ? plain(line) : line);
    if (this.lines.length > 400) this.lines.splice(0, this.lines.length - 400);
    this.host.dirty();
  }

  draw(g: Grid, r: Rect, focused: boolean): Cursor | null {
    if (this.fg) return this.fg.draw(g, r, focused);
    const live = this.job ? [] : [[...this.prompt(), seg(this.input)]];
    const rows = wrapAll([...this.lines, ...live], r.w);
    if (!this.job && rows[rows.length - 1].length >= r.w) rows.push([]);
    const top = Math.max(0, rows.length - r.h);
    paintRows(g, r, rows, top);
    if (this.job) return null;
    return { x: r.x + rows[rows.length - 1].length, y: r.y + rows.length - 1 - top, shape: 'block' };
  }

  screen(): string {
    return this.fg ? this.fg.screen() : this.lines.map((l) => l.map((s) => s.t).join('')).join('\n');
  }

  paste(text: string): void {
    if (this.fg) return this.fg.paste(text);
    if (this.job) return;
    this.input += text.replace(/\s*\n\s*/g, ' ');
    this.host.dirty();
  }

  key(k: Key): void {
    if (this.fg) return this.fg.key(k);
    if (this.job) {
      if (k.ctrl && k.key === 'c') {
        this.job.cancel();
        this.job = null;
        this.print([seg('^C')]);
      }
      return;
    }
    if (k.ctrl) {
      if (k.key === 'c') {
        this.print([...this.prompt(), seg(this.input), seg('^C')]);
        this.input = '';
      } else if (k.key === 'l') this.lines = [];
      else if (k.key === 'd' && !this.input) this.host.exit();
      else if (k.key === 'u') this.input = '';
      this.host.dirty();
      return;
    }
    if (k.key === 'Enter') {
      const cmd = this.input;
      this.input = '';
      this.run(cmd);
    } else if (k.key === 'Backspace') this.input = this.input.slice(0, -1);
    else if (k.key === 'ArrowUp' && this.history.length) {
      this.back = Math.min(this.back + 1, this.history.length);
      this.input = this.history[this.history.length - this.back];
    } else if (k.key === 'ArrowDown') {
      this.back = Math.max(this.back - 1, 0);
      this.input = this.back ? this.history[this.history.length - this.back] : '';
    } else if (k.key.length === 1 && !k.alt) this.input += k.key;
    this.host.dirty();
  }

  type(command: string, then?: () => void): void {
    this.typing?.();
    let i = 0;
    const step = () => {
      if (i < command.length) {
        this.input += command[i];
        i += 1;
        this.host.dirty();
        this.typing = this.host.after(18 + Math.random() * 30, step);
      } else {
        this.typing = this.host.after(160, () => {
          this.typing = null;
          this.key({ key: 'Enter' });
          then?.();
        });
      }
    };
    step();
  }

  start(program: Program): void {
    this.fg = program;
    this.host.dirty();
  }

  finish(): void {
    this.fg?.dispose?.();
    this.fg = null;
    this.host.dirty();
  }

  private stream(name: string, steps: Step[], then?: () => void): void {
    let i = 0;
    let cancel = () => {};
    const next = () => {
      if (i >= steps.length) {
        this.job = null;
        then?.();
        this.host.dirty();
        return;
      }
      const [delay, line] = steps[i];
      cancel = this.host.after(delay, () => {
        this.print(line);
        i += 1;
        next();
      });
    };
    this.job = { name, cancel: () => cancel() };
    next();
  }

  private forever(name: string, head: Line[], make: () => Line, every: () => number): void {
    head.forEach((l) => this.print(l));
    let cancel = () => {};
    const tick = () => {
      cancel = this.host.after(every(), () => {
        this.print(make());
        tick();
      });
    };
    this.job = { name, cancel: () => cancel() };
    tick();
  }

  run(raw: string): void {
    this.print([...this.prompt(), seg(raw)]);
    const cmd = raw.trim();
    if (!cmd) return;
    this.history.push(cmd);
    this.back = 0;
    const place = this.host.place();
    const vars: Record<string, string> = {
      CORNERCASE: '1',
      TERM: 'xterm-256color',
      SHELL: '/bin/bash',
      HOME: '/home/you',
      USER: 'you',
      PWD: [place.root, ...this.cwd].join('/'),
    };
    const expanded = cmd.replace(/\$(\w+)/g, (_, v: string) => vars[v] ?? '');
    const [name, ...args] = expanded.split(/\s+/);
    const here = lookup(place.tree, this.cwd);
    const dir = here && typeof here !== 'string' ? here : {};
    if (name === 'clear') this.lines = [];
    else if (name === 'echo') this.print(args.join(' '));
    else if (name === 'pwd') this.print(vars.PWD);
    else if (name === 'whoami') this.print('you');
    else if (name === 'date') this.print(new Date().toString().replace(/ \(.*\)$/, ''));
    else if (name === 'uname') this.print('Linux');
    else if (name === 'history') this.history.forEach((h, i) => this.print(`  ${i + 1}  ${h}`));
    else if (name === 'env') Object.entries(vars).forEach(([k, v]) => this.print(`${k}=${v}`));
    else if (name === 'help') this.help();
    else if (name === 'exit') this.host.exit();
    else if (name === 'ls') this.ls(args, dir);
    else if (name === 'cd') this.cd(args[0] ?? '~');
    else if (name === 'cat' || name === 'bat' || name === 'less') this.cat(args[0]);
    else if (name === 'git') this.git(args);
    else if (name === 'cargo') this.cargo(args);
    else if (name === 'npm' || name === 'node' || name === 'pnpm') this.node();
    else if (['nvim', 'vim', 'vi'].includes(name)) this.edit(args[0]);
    else if (['top', 'htop', 'btop'].includes(name)) this.start(new Top(this.host, () => this.finish(), name));
    else if (name in AGENT_KINDS) this.start(new Agent(this.host, () => this.finish(), name, args));
    else if (name === 'cornercase') this.cornercase(args);
    else this.print(`bash: ${name}: command not found`);
  }

  private help(): void {
    this.print([seg('This pane is part of a simulation. Try:', { add: BOLD })]);
    const tips: [string, string][] = [
      ['ls, cd, cat, git log, git status', 'look around the repo'],
      ['cargo test', 'run the (fake) tests'],
      ['nvim src/returns/address.rs', 'open the editor, :q to quit'],
      ['claude', 'start an agent, Ctrl+C to leave'],
      ['htop', 'watch the machine, q to quit'],
      ['echo $CORNERCASE', 'every pane knows it is inside'],
    ];
    for (const [c, what] of tips) this.print([seg('  '), seg(pad(c, 32), CYAN), seg(what, DIMMED)]);
  }

  private ls(args: string[], dir: Tree): void {
    const all = args.some((a) => a.startsWith('-') && a.includes('a'));
    const long = args.some((a) => a.startsWith('-') && a.includes('l'));
    const target = args.find((a) => !a.startsWith('-'));
    const node = target ? lookup(this.host.place().tree, resolve(this.cwd, target)) : dir;
    if (!node) return this.print(`ls: cannot access '${target}': No such file or directory`);
    if (typeof node === 'string') return this.print(target ?? '');
    const names = Object.keys(node)
      .filter((n) => all || !n.startsWith('.'))
      .sort();
    if (long) {
      this.print(`total ${names.length * 4}`);
      for (const n of names) {
        const isDir = typeof node[n] !== 'string';
        const size = isDir ? 4096 : (node[n] as string).length;
        this.print([seg(`${isDir ? 'drwxr-xr-x' : '-rw-r--r--'} 1 you you ${String(size).padStart(5)} Oct  3 10:24 `), seg(n, isDir ? BLUE_BOLD : {})]);
      }
      return;
    }
    const line: Line = [];
    names.forEach((n, i) => {
      if (i) line.push(seg('  '));
      line.push(seg(n, typeof node[n] === 'string' ? {} : BLUE_BOLD));
    });
    if (line.length) this.print(line);
  }

  private cd(arg: string): void {
    const next = arg === '~' || arg === '-' ? [] : resolve(this.cwd, arg);
    const node = lookup(this.host.place().tree, next);
    if (!node || typeof node === 'string') return this.print(`bash: cd: ${arg}: No such file or directory`);
    this.cwd = next;
  }

  private file(arg: string | undefined): [string[], string | undefined] {
    if (!arg) return [[], undefined];
    const path = resolve(this.cwd, arg);
    const place = this.host.place();
    if (place.flags.fixed && path.join('/') === 'src/returns/address.rs') return [path, ADDRESS_FIXED_RS];
    const node = lookup(place.tree, path);
    return [path, typeof node === 'string' ? node : undefined];
  }

  private cat(arg: string | undefined): void {
    const [, text] = this.file(arg);
    if (text === undefined) return this.print(`cat: ${arg ?? ''}: No such file or directory`);
    text.replace(/\n$/, '').split('\n').forEach((l) => this.print(l));
  }

  private edit(arg: string | undefined): void {
    const [path, text] = this.file(arg);
    this.start(new Editor(this.host, () => this.finish(), path.join('/') || '[No Name]', text ?? ''));
  }

  private git(args: string[]): void {
    const place = this.host.place();
    if (!place.repo) return this.print('fatal: not a git repository (or any of the parent directories): .git');
    const branch = place.branch ?? 'main';
    const sub = args[0];
    if (sub === 'status') {
      this.print(`On branch ${branch}`);
      if (place.flags.fixed) {
        this.print('Changes not staged for commit:');
        this.print([seg('\tmodified:   src/returns/address.rs', RED)]);
        this.print([seg('\tmodified:   src/returns/mod.rs', RED)]);
      } else this.print('nothing to commit, working tree clean');
    } else if (sub === 'log') {
      const commits = COMMITS[place.project] ?? COMMITS['web-shop'];
      commits.forEach(([hash, msg], i) => {
        const deco: Line = i === 0 ? [seg(' ('), seg('HEAD -> ', { fg: 6, add: BOLD }), seg(branch, { fg: 2, add: BOLD }), seg(')', YELLOW)] : [];
        this.print([seg('* ', RED), seg(hash, YELLOW), ...deco, seg(` ${msg}`)]);
      });
    } else if (sub === 'branch') {
      for (const b of [...new Set(['main', branch, 'feat/dark-mode'])].sort()) {
        this.print(b === branch ? [seg('* '), seg(b, GREEN)] : `  ${b}`);
      }
    } else if (sub === 'diff') {
      if (!place.flags.fixed) return;
      const diff: Line[] = [
        [seg('diff --git a/src/returns/address.rs b/src/returns/address.rs', { add: BOLD })],
        [seg('@@ -11,6 +11,6 @@ impl Address {', CYAN)],
        [seg('-    pub fn first_line(&self) -> &str {', RED)],
        [seg('-        self.lines.first().unwrap()', RED)],
        [seg('+    pub fn first_line(&self) -> Option<&str> {', GREEN)],
        [seg('+        self.lines.first().map(String::as_str)', GREEN)],
      ];
      if (args.includes('--stat')) {
        this.print([seg(' src/returns/address.rs | 4 '), seg('++', GREEN), seg('--', RED)]);
        this.print([seg(' src/returns/mod.rs     | 9 '), seg('+++++++', GREEN), seg('--', RED)]);
        this.print(' 2 files changed, 9 insertions(+), 4 deletions(-)');
      } else diff.forEach((l) => this.print(l));
    } else if (sub === 'worktree') {
      this.print(`${place.root.padEnd(44)} b1b91ec [${branch}]`);
    } else this.print(`git: '${sub ?? ''}' is not a git command in this demo. Try status, log, branch or diff.`);
  }

  private cargo(args: string[]): void {
    const place = this.host.place();
    if (!('Cargo.toml' in place.tree)) {
      return this.print([seg('error', { fg: 1, add: BOLD }), seg(`: could not find \`Cargo.toml\` in \`${place.root}\` or any parent directory`)]);
    }
    const head = (s: string) => seg(s.padStart(12), { fg: 2, add: BOLD });
    if (args[0] === 'run') {
      this.forever(
        'cargo',
        [
          [head('Compiling'), seg(' web-shop v0.5.0')],
          [head('Finished'), seg(' `dev` profile in 3.02s')],
          [head('Running'), seg(' `target/debug/web-shop`')],
          [seg('listening on 0.0.0.0:3000')],
        ],
        () => request(['/returns', '/checkout', '/orders/1042']),
        () => 1400 + Math.random() * 2600,
      );
      return;
    }
    const fixed = place.flags.fixed;
    const tests = ['checkout::totals_include_tax', 'checkout::coupons_stack', 'returns::label_is_printed', 'theme::dark_background_is_dark', 'theme::light_is_the_default', 'returns::postcode_is_trimmed'];
    const steps: Step[] = [
      [120, [head('Compiling'), seg(' web-shop v0.5.0')]],
      [900, [head('Finished'), seg(' test profile in 2.41s')]],
      [80, [head('Running'), seg(' unittests src/main.rs')]],
      [60, []],
      [60, [seg('running 7 tests')]],
      ...tests.map((t): Step => [90, [seg(`test ${t} ... `), seg('ok', GREEN)]]),
      [140, [seg('test returns::empty_address ... '), fixed ? seg('ok', GREEN) : seg('FAILED', RED)]],
      [60, []],
    ];
    if (fixed) steps.push([60, [seg('test result: '), seg('ok', GREEN), seg('. 7 passed; 0 failed')]]);
    else {
      steps.push(
        [40, [seg('failures:')]],
        [40, []],
        [40, [seg('---- returns::empty_address stdout ----')]],
        [40, [seg('panicked at src/returns/address.rs:12:33:')]],
        [40, [seg('called `Option::unwrap()` on a `None` value')]],
        [40, []],
        [60, [seg('test result: '), seg('FAILED', RED), seg('. 6 passed; 1 failed')]],
      );
    }
    this.stream('cargo', steps);
  }

  private node(): void {
    const place = this.host.place();
    if (!('package.json' in place.tree)) return this.print("npm error code ENOENT\nnpm error path package.json");
    this.forever(
      'node',
      [plain(`> ${place.project}@2.1.0 dev`), plain('> node src/server.ts'), [], [seg(`${place.project} listening on `), seg(':8080', CYAN)]],
      () => request(['/v1/orders?page=2', '/v1/returns', '/v1/orders/9f2c', '/v1/health', '/v1/customers/88/orders']),
      () => 900 + Math.random() * 2400,
    );
  }

  private cornercase(args: string[]): void {
    if (args[0] === 'kill-server') {
      this.print([seg('# in the real app this stops the server and every shell in it', DIMMED)]);
      return;
    }
    this.print([seg('Error: ', { fg: 1, add: BOLD }), seg('cornercase is already running in this terminal; nesting it is not supported')]);
  }
}

function request(paths: string[]): Line {
  const post = Math.random() < 0.3;
  const path = paths[Math.floor(Math.random() * paths.length)];
  const roll = Math.random();
  const status = post ? (roll < 0.9 ? 201 : 422) : roll < 0.85 ? 200 : roll < 0.95 ? 404 : 500;
  const color = status < 300 ? GREEN : status < 500 ? YELLOW : RED;
  const ms = Math.round(2 + Math.random() * (post ? 60 : 30));
  const clock = new Date().toTimeString().slice(0, 5);
  return [seg(`${clock} `, DIMMED), seg(pad(post ? 'POST' : 'GET', 5), post ? MAGENTA : CYAN), seg(pad(path, 24)), seg(String(status), color), seg(`${String(ms).padStart(4)}ms`, DIMMED)];
}

export class Editor implements Program {
  readonly name = 'nvim';
  readonly mouse = true;
  private lines: string[];
  private row = 0;
  private col = 0;
  private top = 0;
  private mode: 'normal' | 'insert' | 'command' = 'normal';
  private cmd = '';
  private message = '';
  private modified = false;
  private lang: string;

  constructor(
    private host: Host,
    private done: () => void,
    private path: string,
    text: string,
    line = 0,
  ) {
    this.lines = text.replace(/\n$/, '').split('\n');
    this.lang = language(path);
    this.row = Math.min(line, this.lines.length - 1);
    this.message = text ? `"${path}" ${this.lines.length}L, ${text.length}B` : `"${path}" [New]`;
  }

  draw(g: Grid, r: Rect): Cursor | null {
    const body = r.h - 2;
    if (this.row < this.top) this.top = this.row;
    if (this.row >= this.top + body) this.top = this.row - body + 1;
    const gutter = Math.max(3, String(this.lines.length).length) + 1;
    for (let i = 0; i < body; i++) {
      const n = this.top + i;
      const y = r.y + i;
      if (n >= this.lines.length) {
        g.put(r.x, y, '~', { fg: 4 });
        continue;
      }
      const current = n === this.row;
      if (current) g.fill({ x: r.x, y, w: r.w, h: 1 }, { bg: 236 });
      g.text(r.x, y, String(n + 1).padStart(gutter - 1), current ? { fg: 3, add: BOLD } : { fg: 8 });
      drawLine(g, r.x + gutter + 1, y, highlight(this.lines[n], this.lang), r.w - gutter - 1);
    }
    const status = r.y + r.h - 2;
    g.fill({ x: r.x, y: status, w: r.w, h: 1 }, { bg: 236, fg: 15 }, ' ');
    const label = ` ${this.mode === 'insert' ? 'INSERT' : this.mode === 'command' ? 'COMMAND' : 'NORMAL'} `;
    g.text(r.x, status, label, { fg: 0, bg: this.mode === 'insert' ? 2 : 4, add: BOLD });
    g.text(r.x + label.length + 1, status, truncateRight(`${this.path}${this.modified ? ' [+]' : ''}`, r.w - label.length - 16), { bg: 236, fg: 15 });
    const pos = `${this.row + 1},${this.col + 1}   All `;
    g.text(r.x + r.w - pos.length, status, pos, { bg: 236, fg: 7 });
    const last = r.y + r.h - 1;
    if (this.mode === 'command') {
      g.text(r.x, last, `:${this.cmd}`, {});
      return { x: r.x + 1 + this.cmd.length, y: last, shape: 'block' };
    }
    if (this.mode === 'insert') g.text(r.x, last, '-- INSERT --', { add: BOLD });
    else g.text(r.x, last, truncateRight(this.message, r.w), {});
    const line = this.lines[this.row] ?? '';
    const x = r.x + gutter + 1 + Math.min(this.col, Math.max(0, line.length - (this.mode === 'insert' ? 0 : 1)));
    return { x: Math.min(x, r.x + r.w - 1), y: r.y + this.row - this.top, shape: this.mode === 'insert' ? 'bar' : 'block' };
  }

  screen(): string {
    return this.lines.join('\n');
  }

  click(x: number, y: number): void {
    const gutter = Math.max(3, String(this.lines.length).length) + 1;
    this.row = Math.max(0, Math.min(this.lines.length - 1, this.top + y));
    this.col = Math.max(0, x - gutter - 1);
    this.host.dirty();
  }

  paste(text: string): void {
    if (this.mode !== 'insert') return;
    for (const ch of text) this.key({ key: ch === '\n' ? 'Enter' : ch });
  }

  key(k: Key): void {
    const line = this.lines[this.row] ?? '';
    if (this.mode === 'command') {
      if (k.key === 'Escape') this.mode = 'normal';
      else if (k.key === 'Backspace') this.cmd ? (this.cmd = this.cmd.slice(0, -1)) : (this.mode = 'normal');
      else if (k.key === 'Enter') this.command();
      else if (k.key.length === 1) this.cmd += k.key;
      this.host.dirty();
      return;
    }
    if (this.mode === 'insert') {
      if (k.key === 'Escape' || (k.ctrl && k.key === 'c')) {
        this.mode = 'normal';
        this.col = Math.max(0, this.col - 1);
      } else if (k.key === 'Enter') {
        this.lines.splice(this.row + 1, 0, line.slice(this.col));
        this.lines[this.row] = line.slice(0, this.col);
        this.row += 1;
        this.col = 0;
        this.modified = true;
      } else if (k.key === 'Backspace') {
        if (this.col > 0) {
          this.lines[this.row] = line.slice(0, this.col - 1) + line.slice(this.col);
          this.col -= 1;
          this.modified = true;
        }
      } else if (k.key.length === 1 && !k.ctrl) {
        this.lines[this.row] = line.slice(0, this.col) + k.key + line.slice(this.col);
        this.col += 1;
        this.modified = true;
      } else this.move(k.key);
      this.host.dirty();
      return;
    }
    if (k.key === ':') {
      this.mode = 'command';
      this.cmd = '';
    } else if (k.key === 'i' || k.key === 'a') {
      this.mode = 'insert';
      if (k.key === 'a') this.col = Math.min(this.col + 1, line.length);
      this.message = '';
    } else if (k.key === 'G') this.row = this.lines.length - 1;
    else if (k.key === 'g') this.row = 0;
    else if (k.key === '0') this.col = 0;
    else if (k.key === '$') this.col = Math.max(0, line.length - 1);
    else if (k.key === 'x' && line.length) {
      this.lines[this.row] = line.slice(0, this.col) + line.slice(this.col + 1);
      this.modified = true;
    } else this.move(k.key);
    this.host.dirty();
  }

  private move(key: string): void {
    const map: Record<string, [number, number]> = {
      h: [0, -1], ArrowLeft: [0, -1], l: [0, 1], ArrowRight: [0, 1],
      j: [1, 0], ArrowDown: [1, 0], k: [-1, 0], ArrowUp: [-1, 0],
    };
    const d = map[key];
    if (!d) return;
    this.row = Math.max(0, Math.min(this.lines.length - 1, this.row + d[0]));
    this.col = Math.max(0, Math.min((this.lines[this.row] ?? '').length, this.col + d[1]));
  }

  private command(): void {
    const c = this.cmd.trim();
    this.mode = 'normal';
    if (c === 'q' && this.modified) {
      this.message = 'E37: No write since last change (add ! to override)';
      return;
    }
    if (['q', 'q!', 'wq', 'x', 'qa', 'qa!', 'wqa'].includes(c)) {
      this.done();
      return;
    }
    if (c === 'w') {
      this.modified = false;
      this.message = `"${this.path}" ${this.lines.length}L written`;
      return;
    }
    if (/^\d+$/.test(c)) {
      this.row = Math.max(0, Math.min(this.lines.length - 1, Number(c) - 1));
      return;
    }
    this.message = `E492: Not an editor command: ${c}`;
  }
}

export interface Tool {
  name: string;
  arg: string;
  note: string;
}

export interface Task {
  tools: Tool[];
  reply: string[];
  took: string;
  onStep?: (i: number) => void;
  onDone?: () => void;
}

interface Run {
  task: Task;
  next: number;
  pace: number;
}

interface Entry {
  line: Line;
  indent: number;
}

interface Look {
  bullet: string;
  result: string;
  spinner: string;
  mark: string;
  working: string;
  worked: string;
}

const LOOKS: Record<string, Look> = {
  claude: { bullet: '●', result: '⎿', spinner: '·✢✳✶✻✽', mark: '✻', working: 'Churning', worked: 'Churned' },
  other: { bullet: '•', result: '└', spinner: '⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏', mark: '•', working: 'Working', worked: 'Worked' },
};

const tool = (name: string, arg: string, note: string): Tool => ({ name, arg, note });

export function taskFor(prompt: string, place: Place): Task {
  const p = prompt.toLowerCase();
  if (p.includes('482') || p.includes('address')) {
    return {
      tools: [
        tool('Read', 'src/returns/address.rs', 'Read 14 lines'),
        tool('Read', 'src/returns/mod.rs', 'Read 12 lines'),
        tool('Update', 'src/returns/address.rs', 'Updated src/returns/address.rs with 2 additions and 2 removals'),
        tool('Update', 'src/returns/mod.rs', 'Updated src/returns/mod.rs with 4 additions and 3 removals'),
        tool('Write', 'src/returns/tests.rs', 'Wrote 7 lines to src/returns/tests.rs'),
        tool('Bash', 'cargo test', '7 passed, 0 failed'),
      ],
      reply: [
        '**Fixed.** `first_line()` returns `Option<&str>` now, and `create()` answers 422 instead of panicking when the address has no lines. The new test `empty_address_is_rejected` passes with the other seven.',
        'Review the diff in the changes panel, or run `git diff` in this worktree.',
      ],
      took: '1m 12s',
      onStep: (i) => {
        if (i >= 2) place.flags.fixed = true;
      },
      onDone: () => {
        place.flags.fixed = true;
      },
    };
  }
  if (p.includes('gift')) {
    return {
      tools: [
        tool('Read', 'src/checkout.rs', 'Read 5 lines'),
        tool('Write', 'src/gift_cards.rs', 'Wrote 24 lines to src/gift_cards.rs'),
        tool('Update', 'src/checkout.rs', 'Updated src/checkout.rs with 18 additions and 2 removals'),
        tool('Bash', 'cargo test', '9 passed, 0 failed'),
      ],
      reply: ['**Done.** Gift cards work in the checkout: a `GiftCard` with a balance is applied after coupons and before tax, and a card that runs out keeps its remainder for the next order.'],
      took: '58s',
    };
  }
  if (p.includes('pagina') || p.includes('cursor')) {
    return {
      tools: [
        tool('Read', 'src/orders.ts', 'Read 1 line'),
        tool('Read', 'src/server.ts', 'Read 9 lines'),
        tool('Update', 'src/orders.ts', 'Updated src/orders.ts with 21 additions'),
        tool('Update', 'README.md', 'Updated README.md with 6 additions'),
        tool('Bash', 'npm test', '31 passed'),
      ],
      reply: ['**Done.** `GET /orders` takes a `cursor` and a `limit` of at most 50, and answers the next cursor with the page. The README documents both.'],
      took: '44s',
    };
  }
  if (p.includes('colour')) {
    return {
      tools: [tool('Read', 'src/checkout.rs', 'Read 5 lines'), tool('Read', 'src/theme.rs', 'Read 14 lines')],
      reply: [
        'Nothing reads `Theme` yet. `src/theme.rs:7` defines `background()` with both colours, but `src/checkout.rs` builds the page without it: the templates hardcode `#ffffff` three times.',
        'To use it everywhere I would:',
        '- call `Theme::background()` from the checkout',
        '- keep the brand purple on both themes',
        '- add a test for the dark background',
      ],
      took: '31s',
    };
  }
  if (p.includes('dark') || p.includes('479') || p.includes('sc-48') || p.includes('theme')) {
    return {
      tools: [
        tool('Update', 'src/checkout.rs', 'Updated src/checkout.rs with 3 additions and 3 removals'),
        tool('Update', 'src/theme.rs', 'Updated src/theme.rs with 4 additions'),
        tool('Bash', 'cargo test', '7 passed, 0 failed'),
        tool('Bash', 'cargo clippy --all-targets', '0 warnings'),
      ],
      reply: ['**Done.** The checkout reads `Theme::background()` on every page, the brand purple stays the same on both themes, and `from_system()` picks the theme from `prefers-color-scheme`.'],
      took: '1m 4s',
      onDone: () => {
        place.flags.dark = true;
      },
    };
  }
  if (p.includes('typo')) {
    return {
      tools: [
        tool('Read', 'src/content/posts/worktrees.md', 'Read 6 lines'),
        tool('Update', 'src/content/posts/worktrees.md', 'Updated worktrees.md with 2 additions and 2 removals'),
        tool('Bash', 'npm run build', 'Built 1 page in 1.2s'),
      ],
      reply: ['**Done.** Two typos fixed in the post on worktrees, and the site still builds.'],
      took: '19s',
    };
  }
  return {
    tools: [
      tool('Read', 'README.md', 'Read 7 lines'),
      tool('Grep', '"TODO"', 'Found 3 matches'),
      tool('Update', 'README.md', 'Updated README.md with 2 additions and 2 removals'),
      tool('Bash', 'git status --short', '1 file changed'),
    ],
    reply: ['**Done.** The change is in this workspace; review it in the changes panel.'],
    took: '52s',
  };
}

const INLINE = /\*\*[^*]+\*\*|`[^`]+`/g;

export function inline(text: string, base: Style = {}): Line {
  const line: Line = [];
  let last = 0;
  for (const m of text.matchAll(INLINE)) {
    const at = m.index ?? 0;
    if (at > last) line.push(seg(text.slice(last, at), base));
    const t = m[0];
    line.push(t.startsWith('**') ? seg(t.slice(2, -2), { ...base, add: (base.add ?? 0) | BOLD }) : seg(t.slice(1, -1), { ...base, fg: 4 }));
    last = at + t.length;
  }
  if (last < text.length) line.push(seg(text.slice(last), base));
  return line;
}

function seconds(took: string): number {
  let n = 0;
  for (const [, v, unit] of took.matchAll(/(\d+)([ms])/g)) n += Number(v) * (unit === 'm' ? 60 : 1);
  return n;
}

function elapsedText(s: number): string {
  return s < 60 ? `${s}s` : `${Math.floor(s / 60)}m ${s % 60}s`;
}

function clockText(minutes: number): string {
  const h = Math.floor(minutes / 60) % 24;
  return `${((h + 11) % 12) + 1}:${String(minutes % 60).padStart(2, '0')} ${h < 12 ? 'AM' : 'PM'}`;
}

const AGENT_MODEL = 'Opus 5.5';
const AGENT_WINDOW = 1_000_000;
const MB = 1 << 20;
const CLAUDE_MEMORY = 412 * MB;
const CODEX_MEMORY = 168 * MB;
const SHELL_MEMORY = 6 * MB;
const PROGRAM_MEMORY = 184 * MB;
const MEMORY_PER_TOKEN = 4_000;
const PROMPT_TOKENS = 38_000;
const REPLY_TOKENS = 9_000;
const TICK = 110;
const CLOCK_START = 9 * 60 + 38;

export class Agent implements Program {
  readonly mouse = false;
  private phase: 'boot' | 'trust' | 'ready' | 'working' | 'asking' = 'boot';
  private choice = 0;
  private input = '';
  private log: Entry[] = [];
  private frame = 0;
  private ticks = 0;
  private timers: (() => void)[] = [];
  private spinner: (() => void) | null = null;
  private tokens = 0;
  private run: Run | null = null;
  private step: (() => void) | null = null;
  private running: { entry: Entry; note: string } | null = null;
  private question = '';
  private clock = CLOCK_START;
  private readonly look: Look;

  constructor(
    private host: Host,
    private done: () => void,
    readonly name: string,
    private args: string[],
    opts: { trusted?: boolean; working?: string; shown?: number; pace?: number; history?: string[] } = {},
  ) {
    this.look = LOOKS[name] ?? LOOKS.other;
    for (const prompt of opts.history ?? []) this.recall(prompt);
    if (opts.working) {
      this.phase = 'ready';
      this.submit(opts.working, opts.pace ?? 2600, opts.shown ?? 0);
    } else if (opts.trusted) this.phase = 'ready';
    else this.timers.push(host.after(450, () => this.show('trust')));
  }

  private show(phase: 'trust' | 'ready'): void {
    this.phase = phase;
    this.host.dirty();
  }

  dispose(): void {
    this.timers.forEach((t) => t());
    this.step?.();
    this.stopSpin();
  }

  get trusting(): boolean {
    return this.phase === 'trust';
  }

  get ready(): boolean {
    return this.phase === 'ready';
  }

  get working(): boolean {
    return this.phase === 'working';
  }

  get waiting(): boolean {
    return this.phase === 'asking';
  }

  get pending(): string {
    return this.input;
  }

  get context(): Context | null {
    if (this.name === 'codex') {
      return { model: 'gpt-5.4', percent: this.tokens ? Math.min(100, Math.round((this.tokens / 272_000) * 100)) : null };
    }
    return this.name === 'claude' && this.tokens
      ? { model: AGENT_MODEL, percent: Math.min(100, Math.round((this.tokens / AGENT_WINDOW) * 100)) }
      : null;
  }

  get memory(): number {
    return (this.name === 'codex' ? CODEX_MEMORY : CLAUDE_MEMORY) + this.tokens * MEMORY_PER_TOKEN;
  }

  private reply(): void {
    this.tokens = (this.tokens || PROMPT_TOKENS) + REPLY_TOKENS;
  }

  screen(): string {
    if (this.phase === 'trust') return 'Do you trust the files in this folder?\n❯ 1. Yes, proceed\n  2. No, exit';
    if (this.phase === 'asking') return `Do you want to make this edit to ${this.question}?`;
    return '> ';
  }

  private choices(lines: Line[], options: string[]): void {
    options.forEach((o, i) => lines.push(this.choice === i ? [seg(`❯ ${i + 1}. ${o}`, CYAN)] : plain(`  ${i + 1}. ${o}`)));
  }

  private add(line: Line, indent = 0): Entry {
    const entry = { line, indent };
    this.log.push(entry);
    return entry;
  }

  private note(text: string, style: Style = DIMMED): Line {
    return [seg(`  ${this.look.result}  `, style), seg(text, style)];
  }

  private settle(): void {
    if (this.running) this.running.entry.line = this.note(this.running.note);
    this.running = null;
  }

  private asked(prompt: string): Task {
    this.add([seg('> ', DIMMED), seg(prompt)], 2);
    this.add([]);
    return taskFor(prompt, this.host.place());
  }

  private call(name: string, arg: string, note: string): void {
    this.settle();
    this.add([seg(`${this.look.bullet} `), seg(name, { add: BOLD }), seg(`(${arg})`)], 2);
    this.running = { entry: this.add(this.note('Running…'), 5), note };
    this.add([]);
    this.reply();
  }

  private said(task: Task): void {
    this.settle();
    let after: 'text' | 'bullet' | null = null;
    task.reply.forEach((p, i) => {
      const bullet = p.startsWith('- ');
      if (after && !(after === 'bullet' && bullet)) this.add([]);
      if (bullet) this.add([seg('  - '), ...inline(p.slice(2))], 4);
      else this.add([seg(i === 0 ? `${this.look.bullet} ` : '  '), ...inline(p)], 2);
      after = bullet ? 'bullet' : 'text';
    });
    this.add([]);
    this.clock += Math.max(1, Math.ceil(seconds(task.took) / 60));
    this.add([seg(`${this.look.mark} ${this.look.worked} for ${task.took} · done ${clockText(this.clock)}`, DIMMED)], 2);
    this.add([]);
    this.reply();
  }

  private recall(prompt: string): void {
    const task = this.asked(prompt);
    for (const t of task.tools) this.call(t.name, t.arg, t.note);
    this.said(task);
  }

  private welcome(inner: number): Line[] {
    const place = this.host.place();
    const boxed = (line: Line): Line => [seg('│', MAGENTA), ...line, seg(' '.repeat(Math.max(0, inner - width(line)))), seg('│', MAGENTA)];
    return [
      [seg(`╭${'─'.repeat(inner)}╮`, MAGENTA)],
      boxed([seg(' ✻ ', MAGENTA), seg(truncateRight('Welcome to Claude Code!', Math.max(0, inner - 3)), { add: BOLD })]),
      boxed([]),
      boxed([seg(truncateRight('   /help for help, /status for your status', inner), DIMMED)]),
      boxed([seg(truncateRight(`   cwd: ${place.root}`, inner), DIMMED)]),
      [seg(`╰${'─'.repeat(inner)}╯`, MAGENTA)],
    ];
  }

  private banner(inner: number): Line[] {
    const command = [this.name, ...this.args].join(' ');
    const suffix = [` · ${command}`, ` · ${this.name}`, ''].find((s) => s.length <= inner - 14) ?? '';
    return [
      [seg(`╭${'─'.repeat(inner)}╮`, MAGENTA)],
      [seg('│', MAGENTA), seg(' agent session', { add: BOLD }), seg(suffix.padEnd(Math.max(0, inner - 14)), DIMMED), seg('│', MAGENTA)],
      [seg(`╰${'─'.repeat(inner)}╯`, MAGENTA)],
    ];
  }

  private modeLine(): string {
    if (this.name !== 'claude') return '  ? for shortcuts';
    if (this.args.includes('acceptEdits')) return '  ⏵⏵ accept edits on (shift+tab to cycle)';
    if (this.args.includes('plan')) return '  ⏸ plan mode on (shift+tab to cycle)';
    return '  ? for shortcuts';
  }

  draw(g: Grid, r: Rect): Cursor | null {
    const inner = Math.max(0, Math.min(r.w, 62) - 2);
    const lines: Line[] = this.name === 'claude' ? this.welcome(inner) : this.banner(inner);
    lines.push([]);
    if (this.phase === 'boot') lines.push([seg('starting…', DIMMED)]);
    if (this.phase === 'trust') {
      const place = this.host.place();
      lines.push(plain('Do you trust the files in this folder?'), [seg(place.root, DIMMED)], []);
      this.choices(lines, ['Yes, proceed', 'No, exit']);
    }
    const rows = wrapAll(lines, r.w);
    if (this.phase === 'ready' || this.phase === 'working' || this.phase === 'asking') {
      if (!this.log.length) rows.push(...wrapAll([[seg('Tip: describe a change or paste a link', DIMMED)], []], r.w));
      for (const e of this.log) rows.push(...wrapWords(e.line, r.w, e.indent));
    }
    if (this.phase === 'working') {
      const spin = this.look.spinner[this.frame % this.look.spinner.length];
      rows.push(...wrapWords([seg(`${spin} ${this.look.working}… `, MAGENTA), seg(`(esc to interrupt · ${elapsedText(Math.floor((this.ticks * TICK) / 1000))})`, DIMMED)], r.w, 2), []);
    }
    if (this.phase === 'asking') {
      const asking: Line[] = [[], [seg('● ', YELLOW), seg('Edit ', { add: BOLD }), seg(this.question)], [seg('  Do you want to make this edit?', { add: BOLD })]];
      this.choices(asking, ['Yes', 'Yes, and don’t ask again this session', 'No, and tell Claude what to do']);
      rows.push(...wrapAll(asking, r.w));
    }
    let cursor: Cursor | null = null;
    let prompt: [number, number] | null = null;
    if (this.phase === 'ready' || this.phase === 'working') {
      rows.push(...wrapAll([[seg('─'.repeat(r.w), DIMMED)]], r.w));
      const text = this.phase === 'ready' ? this.input : '';
      prompt = [rows.length, Math.min(2 + [...text].length, r.w - 1)];
      rows.push(...wrapAll([[seg('› '), seg(text)]], r.w));
      rows.push(...wrapAll([[seg(this.modeLine(), DIMMED)]], r.w));
    }
    const top = Math.max(0, rows.length - r.h);
    paintRows(g, r, rows, top);
    if (this.phase === 'ready' && prompt && prompt[0] >= top) cursor = { x: r.x + prompt[1], y: r.y + prompt[0] - top, shape: 'block' };
    return cursor;
  }

  paste(text: string): void {
    if (this.phase !== 'ready') return;
    this.input += text.replace(/\s*\n\s*/g, ' ');
    this.host.dirty();
  }

  key(k: Key): void {
    if (this.phase === 'trust') {
      if (k.key === 'ArrowDown' || k.key === 'ArrowUp') this.choice = 1 - this.choice;
      else if (k.key === 'Enter') {
        if (this.choice === 1) return this.done();
        this.show('ready');
      } else if (k.key === '1') this.show('ready');
      else if (k.key === '2' || (k.ctrl && k.key === 'c')) return this.done();
      this.host.dirty();
      return;
    }
    if (this.phase === 'asking') {
      if (k.key === 'ArrowDown') this.choice = Math.min(2, this.choice + 1);
      else if (k.key === 'ArrowUp') this.choice = Math.max(0, this.choice - 1);
      else if (k.key === '1' || k.key === '2' || (k.key === 'Enter' && this.choice < 2)) this.answer(true);
      else if (k.key === '3' || k.key === 'Escape' || k.key === 'Enter' || (k.ctrl && k.key === 'c')) this.answer(false);
      this.host.dirty();
      return;
    }
    if (this.phase === 'working') {
      if (k.key === 'Escape' || (k.ctrl && k.key === 'c')) this.interrupt();
      return;
    }
    if (this.phase !== 'ready') return;
    if (k.ctrl && (k.key === 'c' || k.key === 'd')) {
      if (this.input) this.input = '';
      else return this.done();
    } else if (k.key === 'Enter') {
      if (this.input.trim()) {
        const prompt = this.input.trim();
        this.input = '';
        this.submit(prompt, 0);
      }
    } else if (k.key === 'Backspace') this.input = this.input.slice(0, -1);
    else if (k.key.length === 1 && !k.ctrl && !k.alt) this.input += k.key;
    this.host.dirty();
  }

  setPace(pace: number): void {
    if (this.run) this.run.pace = pace;
  }

  ask(file: string): boolean {
    if (this.phase !== 'working') return false;
    this.step?.();
    this.step = null;
    this.stopSpin();
    this.question = file;
    this.choice = 0;
    this.phase = 'asking';
    this.host.dirty();
    return true;
  }

  private answer(yes: boolean): void {
    if (!yes) {
      this.settle();
      this.add(this.note(`You declined the edit to ${this.question}`, RED), 5);
      this.add([]);
      this.run = null;
      this.phase = 'ready';
      return;
    }
    this.call('Update', this.question, `Updated ${this.question}`);
    this.phase = 'working';
    this.spin();
    this.schedule();
  }

  private interrupt(): void {
    this.step?.();
    this.step = null;
    this.run = null;
    this.stopSpin();
    const text = this.note(`Interrupted · What should ${this.name === 'claude' ? 'Claude' : this.name} do instead?`, RED);
    if (this.running) this.running.entry.line = text;
    else this.add(text, 5);
    this.running = null;
    this.phase = 'ready';
    this.host.dirty();
  }

  submit(prompt: string, pace: number, shown = 0): void {
    const task = this.asked(prompt);
    this.phase = 'working';
    this.ticks = 0;
    this.spin();
    for (const t of task.tools.slice(0, shown)) this.call(t.name, t.arg, t.note);
    this.run = { task, next: shown, pace: pace || this.host.pace() };
    this.schedule();
  }

  private schedule(): void {
    const run = this.run;
    if (!run) return;
    const { task } = run;
    if (run.next < task.tools.length) {
      this.step = this.host.after(run.pace + Math.random() * 700, () => {
        const i = run.next++;
        const t = task.tools[i];
        this.call(t.name, t.arg, t.note);
        task.onStep?.(i);
        this.host.dirty();
        this.schedule();
      });
      return;
    }
    this.step = this.host.after(900, () => {
      this.step = null;
      this.run = null;
      this.said(task);
      this.phase = 'ready';
      this.stopSpin();
      task.onDone?.();
      this.host.dirty();
    });
  }

  private spin(): void {
    if (this.spinner) return;
    const tick = () => {
      this.frame += 1;
      this.ticks += 1;
      this.host.dirty();
      this.spinner = this.host.after(TICK, tick);
    };
    tick();
  }

  private stopSpin(): void {
    this.spinner?.();
    this.spinner = null;
  }
}

export class Top implements Program {
  readonly mouse = false;
  private cpus = [38, 12, 64, 21];
  private timer: () => void = () => {};

  constructor(
    private host: Host,
    private done: () => void,
    readonly name: string,
  ) {
    this.tick();
  }

  private tick(): void {
    this.cpus = this.cpus.map((c) => Math.max(2, Math.min(98, c + (Math.random() - 0.5) * 30)));
    this.host.dirty();
    this.timer = this.host.after(900, () => this.tick());
  }

  dispose(): void {
    this.timer();
  }

  screen(): string {
    return 'top';
  }

  paste(): void {}

  key(k: Key): void {
    if (k.key === 'q' || k.key === 'Escape' || (k.ctrl && k.key === 'c') || k.key === 'F10') this.done();
  }

  draw(g: Grid, r: Rect): Cursor | null {
    const barWidth = Math.max(8, Math.floor((r.w - 10) / 2) - 8);
    this.cpus.forEach((c, i) => {
      const x = r.x + (i % 2) * Math.floor(r.w / 2);
      const y = r.y + Math.floor(i / 2);
      g.text(x, y, `${String(i).padStart(3)}[`, { fg: 6 });
      const fill = Math.round((c / 100) * barWidth);
      for (let b = 0; b < barWidth; b++) {
        if (b < fill) g.put(x + 4 + b, y, '|', { fg: b < fill * 0.6 ? 2 : b < fill * 0.85 ? 3 : 1 });
      }
      g.text(x + 4 + barWidth, y, `${c.toFixed(1).padStart(5)}%]`, { fg: 8 });
    });
    const mem = r.y + 2;
    g.text(r.x, mem, '  Mem[', { fg: 6 });
    const used = Math.round(barWidth * 0.42);
    for (let b = 0; b < used; b++) g.put(r.x + 6 + b, mem, '|', { fg: 2 });
    g.text(r.x + 6 + barWidth, mem, ' 6.71G/16.0G]', { fg: 8 });
    g.text(r.x + Math.floor(r.w / 2), mem, 'Tasks: 142, 418 thr; 3 running', {});
    const header = r.y + 4;
    g.fill({ x: r.x, y: header, w: r.w, h: 1 }, { bg: 2, fg: 0 }, ' ');
    g.text(r.x, header, '    PID USER      CPU% MEM%  Command', { bg: 2, fg: 0, add: BOLD });
    const rows: [number, number, number, string][] = [
      [4121, this.cpus[2] * 0.7, 2.1, 'claude --permission-mode plan'],
      [4188, this.cpus[0] * 0.5, 1.4, 'cargo test'],
      [3302, this.cpus[1] * 0.4, 0.9, 'node src/server.ts'],
      [2210, 1.3, 0.6, 'cornercase server'],
      [4012, 0.4, 0.3, 'nvim src/returns/address.rs'],
      [3990, 0.0, 0.1, 'bash'],
      [3991, 0.0, 0.1, 'bash'],
    ];
    rows.forEach(([pid, cpu, memory, command], i) => {
      const y = header + 1 + i;
      if (y >= r.y + r.h - 1) return;
      g.text(r.x, y, `${String(pid).padStart(7)} you      ${cpu.toFixed(1).padStart(5)} ${memory.toFixed(1).padStart(4)}  `, {});
      g.text(r.x + 33, y, truncateRight(command, r.w - 34), i === 0 ? { fg: 6, add: BOLD } : {});
    });
    g.text(r.x, r.y + r.h - 1, 'F10', { add: BOLD });
    g.text(r.x + 3, r.y + r.h - 1, 'Quit ', { bg: 6, fg: 0 });
    return null;
  }
}
