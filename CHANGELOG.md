# Changelog

Every pull request that changes the app adds a section here for its new version. The section becomes the notes of the GitHub Release and shows up in the app's update dialog, so write it for users.

## 0.12.132

- **The agents section shows who started whom.** An agent started by another agent (`cornercase start`, `new-tab` or `split` from its pane) is drawn right under it, indented, with lines joining them, and a flow's team hangs under its orchestrator, so you can see which agent works for which. Rows still move only when an agent starts or closes. A child whose parent closes becomes a root again. The tree comes back after a restart or an update.
- **Scripts see it too.** Every pane in `status --json`, `status --follow` and `view` has `parent`, the pane that started it (`null` when none); `status --panes` has it as its last column and the text tree shows `(parent N)`. A pane's `opened` event in `cornercase events` names its parent.

## 0.12.131

- **Programs see the colours of the app that drives cornercase.** `cornercase start-server --theme FILE` (or `-` for standard input) gives the server the foreground, background and 16 ANSI colours that programs in its terminals get when they ask, with no window attached. Claude Code, Neovim and delta pick their light or dark look from them.
  - `cornercase theme FILE` changes them while the server runs, for an app that follows the system's appearance. Programs that turned on colour-scheme reports are told when light and dark switch.
  - `cornercase theme --clear` gives the terminals back the window's colours, and `cornercase theme` says where they come from (also `theme` in `status --json`).
  - A window that attaches later keeps its own look and leaves those colours alone. Without `--theme` nothing changes.
  - See Scripts and agents → Colours for programs.

## 0.12.130

- **Send a TODO to an agent.** In the TODO panel, each item left to do has a **`▸`** next to its `×` (on hover; always shown in the compact layout). It opens a menu of every agent running, by project and workspace, and picking one types the item into that agent and sends it, as `cornercase send --enter` would. Once the agent has taken it, the item is checked off and a message says where it went; what `send` refuses (an agent waiting for your answer, resuming, showing a dialog or its survey) the button refuses too, with the reason, and the item stays to do.
- `cornercase todo send ID [--pane N | --tab N]` does the same from a script or an agent: the same target, refusals and confirmation as `send --enter`, then the item is checked off and it prints `sent todo 3 to pane 5`. A refused or unconfirmed send leaves it pending and exits 1; for an agent whose prompts cornercase cannot confirm (a `/` command, an agent it does not read) the text is sent, the item stays to do and it says so (exit 0). See Scripts and agents → Leave notes.

## 0.12.129

- **Surfaces carry everything a cell looks like.** For programs that draw cornercase's terminals with `cornercase surface`:
  - Underline styles (double, curly, dotted, dashed), underline colours and overline are in the styles, so Neovim's undercurl diagnostics come through.
  - Hyperlinks (OSC 8), from `ls --hyperlink`, compilers or `gh`, come as a table of addresses the cells point to, and a link that wraps onto the next line is one link.
  - Each pane carries the colours its program changed (OSC 4, 10, 11, 12), so a program that redefines a colour looks as it does in a terminal.
  - Clients that don't know these keys keep working: they are left out when a terminal uses none of them.
- **`cornercase link --pane ID --cell X,Y`** says which file a click on that cell opens in the window: the path, its line or range, and whether it is inside the pane's workspace, so a program that draws the terminals gets cornercase's path detection without copying it. See Scripts and agents → Drive a tab's terminals.
- **Backgrounds painted by clearing now show.** A line or region a program clears after setting a background colour (vim with a colour scheme, less, htop) keeps that colour in the window and in surfaces; it showed the terminal's own background before.

## 0.12.128

- **GitHub Copilot CLI is now a full agent.** A tab running `copilot` shows whether it is working, needs you (a permission prompt, a question or a plan to approve) or is done, its model and how full its context is, and the project, workspace and tab rows carry the same icons as for the other agents. A background command it left running shows as working, as Claude Code's does. Its conversation comes back after a restart in the mode it ran in (`copilot --resume=<session id>`), `cornercase send --enter` confirms that it took the prompt, `read --last-message`, `wait` and the search in sessions read it, and Stats counts its prompts, turns, time and tokens.
- Settings → Agents lists `copilot` with its modes (plan, autopilot, allow all), `start --effort` takes its `--reasoning-effort` levels, and flows can give it roles: it runs unattended with `--allow-all --no-ask-user` (never autopilot), gets the run's folder with `--add-dir`, and several roles can share one worktree.
- Copilot's rate-limit and session-limit questions are not written to its session files, so they show as working rather than `!`. A resumed Copilot asks again whether you trust the folder unless you told it to remember it. Installs through npm and `gh copilot` are recognised too, but were not checked on a real install, and nothing was checked on macOS.
- Shells no longer inherit the variables GitHub Copilot CLI sets for its own session (`COPILOT_AGENT_SESSION_ID`, `COPILOT_LOADER_PID` and others) when cornercase was started from one of its shells, so a `copilot` started in a pane is not confused with that session.

## 0.12.127

- **What programs ask of the terminal reaches other programs that draw cornercase.** `cornercase events --host` also prints, for each pane, the text a program copies to the clipboard (OSC 52, such as a yank in Neovim or `/copy` in Claude Code, with the text in `--json`) and its bells, at most one line a second per pane with how many rang. Followers that don't pass `--host` get nothing new. `status --json` and `status --follow` give each pane its `title`. `selection --copy` prints what it copied, so a program without a window can put it in the clipboard itself. Windows work as before.

## 0.12.126

- **Drive a tab's terminals from another program.** For a program that draws cornercase's terminals itself with `surface`:
  - `cornercase input --pane ID EVENT…` types keys with their modifiers, text and pastes into that pane, and sends it mouse events. They are encoded for what the program asked for (its keyboard protocol, application cursor keys, bracketed paste, its mouse mode), whatever the window shows, even with a dialog open there.
  - `scroll` moves a pane's view through its history. `selection` selects text, prints or copies the selection, and clears it. `ratio` moves a divider. `resize --surface ID` lays a `surface` stream out at another size without starting it again: each of its lines now names its stream with `surface`.
  - Input that cannot be sent is refused with a reason and nothing is sent: a program that stopped reading answers `busy`, a pane that changed size since answers `conflict`. Mouse events and key releases the program does not take are said, not dropped silently.
  - `send` and `keys` work as before, with their checks.
  - See Scripts and agents → Drive a tab's terminals.

## 0.12.125

