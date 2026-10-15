pub(super) const ABOUT: &str =
    "A terminal multiplexer for projects, git worktrees and coding agents, driven by the mouse";
pub(super) const LONG_ABOUT: &str =
    "A terminal multiplexer for projects, git worktrees and coding agents, driven by the mouse.

Without a command, cornercase opens its window, starting the server if none is running. The other
commands drive the running server from scripts, git hooks and the agents in its panes; they never
start a server.";
pub(super) const AFTER_HELP: &str =
    "Ids come from `cornercase status` and last while the server runs. Inside a pane, the commands
act on that pane, its tab, workspace and project unless told otherwise; elsewhere on what the window
shows. Where a command takes --workspace ID, --worktree BRANCH names the workspace on that branch
instead, in that same project. Only `focus` and `--focus` change what the window shows. The
commands exit with 1 on errors and timeouts, and 2 on wrong usage, a branch no workspace is on
included, or one several are on (both list the workspaces to pick from).

Examples:
  cornercase status
  pane=$(cornercase new-tab --name tests -- cargo test)
  cornercase wait --pane \"$pane\" --until shell --timeout 900
  cornercase read --pane \"$pane\" --lines 40
  cornercase start claude --worktree fix-login --prompt 'Fix the login form' --wait";
pub(super) const STATUS_HELP: &str = "Each pane shows its program and folder, and for a coding agent what it is doing (working,
waiting for an answer, done out of sight, failed, idle), its model and how full its context is. `(you)`
marks the pane running the command, `(shown)` what the window shows. `working (background shell)`
is a Claude Code agent whose turn is over while a shell it started in the background still runs:
it wakes up when that shell ends (`background_shell` in the JSON, where the status stays working).
`(dialog open)` is a Claude Code agent showing a dialog, a panel or its shell mode instead of its
input box, which `cornercase send` refuses (`dialog` in the JSON).
`(survey open)` is Claude Code's feedback survey above its input box, which `cornercase send`
refuses too (`survey` in the JSON): `cornercase keys --pane N 0` dismisses it.

With --panes it prints one line per pane instead, for scripts, in the sidebar's order: a header
line naming the columns, then one row per pane, its columns separated by tabs. The columns are
pane, tab, workspace, project (their ids), program, agent, status, background_shell, dialog,
at_prompt, model, context, caller (the pane running the command), shown (the pane the window
shows), active (its tab's active pane), tab_name, workspace_name, branch, worktree, project_name,
path and survey (Claude Code's feedback survey shows); new ones only ever go at the end. Values are as in the JSON (true or false, context as a
number, full paths), with - where there is none and a space for a tab or line break in a name.
With --json, it prints {\"panes\": [...]}, one object per pane with the columns as keys and null
where there is no value.

Examples:
  cornercase status
  cornercase status --json
  cornercase status --panes | awk -F'\\t' '$7 == \"waiting\" { print $1 }'
  cornercase status --panes --json | jq '.panes[] | select(.agent) | .pane'";
pub(super) const OPEN_HELP: &str = "Examples:
  cornercase open ~/src/shop
  cornercase open . --focus";
pub(super) const NEW_WORKSPACE_HELP: &str =
    "The workspace gets one tab. A worktree goes in the worktrees folder from settings, on a new
branch from HEAD when the branch does not exist yet; the command returns once git made it.

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
effort (Claude Code, Codex and Grok Build take one; an older server refuses all three rather than
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
`cornercase keys --pane N 0`, or pass --force.

With --enter, the command returns once Claude Code, Codex, Gemini CLI, opencode or Grok Build has
recorded the prompt in its own history, and fails with `not confirmed: the prompt may not have
been submitted` when it records none: read the pane before sending it again. A working Codex,
Gemini or Grok Build holds a prompt until it takes it, so the command returns then. Grok Build
queues that prompt; pressing Enter again sends it now and cancels the turn. Gemini local
slash commands, ? help and ! shell input, and Grok Build's / and ! commands, return after Enter
without confirming a prompt record. In other programs it returns once Enter is pressed.

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
database, Grok Build's session) instead of the screen, so it comes whole even once it scrolled
off, without the input box or status lines. While the agent works it is the newest one so far.
--json adds when it was written and whether the agent's turn is over. It fails on a pane without
Claude Code, Codex, Gemini CLI, opencode or Grok Build, and before the agent wrote anything.

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
opencode or Grok Build in the pane. Without reports, Gemini status needs its dynamic window title. A timeout exits with 1 and does not prove that the
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

Examples:
  cornercase close --tab 9
  cornercase close --workspace 5 --remove-worktree
  cornercase close --worktree fix/login --remove-worktree";
pub(super) const RENAME_HELP: &str = "Examples:
  cornercase rename 'review #42'
  cornercase rename --workspace 5 ''";
pub(super) const FOCUS_HELP: &str = "Examples:
  cornercase focus --pane 12";
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
opencode and Grok Build, the same records as `read --last-message`) and in the history of every
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

It prints the workspace's id, then one line per role: its pane's id and the role's name, separated
by a tab. A name is looked up in yours, then the built-in flows, then the project's: yours replaces
a built-in flow of its name, and a project's flow never stands in for another of the same name; the
command says so on stderr when it happens.

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
--to project). Every agent of a flow runs without permission prompts, so when it sets variables or
arguments it asks first, and refuses without a terminal unless you pass --yes.

A name that is already taken is refused: pass --replace to overwrite that flow, or --name to import
this one under another name. A built-in flow's name is yours to take, with --replace, and it then
stands in its place. A project's flow never takes the place of one of yours or a built-in.

Examples:
  cornercase flow import quick.toml
  cornercase flow import quick.toml --to project --name 'Quick mine'
  ssh host cornercase flow export Quick | cornercase flow import - --yes";
pub(super) const TODO_HELP: &str = "Examples:
  cornercase todo add review the login fix
  cornercase todo list
  cornercase todo done 3";
pub(super) const EVENTS_HELP: &str = "It keeps running and prints one line per event, as it happens:
  pane 5  claude  working → waiting   an agent changed state: working, shell (its turn is over but a
                                      background shell it started still runs), waiting, done, idle,
                                      or none once it left the pane
  pane 7  cargo  exited               the program in the foreground ended and the shell is back
  tab 3  opened in workspace 2        a project, workspace, tab or pane opened, or closed
With --json, each line is a JSON object with the event, the ids, the old and new state and the time.

It ends when the server stops, or once every pane given with --pane has closed. Programs are looked
at every half second while something follows events, so one that ends sooner may not show. Start it
before what you want to follow, or read `cornercase status` once it runs, so nothing falls between.
A reader that falls far behind loses events, and a line says how many.

Examples:
  cornercase events
  cornercase events --pane 12 --pane 14 --json";
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
pub(super) const HERE_PANE: &str = "The pane [default: the one this runs in, else the shown one]";
pub(super) const WAIT_PANE: &str = "The pane; repeat it, or add --tab, to wait on several [default: the one this runs in, else \
    the shown one]";
pub(super) const WAIT_TAB: &str = "A tab: its agent's pane, else its active one; repeat it to wait on several";
pub(super) const HERE_WORKSPACE: &str = "The workspace [default: the one this runs in, else the shown one]";
pub(super) const BY_BRANCH: &str =
    "The workspace on this branch, looked up in the project this runs in, else the shown one";

#[cfg(test)]
mod tests;
