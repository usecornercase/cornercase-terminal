pub(super) const ABOUT: &str =
    "A terminal multiplexer for projects, git worktrees and coding agents, driven by the mouse";
pub(super) const LONG_ABOUT: &str =
    "A terminal multiplexer for projects, git worktrees and coding agents, driven by the mouse.

Without a command, cornercase opens its window, starting the server if none is running. The other
commands drive the running server from scripts, git hooks and the agents in its panes; they never
start a server, except `start-server`.";
pub(super) const AFTER_HELP: &str =
    "Ids come from `cornercase status` and last while the server runs. Inside a pane, the commands
act on that pane, its tab, workspace and project unless told otherwise; elsewhere on what the window
shows. Where a command takes --workspace ID, --worktree BRANCH names the workspace on that branch
instead, in that same project. Only `focus` and `--focus` change what the window shows, or with --view ID what that view shows (see `cornercase view`). The
commands exit with 1 on errors and timeouts, and 2 on wrong usage, a branch no workspace is on
included, or one several are on (both list the workspaces to pick from).

Examples:
  cornercase status
  pane=$(cornercase new-tab --name tests -- cargo test)
  cornercase wait --pane \"$pane\" --until shell --timeout 900
  cornercase read --pane \"$pane\" --lines 40
  cornercase start claude --worktree fix-login --prompt 'Fix the login form' --wait";
pub(super) const STATUS_HELP: &str =
    "Each pane shows its program and folder, and for a coding agent what it is doing (working,
waiting for an answer, done out of sight, failed, idle), its model and how full its context is. `(you)`
marks the pane running the command, `(shown)` what the window shows. `working (background shell)`
is a Claude Code agent whose turn is over while a shell it started in the background still runs:
it wakes up when that shell ends (`background_shell` in the JSON, where the status stays working).
`(dialog open)` is a Claude Code agent showing a dialog, a panel or its shell mode instead of its
input box, which `cornercase send` refuses (`dialog` in the JSON).
`(survey open)` is Claude Code's feedback survey above its input box, which `cornercase send`
refuses too (`survey` in the JSON): `cornercase keys --pane N 0` dismisses it.
`(resuming)` is a pane whose agent is coming back in its conversation after a restart: until it
is back, it takes no input and `cornercase send` refuses it (`resuming` in the JSON).