- **Views for programs that draw cornercase themselves.** `cornercase view --cols N --rows N` opens a view of your own that lives while the command runs. It picks its own project, workspace and tab (`cornercase focus --view ID`, or `--view ID` on `new-tab`, `split`, `start`, `send` and the other commands), so an editor extension or a GUI can show another tab than the window without switching it, and each such program its own. `cornercase surface --view ID` streams the tab the view shows and follows it. The view's lines carry a `projection` number that changes with each move, so a program can drop frames drawn before it. `cornercase view-set` resizes a view, turns it inactive or active, or takes its tab back.
- **Terminal sizes follow whoever last took them.** A tab's terminals take the size of the window or the view that claimed it last, without the window's sidebar and panels. Only that one resizes them, so two programs showing one tab no longer fight over its size. A view claims the tab it shows when it opens, moves there or turns active; using a window takes back the tab it shows. Closing a view or turning it inactive hands the tab back. Windows still mirror each other and share one size. See Scripts and agents → A view of your own.

## 0.12.124

- **A pane takes no input while its agent comes back.** After a restart or an update, a pane where Claude Code, Codex, Gemini CLI, Grok Build or Cursor CLI was running shows **resuming claude…** until the agent is back in its conversation. Keys and pastes typed meanwhile are dropped, with a note saying why, instead of landing in front of the resume command or in the agent's start-up, where an Enter could answer a question you never saw. **cancel** gives you the pane back at once, as a plain shell if the command was not typed yet. A trust question still takes your answer.
- `cornercase send` refuses a pane that is resuming, unless `--force`, and `cornercase status` shows it as `resuming` (also a new last column of `status --panes`).

## 0.12.123

- **Flows have their own tab in Settings**, right after Agents: every flow with the selected one's diagram beside it, and edit, duplicate, export, delete, import and new, one click away. The flows row left the Agents tab. Settings now opens at the size of that list on every tab, so the diagram fits beside it and the tabs stay where they are.
- **Delete the built-in flows you don't use.** **delete** on a built-in flow asks first, then hides it from Settings, the start dialog and `cornercase flow list`; `cornercase flow run` refuses it and says how to bring it back. A built-in you edited offers both **restore default** and **delete**. **deleted built-ins**, at the end of the list, brings them back. A run already going keeps its flow, and a project's flow named like a deleted one still never runs under that name. The names are kept in `hidden_flows` in `config.json`.

## 0.12.122

- **Stream a tab's terminals to another program.** `cornercase surface --tab ID --cols N --rows N` keeps running and prints the tab as one JSON line each time it changes. Each line holds every pane's cells with their characters, colours and attributes, its cursor, the modes its program turned on (mouse, bracketed paste, alternate screen…) and how far it is scrolled, together with the panes' places and the dividers between them, laid out at the size you give. It works on a tab the window isn't showing, without switching to it. It stops when the tab closes or the server stops, and leaves the shells running when you stop it. Each line is complete and carries the server's `boot`, its own `revision` and the `session` revision of `status --follow`, so a program that falls behind just gets the latest state. Terminals keep the size the window gives them for now. See Scripts and agents → Stream a tab's terminals.

## 0.12.121

- **Select a whole file in the files panel.** While you drag over the lines of a file, holding the pointer on the first or last line shown (or past it) scrolls the file and the selection follows; the wheel scrolls it too while you keep the button pressed.
- **copy** with no lines selected now copies the whole file instead of its path. Copied text is now exactly what is in the file, so tabs stay tabs.

## 0.12.120

- **The icon bar is readable on every theme.** Its icons used fixed greys and white, which faded or disappeared on light themes such as Solarized Light. They now use your theme's grey when it stands out enough from the background, else a grey that does, and the hovered icon and its name use your terminal's own text colour. The same goes for the rest of the window's grey and white text: rows that are not selected, inactive tabs, the breadcrumb, buttons in dialogs, and the changes and files panels.

## 0.12.119

- **`cornercase status --follow`** keeps running and prints the whole session as one JSON line each time something in it changes, for other programs that show or drive cornercase: the first line is the state now, and later lines replace it completely. Each line carries the server's `boot` and a `revision` that goes up by one with every change, so a program can tell what it already has. It covers what `cornercase events` does not, such as renaming, reordering, moving a project to another group, splitting a pane, dragging a divider, folding, and changes to branches and pull requests. A reader that falls behind gets at most four older lines and then skips to the latest state instead of piling up old ones; reconnecting gives the state as it is now.
- `cornercase status --json` now also gives each group its icon and colour, each project, workspace and tab the name you gave it (`custom_name`), each project and workspace whether it is collapsed, and each tab its split layout (`layout`), with the direction and ratio of every split and the pane ids at its leaves.

## 0.12.118

- The TODO button's dot for pending items now sits inside the button, after its name (` TODO ● `), like Remote's (` Remote ● `), and a click on it opens the list.

## 0.12.117

- **Read a file full screen.** An open file in the files panel has a **▢** button next to its **×**: it gives the file the whole window, keeping where you were, the selected lines and the highlights. **❐** or `Esc` puts it back in the panel at the width you gave it, and your programs keep their size. Going back to the tree, **open**, **ask agent** (when it finds an agent) and switching tabs put it back too.

## 0.12.116

- **`cornercase start-server`** starts the server without opening a window, or finds the one already running, and returns once its session is restored, so a script or another program can start cornercase and create tabs, split panes and start agents right away. It needs no terminal. New terminals get 160×48 until a window attaches (`--cols` and `--rows` choose another size); the first window then takes over with its own size and colours, and finds the same tabs: nothing is restored twice. Commands that open terminals on a server with no window yet now point to `cornercase start-server`. See Scripts and agents → Start without a window.

## 0.12.115

