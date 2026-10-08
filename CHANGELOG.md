# Changelog

Every pull request that changes the app adds a section here for its new version. The section becomes the notes of the GitHub Release and shows up in the app's update dialog, so write it for users.

## 0.12.7

- `cornercase wait` can watch several panes at once: repeat `--pane` or `--tab` and add `--any` to return as soon as one of them meets the condition, or `--all` to return once every one has. It prints one line per pane, its id and how it ended (`15 idle`), and `--json` gives the same as a list. A pane that closes meanwhile ends as `closed` instead of failing the wait. An agent coordinating others no longer needs one background wait per agent.

## 0.12.6

- Commands for scripts and agents can name a workspace by its branch: wherever they take `--workspace ID` (`new-tab`, `close`, `rename`, `focus`), `--worktree BRANCH` picks the workspace on that branch in the current project, the way `start --worktree` already did. So `cornercase close --worktree feat/x --remove-worktree` takes down a worktree an agent started, without looking its id up in `cornercase status`. A branch no workspace is on, or one several are on, exits with status 2 and lists the workspaces to pick from.

## 0.12.5

- Scripts and orchestrating agents can now tell when a Claude Code agent's turn is over but a command it started in the background (a test watcher, a dev server) still runs. The tab keeps showing it as working, as before, since Claude wakes up when that command ends, but `cornercase status` now says `working (background shell)` (`"background_shell": true` in `--json`), and `cornercase wait --until turn-over` returns as soon as the turn is over, printing `shell` in that case. `start --wait` and `send --wait` take the same `--until`. A plain `wait` that times out on such an agent now says so.

## 0.12.4

- Keyboard shortcuts, if you want them: pick a prefix key in **Settings → UI → prefix key** (press the combination you want, such as `Ctrl+]`). Pressing it opens a small menu at the bottom of the screen listing what each next key does: `n` and `p` switch tabs, `]` and `[` workspaces, `}` and `{` projects, the arrows move between panes, `a` jumps to the next agent that needs you, `c` opens a tab, `|` and `-` split, `f f` finds a file by name and `f w` searches the text in your files, `g d` opens the changes, and more. Nothing to memorise: the menu is on screen, and a click on an entry works too. Press the prefix twice to send it to the program in the pane. It is off by default, so every key still goes to your programs until you turn it on.

## 0.12.3

- opencode tabs now show what opencode is doing, like Claude Code and Codex: `◐` while it works, `✓` when it finished in a tab you weren't looking at, with a toast and a desktop notification, and `○` while it waits for your next message. They also show the model and how full the context is, such as `DeepSeek V4 Pro · 12%`, the memory they use when that setting is on, and they appear in the agents section. opencode doesn't save its permission prompts and questions anywhere, so while it waits for you the tab keeps showing `◐` instead of `!`. With two opencode in the same folder, neither shows its status, model or context until one of them exits.

## 0.12.2

- Claude Code and Codex now come back in their conversations after `cornercase kill-server`, a reboot or the restart after an update: each pane that ran one types `claude --resume <id>` or `codex resume <id>` into its new shell, in the mode the agent was running in. The agent opens idle with the conversation on screen, waiting for you. Turn it off in **Settings → Agents → resume conversations**. Before restarting, cornercase now says how many conversations will resume.

## 0.12.1

- Homebrew installs now update with `brew update && brew upgrade cornercase`, the command the update dialog copies and `cornercase update` prints. `brew upgrade` alone often missed a new release, since Homebrew refreshes its taps only once a day.

## 0.12.0

- Bring your VS Code workspaces along: `.code-workspace` files now show up in the folder picker (**`+ new`** → **open project**), and opening one turns it into a group named after the file, with each of its folders as a project. Folders you already had open move into the group, and folders that no longer exist are skipped. Importing the same file again adds the folders that are new.
- Right-click a group, or click its `⋯`, and choose **add project** to open a folder straight into that group. The picker starts next to the group's projects.

## 0.11.16

- Tabs can move out of the sidebar into a bar above the terminal, like an editor's: set **Settings → UI → tabs** to *top*. Only the active workspace's tabs show there, each with its status (`!`, `✓`, `◐`, `○`), and the sidebar keeps groups, projects and workspaces, so more of them fit; in the tree it ends at the workspaces. A grey line under the tabs shows the model, context and memory of the pane you are in. Click a tab to show it, `⋯` and `×` on hover open its menu and close it, `+` opens a shell, drag a tab sideways to reorder it, and `‹` `›` or the wheel scroll the tabs when they don't fit. The default stays *sidebar*, and below 90 columns the compact menu lists the tabs as before.

