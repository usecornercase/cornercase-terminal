# cornercase

A terminal multiplexer TUI in Rust. A sidebar of **projects** (folders, optionally in **groups** with an icon and a colour), a column with the active project's **workspaces** (lines of work, optionally each in its own git worktree) and their **tabs** (one or more shells, split like Ghostty), the active tab's panes, and an optional **changes** panel on the right with the workspace's git diff. Everything is driven by mouse buttons. An issues modal (GitHub, Shortcut, Linear) reads an issue and starts a coding agent on it in its own worktree. A background server owns the shells, so closing the UI leaves them running and the next `cornercase` reattaches.

## Commands

```sh
cargo run                                   # attach to the server (starts it if none is running)
cargo run -- kill-server                    # stop the server and every shell in it
cargo test --locked                         # unit + snapshot + e2e tests
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo fmt
cargo-machete                               # unused dependencies (not `cargo machete`, which errors)
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
- **Releases are automatic**: on a push to `main`, `.github/workflows/version.yml` runs `.github/version.sh release`, which tags `v<version>` if it is new and dispatches `.github/workflows/release.yml` (`dist`, `dispatch-releases`, because a tag pushed with `GITHUB_TOKEN` triggers no workflow). That builds the four targets, makes the GitHub Release (notes from `CHANGELOG.md`) with `cornercase-installer.sh`, and pushes the formula to `usecornercase/homebrew-tap` (secret `HOMEBREW_TAP_TOKEN`, a fine-grained token that expires). If the app changed but the version was already released, the job fails. `release.yml` is generated: change `dist-workspace.toml` or `.github/build-setup.yml` (the Zig step), then run `dist generate`; never edit it by hand. `site/public/install.sh` (served at `usecornercase.dev/install.sh`) runs the latest release's installer.
- **CI** (`.github/workflows/ci.yml`) runs the same checks: clippy + tests on Linux and macOS (one job per OS so clippy and tests share the Ghostty build; `target/` cached by `Swatinem/rust-cache`, saved from `main` only), and fmt + machete + jscpd on Linux. Dependabot groups action and crate updates monthly; `libghostty-vt` is pinned with `=` (pre-1.0 API), so bumping it needs a manual check. TypeScript majors are ignored in `site/`: `@astrojs/check` 0.9.10 accepts only TypeScript 5 and 6 (7 is the Go port, without the JS API its language server uses); drop the `ignore` once it supports 7.
- **Code review**: CodeRabbit (free for public repos) reviews every non-draft pull request except Dependabot's; `.coderabbit.yaml` configures it. It reads this file as its guidelines. Its docstring check and docstring/test generation are off (the code has no comments on purpose), and so is its clippy (it cannot build Ghostty without Zig; CI runs clippy).

## Code conventions

- Everything in English: identifiers, test names, messages, UI text.
- **No comments at all**, including `///` and `//!`. Rationale goes in this file.
- Errors: `crate::error::Error` (`thiserror`) in the library; `anyhow` only in `src/main.rs`.
- No `unwrap()` outside tests (`clippy::unwrap_used`); in tests prefer `expect("what")`.
- Mutexes are `parking_lot::Mutex`.
- Lints live in `Cargo.toml` (clippy `all` deny, `pedantic` warn). Silence one only locally with `#[expect(clippy::x, reason = "...")]`.
- Imports in three groups: `std`, external crates, then `super`/`crate`.
- `rustfmt.toml`: `max_width = 120`.

## Architecture

