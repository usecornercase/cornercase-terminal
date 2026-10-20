# Architecture

```
src/main.rs       hands argv to cli.rs (uses anyhow)
src/lib.rs        the `pub mod` lines, in the five groups below: shared leaves, domains, view, app, processes

src/error.rs      library error type
src/gh.rs         running gh, GitHub's CLI
src/http.rs       HTTP requests and downloads for issue trackers and updates
src/list.rs       list cursors and search queries with a selection and scroll
src/log.rs        server.log: levels, the line format, the writer thread and its rotation, `Job` timings, `cornercase logs`
src/panics.rs     containing panics: `catch_unwind` wrappers for the server loop and background jobs, the hook that logs them
src/paths.rs      socket and lock paths, and every file cornercase keeps
src/persist.rs    safe JSON writes, backups and the Saver that writes once the state settles
src/process.rs    a pid's cwd, name, arguments, environment, descendants and memory footprint: /proc on Linux, libproc and sysctl on macOS; a socket peer's uid; stopping and reaping a child
src/text.rs       truncation, displayed paths, folder names, breadcrumbs, one-line text and hex
src/time.rs       timestamps, calendar dates and ages
src/version.rs    the binary version, version comparison and build id

src/accounts/     agents' login folders (links.rs: the settings they share) and a pane's Accounts from a profile
src/agents.rs     launch catalogue, shared modes and arguments, which agent takes an issue, detection, trust prompt, AgentState for resuming a pane
                  agents/known.rs Known, the supported identities and exhaustive dispatch to each agent folder
                  agents/activity.rs shared activity and status, done-but-unseen, pending marks, notifications, rollups; agents/launch.rs starting an agent in a new tab (pure state machine)
                  agents/awake.rs keeps the machine awake; agents/stats.rs counts, heatmap, share links; agents/tree.rs the agents section's tree
                  agents/context.rs pane state, Look, Reader, Record dispatch; agents/context/ shared discovery, JSONL reads, counting, last-message reads; agents/usage/ plan probes and modal state
                  agents/claude/ reader, status, title (mod.rs), transcript (transcript.rs), last message (message.rs), input box and survey (screen.rs), plan usage (usage.rs), fake
                  agents/codex/ reader, title (mod.rs), rollout (rollout.rs), last message (message.rs), plan usage (usage.rs), fake; agents/gemini/ reader, title (mod.rs), session (session.rs), fake
                  agents/opencode/ reader and database (database.rs), fake; agents/grok/ reader and session files (session.rs), fake; agents/cursor/ reader, chat store (store.rs), protobuf reader (proto.rs), fake; agents/copilot/ reader, session lock (session.rs), events (events.rs), last message (message.rs), fake
src/changes/      changes panel: mod.rs (panel state, refresh pacing, folds, viewed, branch picker, tints), git.rs (git commands, base, merge-base), diff.rs (patch parser, word emphasis, highlighting), filter.rs (which files a path filter keeps), every.rs (the Every project targets and grouping)
src/config.rs     user settings (config.json), Sidebar and Tabs choices, `~` expansion, validation
src/files/        files panel: mod.rs (open folders, viewer, search bar and mode, jobs, git marks), disk.rs (one folder's listing with `ignore`, reading a file), search.rs (the file index, names with nucleo, text with ripgrep), link.rs (the path under a pane click and where it points)
src/flows/        flows: mod.rs (Flow, roles, stages, validation, the line that starts a role), store.rs (TOML files), builtin.rs, folder.rs (the run's files), run.rs (members, messages), progress.rs (where a run stands), brief.rs (the orchestrator's brief), share.rs (export and import), dialog/ and start/ (dialog models)
src/git.rs        branch from .git/HEAD, repo roots, linked worktrees (no git process); git/worktree.rs `git worktree add`/`remove`, checkout path, `.worktreeinclude`; git/worktree/setup.rs setup wrapper, teardown; git/upstream.rs `git fetch` and commits to pull per workspace (`↓n`)
src/graphics/     images in the files panel: mod.rs (Support, Missing, Picture), detect.rs (which protocol a client's terminal shows), decode.rs, encode.rs (fit, payloads), kitty.rs, iterm.rs, sixel.rs, tmux.rs (passthrough)
src/highlight.rs  syntax highlighting of fenced code (syntect scopes -> palette colours), cached; highlight/syntax.rs tree-sitter highlighting through arborium: which grammar a file gets, captures -> palette colours; highlight/markdown.rs Markdown -> wrapped ratatui Lines
src/host/         the outer terminal: host/theme.rs asks it for its colours and its name (XTVERSION), host/notify.rs desktop notifications through it, host/pointer.rs the resize pointer (OSC 22), host/clipboard.rs OSC 52
src/issues/       issue model and clients: github.rs (gh CLI), gitlab.rs (REST), shortcut.rs (REST), linear.rs (GraphQL), jira.rs (REST; jira/adf.rs turns ADF into Markdown), plane.rs (REST), asana.rs (REST), html.rs (HTML into Markdown, for Plane and Asana), md.rs (Markdown helpers they share), browser.rs (modal state, sizes and hits; app/issues_dialog.rs builds its view) and browser/ (buttons, token_form, people, picker, naming, input, view), cache.rs (lists on disk), issues/secrets.rs (the trackers' tokens in secrets.json, 0600)
src/plugins/      manifest.rs, registry.rs (plugins.json), run.rs (commands, env, log), supervise.rs (services, window processes)
src/project.rs    Project > Workspace > Tab > panes, per-level lookups, labels, removal, moving
                  project/group.rs holds Group, GroupEntry, icons, colours, presets and saved-icon normalization; project/hierarchy.rs holds Hierarchy (lookups by id and walks across levels), group_indices and sidebar_order
                  project/split.rs a tab's split tree; project/search.rs global search candidates and ranking; project/picker.rs folder picker state; project/vscode.rs VS Code `.code-workspace`; project/settings.rs setup/teardown form
src/pull_request.rs a branch's pull request and its checks (`gh pr view`)
src/settings.rs   the settings modal's state and plain row/edit/pick texts; returns Actions for App; settings/activate.rs what choosing a row does, submit.rs finishing a pick or an edit, trackers.rs the Issues page's tokens and tabs, input.rs keys, paste and capturing the prefix key, view.rs row texts
src/shortcuts.rs  the prefix key (parse, match, clashes) and the keys menu's tree of actions
src/state.rs      the saved session as JSON, its Widths schema, migrations and the mapping to and from the hierarchy (`ProjectState::of`/`restore`)
src/messenger/    chat bots: Via, orders, buttons, link.rs, markup.rs
src/telegram/     Telegram: Bot API, updates, html.rs, link.rs (polling)
src/discord/      Discord: REST, gateway.rs (WebSocket), markdown.rs
src/term.rs       a shell in a PTY, its Emulator, and the reader thread; term/emulator.rs wraps libghostty-vt and takes plain Snapshots for ui
                  term/status.rs OSC 7501 reports, records and lifetimes; term/status/stream.rs bounded escape parsing
                  term/keys.rs KeyEvent -> bytes; term/mouse.rs MouseEvent -> bytes; term/memory.rs how much memory a pane uses
src/todo/         the TODO list: mod.rs (ordering, undo, todos.json, panel state), editor.rs (a text field with a cursor, soft wrap, line breaks)
src/worktrees/    Worktrees dialog: mod.rs (list), dialog.rs (state, keys), discover.rs, scan.rs, remove.rs
src/update.rs     update check against GitHub releases, download, checksum, binary swap; update/restart.rs what a server restart stops: running programs from a status Report, the confirmation texts

src/ui.rs         `View`, `draw()` and the re-exports of `ui/`; the rest of the screen is one file per part (no PTYs): ui/theme.rs colours, ui/widgets.rs shared drawing helpers, ui/layout.rs and ui/widths.rs the areas and column widths, ui/rows.rs the list engine, ui/sidebar.rs, ui/workspaces.rs, ui/tree.rs and ui/agents_section.rs the lists, ui/drop.rs where a dragged row lands, ui/chrome.rs footer, compact bar and borders, ui/pane.rs the pane, ui/toast.rs, ui/dialog.rs and ui/dialog/ every dialog (project_settings.rs, worktrees.rs); ui/changes.rs draws the changes panel, ui/todo.rs the TODO panel, ui/files.rs the files panel, ui/remote.rs the reconnecting notice, ui/tab_bar.rs the tab bar above the pane (geometry and drawing), ui/activity.rs the icon bar and ui/icons.rs its glyphs, ui/keys.rs the keys menu, ui/stats.rs the Stats page of the usage dialog, ui/remote_access.rs the ` Remote ` button and its QR dialogs, ui/flow.rs and ui/flow/ the flow diagram and the Flow panel

src/app.rs        App state (a group per concern, see Code layout), new, server-facing methods, refresh and event dispatch, drawing the View; re-exports the event and image types
src/app/          event.rs (AppEvent, Start), overlay.rs (overlay state, view and clicks), dialogs.rs (forms and confirmations), toast.rs, notifications.rs (notification clicks), menus.rs (row and pane menus), worktrees_dialog.rs
                  input.rs (keys, mouse and paste), interaction.rs (drags and timing), jobs.rs (`spawn_job`, the one way to start a background job), layout.rs (focus and side panels), view.rs (View entries)
                  hierarchy.rs (active lookups, closing, renaming, groups and moving), tabs.rs (shells, tabs, workspaces and projects), session.rs (restoring and resuming)
                  sidebar.rs (lists, row menus and agents section), sidebar/drag.rs (row drags), sidebar/tree.rs, tab_bar.rs, panes.rs (pane mouse, scrollback, selection and dividers), links.rs (paths in panes)
                  clock.rs (the spinner's phase and the slow refresh's pace), outbox.rs (what the server takes after a step: copies, notifications, sounds, opens, and the detach, restart and start requests), agent_watch.rs (status, stats samples, notifications and keeping awake), launches.rs (pending launches and typing into agents), worktrees.rs (creating, checking, removing, upstream counts), worktree_setup.rs, project_settings_dialog.rs, pull_requests.rs, actions.rs (editor and agent actions)
                  changes_panel.rs and changes_panel/branches.rs, files_panel.rs, todo_panel.rs, flow_panel.rs (and recording a run's messages); flow_runs.rs (starting and restoring runs), flows_dialog.rs, start_flow_dialog.rs; groups.rs (the groups several concerns share: Screen, Git, Messages, Panels); issues_dialog.rs, update_dialog.rs, picker_dialog.rs, search_dialog.rs, settings_dialog.rs (builds the settings view), usage_dialog.rs
                  control.rs (the request switchboard and target resolution) and control/ (wait.rs waits, idle_restart.rs restart-when-idle, create.rs, talk.rs sending and reading, close.rs, arrange.rs renaming and TODOs, status.rs, flow.rs, contract.rs), events.rs (event subscribers), shortcuts.rs (keys menu actions), trace.rs (changes for logs and events), images.rs (placement and encode jobs), remote_access.rs (browser access), plugins.rs, messenger.rs and messenger/ (pairing.rs, away, notices, replies, /new)

src/cli.rs        `run` and the dispatcher of every command; the commands for scripts and agents send a request and print the answer; cli/args.rs the clap definitions, cli/help.rs their help texts, cli/status.rs the `status` output, cli/update.rs `update`, `restart` and `kill-server`, cli/read.rs `read`, cli/flow.rs `flow`, cli/contract.rs `contract`, cli/plugin.rs `plugin`, cli/start_server.rs
src/client.rs     the UI process: entry points, terminal setup/teardown; client/window.rs the attached window, client/probe.rs the colour query and what the outer terminal can show, client/requests.rs a command talking to the server, client/server_process.rs starting and stopping the server; client/input.rs reads input and filters late replies; client/input/parse.rs decodes keys, mouse and paste
src/control.rs    the JSON of those requests and answers, shared by the commands and the server; CORNERCASE_PANE; control/contract.rs the versioned contract and error codes; control/fields.rs the logged fields of each command
src/protocol.rs   messages and length-prefixed postcard framing
src/remote.rs     `cornercase remote` (ssh, its pipes, the proxy's banner and version check) and `cornercase proxy` (stdio <-> the socket)
src/server.rs     the daemon: owns App and every Term, accepts clients on a Unix socket, draws a ratatui frame per client; server/backend.rs the ratatui backend that writes a client's frame, cropped to what it shows; server/images.rs each client's image state (kitty ids, iTerm2/sixel re-sends); server/startup.rs the restore
src/web/          browser access: mod.rs (listener, routes, pairing), bridge.rs (a WebSocket as a socket client), http.rs, auth.rs (pairing tokens, browsers.json), page.rs and assets/ (the page, vendored xterm.js), web/tailscale.rs (`tailscale serve`)

skills/cornercase/SKILL.md  the agent skill, embedded for `cornercase skill`
```

**Server loop:** the accept thread, one reader thread per client, the signal thread and a forwarder for PTY output all send `ServerEvent`s over one `mpsc` channel. The loop waits for an event or a 500 ms tick, drains the queue, then draws once per client when something on screen may have changed (`Server::due`). Each client has a writer thread so a slow one never blocks the loop. **Client:** the main thread writes each `Frame` to stdout; an input thread sends every crossterm event.

`ui::draw` takes a `View`, not `App`, so rendering is testable with `TestBackend`. Geometry functions (`ui::layout`, `entry_row`, `workspace_row`, `form_buttons`, `picker_item`, …) serve both drawing and hit testing, so tests take positions from them.