- Plugins: a folder with a `cornercase-plugin.toml` can add entries to the ` ⋯ ` and right-click menus of projects, workspaces, tabs and panes, run a command on every event `cornercase events` prints, keep a service running in the server (restarted if it stops), and keep a process running next to each window on this computer. Plugins talk back through the cornercase commands, so they can be written in any language.
- `cornercase plugin link <folder>` adds a plugin, turned off; `enable`, `disable`, `list`, `logs` and `unlink` do the rest, and Settings has a new Plugins tab to turn them on and off.
- `cornercase plugin install owner/repo` installs a plugin from GitHub (or any `https://` git address, or a folder), pinned to a commit, with `@tag`, `@branch` or `@commit` to choose one. A repository can hold several plugins in folders: `cornercase plugin install owner/repo/plugins/notes` (or `address//folder` outside GitHub). Before anything runs it shows where the plugin comes from, what it asks for and every command it will run, and asks; it is installed turned off. A plugin can ship prebuilt archives, checked against their sha256 before they are unpacked, or build steps that run once you confirm.
- `cornercase plugin update` fetches installed plugins again and, when one changed, shows the new version's commands and asks before updating; `cornercase plugin remove` turns a plugin off, waits (up to 10 seconds) until its service and window processes have stopped, then deletes its installed copy; if something still runs, cannot be checked, or the plugin was turned on again meanwhile, it says so and deletes nothing, leaving the plugin installed.
- Find plugins at usecornercase.dev/plugins: every public GitHub repository with the `cornercase-plugin` topic shows there, with each plugin it holds and the command that installs it; see what is trending and new, search it, and sort by stars, trending, recent activity, newest or name. Nobody reviews them, so read a plugin or trust its author before turning it on. To publish yours, add that topic to its repository.
- A plugin runs as you and can do anything you can, including typing into every terminal. What it asks for is shown before you turn it on, but not enforced: only turn on plugins you trust.

## 0.12.114

- **A click on a notification takes you to the agent.** In kitty, VS Code, Contour, foot 1.18 or later and Konsole 24.12 or later, clicking the desktop notification of an agent that needs you or finished shows its project, workspace and tab, with its pane focused. foot and Konsole now get kitty's kind of notification, so Konsole, like foot, no longer shows one while its window is in front.
- The toast that comes with it has a **go** button that does the same in any terminal, and stays 6 seconds. In Ghostty, iTerm2 and WezTerm, where a click only brings the window forward, the toast comes back with its **go** when you return to the window, if the agent still needs you.
- When you quit, restart or stop the server, cornercase closes the last 64 notifications it sent (where the terminal can), so clicking one later types nothing into your shell.

## 0.12.113

- **A folder for each flow run.** Every run gets a folder of its own, outside the worktree so nothing in it is ever committed, and every agent of the team has its path in `$CORNERCASE_RUN_DIR`. Each role writes its answers there, one file per answer, and replies with the file's path and a few lines; the orchestrator passes paths on instead of pasting reports, and writes its own decisions there, so a run leaves a record of what each role produced and what was decided. Claude Code, Codex and Cursor CLI get `--add-dir` for it.
- The **Flow panel** lists the run's files under the diagram, newest first; click one to read it in the files panel. `cornercase flow run` prints the folder on standard error (`run_dir` with `--json`), and `cornercase status --json` gives each workspace's `run_dir`.
- The folder is kept when the run ends, and removed with its workspace, when you close it or remove its worktree. It lives in `~/.local/state/cornercase/runs/` (next to the session), readable only by you.

## 0.12.112

- **A role of a flow can work in a worktree of its own.** Set a role's **worktree** to **own** in the flow editor (`worktree = "own"` in its file, which then says `version = 2`) and cornercase makes it a worktree and a branch of its own, `<run's branch>--<role>`, from the run's branch: its tab opens there, the project's setup runs there, and its box in the diagram says ` own worktree `. The orchestrator's brief tells how to read each candidate from its branch, and a role that works in the run's worktree merges or cherry-picks the winner; cornercase never merges for you.
- The new built-in flow **Arena** puts two or three competing coders, each on a different agent when you have them, in worktrees of their own, then a judge that tests every candidate, an integrator and a reviewer. It is meant for measurable work or an unclear shape, not for everyday features.
- Two roles that edit the run's worktree side by side in one stage now keep a flow from starting, since they would overwrite each other: give all but one a worktree of their own, or put them in stages of their own. opencode can take several roles when they work apart.
- A role's worktree goes with the run's workspace: removing the workspace checks all of them for uncommitted changes, runs the project's teardown in each and removes them; their branches are kept. One cannot be removed alone while its run lives.

## 0.12.111

- A new **Asana** tab in the issues browser reads tasks from Asana and starts an agent on one like on any other tracker's issue. Connect it from the tab: paste a personal access token from **Settings → Apps → Developer apps** in Asana (`ASANA_ACCESS_TOKEN` works too). When the account belongs to several workspaces, cornercase asks which one to use; it is saved in **Settings → Issues → Asana**.
- The tab lists your open tasks, or, when you set **Asana projects** in **Settings → Issues → Asana** (IDs or links, separated by spaces or commas), the open tasks of those projects. **closed** adds completed tasks, and **people** filters by assignee and creator. In your-tasks mode the assignee is always you, so choosing someone else asks you to set Asana projects instead. Subtasks, milestones and approvals are not listed.
- A task's description and comments are read as Markdown (mentions show as plain `@name`, images as `[image]`). Its branch is named after its ID field, such as `ENG-42-fix-login`, or `asana-<task id>-fix-login` when its project has none.
- `cornercase queue add --issue` takes an Asana link, in the old or the new format, or an ID-field key such as `ENG-42`; a bare task id needs `--source asana`.
- **Settings → Issues → Asana** has the workspace, the projects and the access token. Lists saved before get the Asana tab added once, at the end.

## 0.12.110

- **Community flows.** Anyone can publish a flow: add the topic `cornercase-flow` to a public GitHub repository with a `flow.toml`, and usecornercase.dev/flows lists it, most starred first, within a day, after the built-in flows that come with cornercase, which have pages of their own. `cornercase flow import OWNER/REPO` (or typing it on the Flows import page, or an `https://` address) fetches one, shows its roles and always asks before saving: nobody reviews these flows, and their agents never ask for permission.

## 0.12.109

- **`cornercase contract`** says what the running server supports, for scripts and other programs that drive cornercase: its version, the version of its contract, the boot that its ids belong to, its commands and the version of each stream it sends (`--json` for the full description). `cornercase status --json` now names the boot too.
- Programs talking to the socket can opt into a strict mode by sending `contract` or `boot` with a request: errors then carry a code (`unsupported`, `malformed`, `stale-boot`, `not-found`, `unknown-field`, `unavailable`, `failed`), a field the server does not know is refused instead of ignored, a `contract` newer than the server's is refused as `unsupported`, and a request from an earlier boot is refused before it acts on an id that may name something else now. Requests without them are answered exactly as before. See Scripts and agents → Contract and compatibility.

## 0.12.108

