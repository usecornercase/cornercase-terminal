# cornercase

A terminal multiplexer TUI in Rust. A sidebar of **projects** (folders, optionally in **groups** with an icon and a colour), a column with the active project's **workspaces** (lines of work, optionally each in its own git worktree) and their **tabs** (one or more shells, split like Ghostty), the active tab's panes, and an optional **changes** panel on the right with the workspace's git diff. Everything is driven by mouse buttons. An issues modal (GitHub, Shortcut, Linear, Jira) reads an issue and starts a coding agent on it in its own worktree. A background server owns the shells, so closing the UI leaves them running and the next `cornercase` reattaches.

These rules apply to every contributor and coding agent, in `src/`, `tests/` and `site/`. The design decisions and their reasons, area by area, are in [DESIGN.md](DESIGN.md): read the section for the area you touch before changing it, and update it in the same change when a decision changes. How these files are loaded is under [Instruction files](#instruction-files).

## Commands

```sh
cargo run                                   # attach to the server (starts it if none is running)
cargo run -- --help                         # every command, including the ones for scripts and agents
cargo run -- kill-server                    # stop the server and every shell in it
cargo test --locked                         # unit + snapshot + e2e tests
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo fmt
cargo-machete                               # unused dependencies
npx -y jscpd@4.3.0                          # copy-paste detector, reads .jscpd.json
```

- **Building needs Zig 0.15.2 on the PATH**: `libghostty-vt-sys` runs `zig build` on Ghostty's sources, and Ghostty pins the Zig minor version. The first build per profile clones Ghostty (network, ~100 s, ~530 MB in `target/`); `GHOSTTY_SOURCE_DIR` can point at a local checkout.
- **The toolchain is pinned** in `rust-toolchain.toml` (version and components), and CI installs exactly that one, so a new Rust release cannot break CI with new lints. Bump it by hand: change the version, then fix what clippy reports.
- `.cargo/config.toml` sets `LIBGHOSTTY_VT_SYS_OPTIMIZE=ReleaseFast`; a Zig Debug build of Ghostty is too slow to use.
- **After rebuilding, run `cargo run -- kill-server`**: the client refuses to attach to a server from another build.
- Snapshots (`insta`): a changed render writes `src/snapshots/*.snap.new` and fails. Check it, then accept with `INSTA_UPDATE=always cargo test`. Delete snapshots of renamed or removed tests by hand.
- **Before calling a task done, run fmt, clippy, tests, `cargo-machete` and jscpd, and fix what fails.** If one cannot pass, say so.
- **A change that adds, removes or changes a feature updates the website in the same change** (see Website below).
- `jscpd` fails above 1 % duplication (50-token clones) in `src/` and `tests/`. It is a ratchet: extract the shared code, do not raise the threshold. `cargo-machete` is a text search; a false positive goes in `[package.metadata.cargo-machete] ignored`.
- **Every pull request that changes the app bumps `version` in `Cargo.toml` and adds a `## <version>` section to `CHANGELOG.md`** (a patch unless told otherwise; the section is written for users, it becomes the release notes and the update dialog's text). "The app" is `src/`, `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `.cargo/`; docs, tests and `site/` alone need no bump. CI checks it (`.github/version.sh check`, in the guardrails job). Main requires branches to be up to date, so two pull requests cannot ship the same version.
- **Commit messages and pull request titles follow [Conventional Commits](https://www.conventionalcommits.org/)**: `<type>: <summary>`, the summary in the imperative and in lower case, such as `feat: ask before closing a project`. Types: `feat` (a feature users see), `fix`, `docs`, `test`, `refactor`, `perf`, `ci`, `build`, `chore`; a breaking change adds `!` (`feat!: …`). The version bump is its own commit, `chore: release <version>`; Dependabot's are `chore(deps): …` (`commit-message` in `.github/dependabot.yml`). Both reach `main`: a squash merge takes the pull request's title (or its only commit's message), and a merge or a rebase keeps every commit.
- **Releases are automatic**: on a push to `main`, `.github/workflows/version.yml` runs `.github/version.sh release`, which tags `v<version>` if it is new and dispatches `.github/workflows/release.yml` (`dist`, `dispatch-releases`, because a tag pushed with `GITHUB_TOKEN` triggers no workflow). That builds the four targets, makes the GitHub Release (notes from `CHANGELOG.md`) with `cornercase-installer.sh`, and pushes the formula to `usecornercase/homebrew-tap` (secret `HOMEBREW_TAP_TOKEN`, a fine-grained token that expires). If the app changed but the version was already released, the job fails. `release.yml` is generated: change `dist-workspace.toml` or `.github/build-setup.yml` (the Zig step), then run `dist generate`; never edit it by hand. `site/public/install.sh` (served at `usecornercase.dev/install.sh`) runs the latest release's installer.
- **CI** (`.github/workflows/ci.yml`) runs the same checks: clippy + tests on Linux and macOS (one job per OS so clippy and tests share the Ghostty build; `target/` cached by `Swatinem/rust-cache`, saved from `main` only), and fmt + machete + jscpd (with the size of `AGENTS.md`) on Linux. Dependabot groups action and crate updates monthly; `libghostty-vt` is pinned with `=` (pre-1.0 API), so bumping it needs a manual check. TypeScript majors are ignored in `site/`: `@astrojs/check` 0.9.10 accepts only TypeScript 5 and 6 (7 is the Go port, without the JS API its language server uses); drop the `ignore` once it supports 7.
- **Code review**: CodeRabbit (free for public repos) reviews every non-draft pull request except Dependabot's; `.coderabbit.yaml` configures it. It reads `AGENTS.md` and `DESIGN.md` as its guidelines (`knowledge_base.code_guidelines`). Its docstring check and docstring/test generation are off (the code has no comments on purpose), and so is its clippy (it cannot build Ghostty without Zig; CI runs clippy).

## Code conventions

- Everything in English: identifiers, test names, messages, UI text.
- **No comments at all**, including `///` and `//!`. Rationale goes in `DESIGN.md`.
- Errors: `crate::error::Error` (`thiserror`) in the library; `anyhow` only in `src/main.rs`.
- No `unwrap()` outside tests (`clippy::unwrap_used`); in tests prefer `expect("what")`.
- Mutexes are `parking_lot::Mutex`.
- Lints live in `Cargo.toml` (clippy `all` deny, `pedantic` warn). Silence one only locally with `#[expect(clippy::x, reason = "...")]`.
- Imports in three groups: `std`, external crates, then `super`/`crate`.
- `rustfmt.toml`: `max_width = 120`.

## Interaction

- **Mouse first; the only app keys sit behind an opt-in prefix key.** Every key goes to the program in the active pane, except while a modal, the search, the changes panel's filter field, the files panel's search field or a TODO field has them (then `Enter` submits, `Esc` cancels; the files panel's search field also takes `↑` `↓` to pick a result; a TODO field also takes `←` `→` `↑` `↓` `Home` `End` `Delete` to move and edit, since editing an item means fixing a word in the middle), `Esc` while a row is dragged, and the prefix key (`prefix_key`, off by default, chosen by the user), which opens the keys menu (`shortcuts.rs`, see DESIGN.md). Do not add keyboard shortcuts outside that menu, nor a default prefix, without asking. No `Alt` shortcuts (Option is a compose key on macOS), no `Ctrl+letter` (steals shell bindings). `e2e::ctrl_b_reaches_the_shell` guards this.
- Closing the last project leaves the app open and empty. ` quit ` only detaches.
- `×` and ` ⋯ ` (the row's menu, as a right-click) buttons only show while hovering their row, except in compact mode, where they always show dimmed (touch screens have no hover) and closing a tab or a plain workspace asks first (`Overlay::CloseTab`, `Overlay::CloseWorkspace`), since an always-visible `×` is easy to tap by accident. Names are cut at the end (`ui::truncate_right`), paths at the start (`truncate_left`).
- At most one overlay is open (menu, form, confirmation, settings, usage, picker, issues, search, keys menu). While it is open, no mouse event reaches the columns or the pane.
- Overlays, hover, scroll, column widths and the toast live in `App` and are shared by every attached client. A toast can carry an ` undo ` (TODO removals), then lasts 6 s.

## Architecture

```
src/main.rs       hands argv to cli.rs (uses anyhow)
src/cli.rs        clap definitions of every command; the commands for scripts and agents send a request and print the answer
src/control.rs    the JSON of those requests and answers, shared by the commands and the server; CORNERCASE_PANE
src/client.rs     the UI process: terminal setup/teardown, colour query, starts the server, forwards events, writes frames, reconnects a remote window; `kill-server`, `restart` and `update`
src/remote.rs     `cornercase remote`: the ssh command, the proxy's banner and version check, failures; `cornercase proxy` (stdio <-> the socket)
src/server.rs     the daemon: owns App and every Term, accepts clients on a Unix socket, draws a ratatui frame per client
src/protocol.rs   messages, length-prefixed postcard framing, socket and lock paths, build id
src/state.rs      the saved session as JSON, migrations, and the Saver that writes it (or todos.json) once it settles
src/restart.rs    what a server restart stops: running programs from a status Report, the confirmation texts
src/todo/         the TODO list: mod.rs (ordering, undo, todos.json, panel state), editor.rs (a text field with a cursor, soft wrap)
src/config.rs     user settings (config.json), `~` expansion, validation
src/settings.rs   the settings modal's state; returns Actions for App
src/shortcuts.rs  the prefix key (parse, match, clashes) and the keys menu's tree of actions
src/agents.rs     known coding agents, their modes and arguments, which agent takes an issue, detection, trust prompt
src/activity.rs   what the agent in a pane is doing: Claude Code's session file, its title glyph, Codex's title and turns, opencode's turns, done-but-unseen, rollups
src/context.rs    model/context lines for Claude Code, Codex and opencode; context/codex.rs reads Codex rollouts, context/opencode.rs opencode's SQLite database (its session, turn and line)
src/memory.rs     how much memory an agent pane uses (its shell and descendants), measured on a thread
src/usage/        plan usage: claude.rs (the `get_usage` control request), codex.rs (`codex app-server`, `account/rateLimits/read`), mod.rs (running a probe, the modal's state per agent)
src/notify.rs     desktop notifications through the outer terminal: which escape sequence a terminal understands, encoding
src/panics.rs     containing panics: `catch_unwind` wrappers for the server loop and background jobs, the hook that logs them
src/launch.rs     starting an agent in a new tab (pure state machine)
src/log.rs        server.log: levels, the line format, the writer thread and its rotation, `Job` timings, `cornercase logs`
src/secrets.rs    Shortcut / Linear / Jira tokens in secrets.json (0600)
src/markdown.rs   Markdown -> wrapped ratatui Lines
src/highlight.rs  syntax highlighting of fenced code (syntect scopes -> palette colours), cached
src/issues/       issue model and clients: github.rs (gh CLI), shortcut.rs (REST), linear.rs (GraphQL), jira.rs (REST; jira/adf.rs turns ADF into Markdown), http.rs, browser.rs (modal state), cache.rs (lists on disk)
src/clipboard.rs  OSC 52
src/worktree.rs   `git worktree add`/`remove`, checkout path, `.worktreeinclude`
src/upstream.rs   `git fetch` and commits to pull per workspace (`↓n`)
src/files/        files panel: mod.rs (open folders, viewer, search bar and mode, jobs, tree marks and margin marks from the changes diff), disk.rs (one folder's listing with `ignore`, reading a file), search.rs (the workspace's file index, fuzzy names with nucleo, text with ripgrep's searcher), link.rs (the path under a pane click and where it points)
src/syntax.rs     tree-sitter highlighting through arborium: which grammar a file gets, captures -> palette colours
src/changes/      changes panel: mod.rs (panel state, refresh pacing, folds, viewed, branch picker, tints), git.rs (git commands, base, merge-base), diff.rs (patch parser, word emphasis, highlighting), filter.rs (which files a path filter keeps)
src/search.rs     global search: candidates, ranking, state
src/picker.rs     folder picker state
src/vscode.rs     reads a VS Code `.code-workspace` (JSONC: comments and trailing commas) into a name and its folders
src/process.rs    a pid's cwd, name, arguments, environment, descendants and memory footprint: /proc on Linux, libproc and sysctl on macOS; a socket peer's uid
src/project.rs    Group, Project > Workspace > Tab > panes, labels, removal, moving
src/split.rs      a tab's split tree: rects, dividers, splitting, removing, ratios
src/app.rs        App state; turns AppEvents into actions; builds the View; app/todo_panel.rs wires the TODO panel, app/files_panel.rs the files panel; app/control.rs answers the commands for scripts and keeps their waits; app/shortcuts.rs runs the keys menu's actions; app/trace.rs logs what changed after each step
src/term.rs       a shell in a PTY, its Emulator, and the reader thread
src/emulator.rs   wraps libghostty-vt; takes plain Snapshots for ui
src/ui.rs         layout, hit testing and drawing from a plain View (no PTYs); ui/changes.rs draws the changes panel, ui/todo.rs the TODO panel, ui/files.rs the files panel, ui/remote.rs the reconnecting notice, ui/tab_bar.rs the tab bar above the pane (geometry and drawing), ui/keys.rs the keys menu
src/keys.rs       KeyEvent -> bytes (Ghostty's encoder for special keys, legacy encoder for the rest)
src/mouse.rs      MouseEvent -> bytes in the protocol the program asked for
src/host_theme.rs asks the outer terminal for its colours and its name (XTVERSION)
src/git.rs        branch from .git/HEAD, repo roots, linked worktrees (no git process)
src/update.rs     update check against GitHub releases, download, checksum, binary swap
src/error.rs      library error type
skills/cornercase/SKILL.md  the agent skill, embedded for `cornercase skill`
```

**Server loop:** the accept thread, one reader thread per client, the signal thread and a forwarder for PTY output all send `ServerEvent`s over one `mpsc` channel. The loop waits for an event or a 500 ms tick, drains the queue, then draws once per client when something on screen may have changed (`Server::due`). Each client has a writer thread so a slow one never blocks the loop. **Client:** the main thread writes each `Frame` to stdout; an input thread sends every crossterm event.

`ui::draw` takes a `View`, not `App`, so rendering is testable with `TestBackend`. Geometry functions (`ui::layout`, `entry_row`, `workspace_row`, `form_buttons`, `picker_item`, …) serve both drawing and hit testing, so tests take positions from them.

## Tests

- Unit tests sit next to the code in `mod <unit_of_work>` with sentence-like names; tabular cases use `rstest` with named `#[case::...]`.
- UI: render a `View` into `TestBackend`; `insta` snapshots for layout, cell styles for hover.
- App tests `click` with a press and a release (rows act on release); drags start with `press`.
- `term.rs` / `app.rs` tests spawn real `/bin/sh` PTYs (never the user's shell) and wait with `test_util::wait_until`, never sleeps. `/bin/sh` is `bash` on macOS, so tests check its name with `test_util::is_sh`. `TempDir` paths are canonical, because macOS' temp dir is behind a symlink (`/var` → `/private/var`).
- Helpers: `test_util::TempDir`, `git_repo`, `fake_gh`, `FakeHttp` (canned HTTP), `write_executable` (through a `/bin/sh` child to avoid `ETXTBSY`), `Family` (`sh` running `sh` running `sleep`, for process trees). Nothing calls real `gh`, Shortcut, Linear or Jira. App tests clear `App::env_tokens` and never use the real config.
- `cornercase remote` is tested with a fake `ssh` first on the `PATH` (`fake_ssh` in `tests/e2e.rs`): it skips the options and the destination and `exec`s the command locally, so the proxy reaches the test's own socket and keeps the fake's pid (killing it drops the connection; without `exec`, a `sh -c` left between them would keep the pipe open).
- Agents are faked with a script (`FAKE_AGENT`) that asks a trust question and echoes what it reads.
- Agent status is faked with a script named `claude` that writes its own `sessions/$$.json` (and, for the context line, a transcript under `projects/`); app tests point `App::claude_dir` at a temp dir, and e2e sets `CLAUDE_CONFIG_DIR` per `Session`, so nothing reads the real `~/.claude`. Codex's is `FakeCodex`: its rollout gets the turn fixtures appended, and its script sets the title it finds in a `title` signal file (OSC 0). opencode's is `FakeOpencode`: a script named `opencode` that holds a database built from the fixture schema, whose rows the test writes. `app::tests::agent_status::Watched` drives any of them with the same steps, so the notification tests run for each (opencode has no waiting). Tests that read a process's environment spawn `/bin/sleep` with a cleared one and wait until its arguments are `sleep`'s (before `exec`, `/proc` shows the parent's).
- The usage probes are faked with `claude` and `codex` scripts that read the requests and answer, set through `agent_commands`; app tests point the agent they do not fake at `/nonexistent/<kind>`, so no test runs the real `claude` or `codex` (both may be on the `PATH`).
- `tests/e2e.rs` runs the real binary in a PTY (`SHELL=/bin/sh`, `PS1='$ '`), parses output with `vt100`, sends raw bytes and SGR mouse sequences, and answers the startup colour query. Each test gets its own server through a `Session`; dropping it runs `kill-server`. `Session::says` runs a command for scripts against it; `Session::command` removes `CORNERCASE` and `CORNERCASE_PANE`, since the tests may run inside a real cornercase.
- `app::tests::control_requests` sends requests to an `App` with `App::request` and reads `take_answers`; its fake agent (`AGENT`) writes Claude's session file and works until the test writes a `finish` file.
- Avoid races in e2e: wait for output that proves the previous step finished (`echo cat-""starts; cat -v`).
- A safety-net test must fail without the code it protects.
- Panics are tested with a closure that panics, so `App` needs no test-only code: `panics::contain` takes it directly, and the server loop through `Server::serve_with`, which takes the function that runs each `Step` (`Server::step` in production). `server::tests::a_bug` runs that loop on a session in a temp dir (`Server::new` takes the session's path), with a client attached over a socket pair whose frames it reads with `vt100`.

### Manual check in a real terminal

```sh
T() { tmux -L cctest "$@"; }
T new-session -d -s t -x 100 -y 20 ./target/debug/cornercase
T send-keys -t t 'echo hi' Enter
T send-keys -t t -l $'\e[<0;6;5M'         # mouse press at col 6, row 5 (1-based)
T capture-pane -p -t t
T kill-server
```

## Website (`site/`)

Astro + Starlight, deployed to GitHub Pages by `.github/workflows/pages.yml` (Pages source: GitHub Actions). `cd site && npm ci && npm run dev`; `npm run check` and `npm run build` must pass.

- **Keep it in sync with the app.** When a feature, setting, `config.json` key, message, path or click changes, update in the same change: the docs pages that describe it (`src/content/docs/docs/`, search them for the old wording), the landing page if it shows it, the simulation if the UI changed, and the screens it makes stale.
- The landing page (`src/pages/index.astro`, `src/components/landing/`) is custom; the documentation is Starlight content in `src/content/docs/docs/`. Internal doc links are relative with a trailing slash, so the site works under any base path.
- The terminal on the landing page is a simulation in TypeScript (`src/lib/demo/`) that mirrors `ui.rs`: same layout, labels and colours, with fake shells, agents and issues. When `ui.rs` changes, update the simulation too. Its TODO panel (`todo.ts`) leaves out dragging; its files panel (`files.ts`) reads the fake projects' files and ranks names with a simpler fuzzy match than nucleo; its picker lists no `.code-workspace` files (its folders are fake). The same code renders the feature pictures to SVG at build time (`scenes.ts`).
- The hero plays a tour in chapters (`boot.ts`) on the simulation's virtual clock, so a chapter can be fast-forwarded and the clock sped up while detached. It finds what to click by its text (`issues`, `#482`, `quit`, `changes`), so check it still plays to the end after changing the simulation.
- The speed numbers on the landing (`Stats.astro`) and in the FAQ (`Is it fast?`) are a dated snapshot (October 2026, the versions the FAQ names), exempt from keeping the site in sync: never update them as part of other changes; re-measure only when asked, then change both places together.
- Docs screenshots are real: `src/screens/*.ansi` are `tmux capture-pane -e -p -N` dumps of the app, run with a fake `HOME` and its own `XDG_RUNTIME_DIR` (a separate server that still shows the default paths), rendered to SVG at build time by `src/lib/term/`. Box-drawing, block and a few symbol characters are drawn as shapes, not font glyphs, so lines join. Bold, dim, italic, underline, inverse and strikethrough (done TODO items) are rendered. Re-capture them when the UI changes.
- Base URL and origin come from `actions/configure-pages` (`SITE_BASE`, `SITE_ORIGIN`). The site lives on the custom domain `usecornercase.dev` at `/` (DNS in DigitalOcean: GitHub Pages A/AAAA records on the apex, `www` CNAME to `usecornercase.github.io`); the old github.io address redirects there. Local builds default to the same origin and base.

## Known limitations

- No scrollback navigation. Shells do not survive the server: after `kill-server` or a reboot, panes come back as new shells (Claude Code and Codex resume their conversations in them).
- Host colours are read once; a theme switch is not seen.
- Inner programs never get key releases.
- Runs on Linux and macOS only.

## Instruction files

- `AGENTS.md` (this file) is the entry point: Codex and other agents load it on their own. `CLAUDE.md` only imports it and `DESIGN.md` (`@AGENTS.md`, `@DESIGN.md`), so Claude Code loads both. Every rule lives in one of the two; never copy text between them.
- Codex loads at most 32 KiB of project instructions (`project_doc_max_bytes`) and cuts the rest without a warning, so `AGENTS.md` holds the rules and `DESIGN.md`, far larger, is read on demand. CI fails when `AGENTS.md` passes 32 KiB; move detail to `DESIGN.md` rather than raising the limit.
- To check what Codex loads without running a model: `CODEX_HOME=$(mktemp -d) codex debug prompt-input` at the repository root prints its prompt, which must hold `# AGENTS.md instructions for <root>` with this file whole (an empty `CODEX_HOME` leaves out personal settings such as `project_doc_fallback_filenames`). For Claude Code, `claude -p /context` lists `CLAUDE.md`, `AGENTS.md` and `DESIGN.md` under Memory Files.
