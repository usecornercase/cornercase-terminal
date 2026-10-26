# cornercase

A terminal multiplexer TUI in Rust. A sidebar of **projects** (folders, optionally in **groups** with an icon and a colour), a column with the active project's **workspaces** (lines of work, optionally each in its own git worktree) and their **tabs** (one or more shells, split like Ghostty), the active tab's panes, and an optional **changes** panel on the right with the workspace's git diff. Everything is driven by mouse buttons. An issues modal (GitHub, GitLab, Shortcut, Linear, Jira, Plane, Asana) reads an issue and starts a coding agent on it in its own worktree. A background server owns the shells, so closing the UI leaves them running and the next `cornercase` reattaches.

These rules apply to every contributor and coding agent, in `src/`, `tests/` and `site/`. The design decisions and their reasons are in `design/`, one file per area, listed in [DESIGN.md](DESIGN.md): read the files for the areas and topics you touch before changing them, and update them in the same change when a decision changes. How these files are loaded is under [Instruction files](#instruction-files).

## Commands

```sh
cargo run                                   # attach to the server (starts it if none is running)
cargo run -- --help                         # every command, including the ones for scripts and agents
cargo run -- kill-server                    # stop this tree's dev server and its shells
cargo test --locked                         # unit + snapshot + e2e tests
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo fmt
cargo-machete                               # unused dependencies
npx -y jscpd@4.3.0                          # copy-paste detector, reads .jscpd.json
```

- **Building needs Zig 0.15.2 on the PATH**: `libghostty-vt-sys` runs `zig build` on Ghostty's sources, and Ghostty pins the Zig minor version. The first build per profile clones Ghostty (network, ~100 s, ~530 MB in `target/`); `GHOSTTY_SOURCE_DIR` can point at a local checkout.
- **The toolchain is pinned** in `rust-toolchain.toml` (version and components), and CI installs exactly that one, so a new Rust release cannot break CI with new lints. Bump it by hand: change the version, then fix what clippy reports.
- `.cargo/config.toml` sets `LIBGHOSTTY_VT_SYS_OPTIMIZE=ReleaseFast`; a Zig Debug build of Ghostty is too slow to use.
- **A debug build has its own server and folders per source tree** (`design/client-server.md`): after a rebuild, `cargo run -- kill-server` stops only this tree's (a client refuses a server from another build), asking first inside its own server. In a cornercase pane, open the window with `env -u CORNERCASE cargo run`, and never run the installed `cornercase kill-server`.
- Snapshots (`insta`): a changed render writes its `.snap.new` in the `snapshots/` folder next to the test's file (`src/ui/tests/snapshots/` for the `ui` tests) and fails. Check it, then accept with `INSTA_UPDATE=always cargo test`. Delete snapshots of renamed or removed tests by hand.
- **Before calling a task done, run fmt, clippy, tests, `cargo-machete` and jscpd, and fix what fails.** If one cannot pass, say so.
- **A change that adds, removes or changes a feature updates the website in the same change** (see Website below).
- **A feature reaches external clients in the same change**: its state and actions are reachable over the socket (a request, or optional fields of `status`, `watch`, `events` or `surface`), additive under the contract's rules, so other UIs, scripts and plugins can build on it. TUI-only details (overlays, hover, the toast, the keys menu, the compact layout, drawing) are exempt; when exposing a feature is a large job of its own, open an issue for it (`design/commands-for-scripts-and-agents.md`).
- `jscpd` fails above 1 % duplication (50-token clones) in every file of `src/` and `tests/`. It is a ratchet: extract the shared code, do not raise the threshold. `cargo-machete` is a text search; a false positive goes in `[package.metadata.cargo-machete] ignored`.
- **Every pull request that changes the app bumps `version` in `Cargo.toml` and adds a `## <version>` section to `CHANGELOG.md`** (a patch unless told otherwise; the section is written for users, it becomes the release notes and the update dialog's text). "The app" is `src/`, `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `.cargo/`; docs, tests and `site/` alone need no bump. CI checks it (`.github/version.sh check`, in the guardrails job). Main requires branches to be up to date, so two pull requests cannot ship the same version.
- **Commit messages and pull request titles follow [Conventional Commits](https://www.conventionalcommits.org/)**: `<type>: <summary>`, the summary in the imperative and in lower case, such as `feat: ask before closing a project`. Types: `feat` (a feature users see), `fix`, `docs`, `test`, `refactor`, `perf`, `ci`, `build`, `chore`; a breaking change adds `!` (`feat!: …`). The version bump is its own commit, `chore: release <version>`; Dependabot's are `chore(deps): …` (`commit-message` in `.github/dependabot.yml`). Both reach `main`: a squash merge takes the pull request's title (or its only commit's message), and a merge or a rebase keeps every commit.
- **Releases are automatic**: a push to `main` with a new version tags and publishes it (`design/releases.md`); if the app changed but the version was already released, the job fails. `release.yml` is generated: change `dist-workspace.toml` or `.github/build-setup.yml`, then run `dist generate`; never edit it by hand.
- **CI** (`.github/workflows/ci.yml`) runs the same checks: clippy + tests on Linux and macOS (one job per OS so clippy and tests share the Ghostty build; `target/` cached by `Swatinem/rust-cache`, saved from `main` only), and fmt + machete + jscpd (with the size of `AGENTS.md`) on Linux. Dependabot groups updates monthly; `libghostty-vt` is pinned with `=` (pre-1.0 API), so bumping it needs a manual check; TypeScript majors are ignored in `site/` (`design/website.md`).
- **Code review**: CodeRabbit reviews every non-draft pull request except Dependabot's; `.coderabbit.yaml` configures it. It reads `AGENTS.md`, `DESIGN.md` and `design/` as its guidelines (`knowledge_base.code_guidelines`). Its docstring check, test generation and clippy are off (no comments on purpose; it cannot build Ghostty).

## Code conventions

- Everything in English: identifiers, test names, messages, UI text.
- **No comments at all**, including `///` and `//!`. Rationale goes in the area's file in `design/`.
- Errors: `crate::error::Error` (`thiserror`) in the library; `anyhow` only in `src/main.rs`.
- No `unwrap()` outside tests (`clippy::unwrap_used`); in tests prefer `expect("what")`.
- Mutexes are `parking_lot::Mutex`.
- Lints live in `Cargo.toml` (clippy `all` deny, `pedantic` warn). Silence one only locally with `#[expect(clippy::x, reason = "...")]`.
- **Small functions**: `clippy.toml` limits a function to 40 lines, 5 arguments (`self` counts), two levels of nesting inside a method (`excessive-nesting-threshold = 4`) and a cognitive complexity of 10 (`cognitive_complexity`, turned on in `Cargo.toml`). Split the function; never silence these four (`design/code-layout.md`).
- Imports in three groups: `std`, external crates, then `super`/`crate`.
- `rustfmt.toml`: `max_width = 120`.
- **Where code goes** (the reasons are in `design/code-layout.md`): by domain; `impl App` only in `app.rs` and under `src/app/`; only `app` and `server` use `crate::app`, and only `ui`, `app`, `server` and `client` use `crate::ui`; the tests of `foo.rs` in `foo/tests.rs`; at most 1,000 lines per file. `app/` reaches workspaces and tabs through `Hierarchy`'s places, never `.workspaces[…]` or `.tabs[…]`. `tests/layers.rs` checks the imports, the sizes and the indexing, and its lists of exceptions only shrink.
- **`pub` only for what `src/main.rs` or the e2e tests use**; everything else is `pub(crate)` or narrower. `unreachable_pub` (in `Cargo.toml`) enforces it, so `dead_code` reports what nothing uses: delete it, or put it under `#[cfg(test)]` when only tests use it.

## Interaction

- **Mouse first; the only app keys sit behind an opt-in prefix key.** Every key goes to the program in the active pane, except while a plugin's popup is open (every key, `Esc` and the prefix key included, goes to its program; a click outside it does nothing), a modal, the search, the changes panel's filter field, the files panel's search or find field or a TODO field has them (then `Enter` submits, or breaks the line in a multi-line field, `Esc` cancels; the files panel's search field also takes `↑` `↓` to pick a result, and its find field `↑` `↓` to go from match to match; a TODO field also takes the arrows, `Home`, `End` and `Delete`, to fix a word in the middle, and every field with a cursor (`editor::Editor`) takes `Ctrl+←` `Ctrl+→` and `Alt+←` `Alt+→` (or `Esc b` `Esc f`, what Option+arrows send on macOS) to move by word, `Ctrl+A` `Ctrl+E` (what Cmd+arrows send in Ghostty on macOS) like `Home` `End`, and `Shift` with the arrows, `Home` and `End` (with `Ctrl` or `Alt`, by word) to select), `Esc` while a row, border or divider is dragged, which only ends the drag (a side panel stays open), else while a side panel (changes, files, TODO, Flow) shows, which closes it (a field with text clears first), and the prefix key (`prefix_key`, off by default, chosen by the user), which opens the keys menu (`shortcuts.rs`, see `design/keyboard-shortcuts.md`). Do not add keyboard shortcuts outside that menu, nor a default prefix, without asking. No `Alt` shortcuts (Option is a compose key on macOS), except the word moves inside those fields, where Option+arrows are the macOS habit and compose no character, no `Ctrl+letter` (steals shell bindings), except `Ctrl+A` `Ctrl+E` inside those fields, where no shell gets the keys. `e2e::input::ctrl_b_reaches_the_shell` guards this.
- Closing the last project leaves the app open and empty. **quit** (in ` More ▴ `) only detaches.
- `×` and ` ⋯ ` (the row's menu, as a right-click) buttons only show while hovering their row, except in compact mode, where they always show dimmed (touch screens have no hover) and closing a tab or a plain workspace asks first (`Overlay::CloseTab`, `Overlay::CloseWorkspace`), since an always-visible `×` is easy to tap by accident. A plain workspace's `×` also asks, in wide mode too, when closed agents would be forgotten (`design/saved-session.md`). Names are cut at the end (`text::truncate_right`), paths at the start (`truncate_left`).
- At most one overlay is open (menu, form, confirmation, settings, usage, remote, pairing, picker, issues, search, keys menu, plugin popup). While it is open, no mouse event reaches the columns or the pane. A text field's right-click menu (copy, cut, select all) is not an overlay: it floats over its field and closes on any other click or key without closing the dialog.
- Overlays, hover, scroll, column widths and the toast live in `App` and are shared by every attached client. A toast can carry an ` undo ` (TODO removals) or a ` go ` (an agent's notice), then lasts 6 s, or a ` cancel ` (a pending restart, shown for as long as it waits).

## Architecture

What each file and folder of `src/` holds, the server loop and the client are in [`design/architecture.md`](design/architecture.md). A change that adds, moves or removes a module updates it in the same change.

## Tests

- The tests of `foo.rs` (or `foo/mod.rs`) go in `foo/tests.rs`, declared as the last item with `#[cfg(test)] mod tests;`; a test file over 1,000 lines keeps its helpers and `mod` lines and moves each group to `foo/tests/<group>.rs`. Tests are `mod <unit_of_work>` groups with sentence-like names; tabular cases use `rstest` with named `#[case::...]`.
- UI: render a `View` into `TestBackend`; `insta` snapshots for layout, cell styles for hover.
- App tests `click` with a press and a release (rows act on release); drags start with `press`.
- App tests drain events with `drain`, draw with `render` and write their config with `write_config`. Fixtures are read with `test_util::fixture!`, never with a relative `include_str!`.
- `term.rs` / `app.rs` tests spawn real `/bin/sh` PTYs (never the user's shell) and wait with `test_util::wait_until`, never sleeps. `/bin/sh` is `bash` on macOS, so tests check its name with `test_util::is_sh`. `TempDir` paths are canonical, because macOS' temp dir is behind a symlink (`/var` → `/private/var`).
- Helpers: `test_util::TempDir`, `git_repo`, `clone_of`, `fake_gh`, `fixture!`, `SHORTCUT_MEMBER`, `FakeHttp` (canned HTTP), `write_executable` (through a `/bin/sh` child to avoid `ETXTBSY`), `Family` (`sh` running `sh` running `sleep`, for process trees). Nothing calls real `gh`, GitLab, Shortcut, Linear, Jira, Plane, Asana, Telegram or Discord. App tests clear `App::env_tokens` and never use the real config.
- `cornercase remote` and `--remote` are tested with a fake `ssh` first on the `PATH` (`fake_ssh` in `tests/e2e/remote.rs`): it skips the options and the destination and `exec`s the command locally, so the proxy reaches the test's own socket and keeps the fake's pid (`design/remote-windows.md`).
- Agents are faked with a script (`FAKE_AGENT`) that asks a trust question and echoes what it reads.
- Agent status is faked with `FakeClaude`, whose script named `claude` writes its own `sessions/$$.json` (and, for the context line, a transcript under `projects/`); app tests point `App`'s `agents.claude_dir` at a temp dir, and e2e sets `CLAUDE_CONFIG_DIR` per `Session`, so nothing reads the real `~/.claude`. Each other agent has its own fake in its folder (`FakeCodex` and so on; `design/test-helpers.md`). No test runs a real agent. `app::tests::agent_status::Watched` drives any of them with the same steps, so the notification tests run for each (opencode has no waiting). Tests reading a process's environment spawn `/bin/sleep` with a cleared one and wait until its arguments are `sleep`'s (before `exec`, `/proc` shows the parent's).
- The usage probes are faked with `claude` and `codex` scripts that read the requests and answer, set through `agent_commands`; app tests point the agent they do not fake at `/nonexistent/<kind>`, so no test runs the real `claude` or `codex` (both may be on the `PATH`).
- `tests/e2e/` (`main.rs` holds `Session` and `Harness`, one module per area) runs the real binary in a PTY (`SHELL=/bin/sh`, `PS1='$ '`), parses output with `vt100`, sends raw bytes and SGR mouse sequences, and answers the startup colour query. Each test gets its own server through a `Session`; dropping it runs `kill-server`. `Session::says` runs a command for scripts against it; `Session::command` removes `CORNERCASE`, `CORNERCASE_PANE` and `CORNERCASE_REMOTE`, since the tests may run inside a real cornercase.
- `app::tests::control_requests` sends requests to an `App` with `App::request` and reads `take_answers`; its fake agent (`agents::claude::fake::FakeClaude::CONTROL`) writes Claude's session file, records each prompt in a transcript, draws Claude's input box (or a panel without it, after `panel`) and works until the test writes a `finish` file.
- Avoid races in e2e: wait for output that proves the previous step finished (`echo cat-""starts; cat -v`).
- A safety-net test must fail without the code it protects.
- Panics are tested with a closure that panics, so `App` needs no test-only code: `panics::contain` takes it directly, and the server loop through `Server::serve_with`, which takes the function that runs each `Step` (`Server::step` in production). `server::tests::a_bug` runs that loop on a session in a temp dir (`Server::new` takes the session's path), with a client attached over a socket pair whose frames it reads with `vt100`.

### Manual check in a real terminal

```sh
T() { tmux -L cctest "$@"; }
T new-session -d -s t -x 100 -y 20 env -u CORNERCASE ./target/debug/cornercase
T send-keys -t t 'echo hi' Enter
T send-keys -t t -l $'\e[<0;6;5M'         # mouse press at col 6, row 5 (1-based)
T capture-pane -p -t t
T kill-server
```

## Website (`site/`)

Astro + Starlight, deployed to GitHub Pages by `.github/workflows/pages.yml` (Pages source: GitHub Actions). `cd site && npm ci && npm run dev`; `npm run check` and `npm run build` must pass.

- **Keep it in sync with the app.** When a feature, setting, `config.json` key, message, path or click changes, update in the same change: the docs pages that describe it (`src/content/docs/docs/`, search them for the old wording), the landing page if it shows it, the simulation if the UI changed, and the screens it makes stale.
- The landing page (`src/components/landing/Home.astro`, rendered by `src/pages/index.astro`, `es/` and `pt-br/`) is custom; the documentation is Starlight content in `src/content/docs/docs/`. Internal doc links are relative with a trailing slash, so the site works under any base path.
- **The landing page is in English, Spanish and Brazilian Portuguese** (`/`, `/es/`, `/pt-br/`). Its text lives in `src/i18n/`, one file per locale typed against `en.ts`: `page` for the components, `demo` for the tour's captions and the simulation's narration. A change to the landing's text updates the three locales in the same change; `tests/i18n.test.ts` fails when a locale misses a text or a `{mark}` or `` `code` ``. Everything drawn inside the simulated terminal, the docs and the app stay in English.
- The terminal on the landing page is a simulation in TypeScript (`src/lib/demo/`) that mirrors `ui.rs` and `ui/`: same layout, labels and colours, with fake shells, agents and issues. When `ui.rs` or `ui/` changes, update the simulation too. Its TODO panel (`todo.ts`) leaves out dragging; its name fields (new workspace, new group, rename) take no clicks and draw no selection; its shells have no scrollback; its files panel (`files.ts`) reads the fake projects' files and ranks names with a simpler fuzzy match than nucleo; its picker lists no `.code-workspace` files; its tab line never reads `compacted`; its icon bar row stays off and its panels' own glyphs stay plain (it draws the file type icons with the site's symbols subset); its flow dialogs are left out, and its flow run (`flowrun.ts`) plays only while its panel shows. The same code renders the feature pictures to SVG at build time (`scenes.ts`).
- The hero loads as a still picture (`scenes.ts` → `poster`) and mounts the simulation on the first click; `boot.ts` then plays a tour in four chapters on the simulation's virtual clock, so a chapter can be fast-forwarded and the clock sped up while detached. It finds what to click by its text (`Issues`, `#482`, `More ▴` and `quit`, `Changes`), so check it still plays to the end after changing the simulation.
- `npm run check` also runs the tests in `site/tests/` (`node:test`, bundled by `scripts/test-demo.mjs` with esbuild) against the simulation on its virtual clock, including the hero tour to its end. A simulation change adds or updates a test there.
- The speed numbers on the landing (`Stats.astro`) and in the FAQ (`Is it fast?`) are a dated snapshot (October 2026, the versions the FAQ names), exempt from keeping the site in sync: never update them as part of other changes; re-measure only when asked, then change both places together.
- Docs screenshots are real: `src/screens/*.ansi` are `tmux capture-pane -e -p -N` dumps of the app, made by `site/capture/capture.sh` (a fake `HOME` with the landing page's projects, its own server, stand-ins for the agents, a flow's team included, `gh` and `ssh`), rendered to SVG at build time by `src/lib/term/`. Box-drawing, block and a few symbol characters are drawn as shapes, not font glyphs, so lines join. Bold, dim, italic, underline, inverse and strikethrough are rendered. Run `site/capture/capture.sh` after `cargo build` when the UI changes, and add a screen to `site/capture/screens.sh` rather than capturing by hand.
- Base URL and origin come from `actions/configure-pages` (`SITE_BASE`, `SITE_ORIGIN`). The site lives on the custom domain `usecornercase.dev` at `/`; the old github.io address redirects there. Local builds default to the same origin and base.

## Known limitations

- Shells do not survive the server: after `kill-server` or a reboot, panes come back as new shells (every known agent but opencode resumes its conversation in them).
- Host colours are read once; a theme switch is not seen.
- Inner programs never get key releases.
- Runs on Linux and macOS only.

## Instruction files

- `AGENTS.md` (this file) is the entry point: Codex and other agents load it on their own. `CLAUDE.md` only imports it and `DESIGN.md` (`@AGENTS.md`, `@DESIGN.md`), so Claude Code loads both. `DESIGN.md` is only the index of `design/`, whose files nothing loads by itself: read them on demand, for the areas a change touches. Every rule and every decision lives in one place, here or in one file of `design/`; never copy text between them.
- Codex loads at most 32 KiB of project instructions (`project_doc_max_bytes`) and cuts the rest without a warning, so `AGENTS.md` holds the rules and `design/` the decisions. CI fails when `AGENTS.md` passes 32 KiB; move detail to `design/` rather than raising the limit. Claude Code loads `DESIGN.md` whole, so it stays an index of one line per area.
- To check what Codex loads without running a model: `CODEX_HOME=$(mktemp -d) codex debug prompt-input` at the repository root prints its prompt, which must hold `# AGENTS.md instructions for <root>` with this file whole (an empty `CODEX_HOME` leaves out personal settings such as `project_doc_fallback_filenames`). For Claude Code, `claude -p /context` lists `CLAUDE.md`, `AGENTS.md` and `DESIGN.md` under Memory Files, and no file of `design/`.