With --panes it prints one line per pane instead, for scripts, in the sidebar's order: a header
line naming the columns, then one row per pane, its columns separated by tabs. The columns are
pane, tab, workspace, project (their ids), program, agent, status, background_shell, dialog,
at_prompt, model, context, caller (the pane running the command), shown (the pane the window
shows), active (its tab's active pane), tab_name, workspace_name, branch, worktree, project_name,
path, survey (Claude Code's feedback survey shows) and resuming (the agent is coming back in its
conversation); new ones only ever go at the end. Values are as in the JSON (true or false, context as a
number, full paths), with - where there is none and a space for a tab or line break in a name.
With --json, it prints {\"panes\": [...]}, one object per pane with the columns as keys and null
where there is no value.

With --follow it keeps running and prints the whole state as one JSON line: first as it is now,
then again shortly after each time something in it changes, always JSON, with
--json or without. Each line is what --json prints plus `revision`, which grows with every line,
with `caller` always null;
`boot` names the server's run, and revisions only compare within one boot. Each line replaces the
one before. It ends when the server stops or restarts; to go on, run it again, and its first line
is the state then, so nothing is missed. A reader that falls behind reads at most four older
lines, then the latest state.

Examples:
  cornercase status
  cornercase status --json
  cornercase status --panes | awk -F'\\t' '$7 == \"waiting\" { print $1 }'
  cornercase status --panes --json | jq '.panes[] | select(.agent) | .pane'
  cornercase status --follow | jq -c '[.revision, (.projects | length)]'";
pub(super) const OPEN_HELP: &str = "Examples:
  cornercase open ~/src/shop
  cornercase open . --focus";
pub(super) const NEW_WORKSPACE_HELP: &str =
    "The workspace gets one tab. A worktree goes in the worktrees folder from settings, on a new
branch from HEAD when the branch does not exist yet; the command returns once git made it. The
project's setup (settings → project settings) then runs in the new tab, which `start --worktree`
waits for before it types the agent.

Examples:
  cornercase new-workspace experiments
  cornercase new-workspace fix/login --worktree --project 3";
pub(super) const COMMAND_HELP: &str =
    "The words of COMMAND are joined with spaces and typed at the prompt once the shell is ready,
as ssh does, so quote what the shell should read as one argument. The command returns once it
is typed.";
pub(super) const NEW_TAB_HELP: &str = "Examples:
  pane=$(cornercase new-tab --name server -- npm run dev)
  cornercase new-tab --workspace 5 -- 'cargo test 2>&1 | tee test.log'";
pub(super) const SPLIT_HELP: &str = "Examples:
  cornercase split -- htop
  cornercase split --pane 7 --down -- tail -f server.log";
pub(super) const START_HELP: &str =
    "The agent starts with its command and arguments from settings → agents. --mode replaces the mode
chosen there by another of the agent's modes, --model adds the agent's own model flag and --effort its
effort (Claude Code, Codex, Grok Build and GitHub Copilot CLI take one; an older server refuses all three rather than
start the agent without them). If it asks whether
you trust the folder, the question waits for your answer unless settings → agents accept it for
you. Once it is ready, the prompt is pasted and
submitted, and the command prints the pane's id; it fails if the agent never shows up. With
--wait, a second line says how the wait ended: idle, done (it finished out of sight) or waiting
(it needs an answer); with --until it waits for that state instead, as `cornercase wait --until` does.

Examples:
  cornercase start claude --worktree fix-login --prompt 'Fix the login form, then commit'
  cornercase start codex --prompt-file task.md --wait --timeout 1800
  cornercase start claude --mode plan --model opus --prompt 'Plan the login rework'";
pub(super) const SEND_HELP: &str =
    "The text goes in as one paste, bracketed when the program asked for it. A pane whose agent
waits for an answer to a question or a permission prompt is refused, since the text would answer
it; use `cornercase keys` for that. So is a Claude Code agent that shows a dialog, a panel or
its shell mode instead of its input box (`dialog` in `cornercase status --json`), unless you pass
--force. So is its feedback survey above the input box (`1: Bad  2: Fine  3: Good  0: Dismiss`,
`survey` in the JSON), where a digit sent alone would answer it: dismiss it with
`cornercase keys --pane N 0`, or pass --force. So is a pane whose agent is resuming its conversation
after a restart (`resuming` in the JSON), where the text would reach its shell or its start-up:
send again once it is back, or pass --force.

With --enter, the command returns once Claude Code, Codex, Gemini CLI, opencode, Grok Build, Cursor CLI or
GitHub Copilot CLI has recorded the prompt in its own history, and fails with `not confirmed: the prompt may not
have been submitted` when it records none: read the pane before sending it again. A working Codex,
Gemini, Grok Build, Cursor CLI or GitHub Copilot CLI holds a prompt until it takes it, so the command returns then.
Grok Build queues that prompt; pressing Enter again sends it now and cancels the turn. Gemini local
slash commands, ? help and ! shell input, and the / and ! commands of Grok Build and Cursor CLI, and the / and !
commands and a lone ? of GitHub Copilot CLI, return after Enter without confirming a prompt record. In other programs it returns once Enter is pressed.

With --wait, the command fails if the agent does not start working within 10 seconds of the Enter,
and otherwise prints how the wait ended; with --until it waits for that state instead, as
`cornercase wait --until` does.

Examples:
  cornercase send --pane 12 --enter 'Now add tests for it'
  cornercase send --pane 12 --enter --wait --timeout 900 - < next-step.md
  cornercase send --pane 12 --enter --wait --until turn-over 'Run the tests'";
pub(super) const KEYS_HELP: &str = "Keys are encoded the way the program in the pane asked for.

Examples:
  cornercase keys --pane 12 ctrl+c
  cornercase keys --pane 12 down enter";
pub(super) const UNTIL_AFTER: &str = "Wait until the pane is in this state instead";
pub(super) const KEY_NAMES: &str = "enter, esc, tab, backspace, space, up, down, left, right, home, end, pageup,
pagedown, delete, insert, f1 to f12 or one character, each after any of ctrl+, alt+ and shift+";
pub(super) const READ_HELP: &str =
    "Lines the terminal wrapped come back joined, and empty lines at the end are left out.

With --last-message it prints the last message the agent in the pane wrote, as plain text, from
its own record (Claude Code's transcript, Codex's rollout, Gemini's session file, opencode's
database, Grok Build's session, Cursor CLI's chat store, GitHub Copilot CLI's session events) instead of the
screen, so it comes whole even once it scrolled off, without the input box or status lines. While the agent works
it is the newest one so far. --json adds when it was written and whether the agent's turn is over. It fails on a
pane without Claude Code, Codex, Gemini CLI, opencode, Grok Build, Cursor CLI or GitHub Copilot CLI, and before
the agent wrote anything.

With --last-message, repeat --pane or --tab to read several agents at once. Each message has a
header with its pane, agent, turn_over and written time; --json gives {\"panes\": [...]}. Panes come
first, then tabs, in the order given, and a repeated id is read once. Errors stay with their target,
and it exits 1 after reading them all if any failed. One target keeps the output above. Reading
the screen takes only one target.

Claude Code's feedback survey (`How is Claude doing this session? (optional)`, then
`1: Bad  2: Fine  3: Good  0: Dismiss`) may show above its input box: it is Claude Code asking you,
not the agent's output. `cornercase keys --pane N 0` dismisses it.

Examples:
  cornercase read --pane 12
  cornercase read --pane 7 --lines 200 > build.log
  cornercase read --pane 12 --last-message
  cornercase read --tab 4 --last-message --json
  cornercase read --last-message --pane 12 --pane 15 --json";
pub(super) const WAIT_HELP: &str =
    "By default it waits until the agent stops working: idle, done, failed, pending (marked for review) or
waiting. A Claude Code agent whose turn is over while a background shell it started still runs
counts as working, since it wakes up when the shell ends; --until turn-over also ends there, and
prints shell. It then prints how it ended (idle, done, failed, pending, waiting, working, shell or quiet),
or the line that matched.
Waiting for an agent needs OSC 7501 reports, or a reader for Claude Code, Codex, Gemini CLI,
opencode, Grok Build, Cursor CLI or GitHub Copilot CLI in the pane. Without reports, Gemini status needs its dynamic window title,
and Cursor CLI's waiting needs /status-indicators. A timeout exits with 1 and does not prove that the
agent missed what you sent: read the pane before sending it again.

Several panes and tabs take --any or --all, and the condition applies to each. --any returns once
one of them meets it and prints one line, its id and how it ended; --all returns once every one has
met it, and prints one such line each, in the order given. The id is the one given, so a tab's line
starts with the tab's id. A pane that closes meanwhile ends as `closed` instead of failing the wait.

Examples:
  cornercase wait --pane 12 --timeout 600
  cornercase wait --pane 7 --until shell
  cornercase wait --pane 12 --until turn-over --timeout 600
  cornercase wait --pane 7 --text 'test result: (ok|FAILED)'
  cornercase wait --any --pane 12 --pane 15 --tab 9 --timeout 1800";
pub(super) const CLOSE_HELP: &str =
    "Its shells stop, as with its ×, but nothing asks first, not even for a project. A worktree's
workspace stays listed while its worktree exists, and its branch is never deleted.
--remove-worktree runs the project's teardown first; a teardown that fails is a warning on
stderr, and the worktree is removed anyway.

Examples:
  cornercase close --tab 9
  cornercase close --workspace 5 --remove-worktree
  cornercase close --worktree fix/login --remove-worktree";
pub(super) const RENAME_HELP: &str = "Examples:
  cornercase rename 'review #42'
  cornercase rename --workspace 5 ''";
pub(super) const FOCUS_HELP: &str =
    "Shows it in the window, or with --view ID in that view only (`cornercase view`), which then
follows it: the window and other views stay where they are. With --view it prints the view's new
tab and its `projection`, the number a view's lines and surface frames carry, which goes up each time
what the view shows changes.

Examples:
  cornercase focus --pane 12
  cornercase focus --view 41 --tab 7";
pub(super) const NOTIFY_HELP: &str =
    "The desktop notification goes through the terminal of every open window, like the one for an
agent that needs you.

Examples:
  cornercase notify the release build is ready";
pub(super) const AWAY_HELP: &str =
    "It needs a Telegram or Discord bot paired with your phone (Settings › Remote), and turns away on
for each one paired. While you are away, an agent that waits for you or ends its turn sends its
question or its last message to that chat, and what you reply goes to the agent, until you turn it
off: here with --off, from the chat with /back, or from Remote in the window.

Examples:
  cornercase away
  cornercase away --off";
pub(super) const SEARCH_HELP: &str =
    "Looks in what the coding agents in the panes were told and answered (Claude Code, Codex, Gemini CLI,
opencode, Grok Build, Cursor CLI and GitHub Copilot CLI, the same records as `read --last-message`) and in the history of every
pane, shells included, ignoring case. Panes of projects being closed and workspaces being removed
are left out. It prints one line per match, newest first within a pane, its columns separated by
tabs under a header: the pane, the agent or program, where the text was found (prompt, reply or
screen), `project › workspace › tab`, the line on the pane's screen (screen matches only), and the
text. A missing value is -. No match prints the header alone and exits 0. With --json each match
also has its tab, workspace and project ids and, for an agent's message, when it was written.

Examples:
  cornercase search 'migration failed'
  pane=$(cornercase search --json checkout | jq -r '.matches[0].pane')
  cornercase focus --pane \"$pane\"";
pub(super) const MARK_HELP: &str =
    "A pane marked as pending review shows a purple ● on its tab, and on its workspace, project and
group rows, until it is marked as reviewed or its agent starts working again on a new prompt or an
answer to its question. Looking at the tab does not take it off, unlike the ✓ of an agent that finished while you looked elsewhere.
Both marks are saved with the session, so they come back after a restart with the conversation.

Examples:
  cornercase mark
  cornercase mark --pane 12
  cornercase mark --pane 12 --reviewed";
pub(super) const FLOW_HELP: &str =
    "A flow is a team of coding agents and the plan they follow: one tab per role in a new
worktree, every agent started without permission prompts, and an orchestrator agent that sends the
work from role to role with these commands. The built-in flows come with cornercase; yours live in
flows/ next to config.json, and a project's in .cornercase/flows/ in its repository.

Examples:
  cornercase flow list
  cornercase flow run Quick --prompt 'Fix the login form'
  cornercase flow run 'Full issue' --prompt-file task.md --worktree fix-login
  cornercase flow export Quick > quick.toml
  cornercase flow import quick.toml";
pub(super) const FLOW_RUN_HELP: &str =
    "It makes the worktree (or uses a worktree already on that branch, if it runs no flow), opens a
tab per role named after it, the orchestrator first, and starts every agent with its role prompt,
answering their trust prompts. The orchestrator starts last, with a brief: the task, the team and
its pane ids, the stages, and how to drive the agents. Nothing is shown in the window unless you
pass --focus.

Without --worktree, the branch comes from the task's first words, with a short suffix when that
says nothing, is a default branch or is already taken. A chosen branch that is main, master, the
project's default or the one its own folder is on is refused: a flow works on a branch of its own.

A role set to work in a worktree of its own (worktree = own) gets one on the branch <branch>--<role>, made from the
run's branch, with the project's setup; the other roles work in the run's worktree.

It prints the workspace's id, then one line per role: its pane's id and the role's name, separated
by a tab. The run's folder for files, where every role writes its answers, is printed on stderr
(run_dir with --json) and is in $CORNERCASE_RUN_DIR in the team's panes. A name is looked up in yours, then the built-in flows, then the project's: yours replaces
a built-in flow of its name, and a project's flow never stands in for another of the same name; the
command says so on stderr when it happens. A built-in flow deleted in Settings › Flows is refused,
and keeps its name: a project's flow named like it does not run either.

Examples:
  cornercase flow run Quick --prompt 'Fix the login form'
  cornercase flow run 'Bug, test first' --prompt-file bug.md --worktree fix-crash --json";
pub(super) const FLOW_EXPORT_HELP: &str =
    "It prints the flow as TOML, the file a flow is kept in, so a teammate can read its prompts, variables
and arguments before importing it. A name is looked up as `flow run` does. Variables a flow sets are
named on stderr: check no secret goes with them.

Examples:
  cornercase flow export Quick > quick.toml
  cornercase flow export 'Full issue' --json";
pub(super) const FLOW_IMPORT_HELP: &str =
    "It reads a flow at most 256 KiB long, lists its roles, and saves it in yours (or the project's, with
--to project). The source is a file, - for standard input, an https:// address, or OWNER/REPO, a GitHub
repository with a flow.toml (or cornercase-flow.toml) at its root; usecornercase.dev/flows lists them.
Every agent of a flow runs without permission prompts, so a flow from an address or a repository,
which nobody reviewed, always asks first, and so does one that sets
variables or arguments; without a terminal it refuses unless you pass --yes.

A name that is already taken is refused: pass --replace to overwrite that flow, or --name to import
this one under another name. A built-in flow's name is yours to take, with --replace, and it then
stands in its place. A project's flow never takes the place of one of yours or a built-in.

Examples:
  cornercase flow import quick.toml
  cornercase flow import octocat/security-audit
  cornercase flow import https://example.com/flows/mine.toml
  cornercase flow import quick.toml --to project --name 'Quick mine'
  ssh host cornercase flow export Quick | cornercase flow import - --yes";
pub(super) const TODO_HELP: &str = "Examples:
  cornercase todo add review the login fix
  cornercase todo list
  cornercase todo done 3
  cornercase todo send 3 --pane 5";
pub(super) const TODO_SEND_HELP: &str = "Types the item's text into the agent and presses Enter, as
`cornercase send --enter` does, with the same target, refusals and confirmation that the agent took
the prompt. Once it did, the item is checked off. A refused or unconfirmed send leaves it pending,
prints the reason and exits with status 1. When cornercase cannot confirm the send at all (a local
`/` command, an agent it cannot watch), the text is still sent, the item stays to do, and the command
prints that and exits 0, with `unconfirmed: true` in --json. `checked` in --json says whether the
item was checked off; it is false when the item was edited or removed while it was sent. Only a
pending item can be sent.

Examples:
  cornercase todo send 3
  cornercase todo send 3 --pane 5 --json";
pub(super) const EVENTS_HELP: &str = "It keeps running and prints one line per event, as it happens:
  pane 5  claude  working → waiting   an agent changed state: working, shell (its turn is over but a
                                      background shell it started still runs), waiting, done, idle,
                                      or none once it left the pane
  pane 7  cargo  exited               the program in the foreground ended and the shell is back
  tab 3  opened in workspace 2        a project, workspace, tab or pane opened, or closed
With --host, a program's own requests to the terminal are told too, for programs that draw the panes:
  pane 7  clipboard  11 bytes         it wrote to the clipboard (OSC 52); the text itself only with --json
  pane 7  bell  3 times               it rang the bell: one line a second per pane, with how many rang
With --json, each line is a JSON object with the event, the ids, the old and new state and the time.

It ends when the server stops, or once every pane given with --pane has closed. Programs are looked
at every half second while something follows events, so one that ends sooner may not show. Start it
before what you want to follow, or read `cornercase status` once it runs, so nothing falls between.
A reader that falls far behind loses events, and a line says how many.

Examples:
  cornercase events
  cornercase events --pane 12 --pane 14 --json
  cornercase events --host --json";
pub(super) const SURFACE_HELP: &str =
    "For programs that draw cornercase's terminals themselves, such as an editor extension or a
GUI. It keeps running and prints the tab as one JSON object per line: first as it is now, then again
each time something it shows changes, at most about 60 times a second. Each line is complete and
replaces the one before. It holds the panes laid out at the given size, each with its place (rect),
its terminal's own size (size), its cells (with their hyperlinks), the colours its program changed,
cursor, modes and scroll position, plus the split tree, the dividers, and boot, session and revision.

Each line names its stream with `surface`, which `cornercase resize` takes to lay a --tab surface out at
another size. With --tab the surface resizes no terminal: terminals keep the size of whoever sizes the tab (the
window, or a view), so size can differ from rect. With --view ID it streams the tab that view
shows, at the view's size, and follows the view to other tabs (each line carries view and
projection); it pauses while the view is inactive or has no size, and ends when the view closes.
The window keeps showing what it showed, also when the tab is not the one on screen. It ends when the tab closes or
the server stops (status 0); stopping it leaves the shells running. A reader that falls behind gets
fewer lines, each complete.