## 0.11.15

- Keep your agents on another machine and the window on your laptop: `cornercase remote devbox` attaches to the cornercase server on `devbox` over your own `ssh` (your config, keys and jump hosts apply), starting it there if needed. Keys, the mouse, your colours, copying and desktop notifications go through your local terminal. When the connection drops, after the laptop slept or the Wi-Fi changed, the window stays, says it is reconnecting and comes back by itself, with every shell still running there; `Esc` gives up. Both machines need the same version of cornercase, and it tells you which one to update when they differ.
- Answering `y` when cornercase offers to restart a server from another build no longer leaves the window hanging blank now and then.

## 0.11.14

- A split tab keeps the name of its first pane, the top-left one, instead of changing every time you click another pane, and its row ends with a grey `+n` saying how many other panes it holds. A long name is cut before the count is.
## 0.11.13

- The server now keeps a log of what it does, so when something goes wrong there is a trail to follow: windows attaching, commands from scripts and their answers, tabs and panes opening and closing, each step of starting an agent, every change of an agent's status, dialogs opening, git runs and issue lists that failed or took long, and every error. It never holds what you type, paste or see in a pane, nor tokens. `cornercase logs` prints the end of it (`--follow` keeps printing, `--path` says where it is).
- The log moved to `~/.local/state/cornercase/server.log`, next to your session, so it survives a logout or reboot. Past 8 MiB it moves to `server.log.1` and starts over. Restart the server with `CORNERCASE_LOG=debug` to log every click and key name it handled too.

## 0.11.12

- Every project, group, workspace and tab row has a `⋯` button next to its `×`, shown while you hover the row (always, in the compact layout). It opens the same menu as a right-click: rename, move to group, icon and colour, delete group.
- **+ new group** creates the group as soon as you name it, with its own icon and colour: the first eight groups each get a different one, starting with a blue `●`. Change them later from the group's `⋯` or right-click menu, **icon and colour**.
- The button at the end of the projects list now reads `+ new`, since it opens a project or creates a group.
- Restart cornercase from **settings → restart**, or with `cornercase restart` in a shell. Like after an update, it first lists what is running in your terminals, then brings your projects, workspaces, tabs and splits back with new shells.
- **Settings → UI → counts** hides the number of workspaces after each project, and of projects after a folded group. The settings tab called TUI is now UI.
- The TODO item you are editing keeps its `×`, so you can delete it without leaving the field first. Undo brings it back with what you typed.

## 0.11.11

- Removing a worktree that is locked no longer stops its programs and then fails with git's "cannot remove a locked working tree". The dialog says it is locked and why, and when the lock names a process, whether it still runs: Claude Code locks the worktrees it creates, and a restart that ends its session leaves the lock behind. **unlock and remove** unlocks and removes it. `cornercase close --remove-worktree` refuses a locked worktree before anything stops and says how to unlock it.

## 0.11.10

- An agents section for the sidebar: every Claude Code and Codex running in any project, one row each, with its status (`!` waiting for you, `✓` done, `◐` working, `○` idle), its project and workspace, and its model and how full its context is. Click a row to jump straight to that agent, whatever project you're in. Turn it on in **Settings → TUI → agents section**. It sits at the bottom of the sidebar in every layout, and you can drag the line above it to make it taller or shorter. On a narrow terminal it's a third menu: ` agents › ` in the projects menu.

## 0.11.9

- Move a pane within its tab by dragging it with the right button onto another pane: near an edge it goes to that side, in the middle the two swap. The part it would take is tinted while you drag, the programs keep running, and `Esc` cancels. Two panes side by side become one above the other by dropping one on the other's bottom edge. A right-click without moving still opens the pane menu, now on release.

## 0.11.8

- Clicking the checkbox of a TODO item while you edit its text now checks it, keeping what you typed. Before, the click only moved the cursor, and you had to leave the field first.

## 0.11.7

- Searching file names in the files panel puts the file you name first: `workspace-session` now lists `workspace-session.ts` before `workspace-session-handler.ts`, even when the longer name sits in a shallower folder. Between two equally good matches the shorter file name wins.

