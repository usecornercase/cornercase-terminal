# Design decisions

Why cornercase works the way it does, area by area. The rules every change follows are in [AGENTS.md](AGENTS.md), including when to read and update these notes. This file is their index: the notes are in [`design/`](design/), one file per area.

## How to use it

- Each line below gives an area's file, the code it covers and what it decides. Paths are under `src/` unless they name another folder; `#N` is an issue or a pull request.
- A big area is also a folder, `design/<area>/`, of topics, listed at the end of the area's file.
- The notes name other areas and topics ("see Agent status", "see Views of your own"): look the name up in this list, or in the topics of an area.
- A new area gets its file and a line here.

## What goes in an area file

- What was decided and why, the alternatives that were rejected and why, limits that are not obvious, and what was or was not checked live, with the versions.
- Not how the change was built, raw measurements (keep the conclusion and its date), lists of deleted code, or names and values the code already states.
- A file over about 40 KB is split by topic into `design/<area>/<topic>.md`: the area file keeps what applies to the whole area and lists the topics.

## Areas

- [Architecture](design/architecture.md): what each file and folder of `src/` holds, the server loop, the client.
- [Code layout](design/code-layout.md) (`src/`, `tests/layers.rs`): domains, `pub`, the files of `App`, 1,000 lines per file, `tests/layers.rs`.
- [Layout](design/layout.md) (`config.rs`, `state.rs`, `ui.rs`, `ui/`, `app/layout.rs`, `app/sidebar.rs`, `app/tab_bar.rs`, `app/interaction.rs`, `agents/closing.rs`, `app/close_agent.rs`, `app/close_agent/`, `ui/icons.rs`, `ui/icons/`, #450): the sidebar layouts, the agents section (and its Close agent dialog), borders, bars, text colours, compact mode, the icon bar, the file type icons.
- [Text fields](design/text-fields.md) (`editor.rs`, #454): the one text field with a cursor, ratatui-textarea as its model, one-line and multi-line fields, wrapping, word moves, limits.
- [Dialogs](design/dialogs.md) (`ui/dialog.rs`, `ui/dialog/`, `ui/combo.rs`, `list.rs`, `app/overlay.rs`, `app/dialogs.rs`): borders, dimming, buttons, a click outside, comboboxes.
- [Hierarchy](design/hierarchy.md) (`project.rs`, `project/group.rs`, `project/hierarchy.rs`, `app/hierarchy.rs`, `app/tabs.rs`, `app/menus.rs`, `app/sidebar/drag.rs`): groups, projects, workspaces, tabs and panes; ids, labels, closing, reordering.
- [Splits](design/splits.md) (`project/split.rs`, `app/panes.rs`): the split tree, minimum sizes, hidden panes, moving panes.
- [Search](design/search.md) (`project/search.rs`, `project/search/said.rs`, `list.rs`, `app/search_dialog.rs`, `app/search_dialog/sessions.rs`, `app/control/search.rs`, `cli/search.rs`, #272, #467): one search over names, or over what agents and shells said.
- [Folder picker](design/folder-picker.md) (`project/picker.rs`, `list.rs`, `app/picker_dialog.rs`): where it starts and how it is walked.
- [Worktrees](design/worktrees.md) (`git/worktree.rs`, `app/worktrees.rs`): one worktree per workspace, removal in the background, locks, `↓n`.
- [Worktree setup](design/worktree-setup.md) (`git/worktree/setup.rs`, `project/settings.rs`, `app/worktree_setup.rs`, `app/project_settings_dialog.rs`, `ui/dialog/project_settings.rs`, #250, #485): a project's setup and teardown scripts.
- [Ports](design/ports.md) (`ports/`, `control/ports.rs`, `app/control/ports.rs`, `app/ports_dialog.rs`, `ui/dialog/ports.rs`, `cli/ports.rs`, #496): the TCP ports a workspace's programs listen on, which workspace they belong to, the Ports dialog, stopping them.
- [Processes](design/processes.md) (`processes/`, `process.rs`, `process/escalation.rs`, `app/processes_dialog.rs`, `app/control/processes.rs`, `ui/dialog/processes.rs`, `control/processes.rs`, `cli/processes.rs`, #486): every pane's process tree with its CPU and memory, the Processes dialog, stopping a process and what it started.
- [Worktrees dialog](design/worktrees-dialog.md) (`worktrees/`, `app/worktrees_dialog.rs`, `app/worktrees_dialog/`, `ui/dialog/worktrees.rs`, #256, #485, #536): every worktree on disk with when it last changed and what makes it safe to remove, removing several at once.
- [Pull requests](design/pull-requests.md) (`pull_request.rs`, `pull_request/gitlab.rs`, `pull_request/glab.rs`, `pull_request/stack.rs`, `gh.rs`, `app/pull_requests.rs`, `ui/workspaces.rs`, #251, #402, #448): a workspace's pull request or merge request and its checks, from `gh` or `glab`, and its GitHub stack.
- [Merged pull requests](design/merged-pull-requests.md) (`pull_request/merged.rs`, `pull_request/merged/`, `git/remote.rs`, `app/merged_pull_requests.rs`, #293): the Stats row of merged pull requests, from `gh` and `glab`.
- [Shared files](design/shared-files.md) (`changes/overlap.rs`, `changes/merge.rs`, `app/overlaps.rs`, `app/merge_check.rs`, `app/control/merge_check.rs`, `control/changes.rs`, `cli/changes.rs`, #252): files two workspaces of a project both change, the `⇄` tags, the merge check with `git merge-tree` and its request.
- [Changes panel](design/changes-panel.md) (`changes/`, `ui/changes.rs`, `app/changes_panel.rs`): the git diff, its modes, filter, scope, pacing and digests.
- [TODO panel](design/todo-panel.md) (`todo/`, `ui/todo.rs`, `app/todo_panel.rs`): the TODO list, its requests, sending an item to an agent.
- [Star prompt](design/star-prompt.md) (`star.rs`, `app/star_prompt.rs`, `ui/star.rs`, #276): the card that asks for a GitHub star now and then.
- [Tour](design/tour.md) (`tour.rs`, `tour/steps.rs`, `app/tour_card.rs`, `app/tour_card/`, `app/help_menu.rs`, `ui/tour.rs`, `control/tour.rs`, `app/control/tour.rs`, `cli/tour.rs`, #400): the guided tour on first run, the catalogue of short tours, the Help button.
- [Queue](design/queue.md) (`queue/`, `issues/browser.rs`, `ui/dialog/queue.rs`, `app/queue_panel.rs`, `app/queue_dialog.rs`, `app/control/queue/`): scheduled agent starts, their times, places and form.
- [Files panel](design/files-panel.md) (`files/`, `highlight/syntax.rs`, `ui/files.rs`, `ui/code.rs`, `text/stretch.rs`, `app/files_panel.rs`, `app/links.rs`): tree, viewer, highlighting, search, paths in panes, full screen.
- [Images](design/images.md) (`graphics/`, files panel): pictures in the files panel, by kitty, iTerm2 or sixel.
- [Issues](design/issues.md) (`issues/`, `issues/browser/`, `http.rs`, `app/issues_dialog.rs`): the issues browser, its seven trackers and the issue requests.
- [Agents](design/agents.md) (`agents.rs`, `agents/known.rs`, `agents/{claude,codex,gemini,opencode,grok,cursor}/`, `agents/launch.rs`, `app/launches.rs`): the supported agents, adding one, launching, the trust prompt.
- [Accounts and profiles](design/accounts-and-profiles.md) (`accounts/`, `agents.rs`, `app/tabs.rs`, `app/control/accounts.rs`, `app/accounts_page.rs`, `app/account_menu.rs`, `settings/accounts.rs`, `ui/accounts_bar.rs`, `control/accounts.rs`, `cli/accounts.rs`, #379): agent logins in folders of their own, the switcher, profiles.
- [Agent status](design/agent-status.md) (`agents/activity.rs`, `agents/{claude,codex,gemini,opencode,grok,cursor,copilot}/mod.rs`, `app/agent_watch.rs`): working, waiting, done; notifications and their clicks.
- [Sound](design/sound.md) (`host/sound.rs`, `app/agent_watch.rs`, `app/notifications.rs`, `server.rs`, `client/window.rs`, #230): a sound when an agent needs you or finishes, played by the window.
- [Opening in other apps](design/opening-in-other-apps.md) (`host/open.rs`, `app/opening.rs`, `client/window.rs`, `server.rs`, `protocol.rs`, #388): Finder, the file manager or an editor, run by the window.
- [Context use](design/context-use.md) (`agents/context.rs`, `agents/claude/transcript.rs`, `agents/codex/rollout.rs`, `agents/gemini/session.rs`, `agents/opencode/database.rs`, `agents/grok/session.rs`, `agents/cursor/store.rs`, `agents/copilot/session.rs`): the model and context line, from each agent's records.
- [Memory](design/memory.md) (`term/memory.rs`, `app/agent_watch.rs`): the memory a tab uses.
- [Plan usage](design/plan-usage.md) (`agents/usage/`, `agents/{claude,codex}/usage.rs`, `app/usage_dialog.rs`, `control/usage.rs`, `app/control/usage.rs`, `cli/usage.rs`): plan limits per agent and account, the `usage` request.
- [Stats](design/stats.md) (`agents/stats.rs`, `agents/known.rs`, `app/agent_watch.rs`, `app/usage_dialog.rs`): counters, the heatmap, `cornercase stats`, the share card.
- [Keeping awake](design/keeping-awake.md) (`agents/awake.rs`, `app/agent_watch.rs`): holding off idle sleep while agents work.
- [Flows](design/flows.md) (`flows/`, `app/flow_runs.rs`, `app/flow_panel.rs`, `app/flows_dialog.rs`, `app/start_flow_dialog.rs`, `app/control/flow.rs`, `app/control/flow/`, `control/flow.rs`, `cli/flow.rs`, `ui/flow.rs`, `ui/flow/`, `ui/dialog/flows.rs`, `ui/dialog/start_flow.rs`, #263): teams of agents run by an orchestrator, the Flow panel.
- [Settings](design/settings.md) (`config.rs`, `settings.rs`, `settings/`, `settings/share.rs`, `app/settings_dialog.rs`, `app/share_settings.rs`, `app/share_settings_dialog.rs`, `ui/dialog/share_settings.rs`, #414): the settings modal, `config.json` watched, the `config` request, sharing settings with a colleague.
- [Terminals](design/terminals.md) (`term.rs`, `term/emulator.rs`): libghostty-vt, writes to a PTY, colours, snapshots.
- [Keys and mouse](design/keys-and-mouse.md) (`term/keys.rs`, `term/mouse.rs`, `app/input.rs`, `app/panes.rs`): decoding input, encoding keys and mouse, selection.
- [Scrollback](design/scrollback.md) (`term/emulator.rs`, `app/panes.rs`, `ui/pane.rs`): the wheel in a pane's history.
- [Keyboard shortcuts](design/keyboard-shortcuts.md) (`shortcuts.rs`, `app/shortcuts.rs`, `ui/keys.rs`): the opt-in prefix key and the keys menu.
- [Client / server](design/client-server.md) (`client.rs`, `client/`, `server.rs`, `server/startup.rs`, `protocol.rs`, `cli/start_server.rs`, #284): the server, its socket and folders, `start-server`, `kill-server`.
- [Remote windows](design/remote-windows.md) (`remote.rs`, `client.rs`, `client/window.rs`, `client/requests.rs`, `cli/reach.rs`, `ui/remote.rs`, #104, #476, #480): `cornercase remote` over ssh, the proxy, reconnecting, `--remote` for the commands.
- [Browser access](design/browser-access.md) (`web/`, `web/tailscale.rs`, `app/remote_access.rs`, `ui/remote_access.rs`): the window in a phone's browser, pairing, Tailscale.
- [Telegram](design/telegram.md) (`telegram/`, `messenger/`, `app/messenger.rs`, `app/messenger/`): away mode, the Telegram bot and what chat apps share.
- [Discord](design/discord.md) (`discord/`, `app/messenger/pairing.rs`, `app/messenger/every.rs`, `app/messenger/forum.rs`, `app/messenger/forum/`, `ui/forum.rs`, #265, #437): the Discord bot, pairing, the Gateway, slash commands, forum mode.
- [Panics](design/panics.md) (`panics.rs`, `server.rs`): containing panics so that the shells survive.
- [Log](design/log.md) (`log.rs`, `paths.rs`, `time.rs`, `app/trace.rs`): `server.log`, its format, levels and what it never holds.
- [Commands for scripts and agents](design/commands-for-scripts-and-agents.md) (`cli.rs`, `cli/`, `control.rs`, `app/control.rs`, `app/control/`, `app/events.rs`, `files/pasted.rs`, `app/pasted_files.rs`, `control/worktrees.rs`, `cli/worktrees.rs`, #479, #485, #536): the commands and socket requests, streams, the contract.
- [Plugins](design/plugins.md) (`plugins/`, `app/plugins.rs`, `app/plugin_popup.rs`, `ui/dialog/popup.rs`, `cli/plugin.rs`, `client/window.rs`): manifests, trust, actions, buttons, popups, hooks, services, installing, the catalogue.
- [Updates](design/updates.md) (`update.rs`, `cli/update.rs`, `version.rs`, `http.rs`, `app/update_dialog.rs`): checking, installing and restarting, also once agents are idle.
- [Releases](design/releases.md) (`.github/`, `dist-workspace.toml`): how a push to `main` becomes a release.
- [Saved session](design/saved-session.md) (`state.rs`, `persist.rs`, `app.rs`, `app/session.rs`, `app/closed_agents.rs`, `app/closed_agents/`, `control/closed_agents.rs`, `cli/agents.rs`, #430): `session.json`, restoring, resuming agents, closing a workspace's agents and reopening them.
- [Website](design/website.md) (`site/`): the brand, the landing demo, captures, translations.
- [Test helpers](design/test-helpers.md) (`test_util.rs`, `agents/{claude,codex,gemini,opencode,grok,cursor,copilot}/fake.rs`, `app/tests.rs`): fakes, fixtures, keeping tests away from the user's files.