```
src/main.rs       client, `server` or `kill-server` from argv (uses anyhow)
src/client.rs     the UI process: terminal setup/teardown, colour query, starts the server, forwards events, writes frames
src/server.rs     the daemon: owns App and every Term, accepts clients on a Unix socket, draws a ratatui frame per client
src/protocol.rs   messages, length-prefixed postcard framing, socket and lock paths, build id
src/state.rs      the saved session as JSON, migrations, and the Saver that writes it once it settles
src/config.rs     user settings (config.json), `~` expansion, validation
src/settings.rs   the settings modal's state; returns Actions for App
src/agents.rs     known coding agents, their modes and arguments, which agent takes an issue, detection, trust prompt
src/activity.rs   what the agent in a pane is doing: Claude Code's session file, its title glyph, done-but-unseen, rollups
src/context.rs    the model and context use of the Claude in a pane: transcript tail, window size
src/usage.rs      Claude Code plan usage: the `get_usage` control request, parsing, the modal's state
src/notify.rs     desktop notifications through the outer terminal: which escape sequence a terminal understands, encoding
src/launch.rs     starting an agent in a new tab (pure state machine)
src/secrets.rs    Shortcut / Linear tokens in secrets.json (0600)
src/markdown.rs   Markdown -> wrapped ratatui Lines
src/highlight.rs  syntax highlighting of fenced code (syntect scopes -> palette colours), cached
src/issues/       issue model and clients: github.rs (gh CLI), shortcut.rs (REST), linear.rs (GraphQL), http.rs, browser.rs (modal state), cache.rs (lists on disk)
src/clipboard.rs  OSC 52
src/worktree.rs   `git worktree add`/`remove`, checkout path, `.worktreeinclude`
src/upstream.rs   `git fetch` and commits to pull per workspace (`↓n`)
src/changes/      changes panel: mod.rs (panel state, refresh pacing, folds, viewed, branch picker, tints), git.rs (git commands, base, merge-base), diff.rs (patch parser, word emphasis, highlighting)
src/search.rs     global search: candidates, ranking, state
src/picker.rs     folder picker state
src/process.rs    a pid's cwd, name, arguments and environment: /proc on Linux, libproc and sysctl on macOS; a socket peer's uid
src/project.rs    Group, Project > Workspace > Tab > panes, labels, removal, moving
src/split.rs      a tab's split tree: rects, dividers, splitting, removing, ratios
src/app.rs        App state; turns AppEvents into actions; builds the View
src/term.rs       a shell in a PTY, its Emulator, and the reader thread
src/emulator.rs   wraps libghostty-vt; takes plain Snapshots for ui
src/ui.rs         layout, hit testing and drawing from a plain View (no PTYs); ui/changes.rs draws the changes panel
src/keys.rs       KeyEvent -> bytes (Ghostty's encoder for special keys, legacy encoder for the rest)
src/mouse.rs      MouseEvent -> bytes in the protocol the program asked for
src/host_theme.rs asks the outer terminal for its colours and its name (XTVERSION)
src/git.rs        branch from .git/HEAD, repo roots, linked worktrees (no git process)
src/update.rs     update check against GitHub releases, download, checksum, binary swap
src/error.rs      library error type
```

**Server loop:** the accept thread, one reader thread per client, the signal thread and a forwarder for PTY output all send `ServerEvent`s over one `mpsc` channel. The loop waits for an event or a 500 ms tick, drains the queue, then draws once per client. Each client has a writer thread so a slow one never blocks the loop. **Client:** the main thread writes each `Frame` to stdout; an input thread sends every crossterm event.

`ui::draw` takes a `View`, not `App`, so rendering is testable with `TestBackend`. Geometry functions (`ui::layout`, `entry_row`, `workspace_row`, `form_buttons`, `picker_item`, …) serve both drawing and hit testing, so tests take positions from them.

## Design decisions

**Interaction**
- **Mouse buttons only, no app shortcuts.** Every key goes to the program in the active pane, except while a modal or the search is open (then `Enter` submits, `Esc` cancels) and `Esc` while a row is dragged. Do not add keyboard shortcuts without asking. No `Alt` shortcuts (Option is a compose key on macOS), no `Ctrl+letter` (steals shell bindings). `e2e::ctrl_b_reaches_the_shell` guards this.
- Closing the last project leaves the app open and empty. ` quit ` only detaches.
- `×` buttons only show while hovering their row. Names are cut at the end (`ui::truncate_right`), paths at the start (`truncate_left`).
- At most one overlay is open (menu, form, confirmation, settings, usage, picker, issues, search). While it is open, no mouse event reaches the columns or the pane.
- Overlays, hover, scroll, column widths and the toast live in `App` and are shared by every attached client.

**Layout (`ui.rs`)**
- Three columns: projects (32), workspaces (26), pane (after one blank column). Columns are always present so PTY sizes do not jump. Borders drag to resize (double click resets), within `MIN_COLUMN_WIDTH` and `MIN_PANE_WIDTH`; `Widths::fit` squeezes them on small terminals. Widths are saved in the session. PTYs follow the drag live.
- **Stacked sidebar** (`sidebar` in `config.json`: `side_by_side`, `projects_on_top`, `workspaces_on_top`; wide mode only): one column of `Widths::projects` holds both lists, split by a line that drags (`Border::Stack`; `Widths::stack` = rows of the top section, `None` = half, each section at least `MIN_STACK_SECTION`), with one footer anchored at the bottom (issues and changes, settings, usage, quit). `stacked_layout` fills the same `Areas`, so drawing and hit testing are shared; `Areas::sidebar` and `Areas::workspaces` are the two sections, so the wheel scrolls the list under it. Column borders are drawn from their `Areas` rects, not from `Block`s, so a stacked column gets one full-height border.
- Lists scroll by item with the wheel, show ` ↑ n more` / ` ↓ n more`, and keep the `+` button sticky at the bottom. The scroll follows the active item only when it changes (`App::follow`), otherwise the wheel could never move away from it.
- The active project and tab get a cyan `▌` and a surface background (palette 236, or 254 on light themes). Colours use palette indices so they work without truecolor; the brand purple is index 99.
- A hovered project, workspace, tab or group row is filled edge to edge with palette 235 (255 on light themes), one step between the background and the surface (`View::row_background`). The active row keeps its surface. No hover fill while an overlay is open or a border or divider is being dragged. The site's palette tints 235 like 236 (`site/src/lib/term/palette.ts`), so it stays between them.
- **Compact mode** below 90 columns (`ui::compact_layout`): a top bar (` ≡ `, `project › workspace › tab`, ` ⌕ `) and a full-screen menu showing one column at a time. Every clickable item is a 3-row band (`Areas::pitch`). Both columns' areas share one rect and `Areas::shown(nav)` blanks the hidden one, so drawing and hit testing reuse the wide-mode code.