Examples:
  cornercase surface --tab 3 --cols 120 --rows 40
  cornercase surface --tab 3 --cols 120 --rows 40 | jq -c '[.revision, [.panes[].pane]]'
  cornercase surface --view 41";
pub(super) const INPUT_HELP: &str = "For programs that draw cornercase's terminals themselves (see `cornercase surface`). Each
EVENT is one JSON object, sent in order:
  {\"key\": {\"code\": \"c\", \"mods\": [\"ctrl\"]}}   code as for `keys`; mods ctrl, alt, shift, super; kind press, repeat, release
  {\"text\": \"héllo\"}                           typed text, without control characters
  {\"paste\": \"two\\nlines\"}                     bracketed when the program asked for it
  {\"mouse\": {\"kind\": \"down\", \"button\": \"left\", \"x\": 3, \"y\": 1}}
                                             kind down, up, drag, moved, scroll-up, scroll-down,
                                             scroll-left, scroll-right; x and y are cells of the
                                             pane's terminal, from 0
- reads one event per line from standard input.

Events are encoded the way the program in the pane asked for (its keyboard protocol, application
cursor keys, bracketed paste, mouse mode), whatever the window shows: a dialog or a field there
takes nothing. Keys, text and pastes bring the pane's view back to the bottom. Mouse events and key
releases that the program does not take are not sent, and said on stderr. When one event is wrong,
or the program is not reading its input, nothing is sent. Scripts and agents should use `send` and
`keys`, which check before typing into an agent.

Examples:
  cornercase input --pane 12 '{\"text\": \"ls\"}' '{\"key\": {\"code\": \"enter\"}}'
  cornercase input --pane 12 '{\"mouse\": {\"kind\": \"scroll-up\", \"x\": 10, \"y\": 5}}'";
pub(super) const SCROLL_HELP: &str =
    "Moves the pane's view through its history, as the wheel does in the window, which shows the same
view. On its alternate screen, or when it reads the mouse, a program scrolls itself: send it keys or
the wheel with `cornercase input`.

Examples:
  cornercase scroll --pane 12 --up 10
  cornercase scroll --pane 12 --bottom";
pub(super) const SELECTION_HELP: &str =
    "Without --from and --to, prints what is selected in the pane. With them, selects from one cell to
another of its view, columns and rows from 0 as `cornercase surface` numbers them; the window shows
the same selection. --copy also copies it to the clipboard through the window, as selecting there
does, and prints what it copied, so a program without a window can put it in the clipboard itself.

Examples:
  cornercase selection --pane 12 --from 0,3 --to 79,5
  cornercase selection --pane 12 --copy
  cornercase selection --pane 12 --clear";
pub(super) const RESIZE_HELP: &str =
    "Lays a running `cornercase surface` out at another size, without starting it again: its next line
comes at that size. ID is the `surface` of its lines. The terminals keep the size the window gives
them.

Example:
  cornercase resize --surface 4 --cols 100 --rows 30";
pub(super) const RATIO_HELP: &str =
    "RATIO is the share of the divider's first side, between 0 and 1. --path names the divider as a
surface line's `path` does, first or second at each level down; without it, the outermost one. The
terminals follow at once.

Examples:
  cornercase ratio --tab 3 0.3
  cornercase ratio --tab 3 --path second 0.5";
pub(super) const LINK_HELP: &str =
    "For programs that draw cornercase's terminals themselves (see `cornercase surface`). Prints the
file a click on that cell opens in the window: the path a program printed there, looked up from the
folder of the program in the foreground, then from the root of the pane's workspace, with its line or
lines (path:12, path:12-30). CELL is the column and row of the pane's view, from 0, as a surface frame
numbers them. Prints nothing when the cell holds no such path. --json also says whether the file is
inside the pane's workspace and which cells the path takes.

Examples:
  cornercase link --pane 12 --cell 10,3
  cornercase link --pane 12 --cell 10,3 --json";
pub(super) const SKILL_HELP: &str = "Install them for Claude Code, Codex and other agents with
  npx skills add usecornercase/cornercase-terminal --skill cornercase -g
or put this text in an AGENTS.md or CLAUDE.md.";

pub(super) const REMOTE_HELP: &str =
    "cornercase runs `ssh DESTINATION cornercase proxy`, so your ssh config, keys, agent and
ProxyJump apply, and starts the server there if none is running. The shells, agents and files stay
on that machine; this one only shows them, and copies and notifications go through this terminal.
If the connection drops, the window stays and reconnects by itself; Esc or quit gives up. Both
machines need the same version of cornercase. Reconnecting never asks for a password, so use a
key or an agent.

Examples:
  cornercase remote devbox
  cornercase remote me@10.0.0.5 --command '~/bin/cornercase'";
pub(super) const LOGS_HELP: &str =
    "The server writes one line per event: the time in UTC, the level, where it happened and what,
then key=value details. It says which windows attached, which commands ran and how they were
answered, what started and stopped, how long background jobs took, what each agent was doing and
each step of starting one. It never holds what is typed, pasted or shown in a pane, nor tokens.

The log is server.log in the state folder ($XDG_STATE_HOME/cornercase, else ~/.local/state/cornercase),
or next to the socket when CORNERCASE_SOCKET is set. Past 8 MiB it moves to server.log.1 and starts
over. Start the server with CORNERCASE_LOG=debug for more: every event it handled, every git run,
slow frames.

Examples:
  cornercase logs -n 50
  cornercase logs --follow
  cornercase kill-server && CORNERCASE_LOG=debug cornercase";
pub(super) const START_SERVER_HELP: &str =
    "Scripts and programs that drive cornercase without a window run it first: it starts the server
if none runs (no terminal needed), restores the saved session once, as the first window would, and
answers when its terminals exist. Agents that resume their conversations are still typing then.
Until a window attaches, terminals take the size given here; the first window takes it over, and
gives the terminals its colours. Run again, it starts nothing and says the server is already
running, with the size in use and where it comes from.

Examples:
  cornercase start-server
  cornercase start-server --cols 200 --rows 60 --json";
pub(super) const KILL_SERVER_HELP: &str =
    "Run from a terminal inside cornercase, it would stop the server it runs in, and this terminal with
it, so it asks first; without a terminal it refuses unless --yes is given. To stop another server from
inside cornercase, say which with CORNERCASE_SOCKET=/absolute/path/server.sock.

Examples:
  cornercase kill-server
  CORNERCASE_SOCKET=/tmp/cc-dev/server.sock cornercase kill-server";
pub(super) const RESTART_HELP: &str =
    "It lists what runs in the terminals and asks first, since every program in them stops; agents
resume their conversations afterwards. Every attached window reopens on the new server.

With --when-idle, it waits until no agent is working before restarting: idle, done, pending review
and waiting for you count as stopped, and so does a Claude Code agent whose turn is over while a
shell it started in the background still runs. The server restarts in the same step it sees that,
so no agent starts a turn in between, and every window shows that a restart is pending meanwhile,
with a button that cancels it. Programs that are not agents, such as a dev server, stop as with any restart. The pane
this runs in never holds the restart. --timeout gives up with status 1 and the server keeps running,
as it does when the command is interrupted.

Examples:
  cornercase restart
  cornercase restart --when-idle --yes --timeout 3600";
pub(super) const UPDATE_HELP: &str =
    "With --when-idle, it installs the release at once, then restarts the server on it once no agent
is working, as `cornercase restart --when-idle` does.

Examples:
  cornercase update
  cornercase update --when-idle --yes";
pub(super) const WHEN_IDLE: &str = "Wait until no agent is working, then restart";
pub(super) const RESTART_TIMEOUT: &str =
    "With --when-idle, give up after this many seconds and keep the server running";
pub(super) const PANE_REQUIRED: &str = "The pane to act on; `cornercase status --panes` lists them";
pub(super) const HERE_PANE: &str = "The pane [default: the one this runs in, else the shown one]";
pub(super) const WAIT_PANE: &str = "The pane; repeat it, or add --tab, to wait on several [default: the one this runs in, else \
    the shown one]";
pub(super) const WAIT_TAB: &str = "A tab: its agent's pane, else its active one; repeat it to wait on several";
pub(super) const HERE_WORKSPACE: &str = "The workspace [default: the one this runs in, else the shown one]";
pub(super) const BY_BRANCH: &str =
    "The workspace on this branch, looked up in the project this runs in, else the shown one";

pub(super) const CONTRACT_HELP: &str =
    "Programs that drive cornercase without this command line (an editor extension, a dashboard) ask
the server for it before anything else. `contract` is the version of the requests and answers: it
goes up only when one changes in a way an older client would misread, while new commands, fields
and events are added without it. `boot` names this run of the server and changes on every restart,
since ids last only one run. Each command is listed with whether it is available, and each stream
with its version. A server without this command is older than the contract.

Examples:
  cornercase contract
  cornercase contract --json";

#[cfg(test)]
mod tests;

pub(super) const PLUGIN_HELP: &str =
    "A plugin is a folder with a cornercase-plugin.toml that declares commands: actions in the \
row menus, hooks on events, services the server keeps running and processes each local window keeps running. Its \
commands talk back through these same commands.

Install one from GitHub, any https:// git address or a git folder, pinned to a commit; a repository can hold \
several plugins, each in its own folder (owner/repo/folder on GitHub, address//folder elsewhere): you see where it comes \
from, what it asks for and every command it runs before anything runs, and it is installed turned off. Link a \
folder of yours instead while you write one.

A plugin runs as you and can do anything you can, including typing into every terminal. What it asks for is shown \
before you turn it on, not enforced. A running server picks up changes within a second.

Examples:
  cornercase plugin install acme/notes
  cornercase plugin install acme/notes@v1.2.0
  cornercase plugin install acme/tools/plugins/notes
  cornercase plugin install https://git.example.com/acme/tools.git//plugins/notes
  cornercase plugin link ~/code/my-plugin
  cornercase plugin enable my-plugin
  cornercase plugin list
  cornercase plugin logs my-plugin --follow
  cornercase plugin disable my-plugin
  cornercase plugin update
  cornercase plugin remove notes";

pub(super) const VIEW_ARG: &str = "Act in this view (see `cornercase view`): its tab is the default, and --focus moves the view instead of the window";
pub(super) const VIEW_HELP: &str =
    "For programs that draw cornercase themselves, such as an editor extension or a GUI, so that each
picks its own tab and size without touching the window or the others. It keeps running and prints
the session as one JSON line each time it changes, like `status --follow`, with a `view` object:
id, projection, active, cols, rows, shown (the project, workspace, tab and pane it shows) and
controls (whether it sizes its tab's terminals now). The view lives while this command runs:
stopping it closes the view and gives its tabs back. The first line is the state now; the view's id
is in it. Keep it running in the background (`cornercase view --cols 120 --rows 40 > view.jsonl &`),
read the id from the first line, and pass it as --view ID to `focus`, `new-tab`, `split`, `start`,
`send`, `keys`, `read`, `wait`, `status`, `rename`, `mark` or `surface`: the view's tab is their
default and --focus moves it.

A tab's terminals take the size of the window or the view that claimed it last, without the
window's sidebar and panels: a view claims the tab it shows when it opens, moves there or turns
active, and using the window takes back the tab it shows. Only that one resizes them. Without
--cols and --rows the view only follows the session and sizes nothing. Shared with the window and
other views: the processes, the split layout, the focused pane of a tab, scrolling and selection.

Examples:
  cornercase view --cols 120 --rows 40
  cornercase view --cols 120 --rows 40 --tab 7";
pub(super) const VIEW_SET_HELP: &str =
    "Changes a view that `cornercase view` holds open. --cols and --rows resize it, --inactive makes it
size nothing and stream no terminals (its tabs go to whoever claimed them before, else the window),
--active turns it back on and claims its tab, and --claim takes its tab back from the window or
another view. It prints the view as it is now.

Examples:
  cornercase view-set --view 41 --cols 100 --rows 30
  cornercase view-set --view 41 --claim";