## 0.11.6

- When an agent cornercase starts asks whether you trust the folder, you now answer it yourself, and the issue's prompt waits until you have. Saying yes lets the repository's own agent settings, hooks and MCP servers run, so it is your call. Before, cornercase said yes for you, and if you had turned that off, it typed the prompt into the question. **Settings → Agents → trust prompts** still answers for you; it starts off once after this update, even if you had it on.
- Removing a worktree no longer holds the dialog open while its folder is deleted. The dialog says right away when the worktree has changes that are not committed, and **remove** closes it at once: the workspace's row shows `removing…` until git is done, then goes with a `removed …` message, or comes back with git's reason. You can remove several at once and keep working meanwhile.
- Split panes never shrink to nothing. Dragging a divider stops where a pane on either side, panes split inside it included, would get smaller than 10 columns or 3 rows, and a smaller terminal shrinks them down to that. When even that doesn't fit, as when you attach from a phone, the tab shows only its active pane until there is room again.

## 0.11.5

- A files panel: ` files `, under ` issues ` (` ▤ ` in the narrow bar), opens the active workspace's files beside the pane, folders first, leaving out what git ignores. Click a folder to open it and a file to read it, coloured for about 60 languages; the rest borrow the colours of a close relative.
- What changed shows as in the changes panel, following its tab: changed files get their letter and colour in the tree and folders holding changes a dot. In a file the margin marks new lines in green, changed ones in blue and deleted ones in red; click a mark to see the old lines.
- A search bar at the top of the panel finds text in every file of the workspace, grouped by file with the match lit; with nothing typed the panel shows all the files. The ` ▤ ` next to it switches to file names, found by a few letters of their path (`ptyhand` finds `src/relay/pty-handler.ts`). Arrow keys pick a result and Enter opens it; a text match opens the file at its line with the text lit.
- Click a path in a pane, such as one Claude Code or Codex prints, `git status` lists or a compiler error points at, and the file opens in the panel at its lines (`src/app.rs:120-140`), selected. A path under the pointer is underlined when it can be opened. In Claude Code a drag that starts on a path still selects text there.
- Click a line, or drag over several, then ` ask agent ` hands `path:12-30` to the workspace's agent, ` copy ` copies them and ` open ` opens your editor there. An open file follows its edits, so you can watch an agent write it.

## 0.11.4

- Before cornercase restarts, after an update or when a new build finds an old server, it now says what will stop: how many agents are working or waiting for you, and which other programs run in which project. Nothing running? It says so. The update dialog has a **later** button, `cornercase update --yes` prints what it stopped, and so does `cornercase kill-server`.
- cornercase uses less CPU while agents print in tabs you are not looking at: their output no longer redraws the screen, and a tab printing without pause is drawn at most about 60 times a second.

## 0.11.3

- A shell that can't start while your session is restored, for example in a folder you can no longer enter, no longer cuts the restore short and loses the rest: every other project, workspace and tab comes back, a message names the projects that lost a tab, and `server.log` says which folder failed and why. If no shell could start at all, one opens in the folder you started cornercase from. The session as it was saved is kept in `session.json.bak`.
- Starting an agent on an issue no longer types the prompt too early when the agent is slow to start, on a busy machine or the first run after an update, for example. cornercase used to paste it before the agent had asked whether you trust the folder, and the prompt became the answer. It now waits until the agent has drawn something.

## 0.11.2

- A new look. Dialogs have rounded corners and dim the rest of the screen while they are open, their buttons look like buttons, and the selected row in a list is marked with a bar on its left instead of a bright cyan fill.
- In the sidebar, tabs hang from their workspace on thin tree lines, the bar that marks the active row sits on the left edge and covers the whole row, and borders and separators are quieter.
- The usage bars are thin lines, and TODO items have round boxes that turn into a green check when done.
- A workspace without tabs says so in the middle of the pane, and with no project open the pane shows the cornercase logo and how to open a folder.

## 0.11.1

- With **memory** on, an agent tab now shows the memory its agent really uses: on macOS the figure Activity Monitor shows, on Linux each process's private memory, including what was swapped out. Code that several processes share, such as the agent's own program, used to be counted again for every process, so the figure was about twice what the agent costs; a fresh Claude Code tab now shows about 200 MB instead of 440 MB.