**Hierarchy (`project.rs`)**
- Optional group > project (a canonicalized folder) > workspace (the project folder, or its own git worktree) > tab > panes. Every level has an id from one counter; menus, forms and background jobs refer to ids, never indices.
- Labels: custom name, else folder name (project), branch or `default` (workspace), foreground program (tab).
- Closing kills processes; removal waits for `Exited`, never synchronous. Closing the last tab keeps the workspace; closing the last workspace keeps the project.
- **Groups**: one level, no nesting; a new one goes last. A project holds `group: Option<u64>`; the projects column is `ui::sidebar_rows` (loose projects first, then per group a gap, the header and, unless collapsed, its projects), which drawing, hit testing and `App::follow` share. A collapsed group holding the active project gets the `▌` on its header. Deleting a group (the `×` on its header, or its menu) asks first (`Overlay::DeleteGroup`) and only ungroups its projects. Icons are a fixed set of one-cell symbols (`ui::GROUP_ICONS`, no emoji or Nerd Font glyphs, whose width varies); colours are palette indices (`ui::GROUP_COLOURS`, no greys, black or white, which vanish on some themes). A new group gets the next icon and colour, then opens the icon and colour modal, whose clicks apply at once.
- Menus are one `Overlay::Menu { actions: Vec<MenuAction> }`, labels from `App::menu_label`; a submenu (move to group) replaces the menu at the same spot. `+ new project` opens a menu (open project / new group).
- **Reordering** (drag): project, group, workspace and tab rows act on release. A press grabs the row (`App::row_drag`, a `Target`); leaving the row's rect, or releasing outside it, makes it a drag, otherwise the release is the click (activate, collapse), so a cancelled drag changes nothing and the compact menu stays open while dragging. `×` and `+` still act on press. Releasing outside the list, `Esc` or another button cancels; no hover while dragging.
- Where it lands (`ui::sidebar_drop`, `ui::workspace_drop`) comes from the pointer and the layout without the line, so the line never feeds back: the row under the pointer gives its slot (before it when above the dragged row, after when below); a group with its projects and a workspace with its tabs are one block; a group header takes a project in first (last when collapsed); a gap is the end of the section above; tabs stay in their workspace, workspaces in their project (moving a shell or a worktree elsewhere was left out). The line goes on a neighbouring gap, on the line above or below the visible rows at an edge, else as an inserted one-line `Landing` row; it is indented where a grouped project or a tab would start.
- Holding a drag on `↑ n more` / `↓ n more` scrolls one item every `AUTO_SCROLL_EVERY` (150 ms); `App::tick` shortens the server's wait meanwhile. Moving only changes `Vec` positions (`project::move_before` keeps the active index on its item), so ids, menus and jobs are unaffected and the session saves the order as is; worktrees found outside cornercase go last.

**Splits (`split.rs`)**
- A binary tree (`Leaf` / `Split { dir, ratio, first, second }`). Right-click in a pane opens split/close/right-click-passthrough; this works even when the program captured the mouse, since almost no program uses the right button.
- A vertical divider is followed by a blank column so text does not touch it. Inactive panes are dimmed (configurable). A left click on an inactive pane only focuses it. Dividers drag like column borders.
- A pane can end up with no room (a divider dragged to the edge over a nested split, or a small client). Its PTY is still sized at least 1×1, and mouse events for it are dropped (`pane_cell` returns `None`), never clamped into an empty range: `clamp` panics when `min > max`, and a panic ends the server and every shell.