- **Set up each new worktree.** A project's ⋯ → **project settings** takes the commands to run in every new worktree (`npm ci`, copying a `.env`…). They run in the worktree's first tab before its agent starts; the agent starts only if they succeed, and a failure shows a red `!` with the output right there. **teardown** runs before a worktree is removed (a failing teardown is reported but does not stop the removal). **run setup** in a workspace's menu runs the setup again. Saved in `config.json` under `projects`.

## 0.12.107

- `cornercase kill-server` no longer stops the server it runs in without asking. Typed in a terminal inside cornercase, it says what would stop, including that terminal, and asks `Stop it now? [y/N]`; without a terminal, such as from an agent's tool, it refuses unless you pass `--yes`. To stop another server from inside cornercase, set `CORNERCASE_SOCKET` to its socket. Outside cornercase nothing changes.

## 0.12.106

- **Merged pull requests in Stats.** Usage → Stats has a new row, `merged PRs`, with the pull requests and merge requests you opened that were merged, for the last 24 h, 7 days and in total since counting began. It reads them with your own `gh` (every GitHub account you are logged in to, including the ones that are not active, without switching) and `glab`, and counts only repositories that one of your open projects has as a remote, once each however many accounts or projects see them. A `+` after a number means some account could not be read, so the real number may be higher. The row is hidden when `gh` and `glab` are missing, logged out or too old, or when no project has a remote.
- It also shows in `cornercase stats` and its `--json` (`merged_pull_requests`), and goes on the shared card (link version 3) only when every account was read. Turn it off in **Settings → Worktrees → merged PRs in stats** (`merged_pull_requests`); `glab` in `config.json` names the GitLab CLI to run.

## 0.12.105

- **Collapse a workspace.** Click `▾` beside a workspace in the workspaces list to hide its tabs, and `▸` to show them again. Shells and agents keep running; clicking the rest of the row opens the workspace and expands it. Folds are saved with your session and shared with the tree.
- A collapsed workspace shows one icon for its agents, the most urgent first: orange `!`, red `!`, `●`, `✓`, `◐` (or the spinner) and `○`. In the tree, a collapsed workspace now shows working and idle agents too.

## 0.12.104

- Click a workspace's pull request number (`#485✗`) to see its pull request and checks, the same dialog as **pull request #485** in its menu. It works in the lists, the tree and the compact menu, and the number is underlined under the pointer.

## 0.12.103

- The Flows dialog draws the selected flow's diagram beside the list, so you see a flow's plan before opening it. A click selects a flow and a second click edits it, as before; the wheel over the diagram scrolls a tall one. In a narrow dialog, **diagram** shows it in place of the list and **list** brings the list back. A file that cannot be read says why instead of drawing a diagram.

## 0.12.102

- **Cursor CLI is now a full agent.** Tabs running `cursor-agent` show their model and how full the context is, whether the agent is working or finished, and its conversation comes back after a restart (`cursor-agent --resume <chat id>`, in the mode it ran in). `cornercase send --enter` confirms the prompt was taken, `read --last-message` and the search in sessions read its answers, and Stats counts its prompts, turns and time. Settings → Agents lists `cursor` with its modes (plan, ask, auto review, run everything).
- Cursor shows `!` when it needs you only with `/status-indicators` turned on in Cursor; without it a permission prompt shows as working. Cursor keeps no token counts on disk, so Stats has none for it.
- This was built from Cursor CLI's code and has not been checked against a live session: if its chat store looks different from what cornercase expects, the tab shows no model or context rather than a wrong one.
- A tab running a Node 24 program started through a launcher is named after the program instead of `MainThread`.

## 0.12.101

- **Search what was said.** The search has a second mode, **in sessions**, that looks in what your agents were told and answered (Claude Code, Codex, Gemini CLI, opencode and Grok Build) and in everything every pane printed, shells included, in every open project. Click the `❯` button at the right end of the search bar, or press `Tab`, to switch; with a prefix key, `f` `s` opens the search already there. Each result shows the matching line, who said it and `project › workspace › tab`. Choosing one goes to that pane and scrolls to the match, selected.
- It ignores case, runs in the background so typing stays responsive with long scrollbacks, leaves out panes being closed or removed, and lists at most 20 matches per pane and 300 in all.
- `cornercase search TEXT` finds the same matches from a shell, a script or an agent: one line per match with the pane's id, or JSON with `--json`. No match prints a header and exits 0.

## 0.12.100

- The Issues list offers ` start a flow ` beside ` start ` and ` schedule `, so a flow starts on the selected issue without opening it first. It reads the issue and asks where to run it when the tracker needs a project, then opens the same dialog as the issue's own button. A narrow window drops ` schedule ` first, then ` start a flow `, which the issue's page keeps.

## 0.12.99

- After about two weeks of use (days on which a window was open and an agent ran), a small card at the bottom right asks for a star on GitHub. **star on GitHub** copies the link and never asks again, **later** asks again in about three months, and **don't show again** stops it for good. It never appears while an agent is working or waiting for you, never takes the keys from your pane, and is skipped below 90 columns. Turn it off in **Settings → UI → star prompt**, or with `"star_prompt": false` in `config.json`. Nothing is sent anywhere.

## 0.12.98

- **Share a flow with a teammate.** In **Settings → Agents → flows**, **export** copies the selected flow's text to your clipboard, and **import** (or just pasting on the list) opens a flow your teammate sent you in the editor, unsaved, so you can read it before you save it. What the flow sets for its agents (variables and arguments, which run without asking) is shown in red, and agents you do not have installed are named.
- `cornercase flow export NAME > file.toml` and `cornercase flow import FILE` do the same from a shell, into yours or, with `--to project`, the project's flows. A flow that sets variables or arguments asks first (or needs `--yes` without a terminal). A name that is already taken is refused unless you pass `--replace` or `--name`.
- A flow file that cannot be read now says which line is wrong.

## 0.12.97