## 0.11.0

- Jira: the issue browser has a **Jira** tab for Jira Cloud. Open it, type your site (`acme.atlassian.net`, or just `acme`), your email and an API token from id.atlassian.com, and its issues show up next to GitHub, Shortcut and Linear: read them (descriptions and comments are turned into Markdown), filter them by assignee or reporter, and press **start** to put an agent on one in a worktree on a `SHOP-77-…` branch.
- **settings → Issues → Jira** holds the site, the email and the token, plus an optional **Jira filter**: a JQL condition such as `project = SHOP` for when your site has more than you want to see. `JIRA_API_TOKEN` in the environment works like the other trackers' variables.
- If you hid some issue tabs before, the Jira tab is added at the end of your list once; hide it in settings if you don't use Jira.
- Issues from different trackers are now sorted correctly in the **All** tab when their times come with a time zone.

## 0.10.0

- cornercase can now be driven from the command line, by your scripts, git hooks and coding agents. `cornercase status` lists your projects, workspaces, tabs and panes with their ids; `open`, `new-workspace` (with `--worktree` for a git worktree), `new-tab` and `split` make them, typing a command if you give one; `start` starts an agent in a new tab or its own worktree and hands it a prompt; `send` and `keys` type into a pane, `read` prints its screen or its last lines, and `wait` waits until an agent stops working or needs you, a program ends, a line shows up or the output stops. `close`, `rename`, `focus`, `notify` and `todo` do what their buttons do. `cornercase --help` explains each one, and `--json` prints the answers as JSON.
- So an agent in one tab can coordinate others: start them in their own worktrees, wait until they finish or ask something, read what they wrote and send the next step. Teach your agents with the skill: `npx skills add usecornercase/cornercase-terminal --skill cornercase -g`, or `cornercase skill` to print it.
- These commands never move what your window shows, unless you ask with `focus` or `--focus`. Run inside a pane, a command acts on that pane: every shell now has `CORNERCASE_PANE` set to its pane's id.
- `cornercase --help` and `cornercase COMMAND --help` now describe every command, and a mistake in how a command is called exits with status 2.
- A server started before this update answers these commands with "too old": restart it once (cornercase offers to when you open it).
- Pasting a long text into a pane whose program isn't reading it, such as one waiting in `sleep`, no longer freezes cornercase until the program reads.

## 0.9.2

- In the compact layout (below 90 columns, as on a phone over SSH), every group, project, workspace and tab row now shows its `×` all the time, dimmed, instead of only on hover, which touch screens don't have.
- Because it is easy to tap by accident there, closing a tab or a workspace in the compact layout asks first, as closing a project always does. The wide layout is unchanged.

## 0.9.1