**Search (`search.rs`)**
- One global search across groups, projects, workspaces (label and branch) and tabs (label and their workspace's keys). Ranking: exact, prefix, substring; ties by kind (group, project, workspace, tab) then sidebar order. A group result expands it and activates its first project. Results are recomputed on every key and draw; the query is never kept after closing.

**Folder picker (`picker.rs`)**
- Starts in the parent of the active project. `Enter` goes into the selected folder, or opens the current one when nothing is selected. Typing a path walks it; a paste goes through the same path (e2e opens projects this way). Folders are read on the main thread.

**Worktrees (`worktree.rs`)**
- Only when the project folder is a repo root. The name is the branch; the slugged name is the folder, under `<worktrees folder>/<repo>/<slug>` (`~/.cornercase/worktrees` by default). `git worktree add` runs on a thread and answers with an `AppEvent`.
- Every linked worktree of the repo shows up, also ones created outside cornercase (`App::refresh`, throttled to once a second).
- Removing asks first, runs `git worktree remove` (offering `--force` when git refuses) and never deletes the branch.
- `.worktreeinclude`: files matching it **and** ignored by git are copied into a new checkout. Matching is delegated to git (`ls-files`, `check-ignore`).
- **Commits to pull** (`↓n` at the end of a workspace row, before the `×`): every 3 s a thread per project counts `HEAD..@{upstream}` in each workspace and answers with `AppEvent::Behind`; it runs `git fetch --all` first once `fetch_minutes` (default 5, 0 turns all of it off) have passed. One thread per project at a time. Git never prompts (`GIT_TERMINAL_PROMPT=0`, empty `GIT_ASKPASS`, `SSH_ASKPASS_REQUIRE=never`); failures count as 0.

**Changes panel (`changes/`, `ui/changes.rs`)**
- Git workspaces only: ` changes n ` next to ` issues ` (` ± ` in the compact bar) opens a fourth column right of the pane (full screen in compact mode). Its border drags; its width is `Widths::changes` (`None` = half the free space, at most `DEFAULT_WIDTH`).
- Tabs: uncommitted (`git diff HEAD` + untracked), commits (`git diff <merge-base> HEAD`), all (`git diff <merge-base>` + untracked). The base is `Workspace::base` (saved per workspace) or the default branch: `origin/HEAD`, else `main`/`master`. Comparing from the merge-base keeps commits made on the base later out of the diff.
- Git runs on a thread, one job at a time, every second while open and every 3 s while closed (for the button's count). Flags matter: `--no-optional-locks` and `-c diff.autoRefreshIndex=false`, because a plain `git diff` rewrites `.git/index` (takes `index.lock`) even with `--no-optional-locks` and would fight an agent's `git commit`. `--relative` keeps paths inside the workspace folder. Untracked files are read directly (never `git add -N`), capped at 1 MiB, NUL in the first 8000 bytes means binary; a symlink shows its target path, like git, never the file it points to. The branch list and "unchanged lines" also run on threads and answer with `AppEvent::Branches` / `AppEvent::Gap`.
- The job hashes git's output; an unchanged hash answers without a diff, and files whose patch section did not change are reused, so highlighting runs only for what changed. syntect costs ~150-200 µs per line, far more than computing the diff.
- Rendering: lines paired delta-style (word tokens, changed share ≤ 60 %) get word emphasis; each hunk side (context + removed, context + added) is highlighted separately. Tints blend the host background with palette red/green when the client has `COLORTERM=truecolor` and the background is known; otherwise palette colours, and on dark themes only the `▎` bar, coloured numbers and word emphasis (palette greens and reds are too loud as line backgrounds).
- Hunk actions on hover: open (new tab running `/bin/sh -c 'exec ${VISUAL:-${EDITOR:-vi}} "+$1" "$2"'`), ask agent (pastes `path:lines ` into the workspace's agent pane, found with `agents::detect`; without one it copies the reference), copy (the hunk as a patch, OSC 52). Viewed marks are keyed by the file's patch digest, so they clear when the file changes.

**Issues (`issues/`)**
- `Browser` is pure state that returns `Action`s; `App` does the I/O on threads. Answers carry their query and issue key so stale ones are dropped. Lists are cached in memory and in `issues.json` (titles and metadata only, never tokens).
- GitHub goes through `gh` in the project folder. Shortcut (REST v3) and Linear (GraphQL) go through `ureq`, capped at 100 issues. People filters go into each tracker's query.
- Tokens come from `SHORTCUT_API_TOKEN` / `LINEAR_API_KEY` or are typed in the app, checked, and saved to `secrets.json` (0600). `issues::Secret` hides them in `Debug`; they are never logged.
- **Starting an issue**: in a repo root, a worktree on `issue-<n>-<slug>` / `sc-<n>-<slug>` / `ENG-123-<slug>`; elsewhere a new tab. Shortcut and Linear issues ask which open project or workspace to use. Then the agent starts with the prompt.
- **Copy URL** uses OSC 52 through `App::host_writes`, because the server may run on another machine.
- Markdown renders with `pulldown-cmark`; code blocks are highlighted with syntect (pure Rust `regex-fancy`), mapping scopes to palette colours so the terminal theme applies. Highlighting is cached and warmed on the reading thread; syntect and its regex crates build with `opt-level = 3` in dev.

**Agents (`agents.rs`, `launch.rs`)**
- Config: `agent` (`auto` = the one running in the tab, else ask), `agent_args`, `agent_modes`, `agent_commands`, `prompt` (placeholders like `{url}`, `{key}`, `{title}`), `submit`, `auto_accept_trust_prompt`, `trust_prompt_pattern`.
- `launch::Launch` types the command once the shell is quiet, waits until the agent is in the foreground and the screen is quiet, answers the trust prompt, then pastes the prompt. Readiness is a heuristic.
- The trust prompt may highlight "No" by default (Claude Code does), so `answer_keys` moves the selection to the "yes" option before pressing Enter.

**Agent status (`activity.rs`)**
- Every 500 ms (`WATCH_AGENTS_EVERY`, from `App::refresh`) each pane whose foreground process is Claude Code (`agents::detect` on its arguments) gets an `Activity`: working, waiting or idle. The source is the file Claude Code keeps for each running process, `<CLAUDE_CONFIG_DIR or ~/.claude>/sessions/<pid>.json`, the registry behind `claude agents --json` (its documented interface for status bars): `status` is `busy`, `waiting` (permission prompt, question, dialog), `idle` or `shell` (turn over but a Bash it started with `run_in_background` still runs). `shell` counts as working, like Claude's own agents view: Claude wakes up when the command ends, so it was not done (a dev server left running keeps the tab `◐`). A Monitor that runs a command is a background Bash too, so it gives `shell` (checked live on 2.1.289); one on a WebSocket runs no shell and may leave it `idle`. Its fields are not documented, so a missing file or an unknown value falls back to the title Claude sets (`◐`/`◑`, or braille in older versions, while busy; `✳` otherwise; it stays `✳` when Claude sees `TMUX`, `STY` or `ZELLIJ`), then to idle.
- No hooks and no screen scraping. Hooks mean editing the user's `~/.claude/settings.json` (or shipping a plugin), `Stop` never fires on an Esc interrupt and the permission notification comes ~6 s late; Claude's screen changes too often to match. The session file turns idle within a tick of an interrupt and waiting as soon as a prompt shows.
- `Status` adds done: a pane that went from working or waiting to idle while its tab was not the visible one (`App::visible_tab`: the active tab, unless the compact menu covers the pane) is done until that tab is shown. `watch_agents` clears it for the visible tab on every `refresh`, not only every 500 ms, so a click shows `○` at once. An unknown state counts as idle, so a fresh Claude never looks done.
- A tab shows its most urgent pane (`Tab::status`, the order of `Status`): waiting `!` > done `✓` > working `◐` > idle `○`, before the name. Workspace, project and collapsed group rows show only what needs you (`activity::attention`: waiting or done), right-aligned and before `↓n`; working stays on the tab so the sidebar is not always full of icons. The compact `≡` shows the same for every tab but the visible one (`View::attention`).
- Only Claude Code for now; another agent plugs in by returning an `Activity` from its own signals. A Claude running through `ssh` or in a container is not seen (no local process, no local file).
- **Notifications**: `Pane::update` returns `Waiting` or `Done` once that status has held for `NOTIFY_AFTER` (1 s) and its tab was not shown meanwhile, once per change, so a prompt answered at once stays quiet. `App::notify` sets a toast with the status icon (`claude needs you in <project> › <workspace>`, control characters stripped) and queues a `notify::Notification`, which the server encodes per client. Panes that settle on the same status in the same workspace on one tick give one notice: the text would be the same, and Ghostty drops a repeat within 5 s anyway. They go through the outer terminal like OSC 52, so they work over SSH (no `notify-send`/`osascript` on the server's machine).
- The channel is the client's: inside tmux, screen or Zellij it is BEL (tmux drops notification OSCs, passthrough is off by default, BEL goes through); otherwise `notify::detect` trusts the XTVERSION reply when there is one (asked with the colours, so it survives SSH, where only `TERM` arrives; env vars may also be inherited from another terminal), else `TERM_PROGRAM`, `LC_TERMINAL` (iTerm2 sets it for SSH), `TERM`, `KITTY_WINDOW_ID`, `KONSOLE_VERSION`. OSC 777 for Ghostty, WezTerm, foot, Konsole, Warp and Rio; OSC 9 for iTerm2 (its only one); OSC 99 with `o=unfocused` for kitty, Contour and VS Code (VS Code knows only 99); BEL otherwise. Never several, since terminals that know several would show each.
- OSC 9 and 777 end with BEL, which every parser accepts; OSC 99 with ST, as kitty's spec says. `;` in the text becomes `,`: WezTerm, Rio and Konsole cut the body at it, WezTerm drops an OSC 9 holding one. Ghostty on macOS, foot, Konsole and Warp hide notifications while their window is focused; the others show them. `desktop_notifications` (`auto`, a channel, `off`) overrides the channel; the toast always shows. The visible tab never notifies, even while the outer window is unfocused (that would need focus reporting, `CSI ? 1004 h`).

**Context use (`context.rs`)**
- A tab running Claude gets a second line, `Opus 5.5 · 23%`: the model and the share of the window used, as Claude's statusline computes it (`round(used / window * 100)`, at most 100). `sessions/<pid>.json` gives `sessionId` (re-read every tick: `/clear` changes it) and `cwd`; the transcript is `projects/<cwd with every non-alphanumeric UTF-16 unit as '-'; past 200 characters cut, plus '-' and a base36 hash>/<sessionId>.jsonl`. Its last `assistant` line (not `isSidechain`, model not `<synthetic>`) gives `message.model` and used = `input_tokens + cache_creation_input_tokens + cache_read_input_tokens`; a later `system`/`compact_boundary` hides the line until the next reply.
- The transcript is read as it grows (the first time, its last MiB), lines scanned backwards to the first reply or boundary, so a quiet tick costs one `stat`. The window is worked out again only when the reply changes.
- No file holds the window, so it mirrors Claude Code 2.1.289's own rule: `DISABLE_COMPACT` with `CLAUDE_CODE_MAX_CONTEXT_TOKENS` gives that; otherwise 1M for models with a native 1M window (Opus 4.7 and later, anything from version 5) or a model setting ending in `[1m]` that names the transcript's model (`--model`, `ANTHROPIC_MODEL`, then `model` in local > project > user settings), unless `CLAUDE_CODE_DISABLE_1M_CONTEXT`; `CLAUDE_CODE_MAX_CONTEXT_TOKENS` for non-Claude models; else 200k, or 1M once past 200k. Variables are the process's (`process::env`) overlaid by each settings file's `env`, as Claude applies them. Not seen: an unsaved `/model` switch, accounts without 1M, the SDK's 1M beta header. Claude's statusline input has the exact numbers but needs a `statusLine` command in the user's settings (rejected like hooks).
- Wide mode gives the tab two rows (`ui::tab_lines`, height `pitch.max(lines)`), with `×` on the first only; compact mode puts the line on the band's third row. The model is cut first and dropped below 4 columns; the percentage turns orange at 75 % and red at 90 % (`usage::Severity::of`).

**Plan usage (`usage.rs`)**
- ` usage ` sits under ` settings ` (wide: its own row, so the workspaces column keeps two blank rows to keep its separator and ` issues ` level; compact: the footer is settings | usage | quit in thirds). It opens `Overlay::Usage`; ` done `, `Esc` or `Enter` close it.
- The numbers come from Claude Code's SDK control protocol: `claude -p --setting-sources '' --no-session-persistence --input-format stream-json --output-format stream-json --verbose`, then an `initialize` and a `get_usage` control request on stdin; the `control_response` for `get_usage` holds `subscription_type`, `rate_limits_available` and `rate_limits` (`limits[]` with `kind`, `percent`, `severity`, `resets_at`, `scope.model.display_name`; `spend` for extra usage). No prompt, no tokens, ~3 s. Claude handles OAuth (credentials file, Keychain, refresh), so cornercase never reads or refreshes a token; `GET /api/oauth/usage` was rejected for that (and refreshing the token ourselves can log Claude out). `--setting-sources ''` keeps the user's hooks from firing, `--no-session-persistence` keeps the probe out of the session list, `--bare` does not work (API key only).
- The protocol is not documented as a CLI interface, so parsing is lenient (unknown fields and kinds pass, a missing `limits[]` falls back to `five_hour` / `seven_day`, an unknown `severity` comes from the percentage), but `rate_limits_available` must be there, and `rate_limits` too when it is true, so a changed answer shows as unavailable instead of "no plan limits". Every failure shows `usage unavailable: <reason>`.
- On demand only: opening the modal starts one probe on a thread (never two at once) that answers with `AppEvent::Usage`; nothing polls. The last answer stays in memory (not in the session file) and shows with its age while a new one loads. The probe runs on the server's machine, uses `agent_commands.claude`, strips `activity::CLAUDE_SESSION_ENV` like `Term::spawn`, inherits `CLAUDE_CONFIG_DIR`, and is killed after `usage::TIMEOUT` (10 s; `App::usage_timeout` in tests).
- Reset times are relative (`resets in 2h 13m`, `3d 4h`): local time would need a time zone database, and the server may sit in another zone than the client anyway.

**Settings (`config.rs`, `settings.rs`)**
- Tabs: Worktrees, Agents, Issues, TUI. Every change is saved to `config.json` at once. File: `$XDG_CONFIG_HOME/cornercase/config.json`, or next to the socket when `CORNERCASE_SOCKET` is set (so tests never touch the real one). Missing keys take defaults; a corrupt file means all defaults.

**Terminals (`term.rs`, `emulator.rs`)**
- The emulator is libghostty-vt: it answers terminal queries (DSR, DA, DECRQM, kitty keyboard…) that programs like fzf and nvim wait for, and reflows on resize.
- Its types are `!Send`, so the emulator lives on the server's main thread; the reader thread only forwards bytes. Query replies go out from `on_pty_write`, ordered with the output that asked.
- Cells keep palette indices so the outer theme applies. The client asks the outer terminal for its colours (OSC 10/11/4, then XTVERSION, then DA1 as an end marker) before starting the input thread, and every emulator uses them as defaults.
- cwd, name and arguments of the PTY's foreground process group leader come from `process.rs`: `/proc` on Linux, `proc_pidinfo` / `proc_name` / `sysctl(KERN_PROCARGS2)` on macOS (with `getpeereid`, the only `unsafe` and the only use of `libc`). In a pipeline the leader may be dead (`process::alive`), so the shell is used instead. Agent launch and detection depend on it.

**Keys and mouse (`keys.rs`, `mouse.rs`)**
- Special keys, and all keys once the program enabled kitty, go through Ghostty's encoder; a legacy encoder handles the rest. The client asks the outer terminal for kitty's disambiguate level, so `Ctrl+Enter` / `Shift+Enter` keep their modifiers.
- Pane mouse events are forwarded only when the program enabled a mouse mode, in its encoding, relative to the pane, clamped to its edges.
- **Selection:** when the program did not ask for the mouse, press-drag-release selects with Ghostty's selection and copies via OSC 52, with a toast. Programs' own OSC 52 writes are forwarded to the outer terminal too (reads and primary-clipboard writes are dropped).

**Client / server (`client.rs`, `server.rs`, `protocol.rs`)**
- The server owns the PTYs so shells outlive the UI. The client starts it if needed (`cornercase server`, logging to `server.log` next to the socket).
- One server per socket, guarded by `flock` on `server.lock`. The server calls `setsid` and ignores SIGHUP.
- The server renders, the client only writes frames. Input travels as serialized crossterm events. Several clients mirror each other; the shared size is the last used client's (attach, key, paste or mouse other than a bare move), like tmux's `window-size latest`, so a hung client (a phone whose SSH dropped) never shrinks a new one. Ping/pong would not catch that: the hung client is a healthy local process. Smaller clients get the frame cropped by `CropBackend`.
- Every shell gets `CORNERCASE=1`; a client seeing it refuses to start (no nesting).
- Every shell loses Claude Code's per-session variables (`activity::CLAUDE_SESSION_ENV`): a server started from inside Claude Code would otherwise make every pane's `claude` a child of that session and expose its messaging token. An explicit list, not a `CLAUDE_CODE_*` prefix, so user settings like `CLAUDE_CODE_USE_BEDROCK` and `CLAUDE_CONFIG_DIR` pass through.
- `Hello` carries a protocol version and build id, and the notification channel of the client's terminal; a server from another build rejects the client. Keep `ClientMessage::KillServer` and `ServerMessage::Rejected` as the first variants (`protocol::tests::compatibility`).
- Socket: `$XDG_RUNTIME_DIR/cornercase/server.sock` or `$TMPDIR/cornercase-<uid>/server.sock`; `CORNERCASE_SOCKET` overrides it. Paths must fit in 108 bytes.
- **The socket's folder must be the user's alone** (`protocol::check_socket_dir`): a real folder, not a symbolic link, owned by the user and not writable by group or others; it is created `0700` when missing. The client checks it before connecting or starting a server, `kill-server` before connecting, the server before binding, also under `CORNERCASE_SOCKET` (a name without a folder is checked in the current folder). The refusal suggests removing the folder only on the default path, which is cornercase's own; under `CORNERCASE_SOCKET` it asks to change the variable, since that folder may hold other files. Without it, another local user could create `/tmp/cornercase-<uid>` first (no `XDG_RUNTIME_DIR` nor `TMPDIR`, as in some SSH sessions and containers) and pose as the server: read every key and send a `Restart`. Read access for others is allowed, since connecting needs write access to the socket file.
- Both ends check the peer's uid (`protocol::check_peer` on `process::peer_uid`: `SO_PEERCRED` on Linux, `getpeereid` on macOS): the client refuses a server of another user, and the server drops another user's client, which a loose umask could otherwise let in.

**Updates (`update.rs`)**
- Release builds only (`debug_assertions` off), so `cargo run` and tests never call GitHub. The server asks `releases/latest` once a day (`check_updates` in config); a newer one shows ` ↑ x.y.z ` at the end of the settings row and a toast. The dialog shows the `## Release Notes` part of the release body (from `CHANGELOG.md`), scrolled by wheel or ↑/↓.
- Updating downloads `cornercase-<target>.tar.gz` and its `.sha256`, unpacks with `tar` next to the binary, runs `--version` on it (a binary that cannot run here never replaces a working one), then renames it over the old one. Homebrew installs (`/Cellar/`…), a folder we cannot write and unknown platforms get a command to copy instead.
- The running server keeps the old code. ` restart now ` saves the session, sends `ServerMessage::Restart(path)`, and each client `exec`s its own executable (`current_exe`, read at start; the update replaced that file), never the path in the message, which starts a new server that restores the session. A new client rejected by an older server asks `[y/N]` on the plain terminal before running `kill-server`.

**Saved session (`state.rs`)**
- Groups (name, icon, colour, collapsed), projects (with their group's index), workspaces (with their changes base), tabs, panes (cwd), split layouts, custom names, active children, column widths (and the stacked line) and the changes panel (open, tab), in `$XDG_STATE_HOME/cornercase/session.json` (or next to the socket). Processes are not restored; each pane gets a new shell in its folder.
- Saved only once the state is stable for 2 s, so the burst of `Exited` events at logout does not save an empty session.
- Only a fresh server restores, on its first client's `Hello`. Missing folders are skipped. `VERSION` is 4; older versions are migrated (a version 3 file is read as is, every project ungrouped).

## Tests

- Unit tests sit next to the code in `mod <unit_of_work>` with sentence-like names; tabular cases use `rstest` with named `#[case::...]`.
- UI: render a `View` into `TestBackend`; `insta` snapshots for layout, cell styles for hover.
- App tests `click` with a press and a release (rows act on release); drags start with `press`.
- `term.rs` / `app.rs` tests spawn real `/bin/sh` PTYs (never the user's shell) and wait with `test_util::wait_until`, never sleeps. `/bin/sh` is `bash` on macOS, so tests check its name with `test_util::is_sh`. `TempDir` paths are canonical, because macOS' temp dir is behind a symlink (`/var` → `/private/var`).
- Helpers: `test_util::TempDir`, `git_repo`, `fake_gh`, `FakeHttp` (canned HTTP), `write_executable` (through a `/bin/sh` child to avoid `ETXTBSY`). Nothing calls real `gh`, Shortcut or Linear. App tests clear `App::env_tokens` and never use the real config.
- Agents are faked with a script (`FAKE_AGENT`) that asks a trust question and echoes what it reads.
- Agent status is faked with a script named `claude` that writes its own `sessions/$$.json` (and, for the context line, a transcript under `projects/`); app tests point `App::claude_dir` at a temp dir, and e2e sets `CLAUDE_CONFIG_DIR` per `Session`, so nothing reads the real `~/.claude`. Tests that read a process's environment spawn `/bin/sleep` with a cleared one and wait until its arguments are `sleep`'s (before `exec`, `/proc` shows the parent's).
- The usage probe is faked with a `claude` script that answers the control requests, set through `agent_commands`; no test runs the real `claude`.
- `tests/e2e.rs` runs the real binary in a PTY (`SHELL=/bin/sh`, `PS1='$ '`), parses output with `vt100`, sends raw bytes and SGR mouse sequences, and answers the startup colour query. Each test gets its own server through a `Session`; dropping it runs `kill-server`.
- Avoid races in e2e: wait for output that proves the previous step finished (`echo cat-""starts; cat -v`).
- A safety-net test must fail without the code it protects.

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
- The terminal on the landing page is a simulation in TypeScript (`src/lib/demo/`) that mirrors `ui.rs`: same layout, labels and colours, with fake shells, agents and issues. When `ui.rs` changes, update the simulation too. The same code renders the feature pictures to SVG at build time (`scenes.ts`).
- The hero plays a tour in chapters (`boot.ts`) on the simulation's virtual clock, so a chapter can be fast-forwarded and the clock sped up while detached. It finds what to click by its text (`issues`, `#482`, `quit`, `changes`), so check it still plays to the end after changing the simulation.
- The speed numbers on the landing (`Stats.astro`) and in the FAQ (`Is it fast?`) are a dated snapshot (October 2026, the versions the FAQ names), exempt from keeping the site in sync: never update them as part of other changes; re-measure only when asked, then change both places together.
- Docs screenshots are real: `src/screens/*.ansi` are `tmux capture-pane -e -p -N` dumps of the app, run with a fake `HOME` and its own `XDG_RUNTIME_DIR` (a separate server that still shows the default paths), rendered to SVG at build time by `src/lib/term/`. Box-drawing, block and a few symbol characters are drawn as shapes, not font glyphs, so lines join. Re-capture them when the UI changes.
- Base URL and origin come from `actions/configure-pages` (`SITE_BASE`, `SITE_ORIGIN`). The site lives on the custom domain `usecornercase.dev` at `/` (DNS in DigitalOcean: GitHub Pages A/AAAA records on the apex, `www` CNAME to `usecornercase.github.io`); the old github.io address redirects there. Local builds default to the same origin and base.

## Known limitations

- No scrollback navigation. Shells do not survive the server: after `kill-server` or a reboot, panes come back as new shells.
- Host colours are read once; a theme switch is not seen.
- Inner programs never get key releases.
- Runs on Linux and macOS only.