- **Flows** start a whole team of agents on an issue or a prompt. A flow is a saved team and the plan it follows: an orchestrator agent, and roles such as coder, reviewer, security and tester, each with its own agent, model and effort. The plan is a list of stages, which can work in parallel and go back when there are findings. Choose ` start a flow ` next to ` start ` in an issue, **start a flow…** in the workspaces' `+ New`, or `w s` with a prefix key. cornercase makes a worktree, opens a tab per role and starts every agent there, and the orchestrator runs the plan with cornercase's own commands, from the first message to the pull request.
- Four flows come built in (**Full issue**, **Cross review**, **Quick**, **Bug, test first**), with the agents you have installed. Change them to your liking, or make your own, in **Settings → Agents → flows**: stages, roles, agents, models, efforts and prompts, with the plan drawn beside them. **restore default** brings a built-in flow back. Yours are saved in `flows/` next to `config.json`, and a project's in its `.cornercase/flows/`.
- The new **Flow** panel draws the plan as boxes and arrows and lights it up as the work moves: the box of an agent at work, the arrow the work went along, the rounds of each loop. Click a box to go to that agent, or an arrow to read the last message sent along it. It opens when a flow starts, and from the workspace's menu (**show flow**), the icon bar or `w f`.
- Every agent of a flow runs without asking for permission, so nothing stops the team halfway. A project's flow that sets environment variables or arguments on its agents says so in red before it starts, and Claude Code's one-time consent to that mode is still yours to give.
- `cornercase flow list` and `cornercase flow run` start flows from scripts and agents, and `cornercase start` takes `--model`, `--mode` and `--effort`.
- opencode has two modes in **Settings → Agents**: plan, and auto (which skips its permission prompts).
- The workspaces' `+ New workspace` button is now `+ New`, with **new workspace** and **start a flow…**.
- Text fields with long prompts take line breaks, and wide characters (Chinese, Japanese, emoji) no longer put the cursor in the wrong place, in the TODO panel too.

## 0.12.96

- Schedule issues or prompts for later from the **schedule** button in Issues, the new **Queue** tab at the end of the Issues dialog, or `cornercase queue add`. Choose a clock time, a date, a delay such as `in 2h`, or the next Claude/Codex five-hour plan reset, and pick the agent, mode and destination: a project root, a workspace tab or a worktree. Clock times use your client's time zone, including remote terminals and browsers.
- Every due entry starts together, without moving your active pane. Queued starts submit the stored prompt automatically and always accept the agent's folder trust question, so they can run unattended; immediate starts keep your existing trust setting.
- The queue survives server restarts and keeps started entries so you can see what ran. On the **Queue** tab, edit, remove or start entries now, and clear the started history; a removal can be undone from its notice for a few seconds. Work more than ten minutes late is marked **missed** and waits for you. A closed destination or failed start shows its reason; interrupted starts ask you to check the workspace before retrying.
- `cornercase queue add --issue` takes an issue URL or key. A URL must point to the project's own repository, or to the Jira site or the Plane or Linear workspace you are connected to; a bare key such as `482` that more than one tracker could read asks for `--source`.

## 0.12.95

- cornercase can message you on Discord, as it does on Telegram: while you are away, an agent that needs you or finishes its turn sends you a direct message with its question and buttons to answer it, or what it wrote, and what you reply goes to that agent. `/status`, `/new`, `/away` and `/back` work there too, as slash commands: type `/` in the chat with the bot to pick one.
- To set it up, make an application in the Discord Developer Portal with **User Install** on, **Install Link** set to **None** and **Public Bot** off (so the app stays yours alone; cornercase tells you in Settings when it is not), paste its bot token in **Settings › Remote › Discord › bot**, then select **phone** and scan the code to add the app to your account. No code to type and no server to share: the bot writes to the app's owner. The guide is at usecornercase.dev, **On Discord**.
- Telegram and Discord can both be set up and away at the same time. `o` in the keys menu (now **away on chat apps**) and `cornercase away` turn away on or off on every one that is set up, and the **Remote** menu has an entry for each.

## 0.12.94

- No changes in the app. The Telegram bot's shared parts (the notices, replies, away, `/status` and `/new`) no longer depend on Telegram, so a second chat app can use them.

## 0.12.93

- GitLab is an issue source. The new **GitLab** tab of the issue browser lists the issues of the project your repository's remote points to, on gitlab.com or on your own GitLab server, and **start** works on one in its own worktree, on a branch named after its number (`482-returns-page-crashes`), so GitLab links the merge request from it to the issue and, with its automatic issue closing on, closes the issue when the merge request reaches the default branch. Connect it from the tab with your server's address (leave it empty for gitlab.com) and a personal access token with the `read_api` scope; both can be changed later in **Settings → Issues → GitLab**. A token in `GITLAB_TOKEN` takes precedence instead, and is changed outside cornercase, then `cornercase kill-server`.
- Shortcut and Plane no longer follow a redirect with your token: a server that answers with one is reported in the list, so the token is never sent to another address.

## 0.12.92

- A workspace whose branch has a pull request on GitHub shows its number at the end of its row, coloured by its checks: `#485✗` in red when one failed, `#483•` in yellow while they run, `#482✓` in green when they all passed. It turns purple once merged and is crossed out once closed. cornercase asks `gh` in the background, every 20 seconds while checks run and every minute otherwise.
- The workspace's menu (` ⋯ ` or right-click) has **pull request #485**, which lists every check. **copy link** copies the pull request's address, and **ask agent** hands the failed checks, with links to their logs, to the workspace's agent, for you to add a note. With a prefix key, `g p` opens it.
- It needs `gh` installed and logged in: **Settings → Worktrees → pull requests** says why when none show, and turns it off. `cornercase status` lists each workspace's pull request too.

## 0.12.91

- Agents that report their state with OSC 7501 now drive the status icons, spinner and notifications directly. Other recognised agents, including custom commands, get status indicators when they emit the protocol. Agents without it keep their existing status detection.
- A failed agent shows a red `!`, reports `failed` to scripts and sends a failure notification, including on Telegram while away.

## 0.12.90

- On Telegram, answering a question from its buttons now brings the agent's next question, with its own buttons, when it asks several in a row or shows another permission prompt right away. Before, only the first one came.

## 0.12.89

- The buttons at the bottom of the sidebar can become a column of icons on the left edge, like an editor's activity bar: turn on **Settings → UI → icon bar**. Hover an icon to see its name. The number of changed files, the TODO dot and the Remote dot sit at the corner of their icons, and the lists get the rows the buttons took. The icons need a Nerd Font, which Ghostty, kitty and WezTerm include. Off by default.

## 0.12.88

- Mark an agent as **pending review** when you want to check its work later: right-click its tab, its pane or its row in the agents section and pick **mark as pending review**, or press `m` in the keys menu. Its tab, workspace, project and group show a purple `●` that stays when you look at it, until you pick **mark as reviewed**, send the agent a new prompt or answer its question.
- Marked agents, and the `✓` of agents that finished while you were elsewhere, now come back after a restart or an update, with the agent's conversation.
- Scripts and agents can do the same with `cornercase mark` (`--reviewed` to take the mark off); `status`, `wait` and `events` show the state as `pending`.

