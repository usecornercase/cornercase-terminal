# Changelog

Every pull request that changes the app adds a section here for its new version. The section becomes the notes of the GitHub Release and shows up in the app's update dialog, so write it for users.

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