- Most bugs in cornercase no longer take your shells and agents down with them. Until now, when the server hit a bug (a panic), it stopped, and every terminal and coding agent running in it went too. Now cornercase drops only what it was doing at that moment (a dialog or menu you had open may close), says `cornercase hit a bug, see server.log` for a few seconds and carries on.
- `server.log` gets where the bug happened, with a backtrace. Please add it when you [open an issue](https://github.com/usecornercase/cornercase-terminal/issues).
- A dialog waiting for something running in the background (creating or removing a worktree, installing an update, checking a token, starting an issue) no longer stays busy forever when that hits a bug: it shows the error, and you can close it or try again.

## 0.9.0

- cornercase now starts with one column on the left, the projects on top and the active project's workspaces under them, so your panes get the width the workspaces column used to take. Drag the line between the two lists to share the height. If you prefer the three columns, pick **settings → TUI → sidebar → side_by_side**; a `config.json` that already names `side_by_side` keeps it.
- A new sidebar layout, **tree**, shows one list with your groups, every project, their workspaces and their tabs, so a tab in another project is one click away. Click `▾` to fold a project or a workspace and `▸` to open it again; a folded row shows `!` or `✓` when something inside needs you. Each project ends with `+ new workspace` and each workspace with `+ tab`, and rows drag to reorder as before. With **memory** on, the tree measures the tabs of every open workspace, not only the active project's. Below 90 columns the compact menu stays as it was.
- Grey text, such as titles, the buttons at the bottom, the search field and the tree's arrows, no longer disappears in themes whose *bright black* is close to the background, such as Warp's Adeberry or Solarized Dark. cornercase then uses a grey that shows on any background, and keeps your theme's own where it reads well.
- What you fold is saved with your session. The first time, only the project you were on stays open. The session file gets a new format for this, so going back to an older cornercase afterwards starts with an empty session.

## 0.8.0

- A TODO list: click **`todo`** under `changes` (or `☐` in the compact bar) to open it in the column right of your panes. Add items with **`+ new todo`**, click an item's text to edit it (with a cursor you move with the arrows), click `[ ]` to check it off and drag items to reorder them. Done items sink to the bottom, struck through, and **`clear done`** removes them.
- Deleting asks nothing: the message that says what went has an **`undo`** button for a few seconds.
- The list is the same in every project and is kept in `todos.json` next to your session.

## 0.7.2

- A tab no longer gets stuck as `?` after you close it, or after its shell exits, while a process it started keeps running in the background. It happened when a program detached itself from the shell, such as Neovim's server outliving its window when the tab closed under it, and the tab could not be closed again. Now the tab goes as soon as its shell ends, as in tmux.

## 0.7.1

- cornercase now looks for a new version every hour instead of once a day, so a release reaches you the same morning. It is still one small request to GitHub (plus the changelog once a new version is out), and **settings → TUI → check for updates** still turns it off.
- The update dialog now shows what changed in every version since yours, newest first, so if a second release comes out before you update, you also see what the first one brought.

## 0.7.0

- A new setting, **settings → TUI → memory**, shows how much memory each Claude Code and Codex tab uses at the end of the line under its name: `Opus 5.5 · 23% · 1.2 GB`. It counts the agent and everything it started (MCP servers, language servers, background commands), as `ps` does, so take it as a guide to which agent is growing. It is off by default; while on, the tabs of the project on screen are measured every 2 seconds.
- The model and the context on that line are now separate settings, **model** and **context**, so you can show only `Opus 5.5`, only `23%`, or neither. The `context_line` key of 0.6.0 becomes `context` in `config.json`; if you had turned it off, the model stays hidden too.

## 0.6.1

- The **`usage`** dialog now shows your Codex plan too, under Claude Code's: the five-hour session and the week, how much of each you've used and when it resets, plus your credits when you have some. It asks Codex itself (`codex app-server`), so the numbers are live, cost no tokens and cornercase never reads your login.
- Only the agents you have installed get a section (with neither installed, both show and say why they couldn't run), and a Codex you haven't signed in to just says `not signed in · run codex login`. If an agent isn't on the server's `PATH`, tell cornercase where it is with `agent_commands`.

## 0.6.0

- A new setting, **settings → TUI → context line**, hides the line under Claude Code and Codex tabs that shows their model and how full their context is, so every tab takes one row. It is on by default.

## 0.5.2

- Codex tabs now show what Codex is doing, like Claude Code tabs: `◐` while it works, `!` when it waits for your approval, `✓` when it finished while you were in another tab, `○` while it waits for your next message. `!` and `✓` also show on the workspace, project and collapsed group rows, and on the compact `≡`.
- Codex in a tab you aren't looking at now notifies you too, with the same toast and desktop notification as Claude Code: `codex needs you in shop › main` or `codex finished in shop › main`. With Claude Code and Codex in the same workspace, each gets its own notice, named after the agent.
- Nothing to set up: cornercase reads the title Codex gives the terminal and the session file it already follows for the context line. If you take the activity out of Codex's terminal title (`/title`), the tab still shows working and finished, but not `!`.

## 0.5.1

- Filter the changes panel by path: click **`⌕`** next to its `×` and type. A piece of a path (`order`) or a pattern like in `.gitignore` (`*.test.js`, `src/api/**`) keeps only the files that match, `!` leaves files out (`!*.snap !*.lock`), and the summary says how many are left (`3 of 31 files`). `Enter` keeps the filter and gives your keys back to the pane; `Esc` clears it. While the field has the keys, click in a pane to type there again, and on the field to come back.

## 0.5.0

- New `cornercase update` command: installs the latest release from any shell, without waiting for the daily check, then asks whether to restart the server. Your projects, workspaces, tabs and splits come back, each pane with a new shell. `--yes` skips the question and `--check` only says whether a newer version is out. Homebrew installs, and folders cornercase can't write to, get the command to run instead.
- Restarting from `cornercase update` reopens every attached window on the new version, even when you run it inside cornercase. A server from 0.4.2 or older can't do that yet, so this first time its windows close: run `cornercase` again to get your session back.

## 0.4.2

- Tabs running Codex, Gemini CLI or another script file run by Node, Bun, Deno, Python or Ruby are now named after the script, such as `codex`, instead of `node-MainThread` or `node`. Modules (`python3 -m …`) and inline code (`node -e …`) keep the interpreter's name. Known agents are named after their kind (`cursor-agent` shows as `cursor`), and search finds those tabs by that name. A custom tab name still wins.

## 0.4.1

- Codex tabs now show their model and context use under the tab name, such as `gpt-5.4 · 20%`, in wide and compact layouts. The percentage turns orange from 75% and red from 90%.
- Each pane follows its own Codex session, including npm installations and sessions run by Codex's background app-server (matched by folder, so two Codex in the same folder show no line). The line appears after the first message. cornercase respects `CODEX_HOME` and uses Codex's reported context window. When the percentage is unavailable, or after a model change or compaction, it shows the model alone until fresh usage is recorded.

## 0.4.0

- Closing a project now asks first. The `×` on a project row opens a dialog that says how many tabs it stops, agents included, and that its folder and worktrees stay on disk. Press **close** or `Enter` to close it, **cancel** or `Esc` to keep everything running. Tabs, and workspaces without their own worktree, still close at once.

## 0.3.0

- Delete a group from its row: hover the group's header and click the `×` at its right end, like on a project. Right-click → **delete group** still works too.
- Deleting a group now asks first, whichever way you start it, and says what happens to its projects: they stay open, outside the group.

## 0.2.0

- Safer on shared machines: cornercase only uses a socket folder that is yours alone (it refuses one another user owns or can write to, or a symbolic link), checks that the server and every window talking to it run as you, and after an update restarts into its own program instead of the one the server names. Without `XDG_RUNTIME_DIR`, as in some SSH sessions and containers on Linux, another user of the same machine could otherwise pose as the cornercase server and see what you type.
- If cornercase now refuses to start and names its own socket folder, check who owns it (`ls -ld` on that folder). If it is yours, remove it (only the link, if it is a symbolic link) and start cornercase again. If it belongs to someone else, you can't remove it, and someone may be trying to pose as cornercase: tell whoever runs the machine, and meanwhile point `CORNERCASE_SOCKET` at a socket in a folder only you can write to.
- If you set `CORNERCASE_SOCKET` and cornercase refuses its folder, don't remove that folder, which may hold your other files: point the variable at a socket in a folder only you can write to.

## 0.1.16

- Put your projects, groups, workspaces and tabs in the order you want: press on a row and drag it. A cyan line shows where it will land, and it moves there when you let go. Groups move with their projects, workspaces stay in their project and tabs in their workspace.
- Drag a project onto a group's header or between its projects to put it in that group, or among the projects at the top to take it out. The `move to group` menu is still there.
- `Esc`, or letting go outside the list, cancels a drag. A plain click still selects the row, now when you release the button. Holding a drag over `↑ n more` or `↓ n more` scrolls the list.
- The order is saved with your session, so it is still there after a restart.

## 0.1.15

- Updated the diff library the changes panel uses to pick which words of a changed line to highlight. Its matching now follows git's more closely, so on some heavily edited lines the highlighted words may differ slightly from before.

## 0.1.14

- More room for your projects and workspaces: the cornercase logo at the top of the sidebar is gone, so the search bar sits on the first row and both lists start three rows higher.

## 0.1.13

- See how full Claude Code's context is without opening its tab: once Claude has answered, a second line under the tab's name shows its model and how much of its context window the conversation takes up, such as `Opus 5.5 · 23%`, the same percentage as Claude's own status line. It turns orange from 75% and red from 90%.
- The numbers come from the transcript Claude Code keeps for the session. The size of the window is worked out the way Claude Code does it, from the model and from `[1m]`, `CLAUDE_CODE_DISABLE_1M_CONTEXT` or `CLAUDE_CODE_MAX_CONTEXT_TOKENS` in Claude's settings or environment.

## 0.1.12

- Give your panes more room: settings → TUI → sidebar puts the workspaces column below the projects column (projects_on_top), or above it (workspaces_on_top), in one column. Drag the line between the two lists to share the height, and double-click it to split it in half again.

## 0.1.11

- Fixed a crash that could close every shell and agent at once. Once a pane in a split had no room left (after dragging a divider to the edge, or on a small window), a middle click or a drag that started in the sidebar took the server down with it.

## 0.1.10

- See how much of your Claude Code plan you have used without leaving what you are doing: the new `usage` button, under `settings`, opens a dialog with the five-hour session, the week and any per-model weekly limit, each with a bar and when it resets. Extra usage shows too when it is turned on.
- The numbers come from Claude Code itself, asked in the background when you open the dialog: no prompt is sent, no tokens are spent, and cornercase never reads your login.

## 0.1.9

- A Claude Code tab no longer shows `✓` and notifies you that Claude finished while a command it started in the background (a build, a test run) is still running. The tab keeps `◐` until Claude is really done, and you hear about it once.

## 0.1.8

- Find out when Claude Code needs you or finishes in a tab you aren't looking at: a toast says where (`claude needs you in shop › main`), and your terminal shows a desktop notification, so you hear about it from another window too. It works over SSH, because the notification travels through your terminal.
- cornercase asks your terminal its name and sends the notification it understands: Ghostty, iTerm2, kitty, WezTerm, foot, Konsole, Warp, Rio, Contour and VS Code get a real one, other terminals a bell. Choose another kind or turn them off in settings → TUI → desktop notifications.

## 0.1.7

- Starting cornercase from inside Claude Code no longer passes that Claude session's variables to every pane, so a `claude` started in a pane is a session of its own again (it saves its prompt history, for instance) and the other session's messaging token stays out of your shells.

## 0.1.6

- See what Claude Code is doing without opening its tab: a tab running Claude shows `◐` while it works, `!` when it needs you (a permission or a question), `✓` when it finished while you were looking at something else, and `○` while it waits for your next message.
- `!` and `✓` also show at the end of the tab's workspace and project rows, and on the header of a collapsed group, so an agent waiting in another project doesn't go unnoticed. On a small screen, the `≡` button shows them. Opening the tab clears `✓`.
- Nothing to install or configure: cornercase reads the status Claude Code keeps for each of its running sessions.

## 0.1.5

- See what changed without leaving cornercase: in a git workspace, click ` changes ` (next to ` issues `, or ` ± ` on a small screen) to open a panel on the right with the diff of every changed file, new files included. It updates by itself while an agent works.
- Three tabs: uncommitted (what is not in a commit yet), commits (what your branch committed since it left the default branch) and all (both, like the pull request will look). Click `vs main ▾` to compare with another branch; cornercase remembers it for that workspace.
- Changed words stand out inside each line, code keeps its syntax colours, and lockfiles, binary and huge files start folded. Click a file to fold or unfold it, mark it as viewed with ✓, or click "N unchanged lines" to see more of the code around a change.
- Hover a block of changes to open it in your editor at that line, send its file and lines to the agent running in the workspace, or copy it.

## 0.1.4

- Project, workspace, tab and group rows get a subtle background while the mouse is over them, so you can see what a click will hit.

## 0.1.3

- Group your projects: `+ new project` → new group creates a group with a name, an icon and a colour. Right-click a project to move it into a group, and click a group's header to collapse or expand it. Right-click the header to rename it, change its icon and colour, or delete it (its projects stay open).
- Search also finds groups; picking one expands it and opens its first project.
- `+ new project` now opens a small menu: open project or new group.

## 0.1.2

- With several terminals attached, cornercase now takes the size of the one you used last (attached, typed or clicked in) instead of the smallest. Attaching from a computer after a phone whose connection hung no longer leaves the session at the phone's size. A smaller terminal shows the session cut off at its edges.

## 0.1.1

- A workspace whose branch is behind its upstream shows the commits to pull at the end of its row, such as `↓3`. cornercase fetches each project's remotes in the background every 5 minutes, and never pulls for you; change how often, or turn it off, in settings → Worktrees → fetch branches every.

## 0.1.0

- First release with prebuilt binaries for Linux and macOS: install with `curl -fsSL https://usecornercase.dev/install.sh | sh` or `brew install usecornercase/tap/cornercase`.
- cornercase checks for new versions once a day and updates itself from the sidebar; turn it off in settings → TUI.
- `cornercase --version` prints the version.