## 0.12.87

- A working agent shows the still `◐` again, as before 0.12.84. The spinner is still there: choose it in **Settings → UI → working icon**, or set `"working_icon": "spinner"` in `config.json`.

## 0.12.86

- The stats heatmap always shows its 26 weeks: days without agent work are grey squares, also the ones before cornercase started counting, so a few days of work no longer look like loose squares.
- The shared stats card shows the same rows as the Stats tab: turns, time waiting for you, tokens in and tokens from cache join the ones it had. Links shared before keep opening, with the numbers they hold.

## 0.12.85

- Grok Build is followed like the other agents. A tab shows when it is working, when it needs you and when it has finished, plus the model and how full its context is, including after a turn that called the model more than once. The working icon spins, as it does for the other agents.
- After a restart, a Grok Build pane comes back with `grok --resume`.
- `send`, `read --last-message` and `wait` work with it. A prompt typed while it works waits. Pressing Enter again sends that prompt now and cancels the turn. `/` and `!` are local commands.

## 0.12.84

- An agent that is working shows a spinning `⠋` on its tab, in the tab bar and in the agents section. It steps through `⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏`, about ten times a second, and stops when the agent does. `!`, `✓` and `○` stay still.

## 0.12.83

- Keep working with your agents from Telegram when you leave the computer. Make a bot with @BotFather, paste its token in **Settings › Remote**, and scan the code shown under **phone** to pair your chat.
- Click **Remote**, at the bottom of the sidebar, then **continue on Telegram**, press `o` in the keys menu, or run `cornercase away`. While you are away, an agent that needs you sends you its question, with buttons for its options, and one that finishes sends you what it wrote. Reply to a message and your reply goes to that agent. It stays on until you turn it off the same way, or with `/back` from the chat.
- Remote access from your phone's browser moved to the same **Settings › Remote** tab, and **Remote** now opens a menu with both.
- From the chat, `/status` lists your agents, with buttons to write to one, see its screen or stop its turn, and `/new` starts an agent, in a new worktree if you like.

## 0.12.82

- No changes in the app. Each supported agent now has one reader for its conversation and activity.

## 0.12.81

- No changes in the app. The saved agent stats file can now hold any number of agents: a file with fewer or more is read instead of discarded.

## 0.12.80

- No changes in the app. Supported agents now share one identity, with each agent’s rules kept in its own folder.

## 0.12.79

- No changes in the app. Each supported agent's rules, readers, plan probes and test fake moved into its own folder.

## 0.12.78

- No changes in the app. The socket variable is read in one place, the code keeps one calendar and the base64 crate encodes what the clipboard and Jira need.

## 0.12.76

- No changes in the app. Code that nothing used was removed.

## 0.12.75

- No changes in the app. The source files moved into folders by domain.

## 0.12.74

- No changes in the app. The search, the Every project view and the saved session build their data outside App.

## 0.12.73

- No changes in the app. Projects, workspaces, tabs and panes are found by id in one place.

## 0.12.72

- No changes in the app. Each overlay's view, submission and mouse handling run through their own methods.

## 0.12.71

- No changes in the app. It keeps one side panel at a time.

## 0.12.70

- No changes in the app. It keeps one drag at a time.

## 0.12.69

- No changes in the app. Its state is grouped by concern.

## 0.12.68

- No changes in the app. Background job answers are grouped by domain.

## 0.12.67

- No changes in the app. Background jobs start through one helper.

## 0.12.66

- No changes in the app. The issues browser's code is split into files by concern.

## 0.12.65

- No changes in the app. The settings model is split into files by concern: choosing a row, submitting a pick or an edit, the Issues page, keys and row texts.

## 0.12.64

- No changes in the app. The issues dialog's view and hit testing now live with the app, and its browser takes sizes and hits instead of UI types.

## 0.12.63

- No changes in the app. The ssh window and the client no longer import each other.

## 0.12.61

- No changes in the app. The terminal and the browser access no longer import the app: they send their own events, and the app builds the environment of every shell.

## 0.12.60

- No changes in the app. The settings dialog's view is now built in app/settings_dialog.rs instead of in the settings module.

## 0.12.56

- No changes in the app. Session and config types now belong to their models instead of the UI, and group preset and saved-icon rules live with the groups.

## 0.12.55

- No changes in the app. The plan usage dialog's view is now built in app/usage_dialog.rs instead of in the usage module.

## 0.12.54

- The model, context and last message of a Gemini CLI tab no longer disappear for a moment when Gemini starts a process. The same goes for opencode.

## 0.12.53

- No changes in the app. The code of cli.rs and client.rs moved into files of their own.

## 0.12.52

- No changes in the app. The code that answers commands for scripts and agents moved from app/control.rs into one file per family of requests.

## 0.12.51

- No changes in the app. Helpers that several parts share (the version, file locations, times, saving files, cutting text, lists, HTTP) moved into small modules of their own.

## 0.12.50

- No changes in the app. The tests of changes, context, files, graphics, issues, todo, usage and web moved into files of their own.

## 0.12.49

- No changes in the app. The code that draws the screen moved from ui.rs into one file per part of it.

## 0.12.48

- No changes in the app. The tests of the modules at the top of src moved into files of their own.

## 0.12.47

- No changes in the app. The code that draws the screen moved from ui.rs into one file per part of it.

## 0.12.46

- Switching browser access off and on quickly no longer fails with "port is taken" while a command that just started still holds the port. Tests that failed now and then on a slow machine wait for what they check.

## 0.12.45

- No changes in the app. Its largest source file, app.rs, is now one file per concern.

## 0.12.44

- No changes in the app. The tests that count agent work moved into their files' test modules.

## 0.12.43

- No changes in the app. The tests of the app, ui, server, client and cli trees moved into files of their own.

## 0.12.39

- No changes in the app. The tests of app.rs and ui.rs moved into files of their own.

## 0.12.38

- No changes in the app. The app tests share their helpers, and new tests pin what session.json and config.json look like.

## 0.12.37

- Open cornercase from your phone. Turn on **remote access** in Settings › UI, click the new **Remote** button at the right of the Usage row, and scan the QR code with your phone: the same session opens in its browser, in the compact layout, over [Tailscale](https://tailscale.com), so nothing is published on the internet. Swipe to scroll, tap to click; the screen makes room for the keyboard, and the page reconnects by itself when the connection drops. Only the browsers you pair with a code (once, valid ten minutes) and only your Tailscale account can open it, and Settings › UI forgets them all in one go. The button shows a green dot while a browser is attached. You need Tailscale on the computer and on the phone, with HTTPS certificates turned on for your tailnet; the docs page *From your phone* walks through it.

## 0.12.36

- Usage now opens on Stats: see your agents' work over 24 hours, seven days and all time, with parallelism, prompts, turns, time waiting for you, token kinds and a purple heatmap. Counts stay local and include only agents running inside cornercase.
- Run `cornercase stats`, `--json` or `--share`, including with the server stopped. Share copies a link to a card made in your browser that you can download as a PNG; plan limits remain on the Limits tab.

## 0.12.35

- The **TODO** button shows a grey dot, ` TODO ● ` (or ` ☐ ● ` in the compact layout's top bar), while at least one item is left to do, so a closed list no longer looks the same with ten items as with none. The dot carries no number and goes away once every item is done or the list is empty. Turn it off in **Settings → UI → TODO dot** (`todo_dot` in `config.json`).

## 0.12.34

- In Ghostty (1.0 or later), kitty (0.31 or later) and foot (1.12 or later), the mouse pointer turns into a resize arrow over the borders you can drag: ↔ on the borders between columns and between panes side by side, ↕ on the line between stacked lists, the line above the agents section and between panes one above the other. It stays while you drag and goes back to normal when you let go, when a dialog opens and when the window quits. Other terminals, and cornercase inside tmux, screen or Zellij, keep their usual pointer.
- `Esc` now ends a column or divider drag where it is, instead of reaching the program in the pane. The prefix key ends it too as it opens the keys menu.

## 0.12.33

- No changes in the app. A test that fails no longer leaves a fake agent running, which kept piped test runs from ending.

## 0.12.31

- Scroll back through a pane's output with the mouse wheel, over a shell or any program that doesn't use the mouse. The wheel scrolls the pane under the pointer without making it active, and ` ↓ n more ` at the bottom right says how far up you are; new output doesn't move what you are reading. Click it, type or paste to get back to the bottom. You can select and copy text up there too. In `less`, `man` and git's pager, the wheel scrolls the page.
- Each pane now keeps its last 10 MB of output, as Ghostty does: about 14,000 lines at 80 columns. It kept far fewer lines than the 5,000 it was meant to (about 600), so `cornercase read --lines` reaches much further back too.

## 0.12.30

- Restarting cornercase (after an update, from Settings, or with `cornercase restart` or `cornercase update --yes`) no longer flashes the shell you started it from and a blank screen: the window keeps showing your session until it comes back. The first restart into this version still flashes once, since the old version does it.

## 0.12.29

- The Changes panel's button now reads **expand all** and **collapse all** (it said **unfold all** and **fold all**), the words other review tools use for it. The settings note on counts says *collapsed group* too.

## 0.12.28

- `Esc` now closes the changes, files and TODO panels, as their `×` does. A panel field with text clears first, so `Esc` `Esc` clears the search, filter or TODO text and then closes the panel. While a panel is open, that `Esc` no longer reaches your pane; in the compact layout it used to reach a pane hidden behind the panel, where it could interrupt an agent unseen. Press `Esc` again once the panel is closed to reach your program.

## 0.12.27

- Fix windows that stop accepting keys and clicks after startup, including over SSH. The smaller terminal query avoids dropped replies, late replies stay out of your shell or agent prompt, and malformed input no longer disables the window's terminal modes. A lost terminal now detaches cleanly while its shells keep running.

## 0.12.26

- Deleting a group that holds projects now asks what to do with them: **keep projects** leaves them open without a group, as before, and **close projects** closes them along with the group, stopping their shells, instead of closing them one by one afterwards. Folders and worktrees stay on disk. `Enter` still keeps the projects.

## 0.12.25

- Review uncommitted changes across every project in the Changes panel, grouped by project and workspace. Files start folded and load their diff when opened; filters, viewed marks and hunk actions work across the collection.
- Keep your chosen Changes scope across sessions. Background scans visit one workspace at a time and read patches only for unfolded files.

## 0.12.24

- New setting, **keep awake** (Settings → Agents, off by default): while an agent is working, the computer no longer goes to sleep on its own, so a long refactor or test run started before you walk away is finished when you come back. It lets the computer sleep again 30 seconds after the last agent stops working; an agent waiting for you, or one whose turn is over while a shell it started in the background still runs, does not keep it awake. The display still turns off and the screen still locks, and closing the lid still sleeps. It uses `caffeinate` on macOS and `gnome-session-inhibit` or `systemd-inhibit` on Linux; the row says whether the computer is being kept awake now, or why it can't be on this machine. `cornercase status --json` shows it as `keep_awake`.

## 0.12.23

- Gemini CLI tabs now show working, waiting and finished status, notifications, model and context use, and resume their conversations after a restart in the same mode. The status needs Gemini's dynamic window title (on by default). When two Gemini CLI instances run in the same folder with the same Gemini home, cornercase cannot tell their conversations apart, so neither tab shows its model and context, resumes, or answers `read --last-message`; their status icons still work.
- Scripts can wait for Gemini, follow its events, confirm submitted prompts and read its last message. Local slash commands and shell input return after Enter without a prompt confirmation.
- Gemini CLI has a built-in plan mode.

## 0.12.22

- A new **Plane** tab in the issues browser reads work items from Plane (plane.so), on Plane Cloud or a self-hosted server, and starts an agent on one like on any other tracker's issue. Connect it from the tab: type your workspace (the part after `app.plane.so/`, such as `acme`), then paste an API key from **Profile settings → Personal access tokens**; `PLANE_API_KEY` works too. The list shows the work items that are not done, newest first, and **people** filters them by assignee and creator, with *me* or any member. A work item's description and comments are read as Markdown, and its branch is named after its key, such as `ENG-42-fix-login`.
- **Settings → Issues → Plane** has the workspace, the URL of a self-hosted server (empty is Plane Cloud), a filter and the API key. The filter takes query parameters, such as `priority=high&label_id=…`, added to what the list asks for. Lists saved before this version get the Plane tab once; hide it in **Sources shown**.

## 0.12.21

- Click a file path in a pane to read it in the files panel even when it is outside the workspace, such as `/tmp/report.md`, a plan in `~/.claude/plans/` or a file in another project or worktree. Paths beginning with `~/` now expand to your home folder. Outside files show their absolute path, which **open**, **ask agent** and **copy** use too; they have no git marks. **`‹`** returns to the tree or your search results, and closing and reopening the panel keeps the file.

## 0.12.20

- `cornercase read --last-message` reads several agents in one call: repeat `--pane` or `--tab` to get each message under a header with its pane, agent, whether its turn is over and when it was written. `--json` puts their answers in a `panes` list. A pane without a message gets its own error, the others are still read, and the command exits 1 once it has read them all. Reading one agent keeps the same output as before.

## 0.12.19

- The files panel shows images. A click on a PNG, JPEG, GIF, WebP, BMP or ICO file shows the picture itself, drawn by your terminal, with its format, size in pixels and file size above it; ` ask agent ` and ` copy ` take its path. Ghostty and kitty show it with kitty graphics, iTerm2, WezTerm and Warp with iTerm2's inline images, and foot, Konsole and xterm (started as a VT340) with sixel; inside tmux, Ghostty and kitty need `set -g allow-passthrough on`, and a tmux built with sixel draws sixel pictures itself, with no setting. A terminal that cannot show images gets, where the picture would be, a line saying why and what to do about it. Every window attached to the server gets what its own terminal can show, in its own size, and an image an agent rewrites shows again as it changes. `CORNERCASE_IMAGES=kitty`, `iterm`, `sixel` or `off`, set where you start cornercase, picks for a terminal cornercase does not recognise.
- `cornercase remote` no longer loses its window when the link to the other machine is slow while a large frame, such as an image, is on its way.

## 0.12.18

- `cornercase restart --when-idle` and `cornercase update --when-idle` restart the server only once no agent is working, so none loses a turn in the middle of a tool call or a build. Idle, done and waiting for you count as stopped, and so does a Claude Code agent whose turn is over while a shell it started in the background still runs. The server restarts in the same moment it sees that, so no agent starts a new turn in between. Meanwhile every window shows `restart pending until 2 agents end their turn` in its corner, with a **cancel** button. `--timeout` gives up after that many seconds and keeps the server running, as does stopping the command. `update --when-idle` installs the release at once and restarts on it once the agents are done.

## 0.12.17

- `cornercase send` no longer answers Claude Code's feedback survey. While Claude Code shows `How is Claude doing this session? (optional)` with `1: Bad  2: Fine  3: Good  0: Dismiss` above its input box, a digit sent alone, such as `send --enter 2` answering an agent's numbered question, rated the session instead and the prompt was lost. `send` now refuses while the survey shows and says how to dismiss it, `cornercase keys --pane N 0`; `--force` sends anyway. `cornercase status` marks such a pane `(survey open)` (`survey` in `--json`, and a new last column of `status --panes`).

## 0.12.16

- `cornercase status --panes` lists every pane on a line of its own, for scripts: the ids of the pane, its tab, workspace and project, what runs in it and what its agent is doing, the workspace's branch and the pane's folder, in columns separated by tabs under a header line. `--panes --json` gives the same rows as JSON, in one flat list. A script or an agent coordinating others no longer has to walk the tree of `status --json` to find a pane by its agent, status or branch. `status` and `status --json` stay as they were.
- `cornercase status` no longer ends with a panic message when what reads its output stops early, as `head` does.

## 0.12.15

- A click outside a dialog now closes it, as its cancel button would: settings, issues, usage, the folder picker, the update and restart dialogs, confirmations and forms, like right-click menus and the search already did. A dialog that is busy, such as one creating a worktree or installing an update, stays open until it is done.

## 0.12.14

- Tabs without an agent now show an icon before their name too, so every tab's name starts in the same column: a grey `▸` while a program runs in the tab (a dev server, a build, an editor) and a grey `›` while the shell waits at its prompt. They never light up the workspace or project rows.
- With **settings → UI → memory** on, every tab shows how much memory it uses, not only agent tabs: a shell tab running `yarn dev` gets `412 MB` on a second line. A split tab counts every pane, so a shell next to an agent now counts too.

## 0.12.13

- Nothing changes in the app. A test that checks how cornercase tells two opencode in one folder apart failed now and then on macOS; it now runs a fake opencode that cannot be mistaken for a second one.

## 0.12.12

- `cornercase read --last-message` prints the last message the coding agent in a pane wrote, as plain text. It comes from the agent's own record (Claude Code's transcript, Codex's rollout, opencode's database) rather than the screen, so it is whole even once it scrolled off, and free of the input box, the status lines and Claude Code's grey suggestions. `--json` adds when it was written and whether the agent's turn is over. An agent that starts others can now read their answers directly, instead of asking each one to write its report to a file.

## 0.12.11

- `cornercase send --enter` now makes sure the prompt was submitted: for Claude Code, Codex and opencode it returns once the agent has recorded the prompt in its own history, and fails with `not confirmed: the prompt may not have been submitted` when it records nothing, instead of reporting success for a prompt that went nowhere. A Codex that is working keeps a prompt until its next step, so `send` returns then; `--timeout` now works without `--wait` to give up sooner.
- `cornercase send` refuses a Claude Code agent that shows a dialog, a panel or its `!` shell mode instead of its input box, such as the details of a background shell, where the text could stop the shell and the Enter would just close the panel. `--force` sends anyway. `cornercase status` shows such an agent as `(dialog open)`, and `"dialog": true` in `--json`.

## 0.12.9

- The sidebar, the panels and the dialogs start their labels and titles with a capital letter, so they read as names: **Projects**, **Workspaces**, **+ New workspace**, **+ Tab**, **Issues**, **Changes**, **Files**, **TODO**, **Settings**, **Usage**, **Quit**, the changes panel's **Uncommitted**, **Commits** and **All**, and dialog titles such as **New workspace** or **Remove workspace**. Menu entries and the buttons inside dialogs stay as they were.

## 0.12.8

- `cornercase events` follows what happens as it happens, for scripts and agents that coordinate other agents: it prints a line when an agent starts working, waits for an answer, is left with a background shell, finishes or goes idle, when the program in a pane ends, and when a project, workspace, tab or pane opens or closes. `--pane` keeps only some panes and stops once they have all closed, and `--json` prints one JSON object per line, with the ids, the old and new state and the time. Without `--pane` it keeps running until the server stops, so a script can react to each change instead of polling `cornercase status`.

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
